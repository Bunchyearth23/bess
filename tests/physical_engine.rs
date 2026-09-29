use bess::{
    physical::engine::{Commands, Engine, Sample},
    scratch::{PRESETS, Scratch},
};

fn preset(name: &str) -> Scratch {
    let mut scratch = Scratch {
        design: PRESETS.iter().find(|x| x.0 == name).unwrap().1,
        ..Default::default()
    };
    scratch.apply_design();
    scratch
}

fn finite(sample: Sample) {
    for value in [
        sample.rpm,
        sample.map_pa,
        sample.torque_nm,
        sample.heat_j,
        sample.correction_j,
        f64::from(sample.exhaust),
        f64::from(sample.intake),
        f64::from(sample.mechanical),
    ] {
        assert!(value.is_finite(), "nonfinite engine state: {sample:?}");
    }
}

#[test]
fn settled_idle_is_not_dominated_by_a_self_excited_high_frequency_tone() {
    for rate in [48_000, 96_000] {
        let slew = 1. - (-std::f64::consts::TAU * 300. / f64::from(rate)).exp();
        for name in ["Inline-4", "V8 cross-plane", "V12 60°"] {
            let mut engine = Engine::new(&preset(name), rate).unwrap();
            let (mut low1, mut low2, mut low_energy, mut energy, mut heat) = (0., 0., 0., 0., 0.);
            for frame in 0..rate * 2 {
                let s = engine.next(Commands {
                    imposed_rpm: Some(850.),
                    throttle: 0.05,
                    ..Default::default()
                });
                assert!(!engine.failed());
                let x = f64::from(s.exhaust);
                low1 += slew * (x - low1);
                low2 += slew * (low1 - low2);
                if frame >= rate {
                    energy += x * x;
                    low_energy += low2 * low2;
                    heat += s.heat_j;
                }
            }
            assert!(energy > 1e-8 && heat > 100., "{name}: silent/no combustion");
            // A broad spectral guard, independent of listening gain and exact
            // PCM. The rejected delayed-orifice loop puts >85% of idle energy
            // in a 0.5–2 kHz whistle instead of the engine's low firing orders.
            assert!(
                low_energy / energy > 0.2,
                "{name}/{rate}: low-order energy fraction {}",
                low_energy / energy
            );
        }
    }
}

fn harmonic_power(samples: &[f32], frequency: f64) -> f64 {
    let step = std::f64::consts::TAU * frequency / 48000.;
    let (mut re, mut im) = (0., 0.);
    for (i, &sample) in samples.iter().enumerate() {
        let (s, c) = (step * i as f64).sin_cos();
        re += f64::from(sample) * c;
        im += f64::from(sample) * s;
    }
    re * re + im * im
}

#[test]
fn i4_v8_v12_produce_finite_combustion_without_material_numerical_heating() {
    for name in ["Inline-4", "V8 cross-plane", "V12 60°"] {
        let mut engine = Engine::new(&preset(name), 48000).unwrap();
        let commands = Commands {
            imposed_rpm: Some(1200.),
            throttle: 0.35,
            ..Default::default()
        };
        let (mut heat, mut correction, mut energy) = (0., 0., 0.);
        let mut samples = Vec::with_capacity(48000);
        for i in 0..72000 {
            let s = engine.next(commands);
            finite(s);
            assert!(!engine.failed(), "{name} failed at {i}");
            if i >= 24000 {
                heat += s.heat_j;
                correction += s.correction_j.abs();
                energy += f64::from(s.exhaust).powi(2);
                samples.push(s.exhaust);
            }
        }
        assert!(heat > 1000., "{name}: no meaningful combustion, {heat} J");
        assert!(
            correction < heat * 0.01,
            "{name}: correction {correction} / heat {heat}"
        );
        assert!(energy > 1e-8, "{name}: silent exhaust");
        let order = f64::from(preset(name).design.cylinders) / 2.;
        let fundamental = harmonic_power(&samples, order * 20.);
        let adjacent = harmonic_power(&samples, (order - 0.5) * 20.)
            .max(harmonic_power(&samples, (order + 0.5) * 20.));
        assert!(
            fundamental > adjacent * 4.,
            "{name}: expected firing order {order} is absent: {fundamental} vs {adjacent}"
        );
    }
}

