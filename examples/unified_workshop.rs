//! Offline proof of the common Automation/scratch definition and workshop.
//! Run with an output directory and optional --export for an uninstalled variant.
use bess::{
    automation_model::{AutomationModel, ValueOrigin},
    bank::Bank,
    engine_definition::EngineDefinition,
    hybrid::Settings,
    physical::engine::{Commands, Engine},
    project::{Parameters, Project, load_project, save_project},
    realtime::DenormalGuard,
    render::{hybrid_samples, write_pcm},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

fn digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn physical_identity(definition: &EngineDefinition) -> Result<(), String> {
    let encoded = serde_json::to_vec(definition).map_err(|e| e.to_string())?;
    let reloaded: EngineDefinition = serde_json::from_slice(&encoded).map_err(|e| e.to_string())?;
    require(
        &reloaded == definition,
        "Engine definition changed in JSON round trip",
    )?;
    reloaded.validate()?;
    let original = definition.to_scratch();
    let migrated = EngineDefinition::from_scratch(&original).to_scratch();
    let mut a = Engine::new(&original, 48000)?;
    let mut b = Engine::new(&migrated, 48000)?;
    for _ in 0..1024 {
        let commands = Commands {
            imposed_rpm: Some(2000.),
            throttle: 0.6,
            ..Default::default()
        };
        let x = a.next(commands);
        let y = b.next(commands);
        require(
            !a.failed() && !b.failed(),
            "Physical engine failed during identity proof",
        )?;
        for (x, y) in [
            (x.exhaust, y.exhaust),
            (x.intake, y.intake),
            (x.mechanical, y.mechanical),
        ] {
            require(
                x.is_finite() && x.to_bits() == y.to_bits(),
                "Physical output differs after definition round trip",
            )?;
        }
    }
    Ok(())
}

fn stats(samples: &[f32]) -> Result<Value, String> {
    require(
        !samples.is_empty() && samples.iter().all(|v| v.is_finite() && v.abs() <= 1.),
        "Invalid or out-of-PCM-range audition",
    )?;
    let mean = samples.iter().map(|&v| f64::from(v)).sum::<f64>() / samples.len() as f64;
    let rms = (samples
        .iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt();
    require(rms > 1e-7, "Silent audition")?;
    let peak = samples.iter().fold(0f32, |peak, v| peak.max(v.abs()));
    Ok(json!({"frames":samples.len(),"peak":peak,"rms_ac":rms,"finite":true,"pcm_range":true}))
}

fn settings(engine: EngineDefinition, baseline: EngineDefinition, enhanced: bool) -> Settings {
    Settings {
        engine: Some(engine),
        engine_baseline: Some(baseline),
        physical_sound: engine.sound,
        enhanced,
        level_match: false,
        ..Default::default()
    }
}

fn parameters(engine: EngineDefinition) -> Parameters {
    Parameters {
        cylinders: engine.design.cylinders,
        rpm: engine.idle_rpm,
        load: 0.6,
        volume: 0.65,
        ..Default::default()
    }
}

fn edited(mut engine: EngineDefinition) -> EngineDefinition {
    engine.sound.exhaust_body_db = 3.;
    engine.sound.presence_db = -2.;
    engine.tuning.intake.plenum_ratio = Some(1.8);
    engine.experimental.afterfire = 0.25;
    engine
}

fn audition(output: &Path, slug: &str, source: &Path) -> Result<Value, String> {
    let before = digest(source)?;
    let bank = Arc::new(Bank::load(source, None)?);
    let model = AutomationModel::from_bank(&bank)?;
    let engine = edited(model.baseline);
    let active = model.with_definition(&engine)?;
    let p = parameters(engine);
    let h = settings(engine, model.baseline, true);
    let path = output.join(format!("{slug}.bess.json"));
    save_project(
        &path,
        &Project {
            version: 4,
            parameters: p,
            hybrid: h,
            source: Some(bank.source.clone()),
            driving: Default::default(),
            profile_name: "Unified workshop".into(),
            scratch: None,
        },
    )?;
    let reloaded = load_project(&path)?;
    require(
        reloaded.version == 4
            && reloaded.hybrid.engine == Some(engine)
            && reloaded.hybrid.engine_baseline == Some(model.baseline)
            && reloaded.source.as_ref() == Some(&bank.source)
            && reloaded.scratch.is_none(),
        "Imported v4 project did not preserve engine edits and source identity",
    )?;
    let restored = AutomationModel::from_settings(&bank, &reloaded.hybrid)?;
    require(
        EngineDefinition::from_scratch(&restored.scratch) == engine,
        "Project edits were lost during model resolution",
    )?;

    let a = hybrid_samples(
        p,
        settings(model.baseline, model.baseline, false),
        bank.clone(),
        8.,
        true,
    )?;
    let a_after_edits = hybrid_samples(
        p,
        settings(engine, model.baseline, false),
        bank.clone(),
        8.,
        true,
    )?;
    require(
        a == a_after_edits,
        "Editing physical B changed original source A",
    )?;
    let b = hybrid_samples(
        p,
        settings(model.baseline, model.baseline, true),
        bank.clone(),
        8.,
        true,
    )?;
    let changed = hybrid_samples(p, h, bank.clone(), 8., true)?;
    require(b != changed, "Workshop edits did not affect physical B")?;
    let mut waves = Vec::new();
    for (name, samples) in [
        ("A-original", &a),
        ("B-imported", &b),
        ("B-edited", &changed),
    ] {
        let wav = output.join(format!("{slug}-{name}.wav"));
        let metrics = stats(samples)?;
        write_pcm(&wav, samples)?;
        waves.push(json!({"name":name,"path":wav,"sha256":digest(&wav)?,"metrics":metrics}));
    }
    require(
        digest(source)? == before,
        "Source archive changed during audition",
    )?;
    println!("{slug}: source A unchanged; imported/edited B and v4 project verified");
    Ok(
        json!({"name":slug,"source":bank.source,"source_rpm_range":[bank.min_rpm,bank.max_rpm],"archive_sha256":before,"archive_unchanged":true,"project":path,"project_roundtrip":true,"source_a_unchanged_after_edits":true,"physical_b_changed":true,"baseline":model.baseline,"effective":engine,"provenance":active.provenance,"assumptions":model.assumptions,"waves":waves}),
    )
}

fn audit_zip(path: &Path) -> Result<Value, String> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut names = HashSet::new();
    let mut waves = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        require(
            entry.enclosed_name().is_some()
                && !entry.name().contains('\\')
                && names.insert(entry.name().to_owned()),
            "Unsafe or duplicate variant ZIP path",
        )?;
        if entry.name().ends_with(".wav") {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            let mut wav =
                hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
            let spec = wav.spec();
            require(
                spec.sample_rate == 48000
                    && spec.channels == 1
                    && spec.bits_per_sample == 24
                    && wav.len() > 0,
                "Unexpected variant WAV format",
            )?;
            for sample in wav.samples::<i32>() {
                sample.map_err(|e| e.to_string())?;
            }
            waves += 1;
        } else {
            std::io::copy(&mut entry, &mut std::io::sink()).map_err(|e| e.to_string())?;
        }
    }
    require(waves > 0, "No physical audio in exported variant")?;
    Ok(
        json!({"path":path,"sha256":digest(path)?,"entries":names.len(),"waves":waves,"crc_read":true,"safe_unique_paths":true,"pcm24_mono_48000":true}),
    )
}

