//! Externally driven four-stroke cylinder. No oscillator or synthetic pulses:
//! sound excitation is the actual exhaust-port gas flux. Composition is an
//! air/fuel/product tracer model, not chemical kinetics; each constituent has
//! its own heat capacity and gas constant (`thermo::Mixture`), so the charge,
//! the burned gas and every port flux carry their own properties and enthalpy.
//! Reservoir composition is supplied by the coupled network;
//! recirculation cannot replenish oxygen or add fuel merely by crossing a port.
//! Fuel and fresh air leave in the well-mixed donor fraction.

pub use super::cylinder::Reservoir;
use super::{
    combustion_model::{FlameState, PredictiveCombustionConfig},
    config::CylinderConfig,
    fuel_transient::{FuelStep, FuelTransfer, FuelTransient, FuelTransientConfig},
    gas::{DischargeCurve, HarmonicCam, Valve, floor_fast, rem_euclid_near},
    thermo::{self, EnergyInput, EnergyLedger, GasState, Mixture, SliderCrank, ThermoError, Wiebe},
    wave_junction::{ValveFlow, WavePort},
};
use std::f64::consts::{PI, TAU};

const CYCLE: f64 = 2.0 * TAU;
const AFR: f64 = 14.7;
const LHV: f64 = 43e6;
const SPARK_REFERENCE: f64 = -35.0 * PI / 180.0;
const CD: [(f64, f64); 5] = [
    (0., 0.45),
    (0.05, 0.55),
    (0.1, 0.65),
    (0.2, 0.72),
    (0.4, 0.72),
];

#[derive(Clone, Copy, Debug)]
pub struct CycleInput {
    /// Global unwrapped crank angle at the END of this step, radians.
    pub angle_rad: f64,
    pub rpm: f64,
    pub dt_s: f64,
    pub intake: Reservoir,
    pub exhaust: Reservoir,
    /// Unconsumed air and unburned fuel mass fractions in each donor reservoir.
    /// The remainder is combustion product/inert gas. Air contains 23.3% O2.
    pub intake_fresh_air_fraction: f64,
    pub intake_fuel_fraction: f64,
    pub exhaust_fresh_air_fraction: f64,
    pub exhaust_fuel_fraction: f64,
    /// Available reservoir mass for incoming flow, kg/step (shared by caller).
    pub intake_mass_limit_kg: f64,
    pub exhaust_mass_limit_kg: f64,
    /// Relative to stoichiometric injection. Zero cuts new injector delivery;
    /// the legacy path also stops its burn. An enabled liquid model retains
    /// real residual fuel, which may still evaporate and burn after the cut.
    pub fuel_multiplier: f64,
    pub spark_enabled: bool,
    /// Electrical timing shift relative to mechanical compression TDC.
    /// Positive retards the spark. A 360° wrong wire fires on overlap and misses.
    pub spark_shift_rad: f64,
    pub intake_phase_rad: f64,
    /// Correlated cycle variability, 0 deterministic nominal, 1 strong.
    pub variation: f64,
    /// Wiebe duration multiplier, 0.5..=1.5. Changes burn timing, not fuel energy.
    pub burn_duration_scale: f64,
}

impl CycleInput {
    /// Checks of the fields an engine substep shares across its cylinders
    /// (crank, timing, fuel and burn commands). `step` runs them itself;
    /// `step_shared_checked` leaves them to a caller that ran them once.
    pub fn shared_valid(&self) -> bool {
        self.angle_rad.is_finite()
            && self.angle_rad.abs() <= 1e12
            && (0.0..=30000.0).contains(&self.rpm)
            && (1e-9..=1. / 16000.).contains(&self.dt_s)
            && (0.0..=3.0).contains(&self.fuel_multiplier)
            && (0.0..=1.0).contains(&self.variation)
            && (0.5..=1.5).contains(&self.burn_duration_scale)
            && self.intake_phase_rad.is_finite()
    }

    /// Per-cylinder checks: reservoirs, compositions, budgets, spark shift.
    fn cylinder_valid(&self) -> bool {
        self.spark_shift_rad.is_finite()
            && (0.0..=10.0).contains(&self.intake_mass_limit_kg)
            && (0.0..=10.0).contains(&self.exhaust_mass_limit_kg)
            && [
                self.intake_fresh_air_fraction,
                self.intake_fuel_fraction,
                self.exhaust_fresh_air_fraction,
                self.exhaust_fuel_fraction,
            ]
            .iter()
            .all(|f| (0.0..=1.).contains(f))
            && self.intake_fresh_air_fraction + self.intake_fuel_fraction <= 1. + 1e-12
            && self.exhaust_fresh_air_fraction + self.exhaust_fuel_fraction <= 1. + 1e-12
            && [self.intake, self.exhaust].iter().all(|r| {
                // A runner can be nearly evacuated during high-speed overrun.
                // The conservative gas solver supplies positive mass/pressure
                // and an explicit donor budget; a 100 Pa floor rejected those
                // valid states instead of letting their valve flow recover.
                r.pressure_pa > 0.
                    && r.pressure_pa <= 1e7
                    && (200.0..=3500.0).contains(&r.temperature_k)
            })
    }
}

