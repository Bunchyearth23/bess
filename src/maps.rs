//! Small, allocation-free timbre maps. Rows: load; columns: source RPM range.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Map(pub [[f32; 3]; 3]);
impl Default for Map {
    fn default() -> Self {
        Self([[1.; 3]; 3])
    }
}
impl Map {
    pub fn at(&self, rpm_fraction: f32, load: f32) -> f32 {
        let x = rpm_fraction.clamp(0., 1.) * 2.;
        let y = load.clamp(0., 1.) * 2.;
        let i = (x as usize).min(1);
        let j = (y as usize).min(1);
        let (u, v) = (x - i as f32, y - j as f32);
        let a = self.0[j][i] * (1. - u) + self.0[j][i + 1] * u;
        let b = self.0[j + 1][i] * (1. - u) + self.0[j + 1][i + 1] * u;
        a * (1. - v) + b * v
    }
    pub fn validate(&self) -> Result<(), String> {
        if self
            .0
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0.0..=2.0).contains(v))
        {
            Err("Cartographie : multiplicateurs finis entre 0 et 2 attendus".into())
        } else {
            Ok(())
        }
    }
    pub fn follow(&mut self, target: Self, amount: f32) {
        for (a, b) in self.0.iter_mut().flatten().zip(target.0.iter().flatten()) {
            *a += (*b - *a) * amount;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Maps {
    pub pulse: Map,
    pub texture: Map,
    pub intake: Map,
    pub exhaust: Map,
}
impl Maps {
    pub fn validate(&self) -> Result<(), String> {
        for m in [self.pulse, self.texture, self.intake, self.exhaust] {
            m.validate()?;
        }
        Ok(())
    }
    pub fn follow(&mut self, target: Self, amount: f32) {
        self.pulse.follow(target.pulse, amount);
        self.texture.follow(target.texture, amount);
        self.intake.follow(target.intake, amount);
        self.exhaust.follow(target.exhaust, amount);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bilinear_map_hits_nodes_and_interpolates_without_overshoot() {
        let m = Map([[0., 0.5, 1.], [0.5, 1., 1.5], [1., 1.5, 2.]]);
        for j in 0..3 {
            for i in 0..3 {
                assert_eq!(m.at(i as f32 / 2., j as f32 / 2.), m.0[j][i]);
            }
        }
        assert_eq!(m.at(0.25, 0.25), 0.5);
        assert_eq!(m.at(-1., 3.), 1.);
        for j in 0..100 {
            for i in 0..100 {
                assert!((0.0..=2.0).contains(&m.at(i as f32 / 99., j as f32 / 99.)));
            }
        }
    }
    #[test]
    fn legacy_maps_are_neutral_and_invalid_values_rejected() {
        let m: Maps = serde_json::from_str("{}").unwrap();
        assert_eq!(m, Maps::default());
        assert_eq!(m.pulse.at(0.37, 0.83), 1.);
        let mut bad = m;
        bad.texture.0[2][1] = f32::NAN;
        assert!(bad.validate().is_err());
        bad.texture.0[2][1] = 2.01;
        assert!(bad.validate().is_err());
    }
}
