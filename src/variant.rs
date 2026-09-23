//! Add a selectable BESS configuration without replacing the Automation vehicle.
use crate::{
    bank::{self, Bank},
    export,
    hybrid::{Settings, inferred_layer_weight},
    project::Parameters,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::Path,
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_META: u64 = 4_000_000;
const MAX_WAV: u64 = 16_000_000;

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn names(zip: &mut ZipArchive<File>) -> Result<HashSet<String>, String> {
    let mut names = HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        total = total.saturating_add(entry.size());
        if entry.enclosed_name().is_none()
            || entry.name().contains('\\')
            || !names.insert(entry.name().to_owned())
            || total > 2_000_000_000
        {
            return Err("Unsafe, duplicate, or oversized archive entries".into());
        }
    }
    Ok(names)
}

fn read(zip: &mut ZipArchive<File>, name: &str, limit: u64) -> Result<Vec<u8>, String> {
    let entry = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    if entry.size() > limit {
        return Err(format!("Oversized archive entry: {name}"));
    }
    let mut result = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut result)
        .map_err(|e| e.to_string())?;
    if result.len() as u64 > limit {
        return Err(format!("Oversized archive entry: {name}"));
    }
    Ok(result)
}

fn one_match(names: &HashSet<String>, test: impl Fn(&str) -> bool) -> Result<&str, String> {
    let mut matches = names.iter().filter(|name| test(name)).map(String::as_str);
    let first = matches
        .next()
        .ok_or("Required Automation entry is missing")?;
    if matches.next().is_some() {
        return Err("Automation archive has ambiguous matching entries".into());
    }
    Ok(first)
}

fn skip_ws(bytes: &[u8], mut pos: usize) -> usize {
    while bytes.get(pos).is_some_and(u8::is_ascii_whitespace) {
        pos += 1;
    }
    pos
}

// JBeam permits comments and omitted commas. Only isolate the primary engine
// object; duplicating the whole file would overwrite its sibling part keys.
fn object_end(bytes: &[u8], start: usize) -> Result<usize, String> {
    if bytes.get(start) != Some(&b'{') {
        return Err("Engine part is not an object".into());
    }
    let mut depth = 0usize;
    let mut i = start;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            if i == bytes.len() {
                return Err("Unterminated JBeam comment".into());
            }
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b'"' {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            if !closed {
                return Err("Unterminated JBeam string".into());
            }
            continue;
        }
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err("Unterminated JBeam engine part".into())
}

