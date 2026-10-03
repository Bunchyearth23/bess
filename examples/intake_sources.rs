//! Attribute raw intake to acoustic pipe pulsations and flow/compressor noise.
//! Run from the repository root: intake_sources <new-output-directory> [air-noise].
use bdsp::filters::{PolyphaseDecimator, generate_lowpass_taps};
use bess::{
    automation_model::AutomationModel,
    bank::Bank,
    engine_definition::EngineDefinition,
    hybrid::Settings,
    physical::engine::{Commands, Engine},
    realtime::DenormalGuard,
    scratch::SoundTuning,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const RATE: u32 = 48_000;
const WARMUP: usize = RATE as usize;
const CAPTURE: usize = 2 * RATE as usize;
const NAMES: [&str; 3] = ["pulse", "flow", "intake-total"];

fn digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(format!("{:x}", hash.finalize()));
        }
        hash.update(&buffer[..count]);
    }
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&x| f64::from(x).powi(2)).sum()
}

fn save(output: &Path, name: &str, samples: &[f32]) -> Result<Value, String> {
    if samples.len() != CAPTURE || samples.iter().any(|x| !x.is_finite()) {
        return Err(format!("Invalid diagnostic samples: {name}"));
    }
    let path = output.join(format!("{name}.wav"));
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
    let mut sample_hash = Sha256::new();
    for &sample in samples {
        writer.write_sample(sample).map_err(|e| e.to_string())?;
        sample_hash.update(sample.to_bits().to_le_bytes());
    }
    writer.finalize().map_err(|e| e.to_string())?;
    let mean = samples.iter().map(|&x| f64::from(x)).sum::<f64>() / CAPTURE as f64;
    let rms = (energy(samples) / CAPTURE as f64).sqrt();
    let rms_ac = (samples
        .iter()
        .map(|&x| (f64::from(x) - mean).powi(2))
        .sum::<f64>()
        / CAPTURE as f64)
        .sqrt();
    Ok(json!({
        "file":path.file_name().unwrap().to_string_lossy(),
        "format":"IEEE float32 mono 48000 Hz; raw diagnostic",
        "frames":CAPTURE,"bytes":fs::metadata(&path).map_err(|e|e.to_string())?.len(),
        "gain":1,"mean":mean,"rms":rms,"rms_ac":rms_ac,
        "peak":samples.iter().map(|x|x.abs()).fold(0_f32,f32::max),
        "sha256":digest(&path)?,"float_le_sha256":format!("{:x}",sample_hash.finalize())
    }))
}

