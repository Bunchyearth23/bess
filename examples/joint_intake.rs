//! W-011 synthetic comparison, no audio device required.
//! cargo run --release --example joint_intake -- <output-dir>
use bess::{
    physical::engine::{Commands, Engine},
    scratch::Scratch,
};
use std::{fmt::Write, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "out/joint-intake".into()),
    );
    std::fs::create_dir_all(&out)?;
    let mut csv =
        String::from("mode,length_m,rate,mean_torque_nm,mean_map_pa,intake_rms,render_seconds\n");
    for rate in [48000_u32, 96000] {
        for length in [0.25_f32, 0.75] {
            for (mode, intake, exhaust) in [
                ("legacy", false, false),
                ("exhaust", false, true),
                ("joint", true, true),
            ] {
                let mut scratch = Scratch::default();
                scratch.experimental.intake_wave_coupling = intake;
                scratch.experimental.wave_coupling = exhaust;
                scratch.sound.intake_length_m = length;
                let stem = format!("{mode}-{length}-{rate}");
                std::fs::write(
                    out.join(format!("{stem}.scratch.json")),
                    serde_json::to_string_pretty(&scratch)?,
                )?;
                let mut engine = Engine::new(&scratch, rate)?;
                let mut wav = hound::WavWriter::create(
                    out.join(format!("{stem}.wav")),
                    hound::WavSpec {
                        channels: 2,
                        sample_rate: rate,
                        bits_per_sample: 32,
                        sample_format: hound::SampleFormat::Float,
                    },
                )?;
                let mut trace =
                    String::from("time_s,rpm,throttle,torque_nm,map_pa,heat_j,intake,exhaust\n");
                let start = Instant::now();
                let (mut torque, mut map, mut energy, mut count) = (0., 0., 0., 0_u32);
                for frame in 0..rate / 2 {
                    let time = f64::from(frame) / f64::from(rate);
                    let throttle = if time < 0.15 {
                        0.3
                    } else if time < 0.4 {
                        0.85
                    } else {
                        0.03
                    };
                    let sample = engine.next(Commands {
                        imposed_rpm: Some(3200.),
                        throttle,
                        ..Default::default()
                    });
                    if engine.failed() {
                        return Err(format!("{stem} failed at frame {frame}").into());
                    }
                    // Fixed engine observations; channels contain intake and exhaust.
                    wav.write_sample(sample.intake)?;
                    wav.write_sample(sample.exhaust)?;
                    if (0.3..0.4).contains(&time) {
                        torque += sample.torque_nm;
                        map += sample.map_pa;
                        energy += f64::from(sample.intake).powi(2);
                        count += 1;
                    }
                    if frame.is_multiple_of(rate / 1000) {
                        writeln!(
                            trace,
                            "{time:.6},3200,{throttle},{:.6},{:.6},{:.6},{:.6},{:.6}",
                            sample.torque_nm,
                            sample.map_pa,
                            sample.heat_j,
                            sample.intake,
                            sample.exhaust
                        )?;
                    }
                }
                wav.finalize()?;
                std::fs::write(out.join(format!("{stem}.csv")), trace)?;
                let row = format!(
                    "{mode},{length},{rate},{:.6},{:.6},{:.8},{:.6}",
                    torque / f64::from(count),
                    map / f64::from(count),
                    (energy / f64::from(count)).sqrt(),
                    start.elapsed().as_secs_f64()
                );
                println!("{row}");
                writeln!(csv, "{row}")?;
            }
        }
    }
    std::fs::write(out.join("joint-intake-comparison.csv"), csv)?;
    std::fs::write(
        out.join("README.txt"),
        "Synthetic W-011 reference, estimated engine geometry. Same seed/commands and fixed observation gain. 48/96 kHz, 0.25/0.75 m runners, legacy/exhaust/joint coupling. Each WAV has intake left and exhaust right. RPM imposed 3200; throttle 0.3 then0.85 then0.03. Warm-up0–0.3 s; summary0.3–0.4 s. No real-engine, listening, live-device endurance or BeamNG validation. Historical X-028 10% criterion unchanged and separately tested.\n",
    )?;
    Ok(())
}
