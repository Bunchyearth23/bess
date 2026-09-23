//! Standalone replacement mod: preserve every entry except referenced engine WAVs.
use crate::{
    bank::{self, Bank},
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

const MAX_SOURCE_WAV_BYTES: u64 = 16_000_000;

fn source_wav(zip: &mut zip::ZipArchive<File>, name: &str) -> Result<Vec<f32>, String> {
    let entry = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    if entry.size() > MAX_SOURCE_WAV_BYTES {
        return Err(format!("Source WAV is too large: {name}"));
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_SOURCE_WAV_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SOURCE_WAV_BYTES {
        return Err(format!("Source WAV is too large: {name}"));
    }
    bank::decode_wav(&bytes).map(|(_, mono)| mono)
}

fn signal_stats(samples: &[f32]) -> Result<(f32, f32), String> {
    if samples.is_empty() {
        return Err("Silent source or rendered exhaust WAV".into());
    }
    // Automation WAVs can contain a large DC offset. Calibrate audible AC
    // energy, while retaining the absolute sample peak for PCM headroom.
    let mut mean = 0f64;
    let mut centered_power = 0f64;
    let mut peak = 0f32;
    for (index, &sample) in samples.iter().enumerate() {
        if !sample.is_finite() {
            return Err("Non-finite source or rendered exhaust WAV".into());
        }
        let delta = sample as f64 - mean;
        mean += delta / (index + 1) as f64;
        centered_power += delta * (sample as f64 - mean);
        peak = peak.max(sample.abs());
    }
    let rms = (centered_power / samples.len() as f64).sqrt() as f32;
    if !rms.is_finite() || rms < 1e-7 || peak < 1e-7 {
        return Err("Silent source or rendered exhaust WAV".into());
    }
    Ok((rms, peak))
}

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

fn aligned_warmup_frames(rpm: f32) -> usize {
    let warmup_cycles = (rpm as f64 / 120.).ceil();
    (warmup_cycles * 120. * 48000. / rpm as f64).round() as usize
}

pub(crate) fn exhaust_safety_gain(peak: f32) -> f32 {
    (0.95 / peak).min(1.)
}

/// Per-knot level calibration for the selectable two-emitter export only.
/// RMS inputs are AC levels measured after subtracting each signal's mean.
/// Inputs must first pass `signal_stats`. Absolute peak safety takes precedence over
/// the 0.5 gain floor when both limits cannot be satisfied together.
pub(crate) fn exhaust_level_gain(source_rms: f32, exhaust_rms: f32, exhaust_peak: f32) -> f32 {
    const MINUS_ONE_DB: f32 = 0.891_250_9;
    (MINUS_ONE_DB * source_rms / exhaust_rms)
        .clamp(0.5, 2.5)
        .min(0.94 / exhaust_peak)
}

pub(crate) fn loop_stems(
    bank: Arc<Bank>,
    p: Parameters,
    h: Settings,
    rpm: f32,
    load: f32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    render_stems(bank, p, h, rpm, load, 4.)
}

fn render_stems(
    bank: Arc<Bank>,
    mut p: Parameters,
    mut h: Settings,
    rpm: f32,
    load: f32,
    engine_seconds: f32,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
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
    // Start every RPM knot at the same 720-degree phase. A fixed one-second
    // preroll ends at a different crank angle for every RPM and makes adjacent
    // BeamNG samples cancel as they crossfade.
    for _ in 0..aligned_warmup_frames(rpm) {
        engine.next(true);
    }
    let cycle = 48000. * 120. / rpm;
    let frames = (cycle * (2. * 48000. / cycle).ceil()) as usize;
    // The engine-side mechanical variation needs longer before it repeats.
    // Its emitter has independent WAVs; keep the exhaust bank compact.
    let engine_frames = (cycle * (engine_seconds * 48000. / cycle).ceil()) as usize;
    let overlap = cycle.round().max(64.) as usize;
    let stems: Vec<_> = (0..engine_frames + overlap)
        .map(|_| engine.next_stems(true))
        .collect();
    let join = |frames: usize, sample: fn(&crate::hybrid::HybridStems) -> f32| {
        let raw: Vec<f32> = stems
            .iter()
            .map(|stem| sample(stem) / (0.8 * bank.gain))
            .collect();
        let mut out = raw[overlap..overlap + frames].to_vec();
        for i in 0..overlap {
            let t = i as f32 / (overlap - 1) as f32;
            let w = 0.5 - 0.5 * (std::f32::consts::PI * t).cos();
            out[frames - overlap + i] = raw[frames + i] * (1. - w) + raw[i] * w;
        }
        out
    };
    (
        join(frames, |s| s.exhaust),
        join(engine_frames, |s| s.engine),
        join(frames, |s| s.mixed),
    )
}

/// Keep the previous full-replacement sound as a single mixed exhaust bank.
pub fn package(dir: &Path, p: Parameters, h: Settings, bank: Arc<Bank>) -> Result<String, String> {
    package_inner(dir, p, h, bank, false)
}

/// Produce the exhaust stem used by selectable variants with a second emitter.
pub fn package_exhaust_stem(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
) -> Result<String, String> {
    package_inner(dir, p, h, bank, true)
}

fn package_inner(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    exhaust_only: bool,
) -> Result<String, String> {
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
    let mut exhaust_level_gains = BTreeMap::new();
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
            // Replacement and intermediate exhaust rendering use only their
            // two-second channels; variant conversion renders the four-second
            // engine stem separately with `loop_stems`.
            let stems = render_stems(bank.clone(), p, h, rpm, layer as f32, 2.);
            let mut rendered = if exhaust_only { stems.0 } else { stems.2 };
            if exhaust_only {
                let source = source_wav(&mut zip, name)?;
                let (source_rms, _) = signal_stats(&source)?;
                let (rendered_rms, rendered_peak) = signal_stats(&rendered)?;
                let level_gain = exhaust_level_gain(source_rms, rendered_rms, rendered_peak);
                if !level_gain.is_finite() || level_gain <= 0. {
                    return Err(format!("Invalid exhaust level gain: {name}"));
                }
                for sample in &mut rendered {
                    *sample *= level_gain;
                }
                exhaust_level_gains.insert(name.to_owned(), level_gain);
            }
            replacements.insert(name.to_owned(), rendered);
        }
    }
    let peak = replacements
        .values()
        .flatten()
        .fold(0f32, |a, &b| a.max(b.abs()));
    if !peak.is_finite() || peak < 1e-8 || replacements.values().flatten().any(|v| !v.is_finite()) {
        return Err("Silent or non-finite rendering".into());
    }
    let gain = exhaust_safety_gain(peak);
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
                let mut measurement = json!({"path":entry.name(),"frames":samples.len(),"seam":(samples[0]-samples[samples.len()-1]).abs()*gain});
                if let Some(level_gain) = exhaust_level_gains.get(entry.name()) {
                    measurement["exhaust_level_gain"] = json!(level_gain);
                }
                measurements.push(measurement);
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
        let report = json!({"version":env!("CARGO_PKG_VERSION"),"zip_file":zip_name,"display_name":display_name,"display_name_path":info_path,"source":bank.source,"settings":h,"parameters":p,"gain":gain,"loops":measurements,"render_channel":if exhaust_only {"exhaust"} else {"mixed"},"runtime_events":"The original vehicle references for afterfire, turbo, startup, and shutdown are retained. BESS driving transients are not exported.","validation":"BESS reimported the generated archive; testing in BeamNG is still required"});
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let instructions = if exhaust_only {
            "BESS intermediate exhaust-stem render\n\nDo not install this ZIP directly. It contains only the exhaust half of the selectable BESS sound and is an input to variant conversion. Install the final bess-variant-*.zip beside the original Automation vehicle instead.\n".to_owned()
        } else {
            format!(
                "BESS — full vehicle with modified engine loops\n\nThe vehicle selector shows: {display_name}\n\n1. Keep a backup of the original Automation ZIP.\n2. Disable the original vehicle in BeamNG's mod manager.\n3. Install {zip_name} in the mods folder under your BeamNG user folder.\n4. Enable only this copy. Do not enable both versions at once.\n5. Reload the vehicle and compare idle, acceleration, lift-off, and camera views.\n6. To restore the original, disable the BESS copy and re-enable the original.\n\nEach BESS copy has a distinct ZIP name to avoid collisions between vehicles.\nThe off-load and on-load loops cover every RPM point in the original blend.\nEvents and physics remain those of the original vehicle. BESS transients are not exported as a BeamNG driving script.\nThe listening volume is not applied to the mod; one common safety gain preserves the relative dynamics.\nIn-game validation is still required.\n"
            )
        };
        fs::write(dir.join("INSTALLATION.txt"), instructions).map_err(|e| e.to_string())?;
        Ok(format!("{} loops — {}", replacements.len(), dir.display()))
    })();
    if let Err(e) = &result {
        let _ = fs::write(dir.join("ERROR.txt"), e);
    }
    result
}

