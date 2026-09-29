//! Per-cylinder intake runners, plenum and airbox radiated from the open
//! snorkel, or from each trumpet with individual throttles. Linear acoustic
//! perturbations ride on the 0D mean state. This is not 1D CFD.
use crate::{
    engine_build::{EngineBuild, Throttle},
    scratch::SoundTuning,
};
use bdsp::delay::DelayLine;
use std::f64::consts::PI;

// Sound speed of ~305 K intake air. Charge heating shifts it by a few percent.
const C: f64 = 350.;
// Open valve: cylinder volume and valve orifice absorb part of each wave.
const OPEN_REFLECTION: f64 = 0.4;
// Radiated Pa at 1 m to output sample, fixed. Centred on the former 750 Hz /
// x0.06 summed-flow layer (default I4, 96 kHz, 2 s RMS): 0.119 Pa at 850 rpm /
// 0.1 and 2.50 Pa at 3000 / 0.7 land -4.2 and +4.2 dB from it. The spread is
// radiation (d/dt) plus the ~99 Hz plenum mode meeting 100 Hz firing.
const PA_TO_SAMPLE: f64 = 1.08e-3;

struct Tube {
    forward: DelayLine,
    backward: DelayLine,
    area: f64,
    state: [f64; 2],
}
impl Tube {
    fn new(rate: f64, area: f64) -> Self {
        Self {
            forward: DelayLine::new(rate as f32, 0.01),
            backward: DelayLine::new(rate as f32, 0.01),
            area,
            state: [0.; 2],
        }
    }
    /// Waves arriving at the far and near ends, with viscothermal losses.
    /// The loss low-pass group delay, (1 - g) / g samples, is taken off the line.
    fn read(&mut self, delay: f64, filter: f64, loss: f64) -> [f64; 2] {
        let delay = (delay - (1. - filter) / filter).max(1.) as f32;
        let x = [self.forward.read_at(delay), self.backward.read_at(delay)];
        for (state, x) in self.state.iter_mut().zip(x) {
            *state += filter * (f64::from(x) - *state);
        }
        self.state.map(|s| s * loss)
    }
    fn write(&mut self, near: f64, far: f64) {
        self.forward.write(near.clamp(-200000., 200000.) as f32);
        self.backward.write(far.clamp(-200000., 200000.) as f32);
    }
}

struct Runner {
    tube: Tube,
    previous_flow: f64,
    ac_flow: f64,
    open: f64,
    mouth: f64,
}

/// Throttle duct, airbox volume and open snorkel behind the plenum.
struct Airbox {
    duct: Tube,
    snorkel: Tube,
    // Volume * rate / c: backward-Euler compliance of plenum and airbox.
    compliance: [f64; 2],
    pressure: [f64; 2],
    delay: [f64; 2],
    loss: [f64; 2],
    mouth: f64,
    mouth_filter: f64,
}

pub struct IntakeAcoustic {
    runners: Vec<Runner>,
    airbox: Option<Airbox>,
    rate: f64,
    length_m: f64,
    target_length_m: f64,
    length_scale: f64,
    trumpet_filter: f64,
    filter: f64,
    pole: f64,
    open_step: f64,
    length_slew: f64,
    previous_volume_flow: f64,
}

fn loss(length_m: f64) -> f64 {
    // T60 0.12 s of travel time, as in the exhaust primaries.
    0.001_f64.powf(length_m / C / 0.12)
}
// One-pole reflection low-pass at ka = 1: |R| ~ 1 - (ka)²/2 at an open end.
// Its a/c phase delay doubles as a ~0.5 a end correction.
fn open_end(rate: f64, radius_m: f64) -> f64 {
    1. - (-C / (radius_m * rate)).exp()
}

