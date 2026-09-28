//! Retired engine configuration schema retained solely for project migration.
//! No synthesis, sample generation or audio runtime remains in this module.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cycle {
    FourStroke,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cylinder {
    pub name: String,
    /// Crank degrees in [0, 720); zero is the beginning of the cycle.
    pub firing_deg: f32,
    pub bank: usize,
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bank {
    pub exhaust_length_m: f32,
    pub intake_length_m: f32,
    pub sound_speed_m_s: f32,
    /// Signed reflection coefficients, magnitude at most 0.8.
    pub exhaust_reflection: f32,
    pub intake_reflection: f32,
    pub loss: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calibration {
    /// Sound controls, not dimensions measured on an engine.
    pub pulse_ms: f32,
    pub event_variation: f32,
    pub residual: f32,
    pub block_hz: f32,
    pub block_decay_ms: f32,
    pub exhaust_level: f32,
    pub intake_level: f32,
    pub block_level: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub cycle: Cycle,
    pub cylinders: Vec<Cylinder>,
    pub banks: Vec<Bank>,
    /// Opening offsets after the cylinder's firing reference, modulo 720 degrees.
    pub exhaust_open_after_deg: f32,
    pub intake_open_after_deg: f32,
    pub calibration: Calibration,
    pub seed: u64,
}

impl Config {
    pub fn even(cylinders: usize) -> Self {
        let count = cylinders.clamp(1, 12);
        Self {
            version: 1,
            cycle: Cycle::FourStroke,
            cylinders: (0..count)
                .map(|i| Cylinder {
                    name: format!("C{}", i + 1),
                    firing_deg: i as f32 * 720. / count as f32,
                    bank: 0,
                    strength: 1.,
                })
                .collect(),
            banks: vec![Bank {
                exhaust_length_m: 1.6,
                intake_length_m: 0.7,
                sound_speed_m_s: 420.,
                exhaust_reflection: -0.42,
                intake_reflection: 0.28,
                loss: 0.68,
            }],
            exhaust_open_after_deg: 140.,
            intake_open_after_deg: 500.,
            calibration: Calibration {
                pulse_ms: 2.2,
                event_variation: 0.08,
                residual: 0.,
                block_hz: 125.,
                block_decay_ms: 12.,
                exhaust_level: 0.7,
                intake_level: 0.3,
                block_level: 0.025,
            },
            seed: 0xB355,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported standalone configuration version".into());
        }
        if !(1..=12).contains(&self.cylinders.len()) || !(1..=4).contains(&self.banks.len()) {
            return Err("expected 1–12 cylinders and 1–4 banks".into());
        }
        for (name, value, lo, hi) in [
            ("exhaust opening", self.exhaust_open_after_deg, 0., 720.),
            ("intake opening", self.intake_open_after_deg, 0., 720.),
            ("pulse width", self.calibration.pulse_ms, 0.5, 5.),
            (
                "event variation",
                self.calibration.event_variation,
                0.,
                0.25,
            ),
            ("residual", self.calibration.residual, 0., 0.5),
            ("block resonance", self.calibration.block_hz, 50., 2_000.),
            ("block decay", self.calibration.block_decay_ms, 10., 300.),
            ("exhaust level", self.calibration.exhaust_level, 0., 1.),
            ("intake level", self.calibration.intake_level, 0., 1.),
            ("block level", self.calibration.block_level, 0., 1.),
        ] {
            if !value.is_finite() || value < lo || value > hi || (hi == 720. && value == hi) {
                return Err(format!("invalid {name}"));
            }
        }
        for cylinder in &self.cylinders {
            if !cylinder.firing_deg.is_finite()
                || !(0.0..720.0).contains(&cylinder.firing_deg)
                || cylinder.bank >= self.banks.len()
                || !cylinder.strength.is_finite()
                || !(0.1..=2.).contains(&cylinder.strength)
                || cylinder.name.trim().is_empty()
            {
                return Err("invalid cylinder phase, bank, strength or name".into());
            }
        }
        for bank in &self.banks {
            for value in [bank.exhaust_length_m, bank.intake_length_m] {
                if !value.is_finite() || !(0.1..=6.).contains(&value) {
                    return Err("duct length must be 0.1–6 m".into());
                }
            }
            if !bank.sound_speed_m_s.is_finite()
                || !(250.0..=700.0).contains(&bank.sound_speed_m_s)
                || !bank.loss.is_finite()
                || !(0.0..=0.85).contains(&bank.loss)
                || !bank.exhaust_reflection.is_finite()
                || bank.exhaust_reflection.abs() > 0.8
                || !bank.intake_reflection.is_finite()
                || bank.intake_reflection.abs() > 0.8
            {
                return Err("invalid duct speed, loss or reflection".into());
            }
        }
        Ok(())
    }
}
