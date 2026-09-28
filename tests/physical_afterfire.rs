use bess::{
    physical::engine::{Commands, Engine, Sample},
    scratch::{PRESETS, Scratch},
};

const RATE: usize = 48_000;

fn scratch(intensity: f32) -> Scratch {
    let mut s = Scratch::default();
    s.experimental.afterfire = intensity;
    s
}

fn command(throttle: f64) -> Commands {
    Commands {
        imposed_rpm: Some(4000.),
        throttle,
        overrun: 1.,
        ..Default::default()
    }
}

fn next(engine: &mut Engine, command: Commands) -> Sample {
    let sample = engine.next(command);
    assert!(!engine.failed());
    for value in [
        f64::from(sample.exhaust),
        sample.map_pa,
        sample.heat_j,
        sample.afterfire_heat_j,
        sample.fuel_injected_kg,
    ] {
        assert!(value.is_finite());
    }
    sample
}

#[test]
fn opt_in_changes_only_lift_off_and_retains_real_fuel_for_a_bounded_window() {
    let mut off = Engine::new(&scratch(0.), RATE as u32).unwrap();
    let mut on = Engine::new(&scratch(1.), RATE as u32).unwrap();
    // Establish the same hot gas state. An armed tune must not alter power-on
    // sound, heat or fuel, including the seeded cycle variation.
    for _ in 0..RATE * 4 {
        let a = next(&mut off, command(0.8));
        let b = next(&mut on, command(0.8));
        assert_eq!(a.exhaust.to_bits(), b.exhaust.to_bits());
        assert_eq!(a.heat_j.to_bits(), b.heat_j.to_bits());
        assert_eq!(a.fuel_injected_kg.to_bits(), b.fuel_injected_kg.to_bits());
    }
    let (mut fuel_off, mut fuel_on, mut heat_off, mut heat_on) = (0., 0., 0., 0.);
    for frame in 0..RATE {
        let a = next(&mut off, command(0.));
        let b = next(&mut on, command(0.));
        fuel_off += a.fuel_injected_kg;
        fuel_on += b.fuel_injected_kg;
        heat_off += a.afterfire_heat_j;
        heat_on += b.afterfire_heat_j;
        assert!(a.fuel_cut);
        if frame < RATE / 4 {
            assert!(!b.fuel_cut);
        }
        if frame > RATE * 31 / 100 {
            assert!(b.fuel_cut, "tune must return to DFCO after 300 ms");
            assert_eq!(b.fuel_injected_kg, 0.);
        }
    }
    assert_eq!(fuel_off, 0.);
    assert!(fuel_on > 1e-8, "no actual retained injection: {fuel_on}");
    assert!(
        heat_on > heat_off,
        "no extra physical reaction: on {heat_on} J, off {heat_off} J"
    );
    // This comparison checks reaction activity, not a complete energy ledger;
    // warm-up fuel and thermal inventories remain in the open gas system.
    assert!(heat_on.is_finite() && heat_on >= 0.);
    println!("afterfire: retained fuel {fuel_on} kg; heat {heat_on} J vs {heat_off} J");
}

#[test]
fn tune_requires_a_lift_and_releases_on_tip_in_reset_or_rev_limit() {
    let mut engine = Engine::new(&scratch(1.), RATE as u32).unwrap();
    for _ in 0..RATE / 10 {
        assert!(next(&mut engine, command(0.)).fuel_cut);
    }
    next(&mut engine, command(0.8));
    assert!(!next(&mut engine, command(0.)).fuel_cut);
    next(&mut engine, command(0.1)); // Releases the window without rearming.
    assert!(next(&mut engine, command(0.)).fuel_cut);
    next(&mut engine, command(0.8));
    assert!(!next(&mut engine, command(0.)).fuel_cut);
    engine.reset();
    assert!(next(&mut engine, command(0.)).fuel_cut);
    next(&mut engine, command(0.8));
    let limited = Commands {
        imposed_rpm: Some(f64::from(scratch(1.).redline_rpm) + 100.),
        ..command(0.)
    };
    assert!(next(&mut engine, limited).fuel_cut);
}

#[test]
fn intensity_sets_the_short_control_window_without_constant_closed_throttle_retrigger() {
    for (intensity, duration) in [(0.1, 0.21), (0.5, 0.25), (1., 0.3)] {
        let mut engine = Engine::new(&scratch(intensity), RATE as u32).unwrap();
        next(&mut engine, command(0.8));
        let mut retained_frames = 0;
        for _ in 0..RATE / 2 {
            if !next(&mut engine, command(0.)).fuel_cut {
                retained_frames += 1;
            }
        }
        let expected = duration * RATE as f64;
        assert!((retained_frames as f64 - expected).abs() <= 2.);
    }
}

#[test]
fn physical_geometry_limits_have_actionable_errors() {
    for (bore, stroke, message) in [(4., 86., "bore"), (86., 3., "stroke"), (501., 86., "bore")] {
        let mut s = scratch(0.);
        s.build.bore_mm = bore;
        s.build.stroke_mm = stroke;
        let error = Engine::new(&s, RATE as u32).err().unwrap();
        assert!(
            error.contains(message) && error.contains("5–500 mm"),
            "{error}"
        );
    }
    for large in [false, true] {
        let mut s = scratch(0.);
        if large {
            s.design = PRESETS
                .iter()
                .find(|(name, _)| *name == "V12 60°")
                .unwrap()
                .1;
            s.apply_design();
        }
        s.build.bore_mm = if large { 500. } else { 5. };
        s.build.stroke_mm = if large { 500. } else { 5. };
        let error = Engine::new(&s, RATE as u32).err().unwrap();
        assert!(error.contains("1 cm³–500 L"), "{error}");
    }
}