impl IntakeAcoustic {
    pub fn new(
        rate: u32,
        cylinders: usize,
        build: &EngineBuild,
        displacement_m3: f64,
        tuning: &SoundTuning,
    ) -> Self {
        let rate = f64::from(rate);
        let runner_radius = f64::from(build.bore_mm) * 0.47e-3 * 0.5;
        let runner_area = PI * runner_radius.powi(2);
        let individual = build.throttle == Throttle::Individual;
        // Individual throttles: short runners open through trumpets (the old
        // voice used 0.18 m against 0.38 m for a plenum).
        let length_scale = if individual { 0.5 } else { 1. };
        // Throttle bore ~60 mm for 2 L; area follows displacement.
        let throttle_radius = 0.03 * (displacement_m3 / 0.002).sqrt();
        let throttle_area = PI * throttle_radius.powi(2);
        let duct = 0.35;
        let snorkel = 0.25;
        let length = f64::from(tuning.intake_length_m) * length_scale;
        Self {
            runners: (0..cylinders)
                .map(|_| Runner {
                    tube: Tube::new(rate, runner_area),
                    previous_flow: 0.,
                    ac_flow: 0.,
                    open: 0.,
                    mouth: 0.,
                })
                .collect(),
            // Plenum volume as in `Manifolds`; airbox about four displacements.
            airbox: (!individual).then(|| Airbox {
                duct: Tube::new(rate, throttle_area),
                snorkel: Tube::new(rate, throttle_area),
                compliance: [1.25, 4.].map(|v| v * displacement_m3 * rate / C),
                pressure: [0.; 2],
                delay: [duct, snorkel].map(|l| l * rate / C),
                loss: [loss(duct), loss(snorkel)],
                mouth: 0.,
                mouth_filter: open_end(rate, throttle_radius),
            }),
            rate,
            length_m: length,
            target_length_m: length,
            length_scale,
            trumpet_filter: open_end(rate, runner_radius),
            filter: 1. - (-std::f64::consts::TAU * 7000. / rate).exp(),
            pole: (-std::f64::consts::TAU * 12. / rate).exp(),
            open_step: 1. - (-1. / (0.001 * rate)).exp(),
            length_slew: 1. - (-1. / (0.05 * rate)).exp(),
            previous_volume_flow: 0.,
        }
    }

    /// Retarget runner length without clearing waves; it slews over 50 ms.
    pub fn retune(&mut self, tuning: &SoundTuning) {
        self.target_length_m = f64::from(tuning.intake_length_m) * self.length_scale;
    }

