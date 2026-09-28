//! Single-zone cylinder foundation (SI units, crank angles in radians).
//!
//! The air-like gas is an engineering approximation, not a chemical mechanism:
//! `cv(T) = 650 + 0.16 T` J/(kg K), `R = 287` J/(kg K). Integrating cv gives
//! a consistent internal energy and a decreasing, temperature-dependent gamma.
//! No allocation, random source, or wall clock is used by a simulation step.

use std::f64::consts::PI;

pub const GAS_CONSTANT: f64 = 287.0;
pub const MIN_TEMPERATURE_K: f64 = 200.0;
pub const MAX_TEMPERATURE_K: f64 = 3500.0;
const CV_INTERCEPT: f64 = 650.0;
const CV_SLOPE: f64 = 0.16;
const MIN_MASS_KG: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermoError {
    InvalidGeometry,
    InvalidState,
    InvalidInput,
    InvalidCombustion,
}

/// Heat capacity at constant volume, J/(kg K). Temperature must be finite.
pub fn specific_heat_cv(temperature_k: f64) -> f64 {
    CV_INTERCEPT + CV_SLOPE * temperature_k
}

/// Specific internal energy, J/kg, with the reference u(0 K) = 0.
pub fn specific_internal_energy(temperature_k: f64) -> f64 {
    temperature_k * (CV_INTERCEPT + 0.5 * CV_SLOPE * temperature_k)
}

/// Specific stagnation enthalpy for a zero-velocity volume, J/kg.
pub fn specific_enthalpy(temperature_k: f64) -> f64 {
    specific_internal_energy(temperature_k) + GAS_CONSTANT * temperature_k
}

pub fn gamma(temperature_k: f64) -> f64 {
    1.0 + GAS_CONSTANT / specific_heat_cv(temperature_k)
}

fn temperature_from_energy(specific_energy: f64) -> f64 {
    // Rationalized quadratic root avoids cancellation for cold/empty volumes.
    2.0 * specific_energy
        / (CV_INTERCEPT + (CV_INTERCEPT * CV_INTERCEPT + 2.0 * CV_SLOPE * specific_energy).sqrt())
}

/// Exact, zero-offset slider crank. Angle zero is top dead centre.
#[derive(Debug, Clone, Copy)]
pub struct SliderCrank {
    bore_m: f64,
    radius_m: f64,
    rod_m: f64,
    piston_area_m2: f64,
    clearance_m3: f64,
}

impl SliderCrank {
    /// Supported design envelope: bore/stroke 5–500 mm, CR 1.01–50,
    /// rod length 1.01–10 crank radii. Invalid designs are rejected.
    pub fn new(
        bore_m: f64,
        stroke_m: f64,
        rod_m: f64,
        compression_ratio: f64,
    ) -> Result<Self, ThermoError> {
        if !(0.005..=0.5).contains(&bore_m)
            || !(0.005..=0.5).contains(&stroke_m)
            || !(1.01..=50.0).contains(&compression_ratio)
            || !(stroke_m * 0.505..=stroke_m * 5.0).contains(&rod_m)
        {
            return Err(ThermoError::InvalidGeometry);
        }
        let piston_area_m2 = PI * bore_m * bore_m / 4.0;
        Ok(Self {
            bore_m,
            radius_m: stroke_m * 0.5,
            rod_m,
            piston_area_m2,
            clearance_m3: piston_area_m2 * stroke_m / (compression_ratio - 1.0),
        })
    }

    pub fn displacement_m3(self) -> f64 {
        self.piston_area_m2 * 2.0 * self.radius_m
    }

    pub fn clearance_m3(self) -> f64 {
        self.clearance_m3
    }

    /// Instantaneous chamber volume, m³. Caller supplies a finite angle.
    pub fn volume(self, angle_rad: f64) -> f64 {
        let (s, c) = angle_rad.sin_cos();
        let root = (self.rod_m.powi(2) - self.radius_m.powi(2) * s * s).sqrt();
        // Rationalization preserves the small near-TDC piston displacement.
        let travel =
            self.radius_m * (1.0 - c) + self.radius_m.powi(2) * s * s / (self.rod_m + root);
        self.clearance_m3 + self.piston_area_m2 * travel
    }

