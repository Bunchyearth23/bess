//! Reproducible advanced-engine evidence. No audio device and no normalization.
//! advanced_physics <output-dir> [cpu|calibration|vvt|listening|all]
use bess::{
    automation_voice::AutomationVoice,
    engine_build::Headers,
    physical::engine::{Commands, Engine},
    realtime::DenormalGuard,
    scratch::{PRESETS, Scratch},
};
use std::{fmt::Write as _, path::Path, time::Instant};

fn preset(name: &str) -> Scratch {
    let mut s = Scratch {
        design: PRESETS.iter().find(|p| p.0 == name).unwrap().1,
        ..Default::default()
    };
    s.apply_design();
    s
}

fn cpu(dir: &Path) {
    let mut csv =
        String::from("engine,rate,coupled,min_us_per_output_sample,seconds_per_audio_second\n");
    for name in ["Inline-4", "V12 60°"] {
        for rate in [48_000, 96_000] {
            let mut minima = [f64::INFINITY; 2];
            for _ in 0..3 {
                for coupled in [false, true] {
                    let mut s = preset(name);
                    s.experimental.wave_coupling = coupled;
                    let mut e = Engine::new(&s, rate).unwrap();
                    let now = Instant::now();
                    for i in 0..rate {
                        let sample = e.next(Commands {
                            imposed_rpm: Some(1000. + 5000. * f64::from(i) / f64::from(rate)),
                            throttle: 0.6,
                            ..Default::default()
                        });
                        std::hint::black_box(sample);
                    }
                    assert!(!e.failed());
                    minima[usize::from(coupled)] =
                        minima[usize::from(coupled)].min(now.elapsed().as_secs_f64());
                }
            }
            for (coupled, elapsed) in minima.into_iter().enumerate() {
                writeln!(
                    csv,
                    "{name},{rate},{},{:.5},{elapsed:.5}",
                    coupled != 0,
                    elapsed * 1e6 / f64::from(rate)
                )
                .unwrap();
            }
        }
    }
    print!("{csv}");
    std::fs::write(dir.join("cpu.csv"), csv).unwrap();
}

fn calibration(dir: &Path) {
    let mut csv = String::from("engine,rpm,uncoupled_energy,coupled_energy,recommended_fixed_db\n");
    let (mut log_ratio, mut count) = (0., 0_u32);
    for name in ["Inline-4", "V8 cross-plane", "V12 60°"] {
        for rpm in [850., 2500., 4500., 6500.] {
            let mut energies = [0.; 2];
            for coupled in [false, true] {
                let mut s = preset(name);
                s.experimental.wave_coupling = coupled;
                let mut e = Engine::new(&s, 48000).unwrap();
                for i in 0..48000 {
                    let sample = e.next(Commands {
                        imposed_rpm: Some(rpm),
                        throttle: if rpm < 1000. { 0.05 } else { 1. },
                        ..Default::default()
                    });
                    assert!(!e.failed());
                    if i >= 24000 {
                        energies[usize::from(coupled)] += f64::from(sample.exhaust).powi(2);
                    }
                }
            }
            let db = 10. * (energies[0] / energies[1]).log10();
            log_ratio += db;
            count += 1;
            writeln!(
                csv,
                "{name},{rpm},{:.9},{:.9},{db:.5}",
                energies[0], energies[1]
            )
            .unwrap();
        }
    }
    writeln!(
        csv,
        "# fixed least-squares dB offset over equally weighted states: {:.5}",
        log_ratio / f64::from(count)
    )
    .unwrap();
    print!("{csv}");
    std::fs::write(dir.join("calibration.csv"), csv).unwrap();
}

