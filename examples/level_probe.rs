//! Raw steady RMS across Automation A, source-guided B and generated B.
use bess::{
    bank::Bank,
    hybrid::{Hybrid, Settings},
    project::Parameters,
    render,
};
use std::{fs, path::Path, sync::Arc};

fn rms(bank: Arc<Bank>, rpm: f32, load: f32, mode: usize, level_match: bool) -> f64 {
    let p = Parameters {
        cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
        rpm,
        load,
        brightness: 10_000.,
        exhaust: 1.,
        intake: 0.25,
        mechanical: 0.12,
        ..Parameters::default()
    };
    let h = Settings {
        enhanced: mode != 0,
        procedural: mode == 2,
        level_match,
        ..Settings::calibrated(&bank)
    };
    let mut voice = Hybrid::new(48_000, p, h, Some(bank));
    let mut power = 0.;
    for i in 0..(4 * 48_000) {
        let x = voice.next_stems(true).mixed as f64;
        if i >= 3 * 48_000 {
            power += x * x;
        }
    }
    (power / 48_000.).sqrt()
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let bank = Arc::new(Bank::load(
        Path::new(args.get(1).ok_or("Usage: level_probe ZIP")?),
        None,
    )?);
    if args.iter().any(|arg| arg == "--raw-only") {
        for (index, layer) in bank.layers.iter().enumerate() {
            let mut upper = layer[layer.len() / 2..]
                .iter()
                .map(|sample| sample.rms)
                .collect::<Vec<_>>();
            upper.sort_by(f32::total_cmp);
            let reference = upper[upper.len() / 2];
            let early_max = layer
                .iter()
                .filter(|sample| sample.rpm <= bank.min_rpm + 600.)
                .map(|sample| sample.rms)
                .fold(0f32, f32::max);
            println!(
                "{} layer={} early/upper={:.2} upper={:.5}",
                args[1],
                index,
                early_max / reference,
                reference * bank.gain
            );
        }
        return Ok(());
    }
    if let Some(index) = args.iter().position(|arg| arg == "--review") {
        let directory = Path::new(args.get(index + 1).ok_or("Output directory required")?);
        fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        println!("mode,rpm,load,settled_rms");
        for rpm in [
            bank.min_rpm,
            bank.layers[0][4].rpm,
            3000f32.clamp(bank.min_rpm, bank.max_rpm),
        ] {
            for (mode, enhanced, procedural) in [
                ("A", false, false),
                ("B-standard", true, false),
                ("B-experimental", true, true),
            ] {
                let params = Parameters {
                    cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
                    rpm,
                    load: 0.12,
                    brightness: 10_000.,
                    exhaust: 1.,
                    intake: 0.25,
                    mechanical: 0.12,
                    ..Parameters::default()
                };
                let settings = Settings {
                    enhanced,
                    procedural,
                    ..Settings::calibrated(&bank)
                };
                let samples = render::hybrid_samples(params, settings, bank.clone(), 4., false)?;
                let settled = &samples[2 * 48_000..3 * 48_000];
                let level = (settled
                    .iter()
                    .map(|sample| (*sample as f64).powi(2))
                    .sum::<f64>()
                    / settled.len() as f64)
                    .sqrt();
                render::write_pcm(&directory.join(format!("{mode}-{rpm:.0}.wav")), &samples)?;
                println!("{mode},{rpm:.0},0.12,{level:.6}");
            }
        }
        return Ok(());
    }
    println!(
        "bank gain {:.4}, rpm {:.0}..{:.0}",
        bank.gain, bank.min_rpm, bank.max_rpm
    );
    for layer in &bank.layers {
        println!(
            "source knots: {}",
            layer
                .iter()
                .map(|s| format!("{:.0}:{:.5}", s.rpm, s.rms * bank.gain))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    let rpms = [
        bank.min_rpm,
        bank.layers[0][1].rpm,
        bank.layers[0][2].rpm,
        bank.layers[0][4].rpm,
        bank.layers[0][8].rpm,
        bank.layers[0][10].rpm,
        bank.layers[0][11].rpm,
        bank.layers[0][12].rpm,
        bank.layers[0][13].rpm,
        bank.layers[0][15].rpm,
        3000f32.clamp(bank.min_rpm, bank.max_rpm),
        bank.max_rpm,
    ];
    println!("load,rpm,A,B-standard,B-generated,B-standard-no-match,B-generated-no-match");
    for load in [0.12, 0.65] {
        for rpm in rpms {
            let levels = [
                rms(bank.clone(), rpm, load, 0, true),
                rms(bank.clone(), rpm, load, 1, true),
                rms(bank.clone(), rpm, load, 2, true),
                rms(bank.clone(), rpm, load, 1, false),
                rms(bank.clone(), rpm, load, 2, false),
            ];
            println!(
                "{load:.2},{rpm:.0},{}",
                levels.map(|n| format!("{n:.6}")).join(",")
            );
        }
    }
    Ok(())
}
