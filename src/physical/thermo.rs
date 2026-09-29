//! Single-zone cylinder foundation (SI units, crank angles in radians).
//!
//! Gas is an ideal mixture of three frozen species: fresh air, gasoline vapour
//! and stoichiometric combustion products. Each has cv(T) = a + b T, a linear
//! least-squares fit to standard data: air and products from the NASA 7-term
//! polynomials (GRI-Mech 3.0) of N2/O2 (23.3 % O2 by mass) and of the CO2/H2O/N2
//! of stoichiometric C8H15 (dissociation ignored); gasoline vapour from
//! Heywood's C8.26H15.5 cp fit (*Internal Combustion Engine Fundamentals*,
//! table D.4). Mass-weighted mixing keeps cv linear in T, so the energy/
//! temperature inversion stays an analytic quadratic root. Sensible energies
//! share one reference: every species has air's u at 298.15 K, so turning
//! reactants into products at constant energy releases nothing and the heat
//! of combustion is exactly the LHV the caller adds.
//! No allocation, random source, or wall clock is used by a simulation step.

use super::gas::GasProperties;
use std::f64::consts::PI;

pub const MIN_TEMPERATURE_K: f64 = 200.0;
pub const MAX_TEMPERATURE_K: f64 = 3500.0;
const MIN_MASS_KG: f64 = 1e-12;
const REFERENCE_K: f64 = 298.15;
const AIR_CV: (f64, f64) = (662.0, 0.190);
const AIR_REFERENCE_U: f64 = REFERENCE_K * (AIR_CV.0 + 0.5 * AIR_CV.1 * REFERENCE_K);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermoError {
    InvalidGeometry,
    InvalidState,
    InvalidInput,
    InvalidCombustion,
}

/// Frozen ideal-gas mixture: cv = cv0 + cv1 T J/(kg K), u = u0 + ∫cv dT.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mixture {
    cv0: f64,
    cv1: f64,
    u0: f64,
    gas_constant: f64,
}

impl Mixture {
    /// Fit 250–1500 K, within ±1.7 % of the NASA data.
    pub const AIR: Self = Self::species(AIR_CV.0, AIR_CV.1, 287.0);
    /// Fit 300–800 K (compression and end gas), −2.3…+6.4 %; +11 % at 1000 K.
    pub const FUEL: Self = Self::species(640.0, 3.16, 72.4);
    /// Fit 500–3000 K (expansion and exhaust), −3.3…+9.3 %, frozen products.
    pub const PRODUCTS: Self = Self::species(841.0, 0.136, 287.7);

    const fn species(cv0: f64, cv1: f64, gas_constant: f64) -> Self {
        Self {
            cv0,
            cv1,
            u0: AIR_REFERENCE_U - REFERENCE_K * (cv0 + 0.5 * cv1 * REFERENCE_K),
            gas_constant,
        }
    }

    /// Mass fractions of unconsumed air and unburned fuel; the remainder is
    /// products. Fractions summing above one are renormalized.
    pub fn from_fractions(fresh_air: f64, fuel: f64) -> Self {
        let (mut a, mut f) = (fresh_air.clamp(0.0, 1.0), fuel.clamp(0.0, 1.0));
        if a + f > 1.0 {
            let scale = 1.0 / (a + f);
            (a, f) = (a * scale, f * scale);
        }
        let p = (1.0 - a - f).max(0.0);
        let mix = |x: fn(Self) -> f64| a * x(Self::AIR) + f * x(Self::FUEL) + p * x(Self::PRODUCTS);
        Self {
            cv0: mix(|m| m.cv0),
            cv1: mix(|m| m.cv1),
            u0: mix(|m| m.u0),
            gas_constant: mix(|m| m.gas_constant),
        }
    }

    pub fn gas_constant(self) -> f64 {
        self.gas_constant
    }

    /// Heat capacity at constant volume, J/(kg K). Temperature must be finite.
    pub fn cv(self, temperature_k: f64) -> f64 {
        self.cv0 + self.cv1 * temperature_k
    }