#[test]
fn identical_seed_and_commands_produce_identical_multicylinder_samples() {
    let scratch = preset("V8 cross-plane");
    let mut a = Engine::new(&scratch, 48000).unwrap();
    let mut b = Engine::new(&scratch, 48000).unwrap();
    for i in 0..36000 {
        let c = Commands {
            imposed_rpm: Some(1200. + i as f64 / 120.),
            throttle: if i < 18000 { 0.3 } else { 0.1 },
            ..Default::default()
        };
        let x = a.next(c);
        let y = b.next(c);
        finite(x);
        finite(y);
        assert!(!a.failed() && !b.failed());
        assert_eq!(x.exhaust.to_bits(), y.exhaust.to_bits());
        assert_eq!(x.heat_j.to_bits(), y.heat_j.to_bits());
        assert_eq!(x.map_pa.to_bits(), y.map_pa.to_bits());
    }
}

#[test]
fn injection_cut_stops_combustion_while_physical_pumping_remains() {
    let mut engine = Engine::new(&preset("Inline-4"), 48000).unwrap();
    let mut cut_samples = 0;
    let mut before_heat = 0.;
    let mut cut_energy = 0.;
    let mut cut_misfires = None;
    for i in 0..48000 {
        let s = engine.next(Commands {
            imposed_rpm: Some(3000.),
            throttle: if i < 24000 { 0.4 } else { 0. },
            ..Default::default()
        });
        finite(s);
        assert!(!engine.failed());
        if i < 24000 {
            before_heat += s.heat_j;
        }
        if i > 26400 {
            assert!(s.fuel_cut, "DFCO absent after 50ms");
            assert_eq!(s.heat_j, 0.);
            assert_eq!(s.misfires, *cut_misfires.get_or_insert(s.misfires));
            cut_samples += 1;
            cut_energy += f64::from(s.exhaust).powi(2);
        }
    }
    assert!(before_heat > 100.);
    assert!(cut_samples > 20000);
    assert!(cut_energy > 1e-12);
}

#[test]
fn repeated_missing_cylinders_in_firing_order_reduce_released_heat() {
    let good = preset("Inline-4");
    let mut wrong = good.clone();
    wrong.design.firing_order[..4].copy_from_slice(&[1, 1, 1, 1]);
    wrong.apply_design();
    let mut a = Engine::new(&good, 48000).unwrap();
    let mut b = Engine::new(&wrong, 48000).unwrap();
    let (mut heat_a, mut heat_b) = (0., 0.);
    for i in 0..48000 {
        let command = Commands {
            imposed_rpm: Some(1200.),
            throttle: 0.35,
            ..Default::default()
        };
        let x = a.next(command);
        let y = b.next(command);
        finite(x);
        finite(y);
        assert!(!a.failed() && !b.failed());
        if i > 24000 {
            heat_a += x.heat_j;
            heat_b += y.heat_j;
        }
    }
    assert!(
        heat_b < heat_a * 0.6,
        "wrong wiring heat {heat_b} vs normal {heat_a}"
    );
}

#[test]
fn crank_free_running_and_accessory_load_are_finite_and_load_sensitive() {
    let scratch = preset("Inline-4");
    let mut a = Engine::new(&scratch, 48000).unwrap();
    let mut b = Engine::new(&scratch, 48000).unwrap();
    let (mut rpm_a, mut rpm_b) = (0., 0.);
    for i in 0..48000 {
        let c = Commands {
            imposed_rpm: if i < 24000 { Some(1200.) } else { None },
            throttle: 0.05,
            ..Default::default()
        };
        let x = a.next(c);
        let y = b.next(Commands {
            load_nm: if i < 24000 { 0. } else { 22. },
            ..c
        });
        finite(x);
        finite(y);
        assert!(!a.failed() && !b.failed(), "free crank failed at {i}");
        if i > 36000 {
            rpm_a += x.rpm;
            rpm_b += y.rpm;
        }
    }
    assert!(
        rpm_b < rpm_a,
        "22Nm load must reduce finite-time speed: {rpm_b} vs {rpm_a}"
    );
}

#[test]
fn idle_controller_settles_near_target_without_cyclic_fuel_cut() {
    let scratch = preset("Inline-4");
    let target = f64::from(scratch.idle_rpm);
    let mut engine = Engine::new(&scratch, 48000).unwrap();
    let (mut rpm_sum, mut samples, mut cuts) = (0., 0_u32, 0_u32);
    for i in 0..408000 {
        let s = engine.next(Commands {
            imposed_rpm: if i < 24000 { Some(1200.) } else { None },
            throttle: 0.,
            ..Default::default()
        });
        assert!(!engine.failed(), "idle integration failed at {i}");
        finite(s);
        if i >= 360000 {
            rpm_sum += s.rpm;
            samples += 1;
            cuts += u32::from(s.fuel_cut);
        }
    }
    let mean = rpm_sum / f64::from(samples);
    assert!(
        (mean - target).abs() < target * 0.1,
        "idle mean {mean:.2} vs target {target:.2}"
    );
    assert_eq!(cuts, 0, "idle is cycling through overrun cutoff");
}

