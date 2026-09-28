//! Archived event settings retained only to read older projects.
//! Sound generation lives exclusively in the physical engine.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Combustion {
    /// Zero represented an unspecified cylinder count in older projects.
    pub cylinders: u32,
    /// Cylinder-indexed ignition angles in a 720-degree cycle.
    pub angles: [f32; 12],
    pub amount: f32,
    pub width_ms: f32,
    pub exhaust_delay: f32,
}
impl Default for Combustion {
    fn default() -> Self {
        Self {
            cylinders: 0,
            angles: [0.; 12],
            amount: 0.25,
            width_ms: 3.,
            exhaust_delay: 140.,
        }
    }
}
impl Combustion {
    pub fn even(cylinders: u32) -> Self {
        let mut s = Self {
            cylinders: cylinders.min(12),
            ..Self::default()
        };
        for i in 0..s.cylinders as usize {
            s.angles[i] = i as f32 * 720. / s.cylinders as f32;
        }
        s
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.cylinders > 12 {
            return Err("Cylinders: unknown or 1–12".into());
        }
        for (v, lo, hi) in [
            (self.amount, 0., 1.),
            (self.width_ms, 0.5, 8.),
            (self.exhaust_delay, 60., 240.),
        ] {
            if !v.is_finite() || !(lo..=hi).contains(&v) {
                return Err("Combustion setting out of range".into());
            }
        }
        for &v in &self.angles {
            if !v.is_finite() || !(0.0..720.0).contains(&v) {
                return Err("Firing angles must be between 0 and 720° (exclusive)".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archived_settings_validate_and_roundtrip() {
        let c = Combustion::even(4);
        assert!(c.validate().is_ok());
        assert_eq!(c.angles[..4], [0., 180., 360., 540.]);
        let old: Combustion = serde_json::from_str("{}").unwrap();
        assert_eq!(old, Combustion::default());
        old.validate().unwrap();
        let mut invalid = c;
        invalid.angles[0] = f32::NAN;
        assert!(invalid.validate().is_err());
        assert_eq!(
            serde_json::from_str::<Combustion>(&serde_json::to_string(&c).unwrap()).unwrap(),
            c
        );
    }
}
