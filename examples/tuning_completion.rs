//! Reloadable, repeatable independent-cam and ITB comparisons; no realism claim.
use bess::{
    drive::Controls,
    engine_build::Throttle,
    engine_definition::EngineDefinition,
    hybrid::Settings,
    project::{Parameters, Project, load_project, save_project},
    render::{scratch_samples, write_pcm},
    scratch::Scratch,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn main() -> Result<(), String> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "output/tuning-completion-20261003".into());
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let base = Scratch::default();
    let mut intake = base.clone();
    intake.tuning.cam.intake_duration_deg = Some(250.);
    intake.tuning.cam.intake_lift_mm = Some(12.);
    let mut exhaust = base.clone();
    exhaust.tuning.cam.exhaust_duration_deg = Some(250.);
    exhaust.tuning.cam.exhaust_lift_mm = Some(12.);
    exhaust.tuning.cam.exhaust_advance_deg = Some(5.);
    let mut itb = base.clone();
    itb.build.throttle = Throttle::Individual;
    let p = Parameters {
        cylinders: 4,
        rpm: 4000.,
        load: 0.7,
        volume: 0.65,
        ..Default::default()
    };
    let mut results = Vec::new();
    let mut reference = None;
    for (name, scratch) in [
        ("baseline", base),
        ("intake-only", intake),
        ("exhaust-only", exhaust),
        ("individual-intakes", itb),
    ] {
        let definition = EngineDefinition::from_scratch(&scratch);
        let settings = Settings {
            engine: Some(definition),
            physical_sound: scratch.sound,
            level_match: false,
            ..Default::default()
        };
        let project = output.join(format!("{name}.bess.json"));
        save_project(
            &project,
            &Project {
                version: 4,
                parameters: p,
                hybrid: settings,
                source: None,
                driving: Controls::default(),
                profile_name: name.into(),
                scratch: Some(scratch.clone()),
            },
        )?;
        let loaded = load_project(&project)?;
        if loaded.hybrid.engine != Some(definition) {
            return Err("Tuning project changed on reload".into());
        }
        let samples = scratch_samples(p, settings, &scratch, 3., Controls::default())?;
        if samples.iter().any(|v| !v.is_finite() || v.abs() > 1.) {
            return Err("Invalid audition samples".into());
        }
        let mean = samples.iter().map(|&v| f64::from(v)).sum::<f64>() / samples.len() as f64;
        let rms = (samples
            .iter()
            .map(|&v| (f64::from(v) - mean).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        if rms < 1e-7 {
            return Err("Silent audition".into());
        }
        let changed = reference.as_ref().is_some_and(|a| *a != samples);
        if name == "baseline" {
            reference = Some(samples.clone());
        } else if !changed {
            return Err(format!("{name} does not alter physical output"));
        }
        let wav = output.join(format!("{name}.wav"));
        write_pcm(&wav, &samples)?;
        let curve = bess::dyno::sweep(&scratch, 7)?;
        let resolved = scratch.tuning.resolve(&scratch.build, scratch.cylinders());
        results.push(json!({"name":name,"project":project,"project_roundtrip":true,"wav":wav,"sha256":format!("{:x}",Sha256::digest(fs::read(&wav).map_err(|e|e.to_string())?)),"frames":samples.len(),"rms_ac":rms,"changed_from_baseline":changed,"engine":definition,"cam":{"intake_duration_050_deg":resolved.duration_at_050_deg,"intake_lift_mm":resolved.lift_m*1000.,"intake_center_deg":resolved.intake_center_deg,"exhaust_duration_050_deg":resolved.exhaust_duration_at_050_deg,"exhaust_lift_mm":resolved.exhaust_lift_m*1000.,"exhaust_center_deg":resolved.exhaust_center_deg},"dyno":{"rpm":curve.rpm,"torque_nm":curve.torque_nm,"power_kw":curve.power_kw,"map_kpa":curve.map_kpa}}));
        println!("{name}: project reload, 3 s WAV and seven-point computed dyno complete");
    }
    fs::write(output.join("validation.json"), serde_json::to_vec_pretty(&json!({"results":results,"mix_parameters":p,"real_engine_calibration":false,"listening_accepted":false,"limits":"Computed physical output and equivalent ITB supply volume; no measured-engine fidelity claim. All comparisons retain identical user layer levels."})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    Ok(())
}