impl Default for CycleInput {
    fn default() -> Self {
        Self {
            angle_rad: 0.,
            rpm: 1500.,
            dt_s: 1. / 96000.,
            intake: Reservoir {
                pressure_pa: 80000.,
                temperature_k: 310.,
            },
            exhaust: Reservoir {
                pressure_pa: 105000.,
                temperature_k: 650.,
            },
            intake_fresh_air_fraction: 1.,
            intake_fuel_fraction: 0.,
            exhaust_fresh_air_fraction: 0.,
            exhaust_fuel_fraction: 0.,
            intake_mass_limit_kg: 1.,
            exhaust_mass_limit_kg: 1.,
            fuel_multiplier: 1.,
            spark_enabled: true,
            spark_shift_rad: 0.,
            intake_phase_rad: 0.,
            variation: 0.,
            burn_duration_scale: 1.,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CycleOutput {
    pub pressure_pa: f64,
    pub temperature_k: f64,
    pub gas_torque_nm: f64,
    /// Signed reservoir -> cylinder mass and enthalpy, excluding injected fuel.
    pub intake_mass_kg: f64,
    pub intake_enthalpy_j: f64,
    pub intake_fresh_air_kg: f64,
    pub intake_fuel_kg: f64,
    /// Signed cylinder -> exhaust mass and enthalpy.
    pub exhaust_mass_kg: f64,
    pub exhaust_enthalpy_j: f64,
    pub exhaust_fresh_air_kg: f64,
    pub exhaust_fuel_kg: f64,
    pub exhaust_mass_flow_kg_s: f64,
    pub heat_j: f64,
    pub fuel_burned_kg: f64,
    pub unburned_exhaust_kg: f64,
    pub oxygen_exhaust_kg: f64,
    pub unburned_intake_kg: f64,
    pub injected_fuel_kg: f64,
    pub injected_fuel_enthalpy_j: f64,
    /// Fuel delivered to GAS (vapor). Equals injector mass in legacy mode;
    /// differs when liquid inventory and evaporation are enabled.
    pub gas_fuel_added_kg: f64,
    pub gas_fuel_enthalpy_j: f64,
    pub fuel_transfer: FuelTransfer,
    pub predicted_burn_duration_s: Option<f64>,
    pub predicted_flame_speed_m_s: Option<f64>,
    pub fuel_mass_kg: f64,
    pub fresh_air_mass_kg: f64,
    pub wall_temperature_k: f64,
    pub coolant_heat_j: f64,
    /// Metal-wall energy introduced by its numerical temperature guard.
    pub wall_numerical_correction_j: f64,
    /// True only on a spark event that could not initiate combustion.
    pub misfired: bool,
    /// Wiebe burned fraction while a burn is active, from spark to its end.
    pub burn_fraction: Option<f64>,
    pub ledger: EnergyLedger,
}

#[derive(Clone, Copy)]
struct Burn {
    curve: Wiebe,
    fuel_kg: f64,
    stop_angle: f64,
    complete_angle: f64,
}

pub struct CycleCylinder {
    config: CylinderConfig,
    phase_rad: f64,
    geometry: SliderCrank,
    intake_cam: HarmonicCam,
    exhaust_cam: HarmonicCam,
    intake_valve: Valve,
    exhaust_valve: Valve,
    discharge_curve: DischargeCurve<'static>,
    wall_capacity_j_k: f64,
    wall_conductance_w_k: f64,
    gas: Option<GasState>,
    previous_angle: f64,
    fuel_mass: f64,
    fresh_air_mass: f64,
    burn: Option<Burn>,
    seed: u64,
    initial_seed: u64,
    variation_memory: f64,
    wall_temperature: f64,
    coolant_temperature_k: f64,
    heat_transfer_scale: f64,
    heat_coefficient: f64,
    coefficient_clock: u32,
    exhaust_port: Option<WavePort>,
    intake_port: Option<WavePort>,
    intake_port_inflow: f64,
    fuel_transient_config: Option<FuelTransientConfig>,
    fuel_transient: FuelTransient,
    predictive_combustion: Option<PredictiveCombustionConfig>,
    /// Previous joint-solve inflow (kg/s), the next solve's starting point.
    port_inflow: f64,
}

impl CycleCylinder {
    pub fn new(config: CylinderConfig, phase_rad: f64, seed: u64) -> Result<Self, ThermoError> {
        let geometry = SliderCrank::new(
            config.bore_m,
            config.stroke_m,
            config.rod_m,
            config.compression_ratio,
        )?;
        if !phase_rad.is_finite()
            || config.intake_valves > 8
            || config.exhaust_valves > 8
            || !(0.001..=0.5).contains(&config.intake_diameter_m)
            || !(0.001..=0.5).contains(&config.exhaust_diameter_m)
            || !(0.0..=0.1).contains(&config.lift_m)
            || !(0.0..=0.1).contains(&config.exhaust_lift_m)
            || !(0.0..=0.1).contains(&config.lash_m)
            || !(0.0..=720.0).contains(&config.seat_duration_deg)
            || !(0.0..=720.0).contains(&config.exhaust_seat_duration_deg)
            || !config.intake_center_deg.is_finite()
            || !config.exhaust_center_deg.is_finite()
            || !(200.0..=1200.0).contains(&config.wall_temperature_k)
        {
            return Err(ThermoError::InvalidGeometry);
        }
        Ok(Self {
            config,
            phase_rad: phase_rad.rem_euclid(CYCLE),
            geometry,
            intake_cam: HarmonicCam {
                center_rad: config.intake_center_deg.to_radians(),
                duration_rad: config.seat_duration_deg.to_radians(),
                peak_lift_m: config.lift_m,
                shape_exponent: 1.,
                lash_m: config.lash_m,
            },
            exhaust_cam: HarmonicCam {
                center_rad: config.exhaust_center_deg.to_radians(),
                duration_rad: config.exhaust_seat_duration_deg.to_radians(),
                peak_lift_m: config.exhaust_lift_m,
                shape_exponent: 1.,
                lash_m: config.lash_m,
            },
            intake_valve: Valve {
                diameter_m: config.intake_diameter_m,
                port_area_m2: PI * config.intake_diameter_m * config.intake_diameter_m / 4.,
                count: config.intake_valves as u8,
            },
            exhaust_valve: Valve {
                diameter_m: config.exhaust_diameter_m,
                port_area_m2: PI * config.exhaust_diameter_m * config.exhaust_diameter_m / 4.,
                count: config.exhaust_valves as u8,
            },
            discharge_curve: DischargeCurve::new(&CD).expect("constant Cd table"),
            wall_capacity_j_k: 1200. * (geometry.displacement_m3() / 0.0005),
            wall_conductance_w_k: 80. * (geometry.displacement_m3() / 0.0005),
            gas: None,
            previous_angle: 0.,
            fuel_mass: 0.,
            fresh_air_mass: 0.,
            burn: None,
            seed: if seed == 0 { 0x9e3779b97f4a7c15 } else { seed },
            initial_seed: if seed == 0 { 0x9e3779b97f4a7c15 } else { seed },
            variation_memory: 0.,
            wall_temperature: config.wall_temperature_k,
            coolant_temperature_k: 370.,
            heat_transfer_scale: 1.,
            heat_coefficient: 100.,
            coefficient_clock: 0,
            exhaust_port: None,
            intake_port: None,
            intake_port_inflow: 0.,
            fuel_transient_config: None,
            fuel_transient: FuelTransient::default(),
            predictive_combustion: None,
            port_inflow: 0.,
        })
    }

    /// Explicit Hohenberg calibration multiplier, zero gives adiabatic walls.
    pub fn set_heat_transfer_scale(&mut self, scale: f64) -> Result<(), ThermoError> {
        if !(0.0..=5.0).contains(&scale) {
            return Err(ThermoError::InvalidInput);
        }
        self.heat_transfer_scale = scale;
        Ok(())
    }

    /// X-017 hook: solve the NEXT `step`'s exhaust valve jointly with this
    /// pipe port (passive wave/valve junction). Consumed by that step; None,
    /// the default, is the legacy prescribed-flow path.
    pub fn set_exhaust_port(&mut self, port: Option<WavePort>) {
        self.exhaust_port = port;
    }

    /// Joint intake wave/valve boundary; consumed by the next step.
    pub fn set_intake_port(&mut self, port: Option<WavePort>) {
        self.intake_port = port;
    }

    /// Change only on a prepared/restarted cylinder: no inventory is discarded.
    pub fn set_fuel_transient(
        &mut self,
        config: Option<FuelTransientConfig>,
    ) -> Result<(), ThermoError> {
        if self.gas.is_some() && config != self.fuel_transient_config {
            return Err(ThermoError::InvalidInput);
        }
        if let Some(c) = config {
            c.validate()?;
        }
        self.fuel_transient_config = config;
        Ok(())
    }

    pub fn set_predictive_combustion(
        &mut self,
        config: Option<PredictiveCombustionConfig>,
    ) -> Result<(), ThermoError> {
        if let Some(c) = config {
            c.validate()?;
        }
        self.predictive_combustion = config;
        Ok(())
    }

    pub fn gas_state(&self) -> Option<GasState> {
        self.gas
    }

    /// Prepare a real stopped charge before the first simulation step. Thermal
    /// conditions are explicit; no new fuel or heat is introduced by starting.
    pub fn initialize_at_rest(
        &mut self,
        angle_rad: f64,
        reservoir: Reservoir,
        wall_temperature_k: f64,
        coolant_temperature_k: f64,
    ) -> Result<(), ThermoError> {
        if self.gas.is_some()
            || !angle_rad.is_finite()
            || !(200.0..=1200.).contains(&wall_temperature_k)
            || !(200.0..=600.).contains(&coolant_temperature_k)
        {
            return Err(ThermoError::InvalidInput);
        }
        let gas = GasState::at_pressure(
            self.geometry.volume(angle_rad - self.phase_rad),
            reservoir.pressure_pa,
            reservoir.temperature_k,
            Mixture::AIR,
        )?;
        self.fresh_air_mass = gas.mass_kg();
        self.gas = Some(gas);
        self.previous_angle = angle_rad;
        self.wall_temperature = wall_temperature_k;
        self.coolant_temperature_k = coolant_temperature_k;
        Ok(())
    }

    pub fn fuel_inventory_kg(&self) -> f64 {
        self.fuel_mass + self.fuel_transient.liquid_mass_kg()
    }
    pub fn fuel_vapor_kg(&self) -> f64 {
        self.fuel_mass
    }
    pub fn fuel_liquid_kg(&self) -> f64 {
        self.fuel_transient.liquid_mass_kg()
    }
    pub fn fuel_film_kg(&self) -> f64 {
        self.fuel_transient.film_mass_kg()
    }
    pub fn fuel_droplets_kg(&self) -> f64 {
        self.fuel_transient.droplet_mass_kg()
    }
    pub fn fuel_liquid_energy_j(&self) -> f64 {
        self.fuel_transient.liquid_energy_j()
    }
    pub fn wall_temperature_k(&self) -> f64 {
        self.wall_temperature
    }
    /// Gas + retained liquid sensible energy + lumped metal, common references.
    pub fn stored_energy_j(&self) -> f64 {
        self.gas.map_or(0., |g| g.internal_energy_j())
            + self.fuel_transient.liquid_energy_j()
            + self.wall_capacity_j_k * self.wall_temperature
    }

    /// Restart without allocation, preserving geometry and thermal calibration.
    pub fn reset(&mut self) {
        self.gas = None;
        self.previous_angle = 0.;
        self.fuel_mass = 0.;
        self.fresh_air_mass = 0.;
        self.burn = None;
        self.seed = self.initial_seed;
        self.variation_memory = 0.;
        self.wall_temperature = self.config.wall_temperature_k;
        self.coolant_temperature_k = 370.;
        self.heat_coefficient = 100.;
        self.coefficient_clock = 0;
        self.port_inflow = 0.;
        self.exhaust_port = None;
        self.intake_port = None;
        self.intake_port_inflow = 0.;
        self.fuel_transient = FuelTransient::default();
    }

    fn uniform(&mut self) -> f64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 11) as f64 / (1_u64 << 53) as f64
    }

    fn valve_lift(&self, local_angle: f64, intake: bool, intake_phase: f64) -> f64 {
        let mut cam = if intake {
            self.intake_cam
        } else {
            self.exhaust_cam
        };
        if intake {
            cam.center_rad += intake_phase;
        }
        cam.lift_m(local_angle)
    }

    /// `(temperature, pressure)` is `gas.temperature_pressure()`.
    fn port_flow(
        &self,
        gas: GasState,
        (temperature, pressure): (f64, f64),
        reservoir: Reservoir,
        reservoir_mixture: impl Fn() -> Mixture,
        lift: f64,
        intake: bool,
    ) -> f64 {
        let valve = if intake {
            self.intake_valve
        } else {
            self.exhaust_valve
        };
        // A seated/absent valve has identically zero area. Avoid evaluating
        // compressible-flow powers/logarithms for most of the engine cycle.
        if lift == 0. || valve.count == 0 {
            return 0.;
        }
        let upstream = if reservoir.pressure_pa > pressure {
            reservoir_mixture().properties(reservoir.temperature_k)
        } else {
            gas.mixture().properties(temperature)
        };
        valve
            .orifice(lift, self.discharge_curve)
            .mass_flow_from_states(
                reservoir.pressure_pa,
                reservoir.temperature_k,
                pressure,
                temperature,
                upstream,
            )
    }

    /// Advance one externally timed step, at most 1/16 kHz and 0.1 radian.
    /// For the 96 kHz engine use dt=1/96000; halted crankshaft is supported.
    pub fn step(&mut self, input: CycleInput) -> Result<CycleOutput, ThermoError> {
        if !input.shared_valid() {
            self.exhaust_port = None;
            self.intake_port = None;
            return Err(ThermoError::InvalidInput);
        }
        self.step_shared_checked(input)
    }

    /// `step` for an input whose `shared_valid()` the caller has checked, e.g.
    /// once per engine substep for fields common to all cylinders. The
    /// per-cylinder fields are still validated here.
    pub fn step_shared_checked(&mut self, input: CycleInput) -> Result<CycleOutput, ThermoError> {
        let exhaust_port = self.exhaust_port.take();
        let intake_port = self.intake_port.take();
        if !input.cylinder_valid() {
            return Err(ThermoError::InvalidInput);
        }
        let previous = if self.gas.is_some() {
            self.previous_angle
        } else {
            input.angle_rad - input.rpm * TAU / 60. * input.dt_s
        };
        let advance = input.angle_rad - previous;
        if !(-1e-12..=0.1).contains(&advance) {
            return Err(ThermoError::InvalidInput);
        }
        let local0 = previous - self.phase_rad;
        let local1 = input.angle_rad - self.phase_rad;
        // Built only where a port flows (most substeps have both valves shut).
        let intake_mixture =
            || Mixture::from_fractions(input.intake_fresh_air_fraction, input.intake_fuel_fraction);
        let exhaust_mixture = || {
            Mixture::from_fractions(
                input.exhaust_fresh_air_fraction,
                input.exhaust_fuel_fraction,
            )
        };
        if self.gas.is_none() {
            let gas = GasState::at_pressure(
                self.geometry.volume(local0),
                input.intake.pressure_pa,
                input.intake.temperature_k,
                intake_mixture(),
            )?;
            let mass = gas.mass_kg();
            self.gas = Some(gas);
            self.fresh_air_mass = mass * input.intake_fresh_air_fraction;
            self.fuel_mass = mass * input.intake_fuel_fraction;
        }
        let mut gas = self.gas.expect("initialized above");
        let mass0 = gas.mass_kg();
        let (temperature0, pressure0) = gas.temperature_pressure();
        let state0 = (temperature0, pressure0);
        let intake_lift = self.valve_lift(local0, true, input.intake_phase_rad);
        let exhaust_lift = self.valve_lift(local0, false, 0.);
        let intake_requested = match intake_port {
            None => self.port_flow(gas, state0, input.intake, intake_mixture, intake_lift, true),
            Some(_) if intake_lift == 0. => {
                self.intake_port_inflow = 0.;
                0.
            }
            Some(port) => {
                let port = WavePort {
                    guess_inflow_kg_s: self.intake_port_inflow,
                    ..port
                };
                let valve = ValveFlow::new(
                    self.intake_valve.orifice(intake_lift, self.discharge_curve),
                    input.intake.temperature_k,
                    pressure0,
                    temperature0,
                    intake_mixture().properties(input.intake.temperature_k),
                    gas.mixture().properties(temperature0),
                );
                let inflow = port.solve(pressure0, |p| valve.evaluate(p)).0;
                self.intake_port_inflow = inflow;
                inflow
            }
        } * input.dt_s;
        let exhaust_requested = match exhaust_port {
            None => self.port_flow(
                gas,
                state0,
                input.exhaust,
                exhaust_mixture,
                exhaust_lift,
                false,
            ),
            // A seated valve cannot see the wave; skip the joint solve.
            Some(_) if exhaust_lift == 0. => {
                self.port_inflow = 0.;
                0.
            }
            Some(port) => {
                let port = WavePort {
                    guess_inflow_kg_s: self.port_inflow,
                    ..port
                };
                // `port_flow` with its per-step invariants hoisted out of the
                // solve: the same orifice, states and donor properties.
                let orifice = self
                    .exhaust_valve
                    .orifice(exhaust_lift, self.discharge_curve);
                let (cylinder_k, cylinder_pa) = state0;
                let reservoir_k = input.exhaust.temperature_k;
                let backflow = exhaust_mixture().properties(reservoir_k);
                let outflow = gas.mixture().properties(cylinder_k);
                let valve = ValveFlow::new(
                    orifice,
                    reservoir_k,
                    cylinder_pa,
                    cylinder_k,
                    backflow,
                    outflow,
                );
                let inflow = port.solve(cylinder_pa, |p| valve.evaluate(p)).0;
                self.port_inflow = inflow;
                inflow
            }
        } * input.dt_s;
        let intake_in = intake_requested.max(0.).min(input.intake_mass_limit_kg);
        let exhaust_in = exhaust_requested.max(0.).min(input.exhaust_mass_limit_kg);
        let requested_out = (-intake_requested).max(0.) + (-exhaust_requested).max(0.);
        let out = requested_out.min(mass0 * 0.9).min((mass0 - 1e-12).max(0.));
        let out_scale = if requested_out > 0. {
            out / requested_out
        } else {
            1.
        };
        let intake_out = (-intake_requested).max(0.) * out_scale;
        let exhaust_out = (-exhaust_requested).max(0.) * out_scale;
        let fuel_fraction = (self.fuel_mass / mass0).clamp(0., 1.);
        let fresh_fraction = (self.fresh_air_mass / mass0).clamp(0., 1.);
        let oxygen_fraction = fresh_fraction * 0.233;
        let intake_fresh =
            intake_in * input.intake_fresh_air_fraction - intake_out * fresh_fraction;
        let exhaust_fresh =
            exhaust_out * fresh_fraction - exhaust_in * input.exhaust_fresh_air_fraction;
        let intake_fuel = intake_in * input.intake_fuel_fraction - intake_out * fuel_fraction;
        let exhaust_fuel = exhaust_out * fuel_fraction - exhaust_in * input.exhaust_fuel_fraction;
        self.fuel_mass = (self.fuel_mass + intake_fuel - exhaust_fuel).max(0.);
        self.fresh_air_mass = (self.fresh_air_mass + intake_fresh - exhaust_fresh).max(0.);
        // Legacy injection is real gasoline vapor mass at intake temperature.
        // Meter only unconsumed air; previously injected fuel carried by a
        // reverse-flow pocket is not dosed again on reaspiration.
        let legacy_injected_fuel = intake_in
            * (input.intake_fresh_air_fraction * input.fuel_multiplier / AFR
                - input.intake_fuel_fraction)
                .max(0.);
        let fuel_transfer = self
            .fuel_transient_config
            .map_or(FuelTransfer::default(), |c| {
                // Speed-density estimate, not a measured injector calibration. The
                // reservoir's already-carried fuel reduces the new pulse command.
                let dose_fraction = (input.intake_fresh_air_fraction * input.fuel_multiplier / AFR
                    - input.intake_fuel_fraction)
                    .max(0.);
                let dose = self.geometry.displacement_m3() * input.intake.pressure_pa
                    / (intake_mixture().gas_constant() * input.intake.temperature_k)
                    * dose_fraction;
                self.fuel_transient.step(
                    c,
                    FuelStep {
                        local0,
                        local1,
                        dt_s: input.dt_s,
                        cycle_dose_kg: dose,
                        fuel_enabled: input.fuel_multiplier > 0.,
                        injection_temperature_k: input.intake.temperature_k,
                        wall_temperature_k: self.wall_temperature,
                        gas_temperature_k: temperature0,
                        intake_turnover: intake_in / mass0,
                        available_evaporation_heat_j: (gas.internal_energy_j()
                            - mass0 * gas.mixture().internal_energy(200.))
                        .max(0.)
                            * 0.05,
                    },
                )
            });
        let injected_fuel = if self.fuel_transient_config.is_some() {
            fuel_transfer.injected_kg
        } else {
            legacy_injected_fuel
        };
        let gas_fuel_added = if self.fuel_transient_config.is_some() {
            fuel_transfer.evaporated_kg
        } else {
            legacy_injected_fuel
        };
        self.fuel_mass += gas_fuel_added;
        let intake_h = if intake_in > 0. {
            intake_mixture().enthalpy(input.intake.temperature_k)
        } else {
            0.
        };
        let exhaust_h = if exhaust_in > 0. {
            exhaust_mixture().enthalpy(input.exhaust.temperature_k)
        } else {
            0.
        };
        let outflow_h = gas.mixture().enthalpy(temperature0);
        let gas_fuel_enthalpy = if self.fuel_transient_config.is_some() {
            fuel_transfer.vapor_enthalpy_j
        } else {
            injected_fuel * Mixture::FUEL.enthalpy(input.intake.temperature_k)
        };
        let injected_enthalpy = if self.fuel_transient_config.is_some() {
            fuel_transfer.injected_liquid_enthalpy_j
        } else {
            gas_fuel_enthalpy
        };

        let spark_reference = SPARK_REFERENCE + rem_euclid_near(input.spark_shift_rad, CYCLE);
        let before_event = floor_fast((local0 - spark_reference) / CYCLE);
        let after_event = floor_fast((local1 - spark_reference) / CYCLE);
        let mut misfired = false;
        let mut predicted_burn_duration_s = None;
        let mut predicted_flame_speed_m_s = None;
        if after_event > before_event {
            self.burn = None;
            let spark_angle = after_event * CYCLE + spark_reference;
            let mechanical_spark = (spark_angle + PI).rem_euclid(CYCLE) - PI;
            let fresh_fraction = (self.fresh_air_mass
                / (mass0 + intake_in + exhaust_in + gas_fuel_added - out))
                .clamp(0., 1.);
            let lambda = self.fresh_air_mass / (AFR * self.fuel_mass.max(1e-15));
            // Bounded correlated innovation, generated only at sparks.
            let innovation = (self.uniform() + self.uniform() + self.uniform() - 1.5) * 2.;
            self.variation_memory = 0.65 * self.variation_memory + 0.76 * innovation;
            let variability = input.variation * (1. + (1. - fresh_fraction) * 2.);
            let prediction = self.predictive_combustion.map(|c| {
                c.predict(FlameState {
                    pressure_pa: pressure0,
                    temperature_k: temperature0,
                    lambda,
                    fresh_fraction,
                    mean_piston_speed_m_s: 2. * self.config.stroke_m * input.rpm / 60.,
                    bore_m: self.config.bore_m,
                    variation: input.variation,
                    innovation: self.variation_memory,
                })
            });
            let chance = prediction.map_or(((fresh_fraction - 0.15) / 0.35).clamp(0., 1.), |p| {
                p.ignition_probability
            });
            let ignites = input.spark_enabled
                && (input.fuel_multiplier > 0. || self.fuel_transient_config.is_some())
                && (-100_f64.to_radians()..=50_f64.to_radians()).contains(&mechanical_spark)
                && self.valve_lift(spark_angle, true, input.intake_phase_rad) == 0.
                && self.valve_lift(spark_angle, false, 0.) == 0.
                && (0.55..=1.65).contains(&lambda)
                && fresh_fraction > 0.15
                && (input.variation == 0. || self.uniform() <= chance);
            if ignites {
                let legacy_duration = (60_f64.to_radians()
                    * (input.rpm.max(300.) / 1500.).powf(0.15)
                    * (1. + 0.4 * (lambda - 1.).abs() + 0.5 * (1. - fresh_fraction))
                    * (0.08 * variability * self.variation_memory).exp())
                .clamp(35_f64.to_radians(), 110_f64.to_radians())
                    * input.burn_duration_scale;
                let duration = prediction.map_or(legacy_duration, |p| {
                    predicted_burn_duration_s = Some(p.duration_s * input.burn_duration_scale);
                    predicted_flame_speed_m_s = Some(p.turbulent_speed_m_s);
                    // Supported finite Wiebe span; very slow mixtures retain
                    // unburned fuel when the exhaust opens before completion.
                    (p.duration_s * input.rpm * TAU / 60. * input.burn_duration_scale)
                        .clamp(0.001, CYCLE)
                });
                let target_ca50 = spark_angle - SPARK_REFERENCE
                    + 8_f64.to_radians()
                    + 5_f64.to_radians() * variability * self.variation_memory;
                // A sufficiently slow burn cannot keep nominal MBT with fixed
                // spark timing. Preserve positive flame-development delay,
                // allowing its CA50 to move later rather than burn before spark.
                let half_fraction = (-(-0.5 * (1. - (-5_f64).exp())).ln_1p() / 5.).cbrt();
                let ca50 = prediction.map_or_else(
                    || {
                        target_ca50
                            .max(spark_angle + 10_f64.to_radians() + half_fraction * duration)
                    },
                    |p| {
                        spark_angle
                            + p.development_s * input.rpm * TAU / 60.
                            + half_fraction * duration
                    },
                );
                let fuel = self.fuel_mass.min(self.fresh_air_mass / AFR) * 0.96;
                // End combustion at exhaust opening of this compression cycle.
                let firing_tdc = (spark_angle - mechanical_spark) / CYCLE;
                let exhaust_open = self.config.exhaust_center_deg.to_radians()
                    - self.config.exhaust_seat_duration_deg.to_radians() * 0.5;
                let curve = Wiebe::from_ca50(ca50, duration, 2., 1. - (-5_f64).exp())?;
                self.burn = Some(Burn {
                    curve,
                    fuel_kg: fuel,
                    stop_angle: firing_tdc * CYCLE + exhaust_open,
                    complete_angle: curve.start_rad() + duration,
                });
            } else {
                // A commanded fuel cut requests no combustion. Failed sparks
                // remain diagnostic misfires while fuel is requested.
                misfired = input.fuel_multiplier > 0.;
            }
        }
        if input.fuel_multiplier == 0. && self.fuel_transient_config.is_none() {
            self.burn = None;
        }
        // Beyond the completed finite burn both fractions are exactly one.
        // A small angular margin preserves the final floating-point tail.
        if self.burn.is_some_and(|b| local0 > b.complete_angle + 1e-9) {
            self.burn = None;
        }
        let fuel_burned = self
            .burn
            .map_or(0., |burn| {
                burn.curve.released_heat(
                    local0.min(burn.stop_angle),
                    local1.min(burn.stop_angle),
                    burn.fuel_kg,
                )
            })
            .min(self.fuel_mass)
            .min(self.fresh_air_mass / AFR);
        self.fuel_mass -= fuel_burned;
        self.fresh_air_mass = (self.fresh_air_mass - fuel_burned * AFR).max(0.);
        let burn_fraction = self
            .burn
            .map(|burn| burn.curve.fraction(local1.min(burn.stop_angle)));
        if self.burn.is_some_and(|burn| local1 >= burn.stop_angle) {
            self.burn = None;
        }
        if self.coefficient_clock == 0 {
            self.heat_coefficient = hohenberg_w_m2_k(
                gas.volume_m3(),
                pressure0,
                temperature0,
                2. * self.config.stroke_m * input.rpm / 60.,
            );
        }
        self.coefficient_clock = (self.coefficient_clock + 1) % 32;
        let wall_heat = thermo::wall_heat_j(
            self.heat_coefficient * self.heat_transfer_scale,
            self.geometry.chamber_area_from_volume(gas.volume_m3()),
            temperature0,
            self.wall_temperature,
            input.dt_s,
        );
        let (volume, volume_derivative) = self.geometry.volume_and_derivative(local1);
        let mass_in = intake_in + exhaust_in + gas_fuel_added;
        // Outflow leaves the fractions unchanged; inflow and burning move them.
        let mixture = (mass_in > 0. || fuel_burned > 0.).then(|| {
            let inverse = 1. / (mass0 + mass_in - out);
            Mixture::from_fractions(self.fresh_air_mass * inverse, self.fuel_mass * inverse)
        });
        let ledger = gas.step(
            volume,
            EnergyInput {
                heat_j: fuel_burned * LHV,
                // The gas ledger carries both wall exchange and the explicitly
                // reported evaporation sink. Only wall_heat heats the metal.
                wall_heat_j: wall_heat + fuel_transfer.latent_heat_j,
                mass_in_kg: mass_in,
                enthalpy_in_j: intake_in * intake_h + gas_fuel_enthalpy + exhaust_in * exhaust_h,
                mass_out_kg: out,
                mixture,
            },
        )?;
        // Lumped metal heat capacity and coolant conductance are explicit
        // calibration estimates; cooling time constant is about 15 seconds.
        let wall_capacity = self.wall_capacity_j_k;
        let coolant_loss = self.wall_conductance_w_k
            * (self.wall_temperature - self.coolant_temperature_k)
            * input.dt_s;
        let previous_wall_temperature = self.wall_temperature;
        self.wall_temperature =
            (self.wall_temperature + (wall_heat - coolant_loss) / wall_capacity).clamp(200., 1200.);
        let wall_correction = wall_capacity * (self.wall_temperature - previous_wall_temperature)
            - wall_heat
            + coolant_loss;
        self.gas = Some(gas);
        self.previous_angle = input.angle_rad;
        let exhaust_mass = exhaust_out - exhaust_in;
        let (temperature_k, pressure_pa) = gas.temperature_pressure();
        let output = CycleOutput {
            pressure_pa,
            temperature_k,
            gas_torque_nm: (pressure_pa - 101325.) * volume_derivative,
            intake_mass_kg: intake_in - intake_out,
            intake_enthalpy_j: intake_in * intake_h - intake_out * outflow_h,
            intake_fresh_air_kg: intake_fresh,
            intake_fuel_kg: intake_fuel,
            exhaust_mass_kg: exhaust_mass,
            exhaust_enthalpy_j: exhaust_out * outflow_h - exhaust_in * exhaust_h,
            exhaust_fresh_air_kg: exhaust_fresh,
            exhaust_fuel_kg: exhaust_fuel,
            exhaust_mass_flow_kg_s: exhaust_mass / input.dt_s,
            heat_j: ledger.heat_j,
            fuel_burned_kg: fuel_burned,
            unburned_exhaust_kg: exhaust_out * fuel_fraction,
            oxygen_exhaust_kg: exhaust_out * oxygen_fraction,
            unburned_intake_kg: intake_out * fuel_fraction,
            injected_fuel_kg: injected_fuel,
            injected_fuel_enthalpy_j: injected_enthalpy,
            gas_fuel_added_kg: gas_fuel_added,
            gas_fuel_enthalpy_j: gas_fuel_enthalpy,
            fuel_transfer,
            predicted_burn_duration_s,
            predicted_flame_speed_m_s,
            fuel_mass_kg: self.fuel_mass,
            fresh_air_mass_kg: self.fresh_air_mass,
            wall_temperature_k: self.wall_temperature,
            coolant_heat_j: coolant_loss,
            wall_numerical_correction_j: wall_correction,
            misfired,
            burn_fraction,
            ledger,
        };
        Ok(output)
    }
}

/// Hohenberg coefficient W/(m² K), using volume m³, pressure internally in BAR,
/// temperature K, mean piston speed m/s. Constants 130 and 1.4 are empirical;
/// no radiation or measured engine calibration is implied. Unit convention:
/// https://www.nature.com/articles/s41598-025-15819-7 (equation 49).
pub fn hohenberg_w_m2_k(
    volume_m3: f64,
    pressure_pa: f64,
    temperature_k: f64,
    mean_speed_m_s: f64,
) -> f64 {
    if !(1e-12..=100.).contains(&volume_m3)
        || !(0.0..=1e9).contains(&pressure_pa)
        || !(200.0..=3500.).contains(&temperature_k)
        || !(0.0..=500.).contains(&mean_speed_m_s)
    {
        return 0.;
    }
    (130.
        * volume_m3.powf(-0.06)
        * (pressure_pa / 1e5).powf(0.8)
        * temperature_k.powf(-0.4)
        * (mean_speed_m_s + 1.4).powf(0.8))
    .min(20000.)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuel_transients_close_combined_liquid_gas_inventory_and_evaporation_energy() {
        for temperature in [280., 450.] {
            let mut cfg = config();
            cfg.wall_temperature_k = temperature;
            let mut cylinder = CycleCylinder::new(cfg, 0., 123).unwrap();
            cylinder
                .set_fuel_transient(Some(FuelTransientConfig::default()))
                .unwrap();
            cylinder
                .set_predictive_combustion(Some(PredictiveCombustionConfig::default()))
                .unwrap();
            let (mut fuel_balance, mut latent, mut heat, mut after_cut) = (0., 0., 0., 0.);
            for frame in 1..=72000 {
                let time = f64::from(frame) / 96000.;
                let cut = (0.4..0.55).contains(&time);
                let out = cylinder
                    .step(CycleInput {
                        angle_rad: time * 1500. * TAU / 60.,
                        fuel_multiplier: if cut { 0. } else { 1. },
                        intake: Reservoir {
                            temperature_k: temperature,
                            pressure_pa: if time < 0.2 { 45000. } else { 80000. },
                        },
                        ..Default::default()
                    })
                    .unwrap();
                fuel_balance += out.intake_fuel_kg - out.exhaust_fuel_kg + out.injected_fuel_kg
                    - out.fuel_burned_kg;
                let inventory =
                    out.fuel_mass_kg + out.fuel_transfer.film_kg + out.fuel_transfer.droplets_kg;
                assert!((fuel_balance - inventory).abs() < 1e-12);
                assert!(out.fuel_transfer.mass_residual_kg().abs() < 1e-18);
                assert!(out.fuel_transfer.energy_residual_j().abs() < 1e-11);
                assert!(out.ledger.residual_j().abs() < 1e-8);
                assert!(
                    (out.ledger.mass_in_kg - out.ledger.mass_out_kg - out.intake_mass_kg
                        + out.exhaust_mass_kg
                        - out.gas_fuel_added_kg)
                        .abs()
                        < 1e-14
                );
                assert!(
                    (out.ledger.enthalpy_in_j - out.ledger.enthalpy_out_j - out.intake_enthalpy_j
                        + out.exhaust_enthalpy_j
                        - out.gas_fuel_enthalpy_j)
                        .abs()
                        < 1e-8
                );
                latent += out.fuel_transfer.latent_heat_j;
                heat += out.heat_j;
                if cut {
                    assert_eq!(out.injected_fuel_kg, 0.);
                    assert!(
                        !out.misfired,
                        "commanded fuel cut is not a failed combustion"
                    );
                    after_cut += out.gas_fuel_added_kg;
                }
            }
            assert!(latent > 0. && after_cut > 0. && heat > 10.);
            assert!(
                cylinder.set_fuel_transient(None).is_err(),
                "cannot discard live film"
            );
            cylinder.reset();
            assert_eq!(cylinder.fuel_transient.liquid_mass_kg(), 0.);
        }
    }

    #[test]
    fn timed_injection_does_not_redose_premixed_reaspirated_fuel() {
        let mut cylinder = CycleCylinder::new(config(), 0., 123).unwrap();
        cylinder
            .set_fuel_transient(Some(FuelTransientConfig::default()))
            .unwrap();
        for frame in 1..=12000 {
            let out = cylinder
                .step(CycleInput {
                    angle_rad: f64::from(frame) / 96000. * 3000. * TAU / 60.,
                    rpm: 3000.,
                    intake_fresh_air_fraction: 0.8,
                    intake_fuel_fraction: 0.8 / AFR,
                    ..Default::default()
                })
                .unwrap();
            assert!(out.injected_fuel_kg.abs() < 1e-18);
        }
    }

    #[test]
    fn predictive_burn_stays_after_spark_and_conserves_consumed_fuel_energy() {
        let mut cfg = config();
        cfg.intake_valves = 0;
        cfg.exhaust_valves = 0;
        cfg.seat_duration_deg = 0.;
        cfg.exhaust_seat_duration_deg = 0.;
        let mut cylinder = CycleCylinder::new(cfg, 0., 123).unwrap();
        cylinder.set_heat_transfer_scale(0.).unwrap();
        cylinder
            .set_predictive_combustion(Some(PredictiveCombustionConfig::default()))
            .unwrap();
        let mut heat = 0.;
        let mut consumed = 0.;
        let mut predicted = false;
        for frame in 0..5000 {
            let angle = -60_f64.to_radians() + f64::from(frame) * 1500. * TAU / 60. / 96000.;
            let out = cylinder
                .step(CycleInput {
                    angle_rad: angle,
                    intake_fresh_air_fraction: AFR / (AFR + 1.),
                    intake_fuel_fraction: 1. / (AFR + 1.),
                    ..Default::default()
                })
                .unwrap();
            if angle <= SPARK_REFERENCE {
                assert_eq!(out.heat_j, 0.);
            }
            if let Some(burn) = cylinder.burn {
                assert!(burn.curve.start_rad() >= SPARK_REFERENCE);
            }
            predicted |= out.predicted_burn_duration_s.is_some();
            heat += out.heat_j;
            consumed += out.fuel_burned_kg;
            assert!(out.ledger.residual_j().abs() < 1e-8);
        }
        assert!(predicted && heat > 0.);
        assert!((heat - consumed * LHV).abs() < 1e-8);
    }

    #[test]
    fn fuel_and_flame_reset_replays_seeded_cycles_and_converges_in_time() {
        fn render(rate: u32, cylinder: &mut CycleCylinder) -> (u64, [f64; 4]) {
            let mut fingerprint = 0_u64;
            let mut baseline = CycleCylinder::new(config(), 0., 123).unwrap();
            let (mut injected, mut heat, mut correction_after_fuel, mut inherited_correction) =
                (0., 0., 0., 0.);
            for frame in 1..=rate / 2 {
                let input = CycleInput {
                    angle_rad: f64::from(frame) / f64::from(rate) * 1500. * TAU / 60.,
                    dt_s: 1. / f64::from(rate),
                    variation: 0.5,
                    ..Default::default()
                };
                let out = cylinder.step(input).unwrap();
                let reference = baseline.step(input).unwrap();
                fingerprint = fingerprint.rotate_left(1)
                    ^ out.pressure_pa.to_bits()
                    ^ out.fuel_transfer.film_kg.to_bits();
                injected += out.injected_fuel_kg;
                heat += out.heat_j;
                // First motored expansion of the inherited firing-TDC startup
                // reaches the 200 K domain before fuel admission. Require the
                // legacy correction itself, not a relaxed new-model budget.
                if out.ledger.numerical_correction_j.abs() > 1e-8 {
                    assert_eq!(
                        injected, 0.,
                        "new guards after fuel admission are a failure"
                    );
                    assert!(out.temperature_k <= 200. + 1e-8);
                    assert_eq!(out.heat_j, 0.);
                    assert_eq!(out.fuel_transfer.latent_heat_j, 0.);
                    assert!(
                        (out.ledger.numerical_correction_j
                            - reference.ledger.numerical_correction_j)
                            .abs()
                            < 1e-12
                    );
                    inherited_correction += out.ledger.numerical_correction_j.abs();
                }
                if injected > 0. {
                    correction_after_fuel += out.ledger.numerical_correction_j.abs();
                }
                assert!(out.wall_numerical_correction_j.abs() < 1e-7);
            }
            (
                fingerprint,
                [injected, heat, correction_after_fuel, inherited_correction],
            )
        }
        let mut cylinder = CycleCylinder::new(config(), 0., 123).unwrap();
        cylinder
            .set_fuel_transient(Some(FuelTransientConfig::default()))
            .unwrap();
        cylinder
            .set_predictive_combustion(Some(PredictiveCombustionConfig::default()))
            .unwrap();
        let (first, nominal) = render(96000, &mut cylinder);
        cylinder.reset();
        assert_eq!(first, render(96000, &mut cylinder).0);
        cylinder.reset();
        let low_rate = render(48000, &mut cylinder).1;
        cylinder.reset();
        let high_rate = render(192000, &mut cylinder).1;
        eprintln!(
            "fuel/flame convergence [injected_kg, heat_J, active_gas_correction_J, inherited_startup_correction_J]: 48k={low_rate:?}; 96k={nominal:?}; 192k={high_rate:?}"
        );
        for other in [low_rate, high_rate] {
            assert!((other[0] / nominal[0] - 1.).abs() < 0.001);
            assert!((other[1] / nominal[1] - 1.).abs() < 0.03);
        }
        assert!(
            nominal[2] < 1e-6,
            "post-injection gas correction must remain reported and negligible: {nominal:?}"
        );
    }

    #[test]
    fn evacuated_runner_boundary_remains_finite_and_invalid_pressures_still_fail() {
        for pressure_pa in [1., 84.110_613_026_652_36, 99.9] {
            let mut cylinder = CycleCylinder::new(config(), 0., 123).unwrap();
            let dt = 1. / 96000.;
            for frame in 1..=6400 {
                let output = cylinder
                    .step(CycleInput {
                        angle_rad: frame as f64 * 7200. * TAU / 60. * dt,
                        rpm: 7200.,
                        dt_s: dt,
                        intake: Reservoir {
                            pressure_pa,
                            temperature_k: 275.,
                        },
                        fuel_multiplier: 0.,
                        spark_enabled: false,
                        ..Default::default()
                    })
                    .unwrap();
                assert!(output.pressure_pa.is_finite() && output.pressure_pa > 0.);
                assert!(output.temperature_k.is_finite());
                assert!(output.ledger.residual_j().abs() < 1e-8);
            }
        }
        for pressure_pa in [0., -1., f64::NAN, f64::INFINITY, 1e7 + 1.] {
            let mut input = CycleInput::default();
            input.intake.pressure_pa = pressure_pa;
            assert!(!input.cylinder_valid());
            let mut cylinder = CycleCylinder::new(config(), 0., 123).unwrap();
            assert!(matches!(
                cylinder.step(input),
                Err(ThermoError::InvalidInput)
            ));
        }
    }
    use crate::engine_build::EngineBuild;
    fn config() -> CylinderConfig {
        CylinderConfig::from_build(&EngineBuild::default()).unwrap()
    }

    #[test]
    fn independent_exhaust_cam_changes_only_its_geometric_lift_schedule() {
        let original = config();
        let edited = CylinderConfig {
            exhaust_lift_m: 0.014,
            exhaust_seat_duration_deg: original.exhaust_seat_duration_deg + 30.,
            exhaust_center_deg: original.exhaust_center_deg - 5.,
            ..original
        };
        let a = CycleCylinder::new(original, 0., 123).unwrap();
        let b = CycleCylinder::new(edited, 0., 123).unwrap();
        let mut different = 0;
        for degree in 0..720 {
            let angle = (degree as f64).to_radians();
            assert_eq!(
                a.valve_lift(angle, true, 0.).to_bits(),
                b.valve_lift(angle, true, 0.).to_bits()
            );
            different +=
                usize::from(a.valve_lift(angle, false, 0.) != b.valve_lift(angle, false, 0.));
        }
        assert!(different > 200);
        assert!(
            (b.valve_lift(edited.exhaust_center_deg.to_radians(), false, 0.)
                - (edited.exhaust_lift_m - edited.lash_m))
                .abs()
                < 1e-12
        );
        let invalid = CylinderConfig {
            exhaust_lift_m: f64::NAN,
            ..original
        };
        assert!(CycleCylinder::new(invalid, 0., 123).is_err());
    }
    fn run(spark: bool, shift: f64, cut_after: f64, phase: f64) -> (f64, f64, f64, f64, u64) {
        let mut cylinder = CycleCylinder::new(config(), phase, 123).unwrap();
        let mut heat = 0.;
        let mut exhaust = 0.;
        let mut unburned = 0.;
        let mut correction = 0.;
        let mut fingerprint = 0_u64;
        let mut injected = 0.;
        let mut burned = 0.;
        let mut escaped = 0.;
        let mut misfires = 0;
        let mut cut_exhaust = 0.;
        for i in 1..=48000 {
            let time = i as f64 / 96000.;
            let out = cylinder
                .step(CycleInput {
                    angle_rad: phase + time * 1500. * TAU / 60.,
                    spark_enabled: spark,
                    spark_shift_rad: shift,
                    fuel_multiplier: if time > cut_after { 0. } else { 1. },
                    ..Default::default()
                })
                .unwrap();
            if time > cut_after {
                assert_eq!(out.heat_j, 0.);
                assert_eq!(out.injected_fuel_kg, 0.);
                assert!(!out.misfired, "intentional fuel cut counted as a misfire");
                cut_exhaust += out.exhaust_mass_kg.max(0.);
            }
            misfires += usize::from(out.misfired);
            heat += out.heat_j;
            exhaust += out.exhaust_mass_kg.max(0.);
            unburned += out.unburned_exhaust_kg;
            correction += out.ledger.numerical_correction_j.abs();
            injected += out.injected_fuel_kg;
            burned += out.fuel_burned_kg;
            escaped += out.unburned_exhaust_kg + out.unburned_intake_kg;
            assert!((injected - burned - escaped - out.fuel_mass_kg).abs() < 1e-12);
            assert!(
                out.pressure_pa.is_finite()
                    && out.temperature_k.is_finite()
                    && out.gas_torque_nm.is_finite()
            );
            assert!(out.ledger.residual_j().abs() < 1e-8);
            assert!(
                (out.ledger.enthalpy_in_j - out.ledger.enthalpy_out_j - out.intake_enthalpy_j
                    + out.exhaust_enthalpy_j
                    - out.injected_fuel_enthalpy_j)
                    .abs()
                    < 1e-8
            );
            assert!(
                (out.ledger.mass_in_kg - out.ledger.mass_out_kg - out.intake_mass_kg
                    + out.exhaust_mass_kg
                    - out.injected_fuel_kg)
                    .abs()
                    < 1e-14
            );
            fingerprint = fingerprint.rotate_left(1) ^ out.pressure_pa.to_bits();
        }
        if cut_after < 0.5 {
            assert!(cut_exhaust > 0., "fuel cut must retain exhaust pumping");
        }
        if cut_after >= 0.5 && (!spark || shift == TAU) {
            assert!(misfires > 0, "failed fueled sparks must still count");
        }
        (heat, exhaust, unburned, correction, fingerprint)
    }
    #[test]
    fn firing_is_real_heat_and_missing_spark_still_pumps_and_ejects_fuel() {
        let fired = run(true, 0., 1., 0.);
        let motored = run(false, 0., 1., 0.);
        assert!(fired.0 > 1000.);
        assert_eq!(motored.0, 0.);
        assert!(motored.1 > 0.001 && motored.2 > 0.00001);
        assert!(fired.2 < motored.2);
        assert!(
            fired.3 / fired.0 < 0.01,
            "correction ratio {}",
            fired.3 / fired.0
        );
    }
    #[test]
    fn fuel_cut_removes_heat_immediately_and_wrong_wiring_cannot_burn() {
        let cut = run(true, 0., 0.25, 0.);
        assert!(cut.0 > 0. && cut.1 > 0.);
        let wrong = run(true, TAU, 1., 0.);
        assert_eq!(wrong.0, 0.);
        assert!(wrong.2 > 0.);
    }
    #[test]
    fn combustion_duration_changes_heat_profile_without_adding_fuel_energy() {
        fn burn(scale: f64) -> (f64, f64, f64) {
            let mut setup = config();
            setup.intake_valves = 0;
            setup.exhaust_valves = 0;
            setup.seat_duration_deg = 0.;
            setup.exhaust_seat_duration_deg = 0.;
            let mut cylinder = CycleCylinder::new(setup, 0., 123).unwrap();
            cylinder.set_heat_transfer_scale(0.).unwrap();
            let mut total = 0.;
            let mut first = 0.;
            let mut second = 0.;
            let mut peak = 0_f64;
            let mut initial_fuel = 0.;
            for i in 0..2800 {
                let angle = -60_f64.to_radians() + i as f64 * 1500. * TAU / 60. / 96000.;
                let output = cylinder
                    .step(CycleInput {
                        angle_rad: angle,
                        intake_fresh_air_fraction: AFR / (AFR + 1.),
                        intake_fuel_fraction: 1. / (AFR + 1.),
                        burn_duration_scale: scale,
                        ..Default::default()
                    })
                    .unwrap();
                if i == 0 {
                    initial_fuel = output.fuel_mass_kg;
                }
                assert!(output.pressure_pa.is_finite() && output.temperature_k.is_finite());
                assert!(output.ledger.residual_j().abs() < 1e-8);
                assert_eq!(output.injected_fuel_kg, 0.);
                total += output.heat_j;
                first += output.heat_j * angle;
                second += output.heat_j * angle * angle;
                peak = peak.max(output.heat_j);
            }
            assert!((total / (initial_fuel * LHV) - 0.96).abs() < 1e-10);
            (
                total,
                (second / total - (first / total).powi(2)).sqrt(),
                peak,
            )
        }
        let fast = burn(0.5);
        let nominal = burn(1.);
        let slow = burn(1.5);
        assert!((fast.0 - nominal.0).abs() < 1e-8);
        assert!((slow.0 - nominal.0).abs() < 1e-8);
        assert!(slow.1 > fast.1 * 2.9, "burn must broaden in crank angle");
        assert!(
            fast.2 > slow.2 * 2.9,
            "equal energy must spread over more steps"
        );
        assert_eq!(CycleInput::default().burn_duration_scale, 1.);
        for scale in [0.49, 1.51, f64::NAN] {
            assert!(
                CycleCylinder::new(config(), 0., 1)
                    .unwrap()
                    .step(CycleInput {
                        burn_duration_scale: scale,
                        ..Default::default()
                    })
                    .is_err()
            );
        }
    }

    #[test]
    fn repeat_render_is_bit_deterministic_and_shifted_mechanics_equivalent() {
        let a = run(true, 0., 1., 0.);
        eprintln!("cycle optimization reference fingerprint: {:016x}", a.4);
        assert_eq!(a.4, run(true, 0., 1., 0.).4);
        let shifted = run(true, 0., 1., 1.7);
        assert!((shifted.0 - a.0).abs() < 1e-7);
        assert!((shifted.1 - a.1).abs() < 1e-9);
    }
    #[test]
    fn closed_valves_conserve_mass_and_recover_motored_energy() {
        let mut config = config();
        config.intake_valves = 0;
        config.exhaust_valves = 0;
        let mut cylinder = CycleCylinder::new(config, 0., 1).unwrap();
        cylinder.set_heat_transfer_scale(0.).unwrap();
        let first = cylinder
            .step(CycleInput {
                angle_rad: -PI,
                rpm: 0.,
                fuel_multiplier: 0.,
                ..Default::default()
            })
            .unwrap();
        let initial = cylinder.gas_state().unwrap();
        for i in 1..=7200 {
            let output = cylinder
                .step(CycleInput {
                    angle_rad: -PI + i as f64 * TAU / 7200.,
                    fuel_multiplier: 0.,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(output.intake_mass_kg, 0.);
            assert_eq!(output.exhaust_mass_kg, 0.);
        }
        let final_state = cylinder.gas_state().unwrap();
        assert_eq!(initial.mass_kg(), final_state.mass_kg());
        assert!((final_state.pressure_pa() / first.pressure_pa - 1.).abs() < 1e-8);
    }
    #[test]
    fn incoming_flow_obeys_each_reservoir_budget_and_heat_units_are_plausible() {
        let mut cylinder = CycleCylinder::new(config(), 0., 1).unwrap();
        let base = CycleInput {
            angle_rad: 450_f64.to_radians(),
            rpm: 0.,
            ..Default::default()
        };
        cylinder.step(base).unwrap();
        let output = cylinder
            .step(CycleInput {
                intake: Reservoir {
                    pressure_pa: 300000.,
                    temperature_k: 310.,
                },
                intake_mass_limit_kg: 1e-10,
                exhaust_mass_limit_kg: 0.,
                ..base
            })
            .unwrap();
        assert!(output.intake_mass_kg <= 1e-10);
        assert!(hohenberg_w_m2_k(0.0005, 1e5, 400., 5.) < 1000.);
        assert!(hohenberg_w_m2_k(0.0005, 1e5, 400., 5.) > 50.);
    }

    #[test]
    fn correlated_variation_and_reset_are_seed_deterministic() {
        fn fingerprint(cylinder: &mut CycleCylinder) -> u64 {
            let mut result = 0_u64;
            for i in 1..=48000 {
                let output = cylinder
                    .step(CycleInput {
                        angle_rad: i as f64 / 96000. * 1500. * TAU / 60.,
                        variation: 0.8,
                        ..Default::default()
                    })
                    .unwrap();
                result = result.rotate_left(1) ^ output.heat_j.to_bits();
                assert!(output.wall_numerical_correction_j.abs() < 1e-7);
                assert!(output.fuel_mass_kg >= 0. && output.fresh_air_mass_kg >= 0.);
                assert!(
                    output.fuel_mass_kg + output.fresh_air_mass_kg
                        <= cylinder.gas_state().unwrap().mass_kg() + 1e-12
                );
            }
            result
        }
        let mut a = CycleCylinder::new(config(), 0., 123).unwrap();
        let first = fingerprint(&mut a);
        a.reset();
        assert!(a.gas_state().is_none());
        assert_eq!(first, fingerprint(&mut a));
        let mut b = CycleCylinder::new(config(), 0., 321).unwrap();
        assert_ne!(first, fingerprint(&mut b));
    }

    #[test]
    fn recirculated_products_never_become_fresh_air_or_new_fuel() {
        let mut cylinder = CycleCylinder::new(config(), 0., 7).unwrap();
        let (mut incoming, mut outgoing) = (0., 0.);
        for i in 1..=16000 {
            let output = cylinder
                .step(CycleInput {
                    angle_rad: i as f64 / 96000. * 3000. * TAU / 60.,
                    rpm: 3000.,
                    intake_fresh_air_fraction: 0.,
                    intake_fuel_fraction: 0.,
                    exhaust_fresh_air_fraction: 0.,
                    exhaust_fuel_fraction: 0.,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(output.injected_fuel_kg, 0.);
            assert_eq!(output.heat_j, 0.);
            assert_eq!(output.fresh_air_mass_kg, 0.);
            assert_eq!(output.fuel_mass_kg, 0.);
            incoming += output.intake_mass_kg.max(0.);
            outgoing += (-output.intake_mass_kg).max(0.) + output.exhaust_mass_kg.max(0.);
        }
        assert!(
            incoming > 1e-4 && outgoing > 1e-4,
            "test must exercise actual gas exchange"
        );
    }

    #[test]
    fn mixed_reservoir_species_and_burn_consumption_close_mass_ledgers() {
        let mut cylinder = CycleCylinder::new(config(), 0., 9).unwrap();
        let input = CycleInput {
            rpm: 0.,
            intake_fresh_air_fraction: 0.8,
            intake_fuel_fraction: 0.02,
            exhaust_fresh_air_fraction: 0.2,
            exhaust_fuel_fraction: 0.01,
            ..Default::default()
        };
        let first = cylinder.step(input).unwrap();
        let (mut fresh, mut fuel) = (first.fresh_air_mass_kg, first.fuel_mass_kg);
        let (mut intake_in, mut intake_out, mut exhaust_in, mut exhaust_out) = (0., 0., 0., 0.);
        for i in 1..=24000 {
            let output = cylinder
                .step(CycleInput {
                    angle_rad: i as f64 / 96000. * 3000. * TAU / 60.,
                    rpm: 3000.,
                    ..input
                })
                .unwrap();
            fresh += output.intake_fresh_air_kg
                - output.exhaust_fresh_air_kg
                - output.fuel_burned_kg * AFR;
            fuel += output.intake_fuel_kg - output.exhaust_fuel_kg + output.injected_fuel_kg
                - output.fuel_burned_kg;
            assert!((fresh - output.fresh_air_mass_kg).abs() < 1e-12);
            assert!((fuel - output.fuel_mass_kg).abs() < 1e-12);
            assert!(
                output.fresh_air_mass_kg + output.fuel_mass_kg
                    <= cylinder.gas_state().unwrap().mass_kg() + 1e-12
            );
            intake_in += output.intake_mass_kg.max(0.);
            intake_out += (-output.intake_mass_kg).max(0.);
            exhaust_in += (-output.exhaust_mass_kg).max(0.);
            exhaust_out += output.exhaust_mass_kg.max(0.);
        }
        assert!(
            [intake_in, intake_out, exhaust_in, exhaust_out]
                .iter()
                .all(|&x| x > 1e-6),
            "both ports must exercise both directions: {intake_in} {intake_out} {exhaust_in} {exhaust_out}"
        );
    }

    #[test]
    fn premixed_returning_fuel_is_not_injected_twice() {
        let mut cylinder = CycleCylinder::new(config(), 0., 9).unwrap();
        for i in 1..=12000 {
            let output = cylinder
                .step(CycleInput {
                    angle_rad: i as f64 / 96000. * 3000. * TAU / 60.,
                    rpm: 3000.,
                    intake_fresh_air_fraction: 0.8,
                    intake_fuel_fraction: 0.8 / AFR,
                    ..Default::default()
                })
                .unwrap();
            assert!(output.injected_fuel_kg.abs() < 1e-18);
        }
    }
}
