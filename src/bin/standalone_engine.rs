//! Recording-free engine instrument: reproducible WAV scenarios and a render benchmark.
use bess::standalone::{CombustionState, Commands, Config, DEFAULT_RATE, Synth};
use std::{path::Path, time::Instant};

fn value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}
fn number(args: &[String], flag: &str, default: f32) -> Result<f32, String> {
    match value(args, flag) {
        Some(v) => v.parse().map_err(|_| format!("invalid {flag}")),
        None => Ok(default),
    }
}
fn usage() -> &'static str {
    "Usage:\n  standalone_engine --write-presets DIR\n  standalone_engine --compare DIR [--scenario steady|ramp|load|shutdown] [--seconds 4] [--rpm 3000] [--load 0.65] [--volume 0.6]\n  standalone_engine (--preset single|four-even|four-split | --config FILE) --scenario steady|ramp|load|shutdown --out FILE [--seconds 6] [--rate 48000] [--rpm 3000] [--load 0.65] [--volume 0.6]\n  add --benchmark to measure the same render without writing a WAV"
}
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(dir) = value(&args, "--write-presets") {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        for name in ["single", "four-even", "four-split"] {
            let path = Path::new(dir).join(format!("{name}.json"));
            let config = Config::preset(name).unwrap();
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            println!("{}", path.display());
        }
        return Ok(());
    }
    if args.len() == 1 || args.iter().any(|a| a == "--help") {
        println!("{}", usage());
        return Ok(());
    }
    if let Some(dir) = value(&args, "--compare") {
        // Each candidate receives exactly the same commands, duration and sample
        // rate. Leave their levels untouched so the comparison does not hide gain.
        let dir = Path::new(dir);
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut report =
            String::from("Standalone preset comparison; no per-file level normalization.\n");
        for name in ["single", "four-even", "four-split"] {
            let path = dir.join(format!("{name}.wav"));
            let mut command = std::process::Command::new(&exe);
            command.arg("--preset").arg(name).arg("--out").arg(&path);
            for flag in [
                "--scenario",
                "--seconds",
                "--rate",
                "--rpm",
                "--load",
                "--volume",
            ] {
                if let Some(v) = value(&args, flag) {
                    command.arg(flag).arg(v);
                }
            }
            let output = command.output().map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned());
            }
            report.push_str(&format!("\n{name}: {}\n", path.display()));
            report.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        std::fs::write(dir.join("comparison.txt"), &report).map_err(|e| e.to_string())?;
        print!("{report}");
        return Ok(());
    }
    let config = if let Some(path) = value(&args, "--config") {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Config>(&json).map_err(|e| e.to_string())?
    } else {
        Config::preset(value(&args, "--preset").unwrap_or("four-even"))
            .ok_or_else(|| "unknown preset".to_owned())?
    };
    config.validate()?;
    let scenario = value(&args, "--scenario").unwrap_or("steady");
    if !["steady", "ramp", "load", "shutdown"].contains(&scenario) {
        return Err("unknown scenario".into());
    }
    let seconds = number(&args, "--seconds", 6.)?;
    if !seconds.is_finite() || !(2.0..=30.).contains(&seconds) {
        return Err("seconds must be 2–30".into());
    }
    let rate = number(&args, "--rate", DEFAULT_RATE as f32)?;
    if !rate.is_finite() || rate.fract() != 0. || !(8_000.0..=192_000.0).contains(&rate) {
        return Err("rate must be 8000–192000 Hz".into());
    }
    let rate = rate as u32;
    let base = Commands {
        rpm: number(&args, "--rpm", 3_000.)?,
        load: number(&args, "--load", 0.65)?,
        volume: number(&args, "--volume", 0.6)?,
        combustion: CombustionState::Firing,
    };
    base.validate()?;
    let benchmark = args.iter().any(|a| a == "--benchmark");
    let out = value(&args, "--out");
    if !benchmark && out.is_none() {
        return Err("--out FILE is required".into());
    }
    let mut writer = if benchmark {
        None
    } else {
        Some(
            hound::WavWriter::create(
                out.unwrap(),
                hound::WavSpec {
                    channels: 1,
                    sample_rate: rate,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .map_err(|e| e.to_string())?,
        )
    };
    let mut synth = Synth::new(rate, config, base)?;
    let frames = (seconds * rate as f32).round() as usize;
    let mut block = [0f32; 256];
    let mut power = 0f64;
    let mut sum = 0f64;
    let mut peak = 0f32;
    let now = Instant::now();
    for start in (0..frames).step_by(block.len()) {
        let count = (frames - start).min(block.len());
        // One control update per block; the DSP smooths rpm/load/volume per sample.
        let t = start as f32 / frames as f32;
        let mut command = base;
        match scenario {
            "ramp" => {
                command.rpm = if t < 0.5 {
                    800. + t * 2. * 5_200.
                } else {
                    6_000. - (t - 0.5) * 2. * 5_200.
                };
            }
            "load" => {
                command.load = if t < 0.5 { 0.15 } else { 0.9 };
            }
            "shutdown" => {
                if t >= 0.5 {
                    command.combustion = CombustionState::FuelCut;
                }
                if t >= 0.65 {
                    command.rpm = 0.;
                }
            }
            _ => {}
        }
        synth.set_commands(command)?;
        synth.render_block(&mut block[..count]);
        for &sample in &block[..count] {
            if !sample.is_finite() {
                return Err("nonfinite output".into());
            }
            power += (sample as f64).powi(2);
            sum += sample as f64;
            peak = peak.max(sample.abs());
            if let Some(writer) = &mut writer {
                writer
                    .write_sample((sample * 8_388_607.) as i32)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    if let Some(writer) = writer {
        writer.finalize().map_err(|e| e.to_string())?;
    }
    let elapsed = now.elapsed().as_secs_f64();
    let counts = synth.counts();
    println!(
        "scenario={scenario} rate={rate}Hz block=256 frames={frames} elapsed={elapsed:.3}s throughput={:.2}x",
        frames as f64 / rate as f64 / elapsed
    );
    println!(
        "events combustion={} exhaust={} intake={} rms={:.6} peak={peak:.6} mean={:.6}",
        counts.combustion,
        counts.exhaust,
        counts.intake,
        (power / frames as f64).sqrt(),
        sum / frames as f64
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}\n{}", usage());
        std::process::exit(2);
    }
}
