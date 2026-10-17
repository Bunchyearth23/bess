use bess::{
    bench::Bench,
    drive::{Controls, Mode},
    hybrid::Settings,
    physical::{
        crank::TorsionConfig,
        engine::{Commands, Engine},
    },
    project::Parameters,
    scratch::{Scratch, ScratchModel},
};

fn options(all: bool, exhaust: bool) -> Scratch {
    let mut spec = Scratch::default();
    spec.experimental.fuel_transients = Some(Default::default());
    if all {
        spec.experimental.predictive_combustion = Some(Default::default());
        spec.experimental.torsion = Some(Default::default());
        spec.experimental.intake_wave_coupling = true;
    }
    spec.experimental.wave_coupling = exhaust;
    spec
}

#[test]
fn optional_models_hold_free_idle_without_stall_or_fuel_cut() {
    for (all, exhaust) in [(false, false), (true, false), (true, true)] {
        let spec = options(all, exhaust);
        let rate = 48000;
        let mut engine = Engine::new(&spec, rate).unwrap();
        let (mut sum, mut min, mut max, mut cuts) = (0., f64::INFINITY, 0_f64, 0);
        for frame in 0..rate * 10 {
            let s = engine.next(Commands {
                imposed_rpm: (frame < rate).then_some(1200.),
                throttle: 0.,
                ..Default::default()
            });
            assert!(!engine.failed());
            if frame >= rate * 8 {
                sum += s.rpm;
                min = min.min(s.rpm);
                max = max.max(s.rpm);
                cuts += u32::from(s.fuel_cut);
            }
        }
        let mean = sum / f64::from(rate * 2);
        let target = f64::from(spec.idle_rpm);
        println!("all={all} exhaust={exhaust}: idle {mean} [{min},{max}] cuts={cuts}");
        assert!((mean - target).abs() < target * 0.1, "idle {mean}");
        assert!(
            min > target * 0.7 && max - min < target * 0.25,
            "range {min}..{max}"
        );
        assert_eq!(cuts, 0);
    }
}

#[test]
fn torsion_driving_avoids_large_slow_rpm_hunting_near_limiter() {
    for all in [false, true] {
        let mut spec = if all {
            options(true, true)
        } else {
            Scratch::default()
        };
        spec.experimental.torsion = Some(TorsionConfig::default());
        let rate = 8000;
        let model = ScratchModel::build(&spec, rate).unwrap();
        let mut bench = Bench::from_scratch(
            rate,
            Parameters::default(),
            Settings::default(),
            Controls {
                mode: Mode::Simulated,
                automatic: false,
                gear: 1,
                throttle: 0.65,
                ..Default::default()
            },
            model,
        );
        let (mut min, mut max) = (f64::INFINITY, 0_f64);
        for frame in 0..rate * 10 {
            let audio = bench.next(true);
            assert!(audio.is_finite() && !bench.failed());
            if frame >= rate * 8 {
                let rpm = bench.physical_state().unwrap().rpm;
                min = min.min(rpm);
                max = max.max(rpm);
            }
        }
        println!(
            "all={all}: loaded RPM [{min},{max}], speed {}",
            bench.state().speed_kmh
        );
        assert!(bench.state().speed_kmh > 40.);
        assert!(
            max - min < 700. && min > f64::from(spec.redline_rpm) * 0.85,
            "RPM hunting {min}..{max}"
        );
    }
}
