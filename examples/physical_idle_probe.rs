//! Fixed idle diagnosis: raw physical stems at 96 kHz and actual Orbit output at 48 kHz.
use bess::{
    bench::BeamNgCamera,
    drive::{Controls, Mode},
    hybrid::Settings,
    physical::engine::{Commands, Engine},
    project::Parameters,
    realtime::{DenormalGuard, RenderEngine},
    scratch::{PRESETS, Scratch},
};
use std::{fmt::Write as _, path::PathBuf};

const RATE: u32 = 96_000;
const SECONDS: usize = 8;

#[derive(Default)]
struct Stats {
    peak: f64,
    sum: f64,
    squares: f64,
    count: usize,
}
impl Stats {
    fn add(&mut self, x: f32) {
        let x = f64::from(x);
        self.peak = self.peak.max(x.abs());
        self.sum += x;
        self.squares += x * x;
        self.count += 1;
    }
    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "peak_dbfs": 20. * self.peak.log10(),
            "rms_dbfs": 10. * (self.squares / self.count as f64).log10(),
            "mean": self.sum / self.count as f64,
            "frames": self.count,
            "measurement_start_s": 2,
        })
    }
}

fn main() -> Result<(), String> {
    let _denormals = DenormalGuard::enter();
    let dir = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("output/physical-idle-probe"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for (name, slug) in [
        ("Inline-4", "i4"),
        ("V8 cross-plane", "v8-cross-plane"),
        ("V12 60°", "v12-60"),
    ] {
        let mut scratch = Scratch {
            design: PRESETS
                .iter()
                .find(|(preset, _)| *preset == name)
                .unwrap()
                .1,
            ..Scratch::default()
        };
        let mut settings = Settings::default();
        let mut params = Parameters {
            volume: 0.8,
            ..Default::default()
        };
        let mut controls = Controls {
            mode: Mode::Direct,
            ..Default::default()
        };
        scratch.derive_from_build(&mut settings, &mut params, &mut controls);
        params.rpm = 850.;
        params.load = 0.05;
        let commands = Commands {
            imposed_rpm: Some(850.),
            throttle: 0.05,
            overrun: f64::from(settings.fuel_cut),
            ..Default::default()
        };
        let mut raw = Engine::new(&scratch, RATE)?;
        let mut listening = RenderEngine::scratch(RATE / 2, params, settings, controls, &scratch)?;
        listening.bench.set_beamng_camera(BeamNgCamera::Orbit);
        let mut writers = Vec::new();
        for (stem, rate) in [
            ("exhaust", RATE),
            ("intake", RATE),
            ("mechanical", RATE),
            ("orbit", RATE / 2),
        ] {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: rate,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            };
            writers.push(
                hound::WavWriter::create(dir.join(format!("{slug}-{stem}.wav")), spec)
                    .map_err(|e| e.to_string())?,
            );
        }
        let mut stats: [Stats; 4] = std::array::from_fn(|_| Stats::default());
        let mut csv = String::from(
            "time_s,rpm,map_pa,torque_nm,heat_j,afterfire_excitation_j,fuel_injected_kg,misfires,fuel_cut,idle_bypass,fresh_supply_kg_s,tailpipe_fresh_in_kg_s\n",
        );
        let (mut heat, mut afterfire, mut fuel) = (0., 0., 0.);
        for frame in 0..RATE as usize * SECONDS {
            let sample = raw.next(commands);
            let mut values = [sample.exhaust, sample.intake, sample.mechanical, 0.];
            let count = if frame.is_multiple_of(2) {
                values[3] = listening.next_sample(true);
                4
            } else {
                3
            };
            if raw.failed() || listening.bench.failed() {
                return Err(format!("Physical fault in {name} at frame {frame}"));
            }
            for i in 0..count {
                let value = values[i];
                if !value.is_finite() || value.abs() > 1. {
                    return Err(format!(
                        "Invalid PCM level in {name}, stem {i}, frame {frame}: {value}"
                    ));
                }
                writers[i]
                    .write_sample((value * 8_388_607.) as i32)
                    .map_err(|e| e.to_string())?;
                if frame >= RATE as usize * 2 {
                    stats[i].add(value);
                }
            }
            heat += sample.heat_j;
            afterfire += sample.afterfire_heat_j;
            fuel += sample.fuel_injected_kg;
            if (frame + 1).is_multiple_of(RATE as usize / 1000) {
                writeln!(
                    csv,
                    "{:.3},{:.6},{:.6},{:.6},{:.9},{:.9},{:.12},{},{},{:.9},{:.9},{:.9}",
                    (frame + 1) as f64 / f64::from(RATE),
                    sample.rpm,
                    sample.map_pa,
                    sample.torque_nm,
                    heat,
                    afterfire,
                    fuel,
                    sample.misfires,
                    sample.fuel_cut,
                    sample.idle_bypass,
                    sample.fresh_supply_kg_s,
                    sample.fresh_tailpipe_in_kg_s
                )
                .map_err(|e| e.to_string())?;
                (heat, afterfire, fuel) = (0., 0., 0.);
            }
        }
        for writer in writers {
            writer.finalize().map_err(|e| e.to_string())?;
        }
        std::fs::write(dir.join(format!("{slug}-state.csv")), csv).map_err(|e| e.to_string())?;
        let result = serde_json::json!({
            "preset": name, "slug": slug, "seconds": SECONDS, "imposed_rpm": 850,
            "throttle": 0.05, "raw_rate": RATE, "listening_rate": RATE / 2,
            "camera": "Orbit", "volume": params.volume, "normalized": false,
            "raw_stems": "Engine outputs before Bench gains and Listener; PCM24 at 96 kHz",
            "orbit": "Actual RenderEngine with 2x physical engine, Bench gains, Listener and decimation; PCM24 at 48 kHz",
            "measurement": "Unwindowed peak/RMS/DC over seconds 2–8; no fades or level normalization",
            "state_csv": "1 kHz; heat, afterfire excitation and injected fuel summed over each 1 ms; remaining fields sampled at its end",
            "afterfire_excitation": "0.8 times reacted chemical heat; not net gas heat",
            "exhaust": stats[0].json(), "intake": stats[1].json(), "mechanical": stats[2].json(), "listening": stats[3].json(),
            "scratch": scratch, "parameters": params, "settings": settings,
        });
        println!("{name}: {}", stats[3].json());
        results.push(result);
    }
    std::fs::write(
        dir.join("metrics.json"),
        serde_json::to_vec_pretty(&results).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Idle probe complete: {}", dir.display());
    Ok(())
}