fn main() -> Result<(), String> {
    let _denormals = DenormalGuard::enter();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let export = args.iter().any(|arg| arg == "--export");
    let output = args
        .iter()
        .find(|arg| *arg != "--export")
        .map(PathBuf::from)
        .unwrap_or_else(|| "output/unified-workshop-20261003".into());
    fs::create_dir_all(output.join("listening")).map_err(|e| e.to_string())?;
    let mut paths: Vec<_> = fs::read_dir("cars")
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    paths.retain(|path| path.extension().is_some_and(|e| e == "zip"));
    paths.sort();
    require(
        paths.len() == 12,
        "Expected the 12-vehicle Automation verification corpus",
    )?;
    let selected = [
        ("i4-b5c", "bunchyearth23_b5_c.zip"),
        ("i6-genesis", "bunchyearth23_genesis_phantom.zip"),
        ("v8-advent", "bunchyearth23_advent_tc.zip"),
    ];
    let mut corpus = Vec::new();
    let mut turbo = None;
    let mut lightest = None;
    for path in paths {
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let before = digest(&path)?;
        let bank = Arc::new(Bank::load(&path, None)?);
        let model = AutomationModel::from_bank(&bank)?;
        physical_identity(&model.baseline)?;
        let cycle = hybrid_samples(
            parameters(model.baseline),
            settings(model.baseline, model.baseline, true),
            bank.clone(),
            8.,
            true,
        )
        .map_err(|error| format!("{}: complete physical cycle: {error}", path.display()))?;
        let cycle_metrics = stats(&cycle)?;
        require(
            !model.provenance.is_empty()
                && model.provenance.iter().all(|item| {
                    item.origin != ValueOrigin::Modified && item.value == item.baseline
                }),
            "Unexpected edits in the imported baseline",
        )?;
        let meta = bank
            .engine_meta
            .as_ref()
            .ok_or("Missing verified engine metadata")?;
        let filename = path.file_name().unwrap().to_string_lossy();
        if meta.turbocharged == Some(true)
            && !selected.iter().any(|(_, name)| *name == filename)
            && turbo.is_none()
        {
            turbo = Some(path.clone());
        }
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if lightest.as_ref().is_none_or(|(_, best)| size < *best) {
            lightest = Some((path.clone(), size));
        }
        require(
            digest(&path)? == before,
            "Source archive changed during corpus audit",
        )?;
        println!(
            "{}: definition round trip and complete physical cycle verified",
            filename
        );
        corpus.push(json!({"archive":path,"sha256":before,"unchanged":true,"source":bank.source,"metadata":meta,"baseline":model.baseline,"provenance":model.provenance,"assumptions":model.assumptions,"definition_json_roundtrip":true,"physical_output_bit_exact":true,"verified_frames":1024,"complete_cycle_seconds":8,"complete_cycle_metrics":cycle_metrics}));
    }
    let mut auditions = Vec::new();
    for (slug, filename) in selected {
        let source =
            fs::canonicalize(Path::new("cars").join(filename)).map_err(|e| e.to_string())?;
        auditions.push(audition(&output.join("listening"), slug, &source)?);
    }
    auditions.push(audition(
        &output.join("listening"),
        "turbo-additional",
        &turbo.ok_or("No additional verified turbo engine in corpus")?,
    )?);
    let variant = if export {
        let (source, _) = lightest.ok_or("Missing export source")?;
        let before = digest(&source)?;
        let bank = Arc::new(Bank::load(&source, None)?);
        let baseline = AutomationModel::from_bank(&bank)?.baseline;
        let engine = edited(baseline);
        let dir = output.join("beamng-variant");
        println!(
            "Rendering uninstalled variant from smallest archive: {}",
            source.display()
        );
        bess::variant::package_named(
            &dir,
            parameters(engine),
            settings(engine, baseline, true),
            bank,
            "Unified workshop",
        )?;
        let manifest: Value = serde_json::from_slice(
            &fs::read(dir.join("manifest.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let zip = dir.join(
            manifest["zip_file"]
                .as_str()
                .ok_or("Variant manifest lacks ZIP filename")?,
        );
        let audit = audit_zip(&zip)?;
        require(
            digest(&source)? == before,
            "Variant export changed the source archive",
        )?;
        json!({"source":source,"source_sha256":before,"source_unchanged":true,"installed":false,"audit":audit,"manifest":manifest})
    } else {
        json!({"requested":false,"installed":false})
    };
    let report = json!({"version":1,"bess_version":env!("CARGO_PKG_VERSION"),"kind":"offline_common_engine_workshop_validation","sample_rate":48000,"audition_seconds":8,"audition_scenario":"shared comparison cycle: idle, acceleration, load, lift-off and recovery","corpus":corpus,"auditions":auditions,"variant":variant,"acceptance":{"source_archives_modified":false,"beamng_installed":false,"subjective_listening":false,"beamng_gameplay":false},"limits":"Offline structural and deterministic audio evidence only; source measurements absent from Automation remain estimates. Listening and BeamNG acceptance are separate."});
    fs::write(
        output.join("validation.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "Verified 12 source models and 4 reloadable projects: {}",
        output.display()
    );
    Ok(())
}
