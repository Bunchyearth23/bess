//! Full-load dyno sweep of a scratch engine held at imposed crank speeds.
//! Brake torque = cylinder gas torque − crank friction, averaged over whole
//! 720° cycles once manifold and turbo states settle (accessories off; the
//! reciprocating inertia torque averages to zero over a cycle and is omitted).
use crate::engine_build::Aspiration;
use crate::physical::engine::{Commands, Engine};
use crate::scratch::{Scratch, SoundTuning};

/// The lowest rate the physical engine accepts. Its gas/crank substeps stay
/// at ≥96 kHz for any rate, so only torque sampling changes (<0.5 % vs 48 kHz).
const RATE: u32 = 8000;
/// Consecutive window means must agree this closely (fraction): brake torque,
/// and for a turbo the shaft speed over windows of at least `TURBO_WINDOW_S`.
const TOLERANCE: f64 = 0.005;
const TURBO_TOLERANCE: f64 = 0.002;
const TURBO_WINDOW_S: f64 = 0.1;
/// Simulated settling budget per point; a turbo still spooling is cut here.
const MAX_SETTLE_S: f64 = 4.;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curve {
    pub rpm: Vec<f64>,
    pub torque_nm: Vec<f64>,
    pub power_kw: Vec<f64>,
    /// Mean intake manifold absolute pressure.
    pub map_kpa: Vec<f64>,
    /// (value, rpm)
    pub peak_torque: (f64, f64),
    pub peak_power: (f64, f64),
}

impl Curve {
    /// Linearly interpolated (torque Nm, power kW) at `rpm`, clamped to the sweep.
    pub fn at(&self, rpm: f64) -> (f64, f64) {
        let i = self
            .rpm
            .partition_point(|&r| r < rpm)
            .clamp(1, self.rpm.len() - 1);
        let (a, b) = (self.rpm[i - 1], self.rpm[i]);
        let t = ((rpm - a) / (b - a)).clamp(0., 1.);
        let lerp = |v: &[f64]| v[i - 1] + (v[i] - v[i - 1]) * t;
        (lerp(&self.torque_nm), lerp(&self.power_kw))
    }
}

/// The scratch fields that change full-load torque; sound-only and crank
/// inertia settings are reset, so tone tweaks never trigger a new sweep.
pub fn key(scratch: &Scratch) -> Scratch {
    let mut key = scratch.clone();
    let sound = scratch.sound;
    key.sound = SoundTuning {
        cycle_variation: sound.cycle_variation,
        combustion_duration: sound.combustion_duration,
        ignition_retard_deg: sound.ignition_retard_deg,
        ..SoundTuning::default()
    };
    key.inertia = Scratch::default().inertia;
    key.experimental = Default::default();
    key
}

/// Wide-open-throttle sweep over `points` speeds from idle to just below the
/// rev limiter. One engine is walked upwards through all speeds so each point
/// starts from its neighbour's manifold, turbo and wall state.
pub fn sweep(scratch: &Scratch, points: usize) -> Result<Curve, String> {
    sweep_with(scratch, points, |_| true)
}

/// `sweep`, calling `proceed(done_points)` before each point; false cancels.
pub fn sweep_with(
    scratch: &Scratch,
    points: usize,
    mut proceed: impl FnMut(usize) -> bool,
) -> Result<Curve, String> {
    let points = points.max(2);
    let mut engine = Engine::new(&key(scratch), RATE)?;
    let (idle, redline) = (
        f64::from(scratch.idle_rpm),
        f64::from(scratch.redline_rpm) - 1.,
    );
    let mut curve = Curve::default();
    let boosted = scratch.build.aspiration != Aspiration::Natural;
    let close = |a: f64, b: f64, floor: f64, tolerance: f64| {
        (a - b).abs() <= tolerance * a.abs().max(floor)
    };
    for i in 0..points {
        if !proceed(i) {
            return Err("Dyno sweep cancelled".into());
        }
        let rpm = idle + (redline - idle) * i as f64 / (points - 1) as f64;
        let cycle = 120. / rpm;
        // Manifolds settle within a cycle or two; a turbo shaft takes seconds.
        let span = if boosted {
            (TURBO_WINDOW_S / cycle).ceil()
        } else {
            1.
        };
        let mut last = window(&mut engine, rpm, span)?;
        let mut elapsed = span * cycle;
        let (torque, map_kpa) = loop {
            let next = window(&mut engine, rpm, span)?;
            elapsed += span * cycle;
            let done = close(next.0, last.0, 10., TOLERANCE)
                && close(next.2, last.2, 1000., TURBO_TOLERANCE);
            let mean = ((last.0 + next.0) / 2., (last.1 + next.1) / 2.);
            last = next;
            if done || elapsed > MAX_SETTLE_S {
                break mean;
            }
        };
        curve.rpm.push(rpm);
        curve.torque_nm.push(torque);
        curve
            .power_kw
            .push(torque * rpm * std::f64::consts::TAU / 60. / 1000.);
        curve.map_kpa.push(map_kpa);
    }
    let peak = |v: &[f64]| {
        let i = (0..v.len())
            .max_by(|&a, &b| v[a].total_cmp(&v[b]))
            .unwrap_or(0);
        (v[i], curve.rpm[i])
    };
    curve.peak_torque = peak(&curve.torque_nm);
    curve.peak_power = peak(&curve.power_kw);
    Ok(curve)
}

/// Mean (brake torque Nm, MAP kPa) over exactly `cycles` 720° cycles, the
/// final sample weighted by its fraction inside the window, and turbo rpm.
fn window(engine: &mut Engine, rpm: f64, cycles: f64) -> Result<(f64, f64, f64), String> {
    let length = cycles * 120. * f64::from(RATE) / rpm;
    let (mut torque, mut map, mut t, mut turbo) = (0., 0., 0., 0.);
    while t < length {
        let sample = engine.next(Commands {
            imposed_rpm: Some(rpm),
            throttle: 1.,
            overrun: 0.,
            ..Default::default()
        });
        let weight = (length - t).min(1.);
        torque += weight * (sample.torque_nm - engine.friction_nm());
        map += weight * sample.map_pa;
        turbo = sample.turbo_rpm;
        t += 1.;
    }
    if engine.failed() || !torque.is_finite() {
        return Err(format!("Dyno: the physical engine failed at {rpm:.0} rpm"));
    }
    Ok((torque / length, map / length / 1000., turbo))
}
