//! Isolated source preparation and decomposition, with one common gain.
use bdsp::resample::{SincQuality, SincTable};
use bess::{bank::Bank, render};
use serde_json::json;
use std::{fs, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: components ZIP NEW_OUTPUT".into());
    }
    let out = Path::new(&args[2]);
    fs::create_dir(out)?;
    let bank = Bank::load(Path::new(&args[1]), None)?;
    let sinc = SincTable::for_quality(SincQuality::Realtime);
    let mut tracks: [Vec<f32>; 4] = std::array::from_fn(|_| Vec::with_capacity(48000 * 16));
    let mut cycle = 0.;
    let mut max_error = 0f32;
    for i in 0..48000 * 16 {
        let t = i as f32 / 48000.;
        // Slow sweep, both endpoints, then rapid sweeps and load changes.
        let position = if t < 2. {
            0.
        } else if t < 8. {
            (t - 2.) / 6.
        } else if t < 10. {
            1.
        } else {
            ((t - 10.) * 0.8 * std::f32::consts::TAU).sin() * 0.5 + 0.5
        };
        let rpm = bank.min_rpm + (bank.max_rpm - bank.min_rpm) * position;
        let load = if t < 10. {
            0.5 - 0.5 * (t * std::f32::consts::TAU / 4.).cos()
        } else {
            0.5 - 0.5 * (t * std::f32::consts::TAU).cos()
        };
        cycle += rpm as f64 / (120. * 48000.);
        let source = bank.read(cycle, rpm, load, 48000., &sinc);
        let (p, r) = bank.read_components(cycle, rpm, load, 48000., &sinc);
        max_error = max_error.max((source - (p + r)).abs());
        let gain = 0.35 * (t / 0.025).min(1.) * ((16. - t) / 0.05).min(1.);
        for (track, sample) in tracks.iter_mut().zip([
            bank.read_original(cycle, rpm, load, 48000., &sinc),
            source,
            p,
            r,
        ]) {
            track.push(sample * gain);
        }
    }
    if max_error > 1e-6 {
        return Err("Reconstruction error".into());
    }
    let peak = tracks.iter().flatten().map(|s| s.abs()).fold(0., f32::max);
    let safety = (0.95 / peak.max(1e-9)).min(1.);
    let names = [
        "01-original-playback",
        "02-measured-cycles",
        "03-pulses",
        "04-texture",
    ];
    let mut metrics = Vec::new();
    for (name, track) in names.iter().zip(tracks.iter_mut()) {
        for s in track.iter_mut() {
            *s *= safety;
        }
        render::write_pcm(&out.join(format!("{name}.wav")), track)?;
        metrics.push(json!({"name":name,"rms":(track.iter().map(|s|(*s as f64).powi(2)).sum::<f64>()/track.len() as f64).sqrt(),"peak":track.iter().map(|s|s.abs()).fold(0.,f32::max)}));
    }
    let report = json!({"source":bank.source,"version":env!("CARGO_PKG_VERSION"),"max_reconstruction_error":max_error,"common_safety":safety,"metrics":metrics,"periods":bank.layers.iter().map(|l|l.iter().map(|s|json!({"rpm":s.rpm,"period":s.period})).collect::<Vec<_>>()).collect::<Vec<_>>()});
    fs::write(
        out.join("components.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}; reconstruction error {}", out.display(), max_error);
    Ok(())
}
