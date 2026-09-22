//! Render a fixed operating point for investigating audible level changes.
use bdsp::resample::{SincQuality, SincTable};
use bess::{bank::Bank, hybrid::Settings, project::Parameters, render};
use std::{path::Path, sync::Arc};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if !(6..=7).contains(&args.len()) {
        return Err("Usage: steady ARCHIVE RPM LOAD SECONDS OUTPUT_DIRECTORY [VARIANT]".into());
    }
    let bank = Arc::new(Bank::load(Path::new(&args[1]), None)?);
    let rpm = args[2].parse::<f32>().map_err(|e| e.to_string())?;
    let load = args[3].parse::<f32>().map_err(|e| e.to_string())?;
    let seconds = args[4].parse::<f32>().map_err(|e| e.to_string())?;
    let output = Path::new(&args[5]);
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let params = Parameters {
        rpm,
        load,
        brightness: 10_000.,
        exhaust: 1.,
        intake: 0.25,
        mechanical: 0.12,
        ..Parameters::default()
    };
    let mut settings = Settings::calibrated(&bank);
    match args.get(6).map(String::as_str).unwrap_or("normal") {
        "normal" => {}
        "pressure0" => settings.pressure_shape = 0.,
        "texture0" => settings.pulse_texture = 0.,
        "color0" => settings.coloration = 0.,
        "level0" => settings.level_match = false,
        "cycle0" => settings.cycle_life = 0.,
        "pulse0" => settings.pulse_gain = 0.,
        "flat" => {
            settings.pressure_shape = 0.;
            settings.pulse_texture = 0.;
            settings.coloration = 0.;
            settings.cycle_life = 0.;
        }
        other => return Err(format!("Unknown variant: {other}")),
    }
    for (name, enhanced) in [("source", false), ("bess", true)] {
        render::hybrid_wav(
            &output.join(format!("{name}.wav")),
            params,
            Settings {
                enhanced,
                ..settings
            },
            bank.clone(),
            seconds,
            false,
        )?;
    }
    let sinc = SincTable::for_quality(SincQuality::Realtime);
    for (name, read) in [
        (
            "bank-original",
            Bank::read_original as fn(&Bank, f64, f32, f32, f32, &SincTable) -> f32,
        ),
        (
            "bank-modern",
            Bank::read as fn(&Bank, f64, f32, f32, f32, &SincTable) -> f32,
        ),
    ] {
        let samples: Vec<f32> = (0..(seconds * 48_000.) as usize)
            .map(|i| {
                let cycle = i as f64 * rpm as f64 / (120. * 48_000.);
                read(&bank, cycle, rpm, load, 48_000., &sinc) * 0.35
            })
            .collect();
        render::write_pcm(&output.join(format!("{name}.wav")), &samples)?;
    }
    let mut periodic = Vec::with_capacity((seconds * 48_000.) as usize);
    let mut residual = Vec::with_capacity((seconds * 48_000.) as usize);
    for i in 0..(seconds * 48_000.) as usize {
        let cycle = i as f64 * rpm as f64 / (120. * 48_000.);
        let (p, r) = bank.read_components(cycle, rpm, load, 48_000., &sinc);
        periodic.push(p * 0.35);
        residual.push(r * 0.35);
    }
    render::write_pcm(&output.join("bank-periodic.wav"), &periodic)?;
    render::write_pcm(&output.join("bank-residual.wav"), &residual)?;
    println!("{}: {rpm} RPM, load {load}, {seconds} s", output.display());
    Ok(())
}
