//! Measurable fixed-speed, single-cylinder coupling prototype.
//!
//! Reservoir pressures/temperatures and RPM are imposed; there is no crankshaft
//! dynamics, plenum, collector, waveguide coupling or audio here. Combustion is
//! ideal heat addition to air-like gas, not fuel/species transport: the trapped
//! mass at ignition is treated as 85% fresh air, lambda=1.1, AFR=14.7, LHV=43 MJ/kg,
//! and heat efficiency=0.8. These are explicit uncalibrated design assumptions.

use super::{
    config::CylinderConfig,
    gas::{DischargeCurve, GasProperties, HarmonicCam, Valve},
    thermo::{self, EnergyInput, EnergyLedger, GasState, SliderCrank, Wiebe},
};
use crate::engine_build::EngineBuild;
use std::f64::consts::{PI, TAU};

const CYCLE: f64 = 2.0 * TAU;
const BURN_DURATION: f64 = PI / 3.0;
// Illustrative curve, not a measured cylinder head flow bench calibration.
const CD_POINTS: [(f64, f64); 5] = [
    (0.0, 0.45),
    (0.05, 0.55),
    (0.1, 0.65),
    (0.2, 0.72),
    (0.4, 0.72),
];

#[derive(Debug, Clone, Copy)]
pub struct Reservoir {
    pub pressure_pa: f64,
    pub temperature_k: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PrototypeOptions {
    pub rpm: f64,
    pub fired: bool,
    pub intake: Reservoir,
    pub exhaust: Reservoir,
    /// Supplied constant Newton coefficient; not a Hohenberg model.
    pub wall_coefficient_w_m2_k: f64,
}

impl Default for PrototypeOptions {
    fn default() -> Self {
        Self {
            rpm: 1500.0,
            fired: true,
            intake: Reservoir {
                pressure_pa: 80000.0,
                temperature_k: 310.0,
            },
            exhaust: Reservoir {
                pressure_pa: 105000.0,
                temperature_k: 650.0,
            },
            wall_coefficient_w_m2_k: 100.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PrototypeMetrics {
    pub simulated_seconds: f64,
    pub steps: u64,
    pub heat_j: f64,
    pub wall_heat_j: f64,
    pub boundary_work_j: f64,
    pub enthalpy_in_j: f64,
    pub enthalpy_out_j: f64,
    pub mass_in_kg: f64,
    pub mass_out_kg: f64,
    pub numerical_correction_j: f64,
    pub absolute_numerical_correction_j: f64,
    pub mass_residual_kg: f64,
    /// First-law residual including separately reported numerical corrections.
    pub energy_residual_j: f64,
    pub peak_pressure_pa: f64,
    pub peak_temperature_k: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct CylinderSample {
    pub time_s: f64,
    pub crank_angle_rad: f64,
    pub pressure_pa: f64,
    pub temperature_k: f64,
    pub mass_kg: f64,
    pub volume_m3: f64,
    /// Positive reservoir -> cylinder; reversed flow is negative.
    pub intake_mass_flow_kg_s: f64,
    /// Positive cylinder -> exhaust reservoir; reversed flow is negative.
    pub exhaust_mass_flow_kg_s: f64,
    pub ledger: EnergyLedger,
}

pub struct CylinderPrototype {
    config: CylinderConfig,
    options: PrototypeOptions,
    geometry: SliderCrank,
    gas: GasState,
    angle: f64,
    burn: Wiebe,
    burn_ca50: f64,
    burn_heat: Option<f64>,
    initial_mass: f64,
    initial_energy: f64,
    metrics: PrototypeMetrics,
}

impl CylinderPrototype {
    pub fn from_build(build: &EngineBuild, options: PrototypeOptions) -> Result<Self, String> {
        if !(200.0..=12000.0).contains(&options.rpm)
            || !(0.0..=1000.0).contains(&options.wall_coefficient_w_m2_k)
            || [options.intake, options.exhaust].iter().any(|r| {
                !(10000.0..=300000.0).contains(&r.pressure_pa)
                    || !(250.0..=1200.0).contains(&r.temperature_k)
            })
        {
            return Err("Invalid cylinder prototype operating conditions".into());
        }
        let config = CylinderConfig::from_build(build)?;
        let geometry = SliderCrank::new(
            config.bore_m,
            config.stroke_m,
            config.rod_m,
            config.compression_ratio,
        )
        .map_err(|e| format!("Invalid geometry: {e:?}"))?;
        // Start at compression BDC with an intake-equilibrated charge. The first
        // cycle is a startup transient, so compare converged cycles separately.
        let angle = -PI;
        let mass = options.intake.pressure_pa * geometry.volume(angle)
            / (thermo::GAS_CONSTANT * options.intake.temperature_k);
        let gas = GasState::new(mass, options.intake.temperature_k, geometry.volume(angle))
            .map_err(|e| format!("Invalid initial charge: {e:?}"))?;
        let ca50 = 8_f64.to_radians();
        Ok(Self {
            config,
            options,
            geometry,
            gas,
            angle,
            burn: Self::burn(ca50),
            burn_ca50: ca50,
            burn_heat: None,
            initial_mass: mass,
            initial_energy: gas.internal_energy_j(),
            metrics: PrototypeMetrics::default(),
        })
    }

    fn burn(ca50: f64) -> Wiebe {
        Wiebe::from_ca50(ca50, BURN_DURATION, 2.0, 1.0 - (-5_f64).exp())
            .expect("constant valid Wiebe parameters")
    }

    fn reservoir_flow(&self, reservoir: Reservoir, intake: bool) -> f64 {
        let c = self.config;
        let (diameter, count, center) = if intake {
            (c.intake_diameter_m, c.intake_valves, c.intake_center_deg)
        } else {
            (c.exhaust_diameter_m, c.exhaust_valves, c.exhaust_center_deg)
        };
        let lift = HarmonicCam {
            center_rad: center.to_radians(),
            duration_rad: c.seat_duration_deg.to_radians(),
            peak_lift_m: c.lift_m,
            shape_exponent: 1.0,
            lash_m: c.lash_m,
        }
        .lift_m(self.angle);
        let valve = Valve {
            diameter_m: diameter,
            port_area_m2: PI * diameter * diameter / 4.0,
            count: count as u8,
        };
        let orifice = valve.orifice(
            lift,
            DischargeCurve::new(&CD_POINTS).expect("constant sorted Cd curve"),
        );
        let upstream_t = if reservoir.pressure_pa >= self.gas.pressure_pa() {
            reservoir.temperature_k
        } else {
            self.gas.temperature_k()
        };
        orifice.mass_flow_from_states(
            reservoir.pressure_pa,
            reservoir.temperature_k,
            self.gas.pressure_pa(),
            self.gas.temperature_k(),
            GasProperties {
                gas_constant_j_kg_k: thermo::GAS_CONSTANT,
                gamma: thermo::gamma(upstream_t),
            },
        )
    }

    pub fn metrics(&self) -> PrototypeMetrics {
        self.metrics
    }

    /// Fixed or smaller integration time step, max 1/96000 s; no allocations.
    pub fn step(&mut self, dt_s: f64) -> Result<CylinderSample, thermo::ThermoError> {
        if !(1e-8..=1.0 / 96000.0).contains(&dt_s) {
            return Err(thermo::ThermoError::InvalidInput);
        }
        let next_angle = self.angle + self.options.rpm * TAU / 60.0 * dt_s;
        let intake = self.reservoir_flow(self.options.intake, true);
        let exhaust_in = self.reservoir_flow(self.options.exhaust, false);
        let mass_in = (intake.max(0.0) + exhaust_in.max(0.0)) * dt_s;
        let mass_out = ((-intake).max(0.0) + (-exhaust_in).max(0.0)) * dt_s;
        let enthalpy_in =
            intake.max(0.0) * dt_s * thermo::specific_enthalpy(self.options.intake.temperature_k)
                + exhaust_in.max(0.0)
                    * dt_s
                    * thermo::specific_enthalpy(self.options.exhaust.temperature_k);
        if self.angle >= self.burn.start_rad() + BURN_DURATION {
            self.burn_ca50 += CYCLE;
            self.burn = Self::burn(self.burn_ca50);
            self.burn_heat = None;
        }
        if self.burn_heat.is_none() && next_angle > self.burn.start_rad() {
            // Heat-only equivalent-fuel model, with explicit fresh-charge estimate.
            self.burn_heat = Some(if self.options.fired {
                self.gas.mass_kg() * 0.85 / (14.7 * 1.1) * 43e6 * 0.8
            } else {
                0.0
            });
        }
        let input = EnergyInput {
            heat_j: self
                .burn
                .released_heat(self.angle, next_angle, self.burn_heat.unwrap_or(0.0)),
            wall_heat_j: thermo::wall_heat_j(
                self.options.wall_coefficient_w_m2_k,
                self.geometry.chamber_area(self.angle),
                self.gas.temperature_k(),
                self.config.wall_temperature_k,
                dt_s,
            ),
            mass_in_kg: mass_in,
            enthalpy_in_j: enthalpy_in,
            mass_out_kg: mass_out,
        };
        let ledger = self.gas.step(self.geometry.volume(next_angle), input)?;
        // If the donor cap activates, apportion actual outflow to both ports.
        let out_scale = if mass_out > 0.0 {
            ledger.mass_out_kg / mass_out
        } else {
            1.0
        };
        let actual_intake = if intake < 0.0 {
            intake * out_scale
        } else {
            intake
        };
        let actual_exhaust = if exhaust_in < 0.0 {
            -exhaust_in * out_scale
        } else {
            -exhaust_in
        };
        self.angle = next_angle;
        let m = &mut self.metrics;
        m.steps += 1;
        m.simulated_seconds += dt_s;
        m.heat_j += ledger.heat_j;
        m.wall_heat_j += ledger.wall_heat_j;
        m.boundary_work_j += ledger.boundary_work_j;
        m.enthalpy_in_j += ledger.enthalpy_in_j;
        m.enthalpy_out_j += ledger.enthalpy_out_j;
        m.mass_in_kg += ledger.mass_in_kg;
        m.mass_out_kg += ledger.mass_out_kg;
        m.numerical_correction_j += ledger.numerical_correction_j;
        m.absolute_numerical_correction_j += ledger.numerical_correction_j.abs();
        m.mass_residual_kg = self.gas.mass_kg() - self.initial_mass - m.mass_in_kg + m.mass_out_kg;
        m.energy_residual_j = self.gas.internal_energy_j()
            - self.initial_energy
            - (m.heat_j - m.wall_heat_j + m.enthalpy_in_j - m.enthalpy_out_j - m.boundary_work_j
                + m.numerical_correction_j);
        m.peak_pressure_pa = m.peak_pressure_pa.max(self.gas.pressure_pa());
        m.peak_temperature_k = m.peak_temperature_k.max(self.gas.temperature_k());
        Ok(CylinderSample {
            time_s: m.simulated_seconds,
            crank_angle_rad: self.angle,
            pressure_pa: self.gas.pressure_pa(),
            temperature_k: self.gas.temperature_k(),
            mass_kg: self.gas.mass_kg(),
            volume_m3: self.gas.volume_m3(),
            intake_mass_flow_kg_s: actual_intake,
            exhaust_mass_flow_kg_s: actual_exhaust,
            ledger,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(fired: bool, rate: u32) -> PrototypeMetrics {
        let mut p = CylinderPrototype::from_build(
            &EngineBuild::default(),
            PrototypeOptions {
                fired,
                ..Default::default()
            },
        )
        .unwrap();
        for _ in 0..rate / 2 {
            let s = p.step(1.0 / f64::from(rate)).unwrap();
            assert!(s.pressure_pa.is_finite() && s.mass_kg > 0.0);
        }
        p.metrics()
    }
    #[test]
    fn firing_adds_work_and_all_exchange_is_accounted() {
        let motored = run(false, 96000);
        let fired = run(true, 96000);
        assert_eq!(motored.heat_j, 0.0);
        assert!(fired.heat_j > 1000.0);
        assert!(fired.boundary_work_j > motored.boundary_work_j + 500.0);
        assert!(fired.peak_pressure_pa > motored.peak_pressure_pa);
        for m in [motored, fired] {
            assert!(m.mass_residual_kg.abs() < 1e-12, "{m:?}");
            assert!(m.energy_residual_j.abs() < 1e-6, "{m:?}");
            assert!(m.absolute_numerical_correction_j < 1e-5, "{m:?}");
        }
    }
    #[test]
    fn smaller_time_steps_converge_and_repeat_deterministically() {
        let a = run(true, 96000);
        let b = run(true, 192000);
        let repeat = run(true, 96000);
        assert_eq!(
            a.boundary_work_j.to_bits(),
            repeat.boundary_work_j.to_bits()
        );
        assert!(
            (a.boundary_work_j / b.boundary_work_j - 1.0).abs() < 0.01,
            "{a:?}\n{b:?}"
        );
        assert!((a.peak_pressure_pa / b.peak_pressure_pa - 1.0).abs() < 0.01);
    }
    #[test]
    fn rejects_oversized_step_without_advancing() {
        let mut p =
            CylinderPrototype::from_build(&EngineBuild::default(), PrototypeOptions::default())
                .unwrap();
        assert!(p.step(1.0 / 48000.0).is_err());
        assert_eq!(p.metrics().steps, 0);
        assert!(p.step(f64::NAN).is_err());
        assert_eq!(p.metrics().steps, 0);
    }
}