fn vvt(dir: &Path) {
    let mut csv = String::from("cam,coupled,vvt,overlap_safe,rpm,brake_torque_nm,map_kpa\n");
    for cam in [0., 0.3, 0.7] {
        for coupled in [false, true] {
            for (vvt, safe) in [(false, false), (true, false), (true, true)] {
                let mut s = Scratch::default();
                s.build.headers = Headers::EqualLength;
                s.build.cam = cam;
                s.build.vvt = vvt;
                s.experimental.wave_coupling = coupled;
                s.experimental.vvt_overlap_safe = safe;
                let mut e = Engine::new(&s, 48000).unwrap();
                for rpm in [1500_f64, 2000., 2500., 3000., 3500., 4000., 5000., 6000.] {
                    let frames = (48000. * 120. / rpm * 20.).round() as usize;
                    let (mut torque, mut map, mut count) = (0., 0., 0.);
                    for i in 0..frames * 2 {
                        let sample = e.next(Commands {
                            imposed_rpm: Some(rpm),
                            throttle: 1.,
                            ..Default::default()
                        });
                        assert!(!e.failed());
                        if i >= frames {
                            torque += sample.torque_nm - e.friction_nm();
                            map += sample.map_pa * 0.001;
                            count += 1.;
                        }
                    }
                    writeln!(
                        csv,
                        "{cam},{coupled},{vvt},{safe},{rpm},{:.5},{:.5}",
                        torque / count,
                        map / count
                    )
                    .unwrap();
                }
            }
        }
    }
    std::fs::write(dir.join("vvt.csv"), &csv).unwrap();
    println!("VVT grid written ({} rows)", csv.lines().count() - 1);
}

fn listening(dir: &Path) {
    // One explicit presentation gain shared by every clip; never RMS matching.
    const LISTENING_GAIN: f32 = 0.25;
    let mut levels = String::from(
        "engine,mode,coupled_calibration_db,presentation_gain_db,rms_dbfs,peak_dbfs\n",
    );
    for name in ["Inline-4", "V12 60°"] {
        let mut reference = preset(name);
        reference.experimental.native_rate_acoustics = true;
        let measured = bess::physical::engine::calibrate_coupled_level(&reference).unwrap();
        std::fs::write(
            dir.join(format!(
                "{}-calibration.json",
                if name == "Inline-4" { "i4" } else { "v12" }
            )),
            serde_json::to_string_pretty(&measured).unwrap(),
        )
        .unwrap();
        for (mode, native, coupled, gain, safe) in [
            ("reference", false, false, 0., false),
            ("native", true, false, 0., false),
            ("coupled", true, true, 0., false),
            ("calibrated", true, true, measured.gain_db, false),
            ("vvt-safe", true, true, measured.gain_db, true),
        ] {
            let mut s = preset(name);
            s.experimental.native_rate_acoustics = native;
            s.experimental.wave_coupling = coupled;
            s.experimental.coupled_level_db = gain;
            s.experimental.vvt_overlap_safe = safe;
            let mut voice = AutomationVoice::new(48000, &s).unwrap();
            let path = dir.join(format!(
                "{}-{mode}.wav",
                if name == "Inline-4" { "i4" } else { "v12" }
            ));
            let mut audio = Vec::with_capacity(48000 * 6);
            for i in 0..48000 * 6 {
                let t = i as f64 / 48000.;
                let sample = voice.next_commands(Commands {
                    imposed_rpm: Some(if t < 1. {
                        850.
                    } else if t < 4. {
                        850. + (t - 1.) / 3. * 5000.
                    } else {
                        5850. - (t - 4.) * 2000.
                    }),
                    throttle: if t < 1. {
                        0.05
                    } else if t < 4. {
                        0.7
                    } else {
                        0.
                    },
                    overrun: if t >= 4. { 0.4 } else { 0. },
                    ..Default::default()
                });
                assert!(!voice.failed());
                audio.push(sample.exhaust * LISTENING_GAIN);
            }
            bess::render::write_pcm(&path, &audio).unwrap();
            let energy =
                audio.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / audio.len() as f64;
            let peak = audio.iter().map(|x| x.abs()).fold(0_f32, f32::max);
            writeln!(
                levels,
                "{name},{mode},{gain:.5},{:.5},{:.5},{:.5}",
                20. * LISTENING_GAIN.log10(),
                10. * energy.log10(),
                20. * peak.log10()
            )
            .unwrap();
            std::fs::write(
                path.with_extension("scratch.json"),
                serde_json::to_string_pretty(&s).unwrap(),
            )
            .unwrap();
        }
    }
    std::fs::write(dir.join("listening-levels.csv"), levels).unwrap();
}

