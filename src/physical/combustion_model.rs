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
    /// Reduced-model calibration anchor, not a measured engine fit. The default
    /// is a 60-degree Wiebe span at 1500 rpm on the historical 86 mm cylinder.
    pub reference_burn_duration_s: f64,
    pub reference_bore_m: f64,
    pub reference_piston_speed_m_s: f64,
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
            reference_burn_duration_s: 60. / (6. * 1500.),
            reference_bore_m: 0.086,
            reference_piston_speed_m_s: 4.3,
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
            || !(0.00005..=0.08).contains(&self.reference_burn_duration_s)
            || !(0.01..=1.).contains(&self.reference_bore_m)
            || !(0.1..=100.).contains(&self.reference_piston_speed_m_s)
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
        // A geometric traversal time is not a complete Wiebe heat-release span.
        // Anchor the latter explicitly, retaining state/geometry sensitivity.
        // Keep nominal speed independent of the user's flame-speed/turbulence
        // coefficients so editing those still changes actual burn timing.
        let reference_speed = 0.65 + 1.2 * 0.5 * self.reference_piston_speed_m_s;
        let speed_ratio = reference_speed / speed.max(0.05);
        let duration_s = (self.reference_burn_duration_s * s.bore_m / self.reference_bore_m
            * speed_ratio
            * variability)
            .clamp(0.00005, 0.08);
        let development_s = (self.reference_development_s
            * (self.reference_flame_speed_m_s / laminar.max(0.02)).sqrt()
            * speed_ratio
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
    fn explicit_reference_anchor_preserves_duration_and_legacy_json() {
        let c = PredictiveCombustionConfig::default();
        let p = c.predict(FlameState {
            lambda: 0.95,
            fresh_fraction: 1.,
            ..state()
        });
        assert!((p.duration_s - c.reference_burn_duration_s).abs() < 1e-12);
        assert!((p.development_s - c.reference_development_s).abs() < 1e-12);
        let mut old = serde_json::to_value(c).unwrap();
        for field in [
            "reference_burn_duration_s",
            "reference_bore_m",
            "reference_piston_speed_m_s",
        ] {
            old.as_object_mut().unwrap().remove(field);
        }
        assert_eq!(
            serde_json::from_value::<PredictiveCombustionConfig>(old).unwrap(),
            c
        );
        // At representative warm loads, avoid a >200-degree Wiebe span;
        // this does not forbid slow/cold/lean mixtures from burning poorly.
        for rpm in [850., 3000., 5500., 6900.] {
            let p = c.predict(FlameState {
                mean_piston_speed_m_s: 2. * 0.086 * rpm / 60.,
                ..state()
            });
            let span = p.duration_s * rpm * 6.;
            let ca50 = -35. + p.development_s * rpm * 6. + 0.516 * span;
            assert!((30.0..100.).contains(&span), "{rpm}: {span}");
            assert!((-10.0..40.).contains(&ca50), "{rpm}: {ca50}");
        }
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
