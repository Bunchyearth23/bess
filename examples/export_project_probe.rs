//! Reproduce an exported project's settings and record bounded export progress.
//! Usage: export_project_probe PROJECT NEW_OUTPUT [--source ZIP] [--workers 1..8]
//!        [--baseline ZIP] [--cancel-after SECONDS]
//!        [--format replacement|complete-variant]
//! Cancellation is timed from first observing the Rendering phase.
use bess::{
    bank::Bank,
    export,
    export_job::{ExportJob, ExportStage},
    project,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

struct Arguments {
    project: PathBuf,
    output: PathBuf,
    source: Option<PathBuf>,
    baseline: Option<PathBuf>,
    workers: usize,
    cancel_after: Option<f64>,
    complete_variant: bool,
}

fn arguments() -> Result<Arguments, String> {
    let mut args = std::env::args_os().skip(1);
    let project = PathBuf::from(args.next().ok_or("PROJECT is required")?);
    let output = PathBuf::from(args.next().ok_or("NEW_OUTPUT is required")?);
    let mut result = Arguments {
        project,
        output,
        source: None,
        baseline: None,
        workers: 8,
        cancel_after: None,
        complete_variant: false,
    };
    while let Some(flag) = args.next() {
        let value = args.next().ok_or("Option requires a value")?;
        match flag.to_str() {
            Some("--source") => result.source = Some(PathBuf::from(value)),
            Some("--baseline") => result.baseline = Some(PathBuf::from(value)),
            Some("--format") => {
                result.complete_variant = match value.to_str() {
                    Some("replacement") => false,
                    Some("complete-variant") => true,
                    _ => return Err("Format must be replacement or complete-variant".into()),
                };
            }
            Some("--workers") => {
                result.workers = value
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "Invalid workers")?;
                if !(1..=8).contains(&result.workers) {
                    return Err("Worker count must be between 1 and 8".into());
                }
            }
            Some("--cancel-after") => {
                let seconds: f64 = value
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "Invalid seconds")?;
                if !seconds.is_finite() || seconds <= 0. {
                    return Err("Cancellation time must be finite and positive".into());
                }
                result.cancel_after = Some(seconds);
            }
            _ => return Err(format!("Unknown option {}", flag.to_string_lossy())),
        }
    }
    if result.output.exists() {
        return Err("Output must be a new directory".into());
    }
    Ok(result)
}

fn entry_hashes(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut zip = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut hashes = BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let count = entry.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        if hashes
            .insert(entry.name().to_owned(), format!("{:x}", hash.finalize()))
            .is_some()
        {
            return Err("Archive contains duplicate members".into());
        }
    }
    Ok(hashes)
}

fn baseline_comparison(reference: &Path, candidate: &Path) -> Result<Value, String> {
    let before = entry_hashes(reference)?;
    let after = entry_hashes(candidate)?;
    let mut wavs = 0usize;
    let mut unchanged = 0usize;
    let mut differences = Vec::new();
    for name in before
        .keys()
        .chain(after.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        if name.as_str() == bess::babm_exchange::MARKER_PATH {
            continue; // The export timestamp and source filename may differ.
        }
        if before.get(name) != after.get(name) {
            differences.push(name.clone());
        } else if name.to_ascii_lowercase().ends_with(".wav") {
            wavs += 1;
        } else {
            unchanged += 1;
        }
    }
    Ok(json!({
        "reference": reference,
        "candidate": candidate,
        "identical_wavs": wavs,
        "unchanged_other_members": unchanged,
        "differences": differences,
        "passed": wavs > 0 && differences.is_empty(),
    }))
}

