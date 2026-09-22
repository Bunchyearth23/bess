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
    pub volume: f32,
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
            mechanical: 0.12,
            brightness: 2600.0,
            resonance: 1.2,
            pipe_length: 1.8,
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
            ("Volume", self.volume, 0., 0.8),
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
                mechanical: 0.25,
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub parameters: Parameters,
    #[serde(default)]
    pub hybrid: crate::hybrid::Settings,
    #[serde(default)]
    pub source: Option<crate::bank::SourceRef>,
    #[serde(default)]
    pub driving: crate::drive::Controls,
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
        },
    )
}
pub fn save_project(path: &Path, project: &Project) -> Result<(), String> {
    project.parameters.validate()?;
    project.hybrid.validate()?;
    project.driving.validate()?;
    let json = serde_json::to_string_pretty(project).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}
pub fn load(path: &Path) -> Result<Parameters, String> {
    Ok(load_project(path)?.parameters)
}
pub fn load_project(path: &Path) -> Result<Project, String> {
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let project: Project = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    if ![1, 2, 3].contains(&project.version) {
        return Err("Unsupported project version".into());
    }
    project.parameters.validate()?;
    project.hybrid.validate()?;
    project.driving.validate()?;
    Ok(project)
}
