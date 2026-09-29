//! X-017 evidence: passive wave/valve junction (`experimental.wave_coupling`).
//!   cargo run --release --example wave_coupling -- <out_dir> [idle|dyno|cpu|all]
//! idle: raw exhaust spectra at 850 rpm / 0.05, seconds 2–8, off vs on.
//! dyno: WOT torque vs primary length, coupling on (and off as reference).
//! cpu:  engine cost per second of audio, off vs on.
use bess::{
    dyno,
    engine_build::Headers,
    physical::engine::{Commands, Engine},
    realtime::DenormalGuard,
    scratch::{PRESETS, Scratch},
};
use rustfft::{FftPlanner, num_complex::Complex};
use std::{fmt::Write as _, path::PathBuf, time::Instant};

fn preset(name: &str, coupled: bool) -> Scratch {
    let mut scratch = Scratch {
        design: PRESETS.iter().find(|x| x.0 == name).unwrap().1,
        ..Default::default()
    };
    scratch.apply_design();
    scratch.experimental.wave_coupling = coupled;
    scratch
}

const ENGINES: [&str; 3] = ["Inline-4", "V8 cross-plane", "V12 60°"];

fn idle(out: &mut String) {
    writeln!(
        out,
        "engine,rate,coupled,rms_dbfs,below_250,band_500_2000,peak_hz,correction_over_heat,failed"
    )
    .unwrap();
    let jobs: Vec<_> = ENGINES
        .iter()
        .flat_map(|&n| [48_000_u32, 96_000].map(move |r| (n, r)))
        .flat_map(|(n, r)| [false, true].map(move |c| (n, r, c)))
        .collect();
    let rows: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(name, rate, coupled)| {
                s.spawn(move || {
                    let _d = DenormalGuard::enter();
                    let mut engine = Engine::new(&preset(name, coupled), rate).unwrap();
                    let mut x = Vec::new();
                    let (mut heat, mut correction) = (0., 0.);
                    for frame in 0..rate as usize * 8 {
                        let s = engine.next(Commands {
                            imposed_rpm: Some(850.),
                            throttle: 0.05,
                            ..Default::default()
                        });
                        if frame >= rate as usize * 2 {
                            x.push(f64::from(s.exhaust));
                            heat += s.heat_j;
                            correction += s.correction_j;
                        }
                    }
                    let n = x.len();
                    let rms = (x.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
                    let mut buf: Vec<_> = x
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            // Hann window
                            let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
                            Complex::new(v * w, 0.)
                        })
                        .collect();
                    FftPlanner::new().plan_fft_forward(n).process(&mut buf);
                    let hz = f64::from(rate) / n as f64;
                    let power: Vec<f64> = buf[..n / 2].iter().map(|c| c.norm_sqr()).collect();
                    let total: f64 = power[1..].iter().sum();
                    let band = |lo: f64, hi: f64| {
                        power
                            .iter()
                            .enumerate()
                            .filter(|(k, _)| (lo..hi).contains(&(*k as f64 * hz)))
                            .map(|(_, p)| p)
                            .sum::<f64>()
                            / total
                    };
                    let peak = (1..power.len())
                        .max_by(|&a, &b| power[a].total_cmp(&power[b]))
                        .unwrap() as f64
                        * hz;
                    format!(
                        "{name},{rate},{coupled},{:.2},{:.4},{:.4},{peak:.2},{:.2e},{}",
                        20. * rms.log10(),
                        band(1., 250.),
                        band(500., 2000.),
                        correction / heat,
                        engine.failed()
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for row in rows {
        println!("{row}");
        writeln!(out, "{row}").unwrap();
    }
}

fn dyno_curves(out: &mut String) {
    let scales = [0.5_f32, 0.7, 1.0, 1.4, 2.0];
    let mut jobs = Vec::new();
    for headers in [Headers::EqualLength, Headers::CastManifold] {
        jobs.push((headers, 1., false));
        for s in scales {
            jobs.push((headers, s, true));
        }
    }
    let curves: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(headers, scale, coupled)| {
                s.spawn(move || {
                    let _d = DenormalGuard::enter();
                    let mut scratch = Scratch::default();
                    scratch.build.headers = headers;
                    scratch.sound.primary_length_scale = scale;
                    scratch.experimental.wave_coupling = coupled;
                    let start = Instant::now();
                    let curve = dyno::sweep(&scratch, 31).unwrap();
                    (curve, start.elapsed().as_secs_f64())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    writeln!(out, "headers,primary_scale,coupled,rpm,torque_nm,map_kpa").unwrap();
    for ((headers, scale, coupled), (curve, seconds)) in jobs.iter().zip(&curves) {
        println!(
            "{headers:?} scale {scale} coupled {coupled}: peak {:.1} Nm @ {:.0} rpm ({seconds:.1} s)",
            curve.peak_torque.0, curve.peak_torque.1
        );
        for i in 0..curve.rpm.len() {
            writeln!(
                out,
                "{headers:?},{scale},{coupled},{:.0},{:.3},{:.2}",
                curve.rpm[i], curve.torque_nm[i], curve.map_kpa[i]
            )
            .unwrap();
        }
    }
    // Wave-induced torque ratio against the uncoupled curve, and the rpm
    // scaling that best maps one length's ratio curve onto another's.
    for (h, headers) in [Headers::EqualLength, Headers::CastManifold]
        .iter()
        .enumerate()
    {
        let base = &curves[h * 6].0;
        let ratio = |k: usize| -> Vec<f64> {
            let c = &curves[h * 6 + 1 + k].0;
            c.torque_nm
                .iter()
                .zip(&base.torque_nm)
                .map(|(a, b)| a / b - 1.)
                .collect()
        };
        let rpm = &base.rpm;
        let interp = |r: &[f64], x: f64| -> Option<f64> {
            let i = rpm.partition_point(|&v| v < x);
            (i > 0 && i < rpm.len())
                .then(|| r[i - 1] + (r[i] - r[i - 1]) * (x - rpm[i - 1]) / (rpm[i] - rpm[i - 1]))
        };
        let reference = 2; // scale 1.0
        let r0 = ratio(reference);
        for k in 0..scales.len() {
            let r = ratio(k);
            // Find factor f with r_k(N) ≈ r_ref(N·f): tuned rpm ∝ 1/L predicts f = L_k/L_ref.
            let (mut best, mut best_err) = (1., f64::INFINITY);
            for step in 0..=400 {
                let f = 0.4 * (6.25_f64).powf(step as f64 / 400.); // 0.4..2.5
                let (mut err, mut count) = (0., 0);
                for (i, &n) in rpm.iter().enumerate() {
                    if let Some(v) = interp(&r0, n * f) {
                        err += (r[i] - v).powi(2);
                        count += 1;
                    }
                }
                if count >= 10 && err / f64::from(count) < best_err {
                    best_err = err / f64::from(count);
                    best = f;
                }
            }
            let line = format!(
                "{headers:?}: scale {} ratio curve maps onto scale 1 with rpm factor {best:.3} (1/length law: {:.3}), rms mismatch {:.4}, max |ratio| {:.3}",
                scales[k],
                scales[k] / scales[reference],
                best_err.sqrt(),
                r.iter().fold(0_f64, |m, v| m.max(v.abs()))
            );
            println!("{line}");
            writeln!(out, "# {line}").unwrap();
        }
    }
}

/// Single-thread cost, 1000→6000 rpm ramp at throttle 0.6 over 6 s. Off and
/// on are interleaved five times and the minimum kept (shared build host).
fn cpu(out: &mut String) {
    writeln!(out, "engine,rate,coupled,min_seconds_per_audio_second").unwrap();
    for name in ["Inline-4", "V12 60°"] {
        for rate in [48_000_u32, 96_000] {
            let mut best = [f64::INFINITY; 2];
            for _ in 0..5 {
                for coupled in [false, true] {
                    let _d = DenormalGuard::enter();
                    let mut engine = Engine::new(&preset(name, coupled), rate).unwrap();
                    let command = |i: usize| Commands {
                        imposed_rpm: Some(1000. + 5000. * (i as f64 / (f64::from(rate) * 6.))),
                        throttle: 0.6,
                        ..Default::default()
                    };
                    let start = Instant::now();
                    let mut sink = 0_f32;
                    for i in 0..rate as usize * 6 {
                        sink += engine.next(command(i)).exhaust;
                    }
                    assert!(sink.is_finite() && !engine.failed());
                    let cost = start.elapsed().as_secs_f64() / 6.;
                    best[usize::from(coupled)] = best[usize::from(coupled)].min(cost);
                }
            }
            for coupled in [false, true] {
                let row = format!("{name},{rate},{coupled},{:.4}", best[usize::from(coupled)]);
                println!("{row}");
                writeln!(out, "{row}").unwrap();
            }
            let line = format!(
                "# {name} {rate}: coupling costs {:+.1} % ({:+.4} s per audio second)",
                100. * (best[1] / best[0] - 1.),
                best[1] - best[0]
            );
            println!("{line}");
            writeln!(out, "{line}").unwrap();
        }
    }
}

/// Sensitivity of coupled WOT torque to the acoustic sample rate (the dyno
/// runs its acoustics at 8 kHz; gas substeps stay at 96 kHz for every rate).
fn rate(out: &mut String) {
    writeln!(out, "rpm,rate,coupled,brake_torque_nm").unwrap();
    let jobs: Vec<_> = [2000., 3924., 6000.]
        .into_iter()
        .flat_map(|n| [8000_u32, 16000, 48000, 96000].map(move |r| (n, r)))
        .flat_map(|(n, r)| [false, true].map(move |c| (n, r, c)))
        .collect();
    let rows: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(rpm, rate, coupled)| {
                s.spawn(move || {
                    let _d = DenormalGuard::enter();
                    let mut scratch = Scratch::default();
                    scratch.build.headers = Headers::EqualLength;
                    scratch.experimental.wave_coupling = coupled;
                    let mut engine = Engine::new(&scratch, rate).unwrap();
                    let cycle = (120. / rpm * f64::from(rate)).round() as usize;
                    let (mut torque, mut count) = (0., 0);
                    for i in 0..cycle * 40 {
                        let s = engine.next(Commands {
                            imposed_rpm: Some(rpm),
                            throttle: 1.,
                            ..Default::default()
                        });
                        if i >= cycle * 20 {
                            torque += s.torque_nm - engine.friction_nm();
                            count += 1;
                        }
                    }
                    format!("{rpm},{rate},{coupled},{:.3}", torque / f64::from(count))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for row in rows {
        println!("{row}");
        writeln!(out, "{row}").unwrap();
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "out".into()));
    let mode = args.next().unwrap_or_else(|| "all".into());
    std::fs::create_dir_all(&dir).unwrap();
    for (name, run) in [
        ("idle", idle as fn(&mut String)),
        ("dyno", dyno_curves),
        ("cpu", cpu),
        ("rate", rate),
    ] {
        if mode == "all" || mode == name {
            let mut out = String::new();
            run(&mut out);
            std::fs::write(dir.join(format!("wave_coupling_{name}.csv")), out).unwrap();
        }
    }
}
