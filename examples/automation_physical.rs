//! Reproduce raw Automation A / physical B auditions and one uninstalled add-on.
use bdsp::resample::{SincQuality, SincTable};
use bess::{
    automation_model::AutomationModel,
    bank::Bank,
    hybrid::{Hybrid, Settings},
    project::{Parameters, Project, load_project, save_project},
    realtime::DenormalGuard,
    render::write_pcm,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Read, path::Path, sync::Arc};

const RATE: u32 = 48_000;

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

fn stats(samples: &[f32]) -> Result<serde_json::Value, String> {
    if samples.is_empty() || samples.iter().any(|v| !v.is_finite() || v.abs() > 1.) {
        return Err("Empty, non-finite or out-of-PCM-range audition".into());
    }
    let mean = samples.iter().map(|&v| v as f64).sum::<f64>() / samples.len() as f64;
    let rms = (samples
        .iter()
        .map(|&v| (v as f64 - mean).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt();
    if rms < 1e-7 {
        return Err("Silent audition".into());
    }
    let peak = samples.iter().fold(0f32, |p, v| p.max(v.abs()));
    Ok(
        json!({"frames":samples.len(),"peak":peak,"rms_ac":rms,"peak_dbfs":20.*peak.log10(),"rms_dbfs":20.*rms.log10()}),
    )
}

fn audit_zip(path: &Path) -> Result<serde_json::Value, String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut paths = HashSet::new();
    let mut waves = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.enclosed_name().is_none()
            || entry.name().contains('\\')
            || !paths.insert(entry.name().to_owned())
        {
            return Err("Unsafe or duplicate ZIP path".into());
        }
        if entry.name().ends_with(".wav") {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            let mut wav =
                hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
            let spec = wav.spec();
            if spec.sample_rate != RATE
                || spec.bits_per_sample != 24
                || spec.channels != 1
                || wav.len() == 0
            {
                return Err("Unexpected exported WAV format".into());
            }
            for sample in wav.samples::<i32>() {
                sample.map_err(|e| e.to_string())?;
            }
            waves += 1;
        } else {
            std::io::copy(&mut entry, &mut std::io::sink()).map_err(|e| e.to_string())?;
        }
    }
    if waves == 0 {
        return Err("No physical WAVs in prototype".into());
    }
    Ok(
        json!({"entries":paths.len(),"pcm24_mono_48000_wavs":waves,"all_crc_read":true,"safe_unique_paths":true,"sha256":digest(path)?}),
    )
}

