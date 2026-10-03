//! Read-only discovery diagnostic: never modifies BeamNG or BESS preferences.
fn main() -> Result<(), String> {
    let folder = if let Some(path) = std::env::args_os().nth(1) {
        Some(bess::beamng_paths::DetectedFolder {
            mods_dir: bess::beamng_paths::normalize_selected_folder(std::path::Path::new(&path))?,
            description: "Explicit diagnostic folder".into(),
        })
    } else {
        bess::beamng_paths::detect()?
    };
    if let Some(folder) = folder {
        let vehicles = bess::beamng_library::scan_automation_archives(&folder.mods_dir)?;
        let export = bess::beamng_paths::export_directory(&folder.mods_dir)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "mods_directory": folder.mods_dir,
                "source": folder.description,
                "export_directory": export,
                "vehicles": vehicles.iter().map(|vehicle| serde_json::json!({
                    "path": vehicle.path,
                    "name": vehicle.name,
                    "unavailable_reason": vehicle.unavailable_reason,
                })).collect::<Vec<_>>()
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        println!("No BeamNG user folder detected.");
    }
    Ok(())
}
