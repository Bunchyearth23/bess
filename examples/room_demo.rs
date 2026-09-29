//! Live-listening room proof clips (4 s, default scratch engine, 48 kHz) and
//! convolution CPU cost. Rooms exist only on the live path; this example
//! enables one explicitly, as `Audio::with_scratch` does.
use bess::{
    bench::BeamNgCamera,
    drive::{Controls, Mode},
    hybrid::Settings,
    project::Parameters,
    realtime::{DenormalGuard, RenderEngine},
    render::write_pcm,
    room::{Room, RoomReverb},
    scratch::Scratch,
};
use std::{path::PathBuf, time::Instant};

const RATE: usize = 48_000;
const MIX: f32 = 0.35;

fn main() -> Result<(), String> {
    let _denormals = DenormalGuard::enter();
    let dir = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("output/room-demo"), PathBuf::from);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut scratch = Scratch::default();
    let mut params = Parameters {
        volume: 0.8,
        ..Parameters::default()
    };
    let mut settings = Settings::default();
    let mut controls = Controls {
        mode: Mode::Direct,
        ..Controls::default()
    };
    scratch.derive_from_build(&mut settings, &mut params, &mut controls);
    let idle = scratch.idle_rpm;
    for room in Room::ALL {
        let mut engine = RenderEngine::scratch(RATE as u32, params, settings, controls, &scratch)?;
        engine.bench.set_beamng_camera(BeamNgCamera::Orbit);
        if room != Room::Off {
            engine.bench.enable_room();
            engine.bench.set_room(room, MIX);
        }
        let mut samples = Vec::with_capacity(RATE * 4);
        for frame in 0..RATE * 4 {
            if frame.is_multiple_of(RATE / 1000) {
                // Idle, rev to 4000 rpm, lift off, idle: transients excite the tail.
                let t = frame as f32 / RATE as f32;
                (params.rpm, params.load) = match t {
                    t if t < 1. => (idle, 0.05),
                    t if t < 2. => (idle + (4000. - idle) * (t - 1.), 0.85),
                    t if t < 2.6 => (4000. + (idle - 4000.) * (t - 2.) / 0.6, 0.),
                    _ => (idle, 0.05),
                };
                engine.bench.set(params, settings, controls, 0);
            }
            samples.push(engine.next_sample(true));
        }
        let peak = samples.iter().fold(0f32, |p, x| p.max(x.abs()));
        let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt();
        let name = format!("{room:?}.wav").to_lowercase();
        write_pcm(&dir.join(&name), &samples)?;
        println!(
            "{name}: peak {:.2} dBFS, RMS {:.2} dBFS (mix {MIX})",
            20. * peak.log10(),
            20. * rms.log10()
        );
    }
    let mut engine = RenderEngine::scratch(RATE as u32, params, settings, controls, &scratch)?;
    let mut dry = f64::MAX;
    for _ in 0..5 {
        let started = Instant::now();
        for _ in 0..RATE {
            std::hint::black_box(engine.next_sample(true));
        }
        dry = dry.min(started.elapsed().as_secs_f64());
    }
    println!(
        "Dry scratch chain for comparison: {:.2} ms per second of 48 kHz audio",
        dry * 1e3
    );
    for (room, rate) in [
        (Room::Off, 96_000u32),
        (Room::Hall, 48_000),
        (Room::Hall, 96_000),
    ] {
        let mut reverb = RoomReverb::new(rate);
        reverb.set(room, 1.);
        let mut noise = bdsp::noise::Noise::new(bdsp::noise::NoiseColor::White);
        let input: Vec<f32> = (0..rate).map(|_| noise.next_sample() * 0.1).collect();
        // Minimum of 20 one-second runs: a loaded machine only adds time.
        let mut per_second = f64::MAX;
        for _ in 0..20 {
            let started = Instant::now();
            let mut sink = 0.;
            for &x in &input {
                sink += reverb.next(x);
            }
            per_second = per_second.min(started.elapsed().as_secs_f64());
            std::hint::black_box(sink);
        }
        println!(
            "{room:?} convolver at {rate} Hz: {:.2} ms per second of audio ({:.2} % of one core)",
            per_second * 1e3,
            per_second * 100.
        );
    }
    Ok(())
}