/// Upgrade provenance only after checking the complete source archives. Audio
/// is left byte-identical; useful when adding metadata checks to an old audit.
fn audit_existing(output: &Path) -> Result<(), String> {
    let metrics_path = output.join("metrics.json");
    let mut metrics: serde_json::Value =
        serde_json::from_slice(&fs::read(&metrics_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut prototype_fingerprint = None;
    for entry in metrics["auditions"]
        .as_array_mut()
        .ok_or("Missing audition metrics")?
    {
        let slug = entry["name"]
            .as_str()
            .ok_or("Missing audition name")?
            .to_owned();
        let archive = entry["archive"].as_str().ok_or("Missing archive")?;
        let path = fs::canonicalize(Path::new("cars").join(archive)).map_err(|e| e.to_string())?;
        if entry["source_sha256"] != digest(&path)? {
            return Err(format!("Source changed since render: {archive}"));
        }
        let bank = Bank::load(&path, None)?;
        AutomationModel::from_bank(&bank)?;
        let project_path = output.join("listening").join(format!("{slug}.bess.json"));
        let mut project = load_project(&project_path)?;
        if project.source.as_ref().is_none_or(|s| {
            s.fingerprint != bank.source.fingerprint || s.blend != bank.source.blend
        }) {
            return Err("Project audio identity changed".into());
        }
        project.source = Some(bank.source.clone());
        save_project(&project_path, &project)?;
        if load_project(&project_path)?.source != project.source {
            return Err("Project identity failed round trip".into());
        }
        entry["source_engine_fingerprint"] = json!(bank.source.engine_fingerprint);
        if slug == "i4-b5c" {
            prototype_fingerprint = Some(bank.source);
        }
    }
    let source = prototype_fingerprint.ok_or("Prototype identity missing")?;
    let dir = output.join("prototype-i4");
    let manifest_path = dir.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if manifest["source_sha256"] != digest(Path::new(&source.archive))? {
        return Err("Prototype source archive changed".into());
    }
    let prototype_project_path = dir.join("settings.bess.json");
    let mut prototype_project = load_project(&prototype_project_path)?;
    if prototype_project
        .source
        .as_ref()
        .is_none_or(|saved| saved.fingerprint != source.fingerprint || saved.blend != source.blend)
    {
        return Err("Prototype project audio identity changed".into());
    }
    prototype_project.source = Some(source.clone());
    save_project(&prototype_project_path, &prototype_project)?;
    if load_project(&prototype_project_path)?.source != prototype_project.source {
        return Err("Prototype project failed identity round trip".into());
    }
    let zip_path = dir.join(
        manifest["zip_file"]
            .as_str()
            .ok_or("Missing ZIP filename")?,
    );
    let audit = audit_zip(&zip_path)?;
    if metrics["prototype"]["sha256"] != audit["sha256"] {
        return Err("Prototype bytes changed since original audit".into());
    }
    manifest["source_engine_fingerprint"] = json!(source.engine_fingerprint);
    manifest["provenance_upgrade"] = json!(
        "Engine fingerprint added after full original source SHA256 equality check; WAV and ZIP bytes unchanged."
    );
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    metrics["prototype"] = audit;
    metrics["provenance_upgrade"] = manifest["provenance_upgrade"].clone();
    fs::write(
        &metrics_path,
        serde_json::to_vec_pretty(&metrics).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "Existing source hashes, project round trips, unchanged ZIP and CRC verified: {}",
        output.display()
    );
    Ok(())
}

fn main() -> Result<(), String> {
    let _denormals = DenormalGuard::enter();
    let output = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "output/automation-physical-20260928".into());
    if std::env::args().nth(2).as_deref() == Some("--audit-existing") {
        return audit_existing(&output);
    }
    fs::create_dir_all(output.join("listening")).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut prototype = None;
    for (slug, archive) in [
        ("i4-b5c", "bunchyearth23_b5_c.zip"),
        ("i6-genesis", "bunchyearth23_genesis_phantom.zip"),
        ("v8-advent", "bunchyearth23_advent_tc.zip"),
    ] {
        let source =
            fs::canonicalize(Path::new("cars").join(archive)).map_err(|e| e.to_string())?;
        let before = digest(&source)?;
        let bank = Arc::new(Bank::load(&source, None)?);
        let model = AutomationModel::from_bank(&bank)?;
        let idle = model.scratch.idle_rpm.clamp(bank.min_rpm, bank.max_rpm);
        let high = 3500f32
            .max(idle + 500.)
            .min(bank.max_rpm)
            .min(model.scratch.redline_rpm - 200.);
        let mut p = Parameters {
            cylinders: model.scratch.design.cylinders,
            rpm: idle,
            load: 0.08,
            volume: 0.8,
            ..Default::default()
        };
        let h = Settings {
            enhanced: true,
            physical: true,
            level_match: false,
            ..Default::default()
        };
        let project = Project {
            version: 3,
            parameters: p,
            hybrid: h,
            source: Some(bank.source.clone()),
            driving: Default::default(),
            profile_name: "Physical".into(),
            scratch: None,
        };
        let project_path = output.join("listening").join(format!("{slug}.bess.json"));
        save_project(&project_path, &project)?;
        let loaded = load_project(&project_path)?;
        if loaded.version != 3
            || loaded.source != project.source
            || loaded.parameters != p
            || loaded.hybrid.physical_sound != h.physical_sound
            || !loaded.hybrid.physical
        {
            return Err("Project round trip changed physical parameters".into());
        }
        let mut voice = Hybrid::new(RATE, p, h, Some(bank.clone()));
        if let Some(error) = voice.initialization_error() {
            return Err(error.to_owned());
        }
        let sinc = SincTable::for_quality(SincQuality::Realtime);
        let mut cycle = 0f64;
        for _ in 0..RATE * 2 {
            voice.next(true);
            cycle += voice.rpm() as f64 / (120. * RATE as f64);
        }
        let mut a = Vec::with_capacity(RATE as usize * 8);
        let mut b = Vec::with_capacity(a.capacity());
        for frame in 0..RATE * 8 {
            let t = frame as f32 / RATE as f32;
            (p.rpm, p.load) = if t < 2. {
                (idle, 0.08)
            } else if t < 4. {
                (idle + (high - idle) * (t - 2.) / 2., 0.75)
            } else if t < 5. {
                (high, 0.75)
            } else if t < 7. {
                (high - (high - idle) * (t - 5.) / 2., 0.)
            } else {
                (idle, 0.08)
            };
            voice.set(p, h);
            b.push(voice.next(true));
            cycle += voice.rpm() as f64 / (120. * RATE as f64);
            a.push(bank.read_original(cycle, voice.rpm(), voice.load(), RATE as f32, &sinc) * 0.8);
        }
        if voice.failed() {
            return Err(format!("Physical model failed: {slug}"));
        }
        let a_stats = stats(&a)?;
        let b_stats = stats(&b)?;
        write_pcm(
            &output
                .join("listening")
                .join(format!("{slug}-A-original.wav")),
            &a,
        )?;
        write_pcm(
            &output
                .join("listening")
                .join(format!("{slug}-B-physical.wav")),
            &b,
        )?;
        println!("{slug}: A={a_stats} B={b_stats}");
        if digest(&source)? != before {
            return Err(format!("Source archive changed: {archive}"));
        }
        results.push(json!({"name":slug,"archive":archive,"source_sha256":before,"source_unchanged":true,"cylinders":p.cylinders,"idle_rpm":idle,"high_rpm":high,"assumptions":model.assumptions,"A":a_stats,"B":b_stats,"project_roundtrip":true}));
        if prototype.is_none() {
            p.rpm = idle;
            p.load = 0.08;
            prototype = Some((p, h, bank));
        }
    }
    let (p, h, bank) = prototype.ok_or("Missing prototype source")?;
    let prototype_path = output.join("prototype-i4");
    let source_path = Path::new(&bank.source.archive);
    let source_before = digest(source_path)?;
    println!("Rendering physical I4 prototype bank...");
    bess::variant::package_named(&prototype_path, p, h, bank.clone(), "Physical")?;
    let zip_path = fs::read_dir(&prototype_path)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "zip"))
        .ok_or("Prototype ZIP missing")?;
    let zip_audit = audit_zip(&zip_path)?;
    if digest(source_path)? != source_before {
        return Err("Source archive changed".into());
    }
    fs::write(
        output.join("metrics.json"),
        serde_json::to_vec_pretty(
            &json!({"auditions":results,"prototype":zip_audit,"installed":false}),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(output.join("README.md"), "# Automation : A original / B physique\n\nTrois moteurs : I4 B5c turbo, I6 Genesis Phantom, V8 Advent. Projets BESS v3 rechargeables dans listening/. WAV mono PCM24 48 kHz, volume 0,8, sans normalisation entre moteurs. A lit les échantillons originaux avec interpolation au régime demandé ; aucune synthèse ne remplace A. B utilise uniquement le moteur physique avec limiteur d'écoute.\n\nMême commande sur 8 secondes : ralenti 0–2 s (8 %), montée vers 3500 tr/min 2–4 s (75 %), palier 4–5 s, descente 5–7 s (0 %), ralenti 7–8 s. Régime imposé ; deux secondes de chauffe avant enregistrement. Les réponses sont lissées comme dans l'application. Les projets s'ouvrent au ralenti ; l'exemple reproduit la trajectoire.\n\nprototype-i4/ contient un add-on séparé, non installé. Ses stems sont physiques, calibrés au niveau source avec marge PCM ; le limiteur d'écoute n'est pas imprimé dans les stems. Vérifications CRC, chemins uniques/sûrs, PCM24 48 kHz, projets rechargés et archives source inchangées : metrics.json.\n\nLes dimensions non publiées, l'ordre d'allumage et les détails acoustiques restent des estimations consignées dans les manifests. Ces contrôles ne prouvent ni l'identité sonore subjective ni le comportement en jeu.\n").map_err(|e|e.to_string())?;
    println!("Completed: {}", output.display());
    Ok(())
}
