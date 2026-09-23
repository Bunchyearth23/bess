//! Build and audit the 12 side-by-side BeamNG configuration add-ons.
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    error::Error,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name]
        .as_str()
        .ok_or_else(|| invalid(format!("Missing string field {name}")))
        .map_err(Into::into)
}

fn archive_name(path: &str) -> Result<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("Invalid archive path: {path}")))
        .map_err(Into::into)
}

fn member_bytes(zip: &mut zip::ZipArchive<fs::File>, path: &str, limit: u64) -> Result<Vec<u8>> {
    let entry = zip.by_name(path)?;
    if entry.size() > limit {
        return Err(invalid(format!("Oversized archive entry: {path}")).into());
    }
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn member_string(zip: &mut zip::ZipArchive<fs::File>, path: &str, limit: u64) -> Result<String> {
    Ok(String::from_utf8(member_bytes(zip, path, limit)?)?)
}

fn names(zip: &mut zip::ZipArchive<fs::File>) -> Result<HashSet<String>> {
    let mut seen = HashSet::with_capacity(zip.len());
    for index in 0..zip.len() {
        let name = zip.by_index(index)?.name().to_owned();
        if !seen.insert(name.clone()) {
            return Err(invalid(format!("Duplicate ZIP entry: {name}")).into());
        }
    }
    Ok(seen)
}

fn wav_count(zip: &mut zip::ZipArchive<fs::File>, path: &str) -> Result<usize> {
    let entry = zip.by_name(path)?;
    let mut reader = hound::WavReader::new(entry)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 48_000
        || spec.bits_per_sample != 24
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(invalid(format!("Expected mono 48 kHz PCM24 WAV: {path}")).into());
    }
    let mut frames = 0;
    for sample in reader.samples::<i32>() {
        sample?;
        frames += 1;
    }
    if frames == 0 {
        return Err(invalid(format!("Empty WAV: {path}")).into());
    }
    Ok(frames)
}

