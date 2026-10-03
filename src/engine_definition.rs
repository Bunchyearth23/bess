//! The playable physical engine, independent of its source or editing workflow.
//!
//! `Scratch` remains the compatibility format for old projects. Its retired
//! heap-owned event renderer is deliberately absent from realtime settings.
use crate::{
    engine_build::{EngineBuild, EngineTuning},
    scratch::{EngineDesign, ExperimentalSpec, Scratch, SoundTuning},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineDefinition {
    pub design: EngineDesign,
    pub build: EngineBuild,
    pub idle_rpm: f32,
    pub redline_rpm: f32,
    pub experimental: ExperimentalSpec,
    pub inertia: f32,
    pub sound: SoundTuning,
    #[serde(skip_serializing_if = "EngineTuning::is_derived")]
    pub tuning: EngineTuning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineSection {
    Architecture,
    Parts,
    Tuning,
    Operating,
    Combustion,
    Sound,
}

impl Default for EngineDefinition {
    fn default() -> Self {
        Self::from_scratch(&Scratch::default())
    }
}

impl EngineDefinition {
    pub fn from_scratch(scratch: &Scratch) -> Self {
        Self {
            design: scratch.design,
            build: scratch.build,
            idle_rpm: scratch.idle_rpm,
            redline_rpm: scratch.redline_rpm,
            experimental: scratch.experimental,
            inertia: scratch.inertia,
            sound: scratch.sound,
            tuning: scratch.tuning,
        }
    }

    /// Rebuild only the legacy event container required by the compatibility
    /// API. No active physical parameter is derived again or overwritten.
    pub fn to_scratch(&self) -> Scratch {
        let mut scratch = Scratch {
            design: self.design,
            build: self.build,
            idle_rpm: self.idle_rpm,
            redline_rpm: self.redline_rpm,
            experimental: self.experimental,
            inertia: self.inertia,
            sound: self.sound,
            tuning: self.tuning,
            ..Scratch::default()
        };
        // Malformed deserialized designs must report validation errors instead
        // of indexing their cylinder arrays while building compatibility data.
        if scratch.design.validate().is_ok() {
            scratch.apply_design();
        }
        scratch
    }

    pub fn validate(&self) -> Result<(), String> {
        self.design.validate()?;
        self.build.validate()?;
        if !self.idle_rpm.is_finite()
            || !self.redline_rpm.is_finite()
            || !(300.0..=2000.0).contains(&self.idle_rpm)
            || !(self.idle_rpm + 1000.0..=12_000.0).contains(&self.redline_rpm)
        {
            return Err(
                "Idle 300–2000 rpm; redline at least 1000 rpm above idle, up to 12000".into(),
            );
        }
        if !(1e-4..=100.).contains(&self.inertia) {
            return Err("Physical rotating inertia must be within 0.0001–100 kg·m²".into());
        }
        self.experimental.validate()?;
        self.sound.validate()?;
        self.tuning.validate()
    }

    /// Restore a section from the saved/imported reference without changing
    /// independently edited sections or the original recording.
    pub fn reset_section(&mut self, baseline: &Self, section: EngineSection) {
        match section {
            EngineSection::Architecture => self.design = baseline.design,
            EngineSection::Parts => self.build = baseline.build,
            EngineSection::Tuning => self.tuning = baseline.tuning,
            EngineSection::Operating => {
                self.idle_rpm = baseline.idle_rpm;
                self.redline_rpm = baseline.redline_rpm;
                self.inertia = baseline.inertia;
            }
            EngineSection::Combustion => self.experimental = baseline.experimental,
            EngineSection::Sound => self.sound = baseline.sound,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::engine::{Commands, Engine};

    #[test]
    fn unified_definition_preserves_physical_output_and_every_active_setting() {
        let mut scratch = Scratch {
            design: crate::scratch::PRESETS[8].1,
            ..Default::default()
        };
        scratch.apply_design();
        scratch.tuning.cam.lift_mm = Some(11.);
        scratch.experimental.afterfire = 0.4;
        scratch.experimental.knock = 0.2;
        scratch.sound.presence_db = 2.;
        scratch.inertia = 0.31;
        for legacy_cam in [false, true] {
            if legacy_cam {
                scratch.design.cam_revolutions = None;
            }
            let definition = EngineDefinition::from_scratch(&scratch);
            let restored = definition.to_scratch();
            assert_eq!(EngineDefinition::from_scratch(&restored), definition);
            let mut original = Engine::new(&scratch, 48000).unwrap();
            let mut common = Engine::new(&restored, 48000).unwrap();
            for _ in 0..4096 {
                let commands = Commands {
                    imposed_rpm: Some(2200.),
                    throttle: 0.6,
                    ..Default::default()
                };
                let a = original.next(commands);
                let b = common.next(commands);
                assert_eq!(a.exhaust.to_bits(), b.exhaust.to_bits());
                assert_eq!(a.intake.to_bits(), b.intake.to_bits());
                assert_eq!(a.mechanical.to_bits(), b.mechanical.to_bits());
                assert_eq!(a.rpm.to_bits(), b.rpm.to_bits());
                assert_eq!(a.torque_nm.to_bits(), b.torque_nm.to_bits());
            }
        }
    }

    #[test]
    fn unified_definition_reset_is_section_local_and_invalid_design_is_safe() {
        let baseline = EngineDefinition::default();
        let mut edited = baseline;
        edited.build.compression = 11.;
        edited.sound.bass_db = 3.;
        edited.tuning.cam.lift_mm = Some(12.);
        edited.reset_section(&baseline, EngineSection::Parts);
        assert_eq!(edited.build, baseline.build);
        assert_eq!(edited.sound.bass_db, 3.);
        assert_eq!(edited.tuning.cam.lift_mm, Some(12.));
        edited.design.cylinders = 99;
        assert!(edited.validate().is_err());
        let _ = edited.to_scratch();
        edited = baseline;
        edited.inertia = f32::NAN;
        assert!(edited.validate().is_err());
    }
}
