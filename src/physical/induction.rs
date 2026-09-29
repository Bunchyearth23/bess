//! Turbocharger and charge-volume engineering prototype. The compressor
//! characteristic is an analytic estimate, not a measured map. A duct-inertance
//! mass-flow state and nonmonotone cubic characteristic permit reverse-flow
//! surge against the compressible charge volume, without an audio oscillator.
//! Reduced momentum model follows Greitzer's compressor/plenum construction:
//! https://doi.org/10.1115/1.3446138 and
//! https://ntrs.nasa.gov/citations/19850012807 . Duct length and cubic coefficients
//! below are estimates; frequencies/amplitudes are not calibrated turbo data.
//! This lumped model audits reservoir enthalpy and shaft work; it does not resolve
//! a spatial duct kinetic-energy field or rotating-stall cells.
//! Turbine energy is recovered from gas already leaving the collector. The
//! caller must subtract `turbine_energy_j` from tailpipe thermal energy when
//! auditing the complete system. No sound gain participates in these equations.
use super::{
    cylinder::Reservoir,
    gas::Orifice,
    manifolds::{ATMOSPHERE, Composition, Exchange, Species},
    thermo::{EnergyInput, EnergyLedger, GasState, Mixture, ThermoError},
};
use crate::engine_build::{Aspiration, BlowOff, EngineBuild, ResolvedTuning};

