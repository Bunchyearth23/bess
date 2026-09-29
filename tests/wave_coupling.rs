//! X-017 prototype: `experimental.wave_coupling` (passive wave/valve junction).
use bess::{
    dyno,
    engine_build::Headers,
    physical::engine::{Commands, Engine},
    scratch::{PRESETS, Scratch},
};
use rustfft::{FftPlanner, num_complex::Complex};

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

#[test]
fn switch_defaults_off_and_older_projects_load_it_off() {
    let scratch = Scratch::default();
    assert!(!scratch.experimental.wave_coupling);
    let mut json = serde_json::to_value(&scratch).unwrap();
    let removed = json["experimental"]
        .as_object_mut()
        .unwrap()
        .remove("wave_coupling");
    assert_eq!(removed, Some(serde_json::Value::Bool(false)));
    let loaded: Scratch = serde_json::from_value(json).unwrap();
    assert!(!loaded.experimental.wave_coupling);
}

/// Share of Hann-windowed exhaust power in [lo, hi) Hz.
fn shares(x: &[f64], rate: f64, bands: &[(f64, f64)]) -> Vec<f64> {
    let n = x.len();
    let mut buf: Vec<_> = x
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let w = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
            Complex::new(v * w, 0.)
        })
        .collect();
    FftPlanner::new().plan_fft_forward(n).process(&mut buf);
    let power: Vec<f64> = buf[1..n / 2].iter().map(|c| c.norm_sqr()).collect();
    let total: f64 = power.iter().sum();
    bands
        .iter()
        .map(|&(lo, hi)| {
            power
                .iter()
                .enumerate()
                .filter(|(k, _)| (lo..hi).contains(&((k + 1) as f64 * rate / n as f64)))
                .map(|(_, p)| p)
                .sum::<f64>()
                / total
        })
        .collect()
}

#[test]
fn coupled_idle_is_not_dominated_by_a_self_excited_tone() {
    // The rejected delayed coupling put 85–93 % of idle exhaust power in
    // 0.5–2 kHz; the uncoupled engine has ≈0.2–0.4 % there, > 92 % below 250 Hz.
    std::thread::scope(|s| {
        for rate in [48_000_u32, 96_000] {
            for name in ENGINES {
                s.spawn(move || {
                    let mut engine = Engine::new(&preset(name, true), rate).unwrap();
                    let (mut x, mut heat) = (Vec::new(), 0.);
                    for frame in 0..rate as usize * 4 {
                        let s = engine.next(Commands {
                            imposed_rpm: Some(850.),
                            throttle: 0.05,
                            ..Default::default()
                        });
                        assert!(!engine.failed(), "{name}/{rate} failed");
                        if frame >= rate as usize {
                            x.push(f64::from(s.exhaust));
                            heat += s.heat_j;
                        }
                    }
                    let share = shares(&x, f64::from(rate), &[(0., 250.), (500., 2000.)]);
                    println!(
                        "{name}/{rate}: <250 Hz {:.4}, 0.5–2 kHz {:.4}",
                        share[0], share[1]
                    );
                    assert!(heat > 100., "{name}: no combustion");
                    assert!(share[0] > 0.85, "{name}/{rate}: low share {}", share[0]);
                    assert!(
                        share[1] < 0.02,
                        "{name}/{rate}: 0.5–2 kHz share {}",
                        share[1]
                    );
                });
            }
        }
    });
}

#[test]
fn coupled_engines_keep_numerical_heating_small_and_are_deterministic() {
    for name in ENGINES {
        let scratch = preset(name, true);
        let (mut a, mut b) = (
            Engine::new(&scratch, 48000).unwrap(),
            Engine::new(&scratch, 48000).unwrap(),
        );
        let (mut heat, mut correction) = (0., 0.);
        for i in 0..72000 {
            let command = Commands {
                imposed_rpm: Some(1200. + f64::from(i) / 40.),
                throttle: if i < 48000 { 0.35 } else { 1. },
                ..Default::default()
            };
            let (x, y) = (a.next(command), b.next(command));
            assert!(!a.failed() && x.exhaust.is_finite() && x.torque_nm.is_finite());
            assert_eq!(x.exhaust.to_bits(), y.exhaust.to_bits());
            assert_eq!(x.heat_j.to_bits(), y.heat_j.to_bits());
            assert_eq!(x.map_pa.to_bits(), y.map_pa.to_bits());
            if i >= 24000 {
                heat += x.heat_j;
                correction += x.correction_j.abs();
            }
        }
        assert!(heat > 1000., "{name}: {heat} J");
        assert!(correction < heat * 0.01, "{name}: {correction} / {heat}");
    }
}

#[test]
fn coupled_free_idle_settles_near_target_without_fuel_cut() {
    let scratch = preset("Inline-4", true);
    let target = f64::from(scratch.idle_rpm);
    let mut engine = Engine::new(&scratch, 48000).unwrap();
    let (mut rpm, mut samples, mut cuts) = (0., 0_u32, 0_u32);
    for i in 0..408000 {
        let s = engine.next(Commands {
            imposed_rpm: (i < 24000).then_some(1200.),
            throttle: 0.,
            ..Default::default()
        });
        assert!(!engine.failed() && s.exhaust.is_finite(), "failed at {i}");
        if i >= 360000 {
            rpm += s.rpm;
            samples += 1;
            cuts += u32::from(s.fuel_cut);
        }
    }
    let mean = rpm / f64::from(samples);
    assert!(
        (mean - target).abs() < target * 0.1,
        "idle {mean} vs {target}"
    );
    assert_eq!(cuts, 0);
}