#[test]
fn throttle_changes_manifold_filling_and_combustion_energy() {
    let scratch = preset("Inline-4");
    let mut closed = Engine::new(&scratch, 48000).unwrap();
    let mut open = Engine::new(&scratch, 48000).unwrap();
    let (mut low_heat, mut high_heat, mut low_map, mut high_map) = (0., 0., 0., 0.);
    for i in 0..48000 {
        let base = Commands {
            imposed_rpm: Some(1200.),
            overrun: 0.,
            ..Default::default()
        };
        let a = closed.next(Commands {
            throttle: 0.01,
            ..base
        });
        let b = open.next(Commands {
            throttle: 0.5,
            ..base
        });
        finite(a);
        finite(b);
        assert!(!closed.failed() && !open.failed());
        if i >= 24000 {
            low_heat += a.heat_j;
            high_heat += b.heat_j;
            low_map += a.map_pa;
            high_map += b.map_pa;
        }
    }
    assert!(
        high_map - low_map > 20000. * 24000.,
        "throttle failed to change filling"
    );
    assert!(high_heat > low_heat * 2., "heat {high_heat} vs {low_heat}");
}

#[test]
fn turbo_spools_and_adds_manifold_pressure_under_open_throttle() {
    let mut scratch = preset("Inline-4");
    scratch.build.aspiration = bess::engine_build::Aspiration::Turbo;
    scratch.build.boost_bar = 0.8;
    let mut engine = Engine::new(&scratch, 48000).unwrap();
    let (mut mean_map, mut points, mut max_turbo) = (0., 0_u32, 0_f64);
    for i in 0..96000 {
        let output = engine.next(Commands {
            imposed_rpm: Some(3000.),
            throttle: 1.,
            ..Default::default()
        });
        finite(output);
        assert!(!engine.failed(), "turbo failed at {i}");
        max_turbo = max_turbo.max(output.turbo_rpm);
        if i >= 72000 {
            mean_map += output.map_pa;
            points += 1;
        }
    }
    assert!(max_turbo > 10000., "turbo never spooled: {max_turbo}");
    assert!(
        mean_map / f64::from(points) > 110000.,
        "turbo MAP only {}Pa",
        mean_map / f64::from(points)
    );
}

#[test]
fn intake_and_contacts_have_cycle_texture_without_free_running_noise() {
    use bess::physical::engine::{Commands, Engine};
    let mut engine = Engine::new(&bess::scratch::Scratch::default(), 48000).unwrap();
    let command = Commands {
        imposed_rpm: Some(3000.),
        throttle: 0.7,
        ..Default::default()
    };
    let mut prior = [[0.; 2]; 1920];
    let mut difference = [0.; 2];
    let mut energy = [0.; 2];
    for i in 0..96000 {
        let s = engine.next(command);
        assert!(!engine.failed());
        for (stem, x) in [s.intake, s.mechanical].into_iter().enumerate() {
            assert!(x.is_finite());
            if i >= 48000 {
                difference[stem] += (x - prior[i % 1920][stem]).powi(2);
                energy[stem] += x * x;
            }
            prior[i % 1920][stem] = x;
        }
    }
    for i in 0..2 {
        assert!(
            energy[i] > 1e-9 && difference[i] / energy[i] > 0.15,
            "stem {i} repeats identical cycles"
        );
    }
}

#[test]
fn combustion_pressure_rise_makes_mechanics_load_dependent() {
    let energy = |throttle: f64| {
        let mut engine = Engine::new(&Scratch::default(), 48000).unwrap();
        let command = Commands {
            imposed_rpm: Some(3000.),
            throttle,
            ..Default::default()
        };
        let mut energy = 0.;
        for i in 0..96000 {
            let s = engine.next(command);
            finite(s);
            if i >= 48000 {
                energy += f64::from(s.mechanical).powi(2);
            }
        }
        assert!(!engine.failed());
        energy
    };
    let loaded = energy(0.7);
    // Valve contacts are identical at equal RPM; only combustion differs.
    for throttle in [0.1, 0.] {
        let ratio = loaded / energy(throttle);
        assert!(ratio > 1.3, "throttle {throttle}: energy ratio {ratio}");
    }
}
