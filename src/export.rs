//! Standalone replacement mod: preserve every entry except referenced engine WAVs.
use crate::{
    bank::Bank,
    hybrid::{Hybrid, Settings},
    project::Parameters,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::Path,
    sync::Arc,
};

fn vehicle_info_path(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 3 && parts[0] == "vehicles" && !parts[1].is_empty() && parts[2] == "info.json"
}

fn quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut escaped = false;
    for (i, &byte) in bytes.iter().enumerate().skip(start + 1) {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some(i + 1);
        }
    }
    None
}

/// Change only the top-level display name, retaining the rest of Automation's
/// metadata byte for byte (including any duplicate paint keys).
pub fn label_vehicle_info(bytes: &[u8]) -> Result<(Vec<u8>, String), String> {
    if bytes.len() > 1_000_000 {
        return Err("Vehicle info.json exceeds 1 MB".into());
    }
    let info: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let original = info
        .get("Name")
        .and_then(serde_json::Value::as_str)
        .ok_or("Vehicle info.json has no string Name")?;
    let labelled = if original.ends_with(" (BESS)") {
        original.to_owned()
    } else {
        format!("{original} (BESS)")
    };
    let mut depth = 0usize;
    let mut position = 0;
    let mut range = None;
    while position < bytes.len() {
        match bytes[position] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'"' => {
                let end = quoted_end(bytes, position).ok_or("Unterminated JSON string")?;
                if depth == 1 {
                    let after_key = bytes[end..]
                        .iter()
                        .position(|b| !b.is_ascii_whitespace())
                        .map(|offset| end + offset);
                    if after_key.is_some_and(|i| bytes[i] == b':') {
                        let key: String = serde_json::from_slice(&bytes[position..end])
                            .map_err(|e| e.to_string())?;
                        if key == "Name" {
                            let value = after_key.unwrap() + 1;
                            let value = bytes[value..]
                                .iter()
                                .position(|b| !b.is_ascii_whitespace())
                                .map(|offset| value + offset)
                                .ok_or("Missing vehicle name value")?;
                            let value_end = quoted_end(bytes, value)
                                .ok_or("Vehicle Name is not a JSON string")?;
                            if range.replace(value..value_end).is_some() {
                                return Err("Vehicle info.json has multiple top-level Names".into());
                            }
                        }
                    }
                }
                position = end;
                continue;
            }
            _ => {}
        }
        position += 1;
    }
    let range = range.ok_or("Vehicle info.json has no top-level Name")?;
    let mut output = Vec::with_capacity(bytes.len() + 8);
    output.extend_from_slice(&bytes[..range.start]);
    output.extend(serde_json::to_vec(&labelled).map_err(|e| e.to_string())?);
    output.extend_from_slice(&bytes[range.end..]);
    let checked: serde_json::Value = serde_json::from_slice(&output).map_err(|e| e.to_string())?;
    if checked["Name"] != labelled {
        return Err("Vehicle display name validation failed".into());
    }
    Ok((output, labelled))
}

pub fn package_name(bank: &Bank) -> String {
    let stem = Path::new(&bank.source.archive)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let slug: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("bess-{}-{}.zip", slug, &bank.source.fingerprint[..8])
}

fn loop_samples(
    bank: Arc<Bank>,
    mut p: Parameters,
    mut h: Settings,
    rpm: f32,
    load: f32,
) -> Vec<f32> {
    p.rpm = rpm;
    p.load = load;
    p.volume = 0.8;
    h.enhanced = true;
    h.level_match = false;
    // These require runtime state: never bake a repeating lift-off or turbo transient.
    h.overrun = 0.;
    h.turbo = 0.;
    h.attack = 0.;
    h.roughness = 0.;
    h.fuel_cut = 0.;
    let mut engine = Hybrid::new(48000, p, h, Some(bank.clone()));
    for _ in 0..48000 {
        engine.next(true);
    }
    let cycle = 48000. * 120. / rpm;
    let frames = (cycle * (2. * 48000. / cycle).ceil()) as usize;
    let overlap = cycle.round().max(64.) as usize;
    let raw: Vec<f32> = (0..frames + overlap)
        .map(|_| engine.next(true) / (0.8 * bank.gain))
        .collect();
    let mut out = raw[overlap..].to_vec();
    for i in 0..overlap {
        let t = i as f32 / (overlap - 1) as f32;
        let w = 0.5 - 0.5 * (std::f32::consts::PI * t).cos();
        out[frames - overlap + i] = raw[frames + i] * (1. - w) + raw[i] * w;
    }
    out
}

