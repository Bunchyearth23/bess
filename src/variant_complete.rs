//! Complete standalone vehicle with preserved originals and an independent BESS configuration.
use super::*;
use crate::export_job::{ExportJob, ExportStage};
use std::collections::BTreeMap;

/// Export the complete original vehicle and a selectable two-emitter BESS profile.
pub fn package_complete_with_job(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    profile: &str,
    job: &ExportJob,
) -> Result<String, String> {
    let result = package_inner(dir, p, h, bank, profile, job);
    job.finish(&result);
    result
}

fn member_hash(reader: &mut impl Read, job: &ExportJob) -> Result<String, String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        job.check()?;
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn package_inner(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    profile: &str,
    job: &ExportJob,
) -> Result<String, String> {
    job.check()?;
    if dir.exists() {
        return Err("Choose a new output folder: the destination already exists".into());
    }
    job.begin(
        ExportStage::Preparing,
        4,
        0,
        "Checking the original vehicle and profile",
    );
    let profile_slug = profile_identity(profile)?;
    p.validate()?;
    let (h, physical_model) = export::resolved_settings(&bank, export::physical_settings(h))?;
    h.validate()?;
    let source = Path::new(&bank.source.archive);
    let fresh = Bank::load(source, Some(&bank.source.blend))?;
    export::verify_source_identity(&bank.source, &fresh.source)?;
    drop(fresh);
    let source_hash = member_hash(&mut File::open(source).map_err(|e| e.to_string())?, job)?;
    // Stable source/profile identity lets BABM replace a revision of this profile,
    // while two profiles retain disjoint configuration, JBeam, blend and WAV paths.
    let mut identity = Sha256::new();
    identity.update(source_hash.as_bytes());
    identity.update([0]);
    identity.update(profile.trim().as_bytes());
    let variant_id = format!("{:x}", identity.finalize());
    let suffix = format!("{}_{}", &variant_id[..20], profile_slug);
    let mut original = ZipArchive::new(File::open(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let original_names = names(&mut original)?;
    if original_names.contains("bess-export.json") {
        return Err(
            "Open the pristine Automation source to export an independent BESS configuration"
                .into(),
        );
    }
    job.advance(1, "Original vehicle identity verified");
    let source_config = one_match(&original_names, |name| {
        let parts: Vec<_> = name.split('/').collect();
        parts.len() == 3 && parts[0] == "vehicles" && parts[2].ends_with(".pc")
    })?
    .to_owned();
    let parts: Vec<_> = source_config.split('/').collect();
    let vehicle = parts[1];
    // Used in host filenames and add-on entry paths: reject `..`, `:` and the like.
    if vehicle.is_empty()
        || !vehicle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("Unsupported vehicle folder name: {vehicle}"));
    }
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
    let display_name = format!("{base_name} (BESS - {})", profile.trim());
    info["Configuration"] = json!(display_name);
    let info_bytes = serde_json::to_vec_pretty(&info).map_err(|e| e.to_string())?;
    let engine_bytes = clone_engine_part(
        &read(&mut original, &source_engine, MAX_META)?,
        &old_part,
        &new_part,
        old_sample,
        &new_sample,
        &engine_sample,
        Some(physical_model.scratch.design.cylinders),
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

    job.check()?;
    job.advance(
        3,
        "Original configuration preserved; independent sound routes prepared",
    );
    let plans: Vec<_> = wav_pairs
        .iter()
        .map(|(name, _, _, rpm, load)| export::RenderPlan {
            name: name.clone(),
            rpm: *rpm,
            load: *load,
        })
        .collect();
    if plans.is_empty()
        || !plans.iter().any(|p| p.load == 0.)
        || !plans.iter().any(|p| p.load == 1.)
    {
        return Err("Empty blend load layer".into());
    }
    job.advance(4, "Ready to generate both sound emitters");
    let rendered = export::render_variant_stems(&plans, bank.clone(), p, h, job)?;
    let peak = rendered
        .iter()
        .flat_map(|r| &r.samples)
        .fold(0f32, |peak, &s| peak.max(s.abs()));
    let common_gain = export::exhaust_safety_gain(peak);
    let zip_file = format!("bess-complete-{vehicle}-{suffix}.zip");
    let partial_name = format!("{zip_file}.partial");
    job.begin(
        ExportStage::Packaging,
        original.len() + target_paths.len() + 1,
        0,
        "Packaging the original vehicle and BESS configuration",
    );
    let mut added = BTreeMap::<String, Vec<u8>>::new();
    added.insert(config_path.clone(), config_bytes);
    added.insert(info_path.clone(), info_bytes);
    added.insert(engine_path.clone(), engine_bytes);
    added.insert(blend_path.clone(), blend_bytes);
    added.insert(engine_blend_path.clone(), engine_blend_bytes);
    if let Some(bytes) = thumb {
        added.insert(target_thumb.clone(), bytes);
    }
    let mut engine_points = Vec::with_capacity(plans.len());
    let mut exhaust_points = Vec::with_capacity(plans.len());
    for ((old_path, exhaust_path, engine_path, rpm, load), rendered) in
        wav_pairs.iter().zip(rendered)
    {
        job.check()?;
        let exhaust_wav = pcm24(&rendered.samples, common_gain)?;
        // Match the original two-emitter exporter exactly: engine balance uses
        // the calibrated and PCM24-quantized exhaust, after its common safety gain.
        let (_, exhaust) = bank::decode_wav(&exhaust_wav)?;
        let engine = rendered.engine.ok_or("Missing companion engine stem")?;
        if engine.is_empty() || exhaust.is_empty() || engine.len() < exhaust.len() {
            return Err(format!("Invalid engine or exhaust loop length: {old_path}"));
        }
        let rms = |samples: &[f32]| -> f32 {
            (samples.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
                as f32
        };
        let exhaust_rms = rms(&exhaust);
        let engine_rms = rms(&engine);
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
        let gain = engine_stem_gain(exhaust_rms, engine_rms, engine_peak, *load, h.engine_gain);
        engine_points.push(json!({"rpm":rpm,"load":load,"gain":gain,"rms_ratio":gain*engine_rms/exhaust_rms,"stem_source":"physical_mechanics"}));
        exhaust_points.push(
            json!({"path":old_path,"rpm":rpm,"load":load,"gain":rendered.exhaust_level_gain}),
        );
        added.insert(
            exhaust_path.clone(),
            export::apply_master_gain(exhaust_wav, p.master_gain)?,
        );
        added.insert(
            engine_path.clone(),
            export::apply_master_gain(pcm24(&engine, gain)?, p.master_gain)?,
        );
    }
    job.check()?;
    if added.keys().cloned().collect::<HashSet<_>>() != target_paths {
        return Err("Incomplete BESS configuration files".into());
    }
    let source_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid original archive name")?;
    let marker = json!({
        "version":1,"kind":"bess-variant-vehicle",
        "source_archive_sha256":source_hash,"source_archive_name":source_name,
        "vehicle_root":root,"blend_path":source_blend,
        "exported_at_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?.as_millis().min(u64::MAX as u128) as u64,
        "variant_id":variant_id,"configuration_path":config_path,
        "added_files":added.iter().map(|(path, bytes)| json!({"path":path,"sha256":format!("{:x}",Sha256::digest(bytes))})).collect::<Vec<_>>()
    });
    let marker_bytes = serde_json::to_vec_pretty(&marker).map_err(|e| e.to_string())?;
    let mut files = export::OutputFiles::create(dir)?;
    let partial = dir.join(&partial_name);
    let mut output = ZipWriter::new(files.reserve(&partial_name)?);
    for index in 0..original.len() {
        job.check()?;
        let entry = original.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().to_owned();
        // Preserve bytes, compression and metadata of every original entry.
        output.raw_copy_file(entry).map_err(|e| e.to_string())?;
        job.advance(index + 1, format!("Preserving {name}"));
    }
    for (index, (path, bytes)) in added.iter().enumerate() {
        job.check()?;
        write_file(&mut output, path, bytes)?;
        job.advance(original.len() + index + 1, format!("Adding {path}"));
    }
    write_file(&mut output, "bess-export.json", &marker_bytes)?;
    output.finish().map_err(|e| e.to_string())?;
    job.advance(
        original.len() + added.len() + 1,
        "Complete vehicle archive written",
    );
    job.begin(
        ExportStage::Verifying,
        original.len() + added.len() + 5,
        0,
        "Verifying original files and both BESS sound routes",
    );
    let mut check = ZipArchive::new(File::open(&partial).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let expected: HashSet<_> = original_names
        .iter()
        .chain(target_paths.iter())
        .cloned()
        .chain(["bess-export.json".to_owned()])
        .collect();
    if names(&mut check)? != expected {
        return Err("Complete vehicle ZIP member verification failed".into());
    }
    // Decompress and hash every member, forcing CRC validation; compare original
    // payloads directly against the source, including its unmodified sound bank.
    for index in 0..original.len() {
        let mut source_entry = original.by_index(index).map_err(|e| e.to_string())?;
        let name = source_entry.name().to_owned();
        let source_digest = member_hash(&mut source_entry, job)?;
        let digest = member_hash(&mut check.by_name(&name).map_err(|e| e.to_string())?, job)?;
        if digest != source_digest {
            return Err(format!("Original vehicle member changed: {name}"));
        }
        job.advance(index + 1, format!("Original verified: {name}"));
    }
    for (index, (path, bytes)) in added.iter().enumerate() {
        let digest = member_hash(&mut check.by_name(path).map_err(|e| e.to_string())?, job)?;
        if digest != format!("{:x}", Sha256::digest(bytes)) {
            return Err(format!("BESS member verification failed: {path}"));
        }
        job.advance(original.len() + index + 1, format!("BESS verified: {path}"));
    }
    if read(&mut check, "bess-export.json", MAX_META)? != marker_bytes {
        return Err("BESS marker verification failed".into());
    }
    // Final recordings can intentionally be silent at master zero. Verify
    // their PCM and routes without the audition bank's level normalization.
    for (index, (blend, frames)) in [(&engine_blend_path, 192_000), (&blend_path, 96_000)]
        .iter()
        .enumerate()
    {
        export::verify_generated_bank(&mut check, blend, plans.len(), *frames, job)?;
        job.advance(
            original.len() + added.len() + index + 1,
            "BESS sound bank verified",
        );
    }
    drop(check);
    job.check()?;
    let verified_original = Bank::load(&partial, Some(&source_blend))?;
    if verified_original.layers.iter().map(Vec::len).sum::<usize>() != plans.len() {
        return Err("Incomplete original sound bank".into());
    }
    job.advance(
        original.len() + added.len() + 3,
        "Original sound bank verified",
    );
    if member_hash(&mut File::open(source).map_err(|e| e.to_string())?, job)? != source_hash {
        return Err("Original source archive changed during export".into());
    }
    let wav_paths: Vec<_> = wav_pairs.iter().map(|(_, p, _, _, _)| p).collect();
    let engine_wav_paths: Vec<_> = wav_pairs.iter().map(|(_, _, p, _, _)| p).collect();
    let report = json!({
        "kind":"complete_vehicle_variant","render_model":"physical_automation","render_channel":"two_emitters",
        "version":env!("CARGO_PKG_VERSION"),"zip_file":zip_file,"vehicle_id":vehicle,
        "source_config":source_config,"config_path":config_path,"info_path":info_path,
        "engine_path":engine_path,"blend_path":blend_path,"engine_blend_path":engine_blend_path,
        "wav_paths":wav_paths,"engine_wav_paths":engine_wav_paths,"display_name":display_name,"profile_name":profile.trim(),
        "engine_part":new_part,"sample_id":new_sample,"engine_sample_id":engine_sample,"variant_id":variant_id,
        "engine_stem_points":engine_points,"source_archive":source,"source":bank.source,
        "source_sha256":source_hash,"source_engine_fingerprint":bank.source.engine_fingerprint,
        "engine_definition":h.engine,"engine_baseline":physical_model.baseline,"engine_provenance":physical_model.provenance,
        "physical_assumptions":physical_model.assumptions,"physical_sound":h.physical_sound,"loop_policy":export::loop_policy(),
        "settings":h,"parameters":p,"gain":common_gain,"master_gain":p.master_gain,"master_gain_applied":true,"exhaust_level_reference":{"points":exhaust_points},
        "validation":"Original members preserved and both BESS sound banks verified. BeamNG driving and listening remain to be tested."
    });
    files.write(
        "manifest.json",
        &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )?;
    let project = crate::project::Project {
        version: 4,
        parameters: p,
        hybrid: h,
        source: Some(bank.source.clone()),
        driving: Default::default(),
        profile_name: profile.trim().to_owned(),
        scratch: None,
    };
    drop(files.reserve("settings.bess.json")?);
    crate::project::save_project(&dir.join("settings.bess.json"), &project)?;
    files.write("INSTALLATION.txt", format!("BESS complete vehicle: {vehicle}\n\nThis ZIP includes the unchanged original vehicle and the independently selectable {display_name} configuration.\nInstall this complete ZIP on its own, or import it with BABM to update/group your vehicles. BABM handles merging. Avoid enabling an additional copy of the same original vehicle.\nChoose the original configuration for stock audio, or {display_name} for the BESS two-emitter sound.\nStructural and sound-file verification does not replace driving and listening in BeamNG.\n").as_bytes())?;
    job.advance(
        original.len() + added.len() + 5,
        "Verified complete vehicle ready",
    );
    job.publish(|| files.publish(&partial_name, &zip_file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    fn fixture() -> PathBuf {
        const UID: &str = "694E80154252F6189DE80988120C7F13";
        let base = crate::test_support::automation_fixture();
        let path = base.with_extension("complete-source.zip");
        let mut input = ZipArchive::new(File::open(&base).unwrap()).unwrap();
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut output = ZipWriter::new(file);
        for index in 0..input.len() {
            let mut entry = input.by_index(index).unwrap();
            let mut name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name.ends_with(".wav") {
                name = name.replace("art/sound/", &format!("art/sound/engine/{UID}/"));
            } else if name.ends_with(".sfxBlend2D.json") {
                bytes = String::from_utf8(bytes)
                    .unwrap()
                    .replace("art/sound/", &format!("art/sound/engine/{UID}/"))
                    .into_bytes();
            } else if name.ends_with(".jbeam") {
                bytes = format!(r#"{{"Camso_Engine_694e8":{{"slotType": "Camso_Engine", "mainEngine":{{"soundConfigExhaust":"soundConfigExhaust"}},"soundConfigExhaust":{{"sampleName":"{UID}"}}}},"untouched_sibling":{{"slotType":"keep"}}}}"#).into_bytes();
            }
            write_file(&mut output, &name, &bytes).unwrap();
        }
        write_file(
            &mut output,
            "vehicles/test/info_test.json",
            br#"{"Configuration":"Original","Description":"Unchanged original","extra":42}"#,
        )
        .unwrap();
        write_file(
            &mut output,
            "vehicles/test/test.png",
            b"unchanged-thumbnail-bytes",
        )
        .unwrap();
        output.finish().unwrap();
        drop(input);
        fs::remove_file(base).unwrap();
        path
    }

    fn contents(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut archive = ZipArchive::new(File::open(path).unwrap()).unwrap();
        (0..archive.len())
            .map(|index| {
                let mut entry = archive.by_index(index).unwrap();
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                (entry.name().to_owned(), bytes)
            })
            .collect()
    }

    fn compressed(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let bytes = fs::read(path).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        (0..archive.len())
            .map(|index| {
                let entry = archive.by_index(index).unwrap();
                let start = entry.data_start() as usize;
                (
                    entry.name().to_owned(),
                    bytes[start..start + entry.compressed_size() as usize].to_vec(),
                )
            })
            .collect()
    }

    fn export_case(
        source: &Path,
        label: &str,
        settings: Settings,
        profile: &str,
        workers: usize,
    ) -> (PathBuf, Value) {
        let dir = source.with_extension(label);
        let bank = Arc::new(Bank::load(source, None).unwrap());
        let job = ExportJob::with_worker_limit(workers);
        let name =
            package_complete_with_job(&dir, Parameters::default(), settings, bank, profile, &job)
                .unwrap();
        assert_eq!(job.snapshot().stage, ExportStage::Complete);
        job.cancel();
        assert!(!job.is_cancelled());
        let archive = dir.join(name);
        let marker = serde_json::from_slice(&contents(&archive)["bess-export.json"]).unwrap();
        (archive, marker)
    }

    #[test]
    fn generated_bank_verification_preserves_2370_rpm_legacy_lengths_and_rejects_truncation() {
        let source = fixture();
        let bank = Arc::new(Bank::load(&source, None).unwrap());
        let (exhaust, engine, _) =
            export::loop_stems(bank, Parameters::default(), Settings::default(), 2370., 0.)
                .unwrap();
        // These are the lengths in the existing Thunderhawk legacy export.
        // Preserve its f32 cycle rounding rather than changing the sound.
        assert_eq!(exhaust.len(), 97_215);
        assert_eq!(engine.len(), 191_999);
        for (nominal, expected) in [(96_000, 97_215), (192_000, 191_999)] {
            for frames in [expected - 1, expected, expected + 1] {
                let path = source.with_extension(format!("verify-{nominal}-{frames}.zip"));
                let file = File::options()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .unwrap();
                let mut writer = ZipWriter::new(file);
                write_file(
                    &mut writer,
                    "rounded.sfxBlend2D.json",
                    br#"{"samples":[[["off.wav",2370]],[["on.wav",2370]]]}"#,
                )
                .unwrap();
                // Silence is valid with master gain zero; length and format
                // verification must remain strict even for this final output.
                let wav = pcm24(&vec![0.; frames], 1.).unwrap();
                write_file(&mut writer, "off.wav", &wav).unwrap();
                write_file(&mut writer, "on.wav", &wav).unwrap();
                writer.finish().unwrap();
                let mut archive = ZipArchive::new(File::open(&path).unwrap()).unwrap();
                let result = export::verify_generated_bank(
                    &mut archive,
                    "rounded.sfxBlend2D.json",
                    2,
                    nominal,
                    &ExportJob::default(),
                );
                assert_eq!(
                    result.is_ok(),
                    frames == expected,
                    "{nominal} nominal frames, {frames} actual: {result:?}"
                );
                drop(archive);
                fs::remove_file(path).unwrap();
            }
        }
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn master_gain_is_final_and_uncompensated_in_complete_legacy_and_replacement_exports() {
        let source = fixture();
        let original_bytes = fs::read(&source).unwrap();
        let original_members = contents(&source);
        let bank = Arc::new(Bank::load(&source, None).unwrap());
        let mut settings = export::resolved_settings(&bank, Settings::default())
            .unwrap()
            .0;
        settings.engine_gain = 1.;
        settings.engine.as_mut().unwrap().experimental.wave_coupling = true;
        for mode in ["complete", "replacement", "legacy-variant"] {
            let mut reference = BTreeMap::<String, Vec<i32>>::new();
            let mut expected_safety_gain = None;
            for (index, master_gain) in [1., 0.5, 0.].into_iter().enumerate() {
                let out = source.with_extension(format!("master-{mode}-{index}"));
                let p = Parameters {
                    master_gain,
                    ..Parameters::default()
                };
                let job = ExportJob::with_worker_limit(3);
                match mode {
                    "complete" => {
                        package_complete_with_job(&out, p, settings, bank.clone(), "Natural", &job)
                    }
                    "replacement" => {
                        export::package_with_job(&out, p, settings, bank.clone(), &job)
                    }
                    _ => crate::variant::package_named(&out, p, settings, bank.clone(), "Natural"),
                }
                .unwrap();
                if mode != "legacy-variant" {
                    assert_eq!(job.snapshot().stage, ExportStage::Complete);
                }
                let report: Value =
                    serde_json::from_slice(&fs::read(out.join("manifest.json")).unwrap()).unwrap();
                assert_eq!(report["master_gain"], json!(master_gain));
                assert_eq!(report["master_gain_applied"], true);
                if let Some(gain) = &expected_safety_gain {
                    assert_eq!(&report["gain"], gain, "master affected calibration");
                } else {
                    expected_safety_gain = Some(report["gain"].clone());
                }
                let reloaded =
                    crate::project::load_project(&out.join("settings.bess.json")).unwrap();
                assert_eq!(reloaded.parameters.master_gain, master_gain);
                // Legacy APIs return a human-readable completion message;
                // their manifest is the authoritative output ZIP filename.
                let members = contents(&out.join(report["zip_file"].as_str().unwrap()));
                let wavs: Vec<&str> = if mode == "replacement" {
                    report["loops"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| p["path"].as_str().unwrap())
                        .collect()
                } else {
                    report["wav_paths"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .chain(report["engine_wav_paths"].as_array().unwrap())
                        .map(|p| p.as_str().unwrap())
                        .collect()
                };
                assert_eq!(wavs.len(), if mode == "replacement" { 4 } else { 8 });
                for path in wavs {
                    let key = path.rsplit('/').next().unwrap().to_owned();
                    let mut reader = hound::WavReader::new(Cursor::new(&members[path])).unwrap();
                    let samples = reader
                        .samples::<i32>()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    if master_gain == 1. {
                        assert!(
                            samples.iter().any(|s| *s != 0),
                            "fixture should exercise audible {path}"
                        );
                        reference.insert(key, samples);
                    } else {
                        let original = &reference[&key];
                        assert_eq!(samples.len(), original.len());
                        for (&actual, &full) in samples.iter().zip(original) {
                            assert_eq!(
                                actual,
                                (full as f32 * master_gain) as i32,
                                "wrong final gain: {mode} {path}"
                            );
                        }
                    }
                }
                if mode == "complete" {
                    for (path, bytes) in &original_members {
                        assert_eq!(
                            &members[path], bytes,
                            "original A changed with master {master_gain}"
                        );
                    }
                }
                fs::remove_dir_all(out).unwrap();
            }
        }
        assert_eq!(fs::read(&source).unwrap(), original_bytes);
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn complete_vehicle_preserves_originals_and_two_emitter_pcm_across_workers_and_profiles() {
        let source = fixture();
        let source_bytes = fs::read(&source).unwrap();
        let original = contents(&source);
        let original_compressed = compressed(&source);
        let artifact_dir = std::env::var_os("BESS_VARIANT_TEST_ARTIFACTS").map(PathBuf::from);
        if let Some(dir) = &artifact_dir {
            fs::create_dir(dir).unwrap();
            fs::create_dir(dir.join("exports")).unwrap();
            fs::copy(&source, dir.join("original.zip")).unwrap();
        }
        let bank = Bank::load(&source, None).unwrap();
        let mut settings = export::resolved_settings(&bank, Settings::default())
            .unwrap()
            .0;
        settings.engine_gain = 1.;
        let mut outputs = Vec::new();
        let mut first_marker = Value::Null;
        let mut first_members = BTreeMap::new();
        for coupled in [false, true] {
            settings.engine.as_mut().unwrap().experimental.wave_coupling = coupled;
            let (serial, marker) = export_case(
                &source,
                &format!("complete-one-{coupled}"),
                settings,
                "Natural",
                1,
            );
            let (parallel, parallel_marker) = export_case(
                &source,
                &format!("complete-many-{coupled}"),
                settings,
                "Natural",
                3,
            );
            let a = contents(&serial);
            let b = contents(&parallel);
            let b_compressed = compressed(&parallel);
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (name, bytes) in &a {
                if name != "bess-export.json" {
                    assert_eq!(bytes, &b[name], "parallel changed {name}");
                }
            }
            for (name, bytes) in &original {
                assert_eq!(bytes, &b[name], "original member changed: {name}");
                assert_eq!(
                    original_compressed[name], b_compressed[name],
                    "original recompressed: {name}"
                );
            }
            assert_eq!(marker["kind"], "bess-variant-vehicle");
            assert_eq!(marker["added_files"], parallel_marker["added_files"]);
            let added: HashSet<_> = marker["added_files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["path"].as_str().unwrap())
                .collect();
            assert!(added.iter().all(|name| !original.contains_key(*name)));
            assert_eq!(b.len(), original.len() + added.len() + 1);
            for item in marker["added_files"].as_array().unwrap() {
                let path = item["path"].as_str().unwrap();
                assert_eq!(item["sha256"], format!("{:x}", Sha256::digest(&b[path])));
            }
            let config_path = marker["configuration_path"].as_str().unwrap();
            let mut config: Value = serde_json::from_slice(&b[config_path]).unwrap();
            let source_config: Value =
                serde_json::from_slice(&original["vehicles/test/test.pc"]).unwrap();
            let new_part = config["parts"]["Camso_Engine"].as_str().unwrap().to_owned();
            config["parts"]["Camso_Engine"] = source_config["parts"]["Camso_Engine"].clone();
            assert_eq!(config, source_config);
            let engine_path = added.iter().find(|p| p.ends_with(".jbeam")).unwrap();
            let engine: Value = serde_json::from_slice(&b[*engine_path]).unwrap();
            assert_eq!(engine.as_object().unwrap().len(), 1);
            assert!(engine.get("untouched_sibling").is_none());
            for (route, minimum_frames) in
                [("soundConfigExhaust", 96_000), ("soundConfig", 192_000)]
            {
                assert_eq!(engine[&new_part]["mainEngine"][route], route);
                let sample = engine[&new_part][route]["sampleName"].as_str().unwrap();
                let blend_path = format!("art/sound/blends/{sample}.sfxBlend2D.json");
                let blend: Value = serde_json::from_slice(&b[&blend_path]).unwrap();
                for layer in blend["samples"].as_array().unwrap() {
                    for point in layer.as_array().unwrap() {
                        let wav_path = point[0].as_str().unwrap();
                        assert!(added.contains(wav_path));
                        let reader = hound::WavReader::new(Cursor::new(&b[wav_path])).unwrap();
                        assert_eq!(reader.spec().channels, 1);
                        assert_eq!(reader.spec().sample_rate, 48_000);
                        assert_eq!(reader.spec().bits_per_sample, 24);
                        assert!(reader.duration() >= minimum_frames);
                    }
                }
            }
            if coupled {
                first_marker = marker;
                first_members = b;
                if let Some(dir) = &artifact_dir {
                    fs::copy(&parallel, dir.join("exports/first.zip")).unwrap();
                }
            }
            outputs.extend([serial, parallel]);
        }
        settings.engine_gain = 0.;
        let (revised, revised_marker) =
            export_case(&source, "complete-revised", settings, "Natural", 2);
        assert_eq!(first_marker["variant_id"], revised_marker["variant_id"]);
        assert_eq!(
            first_marker["configuration_path"],
            revised_marker["configuration_path"]
        );
        assert_ne!(first_marker["added_files"], revised_marker["added_files"]);
        assert!(
            revised_marker["exported_at_unix_ms"].as_u64().unwrap()
                > first_marker["exported_at_unix_ms"].as_u64().unwrap()
        );
        let revised_members = contents(&revised);
        for (name, bytes) in &revised_members {
            if name.contains("_BESS_ENGINE_") && name.ends_with(".wav") {
                assert!(bank::decode_wav(bytes).unwrap().1.iter().all(|x| *x == 0.));
                assert_ne!(bytes, &first_members[name]);
            }
        }
        let (other, other_marker) = export_case(&source, "complete-other", settings, "Track", 2);
        assert_ne!(first_marker["variant_id"], other_marker["variant_id"]);
        let first_added: HashSet<_> = first_marker["added_files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert!(
            other_marker["added_files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| !first_added.contains(f["path"].as_str().unwrap()))
        );
        if let Some(dir) = &artifact_dir {
            fs::copy(&revised, dir.join("exports/revised.zip")).unwrap();
            fs::copy(&other, dir.join("exports/other-profile.zip")).unwrap();
        }
        outputs.extend([revised, other]);
        for archive in outputs {
            fs::remove_dir_all(archive.parent().unwrap()).unwrap();
        }
        assert_eq!(fs::read(&source).unwrap(), source_bytes);
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn complete_vehicle_cancel_cleans_only_owned_outputs_and_preserves_source() {
        let source = fixture();
        let original = fs::read(&source).unwrap();
        for stage in [ExportStage::Rendering, ExportStage::Packaging] {
            let out = source.with_extension(format!("cancel-{stage:?}"));
            let bank = Arc::new(Bank::load(&source, None).unwrap());
            let job = ExportJob::with_worker_limit(2);
            let observer = job.clone();
            let watched = out.clone();
            let canceller = std::thread::spawn(move || {
                let start = Instant::now();
                let mut previous_fraction = 0.;
                loop {
                    let progress = observer.snapshot();
                    if progress.stage == ExportStage::Rendering {
                        let fraction = progress.fraction.unwrap_or(0.);
                        assert!(fraction >= previous_fraction);
                        previous_fraction = fraction;
                    }
                    let partial_exists = watched.is_dir()
                        && fs::read_dir(&watched).unwrap().any(|p| {
                            p.unwrap()
                                .path()
                                .extension()
                                .is_some_and(|e| e == "partial")
                        });
                    if progress.stage == stage
                        && ((stage == ExportStage::Rendering && previous_fraction > 0.)
                            || partial_exists)
                    {
                        if partial_exists {
                            fs::write(watched.join("user-note.txt"), b"keep this file").unwrap();
                        }
                        observer.cancel();
                        return;
                    }
                    assert!(!matches!(
                        progress.stage,
                        ExportStage::Complete | ExportStage::Failed
                    ));
                    assert!(start.elapsed() < Duration::from_secs(60));
                    std::thread::yield_now();
                }
            });
            let result = package_complete_with_job(
                &out,
                Parameters::default(),
                Settings::default(),
                bank,
                "Natural",
                &job,
            );
            canceller.join().unwrap();
            assert_eq!(result.unwrap_err(), crate::export_job::CANCELLED);
            assert_eq!(job.snapshot().stage, ExportStage::Cancelled);
            if stage == ExportStage::Packaging {
                let entries: Vec<_> = fs::read_dir(&out)
                    .unwrap()
                    .map(|e| e.unwrap().file_name())
                    .collect();
                assert_eq!(entries, [std::ffi::OsString::from("user-note.txt")]);
                assert_eq!(
                    fs::read(out.join("user-note.txt")).unwrap(),
                    b"keep this file"
                );
                fs::remove_file(out.join("user-note.txt")).unwrap();
                fs::remove_dir(out).unwrap();
            } else {
                assert!(!out.exists());
            }
        }
        assert_eq!(fs::read(&source).unwrap(), original);
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn complete_vehicle_rejects_pre_cancel_existing_output_and_invalid_source() {
        let source = fixture();
        let bank = Arc::new(Bank::load(&source, None).unwrap());
        let out = source.with_extension("not-created");
        let job = ExportJob::default();
        job.cancel();
        assert!(
            package_complete_with_job(
                &out,
                Parameters::default(),
                Settings::default(),
                bank.clone(),
                "Natural",
                &job
            )
            .is_err()
        );
        assert_eq!(job.snapshot().stage, ExportStage::Cancelled);
        assert!(!out.exists());
        fs::create_dir(&out).unwrap();
        fs::write(out.join("foreign.txt"), b"preserve").unwrap();
        let job = ExportJob::default();
        assert!(
            package_complete_with_job(
                &out,
                Parameters::default(),
                Settings::default(),
                bank,
                "Natural",
                &job
            )
            .is_err()
        );
        assert_eq!(job.snapshot().stage, ExportStage::Failed);
        assert_eq!(job.snapshot().workers, 0);
        assert_eq!(fs::read(out.join("foreign.txt")).unwrap(), b"preserve");
        fs::remove_file(out.join("foreign.txt")).unwrap();
        fs::remove_dir(out).unwrap();
        fs::remove_file(source).unwrap();
        let invalid = crate::test_support::automation_fixture();
        let bank = Arc::new(Bank::load(&invalid, None).unwrap());
        let job = ExportJob::default();
        let out = invalid.with_extension("invalid-never-created");
        assert!(
            package_complete_with_job(
                &out,
                Parameters::default(),
                Settings::default(),
                bank,
                "Natural",
                &job
            )
            .unwrap_err()
            .contains("configuration info")
        );
        assert_eq!(job.snapshot().stage, ExportStage::Failed);
        assert_eq!(job.snapshot().workers, 0);
        assert!(!out.exists());
        fs::remove_file(invalid).unwrap();
    }

    #[test]
    fn publication_never_overwrites_a_foreign_destination() {
        let source = fixture();
        let out = source.with_extension("publish-collision");
        {
            let mut files = export::OutputFiles::create(&out).unwrap();
            files.write("owned.partial", b"verified new ZIP").unwrap();
            fs::write(out.join("destination.zip"), b"foreign ZIP").unwrap();
            assert!(files.publish("owned.partial", "destination.zip").is_err());
        }
        assert_eq!(
            fs::read(out.join("destination.zip")).unwrap(),
            b"foreign ZIP"
        );
        assert!(!out.join("owned.partial").exists());
        fs::remove_file(out.join("destination.zip")).unwrap();
        fs::remove_dir(out).unwrap();
        fs::remove_file(source).unwrap();
    }
}
