//! Compare the raw intake and generated flow contributions across engine loads.
use bess::{
    bank::Bank,
    hybrid::{Hybrid, Settings},
    project::Parameters,
    render,
};
use std::{fs, path::Path, sync::Arc};

const RATE: usize = 48_000;
const SECONDS: usize = 3;

fn metrics(samples: &[f32]) -> (f64, f64, f64) {
    let mut low = 0.0;
    let alpha = (-2.0 * std::f64::consts::PI * 2000.0 / RATE as f64).exp();
    let mut power = 0.0;
    let mut high = 0.0;
    let mut peak = 0.0f64;
    for &sample in samples {
        let sample = sample as f64;
        low = low * alpha + sample * (1.0 - alpha);
        power += sample * sample;
        high += (sample - low).powi(2);
        peak = peak.max(sample.abs());
    }
    let n = samples.len() as f64;
    ((power / n).sqrt(), (high / n).sqrt(), peak)
}

fn render_pair(bank: Arc<Bank>, rpm: f32, load: f32, procedural: bool, part: &str) -> Vec<f32> {
    let params = Parameters {
        cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
        rpm,
        load,
        exhaust: 1.0,
        intake: 1.0,
        mechanical: 0.0,
        ..Parameters::default()
    };
    let settings = Settings {
        procedural,
        level_match: false,
        ..Settings::calibrated(&bank)
    };
    let mut muted_params = params;
    let mut muted_settings = settings;
    match part {
        "intake" => muted_params.intake = 0.0,
        "flow" => muted_settings.generated_flow = 0.0,
        _ => unreachable!(),
    }
    let mut active = Hybrid::new(RATE as u32, params, settings, Some(bank.clone()));
    let mut muted = Hybrid::new(RATE as u32, muted_params, muted_settings, Some(bank));
    let mut output = Vec::with_capacity(RATE * SECONDS);
    for _ in 0..RATE * SECONDS {
        output.push(active.next_stems(true).mixed - muted.next_stems(true).mixed);
    }
    output
}

fn render_ramp(bank: Arc<Bank>, mode: &str) -> Vec<f32> {
    let mut settings = Settings::calibrated(&bank);
    settings.enhanced = mode != "A";
    settings.procedural = mode.starts_with("experimental");
    if mode.ends_with("no-flow") {
        settings.generated_flow = 0.0;
    }
    if mode.ends_with("no-texture") {
        settings.residual_gain = 0.0;
    }
    let params = Parameters {
        cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
        rpm: bank.min_rpm,
        load: 0.12,
        exhaust: 1.0,
        intake: if mode.ends_with("no-intake") {
            0.0
        } else {
            0.25
        },
        mechanical: 0.12,
        ..Parameters::default()
    };
    let mut voice = Hybrid::new(RATE as u32, params, settings, Some(bank.clone()));
    let mut samples = Vec::with_capacity(RATE * 8);
    for frame in 0..RATE * 8 {
        if frame % 256 == 0 {
            let time = frame as f32 / RATE as f32;
            let progress = ((time - 1.0) / 6.0).clamp(0.0, 1.0);
            voice.set(
                Parameters {
                    rpm: bank.min_rpm + (bank.max_rpm.min(5500.0) - bank.min_rpm) * progress,
                    load: 0.12 + 0.78 * progress,
                    ..params
                },
                settings,
            );
        }
        samples.push(voice.next_stems(true).mixed);
    }
    samples
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let bank = Arc::new(Bank::load(
        Path::new(
            args.get(1)
                .ok_or("Usage: load_noise_probe ZIP OUTPUT_DIR")?,
        ),
        None,
    )?);
    let output = Path::new(args.get(2).ok_or("Output directory required")?);
    fs::create_dir_all(output).map_err(|error| error.to_string())?;
    let rpm = 3000.0f32.clamp(bank.min_rpm, bank.max_rpm);
    println!("mode,part,rpm,load,raw_rms,above_2khz_rms,peak");
    for (mode, procedural) in [("standard", false), ("experimental", true)] {
        for load in [0.2, 0.4, 0.65, 0.9] {
            for part in if procedural {
                &["intake", "flow"][..]
            } else {
                &["intake"][..]
            } {
                let samples = render_pair(bank.clone(), rpm, load, procedural, part);
                let (rms, high, peak) = metrics(&samples[RATE..]);
                println!("{mode},{part},{rpm:.0},{load:.2},{rms:.7},{high:.7},{peak:.7}");
                render::write_pcm(
                    &output.join(format!("{mode}-{part}-{load:.2}.wav")),
                    &samples,
                )?;
            }
        }
    }
    for mode in [
        "A",
        "standard",
        "standard-no-intake",
        "standard-no-texture",
        "experimental",
        "experimental-no-intake",
        "experimental-no-flow",
    ] {
        let samples = render_ramp(bank.clone(), mode);
        render::write_pcm(&output.join(format!("{mode}-ramp.wav")), &samples)?;
    }
    Ok(())
}