#[derive(Debug, Clone, Copy, Default)]
pub struct InductionStep {
    pub turbine_energy_j: f64,
    pub compressor_energy_j: f64,
    pub bearing_loss_j: f64,
    pub overspeed_loss_j: f64,
    pub shaft_energy_change_j: f64,
    pub compressor_mass_kg: f64,
    /// Signed instantaneous accepted flow, atmosphere -> charge positive.
    pub compressor_mass_flow_kg_s: f64,
    pub surge_reverse_mass_kg: f64,
    /// Impeller work on reverse flow, carried out through the inlet.
    pub surge_work_j: f64,
    /// Absolute difference between requested and accepted compressor mass.
    pub compressor_mass_guard_kg: f64,
    pub compressor_fresh_air_kg: f64,
    pub compressor_fuel_kg: f64,
    pub bov_species: Species,
    /// Net signed compressor-port enthalpy into charge (negative on backflow).
    pub compressor_enthalpy_j: f64,
    pub atmospheric_bov_mass_kg: f64,
    pub recirculated_bov_mass_kg: f64,
    /// Enthalpy to the atmospheric or ideal inlet return boundary.
    pub bov_enthalpy_j: f64,
    pub charge_ledger: EnergyLedger,
}
#[derive(Clone)]
pub struct Induction {
    charge: GasState,
    species: Species,
    shaft_energy_j: f64,
    inertia: f64,
    max_boost_pa: f64,
    compressor_displacement_m3: f64,
    compressor_flow_kg_s: f64,
    duct_area_m2: f64,
    bov: BlowOff,
    enabled: bool,
    turbine_area_m2: f64,
    /// Swing-valve area in parallel with the turbine nozzle.
    wastegate_area_m2: f64,
    /// 0 shut … 1 fully open, set by the boost-referenced actuator.
    wastegate: f64,
    turbine_outlet_pa: f64,
}
impl Induction {
    pub fn new(
        build: &EngineBuild,
        tuning: &ResolvedTuning,
        total_displacement_m3: f64,
    ) -> Result<Self, String> {
        build.validate()?;
        if !(1e-6..=0.5).contains(&total_displacement_m3) {
            return Err("Invalid induction displacement".into());
        }
        // Compressor outlet to throttle: hot pipe, intercooler core and tanks,
        // cold pipe, ≈3 L at 2 L (estimate; research_notes' deceleration_physics
        // takes ≈5 L). The former 0.75 × displacement (outlet volume alone) put
        // the duct/charge Helmholtz mode at ≈47 Hz, the 1200–1400 rpm firing
        // frequency: pulses swung compressor flow across the speed-line peak and
        // locked 1200 rpm WOT into surge once X-026 moved it 1 % closer (38 kPa
        // at 27 Hz). At 1.5 × the mode sits at ≈33 Hz, below boosted firing.
        let volume = (total_displacement_m3 * 1.5).max(0.0002);
        let charge = GasState::at_pressure(
            volume,
            ATMOSPHERE.pressure_pa,
            ATMOSPHERE.temperature_k,
            Mixture::AIR,
        )
        .map_err(|e| format!("{e:?}"))?;
        let mass = charge.mass_kg();
        Ok(Self {
            charge,
            species: Species {
                fresh_air_kg: mass,
                fuel_kg: 0.0,
            },
            shaft_energy_j: 0.0,
            inertia: tuning.turbo_inertia_kg_m2,
            max_boost_pa: f64::from(build.boost_bar) * 1e5,
            compressor_displacement_m3: tuning.compressor_displacement_m3,
            compressor_flow_kg_s: 0.0,
            duct_area_m2: 0.0015 * (total_displacement_m3 / 0.002).powf(2.0 / 3.0),
            bov: build.blow_off,
            enabled: build.aspiration != Aspiration::Natural,
            turbine_area_m2: tuning.turbine_area_m2,
            wastegate_area_m2: tuning.wastegate_area_m2,
            wastegate: 0.0,
            turbine_outlet_pa: ATMOSPHERE.pressure_pa,
        })
    }
    /// Wastegate opening for the exhaust network's turbine-bypass area.
    pub fn wastegate(&self) -> f64 {
        self.wastegate
    }
    /// Turbine exit (downstream catalyst/muffler volume) for the next step.
    pub fn set_turbine_outlet_pa(&mut self, pressure_pa: f64) -> Result<(), ThermoError> {
        if !(1000.0..=1e7).contains(&pressure_pa) {
            return Err(ThermoError::InvalidInput);
        }
        self.turbine_outlet_pa = pressure_pa;
        Ok(())
    }
    pub fn supply(&self) -> Reservoir {
        Reservoir {
            pressure_pa: self.charge.pressure_pa(),
            temperature_k: self.charge.temperature_k(),
        }
    }
    pub fn supply_composition(&self) -> Composition {
        self.species.composition(self.charge.mass_kg())
    }
    pub fn species(&self) -> Species {
        self.species
    }
    pub fn shaft_energy_j(&self) -> f64 {
        self.shaft_energy_j
    }
    pub fn shaft_rpm(&self) -> f64 {
        (2.0 * self.shaft_energy_j / self.inertia).sqrt() * 60.0 / std::f64::consts::TAU
    }
    pub fn charge_mass_kg(&self) -> f64 {
        self.charge.mass_kg()
    }
    pub fn charge_internal_energy_j(&self) -> f64 {
        self.charge.internal_energy_j()
    }
    /// Aggregate throttle demand must respect this budget before charge stepping.
    pub fn outgoing_budget_kg(&self) -> f64 {
        let mixture = self.charge.mixture();
        let umin = mixture.internal_energy(200.0);
        ((self.charge.internal_energy_j() - self.charge.mass_kg() * umin)
            / (mixture.enthalpy(self.charge.temperature_k()) - umin))
            .max(0.0)
            .min(self.charge.mass_kg() * 0.9)
            * 0.25
    }
    /// `exchange` is relative to the charge volume: mass_out goes to the engine
    /// manifold; reverse throttle flow adds mass and donor enthalpy. Turbine input
    /// is total positive collector outflow (already accepted by manifold step).
    pub fn step(
        &mut self,
        dt: f64,
        _engine_rpm: f64,
        throttle: f64,
        exhaust: Reservoir,
        exhaust_mass_flow_kg_s: f64,
        exchange: Exchange,
    ) -> Result<InductionStep, ThermoError> {
        // Compatibility reservoir API assumes incoming boundary gas is fresh
        // air. Coupled manifolds must call step_with_species with real tracers.
        let species = Species {
            fresh_air_kg: exchange.mass_in_kg,
            fuel_kg: 0.0,
        };
        self.step_with_species(
            dt,
            _engine_rpm,
            throttle,
            exhaust,
            exhaust_mass_flow_kg_s,
            (exchange, species),
        )
    }
    /// Same mechanics as step, with gas and incoming constituents paired so
    /// reverse throttle products cannot silently become fresh compressor air.
    pub fn step_with_species(
        &mut self,
        dt: f64,
        _engine_rpm: f64,
        throttle: f64,
        exhaust: Reservoir,
        exhaust_mass_flow_kg_s: f64,
        charge_exchange: (Exchange, Species),
    ) -> Result<InductionStep, ThermoError> {
        let (exchange, incoming_species) = charge_exchange;
        if !(1e-8..=1.0 / 96000.0).contains(&dt)
            || !throttle.is_finite()
            || !(0.0..=1.0).contains(&exchange.mass_in_kg)
            || !(0.0..=self.outgoing_budget_kg() * (1.0 + 1e-12)).contains(&exchange.mass_out_kg)
            || !(0.0..=1e7).contains(&exchange.enthalpy_in_j)
            || !(0.0..=1e9).contains(&exhaust.pressure_pa)
            || !(200.0..=3500.0).contains(&exhaust.temperature_k)
            || !exhaust_mass_flow_kg_s.is_finite()
        {
            return Err(ThermoError::InvalidInput);
        }
        let old_energy = self.shaft_energy_j;
        let mut result = InductionStep::default();
        // Turbine inlet gas taken as products (its tracers are not passed in).
        let g = Mixture::PRODUCTS.gamma(exhaust.temperature_k);
        let pressure_ratio = (exhaust.pressure_pa / self.turbine_outlet_pa).max(1.0);
        // Spring-preloaded actuator on charge pressure: cracks at 90 % of the
        // target and is fully open at 110 %, the proportional band of a
        // conventional internal wastegate (no ECU duty cycle modelled).
        let boost = self.charge.pressure_pa() - ATMOSPHERE.pressure_pa;
        let wastegate = if self.enabled && self.max_boost_pa > 0.0 {
            ((boost / self.max_boost_pa - 0.9) / 0.2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        // Nozzle and gate see the same pressure ratio: flow splits by area.
        let turbine_share = if self.turbine_area_m2 > 0.0 {
            self.turbine_area_m2 / (self.turbine_area_m2 + wastegate * self.wastegate_area_m2)
        } else {
            1.0
        };
        if self.enabled {
            result.turbine_energy_j = exhaust_mass_flow_kg_s.max(0.0)
                * turbine_share
                * dt
                * (Mixture::PRODUCTS.cv(exhaust.temperature_k) + Mixture::PRODUCTS.gas_constant())
                * exhaust.temperature_k
                * (1.0 - pressure_ratio.powf(-(g - 1.0) / g))
                * 0.65;
        }
        let available = old_energy + result.turbine_energy_j;
        let omega = (2.0 * available / self.inertia).sqrt();
        let speed_ratio = (omega / 18000.0).min(1.0);
        // Peak (surge-line) pressure rise at the speed limit: 4 × target, a
        // compressor of PR ≈ 4 at 172k rpm. Surge flow ∝ ω and head ∝ ω², so
        // this places the target at half speed and widens the constant-boost
        // flow range surge → choke to ≈2.7×, the span of a WOT line from
        // ~2500 rpm to redline (at 1.6× or less the line fell into surge
        // below and choke above it). The wastegate, not the overspeed guard,
        // then holds boost.
        let head = 4.0 * self.max_boost_pa * speed_ratio * speed_ratio;
        let air = Mixture::AIR;
        let density = ATMOSPHERE.pressure_pa / (air.gas_constant() * ATMOSPHERE.temperature_k);
        // d(mdot)/dt = (A/L)*(compressor pressure rise - charge pressure rise).
        // The positive-slope part of the cubic is unstable against plenum
        // compliance; the reverse branch lets the charge discharge upstream.
        // 1.5 m is an equivalent inertance length, not a measured pipe length.
        let reference_flow =
            density * self.compressor_displacement_m3 * omega.max(4000.0) / std::f64::consts::TAU;
        // X-027: surge (the peak, 2 × reference at the speed limit) sits at a
        // constant diffuser flow coefficient m/(ρ₂U), so it scales with the
        // stage-exit density ρ₂ ∝ PR^(1 − (γ−1)/(γη)) of each speed line's peak,
        // while choke (the zero-head end, 3.10 × reference) stays set by the
        // inlet. Low-speed lines are wider (≈3× surge → zero head at 1500 rpm
        // WOT, 1.55× at the limit), as on measured maps; with inlet-density
        // similarity alone the low-rpm WOT and part-load points sat left of
        // the peak and deep-surged (30–70 kPa at 10–30 Hz). Reverse flow is unchanged.
        let ga = air.gamma(300.0);
        let exit_density = |ratio: f64| ratio.powf(1.0 - (ga - 1.0) / (ga * 0.7));
        let surge = exit_density(1.0 + head / ATMOSPHERE.pressure_pa)
            / exit_density(1.0 + 4.0 * self.max_boost_pa / ATMOSPHERE.pressure_pa);
        const ZERO_HEAD: f64 = 3.1038;
        let acceleration = |flow: f64| {
            let r = (flow / reference_flow).clamp(-8.0, 8.0);
            // Three C¹ pieces of the same cubic (zero slope at x = ±1).
            let x = if r < 0.0 {
                r - 1.0
            } else if r < 2.0 * surge {
                r / surge - 1.0
            } else {
                1.0 + (r - 2.0 * surge) * (ZERO_HEAD - 2.0) / (ZERO_HEAD - 2.0 * surge)
            };
            let characteristic = if self.enabled {
                head * (0.2 + 0.4 * (1.0 + 1.5 * x - 0.5 * x * x * x))
            } else {
                0.0
            };
            let friction =
                flow * flow.abs() / (2.0 * density * self.duct_area_m2 * self.duct_area_m2);
            self.duct_area_m2 / 1.5
                * (characteristic - (self.charge.pressure_pa() - ATMOSPHERE.pressure_pa) - friction)
        };
        // Heun update of the momentum state with old reservoir pressure.
        let first = acceleration(self.compressor_flow_kg_s);
        let next_flow = self.compressor_flow_kg_s
            + 0.5 * dt * (first + acceleration(self.compressor_flow_kg_s + dt * first));
        let compressor_ratio = (self.charge.pressure_pa() / ATMOSPHERE.pressure_pa).max(1.0);
        let ga = air.gamma(300.0);
        let work_per_kg = (air.cv(300.0) + air.gas_constant())
            * 300.0
            * (compressor_ratio.powf((ga - 1.0) / ga) - 1.0)
            / 0.7;
        let requested = next_flow * dt;
        let dm = if requested >= 0.0 && work_per_kg > 0.0 {
            requested.min(available / work_per_kg)
        } else {
            requested
        }
        .max(-self.outgoing_budget_kg());
        result.compressor_mass_kg = dm;
        result.compressor_mass_flow_kg_s = dm / dt;
        result.surge_reverse_mass_kg = (-dm).max(0.0);
        result.compressor_mass_guard_kg = (requested - dm).abs();
        // The impeller also works on surge backflow (the returned gas leaves
        // hot through the inlet, an external boundary): deep surge brakes the
        // shaft instead of freewheeling. Reported separately for audits.
        result.surge_work_j = ((-dm).max(0.0) * work_per_kg).min(available);
        result.compressor_energy_j = dm.max(0.0) * work_per_kg + result.surge_work_j;
        result.compressor_enthalpy_j = if dm >= 0.0 {
            dm * air.enthalpy(300.0) + dm * work_per_kg
        } else {
            dm * self.charge.mixture().enthalpy(self.charge.temperature_k())
        };
        let old_composition = self.supply_composition();
        result.compressor_fresh_air_kg = if dm >= 0.0 {
            dm
        } else {
            dm * old_composition.fresh_air_fraction
        };
        result.compressor_fuel_kg = if dm >= 0.0 {
            0.0
        } else {
            dm * old_composition.fuel_fraction
        };
        let bov_mass = if self.enabled && self.bov != BlowOff::None && throttle < 0.12 {
            Orifice {
                area_m2: 0.0005,
                discharge_coefficient: 0.8,
            }
            .mass_flow_from_states(
                self.charge.pressure_pa(),
                self.charge.temperature_k(),
                ATMOSPHERE.pressure_pa,
                300.0,
                self.charge
                    .mixture()
                    .properties(self.charge.temperature_k()),
            )
            .max(0.0)
                * dt
        } else {
            0.0
        }
        .min(self.outgoing_budget_kg());
        match self.bov {
            BlowOff::Atmospheric => result.atmospheric_bov_mass_kg = bov_mass,
            BlowOff::Recirculating => result.recirculated_bov_mass_kg = bov_mass,
            BlowOff::None => {}
        }
        result.bov_enthalpy_j =
            bov_mass * self.charge.mixture().enthalpy(self.charge.temperature_k());
        result.bov_species = Species {
            fresh_air_kg: bov_mass * old_composition.fresh_air_fraction,
            fuel_kg: bov_mass * old_composition.fuel_fraction,
        };
        let remaining = (available - result.compressor_energy_j).max(0.0);
        result.bearing_loss_j = remaining * (1.0 - (-dt / 1.5).exp());
        let after_drag = remaining - result.bearing_loss_j;
        let max_energy = 0.5 * self.inertia * 18000.0_f64.powi(2);
        result.overspeed_loss_j = (after_drag - max_energy).max(0.0);
        let next_energy = after_drag.min(max_energy);
        let mut next_charge = self.charge;
        result.charge_ledger = next_charge.step(
            self.charge.volume_m3(),
            EnergyInput {
                mass_in_kg: dm.max(0.0) + exchange.mass_in_kg,
                enthalpy_in_j: result.compressor_enthalpy_j.max(0.0) + exchange.enthalpy_in_j,
                mass_out_kg: bov_mass + exchange.mass_out_kg + (-dm).max(0.0),
                ..Default::default()
            },
        )?;
        let next_species = self.species.transport(
            self.charge.mass_kg(),
            result.charge_ledger.mass_out_kg,
            Species {
                fresh_air_kg: incoming_species.fresh_air_kg + dm.max(0.0),
                fuel_kg: incoming_species.fuel_kg,
            },
            result.charge_ledger.mass_in_kg,
        )?;
        // Fixed volume: the new composition changes T/p, never energy.
        next_charge.set_mixture(next_species.composition(next_charge.mass_kg()).mixture());
        self.charge = next_charge;
        self.species = next_species;
        self.shaft_energy_j = next_energy;
        self.compressor_flow_kg_s = dm / dt;
        self.wastegate = wastegate;
        result.shaft_energy_change_j = next_energy - old_energy;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_build::EngineTuning;
    const DT: f64 = 1.0 / 96000.0;
    fn turbo(bov: BlowOff) -> Induction {
        let build = EngineBuild {
            aspiration: Aspiration::Turbo,
            blow_off: bov,
            ..Default::default()
        };
        Induction::new(&build, &EngineTuning::default().resolve(&build, 4), 0.002).unwrap()
    }
    const EXHAUST: Reservoir = Reservoir {
        pressure_pa: 220000.0,
        temperature_k: 1000.0,
    };
    #[test]
    fn throttle_peer_flux_conserves_joint_charge_and_manifold_mass_energy() {
        use super::super::manifolds::{Flows, Manifolds};
        let mut t = turbo(BlowOff::None);
        let build = EngineBuild::default();
        let tuning = EngineTuning::default().resolve(&build, 4);
        let mut m = Manifolds::new(&build, &tuning, 4, 0.002).unwrap();
        let mass = t.charge_mass_kg() + m.total_mass_kg();
        let energy = t.charge_internal_energy_j() + m.total_internal_energy_j();
        let mut added_m = 0.0;
        let mut added_e = 0.0;
        let mut corrections = 0.0;
        for _ in 0..12000 {
            m.set_supply(t.supply()).unwrap();
            m.set_supply_limit(t.outgoing_budget_kg()).unwrap();
            let ms = m.step(DT, 0.7, 0.0, Flows::default()).unwrap();
            let ts = t
                .step(DT, 3000.0, 0.7, EXHAUST, 0.08, ms.supply_exchange)
                .unwrap();
            added_m += ts.compressor_mass_kg;
            added_e += ts.compressor_enthalpy_j;
            corrections +=
                ts.charge_ledger.numerical_correction_j + ms.intake_ledger.numerical_correction_j;
        }
        assert!((t.charge_mass_kg() + m.total_mass_kg() - mass - added_m).abs() < 1e-12);
        assert!(
            (t.charge_internal_energy_j() + m.total_internal_energy_j()
                - energy
                - added_e
                - corrections)
                .abs()
                < 1e-7
        );
    }
    #[test]
    fn turbine_spools_and_compressor_heats_charge_with_closed_energy_ledger() {
        let mut t = turbo(BlowOff::None);
        let initial_m = t.charge_mass_kg();
        let initial_u = t.charge_internal_energy_j();
        let mut incoming = 0.0;
        let mut enthalpy = 0.0;
        let mut peak_pressure: f64 = 0.0;
        for _ in 0..48000 {
            let s = t
                .step(DT, 3000.0, 1.0, EXHAUST, 0.08, Exchange::default())
                .unwrap();
            incoming += s.compressor_mass_kg;
            enthalpy += s.compressor_enthalpy_j;
            peak_pressure = peak_pressure.max(t.supply().pressure_pa);
            assert!(
                (s.shaft_energy_change_j - s.turbine_energy_j
                    + s.compressor_energy_j
                    + s.bearing_loss_j
                    + s.overspeed_loss_j)
                    .abs()
                    < 1e-9
            );
            assert!(s.charge_ledger.numerical_correction_j.abs() < 1e-9);
        }
        assert!(t.shaft_rpm() > 50000.0);
        // Closed downstream volume now surges: instantaneous final pressure
        // depends on the phase. Peak pressure is the appropriate boost check.
        assert!(peak_pressure > 120000.0);
        assert!(t.supply().temperature_k > 300.0);
        assert!((t.charge_mass_kg() - initial_m - incoming).abs() < 1e-12);
        assert!((t.charge_internal_energy_j() - initial_u - enthalpy).abs() < 1e-7);
    }
    #[test]
    fn lift_off_bov_releases_charge_and_distinguishes_return_destination() {
        let mut no = turbo(BlowOff::None);
        for _ in 0..48000 {
            no.step(
                DT,
                3000.0,
                1.0,
                EXHAUST,
                0.08,
                Exchange {
                    mass_out_kg: (0.07 * DT).min(no.outgoing_budget_kg()),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let preboost = no.supply().pressure_pa;
        let mut vent = no.clone();
        vent.bov = BlowOff::Atmospheric;
        let mut recirc = no.clone();
        recirc.bov = BlowOff::Recirculating;
        let mut a = 0.0;
        let mut b = 0.0;
        let mut reverse_no = 0.0;
        let mut reverse_vent = 0.0;
        let mut reverse_recirc = 0.0;
        let mut last_sign = 1.0_f64;
        let mut reversals = 0;
        let mut pressure_no = 0.0;
        let mut pressure_vent = 0.0;
        let mut guard = 0.0;
        let mut bounds_no = (f64::INFINITY, 0.0_f64);
        let mut bounds_vent = (f64::INFINITY, 0.0_f64);
        let mut corrections = 0.0;
        let initial_mass = no.charge_mass_kg();
        let initial_energy = no.charge_internal_energy_j();
        let mut signed_mass = 0.0;
        let mut signed_enthalpy = 0.0;
        // Two volume-scaled deep-surge cycles: the 3 L charge (was 1.5 L) blows
        // down and refills half as often, so the window is twice the former 20000.
        const STEPS: u32 = 40000;
        for i in 0..STEPS {
            let sn = no
                .step(DT, 3000.0, 0.0, ATMOSPHERE, 0.0, Exchange::default())
                .unwrap();
            reverse_no += sn.surge_reverse_mass_kg;
            guard += sn.compressor_mass_guard_kg;
            signed_mass += sn.compressor_mass_kg;
            signed_enthalpy += sn.compressor_enthalpy_j;
            corrections += sn.charge_ledger.numerical_correction_j.abs();
            if sn.compressor_mass_flow_kg_s * last_sign < 0.0 {
                reversals += 1;
                last_sign = sn.compressor_mass_flow_kg_s.signum();
            }
            let sv = vent
                .step(DT, 3000.0, 0.0, ATMOSPHERE, 0.0, Exchange::default())
                .unwrap();
            a += sv.atmospheric_bov_mass_kg;
            reverse_vent += sv.surge_reverse_mass_kg;
            let sr = recirc
                .step(DT, 3000.0, 0.0, ATMOSPHERE, 0.0, Exchange::default())
                .unwrap();
            b += sr.recirculated_bov_mass_kg;
            reverse_recirc += sr.surge_reverse_mass_kg;
            pressure_no += no.supply().pressure_pa;
            pressure_vent += vent.supply().pressure_pa;
            if i > 1000 {
                bounds_no.0 = bounds_no.0.min(no.supply().pressure_pa);
                bounds_no.1 = bounds_no.1.max(no.supply().pressure_pa);
                bounds_vent.0 = bounds_vent.0.min(vent.supply().pressure_pa);
                bounds_vent.1 = bounds_vent.1.max(vent.supply().pressure_pa);
            }
        }
        println!(
            "surge preboost_pa={preboost:.1}, reverse_none_kg={reverse_no:.9}, reverse_vent_kg={reverse_vent:.9}, crossings={reversals}, guard_kg={guard:e}, mean_none_pa={}, mean_vent_pa={}",
            pressure_no / f64::from(STEPS),
            pressure_vent / f64::from(STEPS)
        );
        assert!(preboost > 120000.0);
        assert!(reverse_no > 1e-4 && reversals >= 4);
        assert!(reverse_vent < reverse_no * 0.25);
        assert_eq!(reverse_vent, reverse_recirc);
        assert!(guard < 1e-9);
        assert!(a > 0.0 && b > 0.0);
        // Surge can have a lower *mean* pressure than a stable vented machine;
        // what the BOV should suppress is cyclic backflow and pressure swing.
        assert!(pressure_vent / f64::from(STEPS) < preboost);
        assert!(bounds_vent.1 - bounds_vent.0 < (bounds_no.1 - bounds_no.0) * 0.5);
        assert!(corrections < 1e-7);
        assert!((no.charge_mass_kg() - initial_mass - signed_mass).abs() < 1e-12);
        assert!((no.charge_internal_energy_j() - initial_energy - signed_enthalpy).abs() < 1e-7);
        assert_eq!(vent.supply().pressure_pa, recirc.supply().pressure_pa);
    }
    #[test]
    fn surge_integrals_converge_with_half_step_and_invalid_step_is_atomic() {
        let run = |rate: u32| {
            let dt = 1.0 / f64::from(rate);
            let mut t = turbo(BlowOff::None);
            for _ in 0..rate / 2 {
                t.step(
                    dt,
                    3000.0,
                    1.0,
                    EXHAUST,
                    0.08,
                    Exchange {
                        mass_out_kg: (0.07 * dt).min(t.outgoing_budget_kg()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            let mut reversed = 0.0;
            let mut mean = 0.0;
            for _ in 0..rate / 5 {
                let s = t
                    .step(dt, 3000.0, 0.0, ATMOSPHERE, 0.0, Exchange::default())
                    .unwrap();
                reversed += s.surge_reverse_mass_kg;
                mean += t.supply().pressure_pa;
                assert!(s.charge_ledger.numerical_correction_j.abs() < 1e-9);
                assert_eq!(s.compressor_mass_guard_kg, 0.0);
            }
            (reversed, mean / f64::from(rate / 5))
        };
        let coarse = run(96000);
        let fine = run(192000);
        println!(
            "surge convergence:96k reverse={}kg mean={}Pa;192k reverse={}kg mean={}Pa",
            coarse.0, coarse.1, fine.0, fine.1
        );
        assert!((coarse.0 / fine.0 - 1.0).abs() < 0.02);
        assert!((coarse.1 / fine.1 - 1.0).abs() < 0.01);
        let mut t = turbo(BlowOff::None);
        let before = t.clone();
        assert!(
            t.step(f64::NAN, 3000.0, 0.0, ATMOSPHERE, 0.0, Exchange::default())
                .is_err()
        );
        assert_eq!(t.charge_mass_kg(), before.charge_mass_kg());
        assert_eq!(t.compressor_flow_kg_s, before.compressor_flow_kg_s);
    }
    #[test]
    fn reverse_throttle_products_cannot_regenerate_charge_fresh_air() {
        let mut t = turbo(BlowOff::Recirculating);
        t.enabled = false;
        let initial = t.species();
        let mut removed_air = 0.0;
        let mut inserted_fuel = 0.0;
        let mut removed_fuel = 0.0;
        let mut external_air = 0.0;
        let mut external_fuel = 0.0;
        for _ in 0..2000 {
            let dm = t.charge_mass_kg() * 0.001;
            let old = t.supply_composition();
            let incoming = Species {
                fresh_air_kg: 0.0,
                fuel_kg: dm * 0.02,
            };
            let s = t
                .step_with_species(
                    DT,
                    1200.0,
                    0.0,
                    ATMOSPHERE,
                    0.0,
                    (
                        Exchange {
                            mass_in_kg: dm,
                            mass_out_kg: dm,
                            enthalpy_in_j: dm
                                * Mixture::from_fractions(0.0, 0.02)
                                    .enthalpy(t.supply().temperature_k),
                        },
                        incoming,
                    ),
                )
                .unwrap();
            removed_air += dm * old.fresh_air_fraction;
            inserted_fuel += incoming.fuel_kg;
            removed_fuel += dm * old.fuel_fraction;
            external_air += s.compressor_fresh_air_kg - s.bov_species.fresh_air_kg;
            external_fuel += s.compressor_fuel_kg - s.bov_species.fuel_kg;
        }
        assert!(
            (t.species().fresh_air_kg - initial.fresh_air_kg + removed_air - external_air).abs()
                < 1e-12
        );
        assert!(
            (t.species().fuel_kg - initial.fuel_kg - inserted_fuel + removed_fuel - external_fuel)
                .abs()
                < 1e-12
        );
        assert!(t.supply_composition().fresh_air_fraction < 0.2);
        assert!(t.supply_composition().fuel_fraction > 0.015);
    }
}