    /// Analytic dV/dθ, m³/radian. Gas torque is (p - p_crankcase) * dV/dθ.
    pub fn volume_derivative(self, angle_rad: f64) -> f64 {
        let (s, c) = angle_rad.sin_cos();
        let root = (self.rod_m.powi(2) - self.radius_m.powi(2) * s * s).sqrt();
        self.piston_area_m2 * self.radius_m * s * (1.0 + self.radius_m * c / root)
    }

    /// Joint evaluation for the hot path: exactly the same arithmetic as the
    /// individual methods, sharing their sine/cosine and square root.
    pub fn volume_and_derivative(self, angle_rad: f64) -> (f64, f64) {
        let (s, c) = angle_rad.sin_cos();
        let root = (self.rod_m.powi(2) - self.radius_m.powi(2) * s * s).sqrt();
        let travel =
            self.radius_m * (1.0 - c) + self.radius_m.powi(2) * s * s / (self.rod_m + root);
        (
            self.clearance_m3 + self.piston_area_m2 * travel,
            self.piston_area_m2 * self.radius_m * s * (1.0 + self.radius_m * c / root),
        )
    }

    /// Wall area from an already evaluated volume, avoiding another crank solve.
    pub fn chamber_area_from_volume(self, volume_m3: f64) -> f64 {
        2.0 * self.piston_area_m2 + 4.0 * volume_m3 / self.bore_m
    }

    /// Flat head + piston + exposed cylinder wall approximation, m².
    pub fn chamber_area(self, angle_rad: f64) -> f64 {
        self.chamber_area_from_volume(self.volume(angle_rad))
    }
}

/// Finite-volume energy inputs integrated over one step (not rates).
#[derive(Debug, Default, Clone, Copy)]
pub struct EnergyInput {
    pub heat_j: f64,
    /// Signed heat leaving the gas. Negative values heat gas from a hot wall.
    pub wall_heat_j: f64,
    pub mass_in_kg: f64,
    /// Total enthalpy carried by incoming mass, J; use the donor temperature.
    pub enthalpy_in_j: f64,
    pub mass_out_kg: f64,
}

/// Every accepted step satisfies ΔU = Q - Qwall + Hin - Hout - W + correction.
/// A nonzero correction indicates that a numerical temperature guard intervened;
/// it must never be included as physical combustion or silently ignored in audits.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnergyLedger {
    pub internal_energy_change_j: f64,
    pub heat_j: f64,
    pub wall_heat_j: f64,
    pub enthalpy_in_j: f64,
    pub enthalpy_out_j: f64,
    /// Positive when gas does expansion work on the piston.
    pub boundary_work_j: f64,
    pub numerical_correction_j: f64,
    pub mass_in_kg: f64,
    /// Actual bounded outflow; feed this back into the receiving volume.
    pub mass_out_kg: f64,
}

