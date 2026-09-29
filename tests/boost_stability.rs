//! X-027: the matched compressor holds steady charge pressure at part load and
//! low-rpm full load; lift-off without a blow-off valve still flutters.
use bess::{
    engine_build::{Aspiration, BlowOff},
    physical::engine::{Commands, Engine},
    scratch::Scratch,
};

const RATE: u32 = 8000;

fn turbo(bov: BlowOff) -> Scratch {
    let mut scratch = Scratch::default();
    scratch.build.aspiration = Aspiration::Turbo;
    scratch.build.boost_bar = 0.8;
    scratch.build.blow_off = bov;
    scratch
}

/// Charge-pressure (min, max, mean) and mean MAP in kPa, mean turbo rpm and
/// mean-crossing frequency (Hz) over `measure` seconds after `settle` seconds.
fn steady(bov: BlowOff, rpm: f64, throttle: f64, settle: f64, measure: f64) -> [f64; 6] {
    let mut engine = Engine::new(&turbo(bov), RATE).unwrap();
    let command = Commands {
        imposed_rpm: Some(rpm),
        throttle,
        overrun: 0.,
        ..Default::default()
    };
    for _ in 0..(settle * f64::from(RATE)) as usize {
        engine.next(command);
    }
    let n = (measure * f64::from(RATE)) as usize;
    let trace: Vec<_> = (0..n).map(|_| engine.next(command)).collect();
    assert!(!engine.failed(), "{rpm} rpm / {throttle}");
    let charge = trace.iter().map(|s| s.charge_pa);
    let mean = charge.clone().sum::<f64>() / n as f64;
    let crossings = trace
        .windows(2)
        .filter(|w| (w[0].charge_pa - mean) * (w[1].charge_pa - mean) < 0.)
        .count();
    [
        charge.clone().fold(f64::INFINITY, f64::min) / 1e3,
        charge.fold(0., f64::max) / 1e3,
        mean / 1e3,
        trace.iter().map(|s| s.map_pa).sum::<f64>() / n as f64 / 1e3,
        trace.iter().map(|s| s.turbo_rpm).sum::<f64>() / n as f64,
        crossings as f64 / 2. / measure,
    ]
}

/// Charge swing (kPa) and pressure oscillations (1 kPa hysteresis) in the
/// second after lifting off from settled 3000 rpm full load.
fn lift_off(bov: BlowOff) -> (f64, u32) {
    let mut engine = Engine::new(&turbo(bov), RATE).unwrap();
    let mut command = Commands {
        imposed_rpm: Some(3000.),
        throttle: 1.,
        overrun: 0.,
        ..Default::default()
    };
    for _ in 0..3 * RATE {
        engine.next(command);
    }
    command.throttle = 0.;
    command.overrun = 1.;
    let (mut lo, mut hi) = (f64::INFINITY, 0_f64);
    let (mut extreme, mut rising, mut turns) = (engine.state().charge_pa, false, 0);
    for i in 0..RATE {
        let p = engine.next(command).charge_pa;
        if i > RATE / 20 {
            lo = lo.min(p);
            hi = hi.max(p);
        }
        if (rising && p > extreme) || (!rising && p < extreme) {
            extreme = p;
        } else if (p - extreme).abs() > 1000. {
            rising = !rising;
            extreme = p;
            turns += 1;
        }
    }
    assert!(!engine.failed());
    ((hi - lo) / 1e3, turns)
}

#[test]
fn steady_part_load_and_low_rpm_full_load_do_not_surge() {
    let points = [
        (1500., 1.),
        (1800., 1.),
        (2000., 0.3),
        (2500., 1.),
        (3000., 0.5),
        (4000., 1.),
        (1200., 1.),
        (2000., 0.15),
    ];
    let bovs = [BlowOff::Recirculating, BlowOff::None];
    let rows: Vec<_> = bovs
        .iter()
        .flat_map(|&b| points.iter().map(move |&p| (b, p)))
        .collect();
    let results = std::thread::scope(|s| {
        let jobs: Vec<_> = rows
            .iter()
            .map(|&(b, (rpm, throttle))| s.spawn(move || steady(b, rpm, throttle, 8., 1.)))
            .collect();
        jobs.into_iter()
            .map(|j| j.join().unwrap())
            .collect::<Vec<_>>()
    });
    for (&(bov, (rpm, throttle)), &[lo, hi, mean, map, shaft, hz]) in rows.iter().zip(&results) {
        println!(
            "{bov:?} {rpm:.0} rpm / {throttle}: charge {lo:.1}–{hi:.1} kPa (swing {:.1}, mean {mean:.1}, {hz:.0} Hz), MAP {map:.1}, turbo {shaft:.0} rpm",
            hi - lo
        );
    }
    // Before X-027: 35–69 kPa swings at 1500–2500 rpm WOT and 2000 / 0.3,
    // cycling below the firing frequency. What remains is intake pulsation.
    for (&(_, (rpm, throttle)), &[lo, hi, _, _, _, hz]) in rows.iter().zip(&results) {
        assert!(
            hi - lo < 20.,
            "{rpm} rpm / {throttle}: swing {lo:.1}–{hi:.1} kPa"
        );
        assert!(hz > 0.9 * rpm / 30., "{rpm} rpm / {throttle}: {hz} Hz");
    }
}

#[test]
fn lift_off_without_blow_off_still_flutters() {
    let [none, recirc, vent] = std::thread::scope(|s| {
        [BlowOff::None, BlowOff::Recirculating, BlowOff::Atmospheric]
            .map(|b| s.spawn(move || lift_off(b)))
            .map(|j| j.join().unwrap())
    });
    println!("lift-off swing/turns: none {none:?}, recirculating {recirc:?}, atmospheric {vent:?}");
    assert!(none.1 >= 4 && none.0 > 10., "{none:?}");
    assert!(recirc.0 < none.0 * 0.5 && vent.0 < none.0 * 0.5);
}
