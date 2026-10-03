//! Imported B5C I4: isolated exhaust-decay comparison, with raw diagnostic WAVs.
//! Run from the repository root: exhaust_decay_review [new-output-directory].
use bess::{
    acoustics::{ExhaustLayout, ExhaustNetwork, Geometry},
    automation_model::AutomationModel,
    automation_voice::AutomationVoice,
    bank::Bank,
    engine_build::{Catalyst, Muffler},
    hybrid::Settings,
    physical::engine::Commands,
    realtime::DenormalGuard,
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
const DECAYS: [f32; 4] = [10., 40., 120., 250.];
const NAMES: [&str; 3] = ["exhaust", "intake", "mechanical"];

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
    let mean = samples.iter().map(|&x| f64::from(x)).sum::<f64>() / samples.len() as f64;
    let rms =
        (samples.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    let rms_ac = (samples
        .iter()
        .map(|&x| (f64::from(x) - mean).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt();
    Ok(json!({"file":path.file_name().unwrap().to_string_lossy(),
        "format":"IEEE float32 mono 48000 Hz; raw unscaled diagnostic, not a listening mix",
        "bytes":fs::metadata(&path).map_err(|e|e.to_string())?.len(),"frames":samples.len(),
        "mean":mean,"rms":rms,"rms_ac":rms_ac,
        "peak":samples.iter().map(|x|x.abs()).fold(0_f32,f32::max),
        "sha256":digest(&path)?,"float_le_sha256":format!("{:x}",sample_hash.finalize())}))
}

fn comparison(samples: &[f32], baseline: &[f32]) -> Value {
    let changed = samples
        .iter()
        .zip(baseline)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    let energy = samples
        .iter()
        .zip(baseline)
        .map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2))
        .sum::<f64>();
    let max_difference = samples
        .iter()
        .zip(baseline)
        .map(|(&a, &b)| (f64::from(a) - f64::from(b)).abs())
        .fold(0_f64, f64::max);
    json!({"reference_decay_ms":120,"bit_exact":changed==0,"different_samples":changed,
        "rms_difference":(energy/samples.len() as f64).sqrt(),"max_abs_difference":max_difference})
}

fn run(output: &Path) -> Result<(), String> {
    if output.exists() {
        return Err("Choose a new output directory; existing evidence is never overwritten".into());
    }
    let source = Path::new("cars/bunchyearth23_b5_c.zip");
    let source_before = digest(source)?;
    let bank = Bank::load(source, None)?;
    let baseline = AutomationModel::from_bank(&bank)?.baseline;
    if baseline.design.cylinders != 4
        || !(bank.min_rpm..=bank.max_rpm).contains(&3000.)
        || !(baseline.idle_rpm..=baseline.redline_rpm).contains(&3000.)
    {
        return Err("B5C I4 does not support the requested 3000 RPM point".into());
    }
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let _denormals = DenormalGuard::enter();
    let commands = Commands {
        imposed_rpm: Some(3000.),
        throttle: 0.5,
        overrun: 0.,
        ..Default::default()
    };
    let mut renders = Vec::new();
    for decay_ms in DECAYS {
        let mut definition = baseline;
        definition.experimental.wave_coupling = false;
        definition.experimental.primary_1d = false;
        definition.sound.exhaust_decay_ms = decay_ms;
        let settings = Settings {
            engine: Some(definition),
            engine_baseline: Some(baseline),
            physical_sound: definition.sound,
            enhanced: true,
            level_match: false,
            fuel_cut: 0.,
            ..Default::default()
        };
        settings.validate()?;
        let mut voice = AutomationVoice::from_settings(RATE, &bank, settings)?;
        let mut stems: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::with_capacity(CAPTURE));
        for frame in 0..WARMUP + CAPTURE {
            let sample = voice.next_commands(commands);
            if voice.failed() {
                return Err(format!(
                    "Physical voice failed at {decay_ms} ms, frame {frame}"
                ));
            }
            if frame >= WARMUP {
                for (stem, value) in
                    stems
                        .iter_mut()
                        .zip([sample.exhaust, sample.intake, sample.mechanical])
                {
                    stem.push(value);
                }
            }
        }
        renders.push((definition, settings, stems));
        println!("B5C: {decay_ms:.0} ms, 1s warmup + 2s at 3000 RPM / throttle 0.5");
    }
    let geometry = Geometry {
        header: 0.05,
        tail: baseline.sound.tail_length_m,
        diameter_mm: baseline.build.exhaust_mm,
        chamber_litres: match baseline.build.muffler {
            Muffler::None => 0.3,
            Muffler::StraightThrough => 2.5,
            Muffler::Baffled => 6.,
            Muffler::ReverseFlow => 9.,
        },
        absorption: baseline.sound.muffler_absorption,
        resonance: 1.,
        temperature_c: 400.,
    };
    let layout = ExhaustLayout {
        catalyst: baseline.build.catalyst != Catalyst::None,
        muffler: match baseline.build.muffler {
            Muffler::None => 0,
            Muffler::StraightThrough => 1,
            Muffler::Baffled => 2,
            Muffler::ReverseFlow => 3,
        },
    };
    let mut cases = Vec::new();
    for (case, (definition, settings, stems)) in renders.iter().enumerate() {
        let decay_ms = DECAYS[case];
        let mut files = Vec::new();
        for (index, (name, samples)) in NAMES.iter().zip(stems).enumerate() {
            let delta = comparison(samples, &renders[2].2[index]);
            if index != 0 && delta["bit_exact"] != true {
                return Err(format!(
                    "Exhaust decay changed the {name} stem at {decay_ms} ms"
                ));
            }
            if index == 0 && case != 2 && delta["bit_exact"] == true {
                return Err(format!("Exhaust decay is a no-op at {decay_ms} ms"));
            }
            let mut file = save(output, &format!("b5c-{decay_ms:03.0}ms-{name}"), samples)?;
            file["comparison_to_120_ms"] = delta;
            files.push(file);
        }
        // Separate downstream network: exactly one impulse, then zero source.
        // This is not a throttle cut; a spinning engine keeps exciting its pipes.
        let mut network = ExhaustNetwork::new(RATE as f32, geometry, layout);
        network.set_chamber_length_scale(baseline.sound.muffler_volume_scale);
        network.set_decay_ms(decay_ms);
        network.tune(geometry, true);
        let impulse: Vec<f32> = (0..CAPTURE)
            .map(|frame| network.next(if frame == 0 { 0.1 } else { 0. }))
            .collect();
        let total_energy = impulse.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>();
        if !total_energy.is_finite() || total_energy <= 1e-30 {
            return Err("Invalid or silent isolated network impulse response".into());
        }
        let remaining:Vec<_> = [20,40,120,250].into_iter().map(|ms| {
            let offset = ms*RATE as usize/1000;
            let energy = impulse[offset..].iter().map(|&x|f64::from(x).powi(2)).sum::<f64>();
            json!({"milliseconds_after_excitation":ms,"remaining_energy_fraction":energy/total_energy})
        }).collect();
        let mut impulse_file = save(
            output,
            &format!("network-{decay_ms:03.0}ms-impulse"),
            &impulse,
        )?;
        impulse_file["remaining_energy"] = json!(remaining);
        cases.push(json!({"exhaust_decay_ms":decay_ms,"definition":definition,"settings":settings,
            "definition_json_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(definition).map_err(|e|e.to_string())?)),
            "stems":files,"isolated_downstream_network_impulse":impulse_file}));
    }
    let source_after = digest(source)?;
    if source_before != source_after {
        return Err("Source Automation archive changed".into());
    }
    let report = json!({"version":1,"bess_version":env!("CARGO_PKG_VERSION"),"source":bank.source,
        "source_sha256_before":source_before,"source_sha256_after":source_after,"source_unchanged":true,
        "imported_baseline":baseline,"sample_rate":RATE,"synthesis_rate":baseline.to_scratch().synthesis_rate(RATE),
        "decay_values_ms":DECAYS,"warmup_seconds":1,"capture_seconds":2,"rpm":3000,"throttle":0.5,
        "room":"Off (raw voice path has no room)","wave_coupling":false,"primary_1d":false,
        "extra_gain":1,"added_normalization":false,"intake_and_mechanical_bit_exact_across_all_cases":true,
        "isolated_network":{"header_m":geometry.header,"tail_m":geometry.tail,"diameter_mm":geometry.diameter_mm,
            "chamber_litres":geometry.chamber_litres,"chamber_length_scale":baseline.sound.muffler_volume_scale,
            "absorption":geometry.absorption,"resonance":geometry.resonance,"temperature_c":geometry.temperature_c,
            "catalyst":layout.catalyst,"muffler":layout.muffler,"impulse_amplitude":0.1,"nonzero_source_samples":1},
        "notes":["Same imported engine, fresh deterministic state and identical commands; only exhaust decay varies between cases. Feedback and 1D primaries are explicitly disabled in all cases.",
            "The imported voice follows the engine synthesis-rate policy: this baseline runs at 96 kHz and uses a 95-tap FIR with 2:1 decimation for 48 kHz output. The separate downstream-network impulse runs directly at 48 kHz.",
            "The 1s warmup is reproducible, not a thermal-equilibrium claim. No throttle-off interval is represented as source extinction.",
            "The separate impulse uses the public downstream ExhaustNetwork, not the complete engine/primary/collector boundary. All source samples after its first are zero.",
            "Decay_ms specifies the loss design target; additional filters, absorption and boundaries also affect the measured decay. Remaining-energy fractions include propagation delay and are not a measured T60.",
            "Float32 files are unscaled diagnostic stems and can exceed full scale; no listening limiter, level matching or PCM normalization is applied."],"cases":cases});
    fs::write(
        output.join("review.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Review complete: {}", output.join("review.json").display());
    Ok(())
}

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("output/exhaust-decay-20261003/review"));
    if args.next().is_some() {
        return Err("Usage: exhaust_decay_review [new-output-directory]".into());
    }
    run(&output)
}