fn clone_engine_part(
    bytes: &[u8],
    original_part: &str,
    new_part: &str,
    original_sample: &str,
    new_exhaust_sample: &str,
    new_engine_sample: &str,
    cylinders: Option<u32>,
) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let quoted = serde_json::to_string(original_part).map_err(|e| e.to_string())?;
    let mut positions = text.match_indices(&quoted).map(|(pos, _)| pos);
    let start = positions
        .next()
        .ok_or("Engine part key not found in JBeam")?;
    if positions.next().is_some() {
        return Err("Engine part key is ambiguous in JBeam".into());
    }
    let mut object = skip_ws(bytes, start + quoted.len());
    if bytes.get(object) != Some(&b':') {
        return Err("Engine part key has no object value".into());
    }
    object = skip_ws(bytes, object + 1);
    let end = object_end(bytes, object)?;
    let original = &text[start..end];
    if original.matches(original_sample).count() != 1 {
        return Err("Engine part has no unique sound sampleName".into());
    }
    let new_key = serde_json::to_string(new_part).map_err(|e| e.to_string())?;
    let mut cloned =
        original
            .replacen(&quoted, &new_key, 1)
            .replacen(original_sample, new_exhaust_sample, 1);
    if !cloned.contains("\"slotType\" : \"Camso_Engine\"")
        && !cloned.contains("\"slotType\": \"Camso_Engine\"")
    {
        return Err("Cloned engine part does not use the Camso_Engine slot".into());
    }
    let main_key = "\"mainEngine\"";
    let mut main_positions = cloned
        .match_indices(main_key)
        .map(|(pos, _)| pos)
        .filter(|&pos| {
            cloned
                .as_bytes()
                .get(skip_ws(cloned.as_bytes(), pos + main_key.len()))
                == Some(&b':')
        });
    let main_start = main_positions
        .next()
        .ok_or("Engine part has no mainEngine")?;
    if main_positions.next().is_some() {
        return Err("Engine part has ambiguous mainEngine fields".into());
    }
    let main_colon = skip_ws(cloned.as_bytes(), main_start + main_key.len());
    if cloned.as_bytes().get(main_colon) != Some(&b':') {
        return Err("mainEngine is not a JBeam field".into());
    }
    let main_object = skip_ws(cloned.as_bytes(), main_colon + 1);
    let main_end = object_end(cloned.as_bytes(), main_object)?;
    let reference = "\"soundConfigExhaust\"";
    let references: Vec<_> = cloned[main_object..main_end]
        .match_indices(reference)
        .map(|(pos, _)| main_object + pos)
        .filter(|&pos| {
            cloned
                .as_bytes()
                .get(skip_ws(cloned.as_bytes(), pos + reference.len()))
                == Some(&b':')
        })
        .collect();
    if references.len() != 1 || cloned[main_object..main_end].contains("\"soundConfig\"") {
        return Err("Expected one exhaust-only mainEngine sound reference".into());
    }
    let reference_pos = references[0];
    let reference_colon = skip_ws(cloned.as_bytes(), reference_pos + reference.len());
    let reference_value = skip_ws(cloned.as_bytes(), reference_colon + 1);
    if cloned.as_bytes().get(reference_colon) != Some(&b':')
        || !cloned[reference_value..].starts_with("\"soundConfigExhaust\"")
    {
        return Err("mainEngine exhaust sound reference is unsupported".into());
    }
    cloned.insert_str(reference_pos, "\"soundConfig\": \"soundConfig\",\n\t\t\t");
    let mut engine_config = json!({
        "sampleName": new_engine_sample,
        "mainGain": -2,
        "intakeMuffling": 0.5,
        "onLoadGain": 1.1,
        "offLoadGain": 0.8,
        "maxLoadMix": 1,
        "minLoadMix": 0,
        "eqFundamentalGain": -3
    });
    if let Some(cylinders) = cylinders {
        engine_config["fundamentalFrequencyCylinderCount"] = json!(cylinders);
    }
    let close = cloned.len() - 1;
    let separator = if cloned[..close].trim_end().ends_with(',') {
        ""
    } else {
        ","
    };
    cloned.insert_str(
        close,
        &format!("{separator}\n\t\t\"soundConfig\": {engine_config}\n\t"),
    );
    Ok(format!("{{\n{cloned}\n}}\n").into_bytes())
}

fn write_file(writer: &mut ZipWriter<File>, path: &str, bytes: &[u8]) -> Result<(), String> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file(path, options)
        .map_err(|e| e.to_string())?;
    writer.write_all(bytes).map_err(|e| e.to_string())
}

fn pcm24(samples: &[f32], gain: f32) -> Result<Vec<u8>, String> {
    let mut wav = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(
            &mut wav,
            hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|e| e.to_string())?;
        for &sample in samples {
            let value = sample * gain;
            if !value.is_finite() || value.abs() > 0.951 {
                return Err("Engine stem is non-finite or clips PCM24".into());
            }
            writer
                .write_sample((value * 8_388_607.) as i32)
                .map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
    }
    Ok(wav.into_inner())
}

pub(crate) fn engine_stem_gain(
    exhaust_rms: f32,
    engine_rms: f32,
    engine_peak: f32,
    load: f32,
    residual_reliability: f32,
) -> f32 {
    if engine_rms < 1e-7 || engine_peak < 1e-7 {
        // A rejected source period has no trustworthy inferred texture. A
        // silent engine-side WAV is valid and preferable to an exhaust copy.
        return 1.;
    }
    // If period analysis could not isolate the exhaust orders at this knot,
    // its "residual" may contain the entire exhaust waveform. Keep that
    // inferred engine layer subordinate instead of normalizing it to the same
    // loudness as a trustworthy residual.
    let target_ratio =
        (if load == 0. { 0.35 } else { 0.55 }) * inferred_layer_weight(residual_reliability);
    // Rejected period templates can leave a very loud exhaust copy in the
    // inferred stem. A fixed minimum gain would defeat this suppression.
    (target_ratio * exhaust_rms / engine_rms)
        .min(12.)
        .min(0.94 / engine_peak)
}