#[test]
fn coupled_extreme_lengths_and_rates_stay_finite_and_bounded() {
    let mut jobs = Vec::new();
    for name in ["Inline-4", "V12 60°"] {
        for scale in [0.5_f32, 2.] {
            for rate in [8000_u32, 48000, 96000, 192000] {
                for (rpm, throttle) in [(850., 0.05), (7000., 1.)] {
                    jobs.push((name, scale, rate, rpm, throttle));
                }
            }
        }
    }
    std::thread::scope(|s| {
        for &(name, scale, rate, rpm, throttle) in &jobs {
            s.spawn(move || {
                let energy = |coupled| {
                    let mut scratch = preset(name, coupled);
                    scratch.build.headers = Headers::EqualLength;
                    scratch.sound.primary_length_scale = scale;
                    let mut engine = Engine::new(&scratch, rate).unwrap();
                    let mut energy = 0.;
                    for frame in 0..rate as usize / 2 {
                        let s = engine.next(Commands {
                            imposed_rpm: Some(rpm),
                            throttle,
                            ..Default::default()
                        });
                        assert!(
                            !engine.failed() && s.exhaust.is_finite() && s.torque_nm.is_finite()
                        );
                        if frame >= rate as usize / 4 {
                            energy += f64::from(s.exhaust).powi(2);
                        }
                    }
                    energy
                };
                let db = 10. * (energy(true) / energy(false)).log10();
                println!("{name} x{scale} {rate} Hz {rpm} rpm: coupled/uncoupled {db:+.2} dB");
                assert!(db.abs() < 12., "{name} x{scale} {rate} {rpm}: {db:+.2} dB");
            });
        }
    });
}

#[test]
fn primary_length_moves_the_tuned_torque_peak_as_one_over_length() {
    let sweep = |scale: f32, coupled: bool| {
        let mut scratch = Scratch::default();
        scratch.build.headers = Headers::EqualLength;
        scratch.sound.primary_length_scale = scale;
        scratch.experimental.wave_coupling = coupled;
        dyno::sweep(&scratch, 31).unwrap()
    };
    // Uncoupled, length is sound only (X-021): the dyno ignores it.
    let base = sweep(0.7, false);
    assert_eq!(base, sweep(1.4, false));
    let scales = [0.7_f32, 1., 1.4];
    let curves: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = scales
            .iter()
            .map(|&k| s.spawn(move || sweep(k, true)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    // Wave-induced torque ratio against the uncoupled curve. It has several
    // lobes (successive reflections), so compare whole curves: find the rpm
    // factor f with ratio_k(N) ≈ ratio_1(N·f). Tuned rpm ∝ 1/L predicts f = k.
    let ratio = |c: &dyno::Curve| -> Vec<f64> {
        c.torque_nm
            .iter()
            .zip(&base.torque_nm)
            .map(|(a, b)| a / b - 1.)
            .collect()
    };
    let rpm = &base.rpm;
    let reference = ratio(&curves[1]);
    let at = |r: &[f64], x: f64| {
        let i = rpm.partition_point(|&v| v < x);
        (i > 0 && i < rpm.len())
            .then(|| r[i - 1] + (r[i] - r[i - 1]) * (x - rpm[i - 1]) / (rpm[i] - rpm[i - 1]))
    };
    for (k, curve) in [(0, &curves[0]), (2, &curves[2])] {
        let r = ratio(curve);
        let (mut best, mut error) = (0., f64::INFINITY);
        for step in 0..=400 {
            let f = 0.4 * 6.25_f64.powf(f64::from(step) / 400.);
            let pairs: Vec<f64> = rpm
                .iter()
                .zip(&r)
                .filter_map(|(&n, v)| at(&reference, n * f).map(|w| (v - w).powi(2)))
                .collect();
            let mean = pairs.iter().sum::<f64>() / pairs.len().max(1) as f64;
            if pairs.len() >= 10 && mean < error {
                (best, error) = (f, mean);
            }
        }
        let expected = f64::from(scales[k]);
        let gain = r.iter().fold(0_f64, |m, v| m.max(*v));
        println!(
            "x{expected:.1}: rpm factor {best:.3} (1/length: {expected:.3}), rms mismatch {:.3}, max gain {gain:.3}",
            error.sqrt()
        );
        assert!(gain > 0.1, "x{expected}: no tuning lobe");
        // 15 %: the ratio also holds a length-independent part, Blair's mass
        // flux lowering each primary's own-pulse backpressure at high flow
        // (X-028), which a pure rpm shift cannot fit. Linear port: −7…−9 %;
        // ratio superposition with mass flux: −11…−13 %.
        assert!(
            (best / expected - 1.).abs() < 0.15,
            "x{expected}: factor {best}"
        );
        assert!(
            error.sqrt() < 0.5 * gain,
            "x{expected}: shapes do not match"
        );
    }
}
