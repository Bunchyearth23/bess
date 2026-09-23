//! Raw A/B renders and low-RPM component controls for pressure-sound review.
use bess::{
    bank::Bank,
    hybrid::{Hybrid, Settings},
    project::Parameters,
    render,
};
use std::{fs, path::Path, sync::Arc};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let bank = Arc::new(Bank::load(
        Path::new(args.get(1).ok_or("Usage: punch_review ZIP OUT_DIR")?),
        None,
    )?);
    let out = Path::new(args.get(2).ok_or("Output directory required")?);
    fs::create_dir_all(out).map_err(|error| error.to_string())?;
    let idle = bank.layers[0][4].rpm;
    println!("low-RPM review at {idle:.0} rpm; loaded review at 3000 rpm");
    for (label, rpm, load) in [("idle", idle, 0.12), ("loaded", 3000., 0.65)] {
        for mode in [
            "A",
            "standard",
            "standard-no-edge",
            "standard-recorded",
            "standard-generated",
            "experimental",
        ] {
            if label == "loaded" && !["A", "standard", "experimental"].contains(&mode) {
                continue;
            }
            let params = Parameters {
                cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
                rpm: rpm.clamp(bank.min_rpm, bank.max_rpm),
                load,
                brightness: 10_000.,
                exhaust: 1.,
                intake: 0.25,
                mechanical: 0.12,
                ..Parameters::default()
            };
            let mut settings = Settings::calibrated(&bank);
            settings.enhanced = mode != "A";
            settings.procedural = mode == "experimental";
            match mode {
                "standard-no-edge" => settings.pressure_shape = 0.,
                "standard-recorded" => settings.source_timbre = 1.,
                "standard-generated" => settings.source_timbre = 0.,
                _ => {}
            }
            let samples = render::hybrid_samples(params, settings, bank.clone(), 5., false)?;
            render::write_pcm(&out.join(format!("{label}-{mode}.wav")), &samples)?;
        }
    }
    for mode in ["A", "standard", "experimental"] {
        let params = Parameters {
            cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
            rpm: 3000f32.clamp(bank.min_rpm, bank.max_rpm),
            load: 0.12,
            brightness: 10_000.,
            exhaust: 1.,
            intake: 0.25,
            mechanical: 0.12,
            ..Parameters::default()
        };
        let settings = Settings {
            enhanced: mode != "A",
            procedural: mode == "experimental",
            ..Settings::calibrated(&bank)
        };
        let mut voice = Hybrid::new(48_000, params, settings, Some(bank.clone()));
        let mut samples = Vec::with_capacity(6 * 48_000);
        for frame in 0..6 * 48_000 {
            if frame == 2 * 48_000 || frame == 4 * 48_000 {
                voice.set(
                    Parameters {
                        load: if frame == 2 * 48_000 { 0.85 } else { 0.12 },
                        ..params
                    },
                    settings,
                );
            }
            samples.push(voice.next_stems(true).mixed);
        }
        render::write_pcm(&out.join(format!("step-{mode}.wav")), &samples)?;
    }
    Ok(())
}