fn audit(
    source_path: &Path,
    processed_path: &Path,
    variant_dir: &Path,
    returned_name: &str,
) -> Result<Value> {
    let manifest: Value = serde_json::from_slice(&fs::read(variant_dir.join("manifest.json"))?)?;
    let zip_name = field(&manifest, "zip_file")?;
    if zip_name != returned_name || archive_name(zip_name)? != zip_name {
        return Err(invalid("Variant ZIP name does not match convert result").into());
    }
    if fs::canonicalize(field(&manifest, "source_archive")?)? != fs::canonicalize(source_path)?
        || fs::canonicalize(field(&manifest, "processed_archive")?)?
            != fs::canonicalize(processed_path)?
    {
        return Err(invalid("Variant manifest points to the wrong input archives").into());
    }

    let config_path = field(&manifest, "config_path")?;
    let info_path = field(&manifest, "info_path")?;
    let engine_path = field(&manifest, "engine_path")?;
    let blend_path = field(&manifest, "blend_path")?;
    let engine_blend_path = field(&manifest, "engine_blend_path")?;
    let display_name = field(&manifest, "display_name")?;
    let thumbnail_path = match &manifest["thumbnail_path"] {
        Value::Null => None,
        Value::String(path) => Some(path.as_str()),
        _ => return Err(invalid("thumbnail_path must be a string or null").into()),
    };
    if !display_name.ends_with(" (BESS)") {
        return Err(invalid("Configuration is not labelled (BESS)").into());
    }
    let wav_paths = manifest["wav_paths"]
        .as_array()
        .ok_or_else(|| invalid("Missing wav_paths array"))?;
    let engine_wav_paths = manifest["engine_wav_paths"]
        .as_array()
        .ok_or_else(|| invalid("Missing engine_wav_paths array"))?;
    let mut expected = HashSet::new();
    for path in [
        config_path,
        info_path,
        engine_path,
        blend_path,
        engine_blend_path,
    ] {
        if !expected.insert(path.to_owned()) {
            return Err(invalid(format!("Duplicate declared add-on path: {path}")).into());
        }
    }
    if let Some(path) = thumbnail_path
        && (!path.ends_with(".png") || !expected.insert(path.to_owned()))
    {
        return Err(invalid(format!("Duplicate or invalid thumbnail path: {path}")).into());
    }
    for wav in wav_paths.iter().chain(engine_wav_paths.iter()) {
        let path = wav.as_str().ok_or_else(|| invalid("Non-string WAV path"))?;
        if !path.ends_with(".wav") || !expected.insert(path.to_owned()) {
            return Err(invalid(format!("Duplicate or invalid WAV path: {path}")).into());
        }
    }

    let mut source = zip::ZipArchive::new(fs::File::open(source_path)?)?;
    let source_names = names(&mut source)?;
    let mut addon = zip::ZipArchive::new(fs::File::open(variant_dir.join(zip_name))?)?;
    let addon_names = names(&mut addon)?;
    if expected != addon_names {
        return Err(invalid("Add-on ZIP entries differ from its manifest paths").into());
    }
    if let Some(path) = addon_names.intersection(&source_names).next() {
        return Err(invalid(format!("Add-on overwrites an original entry: {path}")).into());
    }

    let (root, config_file) = config_path
        .rsplit_once('/')
        .ok_or_else(|| invalid("Invalid configuration path"))?;
    let config_id = config_file
        .strip_suffix(".pc")
        .ok_or_else(|| invalid("Configuration is not a .pc file"))?;
    if info_path != format!("{root}/info_{config_id}.json")
        || !engine_path.starts_with(&format!("{root}/"))
    {
        return Err(
            invalid("Configuration, metadata, and engine have different vehicle roots").into(),
        );
    }
    let info: Value = serde_json::from_str(&member_string(&mut addon, info_path, 1_000_000)?)?;
    if info["Configuration"] != display_name {
        return Err(invalid("Configuration display name differs from variant manifest").into());
    }
    let pc: Value = serde_json::from_str(&member_string(&mut addon, config_path, 100_000)?)?;
    let new_part = pc["parts"]["Camso_Engine"]
        .as_str()
        .ok_or_else(|| invalid("Variant .pc does not select a Camso_Engine part"))?;
    let original_pc_name = source_names
        .iter()
        .find(|name| {
            name.starts_with(&format!("{root}/"))
                && name.ends_with(".pc")
                && name.matches('/').count() == 2
        })
        .ok_or_else(|| invalid("Original vehicle configuration is missing"))?
        .to_owned();
    let source_config = field(&manifest, "source_config")?;
    if source_config != original_pc_name {
        return Err(invalid("Variant manifest points to the wrong original configuration").into());
    }
    let source_thumbnail = source_config
        .strip_suffix(".pc")
        .map(|stem| format!("{stem}.png"))
        .ok_or_else(|| invalid("Original configuration is not a .pc file"))?;
    if source_names.contains(&source_thumbnail) != thumbnail_path.is_some() {
        return Err(invalid("Variant thumbnail presence differs from the original").into());
    }
    if let Some(path) = thumbnail_path {
        let expected_path = format!("{root}/{config_id}.png");
        if path != expected_path {
            return Err(invalid("Variant thumbnail path does not match its configuration").into());
        }
        let original_bytes = member_bytes(&mut source, &source_thumbnail, 1_000_000)?;
        let variant_bytes = member_bytes(&mut addon, path, 1_000_000)?;
        if original_bytes != variant_bytes {
            return Err(invalid("Variant thumbnail differs from the original").into());
        }
    }
    let original_pc: Value =
        serde_json::from_str(&member_string(&mut source, &original_pc_name, 100_000)?)?;
    if original_pc["parts"]["Camso_Engine"] == new_part {
        return Err(invalid("Variant still selects the original engine part").into());
    }
    let engine = member_string(&mut addon, engine_path, 1_000_000)?;
    if !engine.contains(&format!("\"{new_part}\"")) || !engine.contains("Camso_Engine") {
        return Err(invalid("New engine part is absent from the add-on JBeam").into());
    }
    let blend_uid = blend_path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".sfxBlend2D.json"))
        .ok_or_else(|| invalid("Invalid blend path"))?;
    let engine_blend_uid = engine_blend_path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".sfxBlend2D.json"))
        .ok_or_else(|| invalid("Invalid engine blend path"))?;
    if !engine.contains(&format!("\"{blend_uid}\""))
        || !engine.contains(&format!("\"{engine_blend_uid}\""))
        || !engine.contains("\"soundConfig\": \"soundConfig\"")
    {
        return Err(invalid("New engine does not reference both sound blends").into());
    }
    let mut referenced_wavs = HashSet::new();
    for path in [blend_path, engine_blend_path] {
        let blend: Value = serde_json::from_str(&member_string(&mut addon, path, 1_000_000)?)?;
        for row in blend["samples"]
            .as_array()
            .ok_or_else(|| invalid("Blend has no sample rows"))?
        {
            for sample in row
                .as_array()
                .ok_or_else(|| invalid("Blend sample row is not an array"))?
            {
                let path = sample[0]
                    .as_str()
                    .ok_or_else(|| invalid("Blend contains an invalid WAV reference"))?;
                if !referenced_wavs.insert(path.to_owned()) {
                    return Err(invalid(format!("Duplicate blend WAV reference: {path}")).into());
                }
            }
        }
    }
    let declared_wavs: HashSet<String> = wav_paths
        .iter()
        .chain(engine_wav_paths.iter())
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    if referenced_wavs != declared_wavs {
        return Err(invalid("Blend WAV references differ from variant manifest").into());
    }
    let mut total_frames = 0usize;
    for path in &declared_wavs {
        total_frames += wav_count(&mut addon, path)?;
    }

    Ok(json!({
        "source_archive": source_path,
        "processed_archive": processed_path,
        "addon_archive": variant_dir.join(zip_name),
        "display_name": display_name,
        "engine_part": new_part,
        "wav_loops": declared_wavs.len(),
        "engine_wav_loops": engine_wav_paths.len(),
        "exhaust_wav_loops": wav_paths.len(),
        "wav_frames": total_frames,
        "unique_addon_entries": addon_names.len(),
        "thumbnail_preserved": thumbnail_path.is_some(),
        "source_paths_preserved": true,
        "audio_format": "mono 48000 Hz PCM24"
    }))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err(invalid("Usage: variant_delivery CARS PROCESSED_OUTPUT NEW_OUTPUT").into());
    }
    let cars = Path::new(&args[1]);
    let processed = Path::new(&args[2]);
    let output = Path::new(&args[3]);
    let mut sources: Vec<PathBuf> = fs::read_dir(cars)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::result::Result<_, _>>()?;
    sources.retain(|path| path.extension().is_some_and(|extension| extension == "zip"));
    sources.sort();
    if sources.len() != 12 {
        return Err(invalid(format!(
            "Expected 12 source vehicles, found {}",
            sources.len()
        ))
        .into());
    }

    let mut processed_by_source = BTreeMap::new();
    for entry in fs::read_dir(processed)? {
        let directory = entry?.path();
        if !directory.is_dir() || !directory.join("manifest.json").is_file() {
            continue;
        }
        let manifest: Value = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
        let source_name = archive_name(field(&manifest["source"], "archive")?)?;
        let zip_name = field(&manifest, "zip_file")?;
        if archive_name(zip_name)? != zip_name {
            return Err(invalid("Processed manifest ZIP name is not a filename").into());
        }
        let processed_zip = directory.join(zip_name);
        if !processed_zip.is_file() {
            return Err(invalid(format!(
                "Missing processed ZIP: {}",
                processed_zip.display()
            ))
            .into());
        }
        if processed_by_source
            .insert(source_name.clone(), processed_zip)
            .is_some()
        {
            return Err(invalid(format!("Duplicate processed source: {source_name}")).into());
        }
    }
    if processed_by_source.len() != sources.len() {
        return Err(invalid(format!(
            "Expected {} processed manifests, found {}",
            sources.len(),
            processed_by_source.len()
        ))
        .into());
    }

    fs::create_dir_all(output)?;
    let mut variants = Vec::with_capacity(sources.len());
    let mut wav_loops = 0usize;
    for source in sources {
        let source_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("Invalid source ZIP filename"))?;
        let processed_zip = processed_by_source
            .get(source_name)
            .ok_or_else(|| invalid(format!("No processed archive for {source_name}")))?;
        let directory = output.join(
            source
                .file_stem()
                .ok_or_else(|| invalid("Invalid source ZIP stem"))?,
        );
        println!("Add BESS configuration: {source_name}");
        let zip_name = bess::variant::convert(&source, processed_zip, &directory)?;
        let report = audit(&source, processed_zip, &directory, &zip_name)?;
        wav_loops += report["wav_loops"].as_u64().unwrap() as usize;
        println!("Verified {} WAV loops", report["wav_loops"]);
        variants.push(report);
    }
    if wav_loops != 1376 {
        return Err(invalid(format!("Expected 1376 WAV loops, found {wav_loops}")).into());
    }
    let report = json!({"vehicles": variants.len(), "wav_loops": wav_loops, "variants": variants});
    fs::write(
        output.join("verification.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Verified {} variants and {wav_loops} WAV loops",
        variants.len()
    );
    Ok(())
}
