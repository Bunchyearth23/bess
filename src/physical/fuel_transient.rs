//! Opt-in reduced admitted-charge liquid model. Constants are estimates, not
//! injector/port measurements. Liquid is retained in the cylinder's combined
//! inventory. Admission delivery lumps evaporation and port transport while
//! cylinder delivery permits evaporation with closed valves. No spatial film
//! or separate port-vapor reservoir is solved.
use super::thermo::{Mixture, ThermoError};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FuelDelivery {
    #[default]
    Admission,
    Cylinder,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FuelTransientConfig {
    /// Explicit delivery location, independent of fuel type and pulse timing.
    pub delivery: FuelDelivery,
    /// Start in crank degrees after firing TDC. Default is indirect admission;
    /// a direct-injection experiment must explicitly choose e.g. 550 degrees.
    pub injection_start_deg: f64,
    pub injection_duration_deg: f64,
    pub deposited_fraction: f64,
    pub film_time_constant_s: f64,
    pub droplet_time_constant_s: f64,
    pub evaporation_reference_k: f64,
    pub evaporation_activation_k: f64,
    pub latent_heat_j_kg: f64,
    pub entrainment_per_turnover: f64,
}

impl Default for FuelTransientConfig {
    fn default() -> Self {
        Self {
            delivery: FuelDelivery::Admission,
            injection_start_deg: 330.,
            injection_duration_deg: 120.,
            deposited_fraction: 0.35,
            film_time_constant_s: 0.25,
            droplet_time_constant_s: 0.006,
            evaporation_reference_k: 370.,
            evaporation_activation_k: 3000.,
            latent_heat_j_kg: 350_000.,
            entrainment_per_turnover: 0.1,
        }
    }
}

impl FuelTransientConfig {
    pub fn validate(self) -> Result<(), ThermoError> {
        if !(0.0..720.).contains(&self.injection_start_deg)
            || !(1.0..=180.).contains(&self.injection_duration_deg)
            || self.injection_start_deg + self.injection_duration_deg > 720.
            || !(0.0..=1.).contains(&self.deposited_fraction)
            || !(0.001..=10.).contains(&self.film_time_constant_s)
            || !(0.0001..=1.).contains(&self.droplet_time_constant_s)
            || !(200.0..=800.).contains(&self.evaporation_reference_k)
            || !(0.0..=10000.).contains(&self.evaporation_activation_k)
            || !(0.0..=1e6).contains(&self.latent_heat_j_kg)
            || !(0.0..=5.).contains(&self.entrainment_per_turnover)
        {
            return Err(ThermoError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FuelTransfer {
    pub injected_kg: f64,
    /// External liquid sensible enthalpy with the same reference as gas, minus
    /// latent heat. It may be negative; only vapor enthalpy enters GasState.
    pub injected_liquid_enthalpy_j: f64,
    pub evaporated_kg: f64,
    pub vapor_enthalpy_j: f64,
    pub latent_heat_j: f64,
    pub liquid_change_kg: f64,
    pub liquid_energy_change_j: f64,
    pub film_kg: f64,
    pub droplets_kg: f64,
}

impl FuelTransfer {
    pub fn mass_residual_kg(self) -> f64 {
        self.injected_kg - self.evaporated_kg - self.liquid_change_kg
    }
    pub fn energy_residual_j(self) -> f64 {
        self.injected_liquid_enthalpy_j + self.latent_heat_j
            - self.vapor_enthalpy_j
            - self.liquid_energy_change_j
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Pool {
    mass: f64,
    energy: f64,
}
impl Pool {
    fn take(&mut self, mass: f64) -> Self {
        let energy = if self.mass > 0. {
            self.energy * (mass / self.mass)
        } else {
            0.
        };
        self.mass -= mass;
        self.energy -= energy;
        Self { mass, energy }
    }
    fn add(&mut self, other: Self) {
        self.mass += other.mass;
        self.energy += other.energy;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FuelTransient {
    film: Pool,
    droplets: Pool,
    pulse_start: f64,
    pulse_mass: f64,
}

/// Each input is prepared from an already validated cylinder state.
pub struct FuelStep {
    pub local0: f64,
    pub local1: f64,
    pub dt_s: f64,
    pub cycle_dose_kg: f64,
    pub fuel_enabled: bool,
    pub injection_temperature_k: f64,
    pub wall_temperature_k: f64,
    pub gas_temperature_k: f64,
    pub intake_turnover: f64,
    /// Evaporation draws heat from the admitted gas; explicit energy budget.
    pub available_evaporation_heat_j: f64,
}

impl FuelTransient {
    /// Detect a pulse by crossing its cycle index. Comparing the reconstructed
    /// absolute angle to both endpoints can lose an event by one floating-point
    /// ulp at an otherwise exact start angle. Share this decision with metering.
    pub(crate) fn injection_start(c: FuelTransientConfig, local0: f64, local1: f64) -> Option<f64> {
        let cycle = 2. * TAU;
        let phase = c.injection_start_deg.to_radians();
        let before = ((local0 - phase) / cycle).floor();
        let after = ((local1 - phase) / cycle).floor();
        (after > before).then(|| (after * cycle + phase).clamp(local0, local1))
    }

    pub fn liquid_mass_kg(&self) -> f64 {
        self.film.mass + self.droplets.mass
    }

    pub fn liquid_energy_j(&self) -> f64 {
        self.film.energy + self.droplets.energy
    }

    pub fn film_mass_kg(&self) -> f64 {
        self.film.mass
    }

    pub fn droplet_mass_kg(&self) -> f64 {
        self.droplets.mass
    }

    pub fn step(&mut self, c: FuelTransientConfig, s: FuelStep) -> FuelTransfer {
        let old_mass = self.film.mass + self.droplets.mass;
        let old_energy = self.film.energy + self.droplets.energy;
        let width = c.injection_duration_deg.to_radians();
        if let Some(event) = Self::injection_start(c, s.local0, s.local1) {
            self.pulse_start = event;
            self.pulse_mass = s.cycle_dose_kg;
        }
        if !s.fuel_enabled {
            self.pulse_mass = 0.;
        }
        let overlap =
            (s.local1.min(self.pulse_start + width) - s.local0.max(self.pulse_start)).max(0.);
        let injected = self.pulse_mass * overlap / width;
        let liquid_h = Mixture::FUEL.enthalpy(s.injection_temperature_k) - c.latent_heat_j_kg;
        let deposited = injected * c.deposited_fraction;
        self.film.add(Pool {
            mass: deposited,
            energy: deposited * liquid_h,
        });
        self.droplets.add(Pool {
            mass: injected - deposited,
            energy: (injected - deposited) * liquid_h,
        });
        let entrained = self
            .film
            .take(self.film.mass * (-(-c.entrainment_per_turnover * s.intake_turnover).exp_m1()));
        self.droplets.add(entrained);
        let fraction = |t: f64, tau: f64| {
            let rate = (c.evaporation_activation_k * (1. / c.evaporation_reference_k - 1. / t))
                .clamp(-20., 20.)
                .exp()
                / tau;
            -(-s.dt_s * rate).exp_m1()
        };
        let film_requested =
            self.film.mass * fraction(s.wall_temperature_k, c.film_time_constant_s);
        let droplet_requested =
            self.droplets.mass * fraction(s.gas_temperature_k, c.droplet_time_constant_s);
        let requested = film_requested + droplet_requested;
        let scale = if requested * c.latent_heat_j_kg > s.available_evaporation_heat_j {
            s.available_evaporation_heat_j / (requested * c.latent_heat_j_kg)
        } else {
            1.
        };
        let evaporated_film = self.film.take(film_requested * scale);
        let evaporated_droplets = self.droplets.take(droplet_requested * scale);
        let evaporated = evaporated_film.mass + evaporated_droplets.mass;
        let latent = evaporated * c.latent_heat_j_kg;
        FuelTransfer {
            injected_kg: injected,
            injected_liquid_enthalpy_j: injected * liquid_h,
            evaporated_kg: evaporated,
            vapor_enthalpy_j: evaporated_film.energy + evaporated_droplets.energy + latent,
            latent_heat_j: latent,
            liquid_change_kg: self.film.mass + self.droplets.mass - old_mass,
            liquid_energy_change_j: self.film.energy + self.droplets.energy - old_energy,
            film_kg: self.film.mass,
            droplets_kg: self.droplets.mass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rate: u32, temperature: f64, cut: f64) -> [f64; 5] {
        let c = FuelTransientConfig::default();
        let mut fuel = FuelTransient::default();
        let dt = 1. / f64::from(rate);
        let (mut injected, mut vapor, mut tail, mut peak, mut liquid) = (0., 0., 0., 0_f64, 0.);
        for frame in 1..=rate {
            let t = f64::from(frame) * dt;
            let out = fuel.step(
                c,
                FuelStep {
                    local0: (t - dt) * 1500. * TAU / 60.,
                    local1: t * 1500. * TAU / 60.,
                    dt_s: dt,
                    cycle_dose_kg: 25e-6,
                    fuel_enabled: t < cut,
                    injection_temperature_k: 300.,
                    wall_temperature_k: temperature,
                    gas_temperature_k: temperature,
                    intake_turnover: dt * 20.,
                    available_evaporation_heat_j: 1.,
                },
            );
            injected += out.injected_kg;
            vapor += out.evaporated_kg;
            if t >= cut {
                tail += out.evaporated_kg;
                assert_eq!(out.injected_kg, 0.);
            }
            liquid = out.film_kg + out.droplets_kg;
            peak = peak.max(liquid);
            assert!(out.mass_residual_kg().abs() < 1e-18);
            assert!(out.energy_residual_j().abs() < 1e-12);
            assert!((injected - vapor - liquid).abs() < 1e-15);
            assert!(out.film_kg >= 0. && out.droplets_kg >= 0.);
            if out.injected_kg > 0. {
                let angle = ((t - dt * 0.5) * 1500. * TAU / 60.)
                    .rem_euclid(2. * TAU)
                    .to_degrees();
                assert!((329.8..=450.2).contains(&angle));
            }
        }
        [injected, vapor, tail, peak, liquid]
    }

    #[test]
    fn every_cycle_delivers_its_pulse_at_aligned_and_fractional_rpms() {
        let c = FuelTransientConfig::default();
        let dose = 25e-6;
        for rpm in [1500., 3000., 5500., 6900.] {
            for rate in [48000, 96000, 192000] {
                let mut model = FuelTransient::default();
                let frames = (64. * 120. / rpm * f64::from(rate)).ceil() as u32;
                let mut injected = 0.;
                for frame in 1..=frames {
                    let angle = |f: u32| f64::from(f) / f64::from(rate) * rpm * TAU / 60.;
                    let out = model.step(
                        c,
                        FuelStep {
                            local0: angle(frame - 1),
                            local1: angle(frame),
                            dt_s: 1. / f64::from(rate),
                            cycle_dose_kg: dose,
                            fuel_enabled: true,
                            injection_temperature_k: 310.,
                            wall_temperature_k: 450.,
                            gas_temperature_k: 600.,
                            intake_turnover: 0.,
                            available_evaporation_heat_j: 1.,
                        },
                    );
                    injected += out.injected_kg;
                    assert!(out.mass_residual_kg().abs() < 1e-18);
                }
                assert!(
                    (injected - 64. * dose).abs() < 1e-14,
                    "{rpm} rpm/{rate} Hz: delivered {injected}, expected {}",
                    64. * dose
                );
            }
        }
    }

    #[test]
    fn cold_start_retains_liquid_and_cut_leaves_a_conservative_evaporation_tail() {
        let cold = run(96000, 280., 0.6);
        let hot = run(96000, 450., 0.6);
        assert!(cold[3] > hot[3] * 2., "cold film must retain more fuel");
        assert!(
            cold[2] > 0. && hot[2] > 0.,
            "residual fuel evaporates after cut"
        );
        assert!(cold[4] > hot[4]);
        let low_rate = run(48000, 280., 0.6);
        for (at_96k, at_48k) in cold.iter().zip(low_rate) {
            assert!((at_96k - at_48k).abs() < 1e-7);
        }
    }

    #[test]
    fn latent_heat_budget_limits_evaporation_without_destroying_liquid() {
        let c = FuelTransientConfig {
            injection_start_deg: 0.,
            injection_duration_deg: 1.,
            ..Default::default()
        };
        let mut f = FuelTransient::default();
        let s = FuelStep {
            local0: -0.001,
            local1: 0.02,
            dt_s: 1. / 16000.,
            cycle_dose_kg: 1e-3,
            fuel_enabled: true,
            injection_temperature_k: 300.,
            wall_temperature_k: 800.,
            gas_temperature_k: 1200.,
            intake_turnover: 0.,
            available_evaporation_heat_j: 1e-4,
        };
        let out = f.step(c, s);
        assert!((out.latent_heat_j - 1e-4).abs() < 1e-15);
        assert!(out.film_kg + out.droplets_kg > 0.00099);
        assert!(out.energy_residual_j().abs() < 1e-10);
        assert!(out.mass_residual_kg().abs() < 1e-18);
    }

    #[test]
    fn invalid_injector_parameters_are_rejected() {
        for c in [
            FuelTransientConfig {
                injection_duration_deg: f64::NAN,
                ..Default::default()
            },
            FuelTransientConfig {
                deposited_fraction: 1.01,
                ..Default::default()
            },
            FuelTransientConfig {
                injection_start_deg: 700.,
                ..Default::default()
            },
            FuelTransientConfig {
                latent_heat_j_kg: -1.,
                ..Default::default()
            },
        ] {
            assert!(c.validate().is_err());
        }
        assert!(FuelTransientConfig::default().validate().is_ok());
    }
}
