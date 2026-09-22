//! Explicit four-stroke event timing, never inferred from audio harmonics.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Combustion {
    /// Zero means unknown: source-only hybrid fallback.
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
    /// Smooth, finite pressure windows, delayed to the exhaust opening.
    /// A phenomenological excitation, not a thermodynamic pressure solver.
    pub fn pressure(&self, cycle: f64, rpm: f32, load: f32) -> f32 {
        if self.cylinders == 0 {
            return 0.;
        }
        let width = (self.width_ms * 0.001 * rpm / 120.).min(0.8 / self.cylinders as f32);
        let mut sum = 0.;
        for &angle in &self.angles[..self.cylinders as usize] {
            let phase = (cycle - (angle + self.exhaust_delay) as f64 / 720.).rem_euclid(1.) as f32;
            if phase < width {
                let w = (std::f32::consts::PI * phase / width).sin();
                sum += w * w;
            }
        }
        sum * (0.2 + 0.8 * load) * 0.18
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_events_and_unknown_fallback() {
        let c = Combustion::even(4);
        assert!(c.validate().is_ok());
        let mut starts = 0;
        let mut was = false;
        for n in 0..7200 {
            let active = c.pressure(n as f64 / 7200., 1200., 1.) > 0.;
            if active && !was {
                starts += 1;
            }
            was = active;
            assert_eq!(
                Combustion::default().pressure(n as f64 / 7200., 1200., 1.),
                0.
            );
        }
        assert_eq!(starts, 4);
        let mut invalid = c;
        invalid.angles[0] = f32::NAN;
        assert!(invalid.validate().is_err());
        assert_eq!(
            serde_json::from_str::<Combustion>(&serde_json::to_string(&c).unwrap()).unwrap(),
            c
        );
    }
}
