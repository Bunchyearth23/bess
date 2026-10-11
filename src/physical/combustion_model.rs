//! Opt-in reduced flame development/propagation correlation. Reference values
//! and exponents are explicit engineering estimates, not measured engine data.
//! It predicts timing and stability only; the cylinder still accounts actual
//! fuel/oxygen consumption and LHV. No auto-ignition or knock calibration claim.
use super::thermo::ThermoError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PredictiveCombustionConfig {
    pub reference_pressure_pa: f64,
    pub reference_temperature_k: f64,
    pub reference_flame_speed_m_s: f64,
    pub reference_development_s: f64,
    pub temperature_exponent: f64,
    pub pressure_exponent: f64,
    pub turbulence_gain: f64,
    pub dilution_penalty: f64,
}
impl Default for PredictiveCombustionConfig {
    fn default() -> Self {
        Self {
            reference_pressure_pa: 1e6,
            reference_temperature_k: 600.,
            reference_flame_speed_m_s: 0.65,
            reference_development_s: 0.0006,
            temperature_exponent: 1.7,
            pressure_exponent: -0.15,
            turbulence_gain: 1.2,
            dilution_penalty: 2.5,
        }
    }
}
impl PredictiveCombustionConfig {
    pub fn validate(self) -> Result<(), ThermoError> {
        if !(1e4..=1e7).contains(&self.reference_pressure_pa)
            || !(200.0..=1500.).contains(&self.reference_temperature_k)
            || !(0.05..=5.).contains(&self.reference_flame_speed_m_s)
            || !(0.00001..=0.01).contains(&self.reference_development_s)
            || !(0.0..=3.).contains(&self.temperature_exponent)
            || !(-1.0..=1.).contains(&self.pressure_exponent)
            || !(0.0..=5.).contains(&self.turbulence_gain)
            || !(0.0..=10.).contains(&self.dilution_penalty)
        {
            return Err(ThermoError::InvalidCombustion);
        }
        Ok(())
    }
    pub fn predict(self, s: FlameState) -> FlameTiming {
        let dilution = (1. - s.fresh_fraction).clamp(0., 1.);
        let mixture_speed = (-3. * (s.lambda - 0.95).powi(2)).exp();
        let laminar = self.reference_flame_speed_m_s
            * (s.temperature_k / self.reference_temperature_k).powf(self.temperature_exponent)
            * (s.pressure_pa.max(100.) / self.reference_pressure_pa).powf(self.pressure_exponent)
            * mixture_speed
            * (-self.dilution_penalty * dilution).exp();
        let estimated_turbulence = 0.5 * s.mean_piston_speed_m_s;
        let speed = laminar
            + self.turbulence_gain
                * estimated_turbulence
                * (laminar / self.reference_flame_speed_m_s).sqrt();
        let variability = (0.08 * s.variation * s.innovation * (1. + 2. * dilution)).exp();
        let duration_s = (0.7 * s.bore_m / speed.max(0.05) * variability).clamp(0.00005, 0.08);
        let development_s = (self.reference_development_s
            * (self.reference_flame_speed_m_s / laminar.max(0.02)).sqrt()
            * variability)
            .clamp(0.00001, 0.02);
        // Low-energy diluted/very lean mixtures are less stable. The random
        // draw remains cylinder-seeded and occurs at spark only.
        let stability = ((s.fresh_fraction - 0.12) / 0.4).clamp(0., 1.)
            * ((s.temperature_k - 200.) / 250.).clamp(0., 1.)
            * (1. - ((s.lambda - 1.).abs() / 0.7).powi(4)).clamp(0., 1.);
        FlameTiming {
            duration_s,
            development_s,
            laminar_speed_m_s: laminar,
            turbulent_speed_m_s: speed,
            ignition_probability: stability,
        }
    }
}
pub struct FlameState {
    pub pressure_pa: f64,
    pub temperature_k: f64,
    pub lambda: f64,
    pub fresh_fraction: f64,
    pub mean_piston_speed_m_s: f64,
    pub bore_m: f64,
    pub variation: f64,
    pub innovation: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct FlameTiming {
    pub duration_s: f64,
    pub development_s: f64,
    pub laminar_speed_m_s: f64,
    pub turbulent_speed_m_s: f64,
    pub ignition_probability: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> FlameState {
        FlameState {
            pressure_pa: 1e6,
            temperature_k: 600.,
            lambda: 1.,
            fresh_fraction: 0.95,
            mean_piston_speed_m_s: 4.3,
            bore_m: 0.086,
            variation: 0.,
            innovation: 0.,
        }
    }

    #[test]
    fn hotter_charge_and_turbulence_accelerate_while_dilution_delays_flame() {
        let c = PredictiveCombustionConfig::default();
        let base = c.predict(state());
        let hot = c.predict(FlameState {
            temperature_k: 800.,
            ..state()
        });
        let turbulent = c.predict(FlameState {
            mean_piston_speed_m_s: 8.,
            ..state()
        });
        let diluted = c.predict(FlameState {
            fresh_fraction: 0.4,
            ..state()
        });
        let high_pressure = c.predict(FlameState {
            pressure_pa: 2e6,
            ..state()
        });
        let lean = c.predict(FlameState {
            lambda: 1.5,
            ..state()
        });
        assert!(hot.duration_s < base.duration_s && hot.development_s < base.development_s);
        assert!(turbulent.duration_s < base.duration_s);
        assert!(
            diluted.duration_s > base.duration_s
                && diluted.ignition_probability < base.ignition_probability
        );
        assert!(high_pressure.laminar_speed_m_s < base.laminar_speed_m_s);
        assert!(lean.duration_s > base.duration_s);
    }

    #[test]
    fn config_bounds_and_variation_envelope_are_explicit() {
        let c = PredictiveCombustionConfig::default();
        assert!(c.validate().is_ok());
        assert!(
            PredictiveCombustionConfig {
                temperature_exponent: f64::NAN,
                ..c
            }
            .validate()
            .is_err()
        );
        for temperature_k in [200., 3500.] {
            for pressure_pa in [1., 1e8] {
                let p = c.predict(FlameState {
                    temperature_k,
                    pressure_pa,
                    ..state()
                });
                assert!(p.duration_s.is_finite() && p.development_s > 0.);
                assert!((0.0..=1.).contains(&p.ignition_probability));
            }
        }
        assert!(
            c.predict(FlameState {
                variation: 1.,
                innovation: 1.,
                ..state()
            })
            .duration_s
                > c.predict(FlameState {
                    variation: 1.,
                    innovation: -1.,
                    ..state()
                })
                .duration_s
        );
    }
}