impl EnergyLedger {
    pub fn residual_j(self) -> f64 {
        self.internal_energy_change_j
            - (self.heat_j - self.wall_heat_j + self.enthalpy_in_j
                - self.enthalpy_out_j
                - self.boundary_work_j
                + self.numerical_correction_j)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GasState {
    mass_kg: f64,
    internal_energy_j: f64,
    volume_m3: f64,
}

impl GasState {
    /// Positive mass 1e-12–10 kg, volume 1e-12–100 m³, temperature 200–3500 K.
    pub fn new(mass_kg: f64, temperature_k: f64, volume_m3: f64) -> Result<Self, ThermoError> {
        if !(MIN_MASS_KG..=10.0).contains(&mass_kg)
            || !(MIN_TEMPERATURE_K..=MAX_TEMPERATURE_K).contains(&temperature_k)
            || !(1e-12..=100.0).contains(&volume_m3)
        {
            return Err(ThermoError::InvalidState);
        }
        Ok(Self {
            mass_kg,
            internal_energy_j: mass_kg * specific_internal_energy(temperature_k),
            volume_m3,
        })
    }

    pub fn mass_kg(self) -> f64 {
        self.mass_kg
    }

    pub fn internal_energy_j(self) -> f64 {
        self.internal_energy_j
    }

    pub fn volume_m3(self) -> f64 {
        self.volume_m3
    }

    pub fn temperature_k(self) -> f64 {
        temperature_from_energy(self.internal_energy_j / self.mass_kg)
    }

    pub fn pressure_pa(self) -> f64 {
        self.mass_kg * GAS_CONSTANT * self.temperature_k() / self.volume_m3
    }

    /// Conservative first-law step using trapezoidal p dV and old-state outflow
    /// enthalpy. The pressure/energy coupling is solved analytically. Integrate
    /// valve rates with small external time steps; this is not an adaptive ODE.
    /// At most 90% of the original donor mass can leave in one call. Invalid
    /// inputs return an error without changing state. Heat inputs are limited
    /// to a deliberately broad ±1e9 J; safety clamps are reported in the ledger.
    pub fn step(
        &mut self,
        new_volume_m3: f64,
        input: EnergyInput,
    ) -> Result<EnergyLedger, ThermoError> {
        if !(1e-12..=100.0).contains(&new_volume_m3)
            || !(0.0..=10.0).contains(&input.mass_in_kg)
            || !(0.0..=10.0).contains(&input.mass_out_kg)
            || !(-1e9..=1e9).contains(&input.heat_j)
            || !(-1e9..=1e9).contains(&input.wall_heat_j)
            || !(0.0..=1e9).contains(&input.enthalpy_in_j)
            || (input.mass_in_kg == 0.0 && input.enthalpy_in_j != 0.0)
        {
            return Err(ThermoError::InvalidInput);
        }
        let mass_out = input
            .mass_out_kg
            .min(0.9 * self.mass_kg)
            .min((self.mass_kg - MIN_MASS_KG).max(0.0));
        let new_mass = self.mass_kg + input.mass_in_kg - mass_out;
        if !(MIN_MASS_KG..=10.0).contains(&new_mass) {
            return Err(ThermoError::InvalidInput);
        }
        let old_temperature = self.temperature_k();
        let old_pressure = self.pressure_pa();
        let delta_volume = new_volume_m3 - self.volume_m3;
        let enthalpy_out = mass_out * specific_enthalpy(old_temperature);
        let supplied = input.heat_j - input.wall_heat_j + input.enthalpy_in_j - enthalpy_out;
        let rhs =
            (self.internal_energy_j + supplied - 0.5 * old_pressure * delta_volume) / new_mass;
        let linear = CV_INTERCEPT + 0.5 * GAS_CONSTANT * delta_volume / new_volume_m3;
        let discriminant = linear * linear + 2.0 * CV_SLOPE * rhs;
        let temperature = if discriminant >= 0.0 && rhs > 0.0 {
            if linear >= 0.0 {
                2.0 * rhs / (linear + discriminant.sqrt())
            } else {
                (-linear + discriminant.sqrt()) / CV_SLOPE
            }
        } else {
            MIN_TEMPERATURE_K
        }
        .clamp(MIN_TEMPERATURE_K, MAX_TEMPERATURE_K);
        let new_energy = new_mass * specific_internal_energy(temperature);
        let new_pressure = new_mass * GAS_CONSTANT * temperature / new_volume_m3;
        let work = 0.5 * (old_pressure + new_pressure) * delta_volume;
        let energy_change = new_energy - self.internal_energy_j;
        let ledger = EnergyLedger {
            internal_energy_change_j: energy_change,
            heat_j: input.heat_j,
            wall_heat_j: input.wall_heat_j,
            enthalpy_in_j: input.enthalpy_in_j,
            enthalpy_out_j: enthalpy_out,
            boundary_work_j: work,
            numerical_correction_j: energy_change - supplied + work,
            mass_in_kg: input.mass_in_kg,
            mass_out_kg: mass_out,
        };
        self.mass_kg = new_mass;
        self.internal_energy_j = new_energy;
        self.volume_m3 = new_volume_m3;
        Ok(ledger)
    }
}

/// A finite-duration, normalized Wiebe burn. CA50 is exactly 50% of total heat,
/// including the normalization at the end of the prescribed duration.
#[derive(Debug, Clone, Copy)]
pub struct Wiebe {
    start_rad: f64,
    duration_rad: f64,
    exponent: f64,
    coefficient: f64,
    normalization: f64,
}

impl Wiebe {
    /// Shape is the Wiebe m (exponent m+1). Completion is the underlying
    /// unnormalized burned fraction at duration, normally 1-exp(-5).
    pub fn from_ca50(
        ca50_rad: f64,
        duration_rad: f64,
        shape: f64,
        completion: f64,
    ) -> Result<Self, ThermoError> {
        if !ca50_rad.is_finite()
            || !(0.001..=4.0 * PI).contains(&duration_rad)
            || !(0.0..=10.0).contains(&shape)
            || !(0.9..=0.999999).contains(&completion)
        {
            return Err(ThermoError::InvalidCombustion);
        }
        let coefficient = -(-completion).ln_1p();
        let exponent = shape + 1.0;
        let fraction50 = (-(-0.5 * completion).ln_1p() / coefficient).powf(1.0 / exponent);
        Ok(Self {
            start_rad: ca50_rad - duration_rad * fraction50,
            duration_rad,
            exponent,
            coefficient,
            normalization: completion,
        })
    }

    pub fn start_rad(self) -> f64 {
        self.start_rad
    }

    /// Unwrapped angle relative to this particular burn; do not wrap at TDC.
    pub fn fraction(self, angle_rad: f64) -> f64 {
        let x = ((angle_rad - self.start_rad) / self.duration_rad).clamp(0.0, 1.0);
        -(-self.coefficient * x.powf(self.exponent)).exp_m1() / self.normalization
    }

    /// Integrated release avoids missing heat when a step straddles ignition.
    pub fn released_heat(self, from_rad: f64, to_rad: f64, total_j: f64) -> f64 {
        (self.fraction(to_rad) - self.fraction(from_rad)).max(0.0) * total_j.max(0.0)
    }
}

/// Newton wall heat over a time step, J, positive out of gas. The coefficient
/// W/(m² K) is supplied by the caller, so uncalibrated correlations are explicit.
/// Invalid inputs return zero. Use GasState's correction ledger to detect an
/// excessively large cooling step rather than silently counting it as physics.
pub fn wall_heat_j(
    coefficient_w_m2_k: f64,
    area_m2: f64,
    gas_temperature_k: f64,
    wall_temperature_k: f64,
    dt_s: f64,
) -> f64 {
    if !(0.0..=1e6).contains(&coefficient_w_m2_k)
        || !(0.0..=100.0).contains(&area_m2)
        || !(0.0..=1.0).contains(&dt_s)
        || !(MIN_TEMPERATURE_K..=MAX_TEMPERATURE_K).contains(&gas_temperature_k)
        || !(MIN_TEMPERATURE_K..=MAX_TEMPERATURE_K).contains(&wall_temperature_k)
    {
        return 0.0;
    }
    coefficient_w_m2_k * area_m2 * (gas_temperature_k - wall_temperature_k) * dt_s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> SliderCrank {
        SliderCrank::new(0.086, 0.086, 0.143, 10.0).unwrap()
    }

    #[test]
    fn geometry_endpoints_and_analytic_derivative() {
        let g = geometry();
        assert!((g.volume(PI) / g.volume(0.0) - 10.0).abs() < 1e-12);
        for i in 0..1000 {
            let angle = i as f64 * 2.0 * PI / 1000.0;
            let joint = g.volume_and_derivative(angle);
            assert_eq!(joint.0.to_bits(), g.volume(angle).to_bits());
            assert_eq!(joint.1.to_bits(), g.volume_derivative(angle).to_bits());
            let numerical = (g.volume(angle + 1e-5) - g.volume(angle - 1e-5)) / 2e-5;
            assert!((numerical - g.volume_derivative(angle)).abs() < 1e-12);
        }
        assert!(SliderCrank::new(0.086, 0.086, 0.04, 10.0).is_err());
        assert!(SliderCrank::new(f64::NAN, 0.086, 0.143, 10.0).is_err());
    }

    #[test]
    fn variable_heat_capacity_has_consistent_energy_derivative() {
        for t in [200.0, 300.0, 1000.0, 2500.0, 3500.0] {
            let derivative =
                (specific_internal_energy(t + 0.01) - specific_internal_energy(t - 0.01)) / 0.02;
            assert!((derivative - specific_heat_cv(t)).abs() < 1e-6);
            assert!((temperature_from_energy(specific_internal_energy(t)) - t).abs() < 1e-10);
        }
        assert!(gamma(300.0) > gamma(2500.0));
    }

    #[test]
    fn motored_compression_obeys_variable_cv_entropy_and_returns_energy() {
        let g = geometry();
        let mass = 101325.0 * g.volume(PI) / (GAS_CONSTANT * 300.0);
        let mut gas = GasState::new(mass, 300.0, g.volume(PI)).unwrap();
        let initial_energy = gas.internal_energy_j();
        let mut work = 0.0;
        let mut correction = 0.0;
        for i in 1..=7200 {
            let angle = PI + i as f64 * 2.0 * PI / 7200.0;
            let ledger = gas.step(g.volume(angle), EnergyInput::default()).unwrap();
            work += ledger.boundary_work_j;
            correction += ledger.numerical_correction_j.abs();
            assert!(ledger.residual_j().abs() < 1e-10);
            if i == 3600 {
                // ds = cv0 ln(T/T0) + slope*(T-T0) + R ln(V/V0).
                let t = gas.temperature_k();
                let entropy = CV_INTERCEPT * (t / 300.0).ln()
                    + CV_SLOPE * (t - 300.0)
                    + GAS_CONSTANT * (gas.volume_m3() / g.volume(PI)).ln();
                assert!(entropy.abs() < 0.001, "ds={entropy}");
                assert!(gas.pressure_pa() > 101325.0 * 10_f64.powf(1.35));
                assert!(gas.pressure_pa() < 101325.0 * 10_f64.powf(1.42));
            }
        }
        assert!((gas.internal_energy_j() / initial_energy - 1.0).abs() < 1e-8);
        assert!(work.abs() < initial_energy * 1e-8);
        assert!(correction < initial_energy * 1e-8);
    }

    #[test]
    fn closed_volume_heat_and_open_volume_enthalpy_are_accounted() {
        let mut gas = GasState::new(0.001, 500.0, 0.001).unwrap();
        let before = gas.internal_energy_j();
        let l = gas
            .step(
                0.001,
                EnergyInput {
                    heat_j: 100.0,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!((gas.internal_energy_j() - before - 100.0).abs() < 1e-10);
        assert!(l.numerical_correction_j.abs() < 1e-10);
        let old_mass = gas.mass_kg();
        let old_t = gas.temperature_k();
        let l = gas
            .step(
                0.001,
                EnergyInput {
                    mass_in_kg: 0.00001,
                    enthalpy_in_j: 0.00001 * specific_enthalpy(300.0),
                    mass_out_kg: 0.00002,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!((gas.mass_kg() - (old_mass - 0.00001)).abs() < 1e-15);
        assert!((l.enthalpy_out_j - 0.00002 * specific_enthalpy(old_t)).abs() < 1e-12);
        assert!(l.residual_j().abs() < 1e-10);
        assert!(l.numerical_correction_j.abs() < 1e-10);
    }

    #[test]
    fn bounded_steps_expose_numerical_energy_and_preserve_invalid_state() {
        let mut gas = GasState::new(0.001, 300.0, 0.001).unwrap();
        let l = gas
            .step(
                0.001,
                EnergyInput {
                    heat_j: 1e6,
                    mass_out_kg: 1.0,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(l.numerical_correction_j.abs() > 1.0);
        assert!((l.mass_out_kg - 0.0009).abs() < 1e-15);
        assert!(l.residual_j().abs() < 1e-8);
        assert!(gas.temperature_k() <= MAX_TEMPERATURE_K + 1e-9);
        let before = gas.internal_energy_j();
        assert!(gas.step(f64::NAN, EnergyInput::default()).is_err());
        assert_eq!(gas.internal_energy_j(), before);
    }

    #[test]
    fn wiebe_ca50_and_integrated_heat_are_exact() {
        let ca50 = 8_f64.to_radians();
        let burn = Wiebe::from_ca50(ca50, 60_f64.to_radians(), 2.0, 1.0 - (-5_f64).exp()).unwrap();
        assert!((burn.fraction(ca50) - 0.5).abs() < 1e-14);
        let mut total = 0.0;
        for i in 0..1000 {
            total += burn.released_heat(
                -PI + i as f64 * 2.0 * PI / 1000.0,
                -PI + (i + 1) as f64 * 2.0 * PI / 1000.0,
                1000.0,
            );
        }
        assert!((total - 1000.0).abs() < 1e-9);
        assert_eq!(burn.released_heat(1.0, 0.0, 1000.0), 0.0);
    }

    #[test]
    fn fired_cycle_delivers_work_without_numerical_heat() {
        let g = geometry();
        let mass = 101325.0 * g.volume(PI) / (GAS_CONSTANT * 300.0);
        let mut gas = GasState::new(mass, 300.0, g.volume(PI)).unwrap();
        let before = gas.internal_energy_j();
        let burn = Wiebe::from_ca50(
            2.0 * PI + 8_f64.to_radians(),
            60_f64.to_radians(),
            2.0,
            1.0 - (-5_f64).exp(),
        )
        .unwrap();
        let mut heat = 0.0;
        let mut wall = 0.0;
        let mut work = 0.0;
        let mut correction = 0.0;
        let mut previous_angle = PI;
        for i in 1..=7200 {
            let angle = PI + i as f64 * 2.0 * PI / 7200.0;
            let input = EnergyInput {
                heat_j: burn.released_heat(previous_angle, angle, 500.0),
                wall_heat_j: wall_heat_j(
                    100.0,
                    g.chamber_area(previous_angle),
                    gas.temperature_k(),
                    370.0,
                    1.0 / (50.0 * 7200.0),
                ),
                ..Default::default()
            };
            let l = gas.step(g.volume(angle), input).unwrap();
            heat += l.heat_j;
            wall += l.wall_heat_j;
            work += l.boundary_work_j;
            correction += l.numerical_correction_j.abs();
            previous_angle = angle;
        }
        assert!((heat - 500.0).abs() < 1e-9);
        assert!(work > 100.0, "cycle work = {work} J");
        assert!(wall > 0.0);
        assert!(correction < 1e-7);
        let energy_error = gas.internal_energy_j() - before - heat + wall + work;
        assert!(energy_error.abs() / heat < 1e-8);
    }

    #[test]
    fn ten_thousand_designs_are_finite_and_deterministic() {
        fn sweep() -> u64 {
            let mut seed = 0x123456789abcdef0_u64;
            let mut random = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                (seed >> 11) as f64 / ((1_u64 << 53) as f64)
            };
            let mut fingerprint = 0_u64;
            for _ in 0..10_000 {
                let bore = 0.005 + random() * 0.495;
                let stroke = 0.005 + random() * 0.495;
                let rod = stroke * (0.505 + random() * 4.495);
                let g = SliderCrank::new(bore, stroke, rod, 1.01 + random() * 48.99).unwrap();
                let t = 250.0 + random() * 400.0;
                let mass = 101325.0 * g.volume(PI) / (GAS_CONSTANT * t);
                let mut gas = GasState::new(mass, t, g.volume(PI)).unwrap();
                for i in 1..=128 {
                    let angle = PI + i as f64 * 2.0 * PI / 128.0;
                    let l = gas.step(g.volume(angle), EnergyInput::default()).unwrap();
                    assert!(gas.pressure_pa().is_finite());
                    assert!(gas.temperature_k().is_finite());
                    assert!(l.residual_j().abs() < 1e-6);
                    assert!(g.volume_derivative(angle).is_finite());
                    fingerprint = fingerprint.rotate_left(1) ^ gas.pressure_pa().to_bits();
                }
            }
            fingerprint
        }
        assert_eq!(sweep(), sweep());
    }
}
