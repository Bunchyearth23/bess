//! Public contracts of the opt-in advanced modes, independent of local cars.
use bess::{
    automation_voice::AutomationVoice,
    physical::engine::{Commands, Engine},
    scratch::{PRESETS, Scratch},
};

fn v8() -> Scratch {
    let mut s = Scratch {
        design: PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1,
        ..Default::default()
    };
    s.apply_design();
    s.experimental.wave_coupling = true;
    s
}

#[test]
fn bank_delay_gain_and_coupled_calibration_never_change_physical_state() {
    let mut reference = v8();
    reference.design.bank_delay_ms = 0.;
    let mut observed = reference.clone();
    observed.design.bank_delay_ms = 2.5;
    observed.design.bank_gain_db = -4.;
    observed.experimental.coupled_level_db = 3.;
    assert_eq!(reference.life().seed, observed.life().seed);
    assert_eq!(bess::dyno::key(&reference), bess::dyno::key(&observed));
    let (mut a, mut b) = (
        Engine::new(&reference, 48000).unwrap(),
        Engine::new(&observed, 48000).unwrap(),
    );
    let mut changed = 0.;
    for i in 0..24000 {
        let c = Commands {
            imposed_rpm: Some(2000. + f64::from(i) / 12.),
            throttle: 0.7,
            ..Default::default()
        };
        let (x, y) = (a.next(c), b.next(c));
        assert!(!a.failed() && !b.failed());
        assert_eq!(x.torque_nm.to_bits(), y.torque_nm.to_bits());
        assert_eq!(x.map_pa.to_bits(), y.map_pa.to_bits());
        assert_eq!(x.heat_j.to_bits(), y.heat_j.to_bits());
        assert_eq!(x.fuel_injected_kg.to_bits(), y.fuel_injected_kg.to_bits());
        changed += f64::from((x.exhaust - y.exhaust).abs());
    }
    assert!(changed > 1.);
    // Installing another observation setting must preserve the running gas.
    let mut prepared = Engine::new(&observed, 48000).unwrap();
    let before = a.state();
    assert!(a.apply_sound_tuning(&mut prepared));
    assert_eq!(a.state().rpm.to_bits(), before.rpm.to_bits());
    assert_eq!(a.state().map_pa.to_bits(), before.map_pa.to_bits());
}

#[test]
fn native_acoustics_uses_the_same_commands_and_physical_substeps() {
    let mut s = Scratch::default();
    s.experimental.native_rate_acoustics = true;
    assert_eq!(s.synthesis_rate(48000), 48000);
    let mut voice = AutomationVoice::new(48000, &s).unwrap();
    let mut direct = Engine::new(&s, 48000).unwrap();
    for i in 0..8000 {
        let commands = Commands {
            imposed_rpm: Some(1200. + f64::from(i) / 10.),
            throttle: 0.45,
            ..Default::default()
        };
        let stem = voice.next_commands(commands);
        let sample = direct.next(commands);
        assert_eq!(stem.exhaust.to_bits(), sample.exhaust.to_bits());
        assert_eq!(stem.intake.to_bits(), sample.intake.to_bits());
        assert_eq!(stem.mechanical.to_bits(), sample.mechanical.to_bits());
        assert_eq!(
            voice.state().torque_nm.to_bits(),
            sample.torque_nm.to_bits()
        );
    }
    assert!(!voice.failed() && !direct.failed());
}

#[test]
fn optional_physics_defaults_off_and_old_projects_keep_that_policy() {
    let mut value = serde_json::to_value(Scratch::default()).unwrap();
    let experimental = value["experimental"].as_object_mut().unwrap();
    for key in [
        "native_rate_acoustics",
        "primary_1d",
        "coupled_level_db",
        "vvt_overlap_safe",
    ] {
        experimental.remove(key);
    }
    let loaded: Scratch = serde_json::from_value(value).unwrap();
    assert_eq!(loaded.synthesis_rate(48000), 96000);
    assert!(!loaded.experimental.primary_1d && !loaded.experimental.vvt_overlap_safe);
    assert_eq!(loaded.experimental.coupled_level_db, 0.);
}