#[cfg(test)]
mod phase_tests {
    use super::aligned_warmup_frames;

    #[test]
    fn export_preroll_lands_at_a_shared_crank_phase() {
        for rpm in [803., 2215., 4989., 5338., 5712., 10_000.] {
            let frames = aligned_warmup_frames(rpm);
            assert!(frames >= 48_000);
            let cycles = frames as f64 * rpm as f64 / (48_000. * 120.);
            let sample_tolerance = rpm as f64 / (2. * 48_000. * 120.);
            assert!((cycles - cycles.round()).abs() <= sample_tolerance + 1e-9);
        }
    }
}

#[cfg(test)]
mod level_tests {
    use super::{exhaust_level_gain, signal_stats};

    #[test]
    fn selectable_exhaust_gain_targets_one_db_below_source_with_bounds() {
        let target = exhaust_level_gain(0.1, 0.05, 0.1);
        assert!((target - 1.782_501_8).abs() < 1e-6);
        assert_eq!(exhaust_level_gain(0.1, 0.2, 0.5), 0.5);
        assert_eq!(exhaust_level_gain(0.1, 0.01, 0.1), 2.5);
        assert!((exhaust_level_gain(0.1, 0.05, 0.8) - 1.175).abs() < 1e-6);
        // If a render is already very hot, the peak ceiling wins over the
        // nominal minimum gain rather than allowing a clipped WAV.
        assert!((exhaust_level_gain(0.1, 0.05, 2.) - 0.47).abs() < 1e-6);
    }

    #[test]
    fn source_and_render_stats_reject_silent_or_non_finite_audio() {
        assert!(signal_stats(&[]).is_err());
        assert!(signal_stats(&[0.; 512]).is_err());
        assert!(signal_stats(&[0.15; 512]).is_err());
        assert!(signal_stats(&[f32::NAN, 0.1]).is_err());
        assert!(signal_stats(&[f32::INFINITY, 0.1]).is_err());
        assert!(signal_stats(&[0.2, -0.2]).is_ok());
    }

    #[test]
    fn calibration_uses_ac_rms_but_absolute_pcm_peak() {
        let (source_rms, source_peak) = signal_stats(&[0.15, 0.05]).unwrap();
        let (rendered_rms, rendered_peak) = signal_stats(&[0.06, -0.04]).unwrap();
        assert!((source_rms - 0.05).abs() < 1e-7);
        assert!((source_peak - 0.15).abs() < 1e-7);
        assert!((rendered_rms - 0.05).abs() < 1e-7);
        assert!((rendered_peak - 0.06).abs() < 1e-7);
        assert!(
            (exhaust_level_gain(source_rms, rendered_rms, rendered_peak) - 0.891_250_9).abs()
                < 1e-6
        );
    }
}