fn lengths(dir: &Path) {
    let mut csv = String::from(
        "reference,map,cam,vvt,overlap_safe,scale,rpm_factor,error_pct,rms_mismatch,max_gain\n",
    );
    for (label, cam, vvt, safe) in [
        ("original", 0.3, true, false),
        ("overlap_safe", 0.3, true, true),
        ("fixed", 0.3, false, false),
        ("mild", 0., false, false),
    ] {
        let mut s = Scratch::default();
        s.build.headers = Headers::EqualLength;
        s.build.cam = cam;
        s.build.vvt = vvt;
        s.experimental.vvt_overlap_safe = safe;
        let base = bess::dyno::sweep_at_rate(&s, 31, 48000).unwrap();
        s.experimental.wave_coupling = true;
        let scales = [0.7_f32, 1., 1.4];
        let curves: Vec<_> = scales
            .iter()
            .map(|&k| {
                s.sound.primary_length_scale = k;
                bess::dyno::sweep_at_rate(&s, 31, 48000).unwrap()
            })
            .collect();
        let anechoic = bess::dyno::sweep_anechoic_reference(&s, 31, 48000).unwrap();
        for (reference_label, base) in [("0d", base), ("anechoic", anechoic)] {
            let ratio = |c: &bess::dyno::Curve| {
                c.torque_nm
                    .iter()
                    .zip(&base.torque_nm)
                    .map(|(a, b)| a / b - 1.)
                    .collect::<Vec<_>>()
            };
            let reference = ratio(&curves[1]);
            let rpm = &base.rpm;
            for k in [0, 2] {
                let response = ratio(&curves[k]);
                let (mut best, mut error) = (0., f64::INFINITY);
                for step in 0..=400 {
                    let factor = 0.4 * 6.25_f64.powf(f64::from(step) / 400.);
                    let (mut squared, mut count) = (0., 0);
                    for (&n, &v) in rpm.iter().zip(&response) {
                        let n = n * factor;
                        let i = rpm.partition_point(|&r| r < n);
                        if i > 0 && i < rpm.len() {
                            let w = reference[i - 1]
                                + (reference[i] - reference[i - 1]) * (n - rpm[i - 1])
                                    / (rpm[i] - rpm[i - 1]);
                            squared += (v - w).powi(2);
                            count += 1;
                        }
                    }
                    if count >= 10 && squared / f64::from(count) < error {
                        (best, error) = (factor, squared / f64::from(count));
                    }
                }
                let scale = f64::from(scales[k]);
                let gain = response.into_iter().fold(0_f64, f64::max);
                writeln!(
                csv,
                "{reference_label},{label},{cam},{vvt},{safe},{scale:.3},{best:.6},{:.4},{:.5},{gain:.5}",
                100. * (best / scale - 1.),
                error.sqrt()
            )
            .unwrap();
            }
        }
    }
    print!("{csv}");
    std::fs::write(dir.join("lengths.csv"), csv).unwrap();
}

fn rates(dir: &Path) {
    let mut csv = String::from("engine,coupled,rpm,torque_48k,torque_96k,relative_change_pct\n");
    for name in ["Inline-4", "V12 60°"] {
        for coupled in [false, true] {
            let mut s = preset(name);
            s.experimental.wave_coupling = coupled;
            let a = bess::dyno::sweep_at_rate(&s, 13, 48000).unwrap();
            let b = bess::dyno::sweep_at_rate(&s, 13, 96000).unwrap();
            for ((rpm, ta), tb) in a.rpm.iter().zip(&a.torque_nm).zip(&b.torque_nm) {
                writeln!(
                    csv,
                    "{name},{coupled},{rpm:.3},{ta:.5},{tb:.5},{:.5}",
                    100. * (tb / ta - 1.)
                )
                .unwrap();
            }
        }
    }
    std::fs::write(dir.join("rates.csv"), csv).unwrap();
}

fn main() {
    let _guard = DenormalGuard::enter();
    let mut args = std::env::args().skip(1);
    let directory = args
        .next()
        .unwrap_or_else(|| "output/advanced-physics-20261003".into());
    let mode = args.next().unwrap_or_else(|| "all".into());
    let dir = Path::new(&directory);
    std::fs::create_dir_all(dir).unwrap();
    for (name, run) in [
        ("cpu", cpu as fn(&Path)),
        ("calibration", calibration),
        ("vvt", vvt),
        ("listening", listening),
        ("lengths", lengths),
        ("rates", rates),
    ] {
        if name == mode || mode == "all" {
            run(dir);
        }
    }
}
