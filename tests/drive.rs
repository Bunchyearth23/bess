use bess::drive::{Controls, Mode, Simulator, State};

fn run(c: Controls, seconds: usize) -> State {
    let mut sim = Simulator::new(803., 4989., c);
    for _ in 0..seconds * 1000 {
        sim.step(c);
    }
    sim.state()
}
#[test]
fn throttle_gears_and_output_load_change_motion() {
    let c = Controls {
        mode: Mode::Simulated,
        throttle: 0.8,
        automatic: false,
        ..Default::default()
    };
    let idle = run(Controls { throttle: 0., ..c }, 8);
    let first = run(c, 8);
    let heavy = run(
        Controls {
            resistance_nm: 1800.,
            ..c
        },
        8,
    );
    let second = run(Controls { gear: 2, ..c }, 8);
    let neutral = run(Controls { gear: 0, ..c }, 8);
    println!(
        "idle {idle:?}\nfirst {first:?}\nheavy {heavy:?}\nsecond {second:?}\nneutral {neutral:?}"
    );
    assert_eq!(idle.speed_kmh, 0.);
    assert_eq!(neutral.speed_kmh, 0.);
    assert!(neutral.rpm > 4000.);
    assert!(first.speed_kmh > 10. && first.speed_kmh > heavy.speed_kmh + 5.);
    assert!(second.speed_kmh > first.speed_kmh * 1.3);
}
#[test]
fn automatic_shifts_release_load_and_brakes_stop_the_vehicle() {
    let c = Controls {
        throttle: 0.85,
        mode: Mode::Simulated,
        ..Default::default()
    };
    let mut sim = Simulator::new(803., 4989., c);
    let mut shifts = 0;
    let mut shift_load = 1f32;
    for _ in 0..20000 {
        let before = sim.state();
        let after = sim.step(c);
        if after.gear != before.gear {
            shifts += 1;
        }
        if after.shifting {
            shift_load = shift_load.min(after.load);
        }
        assert!((802.99..=4989.01).contains(&after.rpm));
    }
    println!(
        "after 20s {:?}, shifts={shifts}, shift_load={shift_load}",
        sim.state()
    );
    assert!(shifts >= 2 && shift_load < 0.35);
    for _ in 0..12000 {
        sim.step(Controls {
            throttle: 0.,
            brake: 1.,
            ..c
        });
    }
    assert!(sim.state().speed_kmh < 0.1);
}
#[test]
fn overspeed_downshift_is_rejected_and_reset_is_repeatable() {
    let mut c = Controls {
        throttle: 0.9,
        mode: Mode::Simulated,
        ..Default::default()
    };
    let mut sim = Simulator::new(803., 4989., c);
    for _ in 0..25000 {
        sim.step(c);
    }
    c.automatic = false;
    c.gear = 1;
    let result = sim.step(c);
    assert!(result.shift_rejected && result.gear > 1);
    assert!(result.rpm <= 4989.01);
    sim.reset(c);
    assert_eq!(sim.state().speed_kmh, 0.);
    let mut other = Simulator::new(803., 4989., c);
    for _ in 0..3000 {
        let a = sim.step(c);
        let b = other.step(c);
        assert_eq!(a.rpm, b.rpm);
        assert_eq!(a.speed_kmh, b.speed_kmh);
    }
}
#[test]
fn invalid_controls_are_rejected_and_old_projects_use_direct_mode() {
    for c in [
        Controls {
            gear: 7,
            ..Default::default()
        },
        Controls {
            mass_kg: 0.,
            ..Default::default()
        },
        Controls {
            throttle: f32::NAN,
            ..Default::default()
        },
        Controls {
            ratios: [1.; 6],
            ..Default::default()
        },
    ] {
        assert!(c.validate().is_err());
    }
    let project: bess::project::Project =
        serde_json::from_str(r#"{"version":2,"parameters":{}}"#).unwrap();
    assert_eq!(project.driving.mode, Mode::Direct);
}

#[test]
fn extreme_bench_parameters_stay_finite_and_inside_the_source_range() {
    for (min, max) in [(200., 20000.), (803., 4989.), (800., 800.)] {
        for c in [
            Controls {
                throttle: 1.,
                mass_kg: 300.,
                peak_torque_nm: 2000.,
                inertia: 0.1,
                wheel_radius: 0.2,
                final_drive: 6.,
                ..Default::default()
            },
            Controls {
                throttle: 1.,
                mass_kg: 6000.,
                peak_torque_nm: 30.,
                inertia: 2.,
                wheel_radius: 0.6,
                grade_percent: 25.,
                resistance_nm: 4000.,
                ..Default::default()
            },
        ] {
            c.validate().unwrap();
            let mut sim = Simulator::new(min, max, c);
            for _ in 0..40000 {
                let s = sim.step(c);
                assert!(s.rpm.is_finite() && (min - 0.02..=max + 0.02).contains(&s.rpm));
                assert!(s.load.is_finite() && (0.0..=1.0).contains(&s.load));
                assert!(s.speed_kmh.is_finite() && s.speed_kmh >= 0.);
                assert!(s.wheel_torque.is_finite() && s.resisting_torque.is_finite());
            }
        }
    }
}