    /// Valve mass flows (kg/s, into the cylinder) to radiated pressure.
    pub fn next(&mut self, flow_kg_s: &[f64; 12]) -> f32 {
        self.length_m += self.length_slew * (self.target_length_m - self.length_m);
        let delay = self.length_m * self.rate / C;
        let runner_loss = loss(self.length_m);
        let mut arrivals = [[0.; 2]; 12];
        for (arrival, r) in arrivals.iter_mut().zip(&mut self.runners) {
            *arrival = r.tube.read(delay, self.filter, runner_loss);
        }
        let mut far = [0.; 12];
        let mut volume_flow = 0.;
        if let Some(a) = &mut self.airbox {
            let duct = a.duct.read(a.delay[0], self.filter, a.loss[0]);
            let snorkel = a.snorkel.read(a.delay[1], self.filter, a.loss[1]);
            // Compliant junction, areas as admittances (rho*c cancels):
            // V/c dp/dt = sum A (2 a - p), backward Euler, passive.
            let mut weighted = a.duct.area * duct[1];
            let mut area = a.duct.area;
            for (arrival, r) in arrivals.iter().zip(&self.runners) {
                weighted += r.tube.area * arrival[0];
                area += r.tube.area;
            }
            a.pressure[0] =
                (a.compliance[0] * a.pressure[0] + 2. * weighted) / (a.compliance[0] + area);
            a.pressure[1] = (a.compliance[1] * a.pressure[1]
                + 2. * (a.duct.area * duct[0] + a.snorkel.area * snorkel[1]))
                / (a.compliance[1] + a.duct.area + a.snorkel.area);
            a.mouth += a.mouth_filter * (snorkel[0] - a.mouth);
            a.duct
                .write(a.pressure[0] - duct[1], a.pressure[1] - duct[0]);
            a.snorkel.write(a.pressure[1] - snorkel[1], -a.mouth);
            volume_flow = a.snorkel.area * (snorkel[0] + a.mouth);
            for (f, arrival) in far.iter_mut().zip(&arrivals) {
                *f = a.pressure[0] - arrival[0];
            }
        } else {
            for ((f, arrival), r) in far.iter_mut().zip(&arrivals).zip(&mut self.runners) {
                r.mouth += self.trumpet_filter * (arrival[0] - r.mouth);
                *f = -r.mouth;
                volume_flow += r.tube.area * (arrival[0] + r.mouth);
            }
        }
        for (i, r) in self.runners.iter_mut().enumerate() {
            // Remove the slowly varying mean already represented by 0D mass.
            r.ac_flow = flow_kg_s[i] - r.previous_flow + self.pole * r.ac_flow;
            r.previous_flow = flow_kg_s[i];
            let open = if flow_kg_s[i].abs() > 1e-9 { 1. } else { 0. };
            r.open += self.open_step * (open - r.open);
            let reflection = 1. - (1. - OPEN_REFLECTION) * r.open;
            // Flow source Q = -mdot/rho in parallel with the valve impedance:
            // p+ = R p- + Z Q (1 + R) / 2, Z Q = -c mdot / A. Closed: rigid.
            let source = -C / r.tube.area * r.ac_flow;
            r.tube.write(
                reflection * arrivals[i][1] + source * 0.5 * (1. + reflection),
                far[i],
            );
        }
        // Monopole at 1 m: p = rho dQ/dt / (4 pi), Q = A (p+ - p-) / (rho c).
        let pressure = (volume_flow - self.previous_volume_flow) * self.rate / (4. * PI * C);
        self.previous_volume_flow = volume_flow;
        (pressure * PA_TO_SAMPLE) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(rate: u32, throttle: Throttle, length: f32) -> IntakeAcoustic {
        let build = EngineBuild {
            throttle,
            ..Default::default()
        };
        let tuning = SoundTuning {
            intake_length_m: length,
            ..Default::default()
        };
        IntakeAcoustic::new(rate, 4, &build, 0.002, &tuning)
    }
    fn impulse(n: &mut IntakeAcoustic, samples: u32) -> Vec<f64> {
        (0..samples)
            .map(|i| {
                let mut flow = [0.; 12];
                if i == 0 {
                    flow[0] = 0.01;
                }
                let y = f64::from(n.next(&flow));
                assert!(y.is_finite());
                y
            })
            .collect()
    }
    fn energy(x: &[f64]) -> f64 {
        x.iter().map(|v| v * v).sum()
    }
    // Spectral maxima within 30 dB of the strongest, 40 Hz-2 kHz, 1 Hz steps.
    fn peaks(x: &[f64], rate: f64) -> Vec<f64> {
        let power: Vec<f64> = (40..2000)
            .map(|f| {
                let w = std::f64::consts::TAU * f64::from(f) / rate;
                let (re, im) = x.iter().enumerate().fold((0., 0.), |(re, im), (n, v)| {
                    (re + v * (w * n as f64).cos(), im - v * (w * n as f64).sin())
                });
                re * re + im * im
            })
            .collect();
        let max = power.iter().copied().fold(0., f64::max);
        (1..power.len() - 1)
            .filter(|&k| power[k] > power[k - 1] && power[k] >= power[k + 1])
            .filter(|&k| power[k] > max * 1e-3)
            .map(|k| k as f64 + 40.)
            .collect()
    }

    #[test]
    fn silence_stays_silent_and_extreme_lengths_decay_at_all_rates() {
        for rate in [8000, 48000, 96000, 384000] {
            for throttle in [Throttle::Single, Throttle::Individual] {
                for length in [0.15, 1.5] {
                    let mut n = network(rate, throttle, length);
                    for _ in 0..1000 {
                        assert_eq!(n.next(&[0.; 12]), 0.);
                    }
                    let x = impulse(&mut n, rate);
                    let early = energy(&x[..x.len() / 4]);
                    let late = energy(&x[x.len() * 3 / 4..]);
                    assert!(
                        early > 0. && late < early * 1e-8,
                        "{rate} {throttle:?} {length}: {late}/{early}"
                    );
                }
            }
        }
    }

    #[test]
    fn runner_length_moves_the_quarter_wave_resonance() {
        // Trumpets isolate the runner: closed valve, open end, c/(4 L').
        let mut previous = f64::INFINITY;
        for length in [0.3, 0.6, 1.2] {
            let x = impulse(&mut network(48000, Throttle::Individual, length), 12000);
            let effective = f64::from(length) * 0.5 + 0.6 * 86. * 0.47e-3 * 0.5;
            let expected = C / (4. * effective);
            let found = peaks(&x, 48000.)[0];
            println!("ITB runner {effective:.3} m: first peak {found} Hz, c/4L {expected:.0} Hz");
            assert!((found / expected - 1.).abs() < 0.1, "{found} vs {expected}");
            assert!(found < previous);
            previous = found;
        }
        let x = impulse(&mut network(48000, Throttle::Single, 0.38), 12000);
        println!("plenum/airbox default peaks {:?} Hz", peaks(&x, 48000.));
    }

    #[test]
    fn per_cylinder_drive_differs_from_summed_drive() {
        // Valve states differ per runner; the plenum and airbox low-pass hide
        // most of that at the snorkel, trumpets radiate it directly.
        for (throttle, minimum) in [(Throttle::Single, 3e-4), (Throttle::Individual, 3e-2)] {
            let mut split = network(48000, throttle, 0.38);
            let mut summed = network(48000, throttle, 0.38);
            let (mut difference, mut power) = (0., 0.);
            // 3000 rpm I4: each valve open for a 180° half-sine, 1-3-4-2.
            for i in 0..48000 {
                let cycle = (i % 1920) as f64 / 1920.;
                let mut flow = [0.; 12];
                for (k, cylinder) in [0, 2, 3, 1].into_iter().enumerate() {
                    let phase = (cycle - k as f64 * 0.25).rem_euclid(1.) * 4.;
                    if phase < 1. {
                        flow[cylinder] = 0.03 * (phase * PI).sin();
                    }
                }
                let a = f64::from(split.next(&flow));
                let mut sum = [0.; 12];
                sum[0] = flow.iter().sum();
                let b = f64::from(summed.next(&sum));
                if i > 24000 {
                    difference += (a - b).powi(2);
                    power += a * a;
                }
            }
            println!(
                "{throttle:?}: split/summed difference {}",
                difference / power
            );
            assert!(power > 0. && difference > power * minimum, "{throttle:?}");
        }
    }

    #[test]
    fn retune_slews_without_clearing_waves() {
        let mut n = network(48000, Throttle::Single, 0.38);
        impulse(&mut n, 100);
        let before = n.length_m;
        let wave = n.runners[0].tube.forward.read_at(10.);
        n.retune(&SoundTuning {
            intake_length_m: 1.2,
            ..Default::default()
        });
        assert_eq!(n.length_m, before);
        assert_eq!(n.runners[0].tube.forward.read_at(10.), wave);
        for _ in 0..2400 {
            n.next(&[0.; 12]);
        }
        let target = f64::from(1.2_f32);
        let remaining = (target - n.length_m) / (target - before);
        assert!((remaining - (-1_f64).exp()).abs() < 1e-6);
    }
}
