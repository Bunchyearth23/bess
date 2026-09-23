//! Render matched-level source, standard B and experimental B for listening.
use bess::{
    bank::Bank,
    hybrid::{Hybrid, Settings},
    project::Parameters,
    render,
};
use std::{path::Path, sync::Arc};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let archive = args
        .get(1)
        .ok_or("Usage: noise_review ZIP OUT_DIR RPM LOAD")?;
    let output = args.get(2).ok_or("Output directory required")?;
    let bank = Arc::new(Bank::load(Path::new(archive), None)?);
    let rpm = args
        .get(3)
        .ok_or("RPM required")?
        .parse::<f32>()
        .map_err(|_| "Invalid RPM")?
        .clamp(bank.min_rpm, bank.max_rpm);
    let load = args
        .get(4)
        .ok_or("Load required")?
        .parse::<f32>()
        .map_err(|_| "Invalid load")?;
    let params = Parameters {
        cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
        rpm,
        load,
        brightness: 10_000.,
        exhaust: 1.,
        intake: 0.25,
        mechanical: 0.12,
        ..Parameters::default()
    };
    println!(
        "{}",
        render::steady_procedural_comparison(Path::new(output), params, bank.clone(), 6.)?
    );
    for (mode, procedural) in [("standard", false), ("experimental", true)] {
        for (part, intake, mechanical) in [("intake", 1., 0.), ("mechanics", 0., 1.)] {
            let settings = Settings {
                procedural,
                level_match: false,
                ..Settings::calibrated(&bank)
            };
            let mut active = Hybrid::new(
                48_000,
                Parameters {
                    intake,
                    mechanical,
                    ..params
                },
                settings,
                Some(bank.clone()),
            );
            let mut muted = Hybrid::new(
                48_000,
                Parameters {
                    intake: 0.,
                    mechanical: 0.,
                    ..params
                },
                settings,
                Some(bank.clone()),
            );
            let mut samples = Vec::with_capacity(6 * 48_000);
            for _ in 0..6 * 48_000 {
                samples.push(active.next_stems(true).mixed - muted.next_stems(true).mixed);
            }
            let settled = &samples[48_000..];
            let rms = (settled.iter().map(|x| (*x as f64).powi(2)).sum::<f64>()
                / settled.len() as f64)
                .sqrt() as f32;
            let peak = samples.iter().fold(0f32, |max, value| max.max(value.abs()));
            let gain = (0.035 / rms.max(1e-9)).min(0.95 / peak.max(1e-9));
            for sample in &mut samples {
                *sample *= gain;
            }
            render::write_pcm(
                &Path::new(output).join(format!("{mode}-{part}.wav")),
                &samples,
            )?;
            println!("{mode}-{part}: raw RMS {rms:.6}, listening gain {gain:.2}x");
        }
    }
    Ok(())
}