#[test]
fn incompatible_finite_volume_and_wave_port_references_are_rejected() {
    let mut scratch = Scratch::default();
    scratch.experimental.primary_1d = true;
    scratch.experimental.wave_coupling = true;
    assert!(
        scratch
            .validate()
            .unwrap_err()
            .contains("thermodynamic reference")
    );
    assert!(Engine::new(&scratch, 48000).is_err());
}

#[test]
fn anechoic_diagnostic_removes_length_feedback_but_keeps_local_coupling() {
    let mut short = Scratch::default();
    short.experimental.wave_coupling = true;
    short.sound.primary_length_scale = 0.7;
    let mut long = short.clone();
    long.sound.primary_length_scale = 1.4;
    let (mut a, mut b) = (
        Engine::new(&short, 48000).unwrap(),
        Engine::new(&long, 48000).unwrap(),
    );
    a.suppress_primary_reflections_for_reference();
    b.suppress_primary_reflections_for_reference();
    for _ in 0..16000 {
        let command = Commands {
            imposed_rpm: Some(3200.),
            throttle: 1.,
            ..Default::default()
        };
        let (x, y) = (a.next(command), b.next(command));
        assert!(!a.failed() && !b.failed());
        assert_eq!(x.torque_nm.to_bits(), y.torque_nm.to_bits());
        assert_eq!(x.map_pa.to_bits(), y.map_pa.to_bits());
    }
}

#[test]
fn quality_propagation_changes_the_dyno_key_only_with_feedback() {
    let plain = Scratch::default();
    let mut quality = plain.clone();
    quality.experimental.primary_1d = true;
    assert_eq!(bess::dyno::key(&plain), bess::dyno::key(&quality));
    quality.experimental.wave_coupling = true;
    let mut coupled = plain;
    coupled.experimental.wave_coupling = true;
    assert_ne!(bess::dyno::key(&coupled), bess::dyno::key(&quality));
    let mut native = coupled.clone();
    native.experimental.native_rate_acoustics = true;
    assert_ne!(bess::dyno::key(&coupled), bess::dyno::key(&native));
    native.experimental.wave_coupling = false;
    coupled.experimental.wave_coupling = false;
    assert_eq!(bess::dyno::key(&coupled), bess::dyno::key(&native));
}

#[test]
fn coupled_dyno_key_keeps_downstream_reflections_but_never_observation_eq() {
    for feedback in [false, true] {
        let mut plain = Scratch::default();
        plain.experimental.wave_coupling = feedback;
        for part in 0..4 {
            let mut changed = plain.clone();
            match part {
                0 => changed.sound.tail_length_m *= 1.2,
                1 => changed.sound.muffler_volume_scale *= 1.2,
                2 => changed.sound.muffler_absorption *= 0.8,
                _ => changed.sound.exhaust_decay_ms = 35.,
            }
            assert_eq!(
                bess::dyno::key(&plain) != bess::dyno::key(&changed),
                feedback
            );
        }
        let mut equalized = plain.clone();
        equalized.sound.exhaust_body_db = 6.;
        assert_eq!(bess::dyno::key(&plain), bess::dyno::key(&equalized));
    }
}

#[test]
fn fixed_coupled_gain_is_exact_in_the_observation_only() {
    let mut raw = Scratch::default();
    raw.experimental.wave_coupling = true;
    let mut calibrated = raw.clone();
    calibrated.experimental.coupled_level_db = 3.;
    let gain = 10_f32.powf(3. / 20.);
    let (mut a, mut b) = (
        Engine::new(&raw, 48000).unwrap(),
        Engine::new(&calibrated, 48000).unwrap(),
    );
    for _ in 0..16000 {
        let c = Commands {
            imposed_rpm: Some(3500.),
            throttle: 0.8,
            ..Default::default()
        };
        let (x, y) = (a.next(c), b.next(c));
        assert_eq!((x.exhaust * gain).to_bits(), y.exhaust.to_bits());
        assert_eq!(x.intake.to_bits(), y.intake.to_bits());
        assert_eq!(x.torque_nm.to_bits(), y.torque_nm.to_bits());
    }
}

