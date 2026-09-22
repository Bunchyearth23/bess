//! Reproducible real-archive baseline using the same importer/render path as BESS.
use bess::{bank::Bank, beamng, drive, hybrid::Settings, project, render};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, sync::Arc};

fn content_hash(path: &Path, blend: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut bytes = Vec::new();
    zip.by_name(blend)?
        .take(1_000_001)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 1_000_000 {
        return Err("Blend too large".into());
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    let mut hash = Sha256::new();
    // Ignore asset names, retaining layer, RPM and exact WAV bytes.
    for (layer, rows) in value["samples"]
        .as_array()
        .ok_or("Missing layers")?
        .iter()
        .enumerate()
    {
        let mut rows = rows.as_array().ok_or("Missing samples")?.clone();
        rows.sort_by(|a, b| {
            a[1].as_f64()
                .unwrap_or_default()
                .total_cmp(&b[1].as_f64().unwrap_or_default())
        });
        for row in rows {
            hash.update((layer as u64).to_le_bytes());
            hash.update(row[1].as_f64().ok_or("Missing RPM")?.to_le_bytes());
            bytes.clear();
            zip.by_name(row[0].as_str().ok_or("Missing name")?)?
                .take(16_000_001)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 16_000_000 {
                return Err("WAV too large".into());
            }
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn metrics(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    if spec.sample_rate != 48000 || spec.channels != 1 || spec.bits_per_sample != 24 {
        return Err("Unexpected output format".into());
    }
    let data = reader.samples::<i32>().collect::<Result<Vec<_>, _>>()?;
    if data.len() != 16 * 48000 {
        return Err("Unexpected duration".into());
    }
    let peak = data
        .iter()
        .map(|s| (*s as f64 / 8388607.).abs())
        .fold(0., f64::max);
    let rms = (data
        .iter()
        .map(|s| (*s as f64 / 8388607.).powi(2))
        .sum::<f64>()
        / data.len() as f64)
        .sqrt();
    if rms < 1e-6 || peak > 0.950001 {
        return Err("Silent or clipped output".into());
    }
    let step = data
        .windows(2)
        .map(|s| (s[1] as f64 - s[0] as f64).abs() / 8388607.)
        .fold(0., f64::max);
    Ok(json!({"frames":data.len(), "rate":spec.sample_rate,"rms":rms,"peak":peak,"max_step":step}))
}

fn audit(path: &Path, out: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let bank = Arc::new(Bank::load(path, None)?);
    let vehicle = beamng::inspect(path)?;
    let audio_hash = content_hash(path, &bank.source.blend)?;
    let params = project::Parameters {
        rpm: vehicle
            .idle_rpm
            .unwrap_or(bank.min_rpm)
            .clamp(bank.min_rpm, bank.max_rpm),
        load: 0.12,
        brightness: 10000.,
        exhaust: 1.,
        intake: 0.25,
        mechanical: 0.12,
        ..Default::default()
    };
    let settings = Settings::calibrated(&bank);
    render::comparison(out, params, settings, bank.clone())?;
    let driving = drive::Controls {
        mode: drive::Mode::Cycle,
        ..Default::default()
    };
    for (name, enhanced) in [("source", false), ("bess", true)] {
        let file = out.join(format!("{name}.bess.json"));
        project::save_project(
            &file,
            &project::Project {
                version: 3,
                parameters: params,
                hybrid: Settings {
                    enhanced,
                    ..settings
                },
                source: Some(bank.source.clone()),
                driving,
            },
        )?;
        let reopened = project::load_project(&file)?;
        if reopened.source.as_ref() != Some(&bank.source) {
            return Err("Project provenance mismatch".into());
        }
    }
    let a = metrics(&out.join("01-source-automation.wav"))?;
    let b = metrics(&out.join("02-bess-enhanced.wav"))?;
    let ratio = a["rms"].as_f64().unwrap() / b["rms"].as_f64().unwrap();
    if (20. * ratio.log10()).abs() > 0.001 {
        return Err("RMS comparison mismatch".into());
    }
    Ok(
        json!({"status":"ok","vehicle":vehicle.name,"source":bank.source,
        "audio_content_sha256":audio_hash,"min_rpm":bank.min_rpm,"max_rpm":bank.max_rpm,
        "layers":bank.layers.iter().map(|l|l.iter().map(|s|json!({"rpm":s.rpm,"rate":s.rate,"rms":s.rms,"peak":s.peak,"period":s.period})).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "metadata":{"idle_rpm":vehicle.idle_rpm,"damage_max_rpm":vehicle.max_rpm,"cylinders":vehicle.cylinders,"engine_files":vehicle.engine_files,"blend_files":vehicle.blend_files,"automation_engine":bank.engine_meta},
        "source_metrics":a,"bess_metrics":b,"parameters":params,"settings":settings,"calibration":bank.character(),"driving":driving}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: corpus CARS OUTPUT (new directory)".into());
    }
    let out = Path::new(&args[2]);
    // Never overwrite a baseline used to assess later synthesis revisions.
    fs::create_dir(out)?;
    let mut paths = fs::read_dir(&args[1])?
        .map(|e| e.map(|x| x.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")));
    paths.sort();
    let mut rows = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        println!("{}/{} {}", i + 1, paths.len(), path.display());
        let folder = format!("vehicle-{:02}", i + 1);
        let mut result = match audit(path, &out.join(&folder)) {
            Ok(v) => v,
            Err(e) => json!({"status":"error","error":e.to_string()}),
        };
        result["archive"] = json!(path.file_name().unwrap().to_string_lossy());
        result["folder"] = json!(folder);
        rows.push(result);
        fs::write(
            out.join("corpus.json"),
            serde_json::to_vec_pretty(
                &json!({"version":env!("CARGO_PKG_VERSION"),"vehicles":rows}),
            )?,
        )?;
    }
    let errors = rows.iter().filter(|r| r["status"] != "ok").count();
    println!(
        "{} imports, {} errors. Report: {}",
        rows.len(),
        errors,
        out.join("corpus.json").display()
    );
    if rows.is_empty() || errors > 0 {
        return Err("Corpus incomplete; see report".into());
    }
    Ok(())
}