    /// Specific sensible internal energy, J/kg (common 298.15 K reference).
    pub fn internal_energy(self, temperature_k: f64) -> f64 {
        self.u0 + temperature_k * (self.cv0 + 0.5 * self.cv1 * temperature_k)
    }

    /// Specific stagnation enthalpy for a zero-velocity volume, J/kg.
    pub fn enthalpy(self, temperature_k: f64) -> f64 {
        self.internal_energy(temperature_k) + self.gas_constant * temperature_k
    }

    pub fn gamma(self, temperature_k: f64) -> f64 {
        1.0 + self.gas_constant / self.cv(temperature_k)
    }

    /// Orifice properties of this gas as the upstream donor at `temperature_k`.
    pub fn properties(self, temperature_k: f64) -> GasProperties {
        GasProperties {
            gas_constant_j_kg_k: self.gas_constant,
            gamma: self.gamma(temperature_k),
        }
    }

    fn temperature(self, specific_energy: f64) -> f64 {
        // Rationalized quadratic root avoids cancellation for cold/empty volumes.
        let e = specific_energy - self.u0;
        2.0 * e / (self.cv0 + (self.cv0 * self.cv0 + 2.0 * self.cv1 * e).sqrt())
    }
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
    /// Composition after this step (inflow, reaction); None keeps the current.
    /// Outflow always leaves with the old, well-mixed composition.
    pub mixture: Option<Mixture>,
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
    mixture: Mixture,
}

impl GasState {
    /// Positive mass 1e-12–10 kg, volume 1e-12–100 m³, temperature 200–3500 K.
    pub fn new(
        mass_kg: f64,
        temperature_k: f64,
        volume_m3: f64,
        mixture: Mixture,
    ) -> Result<Self, ThermoError> {
        if !(MIN_MASS_KG..=10.0).contains(&mass_kg)
            || !(MIN_TEMPERATURE_K..=MAX_TEMPERATURE_K).contains(&temperature_k)
            || !(1e-12..=100.0).contains(&volume_m3)
        {
            return Err(ThermoError::InvalidState);
        }
        Ok(Self {
            mass_kg,
            internal_energy_j: mass_kg * mixture.internal_energy(temperature_k),
            volume_m3,
            mixture,
        })
    }

    /// Density-consistent state at pressure/temperature: m = pV/(RT).
    pub fn at_pressure(
        volume_m3: f64,
        pressure_pa: f64,
        temperature_k: f64,
        mixture: Mixture,
    ) -> Result<Self, ThermoError> {
        let mass = pressure_pa * volume_m3 / (mixture.gas_constant() * temperature_k);
        Self::new(mass, temperature_k, volume_m3, mixture)
    }

    pub fn mixture(self) -> Mixture {
        self.mixture
    }