pub fn package(dir: &Path, p: Parameters, h: Settings, bank: Arc<Bank>) -> Result<String, String> {
    p.validate()?;
    h.validate()?;
    // Verify the file has not been swapped since import before copying the vehicle.
    let fresh = Bank::load(Path::new(&bank.source.archive), Some(&bank.source.blend))?;
    if fresh.source.fingerprint != bank.source.fingerprint {
        return Err("Source archive changed since import".into());
    }
    drop(fresh);
    let mut zip =
        zip::ZipArchive::new(File::open(&bank.source.archive).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut names = std::collections::HashSet::new();
    let mut info_path = None;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        total = total.saturating_add(entry.size());
        if entry.enclosed_name().is_none()
            || entry.name().contains('\\')
            || !names.insert(entry.name().to_owned())
            || total > 2_000_000_000
        {
            return Err("Ambiguous archive, unsafe paths, or uncompressed size above 2 GB".into());
        }
        if vehicle_info_path(entry.name()) && info_path.replace(entry.name().to_owned()).is_some() {
            return Err("Archive has multiple vehicle info.json files".into());
        }
    }
    let info_path = info_path.ok_or("Archive has no vehicle info.json")?;
    let mut source_info = Vec::new();
    zip.by_name(&info_path)
        .map_err(|e| e.to_string())?
        .take(1_000_001)
        .read_to_end(&mut source_info)
        .map_err(|e| e.to_string())?;
    let (labelled_info, display_name) = label_vehicle_info(&source_info)?;
    let mut bytes = Vec::new();
    zip.by_name(&bank.source.blend)
        .map_err(|e| e.to_string())?
        .take(1_000_001)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let blend: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let mut replacements = BTreeMap::new();
    for (layer, rows) in blend["samples"]
        .as_array()
        .ok_or("Missing blend")?
        .iter()
        .enumerate()
    {
        for row in rows.as_array().ok_or("Missing load layer")? {
            let name = row[0].as_str().ok_or("Missing WAV name")?;
            let rpm = row[1].as_f64().ok_or("Missing RPM")? as f32;
            if replacements.contains_key(name) {
                return Err(
                    "A WAV shared by multiple RPM points cannot be replaced unambiguously".into(),
                );
            }
            replacements.insert(
                name.to_owned(),
                loop_samples(bank.clone(), p, h, rpm, layer as f32),
            );
        }
    }
    let peak = replacements
        .values()
        .flatten()
        .fold(0f32, |a, &b| a.max(b.abs()));
    if !peak.is_finite() || peak < 1e-8 || replacements.values().flatten().any(|v| !v.is_finite()) {
        return Err("Silent or non-finite rendering".into());
    }
    let gain = (0.95 / peak).min(1.);
    let zip_name = package_name(&bank);
    fs::create_dir(dir).map_err(|e| format!("Choose a new output folder: {e}"))?;
    let result = (|| -> Result<String, String> {
        let partial = dir.join(format!("{zip_name}.partial"));
        let mut output = zip::ZipWriter::new(File::create(&partial).map_err(|e| e.to_string())?);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut measurements = Vec::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(|e| e.to_string())?;
            if let Some(samples) = replacements.get(entry.name()) {
                let mut wav = Cursor::new(Vec::new());
                {
                    let mut writer = hound::WavWriter::new(
                        &mut wav,
                        hound::WavSpec {
                            channels: 1,
                            sample_rate: 48000,
                            bits_per_sample: 24,
                            sample_format: hound::SampleFormat::Int,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    for s in samples {
                        writer
                            .write_sample((s * gain * 8388607.) as i32)
                            .map_err(|e| e.to_string())?;
                    }
                    writer.finalize().map_err(|e| e.to_string())?;
                }
                output
                    .start_file(entry.name(), options)
                    .map_err(|e| e.to_string())?;
                output.write_all(wav.get_ref()).map_err(|e| e.to_string())?;
                measurements.push(json!({"path":entry.name(),"frames":samples.len(),"seam":(samples[0]-samples[samples.len()-1]).abs()*gain}));
            } else if entry.name() == info_path {
                output
                    .start_file(entry.name(), options)
                    .map_err(|e| e.to_string())?;
                output
                    .write_all(&labelled_info)
                    .map_err(|e| e.to_string())?;
            } else {
                output.raw_copy_file(entry).map_err(|e| e.to_string())?;
            }
        }
        output.finish().map_err(|e| e.to_string())?;
        // Reimport the real produced archive, not only an in-memory rendering.
        let check = Bank::load(&partial, Some(&bank.source.blend))?;
        if check.layers.iter().map(Vec::len).sum::<usize>() != replacements.len() {
            return Err("Incomplete loop coverage".into());
        }
        let mut check_zip = zip::ZipArchive::new(File::open(&partial).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut check_info = Vec::new();
        check_zip
            .by_name(&info_path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut check_info)
            .map_err(|e| e.to_string())?;
        if check_info != labelled_info {
            return Err("Exported vehicle display metadata differs from the prepared label".into());
        }
        fs::rename(&partial, dir.join(&zip_name)).map_err(|e| e.to_string())?;
        crate::project::save_project(
            &dir.join("settings.bess.json"),
            &crate::project::Project {
                version: 3,
                parameters: p,
                hybrid: h,
                source: Some(bank.source.clone()),
                driving: Default::default(),
            },
        )?;
        let report = json!({"version":env!("CARGO_PKG_VERSION"),"zip_file":zip_name,"display_name":display_name,"display_name_path":info_path,"source":bank.source,"settings":h,"parameters":p,"gain":gain,"loops":measurements,"runtime_events":"The original vehicle references for afterfire, turbo, startup, and shutdown are retained. BESS driving transients are not exported.","validation":"BESS reimported the generated archive; testing in BeamNG is still required"});
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::write(dir.join("INSTALLATION.txt"),format!("BESS — full vehicle with modified engine loops\n\nThe vehicle selector shows: {display_name}\n\n1. Keep a backup of the original Automation ZIP.\n2. Disable the original vehicle in BeamNG's mod manager.\n3. Install {zip_name} in the mods folder under your BeamNG user folder.\n4. Enable only this copy. Do not enable both versions at once.\n5. Reload the vehicle and compare idle, acceleration, lift-off, and camera views.\n6. To restore the original, disable the BESS copy and re-enable the original.\n\nEach BESS copy has a distinct ZIP name to avoid collisions between vehicles.\nThe off-load and on-load loops cover every RPM point in the original blend.\nEvents and physics remain those of the original vehicle. BESS transients are not exported as a BeamNG driving script.\nThe listening volume is not applied to the mod; one common safety gain preserves the relative dynamics.\nIn-game validation is still required.\n")).map_err(|e|e.to_string())?;
        Ok(format!("{} loops — {}", replacements.len(), dir.display()))
    })();
    if let Err(e) = &result {
        let _ = fs::write(dir.join("ERROR.txt"), e);
    }
    result
}
