//! Declared vehicle setup import audit. Reads ZIPs without extraction.
use bess::{drive::Controls, vehicle_setup};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

fn sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn main() -> Result<(), String> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "cars".into());
    let output = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| "output/final-readiness-20261003/vehicle-import/corpus.json".into());
    let mut archives = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|s| s == "zip"))
        .collect::<Vec<_>>();
    archives.sort();
    let mut results = Vec::new();
    for archive in archives {
        let before = sha256(&archive)?;
        let setup = vehicle_setup::inspect(&archive)?;
        let mut controls = Controls::default();
        setup.apply(&mut controls);
        controls.validate()?;
        let after = sha256(&archive)?;
        if before != after {
            return Err(format!(
                "Source changed during import: {}",
                archive.display()
            ));
        }
        if archive
            .file_name()
            .is_some_and(|s| s == "bunchyearth23_genesis_phantom.zip")
            && (setup.mass_kg != Some(1372.)
                || setup.clutch_torque_nm != Some(278.)
                || setup.gear_ratios != [2.17, 1.73, 1.34, 1.05, 0.85, 0.71, 0.60]
                || setup.final_drive != Some(5.62)
                || setup.wheel_radius != Some(0.365))
        {
            return Err(format!("Genesis declared setup regression: {setup:?}"));
        }
        results.push(json!({"source":archive,"sha256_before":before,"sha256_after":after,"source_unchanged":true,"setup":setup,"driving":controls}));
    }
    if results.is_empty() {
        return Err("No source archives found".into());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&output, serde_json::to_vec_pretty(&json!({"schema":1,"importer":"data-only JBeam subset, unique PC and rooted active slots, no expressions", "entries":results, "verification":{"driving_settings_valid":true,"gameplay_or_real_vehicle_calibration":false}})).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    println!("{}", output.display());
    Ok(())
}