fn run() -> Result<(), String> {
    let args = arguments()?;
    let report_path = args.output.with_extension("probe.json");
    let mut report_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&report_path)
        .map_err(|e| format!("Cannot reserve probe report {}: {e}", report_path.display()))?;
    let setup = Instant::now();
    let project = project::load_project(&args.project)?;
    let source = project
        .source
        .as_ref()
        .ok_or("Project has no imported source")?;
    let source_path = args
        .source
        .clone()
        .unwrap_or_else(|| PathBuf::from(&source.archive));
    let bank = Arc::new(Bank::load(&source_path, Some(&source.blend))?);
    if source.fingerprint != bank.source.fingerprint
        || source.blend != bank.source.blend
        || (source.engine_fingerprint.is_some()
            && source.engine_fingerprint != bank.source.engine_fingerprint)
    {
        return Err("Copied source identity does not match the saved project".into());
    }
    let setup_seconds = setup.elapsed().as_secs_f64();
    let job = ExportJob::with_worker_limit(args.workers);
    let worker_job = job.clone();
    let output = args.output.clone();
    let complete_variant = args.complete_variant;
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    let worker = std::thread::spawn(move || {
        let result = if complete_variant {
            bess::variant::package_complete_with_job(
                &output,
                project.parameters,
                project.hybrid.for_beamng_export(),
                bank,
                &project.profile_name,
                &worker_job,
            )
        } else {
            export::package_with_job(
                &output,
                project.parameters,
                project.hybrid.for_beamng_export(),
                bank,
                &worker_job,
            )
        };
        let _ = tx.send(result);
    });
    let mut snapshots = Vec::new();
    let mut previous = None;
    let mut cancellation_sent = false;
    let mut render_started = None;
    let mut cancellation_started = None;
    let mut cancellation_stage = None;
    let result = loop {
        let progress = job.snapshot();
        if progress.stage == ExportStage::Rendering && render_started.is_none() {
            render_started = Some(Instant::now());
        }
        let key = (
            format!("{:?}", progress.stage),
            progress.completed,
            progress.total,
            progress.detail.clone(),
        );
        if previous.as_ref() != Some(&key) {
            let snapshot = json!({
                "stage": key.0, "completed": progress.completed, "total": progress.total,
                "rpm": progress.current_rpm, "load": progress.current_load,
                "workers": progress.workers, "elapsed_seconds": progress.elapsed_seconds,
                "estimated_remaining_seconds": progress.estimated_remaining_seconds,
                "fraction": progress.fraction, "detail": progress.detail,
                "observed_seconds": started.elapsed().as_secs_f64(),
            });
            println!("{snapshot}");
            snapshots.push(snapshot);
            previous = Some(key);
        }
        if !cancellation_sent
            && progress.stage == ExportStage::Rendering
            && args.cancel_after.is_some_and(|seconds| {
                render_started
                    .is_some_and(|start: Instant| start.elapsed().as_secs_f64() >= seconds)
            })
        {
            cancellation_sent = true;
            cancellation_started = Some(Instant::now());
            cancellation_stage = Some(format!("{:?}", progress.stage));
            job.cancel();
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("Export worker disconnected".into());
            }
        }
    };
    let worker_joined = worker.join().is_ok();
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let cancellation_response_seconds =
        cancellation_started.map(|start| start.elapsed().as_secs_f64());
    let final_progress = job.snapshot();
    let comparison = if result.is_ok() {
        let manifest: Value = serde_json::from_slice(
            &std::fs::read(args.output.join("manifest.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let zip_file = manifest["zip_file"]
            .as_str()
            .filter(|name| {
                Path::new(name).file_name().and_then(|name| name.to_str()) == Some(*name)
                    && name.ends_with(".zip")
            })
            .ok_or("Output manifest has no safe ZIP filename")?;
        let zip_path = args.output.join(zip_file);
        args.baseline
            .as_deref()
            .map(|reference| baseline_comparison(reference, &zip_path))
            .transpose()?
    } else {
        None
    };
    let cancelled_cleanly = cancellation_sent
        && result.is_err()
        && final_progress.stage == ExportStage::Cancelled
        && !args.output.exists();
    let comparison_passed = comparison
        .as_ref()
        .is_none_or(|value| value["passed"] == true);
    let passed = worker_joined
        && if args.cancel_after.is_some() {
            cancelled_cleanly
        } else {
            result.is_ok() && comparison_passed
        };
    let report = json!({
        "project": args.project, "source": source_path, "output": args.output,
        "format": if args.complete_variant { "complete-variant" } else { "replacement" },
        "requested_workers": args.workers, "setup_seconds": setup_seconds,
        "export_seconds": elapsed_seconds, "success": result.is_ok(), "result": result,
        "snapshots": snapshots, "final_stage": format!("{:?}", final_progress.stage),
        "final_completed": final_progress.completed, "final_total": final_progress.total,
        "cancellation_sent": cancellation_sent, "cancelled_cleanly": cancelled_cleanly,
        "cancellation_stage": cancellation_stage,
        "cancellation_response_seconds": cancellation_response_seconds,
        "comparison": comparison, "passed": passed,
    });
    report_file
        .write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    report_file.sync_all().map_err(|e| e.to_string())?;
    println!(
        "{}",
        json!({"report":report_path,"passed":passed,"export_seconds":elapsed_seconds})
    );
    if passed {
        Ok(())
    } else {
        Err("Export probe did not pass; see its report".into())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
