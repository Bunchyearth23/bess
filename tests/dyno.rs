use bess::{dyno, engine_build::Aspiration, scratch::Scratch};

#[test]
fn default_two_litre_curve_is_plausible_and_deterministic() {
    let scratch = Scratch::default();
    let curve = dyno::sweep(&scratch, 13).unwrap();
    println!("{curve:?}");
    assert_eq!(curve.rpm.len(), 13);
    assert_eq!(curve.rpm[0], f64::from(scratch.idle_rpm));
    assert!(*curve.rpm.last().unwrap() < f64::from(scratch.redline_rpm));
    for i in 0..curve.rpm.len() {
        let (rpm, torque, power) = (curve.rpm[i], curve.torque_nm[i], curve.power_kw[i]);
        assert!(torque.is_finite() && curve.map_kpa[i].is_finite());
        assert!((power - torque * rpm * std::f64::consts::TAU / 60e3).abs() < 1e-9);
        if (2000.0..=5000.).contains(&rpm) {
            assert!(torque > 50., "{rpm} rpm: {torque} Nm");
        }
    }
    // X-024: runner ram gives a mid-range peak; power peaks near redline.
    let (peak, at) = curve.peak_torque;
    assert!((170.0..=230.).contains(&peak), "{peak} Nm");
    assert!((3000.0..=5500.).contains(&at), "{at} rpm");
    assert!(curve.peak_power.1 >= 0.8 * f64::from(scratch.redline_rpm));
    assert_eq!(dyno::sweep(&scratch, 13).unwrap(), curve);
}

#[test]
fn boost_raises_torque_and_manifold_pressure() {
    let na = dyno::sweep(&Scratch::default(), 7).unwrap();
    let mut scratch = Scratch::default();
    scratch.build.aspiration = Aspiration::Turbo;
    scratch.build.boost_bar = 0.8;
    let turbo = dyno::sweep(&scratch, 7).unwrap();
    println!("{:?}\n{:?}", na.peak_torque, turbo);
    assert!(turbo.peak_torque.0 > na.peak_torque.0 * 1.1);
    // Target 0.8 bar: ≥0.7 bar by 3924 rpm, wastegate-held ≥0.6 bar at 5974.
    assert!(turbo.map_kpa[3] > 101.325 + 70., "{:?}", turbo.map_kpa);
    assert!(turbo.map_kpa[5] > 101.325 + 60., "{:?}", turbo.map_kpa);
    assert!(turbo.map_kpa.iter().all(|&p| p < 101.325 + 90.));
}

#[test]
fn curve_follows_compression_and_cam() {
    let base = dyno::sweep(&Scratch::default(), 7).unwrap();
    let mut scratch = Scratch::default();
    scratch.build.compression = 13.;
    let compression = dyno::sweep(&scratch, 7).unwrap();
    let mut scratch = Scratch::default();
    scratch.build.cam = 1.;
    let cam = dyno::sweep(&scratch, 7).unwrap();
    println!("{base:?}\n{compression:?}\n{cam:?}");
    assert!(compression.peak_torque.0 > base.peak_torque.0 * 1.02);
    let moved = (0..7).any(|i| (cam.torque_nm[i] / base.torque_nm[i] - 1.).abs() > 0.03);
    assert!(moved);
}

#[test]
#[ignore = "measurement table: cargo test --release --test dyno preset_curves -- --ignored --nocapture"]
fn preset_curves() {
    use bess::scratch::PRESETS;
    let preset = |name: &str| {
        let mut s = Scratch {
            design: PRESETS.iter().find(|p| p.0 == name).unwrap().1,
            ..Default::default()
        };
        s.apply_design();
        s
    };
    let mut turbo = Scratch::default();
    turbo.build.aspiration = Aspiration::Turbo;
    turbo.build.boost_bar = 0.8;
    for (label, scratch) in [
        ("NA I4", Scratch::default()),
        ("turbo I4", turbo),
        ("V8", preset("V8 cross-plane")),
        ("V12", preset("V12 60°")),
    ] {
        let c = dyno::sweep(&scratch, 13).unwrap();
        let row = |v: &[f64]| {
            v.iter()
                .map(|x| format!("{x:.1}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        println!(
            "{label}: peak {:.1} Nm @ {:.0}, {:.1} kW @ {:.0}\n  rpm {}\n  Nm {}\n  MAP {}",
            c.peak_torque.0,
            c.peak_torque.1,
            c.peak_power.0,
            c.peak_power.1,
            row(&c.rpm),
            row(&c.torque_nm),
            row(&c.map_kpa)
        );
    }
}
