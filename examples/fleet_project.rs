//! Prepare a source-bound fleet project without changing engine hardware.
//! Usage: fleet_project SOURCE PROFILE_JSON NEW_OUTPUT
use bess::{
    automation_model::AutomationModel,
    bank::Bank,
    engine_definition::EngineDefinition,
    hybrid::Settings,
    project::{self, Parameters, Project},
    vehicle_setup,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 3 {
        return Err("SOURCE PROFILE_JSON NEW_OUTPUT required".into());
    }
    let source = fs::canonicalize(&args[0]).map_err(|e| e.to_string())?;
    let profile: Value = serde_json::from_slice(&fs::read(&args[1]).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let bank = Bank::load(&source, None)?;
    let model = AutomationModel::from_bank(&bank)?;
    let mut definition = EngineDefinition::from_scratch(&model.scratch);
    let mut sound = serde_json::to_value(definition.sound).map_err(|e| e.to_string())?;
    for (key, value) in profile["sound"]
        .as_object()
        .ok_or("sound object required")?
    {
        if sound.get(key).is_none() {
            return Err(format!("Unknown sound control {key}"));
        }
        sound[key] = value.clone();
    }
    definition.sound = serde_json::from_value(sound).map_err(|e| e.to_string())?;
    definition.validate()?;
    let hybrid = Settings {
        engine: Some(definition),
        engine_baseline: Some(model.baseline),
        physical_sound: definition.sound,
        engine_gain: profile["engine_gain"]
            .as_f64()
            .ok_or("engine_gain required")? as f32,
        ..Settings::calibrated(&bank)
    };
    let parameters = Parameters {
        cylinders: definition.design.cylinders,
        rpm: definition.idle_rpm,
        load: 0.12,
        exhaust: 1.,
        intake: 0.25,
        mechanical: 0.1,
        brightness: 10000.,
        master_gain: profile["master_gain"]
            .as_f64()
            .ok_or("master_gain required")? as f32,
        ..Parameters::default()
    };
    let setup = vehicle_setup::inspect(&source)?;
    let mut driving = Default::default();
    setup.apply(&mut driving);
    let project = Project {
        version: 4,
        parameters,
        hybrid,
        source: Some(bank.source.clone()),
        driving,
        profile_name: profile["name"].as_str().ok_or("name required")?.into(),
        scratch: None,
    };
    fs::create_dir(&args[2]).map_err(|e| e.to_string())?;
    project::save_project(&args[2].join("project.bess.json"), &project)?;
    fs::write(
        args[2].join("metadata.json"),
        serde_json::to_vec_pretty(&json!({
            "source": source, "engine": bank.engine_meta, "profile": profile,
            "baseline": model.baseline, "active": definition, "setup": setup,
            "assumptions": model.assumptions, "provenance": model.provenance,
            "boundary": "Source metadata plus BESS estimates; sound matching requires listening"
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("{}", args[2].display());
    Ok(())
}
