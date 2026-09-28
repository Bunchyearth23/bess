use bess::{
    physical::engine::{Commands, Engine},
    scratch::{PRESETS, Scratch},
};

#[test]
fn arbitrary_pins_banks_and_missing_sparks_remain_finite() {
    for count in 1..=12 {
        for all_second_bank in [false, true] {
            let mut scratch = Scratch::default();
            scratch.design.set_cylinders(count);
            for i in 0..count as usize {
                scratch.design.pins[i] = ((i * 137 + count as usize * 23) % 360) as f32;
                scratch.design.banks[i] = if all_second_bank { 1 } else { (i % 2) as u8 };
            }
            scratch.design.firing_order[..count as usize].fill(1);
            scratch.build.bore_mm = 35. + count as f32 * 8.;
            scratch.build.stroke_mm = 25. + count as f32 * 7.;
            scratch.build.cam = if all_second_bank { 1. } else { 0. };
            scratch.build.compression = if all_second_bank { 14. } else { 7. };
            scratch.apply_design();
            let mut engine = Engine::new(&scratch, 48000).unwrap();
            for i in 0..4800 {
                let sample = engine.next(Commands {
                    imposed_rpm: Some(if i < 2400 { 1200. } else { 12000. }),
                    throttle: 0.8,
                    ..Default::default()
                });
                assert!(
                    !engine.failed(),
                    "{count} cylinders, second_bank={all_second_bank}, frame={i}"
                );
                assert!(sample.exhaust.is_finite() && sample.map_pa.is_finite());
            }
        }
    }
}

#[test]
fn rewiring_first_spark_does_not_move_mechanical_cams() {
    let mut original = Scratch {
        design: PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1,
        ..Default::default()
    };
    original.apply_design();
    original.design.resolve_cam_revolutions();
    let mut rewired = original.clone();
    rewired.design.firing_order[..8].rotate_left(1);
    let mut a = Engine::new(&original, 48000).unwrap();
    let mut b = Engine::new(&rewired, 48000).unwrap();
    let command = Commands {
        imposed_rpm: Some(3000.),
        throttle: 0.,
        overrun: 1.,
        ..Default::default()
    };
    for _ in 0..24000 {
        let x = a.next(command);
        let y = b.next(command);
        assert_eq!(x.heat_j, 0.);
        assert_eq!(y.heat_j, 0.);
        assert_eq!(
            x.bank_pressure.map(f32::to_bits),
            y.bank_pressure.map(f32::to_bits)
        );
        assert_eq!(x.map_pa.to_bits(), y.map_pa.to_bits());
    }
    assert!(!a.failed() && !b.failed());
}

#[test]
fn cross_plane_v8_has_bank_half_orders_that_cancel_in_the_sum() {
    let mut scratch = Scratch {
        design: PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1,
        ..Default::default()
    };
    scratch.apply_design();
    let mut engine = Engine::new(&scratch, 48000).unwrap();
    let command = Commands {
        imposed_rpm: Some(1200.),
        throttle: 0.35,
        ..Default::default()
    };
    let mut spectra = [[[0_f64; 2]; 8]; 3];
    for i in 0..72000 {
        let sample = engine.next(command);
        if i >= 24000 {
            for order in 0..8 {
                let (s, c) =
                    (std::f64::consts::TAU * (order + 1) as f64 * 10. * (i - 24000) as f64
                        / 48000.)
                        .sin_cos();
                let banks = [
                    sample.bank_pressure[0],
                    sample.bank_pressure[1],
                    sample.bank_pressure.iter().sum(),
                ];
                for (spectrum, bank) in spectra.iter_mut().zip(banks) {
                    spectrum[order][0] += f64::from(bank) * c;
                    spectrum[order][1] += f64::from(bank) * s;
                }
            }
        }
    }
    assert!(!engine.failed());
    let power = spectra.map(|bank| bank.map(|[re, im]| re * re + im * im));
    let half = power.map(|bank| [0, 2, 4, 6].iter().map(|&i| bank[i]).sum::<f64>());
    assert!(
        half[0] > power[0][7] * 0.01,
        "No bank half orders: {half:?}, power={power:?}"
    );
    assert!(half[1] > power[1][7] * 0.01);
    assert!(
        half[2] < (half[0] + half[1]) * 0.5,
        "Bank phasing failed to cancel: {half:?}"
    );
}
