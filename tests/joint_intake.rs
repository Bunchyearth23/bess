//! W-011: joint intake orifice and conservative runner/cylinder exchange.
use bess::{
    physical::engine::{Commands, Engine},
    scratch::Scratch,
};

#[test]
fn joint_intake_option_migrates_off_and_remains_independent_of_exhaust() {
    let scratch = Scratch::default();
    assert!(!scratch.experimental.intake_wave_coupling);
    let mut json = serde_json::to_value(&scratch).unwrap();
    json["experimental"]
        .as_object_mut()
        .unwrap()
        .remove("intake_wave_coupling");
    let migrated: Scratch = serde_json::from_value(json).unwrap();
    assert!(!migrated.experimental.intake_wave_coupling);
}

#[test]
fn joint_intake_and_exhaust_are_finite_at_two_rates_and_change_physical_torque() {
    for rate in [48_000, 96_000] {
        for exhaust in [false, true] {
            let run = |intake: bool, length: f32| {
                let mut scratch = Scratch::default();
                scratch.experimental.intake_wave_coupling = intake;
                scratch.experimental.wave_coupling = exhaust;
                scratch.sound.intake_length_m = length;
                let mut engine = Engine::new(&scratch, rate).unwrap();
                let mut torque = 0.;
                for i in 0..rate / 3 {
                    let s = engine.next(Commands {
                        imposed_rpm: Some(3200.),
                        throttle: if i < rate / 6 { 0.3 } else { 0.85 },
                        ..Default::default()
                    });
                    assert!(
                        !engine.failed(),
                        "rate {rate}, exhaust {exhaust}, intake {intake}, length {length}, frame {i}"
                    );
                    assert!(s.intake.is_finite() && s.torque_nm.is_finite() && s.map_pa > 0.);
                    if i >= rate / 4 {
                        torque += s.torque_nm;
                    }
                }
                torque / f64::from(rate / 3 - rate / 4)
            };
            let off = run(false, 0.38);
            let on = run(true, 0.38);
            let long = run(true, 0.75);
            assert!(
                (on - off).abs() > 0.01,
                "intake feedback did not affect gas torque"
            );
            assert!(
                (long - on).abs() > 0.01,
                "runner length did not affect coupled gas torque"
            );
        }
    }
}

#[test]
fn intake_observation_gain_cannot_change_joint_physics() {
    let mut a = Scratch::default();
    a.experimental.intake_wave_coupling = true;
    a.experimental.wave_coupling = true;
    let mut b = a.clone();
    b.experimental.coupled_level_db = 12.;
    b.sound.intake_air_noise = 0.;
    let mut x = Engine::new(&a, 48000).unwrap();
    let mut y = Engine::new(&b, 48000).unwrap();
    for _ in 0..16000 {
        let commands = Commands {
            imposed_rpm: Some(2600.),
            throttle: 0.7,
            ..Default::default()
        };
        let sx = x.next(commands);
        let sy = y.next(commands);
        assert!(!x.failed() && !y.failed());
        assert_eq!(sx.torque_nm.to_bits(), sy.torque_nm.to_bits());
        assert_eq!(sx.map_pa.to_bits(), sy.map_pa.to_bits());
        assert_eq!(sx.heat_j.to_bits(), sy.heat_j.to_bits());
    }
}

#[test]
fn joint_wave_boundaries_cover_i6_v8_v12_and_extreme_runner_lengths() {
    for name in ["Inline-6", "V8 cross-plane", "V12 60°"] {
        for rate in [48000, 96000] {
            for length in [0.15, 1.5] {
                let mut scratch = Scratch::default();
                scratch.design = bess::scratch::PRESETS
                    .iter()
                    .find(|p| p.0 == name)
                    .unwrap()
                    .1;
                scratch.apply_design();
                scratch.experimental.intake_wave_coupling = true;
                scratch.experimental.wave_coupling = true;
                scratch.sound.intake_length_m = length;
                let mut engine = Engine::new(&scratch, rate).unwrap();
                for frame in 0..rate / 8 {
                    let throttle = if frame < rate / 16 { 1. } else { 0.03 };
                    let s = engine.next(Commands {
                        imposed_rpm: Some(6000.),
                        throttle,
                        ..Default::default()
                    });
                    assert!(
                        !engine.failed() && s.intake.is_finite() && s.exhaust.is_finite(),
                        "{name}, {rate}, {length}, frame {frame}"
                    );
                }
            }
        }
    }
}