fn profile_identity(name: &str) -> Result<String, String> {
    crate::project::validate_profile_name(name)?;
    let name = name.trim();
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('_') && !slug.is_empty() {
            slug.push('_');
        }
        if slug.len() >= 24 {
            break;
        }
    }
    let slug = slug.trim_matches('_');
    let slug = if slug.is_empty() { "profile" } else { slug };
    let hash = Sha256::digest(name.as_bytes());
    Ok(format!(
        "{slug}_{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2]
    ))
}

fn build(
    source: &Path,
    processed: &Path,
    dir: &Path,
    profile: Option<&str>,
) -> Result<String, String> {
    let source_hash = sha256_file(source)?;
    let processed_hash = sha256_file(processed)?;
    let short = &processed_hash[..10];
    let suffix = if let Some(name) = profile {
        format!("{short}_{}", profile_identity(name)?)
    } else {
        short.to_owned()
    };
    let processed_manifest = processed
        .parent()
        .unwrap_or(Path::new("."))
        .join("manifest.json");
    let previous: Value =
        serde_json::from_slice(&fs::read(&processed_manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if previous["zip_file"]
        != processed
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref()
    {
        return Err("Processed ZIP does not match its manifest".into());
    }
    if previous["render_channel"] != "exhaust" {
        return Err("Variant requires a BESS exhaust-stem render, not a mixed replacement".into());
    }
    if previous["settings"]["procedural"] == true {
        return Err(
            "Experimental synthesis is for the listening interface, not BeamNG export".into(),
        );
    }
    let mut original = ZipArchive::new(File::open(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut rendered = ZipArchive::new(File::open(processed).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let original_names = names(&mut original)?;
    let rendered_names = names(&mut rendered)?;
    if original_names != rendered_names {
        return Err("Rendered vehicle entries differ from the Automation source".into());
    }
    let source_config = one_match(&original_names, |name| {
        let parts: Vec<_> = name.split('/').collect();
        parts.len() == 3 && parts[0] == "vehicles" && parts[2].ends_with(".pc")
    })?
    .to_owned();
    let parts: Vec<_> = source_config.split('/').collect();
    let vehicle = parts[1];
    let root = format!("vehicles/{vehicle}/");
    let old_config = parts[2]
        .strip_suffix(".pc")
        .ok_or("Invalid source config")?;
    let source_info = format!("{root}info_{old_config}.json");
    if !original_names.contains(&source_info) {
        return Err("Original configuration info is missing".into());
    }
    let source_engine = one_match(&original_names, |name| {
        name.starts_with(&root)
            && name.contains("/eng_")
            && name
                .rsplit('/')
                .next()
                .is_some_and(|base| base.starts_with("camso_engine_") && base.ends_with(".jbeam"))
    })?
    .to_owned();
    let source_blend = one_match(&original_names, |name| {
        name.starts_with("art/sound/blends/") && name.ends_with(".sfxBlend2D.json")
    })?
    .to_owned();
    let old_sample = source_blend
        .rsplit('/')
        .next()
        .and_then(|base| base.strip_suffix(".sfxBlend2D.json"))
        .ok_or("Invalid source blend name")?;
    let new_sample = format!("{old_sample}_BESS_{suffix}");
    let engine_sample = format!("{old_sample}_BESS_ENGINE_{suffix}");
    let config_id = format!("bess_{old_config}_{suffix}");
    let config_path = format!("{root}{config_id}.pc");
    let info_path = format!("{root}info_{config_id}.json");
    let engine_path = format!("{root}bess_engine_{suffix}.jbeam");
    let blend_path = format!("art/sound/blends/{new_sample}.sfxBlend2D.json");
    let engine_blend_path = format!("art/sound/blends/{engine_sample}.sfxBlend2D.json");

    let bank = Arc::new(Bank::load(source, Some(&source_blend))?);
    // Standalone conversion may be given a different Automation ZIP with the
    // same member names. The rendered exhaust and reconstructed engine must
    // still come from the exact sound bank recorded in the render manifest.
    if previous["source"]["fingerprint"] != bank.source.fingerprint
        || previous["source"]["blend"] != bank.source.blend
    {
        return Err("Automation source does not match the rendered sound bank".into());
    }
    let p: Parameters = serde_json::from_value(previous["parameters"].clone())
        .map_err(|e| format!("Invalid processed parameters: {e}"))?;
    let h: Settings = serde_json::from_value(previous["settings"].clone())
        .map_err(|e| format!("Invalid processed settings: {e}"))?;
    p.validate()?;
    h.validate()?;

    let mut config: Value = serde_json::from_slice(&read(&mut original, &source_config, MAX_META)?)
        .map_err(|e| e.to_string())?;
    let old_part = config["parts"]["Camso_Engine"]
        .as_str()
        .ok_or("Original configuration has no Camso_Engine part")?
        .to_owned();
    let new_part = format!("{old_part}_BESS_{suffix}");
    config["parts"]["Camso_Engine"] = json!(new_part);
    let config_bytes = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
    let mut info: Value = serde_json::from_slice(&read(&mut original, &source_info, MAX_META)?)
        .map_err(|e| e.to_string())?;
    let base_name = info["Configuration"]
        .as_str()
        .ok_or("Original configuration has no display name")?;
    let display_name = if let Some(name) = profile {
        format!("{base_name} (BESS - {})", name.trim())
    } else {
        format!("{base_name} (BESS)")
    };
    info["Configuration"] = json!(display_name);
    let info_bytes = serde_json::to_vec_pretty(&info).map_err(|e| e.to_string())?;
    let engine_bytes = clone_engine_part(
        &read(&mut original, &source_engine, MAX_META)?,
        &old_part,
        &new_part,
        old_sample,
        &new_sample,
        &engine_sample,
        bank.engine_meta.as_ref().map(|meta| meta.cylinders),
    )?;
    let blend: Value = serde_json::from_slice(&read(&mut original, &source_blend, MAX_META)?)
        .map_err(|e| e.to_string())?;
    let layers = blend["samples"]
        .as_array()
        .filter(|layers| layers.len() == 2)
        .cloned()
        .ok_or("Original blend must have two load layers")?;
    let old_prefix = format!("art/sound/engine/{old_sample}/");
    let new_prefix = format!("art/sound/engine/{new_sample}/");
    let engine_prefix = format!("art/sound/engine/{engine_sample}/");
    let mut exhaust_blend = blend.clone();
    let mut engine_blend = blend;
    let mut wav_pairs = Vec::new();
    let mut seen_old = HashSet::new();
    for (load, layer) in layers.iter().enumerate() {
        for (index, point) in layer
            .as_array()
            .ok_or("Invalid blend load layer")?
            .iter()
            .enumerate()
        {
            let old_path = point[0]
                .as_str()
                .ok_or("Invalid blend WAV path")?
                .to_owned();
            let rpm = point[1]
                .as_f64()
                .filter(|rpm| rpm.is_finite() && (200.0..=20_000.0).contains(rpm))
                .ok_or("Invalid blend RPM")? as f32;
            let suffix = old_path
                .strip_prefix(&old_prefix)
                .filter(|suffix| !suffix.is_empty() && !suffix.contains('/'))
                .ok_or("Blend WAV path does not match sampleName")?;
            if !seen_old.insert(old_path.clone()) || !original_names.contains(&old_path) {
                return Err("Blend has missing or duplicate WAV paths".into());
            }
            let new_path = format!("{new_prefix}{suffix}");
            let engine_path = format!("{engine_prefix}ENG_{suffix}");
            exhaust_blend["samples"][load][index][0] = json!(new_path);
            engine_blend["samples"][load][index][0] = json!(engine_path);
            wav_pairs.push((old_path, new_path, engine_path, rpm, load as f32));
        }
    }
    let blend_bytes = serde_json::to_vec_pretty(&exhaust_blend).map_err(|e| e.to_string())?;
    let engine_blend_bytes = serde_json::to_vec_pretty(&engine_blend).map_err(|e| e.to_string())?;
    let source_thumb = format!("{root}{old_config}.png");
    let target_thumb = format!("{root}{config_id}.png");
    let thumb = if original_names.contains(&source_thumb) {
        Some(read(&mut original, &source_thumb, MAX_META)?)
    } else {
        None
    };
    let mut target_paths: HashSet<String> = [
        config_path.clone(),
        info_path.clone(),
        engine_path.clone(),
        blend_path.clone(),
        engine_blend_path.clone(),
    ]
    .into_iter()
    .collect();
    if thumb.is_some() {
        target_paths.insert(target_thumb.clone());
    }
    for (_, exhaust_path, engine_path, _, _) in &wav_pairs {
        if !target_paths.insert(exhaust_path.clone()) || !target_paths.insert(engine_path.clone()) {
            return Err("Duplicate BESS variant path".into());
        }
    }
    if target_paths
        .iter()
        .any(|path| original_names.contains(path))
    {
        return Err("BESS variant would overwrite an original vehicle file".into());
    }

    // The two BeamNG emitters need separate recordings. The exhaust loops come
    // from the processed archive; the companion stem is reconstructed from the
    // same source, RPM points, load rows, and saved BESS settings.
    let mut exhaust_wavs = Vec::with_capacity(wav_pairs.len());
    let mut engine_loops = Vec::with_capacity(wav_pairs.len());
    let mut engine_gains = Vec::with_capacity(wav_pairs.len());
    let mut engine_ratios = Vec::with_capacity(wav_pairs.len());
    let mut engine_points = Vec::with_capacity(wav_pairs.len());
    for (old_path, _, _, rpm, load) in &wav_pairs {
        let wav = read(&mut rendered, old_path, MAX_WAV)?;
        let spec = hound::WavReader::new(Cursor::new(&wav))
            .map_err(|e| e.to_string())?
            .spec();
        if spec.channels != 1
            || spec.sample_rate != 48_000
            || spec.bits_per_sample != 24
            || spec.sample_format != hound::SampleFormat::Int
        {
            return Err(format!(
                "Rendered WAV is not mono 48 kHz / PCM24: {old_path}"
            ));
        }
        let (_, exhaust) = bank::decode_wav(&wav)?;
        let (_, engine, _) = export::loop_stems(bank.clone(), p, h, *rpm, *load);
        if engine.is_empty() || exhaust.is_empty() || engine.len() < exhaust.len() {
            return Err(format!("Invalid engine or exhaust loop length: {old_path}"));
        }
        let mean_power = |samples: &[f32]| -> f64 {
            samples.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / samples.len() as f64
        };
        let exhaust_rms = mean_power(&exhaust).sqrt() as f32;
        let engine_rms = mean_power(&engine).sqrt() as f32;
        let engine_peak = engine.iter().fold(0f32, |peak, &x| peak.max(x.abs()));
        if !exhaust_rms.is_finite()
            || !engine_rms.is_finite()
            || !engine_peak.is_finite()
            || exhaust_rms < 1e-7
        {
            return Err(format!(
                "Exhaust stem is silent or a rendered stem is non-finite: {old_path}"
            ));
        }
        // The engine-side signal is inferred from an exhaust recording. A
        // fixed row-wide gain made some RPM knots dominate the sound. Balance
        // each knot, but never rescue a weak proxy with an enormous boost.
        let reliability = bank.residual_reliability(*rpm, *load);
        let inference_weight = if h.procedural { 1. } else { reliability };
        let gain = engine_stem_gain(
            exhaust_rms,
            engine_rms,
            engine_peak,
            *load,
            inference_weight,
        );
        let ratio = gain * engine_rms / exhaust_rms;
        engine_gains.push(gain);
        engine_ratios.push(ratio);
        engine_points.push(json!({"rpm":rpm,"load":load,"gain":gain,"rms_ratio":ratio,"residual_reliability":reliability,"engine_inference_weight":inference_weight}));
        exhaust_wavs.push(wav);
        engine_loops.push(engine);
    }
    let row_range = |values: &[f32], layer: f32| -> (f32, f32) {
        wav_pairs
            .iter()
            .zip(values.iter())
            .filter(|((_, _, _, _, load), _)| *load == layer)
            .fold((f32::INFINITY, 0f32), |(min, max), (_, &value)| {
                (min.min(value), max.max(value))
            })
    };
    if row_range(&engine_ratios, 0.).0 == f32::INFINITY
        || row_range(&engine_ratios, 1.).0 == f32::INFINITY
    {
        return Err("Empty blend load layer".into());
    }

    let zip_file = format!("bess-variant-{vehicle}-{suffix}.zip");
    let partial = dir.join(format!("{zip_file}.partial"));
    let mut output = ZipWriter::new(File::create(&partial).map_err(|e| e.to_string())?);
    write_file(&mut output, &config_path, &config_bytes)?;
    write_file(&mut output, &info_path, &info_bytes)?;
    write_file(&mut output, &engine_path, &engine_bytes)?;
    write_file(&mut output, &blend_path, &blend_bytes)?;
    write_file(&mut output, &engine_blend_path, &engine_blend_bytes)?;
    if let Some(bytes) = thumb {
        write_file(&mut output, &target_thumb, &bytes)?;
    }
    for (((_, exhaust_path, engine_path, _, _), (wav, engine)), &gain) in wav_pairs
        .iter()
        .zip(exhaust_wavs.iter().zip(engine_loops.iter()))
        .zip(engine_gains.iter())
    {
        write_file(&mut output, exhaust_path, wav)?;
        write_file(&mut output, engine_path, &pcm24(engine, gain)?)?;
    }
    output.finish().map_err(|e| e.to_string())?;
    let mut check = ZipArchive::new(File::open(&partial).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if names(&mut check)? != target_paths {
        return Err("BESS variant ZIP verification failed".into());
    }
    fs::rename(&partial, dir.join(&zip_file)).map_err(|e| e.to_string())?;
    let wav_paths: Vec<_> = wav_pairs
        .iter()
        .map(|(_, path, _, _, _)| path.clone())
        .collect();
    let engine_wav_paths: Vec<_> = wav_pairs
        .iter()
        .map(|(_, _, path, _, _)| path.clone())
        .collect();
    let report = json!({
        "kind":"configuration_addon",
        "version":env!("CARGO_PKG_VERSION"),
        "zip_file":zip_file,
        "vehicle_id":vehicle,
        "source_config":source_config,
        "config_path":config_path,
        "info_path":info_path,
        "thumbnail_path":if original_names.contains(&source_thumb) { Some(target_thumb) } else { None },
        "engine_path":engine_path,
        "blend_path":blend_path,
        "engine_blend_path":engine_blend_path,
        "wav_paths":wav_paths,
        "engine_wav_paths":engine_wav_paths,
        "display_name":display_name,
        "profile_name":profile,
        "engine_part":new_part,
        "sample_id":new_sample,
        "engine_sample_id":engine_sample,
        "engine_stem_gain":{"off_load":row_range(&engine_gains,0.),"on_load":row_range(&engine_gains,1.)},
        "engine_stem_rms_ratio":{"off_load":row_range(&engine_ratios,0.),"on_load":row_range(&engine_ratios,1.)},
        "engine_stem_points":engine_points,
        "source_archive":source,
        "processed_archive":processed,
        "source_sha256":source_hash,
        "processed_sha256":processed_hash,
        "settings":previous["settings"],
        "parameters":previous["parameters"],
        "gain":previous["gain"],
        "exhaust_level_reference":previous["exhaust_level_reference"],
        "validation":"Add-on paths are disjoint from the original; BeamNG driving and audible behavior still require in-game testing"
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let source_settings = processed
        .parent()
        .unwrap_or(Path::new("."))
        .join("settings.bess.json");
    if source_settings.is_file() {
        let settings_path = dir.join("settings.bess.json");
        fs::copy(source_settings, &settings_path).map_err(|e| e.to_string())?;
        if let Some(name) = profile {
            let mut project: Value =
                serde_json::from_slice(&fs::read(&settings_path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            project["profile_name"] = json!(name.trim());
            fs::write(
                settings_path,
                serde_json::to_vec_pretty(&project).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    fs::write(
        dir.join("INSTALLATION.txt"),
        format!("BESS configuration add-on for {vehicle}\n\n1. Keep the original Automation vehicle ZIP enabled.\n2. Disable any earlier full-replacement BESS ZIP for this vehicle.\n3. Place {zip_file} in your active BeamNG mods folder.\n4. In the vehicle selector, open the original vehicle and choose the {display_name} configuration.\n5. Compare the stock and BESS configurations from the hood, cockpit, and tailpipe cameras.\n\nThe add-on adds a configuration, an alternate engine part, and separate engine/intake and exhaust audio banks. It does not replace the original model or sound files. BESS driving transients are not exported as a BeamNG controller. Structural validation is not an in-game listening test.\n"),
    )
    .map_err(|e| e.to_string())?;
    Ok(zip_file)
}

/// Convert a verified BESS full-vehicle render into an additive BeamNG configuration.
pub fn convert(source: &Path, processed: &Path, dir: &Path) -> Result<String, String> {
    fs::create_dir(dir).map_err(|e| format!("Choose a new output folder: {e}"))?;
    let result = build(source, processed, dir, None);
    if let Err(error) = &result {
        let _ = fs::write(dir.join("ERROR.txt"), error);
    }
    result
}

/// Render and export the default GUI format: one additive BESS configuration.
pub fn package(dir: &Path, p: Parameters, h: Settings, bank: Arc<Bank>) -> Result<String, String> {
    package_with_profile(dir, p, h, bank, None)
}

/// Export a named, independently selectable sound profile for the vehicle.
pub fn package_named(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    profile: &str,
) -> Result<String, String> {
    profile_identity(profile)?;
    package_with_profile(dir, p, h, bank, Some(profile))
}

fn package_with_profile(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    profile: Option<&str>,
) -> Result<String, String> {
    fs::create_dir(dir).map_err(|e| format!("Choose a new output folder: {e}"))?;
    let h = h.for_beamng_export();
    let staging = dir.join(".bess-render");
    let result = (|| {
        export::package_exhaust_stem(&staging, p, h, bank.clone())?;
        let processed = staging.join(export::package_name(&bank));
        let zip_file = build(Path::new(&bank.source.archive), &processed, dir, profile)?;
        let manifest_path = dir.join("manifest.json");
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(&manifest_path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        manifest["processed_archive"] = Value::Null;
        manifest["processed_archive_note"] = json!("Temporary render removed after export");
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Ok(format!("{zip_file} — {}", dir.display()))
    })();
    if result.is_ok() {
        for name in [
            export::package_name(&bank),
            "settings.bess.json".into(),
            "manifest.json".into(),
            "INSTALLATION.txt".into(),
        ] {
            fs::remove_file(staging.join(name)).map_err(|e| e.to_string())?;
        }
        fs::remove_dir(&staging).map_err(|e| e.to_string())?;
    } else if let Err(error) = &result {
        let _ = fs::write(dir.join("ERROR.txt"), error);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_profiles_are_distinct_and_path_safe() {
        let raw = profile_identity("Raw").unwrap();
        let smooth = profile_identity("Smooth").unwrap();
        assert_ne!(raw, smooth);
        assert_eq!(raw, profile_identity("Raw").unwrap());
        assert!(raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        assert!(profile_identity("../bad").is_err());
        assert!(profile_identity(" ").is_err());
    }

    #[test]
    fn rejected_period_cannot_receive_a_normal_engine_stem_balance() {
        let exhaust_rms = 0.05;
        let engine_rms = 0.01;
        let engine_peak = 0.1;
        let trusted = engine_stem_gain(exhaust_rms, engine_rms, engine_peak, 1., 1.);
        let rejected = engine_stem_gain(exhaust_rms, engine_rms, engine_peak, 1., 0.);
        assert!(trusted * engine_rms / exhaust_rms > 0.5);
        assert!(rejected * engine_rms / exhaust_rms < 0.1);
        assert!(rejected < trusted);
        let loud_proxy = engine_stem_gain(0.05, 0.05, 0.1, 1., 0.);
        assert!(loud_proxy * 0.05 / 0.05 <= 0.55 * 0.15 + 1e-6);
        assert_eq!(engine_stem_gain(0.05, 0., 0., 1., 0.), 1.);
    }

    #[test]
    fn clone_only_primary_part_skips_comments_strings_and_siblings() {
        let source = br#"{
"Camso_Engine_abc": { // } not a part end
  "information": {"name":"Literal { brace"},
  "slotType" : "Camso_Engine",
  "soundConfigExhaust": {"sampleName":"SOURCE"},
  /* { ignored } */ "mainEngine": {"note":"escaped \" }", "soundConfigExhaust":"soundConfigExhaust"}
},
"Camso_EngineManagement_abc": {"slotType":"Camso_EngineManagement"}
}"#;
        let cloned = clone_engine_part(
            source,
            "Camso_Engine_abc",
            "Camso_Engine_abc_BESS",
            "SOURCE",
            "SOURCE_BESS",
            "SOURCE_BESS_ENGINE",
            Some(8),
        )
        .unwrap();
        let text = String::from_utf8(cloned).unwrap();
        assert!(text.contains("Camso_Engine_abc_BESS"));
        assert!(text.contains("SOURCE_BESS"));
        assert!(text.contains("SOURCE_BESS_ENGINE"));
        assert!(text.contains("\"soundConfig\": \"soundConfig\""));
        assert!(text.contains("\"fundamentalFrequencyCylinderCount\":8"));
        assert!(!text.contains("Camso_EngineManagement_abc"));
    }
}
