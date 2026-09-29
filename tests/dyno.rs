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
    let (peak, _) = curve.peak_torque;
    assert!((120.0..=260.).contains(&peak), "{peak} Nm");
    assert!(curve.peak_power.0 > 40., "{:?}", curve.peak_power);
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
    assert!(turbo.map_kpa.iter().any(|&p| p > 120.));
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
