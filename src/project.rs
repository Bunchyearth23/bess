use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Parameters {
    pub cylinders: u32,
    pub rpm: f32,
    pub load: f32,
    pub uneven: f32,
    pub intake: f32,
    pub exhaust: f32,
    pub mechanical: f32,
    pub brightness: f32,
    pub resonance: f32,
    pub pipe_length: f32,
    /// Final BESS engine level, independent of listening volume and original A.
    #[serde(default = "default_master_gain")]
    pub master_gain: f32,
    pub volume: f32,
}
fn default_master_gain() -> f32 {
    1.
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            cylinders: 4,
            rpm: 900.0,
            load: 0.35,
            uneven: 0.0,
            intake: 0.3,
            exhaust: 0.8,
            mechanical: 0.0,
            brightness: 2600.0,
            resonance: 1.2,
            pipe_length: 1.8,
            master_gain: default_master_gain(),
            volume: 0.35,
        }
    }
}
impl Parameters {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=12).contains(&self.cylinders) {
            return Err("Cylinders: 1 to 12".into());
        }
        for (name, v, min, max) in [
            // Same accepted envelope as imported blend RPM metadata; the bank
            // supplies the tighter limits for actual hybrid playback.
            ("RPM", self.rpm, 200., 20000.),
            ("Load", self.load, 0., 1.),
            ("Uneven firing", self.uneven, 0., 0.8),
            ("Intake", self.intake, 0., 1.),
            ("Exhaust", self.exhaust, 0., 1.),
            ("Mechanical", self.mechanical, 0., 1.),
            ("Brightness", self.brightness, 200., 10000.),
            ("Resonance", self.resonance, 0.5, 4.),
            ("Length", self.pipe_length, 0.2, 5.),
            ("BESS master gain", self.master_gain, 0., 1.),
            ("Volume", self.volume, 0., 1.),
        ] {
            if !v.is_finite() || v < min || v > max {
                return Err(format!("{name} out of range"));
            }
        }
        Ok(())
    }
    pub fn preset(index: usize) -> Self {
        match index {
            1 => Self {
                cylinders: 6,
                rpm: 850.,
                brightness: 3200.,
                ..Self::default()
            },
            2 => Self {
                cylinders: 8,
                rpm: 750.,
                uneven: 0.55,
                brightness: 1600.,
                pipe_length: 2.8,
                ..Self::default()
            },
            3 => Self {
                cylinders: 2,
                rpm: 1100.,
                uneven: 0.65,
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub parameters: Parameters,
    #[serde(default)]
    pub hybrid: crate::hybrid::Settings,
    #[serde(default)]
    pub source: Option<crate::bank::SourceRef>,
    #[serde(default)]
    pub driving: crate::drive::Controls,
    #[serde(default = "default_profile_name")]
    pub profile_name: String,
    /// Legacy scratch mirror; `hybrid.engine` is canonical in version 4.
    /// Imported engines keep their original `source` and no scratch mirror.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch: Option<crate::scratch::Scratch>,
}
pub fn default_profile_name() -> String {
    "Natural".to_owned()
}
pub fn validate_profile_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 48
        || name
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '/' | '\\'))
    {
        return Err("Profile name must be 1–48 characters without slashes or controls".into());
    }
    Ok(())
}
pub fn save(path: &Path, parameters: Parameters) -> Result<(), String> {
    save_project(
        path,
        &Project {
            version: 2,
            parameters,
            hybrid: Default::default(),
            source: None,
            driving: Default::default(),
            profile_name: default_profile_name(),
            scratch: None,
        },
    )
}
pub fn save_project(path: &Path, project: &Project) -> Result<(), String> {
    let mut project = project.clone();
    project.normalize()?;
    project.validate()?;
    let json = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

impl Project {
    fn validate(&self) -> Result<(), String> {
        let project = self;
        project.parameters.validate()?;
        project.hybrid.validate()?;
        project.driving.validate()?;
        validate_profile_name(&project.profile_name)?;
        if let Some(scratch) = &project.scratch {
            scratch.validate()?;
        }
        Ok(())
    }

    /// Upgrade on load/save without opening a source archive. Imported legacy
    /// projects retain physical_sound until the verified bank can be resolved.
    fn normalize(&mut self) -> Result<(), String> {
        if ![1, 2, 3, 4].contains(&self.version) {
            return Err("Unsupported project version".into());
        }
        if self.source.is_some() && self.scratch.is_some() {
            return Err(
                "A project cannot have both an Automation source and a scratch origin".into(),
            );
        }
        if self.source.is_none() {
            self.hybrid.engine_baseline = None;
        }
        if (self.version < 4 || self.hybrid.engine.is_none())
            && let Some(scratch) = &self.scratch
        {
            scratch.validate()?;
            self.hybrid.engine = Some(crate::engine_definition::EngineDefinition::from_scratch(
                scratch,
            ));
        }
        if let Some(engine) = self.hybrid.engine {
            engine.validate()?;
            self.hybrid.physical_sound = engine.sound;
            if self.source.is_none() {
                // Preserve the historical scratch container when already in
                // sync; its dormant legacy details need not be rewritten.
                if self.scratch.as_ref().is_none_or(|scratch| {
                    crate::engine_definition::EngineDefinition::from_scratch(scratch) != engine
                }) {
                    self.scratch = Some(engine.to_scratch());
                }
            }
        }
        self.version = 4;
        Ok(())
    }
}
pub fn load(path: &Path) -> Result<Parameters, String> {
    Ok(load_project(path)?.parameters)
}
pub fn load_project(path: &Path) -> Result<Project, String> {
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut project: Project = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    project.normalize()?;
    project.validate()?;
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bank::SourceRef, engine_definition::EngineDefinition, scratch::Scratch};

    #[test]
    fn master_gain_defaults_for_legacy_projects_and_persists_valid_limits() {
        for version in 1..=4 {
            let mut legacy = serde_json::to_value(project(version)).unwrap();
            legacy["parameters"]
                .as_object_mut()
                .unwrap()
                .remove("master_gain");
            let mut loaded: Project = serde_json::from_value(legacy).unwrap();
            loaded.normalize().unwrap();
            loaded.validate().unwrap();
            assert_eq!(loaded.parameters.master_gain, 1.);
        }
        for gain in [0., 0.5, 1.] {
            let mut saved = project(4);
            saved.parameters.master_gain = gain;
            let mut loaded: Project =
                serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
            loaded.normalize().unwrap();
            loaded.validate().unwrap();
            assert_eq!(loaded.parameters.master_gain, gain);
            assert_eq!(loaded.parameters.volume, saved.parameters.volume);
        }
        for gain in [-0.01, 1.01, f32::NAN, f32::INFINITY] {
            assert!(
                Parameters {
                    master_gain: gain,
                    ..Parameters::default()
                }
                .validate()
                .unwrap_err()
                .contains("master gain")
            );
        }
    }

    fn project(version: u32) -> Project {
        Project {
            version,
            parameters: Parameters::default(),
            hybrid: Default::default(),
            source: None,
            driving: Default::default(),
            profile_name: default_profile_name(),
            scratch: None,
        }
    }

    fn source() -> SourceRef {
        SourceRef {
            archive: "original.zip".into(),
            blend: "original.sfxBlend2D.json".into(),
            fingerprint: "original-audio".into(),
            engine_fingerprint: Some("original-engine".into()),
        }
    }

    #[test]
    fn unified_project_migration_keeps_scratch_and_legacy_import_sound() {
        for version in 1..=3 {
            let mut old = project(version);
            let mut scratch = Scratch::default();
            scratch.sound.bass_db = 2.;
            scratch.tuning.cam.lift_mm = Some(10.5);
            old.scratch = Some(scratch.clone());
            old.normalize().unwrap();
            old.validate().unwrap();
            assert_eq!(old.version, 4);
            assert_eq!(
                old.hybrid.engine,
                Some(EngineDefinition::from_scratch(&scratch))
            );
            assert_eq!(old.scratch, Some(scratch));

            let mut imported = project(version);
            imported.source = Some(source());
            imported.hybrid.physical_sound.presence_db = -3.;
            imported.normalize().unwrap();
            assert!(imported.hybrid.engine.is_none());
            assert!(imported.scratch.is_none());
            assert_eq!(imported.hybrid.physical_sound.presence_db, -3.);
            assert_eq!(imported.source.unwrap().fingerprint, "original-audio");
        }
    }

    #[test]
    fn unified_project_v4_definition_is_canonical_and_origin_is_unambiguous() {
        let mut current = project(4);
        let mut engine = EngineDefinition::default();
        engine.sound.exhaust_bass_db = 4.;
        engine.tuning.turbo.size = Some(1.2);
        current.hybrid.engine = Some(engine);
        current.hybrid.engine_baseline = Some(EngineDefinition::default());
        current.scratch = Some(Scratch::default());
        current.normalize().unwrap();
        assert!(current.hybrid.engine_baseline.is_none());
        assert_eq!(
            EngineDefinition::from_scratch(current.scratch.as_ref().unwrap()),
            engine
        );
        assert_eq!(current.hybrid.physical_sound, engine.sound);
        current.source = Some(source());
        assert!(current.normalize().unwrap_err().contains("both"));
        current.scratch = None;
        let baseline = EngineDefinition::default();
        current.hybrid.engine_baseline = Some(baseline);
        current.normalize().unwrap();
        let json = serde_json::to_string(&current).unwrap();
        let mut reloaded: Project = serde_json::from_str(&json).unwrap();
        reloaded.normalize().unwrap();
        reloaded.validate().unwrap();
        assert!(reloaded.scratch.is_none());
        assert_eq!(reloaded.hybrid.engine, Some(engine));
        assert_eq!(reloaded.hybrid.engine_baseline, Some(baseline));
        assert_eq!(
            reloaded.source.unwrap().engine_fingerprint.as_deref(),
            Some("original-engine")
        );
    }
}
