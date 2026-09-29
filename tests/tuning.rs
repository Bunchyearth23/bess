//! W-007 S5: optional overrides of part-derived quantities (`Scratch.tuning`).
use bess::{
    engine_build::{Aspiration, EngineBuild, EngineTuning, Throttle},
    physical::{
        config::CylinderConfig,
        engine::{Commands, Engine},
    },
    project::{Parameters, Project, load_project, save_project},
    scratch::Scratch,
};
use std::f64::consts::PI;

#[test]
fn resolve_without_overrides_is_the_part_formulas_bit_for_bit() {
    for cam in [0., 0.3, 1.] {
        for valves in 2..=5 {
            for throttle in [Throttle::Single, Throttle::Individual] {
                for aspiration in [Aspiration::Turbo, Aspiration::TwinTurbo] {
                    for (cylinders, bore) in [(1, 55.), (4, 86.), (12, 99.5)] {
                        let build = EngineBuild {
                            cam,
                            valves,
                            throttle,
                            aspiration,
                            bore_mm: bore,
                            stroke_mm: 77.3,
                            ..Default::default()
                        };
                        let r = EngineTuning::default().resolve(&build, cylinders);
                        // Formulas as they stood in config.rs, manifolds.rs and induction.rs.
                        let cam = f64::from(cam);
                        let bore_m = f64::from(bore) * 0.001;
                        let stroke_m = f64::from(77.3_f32) * 0.001;
                        let (iv, ev) = (u32::from(valves).div_ceil(2), u32::from(valves) / 2);
                        let lsa = 114. - 8. * cam;
                        let v = PI * bore_m.powi(2) * stroke_m / 4. * f64::from(cylinders);
                        let individual = throttle == Throttle::Individual;
                        let old = [
                            200. + 60. * cam,
                            0.009 + 0.004 * cam,
                            360. + lsa,
                            360. - lsa,
                            bore_m * if iv == 1 { 0.43 } else { 0.36 },
                            bore_m * if ev == 1 { 0.37 } else { 0.31 },
                            stroke_m * 1.75,
                            PI * 0.055_f64.powi(2) / 4.0
                                * (v / 0.002).powf(2.0 / 3.0)
                                * if individual { 1.35 } else { 1.0 },
                            v * if individual { 0.35 } else { 1.25 },
                            v * 0.005,
                            if aspiration == Aspiration::TwinTurbo {
                                1.2e-5
                            } else {
                                2.0e-5
                            },
                        ];
                        let new = [
                            r.duration_at_050_deg,
                            r.lift_m,
                            r.intake_center_deg,
                            r.exhaust_center_deg,
                            r.intake_diameter_m,
                            r.exhaust_diameter_m,
                            r.rod_m,
                            r.throttle_area_m2,
                            r.plenum_volume_m3,
                            r.compressor_displacement_m3,
                            r.turbo_inertia_kg_m2,
                        ];
                        assert_eq!(old.map(f64::to_bits), new.map(f64::to_bits), "{build:?}");
                        assert_eq!((r.intake_valves, r.exhaust_valves), (iv, ev));
                    }
                }
            }
        }
    }
}

fn project(scratch: Scratch) -> Project {
    Project {
        version: 3,
        parameters: Parameters::default(),
        hybrid: Default::default(),
        source: None,
        driving: Default::default(),
        profile_name: "Natural".into(),
        scratch: Some(scratch),
    }
}

#[test]
fn projects_without_tuning_round_trip_unchanged() {
    let path = std::env::temp_dir().join(format!("bess-tuning-{}.json", std::process::id()));
    save_project(&path, &project(Scratch::default())).unwrap();
    let json = std::fs::read_to_string(&path).unwrap();
    assert!(!json.contains("tuning"), "no overrides, no new key");
    assert_eq!(
        load_project(&path).unwrap().scratch,
        Some(Scratch::default())
    );

    let mut tuned = Scratch::default();
    tuned.tuning.cam.duration_deg = Some(248.);
    tuned.tuning.bottom.rod_to_stroke = Some(1.55);
    save_project(&path, &project(tuned.clone())).unwrap();
    assert_eq!(load_project(&path).unwrap().scratch, Some(tuned.clone()));

    // A hand-edited file with an absurd override is refused on load.
    let json = std::fs::read_to_string(&path)
        .unwrap()
        .replace("1.55", "0.4");
    std::fs::write(&path, json).unwrap();
    assert!(load_project(&path).is_err());
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn overrides_are_range_checked() {
    let mut s = Scratch::default();
    for bad in [170., 310., f32::NAN, f32::INFINITY] {
        s.tuning.cam.duration_deg = Some(bad);
        assert!(s.validate().is_err(), "{bad}");
    }
    s.tuning.cam.duration_deg = Some(300.);
    s.tuning.intake.throttle_mm = Some(29.);
    assert!(s.validate().is_err());
    s.tuning.intake.throttle_mm = Some(30.);
    assert!(s.validate().is_ok());
}

#[test]
fn tuning_is_an_engine_change_but_keeps_the_seed() {
    let base = Scratch::default();
    let mut tuned = base.clone();
    tuned.tuning.turbo.size = Some(1.4);
    assert!(!base.same_engine_except_sound(&tuned));
    assert!(base.same_engine_except_sound(&base.clone()));
    // Overrides never redraw the per-design dispersions (A/B stays honest).
    assert_eq!(base.life().seed, tuned.life().seed);
}

fn render(scratch: &Scratch) -> Vec<f32> {
    let mut engine = Engine::new(scratch, 48000).unwrap();
    let x = (0..12000)
        .flat_map(|_| {
            let s = engine.next(Commands {
                imposed_rpm: Some(4000.),
                throttle: 1.,
                ..Default::default()
            });
            [s.exhaust, s.intake, s.mechanical]
        })
        .collect();
    assert!(!engine.failed());
    x
}

#[test]
fn overrides_reach_cylinders_manifolds_and_turbo() {
    let base = Scratch::default();
    let config = |s: &Scratch| {
        CylinderConfig::from_tuning(&s.build, &s.tuning.resolve(&s.build, s.design.cylinders))
            .unwrap()
    };
    let mut cam = base.clone();
    cam.tuning.cam.duration_deg = Some(280.);
    assert!(config(&cam).seat_duration_deg > config(&base).seat_duration_deg);
    let mut rod = base.clone();
    rod.tuning.bottom.rod_to_stroke = Some(1.45);
    assert!(config(&rod).rod_m < config(&base).rod_m);
    let mut throttle = base.clone();
    throttle.tuning.intake.throttle_mm = Some(32.);
    let mut boosted = base.clone();
    boosted.build.aspiration = Aspiration::Turbo;
    let mut small_turbo = boosted.clone();
    small_turbo.tuning.turbo.size = Some(0.6);
    let reference = render(&base);
    for tuned in [&cam, &rod, &throttle] {
        assert_ne!(render(tuned), reference, "{:?}", tuned.tuning);
    }
    assert_ne!(render(&small_turbo), render(&boosted));
}