    /// New composition of the same mass and energy, e.g. after species
    /// transport or a tracer reaction in a fixed volume: T and p follow.
    pub fn set_mixture(&mut self, mixture: Mixture) {
        self.mixture = mixture;
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

    /// Clamped to the step's guard range: a state `step` placed on a bound
    /// must not invert to 1e-13 K outside it (the energy offset costs a ulp).
    pub fn temperature_k(self) -> f64 {
        self.mixture
            .temperature(self.internal_energy_j / self.mass_kg)
            .clamp(MIN_TEMPERATURE_K, MAX_TEMPERATURE_K)
    }

    pub fn pressure_pa(self) -> f64 {
        self.pressure_at(self.temperature_k())
    }

    /// `(temperature_k(), pressure_pa())` sharing one energy inversion.
    pub fn temperature_pressure(self) -> (f64, f64) {
        let temperature = self.temperature_k();
        (temperature, self.pressure_at(temperature))
    }

    fn pressure_at(self, temperature_k: f64) -> f64 {
        self.mass_kg * self.mixture.gas_constant() * temperature_k / self.volume_m3
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
        let (old_temperature, old_pressure) = self.temperature_pressure();
        let delta_volume = new_volume_m3 - self.volume_m3;
        let enthalpy_out = mass_out * self.mixture.enthalpy(old_temperature);
        let mixture = input.mixture.unwrap_or(self.mixture);
        let supplied = input.heat_j - input.wall_heat_j + input.enthalpy_in_j - enthalpy_out;
        // u0 + cv0 T + cv1 T²/2 + R T ΔV/(2V) = rhs + u0, solved for T.
        let rhs = (self.internal_energy_j + supplied - 0.5 * old_pressure * delta_volume)
            / new_mass
            - mixture.u0;
        let linear = mixture.cv0 + 0.5 * mixture.gas_constant * delta_volume / new_volume_m3;
        let discriminant = linear * linear + 2.0 * mixture.cv1 * rhs;
        let temperature = if discriminant >= 0.0 && rhs > 0.0 {
            if linear >= 0.0 {
                2.0 * rhs / (linear + discriminant.sqrt())
            } else {
                (-linear + discriminant.sqrt()) / mixture.cv1
            }
        } else {
            MIN_TEMPERATURE_K
        }
        .clamp(MIN_TEMPERATURE_K, MAX_TEMPERATURE_K);
        let new_energy = new_mass * mixture.internal_energy(temperature);
        let new_pressure = new_mass * mixture.gas_constant * temperature / new_volume_m3;
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
        self.mixture = mixture;
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

    /// Stoichiometric gasoline–air charge with 5 % residual products.
    fn charge() -> Mixture {
        Mixture::from_fractions(0.95 * 14.7 / 15.7, 0.95 / 15.7)
    }

    #[test]
    fn variable_heat_capacity_has_consistent_energy_derivative() {
        for m in [Mixture::AIR, Mixture::FUEL, Mixture::PRODUCTS, charge()] {
            for t in [200.0, 300.0, 1000.0, 2500.0, 3500.0] {
                let derivative = (m.internal_energy(t + 0.01) - m.internal_energy(t - 0.01)) / 0.02;
                assert!((derivative - m.cv(t)).abs() < 1e-6);
                assert!((m.temperature(m.internal_energy(t)) - t).abs() < 1e-9);
                assert!(m.internal_energy(t) > 0.0 && m.enthalpy(t) > 0.0);
            }
            assert!(m.gamma(300.0) > m.gamma(2500.0));
            // Common reference: a reaction at constant sensible energy is
            // athermal, so the caller's LHV is the whole heat of combustion.
            let u = m.internal_energy(REFERENCE_K);
            assert!((u - Mixture::AIR.internal_energy(REFERENCE_K)).abs() < 1e-9);
        }
        assert_eq!(Mixture::from_fractions(1.0, 0.0), Mixture::AIR);
        assert_eq!(Mixture::from_fractions(0.0, 0.0), Mixture::PRODUCTS);
        let over = Mixture::from_fractions(0.7, 0.6).enthalpy(500.0);
        assert!(
            (over - Mixture::from_fractions(0.7 / 1.3, 0.6 / 1.3).enthalpy(500.0)).abs() < 1e-6
        );
        // Heywood §4.7: γ of the unburned charge ≈ 1.3 in compression; burned
        // gas lower still. The former single air cv gave 1.39 here.
        assert!((1.29..1.34).contains(&charge().gamma(600.0)));
        assert!((1.25..1.29).contains(&Mixture::PRODUCTS.gamma(2000.0)));
    }

    /// Adiabatic 10:1 compression and return: isentrope of the variable cv,
    /// full energy recovery, and the polytropic exponent of the composition.
    fn motored(mixture: Mixture, t0: f64) -> f64 {
        let g = geometry();
        let mut gas = GasState::at_pressure(g.volume(PI), 101325.0, t0, mixture).unwrap();
        let initial_energy = gas.internal_energy_j();
        let mut work = 0.0;
        let mut correction = 0.0;
        let mut exponent = 0.0;
        for i in 1..=7200 {
            let angle = PI + i as f64 * 2.0 * PI / 7200.0;
            let ledger = gas.step(g.volume(angle), EnergyInput::default()).unwrap();
            work += ledger.boundary_work_j;
            correction += ledger.numerical_correction_j.abs();
            assert!(ledger.residual_j().abs() < 1e-10);
            if i == 3600 {
                // ds = cv0 ln(T/T0) + cv1 (T-T0) + R ln(V/V0).
                let t = gas.temperature_k();
                let entropy = mixture.cv0 * (t / t0).ln()
                    + mixture.cv1 * (t - t0)
                    + mixture.gas_constant * (gas.volume_m3() / g.volume(PI)).ln();
                assert!(entropy.abs() < 0.001, "ds={entropy}");
                exponent = (gas.pressure_pa() / 101325.0).ln() / 10_f64.ln();
            }
        }
        assert!((gas.internal_energy_j() / initial_energy - 1.0).abs() < 1e-8);
        assert!(work.abs() < initial_energy * 1e-8);
        assert!(correction < initial_energy * 1e-8);
        exponent
    }

    #[test]
    fn motored_compression_obeys_variable_cv_entropy_and_returns_energy() {
        let air = motored(Mixture::AIR, 300.0);
        let fuel_air = motored(charge(), 350.0);
        println!("adiabatic 10:1 exponent: air {air:.4}, fuel–air charge {fuel_air:.4}");
        assert!((1.36..1.40).contains(&air), "{air}");
        assert!((1.30..1.33).contains(&fuel_air), "{fuel_air}");
    }

    #[test]
    fn mixed_composition_inflow_reaction_and_outflow_close_the_ledger() {
        let mut gas = GasState::new(0.0005, 700.0, 0.0005, charge()).unwrap();
        let old = gas;
        let incoming = Mixture::from_fractions(0.3, 0.02);
        let l = gas
            .step(
                0.0004,
                EnergyInput {
                    heat_j: 50.0,
                    wall_heat_j: 3.0,
                    mass_in_kg: 1e-5,
                    enthalpy_in_j: 1e-5 * incoming.enthalpy(900.0),
                    mass_out_kg: 2e-5,
                    mixture: Some(Mixture::from_fractions(0.5, 0.01)),
                },
            )
            .unwrap();
        assert!(l.residual_j().abs() < 1e-10);
        assert!(l.numerical_correction_j.abs() < 1e-10);
        let outflow = 2e-5 * charge().enthalpy(old.temperature_k());
        assert!((l.enthalpy_out_j - outflow).abs() < 1e-12);
        assert_eq!(gas.mixture(), Mixture::from_fractions(0.5, 0.01));
        // Changing composition at fixed mass and energy moves T, not U.
        let (u, t) = (gas.internal_energy_j(), gas.temperature_k());
        gas.set_mixture(Mixture::PRODUCTS);
        assert_eq!(gas.internal_energy_j(), u);
        assert!(gas.temperature_k() < t);
    }

    #[test]
    fn closed_volume_heat_and_open_volume_enthalpy_are_accounted() {
        let mut gas = GasState::new(0.001, 500.0, 0.001, Mixture::AIR).unwrap();
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
                    enthalpy_in_j: 0.00001 * Mixture::AIR.enthalpy(300.0),
                    mass_out_kg: 0.00002,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!((gas.mass_kg() - (old_mass - 0.00001)).abs() < 1e-15);
        assert!((l.enthalpy_out_j - 0.00002 * Mixture::AIR.enthalpy(old_t)).abs() < 1e-12);
        assert!(l.residual_j().abs() < 1e-10);
        assert!(l.numerical_correction_j.abs() < 1e-10);
    }

    #[test]
    fn bounded_steps_expose_numerical_energy_and_preserve_invalid_state() {
        let mut gas = GasState::new(0.001, 300.0, 0.001, Mixture::AIR).unwrap();
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
        let mut gas = GasState::at_pressure(g.volume(PI), 101325.0, 300.0, charge()).unwrap();
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
                let mut gas = GasState::at_pressure(g.volume(PI), 101325.0, t, charge()).unwrap();
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