#[test]
fn prepared_observation_gains_cross_twenty_ms_without_steps_or_physical_changes() {
    for bank_control in [false, true] {
        let mut source = v8();
        source.design.bank_delay_ms = 0.;
        source.design.bank_gain_db = 0.;
        let mut changed = source.clone();
        if bank_control {
            changed.design.bank_gain_db = 6.;
        } else {
            changed.experimental.coupled_level_db = 6.;
        }
        let (mut reference, mut edited, mut prepared) = (
            Engine::new(&source, 48000).unwrap(),
            Engine::new(&source, 48000).unwrap(),
            Engine::new(&changed, 48000).unwrap(),
        );
        let command = Commands {
            imposed_rpm: Some(3200.),
            throttle: 0.7,
            ..Default::default()
        };
        for _ in 0..9600 {
            assert_eq!(
                reference.next(command).exhaust.to_bits(),
                edited.next(command).exhaust.to_bits()
            );
        }
        let before = edited.state();
        assert!(edited.apply_sound_tuning(&mut prepared));
        assert_eq!(before.rpm.to_bits(), edited.state().rpm.to_bits());
        assert_eq!(before.map_pa.to_bits(), edited.state().map_pa.to_bits());
        let target = f64::from(10_f32.powf(6. / 20.));
        let (
            mut first_max,
            mut first_count,
            mut around_tau,
            mut tau_count,
            mut settled,
            mut settled_count,
        ) = (0_f64, 0, 0., 0, 0., 0);
        for i in 0..4800 {
            let (a, b) = (reference.next(command), edited.next(command));
            assert!(!reference.failed() && !edited.failed());
            assert_eq!(a.torque_nm.to_bits(), b.torque_nm.to_bits());
            assert_eq!(a.map_pa.to_bits(), b.map_pa.to_bits());
            assert_eq!(a.heat_j.to_bits(), b.heat_j.to_bits());
            assert_eq!(a.fuel_injected_kg.to_bits(), b.fuel_injected_kg.to_bits());
            assert_eq!(a.bank_pressure, b.bank_pressure);
            // Recover the audible gain from the output and unchanged physical
            // pressures; these defaults bypass every tone-shaping filter.
            let gain = if bank_control && a.bank_pressure[1].abs() > 1. {
                Some(
                    (f64::from(b.exhaust) * 3000. - f64::from(a.bank_pressure[0]))
                        / f64::from(a.bank_pressure[1]),
                )
            } else if !bank_control && a.exhaust.abs() > 1e-4 {
                Some(f64::from(b.exhaust / a.exhaust))
            } else {
                None
            };
            if let Some(gain) = gain {
                let fraction = (gain - 1.) / (target - 1.);
                if i < 32 {
                    first_max = first_max.max(fraction);
                    first_count += 1;
                }
                if (940..980).contains(&i) {
                    around_tau += fraction;
                    tau_count += 1;
                }
                if i >= 4700 {
                    settled += fraction;
                    settled_count += 1;
                }
            }
        }
        assert!(
            first_count > 0 && first_max < 0.04,
            "instantaneous gain jump: {first_max}"
        );
        assert!(
            tau_count > 0 && (0.60..0.66).contains(&(around_tau / f64::from(tau_count))),
            "20ms transition: {around_tau}/{tau_count}"
        );
        assert!(settled_count > 0 && (0.99..1.001).contains(&(settled / f64::from(settled_count))));
    }
}

#[test]
fn alternative_vvt_map_keeps_the_high_speed_schedule_and_is_dyno_visible() {
    let original = Scratch::default();
    let mut alternative = original.clone();
    alternative.experimental.vvt_overlap_safe = true;
    assert_ne!(bess::dyno::key(&original), bess::dyno::key(&alternative));
    for coupled in [false, true] {
        let mut a = original.clone();
        let mut b = alternative.clone();
        a.experimental.wave_coupling = coupled;
        b.experimental.wave_coupling = coupled;
        let (mut a, mut b) = (
            Engine::new(&a, 48000).unwrap(),
            Engine::new(&b, 48000).unwrap(),
        );
        for _ in 0..8000 {
            let c = Commands {
                imposed_rpm: Some(5500.),
                throttle: 1.,
                ..Default::default()
            };
            let (x, y) = (a.next(c), b.next(c));
            assert!(!a.failed() && !b.failed());
            assert_eq!(x.torque_nm.to_bits(), y.torque_nm.to_bits());
            assert_eq!(x.exhaust.to_bits(), y.exhaust.to_bits());
        }
    }
}