fn review(
    output: &Path,
    slug: &str,
    cylinders: u32,
    air_noise: Option<f32>,
) -> Result<Value, String> {
    let source = PathBuf::from("cars").join(format!("bunchyearth23_{slug}.zip"));
    let source_before = digest(&source)?;
    let bank = Bank::load(&source, None)?;
    let baseline = AutomationModel::from_bank(&bank)?.baseline;
    if baseline.design.cylinders != cylinders {
        return Err(format!("Unexpected cylinder count for {slug}"));
    }
    let mut scratch = baseline.to_scratch();
    if let Some(gain) = air_noise {
        scratch.sound.intake_air_noise = gain;
    }
    scratch.validate()?;
    // The diagnostic taps are before Tone. Only the airflow source gain may
    // differ from defaults, so their sum remains comparable to Sample::intake.
    let neutral_tone = SoundTuning {
        intake_air_noise: scratch.sound.intake_air_noise,
        ..Default::default()
    };
    if scratch.sound != neutral_tone {
        return Err(format!(
            "Intake attribution requires default sound controls except air noise: {slug}"
        ));
    }
    let definition = EngineDefinition::from_scratch(&scratch);
    let settings = Settings {
        engine: Some(definition),
        engine_baseline: Some(baseline),
        physical_sound: definition.sound,
        enhanced: true,
        level_match: false,
        ..Default::default()
    };
    let synthesis_rate = scratch.synthesis_rate(RATE);
    let physical_steps = if synthesis_rate == RATE { 1 } else { 2 };
    let low = scratch.idle_rpm.max(bank.min_rpm);
    let high = scratch.redline_rpm.min(bank.max_rpm);
    if low > high {
        return Err(format!("No common physical/source RPM range for {slug}"));
    }
    let mut points = Vec::new();
    for (label, requested_rpm, throttle) in [
        ("idle", scratch.idle_rpm, 0.1_f32),
        ("3000", 3000., 0.5),
        ("6000", 6000., 0.8),
    ] {
        let rpm = requested_rpm.clamp(low, high);
        let commands = Commands {
            imposed_rpm: Some(f64::from(rpm)),
            throttle: f64::from(throttle),
            overrun: f64::from(settings.fuel_cut),
            ..Default::default()
        };
        let mut engine = Engine::new(&scratch, synthesis_rate)?;
        // Match AutomationVoice exactly, with independent identical FIRs for
        // the two diagnostic sources and the engine's intake total.
        let mut decimators: Option<[PolyphaseDecimator; 3]> = (physical_steps == 2).then(|| {
            std::array::from_fn(|_| {
                PolyphaseDecimator::new(
                    generate_lowpass_taps(synthesis_rate as f32, RATE as f32 * 0.45, 95),
                    2,
                )
            })
        });
        let mut stems: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::with_capacity(CAPTURE));
        for frame in 0..WARMUP + CAPTURE {
            let mut physical = [[0_f32; 3]; 2];
            for values in physical.iter_mut().take(physical_steps) {
                let sample = engine.next(commands);
                let [pulse, flow] = engine.intake_sources();
                *values = [pulse, flow, sample.intake];
                if engine.failed() || values.iter().any(|x| !x.is_finite()) {
                    return Err(format!(
                        "Non-finite or failed engine: {slug}/{label}, frame {frame}"
                    ));
                }
            }
            let inputs: [[f32; 2]; 3] = std::array::from_fn(|i| [physical[0][i], physical[1][i]]);
            let mut values = inputs.map(|input| input[0]);
            if let Some(decimators) = &mut decimators {
                for ((decimator, input), value) in
                    decimators.iter_mut().zip(inputs).zip(&mut values)
                {
                    let mut output = [0_f32];
                    decimator.process_block(&input, &mut output);
                    *value = output[0];
                }
            }
            if frame >= WARMUP {
                for (stem, value) in stems.iter_mut().zip(values) {
                    stem.push(value);
                }
            }
        }
        let total_energy = energy(&stems[2]);
        if total_energy <= 1e-30 {
            return Err(format!("Silent intake observation: {slug}/{label}"));
        }
        let mut error_energy = 0_f64;
        let mut max_error = 0_f64;
        for ((&pulse, &flow), &total) in stems[0].iter().zip(&stems[1]).zip(&stems[2]) {
            let sum = f64::from(pulse) + f64::from(flow);
            let error = sum - f64::from(total);
            error_energy += error * error;
            max_error = max_error.max(error.abs());
        }
        let relative_rms_error = (error_energy / total_energy).sqrt();
        if !relative_rms_error.is_finite() || relative_rms_error > 1e-5 {
            return Err(format!(
                "Intake reconstruction failed: {slug}/{label}, relative RMS error {relative_rms_error:e}"
            ));
        }
        let mut files = Vec::new();
        for (name, samples) in NAMES.iter().zip(&stems) {
            let mut file = save(output, &format!("{slug}-{label}-{name}"), samples)?;
            let ratio = energy(samples) / total_energy;
            file["energy_ratio_to_total"] = json!(ratio);
            file["rms_ratio_to_total"] = json!(ratio.sqrt());
            file["rms_db_relative_total"] = json!(10. * ratio.max(1e-30).log10());
            files.push(file);
        }
        points.push(json!({
            "label":label,"requested_rpm":requested_rpm,"actual_rpm":rpm,
            "rpm_clamped":rpm!=requested_rpm,"throttle":throttle,"load":throttle,
            "commands_throttle_f64":commands.throttle,"commands_overrun":commands.overrun,
            "source_gains":[1,1],"total_gain":1,"files":files,
            "reconstruction":{"relative_rms_error":relative_rms_error,
                "rms_error":(error_energy/CAPTURE as f64).sqrt(),
                "max_abs_error":max_error,"relative_rms_limit":1e-5,"passed":true}
        }));
        println!(
            "{slug}/{label}: {rpm:.0} RPM, reconstruction relative RMS {relative_rms_error:.3e}"
        );
    }
    let source_after = digest(&source)?;
    if source_before != source_after {
        return Err(format!("Source archive changed: {}", source.display()));
    }
    Ok(json!({
        "slug":slug,"source":bank.source,"source_sha256_before":source_before,
        "source_sha256_after":source_after,"source_unchanged":true,
        "imported_baseline":baseline,"definition":definition,"settings":settings,
        "intake_air_noise":scratch.sound.intake_air_noise,
        "synthesis_rate":synthesis_rate,"physical_steps_per_output_sample":physical_steps,
        "definition_json_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&definition).map_err(|e|e.to_string())?)),
        "source_bank_gain_not_applied":bank.gain,
        "source_rpm_range":[bank.min_rpm,bank.max_rpm],
        "physical_rpm_range":[scratch.idle_rpm,scratch.redline_rpm],"points":points
    }))
}

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or("Usage: intake_sources <new-output-directory> [air-noise: 0..1]")?,
    );
    let air_noise = args
        .next()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| "Air noise must be a number in 0..1".to_owned())?
                .parse::<f32>()
                .map_err(|_| "Air noise must be a number in 0..1".to_owned())
        })
        .transpose()?;
    if air_noise.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err("Air noise must be a finite number in 0..1".into());
    }
    if args.next().is_some() {
        return Err("Usage: intake_sources <new-output-directory> [air-noise: 0..1]".into());
    }
    if output.exists() {
        return Err("Choose a new output directory; existing evidence is never overwritten".into());
    }
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let _denormals = DenormalGuard::enter();
    let mut corpus = Vec::new();
    for (slug, cylinders) in [("b5_c", 4), ("genesis_phantom", 6), ("advent_tc", 8)] {
        corpus.push(review(&output, slug, cylinders, air_noise)?);
    }
    let report = json!({
        "version":1,"bess_version":env!("CARGO_PKG_VERSION"),
        "sample_rate":RATE,"warmup_seconds":1,"capture_seconds":2,
        "path":"Engine at scratch.synthesis_rate(48000) + intake_sources taps + AutomationVoice-equivalent FIR decimation",
        "neutral_tone_controls":true,"intake_air_noise_override":air_noise,
        "added_normalization":false,"gain":1,"room":"Off",
        "notes":[
            "Sources are captured at every physical step and independently decimated, including the complete warmup. Read-only taps do not modify the engine.",
            "Same synthesis-rate policy, 95-tap FIR decimation, float32-to-float64 commands and default overrun as mechanical_review's AutomationVoice. No application gain or limiting; each engine's physical rate is recorded.",
            "Pulse is the acoustic runner/plenum contribution; flow is the residual flow/compressor sound before Tone. Tone controls remain at defaults; the optional air-noise gain is recorded explicitly. Their sum must reconstruct Sample::intake within 1e-5 relative RMS error.",
            "Source energies are correlated: energy ratios are not additive percentage shares of the total. RMS ratios and reconstruction error are reported explicitly.",
            "All WAVs are IEEE float32 with unity gain and may exceed full scale. One second warmup is reproducible, not a thermal-equilibrium claim."
        ],"corpus":corpus
    });
    fs::write(
        output.join("review.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Review complete: {}", output.join("review.json").display());
    Ok(())
}
