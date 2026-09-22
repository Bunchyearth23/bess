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
    }
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
        let report = json!({"version":env!("CARGO_PKG_VERSION"),"zip_file":zip_name,"source":bank.source,"settings":h,"parameters":p,"gain":gain,"loops":measurements,"runtime_events":"The original vehicle references for afterfire, turbo, startup, and shutdown are retained. BESS driving transients are not exported.","validation":"BESS reimported the generated archive; testing in BeamNG is still required"});
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::write(dir.join("INSTALLATION.txt"),format!("BESS — full vehicle with modified engine loops\n\n1. Keep a backup of the original Automation ZIP.\n2. Disable the original vehicle in BeamNG's mod manager.\n3. Install {zip_name} in the mods folder under your BeamNG user folder.\n4. Enable only this copy. Do not enable both versions at once.\n5. Reload the vehicle and compare idle, acceleration, lift-off, and camera views.\n6. To restore the original, disable the BESS copy and re-enable the original.\n\nEach BESS copy has a distinct ZIP name to avoid collisions between vehicles.\nThe off-load and on-load loops cover every RPM point in the original blend.\nEvents and physics remain those of the original vehicle. BESS transients are not exported as a BeamNG driving script.\nThe listening volume is not applied to the mod; one common safety gain preserves the relative dynamics.\nIn-game validation is still required.\n")).map_err(|e|e.to_string())?;
        Ok(format!("{} loops — {}", replacements.len(), dir.display()))
    })();
    if let Err(e) = &result {
        let _ = fs::write(dir.join("ERROR.txt"), e);
    }
    result
}
