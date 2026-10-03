//! Reproducible, fixed-gain mechanical review through the public physical/app paths.
//! Run from the repository root: mechanical_review <new-output-directory>.
use bess::{
    automation_model::AutomationModel,
    automation_voice::AutomationVoice,
    bank::Bank,
    bench::{AuditionMix, BeamNgCamera, Bench},
    drive::Controls,
    hybrid::Settings,
    physical::engine::Commands,
    project::Parameters,
    realtime::{DenormalGuard, RenderEngine},
    render::write_pcm,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

const RATE: u32 = 48_000;
const WARMUP: usize = RATE as usize;
const CAPTURE: usize = 2 * RATE as usize;
const VOLUME: f32 = 0.8;

fn digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65_536];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(format!("{:x}", hash.finalize()));
        }
        hash.update(&bytes[..n]);
    }
}

fn stats(samples: &[f32]) -> Result<Value, String> {
    if samples.len() != CAPTURE || samples.iter().any(|s| !s.is_finite()) {
        return Err("Invalid length or non-finite rendered audio".into());
    }
    let n = samples.len() as f64;
    let mean = samples.iter().map(|&x| f64::from(x)).sum::<f64>() / n;
    let rms = (samples.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / n).sqrt();
    let rms_ac = (samples
        .iter()
        .map(|&x| (f64::from(x) - mean).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    let peak = samples.iter().map(|x| x.abs()).fold(0f32, f32::max);
    let mut hash = Sha256::new();
    for x in samples {
        hash.update(x.to_bits().to_le_bytes());
    }
    Ok(json!({
        "frames":samples.len(), "mean":mean, "rms":rms, "rms_ac":rms_ac,
        "peak":peak, "rms_ac_dbfs":20. * rms_ac.max(1e-20).log10(),
        "peak_dbfs":20. * f64::from(peak).max(1e-20).log10(),
        "float_le_sha256":format!("{:x}",hash.finalize()),
        "samples_at_or_above_full_scale":samples.iter().filter(|x|x.abs() >= 1.).count(),
    }))
}

fn write_audio(output: &Path, name: &str, samples: &[f32], raw: bool) -> Result<Value, String> {
    let metrics = stats(samples)?;
    let path = output.join(format!("{name}.wav"));
    if raw {
        let mut writer = hound::WavWriter::create(
            &path,
            hound::WavSpec {
                channels: 1,
                sample_rate: RATE,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .map_err(|e| e.to_string())?;
        for &sample in samples {
            writer.write_sample(sample).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
    } else {
        if samples.iter().any(|x| x.abs() >= 1.) {
            return Err(format!("PCM listening output exceeds full scale: {name}"));
        }
        write_pcm(&path, samples)?;
    }
    Ok(json!({
        "file":path.file_name().unwrap().to_string_lossy(), "sha256":digest(&path)?,
        "format":if raw {"IEEE float32 mono 48000 Hz; unscaled diagnostic, not a listening file"}
                  else {"PCM24 mono 48000 Hz; application output"},
        "stats":metrics,
    }))
}

fn review(output: &Path, slug: &str, expected_cylinders: u32) -> Result<Value, String> {
    let source = PathBuf::from("cars").join(format!("bunchyearth23_{slug}.zip"));
    let source_before = digest(&source)?;
    let bank = Arc::new(Bank::load(&source, None)?);
    let model = AutomationModel::from_bank(&bank)?;
    let definition = model.baseline;
    if definition.design.cylinders != expected_cylinders {
        return Err(format!("Unexpected cylinder count for {slug}"));
    }
    let settings = Settings {
        engine: Some(definition),
        engine_baseline: Some(definition),
        physical_sound: definition.sound,
        enhanced: true,
        level_match: false,
        ..Default::default()
    };
    settings.validate()?;
    let low = definition.idle_rpm.max(bank.min_rpm);
    let high = definition.redline_rpm.min(bank.max_rpm);
    if low > high {
        return Err(format!("No common physical/source RPM range for {slug}"));
    }
    let mut points = Vec::new();
    for (label, requested_rpm, load) in [
        ("idle", definition.idle_rpm, 0.1),
        ("3000", 3000., 0.5),
        ("6000", 6000., 0.8),
    ] {
        let rpm = requested_rpm.clamp(low, high);
        let p = Parameters {
            cylinders: definition.design.cylinders,
            rpm,
            load,
            volume: VOLUME,
            // Pin the historical comparison mix even when product defaults change.
            mechanical: 0.12,
            ..Default::default()
        };
        p.validate()?;
        let prefix = format!("{slug}-{label}");
        let mut files = Vec::new();

        // Direct public stem output: no bank/listening gain, no audition limiter.
        // All three are retained so before/after hashes can prove layer isolation.
        let mut voice = AutomationVoice::from_settings(RATE, &bank, settings)?;
        let commands = Commands {
            imposed_rpm: Some(f64::from(rpm)),
            throttle: f64::from(load),
            overrun: f64::from(settings.fuel_cut),
            ..Default::default()
        };
        let mut raw: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::with_capacity(CAPTURE));
        for frame in 0..WARMUP + CAPTURE {
            let s = voice.next_commands(commands);
            if voice.failed() {
                return Err(format!(
                    "Raw physical voice failed: {prefix}, frame {frame}"
                ));
            }
            if frame >= WARMUP {
                for (samples, value) in raw.iter_mut().zip([s.mechanical, s.intake, s.exhaust]) {
                    samples.push(value);
                }
            }
        }
        for (name, samples) in ["mechanical", "intake", "exhaust"].into_iter().zip(&raw) {
            files.push(write_audio(
                output,
                &format!("{prefix}-raw-{name}"),
                samples,
                true,
            )?);
        }

        // Preserve application protection and mixing exactly. No separate gain,
        // normalization or fade is applied by this diagnostic after the bench.
        for (name, intake, mechanical, exhaust, mix, camera) in [
            (
                "mechanical-only",
                0.,
                p.mechanical,
                0.,
                AuditionMix::Live,
                BeamNgCamera::Hood,
            ),
            (
                "engine-mix",
                p.intake,
                p.mechanical,
                0.,
                AuditionMix::Live,
                BeamNgCamera::Hood,
            ),
            (
                "full-live",
                p.intake,
                p.mechanical,
                p.exhaust,
                AuditionMix::Live,
                BeamNgCamera::Hood,
            ),
            (
                "full-preview-hood",
                p.intake,
                p.mechanical,
                p.exhaust,
                AuditionMix::BeamNgTwoEmitter,
                BeamNgCamera::Hood,
            ),
            (
                "full-preview-cockpit",
                p.intake,
                p.mechanical,
                p.exhaust,
                AuditionMix::BeamNgTwoEmitter,
                BeamNgCamera::Cockpit,
            ),
            (
                "new-default-full-live",
                0.25,
                0.,
                p.exhaust,
                AuditionMix::Live,
                BeamNgCamera::Hood,
            ),
            (
                "new-default-preview-hood",
                0.25,
                0.,
                p.exhaust,
                AuditionMix::BeamNgTwoEmitter,
                BeamNgCamera::Hood,
            ),
            (
                "new-default-preview-cockpit",
                0.25,
                0.,
                p.exhaust,
                AuditionMix::BeamNgTwoEmitter,
                BeamNgCamera::Cockpit,
            ),
        ] {
            let parameters = Parameters {
                intake,
                mechanical,
                exhaust,
                ..p
            };
            let mut bench = Bench::new(
                RATE,
                parameters,
                settings,
                Controls::default(),
                Some(bank.clone()),
            );
            if let Some(error) = bench.initialization_error() {
                return Err(error.to_owned());
            }
            bench.set_audition_mix(mix);
            bench.set_beamng_camera(camera);
            let mut engine = RenderEngine::native(bench, RATE);
            let mut samples = Vec::with_capacity(CAPTURE);
            for frame in 0..WARMUP + CAPTURE {
                let sample = engine.next_sample(true);
                if engine.bench.failed() {
                    return Err(format!(
                        "Application voice failed: {prefix}/{name}, frame {frame}"
                    ));
                }
                if frame >= WARMUP {
                    samples.push(sample);
                }
            }
            let mut artifact = write_audio(output, &format!("{prefix}-{name}"), &samples, false)?;
            artifact["parameters"] = json!(parameters);
            artifact["path"] = json!(format!(
                "Bench + RenderEngine native; {mix:?}; {camera:?}; room Off"
            ));
            files.push(artifact);
        }
        points.push(
            json!({"label":label,"requested_rpm":requested_rpm,"actual_rpm":rpm,
            "rpm_clamped":rpm != requested_rpm,"load":load,"files":files}),
        );
        println!(
            "{prefix}: {rpm:.0} rpm, load {load}, raw stems and eight application mixes saved"
        );
    }
    let source_after = digest(&source)?;
    if source_before != source_after {
        return Err(format!("Source archive changed: {}", source.display()));
    }
    Ok(json!({
        "slug":slug,"source":bank.source,"source_sha256_before":source_before,
        "source_sha256_after":source_after,"source_unchanged":true,
        "definition":definition,
        "definition_json_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&definition).map_err(|e|e.to_string())?)),
        "settings":settings,"source_bank_gain":bank.gain,
        "source_rpm_range":[bank.min_rpm,bank.max_rpm],
        "physical_rpm_range":[definition.idle_rpm,definition.redline_rpm],
        "points":points,
    }))
}

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or("Usage: mechanical_review <new-output-directory>")?,
    );
    if args.next().is_some() {
        return Err("Usage: mechanical_review <new-output-directory>".into());
    }
    if output.exists() {
        return Err("Choose a new output directory; existing evidence is never overwritten".into());
    }
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let _denormals = DenormalGuard::enter();
    let mut corpus = Vec::new();
    // The twelve local Automation archives have I4/I6/V8 engines, no V12.
    for (slug, cylinders) in [("b5_c", 4), ("genesis_phantom", 6), ("advent_tc", 8)] {
        corpus.push(review(&output, slug, cylinders)?);
    }
    let report = json!({
        "version":1,"bess_version":env!("CARGO_PKG_VERSION"),
        "sample_rate":RATE,"warmup_seconds":1,"capture_seconds":2,
        "fixed_listening_volume":VOLUME,"room":"Off","added_normalization":false,
        "notes":[
            "Historical comparison mixes pin mechanical 0.12, intake 0.3 and exhaust 0.8 except explicitly isolated layers. New-default mixes use fresh import levels: mechanical 0, intake 0.25, exhaust 0.8. Engine gain and idle gain stay 1.",
            "Live level_match is disabled. Two-emitter preview retains its real application level followers, camera filters and limiter; it is not an unnormalized raw sum.",
            "Raw IEEE-float stem WAVs have unity diagnostic gain and no limiter. They are distinct from PCM24 listening files and may exceed full scale.",
            "Float sample hashes precede PCM quantization. Compare raw intake/exhaust hashes across before/after runs to establish that those layers stayed unchanged.",
            "One second warmup is a repeatable initial condition, not a claim of thermal equilibrium. Steady controls use the common physical engine, without source recording excitation.",
            "No V12 Automation archive exists in this local corpus; I4, Genesis I6 and V8 are included without fabricated source metadata."
        ],"corpus":corpus,
    });
    fs::write(
        output.join("review.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Review complete: {}", output.join("review.json").display());
    Ok(())
}
