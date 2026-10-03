//! Reduced pressure-wave model, excited by the imported recordings.
//! This models acoustic colour, not cylinder pressure or a reconstructed vehicle.
use bdsp::{
    delay::DelayLine,
    svf::{StateVariableFilter, SvfMode},
};

struct Tube {
    forward: DelayLine,
    backward: DelayLine,
    filters: [StateVariableFilter; 2],
    delay: f32,
    target_delay: f32,
    slew: f32,
    loss: f32,
}
impl Tube {
    fn new(rate: f32) -> Self {
        Self {
            forward: DelayLine::new(rate, 0.04),
            backward: DelayLine::new(rate, 0.04),
            filters: std::array::from_fn(|_| {
                StateVariableFilter::new(rate, 6000., 0.707, SvfMode::Lowpass)
            }),
            delay: 1.,
            target_delay: 1.,
            slew: 1. / (rate * 0.06),
            loss: 0.97,
        }
    }
    fn tune(&mut self, length: f32, speed: f32, rate: f32, absorption: f32, initial: bool) {
        self.target_delay = (length * rate / speed).max(1.);
        if initial {
            self.delay = self.target_delay;
        }
        self.loss = 0.995 - absorption * 0.12;
        for f in &mut self.filters {
            f.set_cutoff((8500. - absorption * 6500.).min(rate * 0.4));
        }
    }
    // Both arriving waves are read before any outgoing wave is written.
    fn arrivals(&mut self) -> (f32, f32) {
        self.delay += (self.target_delay - self.delay) * self.slew;
        (
            self.filters[0].next_sample(self.forward.read_at(self.delay)) * self.loss,
            self.filters[1].next_sample(self.backward.read_at(self.delay)) * self.loss,
        )
    }
    fn launch(&mut self, rightward: f32, leftward: f32) {
        self.forward.write(rightward);
        self.backward.write(leftward);
    }
}

/// Junction pressure continuity and volume-flow conservation. Admittance is
/// proportional to area for equal gas density/sound speed on both sides.
fn scatter(left: f32, right: f32, left_weight: f32) -> (f32, f32) {
    let pressure = 2. * (left * left_weight + right * (1. - left_weight));
    (pressure - left, pressure - right)
}

#[derive(Clone, Copy)]
pub struct Geometry {
    pub header: f32,
    pub tail: f32,
    pub diameter_mm: f32,
    pub chamber_litres: f32,
    pub absorption: f32,
    pub resonance: f32,
    pub temperature_c: f32,
}

pub struct Exhaust {
    tubes: [Tube; 3],
    rate: f32,
    weight: f32,
    inlet_reflection: f32,
    outlet_reflection: f32,
}
impl Exhaust {
    pub fn new(rate: f32, g: Geometry) -> Self {
        let mut this = Self {
            tubes: std::array::from_fn(|_| Tube::new(rate)),
            rate,
            weight: 0.5,
            inlet_reflection: 0.4,
            outlet_reflection: -0.6,
        };
        this.tune(g, true);
        this
    }
    pub fn tune(&mut self, g: Geometry, initial: bool) {
        // Ideal-air approximation; effective acoustic temperature is a user control.
        let speed = 331.3 * (1. + g.temperature_c / 273.15).sqrt();
        let area = std::f32::consts::PI * (g.diameter_mm * 0.0005).powi(2);
        let chamber_area = area + g.chamber_litres / (1000. * 0.42);
        self.weight = area / (area + chamber_area);
        self.inlet_reflection = 0.1 + (g.resonance - 0.5) / 3.5 * 0.48;
        self.outlet_reflection = -0.75 + g.absorption * 0.3;
        for (tube, length) in self.tubes.iter_mut().zip([g.header, 0.42, g.tail]) {
            tube.tune(length, speed, self.rate, g.absorption, initial);
        }
    }
    pub fn next(&mut self, excitation: f32) -> f32 {
        let a = self.tubes[0].arrivals();
        let b = self.tubes[1].arrivals();
        let c = self.tubes[2].arrivals();
        let first = scatter(a.0, b.1, self.weight);
        let second = scatter(b.0, c.1, 1. - self.weight);
        self.tubes[0].launch(excitation + a.1 * self.inlet_reflection, first.0);
        self.tubes[1].launch(first.1, second.0);
        self.tubes[2].launch(second.1, c.0 * self.outlet_reflection);
        // Flow proxy at the open termination; downstream radiation is high-passed
        // by Hybrid. There is no fictitious cylinder oscillator in this network.
        c.0 * (1. - self.outlet_reflection) * 0.5
    }
}

/// Historical default propagation-loss duration, seconds. The complete
/// network's measured tail also depends on its geometry and terminations.
const NETWORK_T60: f32 = 0.12;

/// Which parts sit between the collector and the tailpipe of a scratch engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExhaustLayout {
    pub catalyst: bool,
    /// 0 none, 1 straight-through, 2 baffled, 3 reverse-flow.
    pub muffler: u8,
}

#[derive(Clone, Copy)]
enum Role {
    Header,
    Fixed(f32),
    /// Chamber length as a multiple of the length that holds its volume.
    Chamber(f32),
    Tail,
}

struct Segment {
    tube: Tube,
    area: f32,
    role: Role,
    /// Fraction of the header gas temperature (°C) this far downstream.
    cooling: f32,
    absorption: f32,
    target_loss: f32,
}

/// Series of stages, each one or more parallel tubes, joined by N-port
/// pressure-continuity / flow-conservation junctions (admittance ∝ area).
/// Every tube carries its own temperature, hence its own sound speed: hot
/// header, cooler tail. Parallel muffler paths of incommensurate lengths
/// (SDT uses 0.7/0.9/1.1/1.3×) avoid a single tubey comb pattern.
pub struct ExhaustNetwork {
    stages: Vec<Vec<Segment>>,
    rate: f32,
    inlet_reflection: f32,
    end_coefficient: f32,
    end_state: f32,
    forward_in: Vec<Vec<f32>>,
    backward_in: Vec<Vec<f32>>,
    arrivals: Vec<Vec<(f32, f32)>>,
    arrivals_prepared: bool,
    chamber_length_scale: f32,
    decay_seconds: f32,
    loss_fade_remaining: u32,
}

impl ExhaustNetwork {
    pub fn new(rate: f32, g: Geometry, layout: ExhaustLayout) -> Self {
        let pipe = std::f32::consts::PI * (g.diameter_mm * 0.0005).powi(2);
        let segment = |role: Role, area: f32, cooling: f32, absorption: f32| Segment {
            tube: Tube::new(rate),
            area,
            role,
            cooling,
            absorption,
            target_loss: 0.97,
        };
        let mut stages = vec![
            vec![segment(Role::Header, pipe, 1., 0.03)],
            vec![segment(Role::Fixed(0.6), pipe, 0.85, 0.03)],
        ];
        if layout.catalyst {
            // Area step plus monolith dissipation.
            stages.push(vec![segment(Role::Fixed(0.15), pipe * 5., 0.75, 0.6)]);
        }
        stages.push(vec![segment(Role::Fixed(0.8), pipe, 0.6, 0.03)]);
        let chamber = |ratio: f32, scales: &[f32], packing: f32| -> Vec<Segment> {
            scales
                .iter()
                .map(|&k| {
                    segment(
                        Role::Chamber(k),
                        pipe * ratio / scales.len() as f32,
                        0.5,
                        packing,
                    )
                })
                .collect()
        };
        match layout.muffler {
            1 => stages.push(chamber(4., &[1.], 0.85)),
            2 => stages.push(chamber(9., &[0.87, 1.15], 0.5)),
            3 => stages.push(chamber(16., &[0.7, 1., 1.3], 0.65)),
            _ => {}
        }
        stages.push(vec![segment(Role::Tail, pipe, 0.4, 0.03)]);
        let shape = |stages: &Vec<Vec<Segment>>| stages.iter().map(|s| vec![0.; s.len()]).collect();
        let mut this = Self {
            forward_in: shape(&stages),
            backward_in: shape(&stages),
            arrivals: stages.iter().map(|s| vec![(0., 0.); s.len()]).collect(),
            arrivals_prepared: false,
            chamber_length_scale: 1.,
            decay_seconds: NETWORK_T60,
            loss_fade_remaining: 0,
            stages,
            rate,
            inlet_reflection: 0.4,
            end_coefficient: 1.,
            end_state: 0.,
        };
        this.tune(g, true);
        this
    }
    /// Additional acoustic chamber scaling, applied after the legacy geometry
    /// bounds. A value of one preserves existing callers exactly. Call `tune`
    /// afterwards to retarget lengths without clearing propagation state.
    pub fn set_chamber_length_scale(&mut self, scale: f32) {
        self.chamber_length_scale = if scale.is_finite() {
            scale.clamp(0.25, 3.)
        } else {
            1.
        };
    }
    /// Acoustic loss duration, independent of any listening-room effect.
    /// Call `tune` afterwards. Live changes interpolate per-pass losses over
    /// 30 ms; initial tuning adopts the requested losses without a fade.
    pub fn set_decay_ms(&mut self, decay_ms: f32) {
        let seconds = if decay_ms.is_finite() {
            decay_ms.clamp(10., 250.) / 1000.
        } else {
            NETWORK_T60
        };
        if seconds != self.decay_seconds {
            self.decay_seconds = seconds;
            self.loss_fade_remaining = (self.rate * 0.03).ceil().max(1.) as u32;
        }
    }
    pub fn tune(&mut self, g: Geometry, initial: bool) {
        if initial {
            self.loss_fade_remaining = 0;
        }
        self.inlet_reflection = 0.1 + (g.resonance - 0.5) / 3.5 * 0.48;
        let radius = g.diameter_mm * 0.0005;
        let pipe = std::f32::consts::PI * radius * radius;
        let volume_length = (g.chamber_litres / 1000. / (pipe * 9.)).clamp(0.25, 0.7);
        let mut tail_speed = 343.;
        for stage in &mut self.stages {
            for s in stage.iter_mut() {
                let celsius = (g.temperature_c * s.cooling).max(60.);
                let speed = 331.3 * (1. + celsius / 273.15).sqrt();
                let length = match s.role {
                    Role::Header => g.header,
                    Role::Fixed(length) => length,
                    Role::Chamber(scale) => volume_length * scale * self.chamber_length_scale,
                    // Unflanged end correction 0.6133·a.
                    Role::Tail => g.tail + 0.6133 * radius,
                };
                // Bare pipe walls barely damp below 5 kHz; the geometry's
                // absorption setting scales the muffler packing only.
                let absorption = match s.role {
                    Role::Chamber(_) => (s.absorption + g.absorption * 0.3).min(1.),
                    _ => s.absorption,
                };
                let previous_loss = s.tube.loss;
                s.tube.tune(length, speed, self.rate, absorption, initial);
                // User-selected propagation loss (10–250 ms). This per-pass
                // T60 model damps acoustic memory; the full network's measured
                // tail also depends on junctions, wall filters and radiation.
                let pass = length / speed;
                s.target_loss = s.tube.loss.min(0.001f32.powf(pass / self.decay_seconds));
                s.tube.loss = if self.loss_fade_remaining == 0 {
                    s.target_loss
                } else {
                    previous_loss
                };
                tail_speed = speed;
            }
        }
        // |R| falls to ~0.5 near ka = 1; a one-pole low-pass at 0.58·f(ka=1) matches.
        let corner = 0.58 * tail_speed / (std::f32::consts::TAU * radius);
        self.end_coefficient = 1. - (-std::f32::consts::TAU * corner / self.rate).exp();
    }
    /// Most recently prepared inlet wave. Coupled callers must prepare the
    /// current sample before scattering at their external junction.
    pub fn inlet_wave(&self) -> f32 {
        self.arrivals[0][0].1
    }
    /// Read/filter every arriving wave once for the current sample, before an
    /// external junction computes its outgoing wave. Repeated preparation is
    /// idempotent until `next_coupled` (or `next`) launches and consumes it.
    pub fn prepare_inlet(&mut self) -> f32 {
        if !self.arrivals_prepared {
            for (stage, arrivals) in self.stages.iter_mut().zip(&mut self.arrivals) {
                for (s, a) in stage.iter_mut().zip(arrivals.iter_mut()) {
                    if self.loss_fade_remaining > 0 {
                        s.tube.loss +=
                            (s.target_loss - s.tube.loss) / self.loss_fade_remaining as f32;
                    }
                    *a = s.tube.arrivals();
                }
            }
            self.loss_fade_remaining = self.loss_fade_remaining.saturating_sub(1);
            self.arrivals_prepared = true;
        }
        self.inlet_wave()
    }
    pub fn next_coupled(&mut self, excitation: f32) -> f32 {
        self.step_boundary(excitation, 0.)
    }
    pub fn next(&mut self, excitation: f32) -> f32 {
        self.step_boundary(excitation, self.inlet_reflection)
    }
    fn step_boundary(&mut self, excitation: f32, inlet_reflection: f32) -> f32 {
        self.prepare_inlet();
        // Source end: the valve side reflects part of the returning wave.
        self.forward_in[0][0] = excitation + self.arrivals[0][0].1 * inlet_reflection;
        for k in 0..self.stages.len() - 1 {
            let (mut flux, mut admittance) = (0., 0.);
            for (s, a) in self.stages[k].iter().zip(&self.arrivals[k]) {
                flux += s.area * a.0;
                admittance += s.area;
            }
            for (s, a) in self.stages[k + 1].iter().zip(&self.arrivals[k + 1]) {
                flux += s.area * a.1;
                admittance += s.area;
            }
            let junction = 2. * flux / admittance;
            for (i, a) in self.arrivals[k].iter().enumerate() {
                self.backward_in[k][i] = junction - a.0;
            }
            for (i, a) in self.arrivals[k + 1].iter().enumerate() {
                self.forward_in[k + 1][i] = junction - a.1;
            }
        }
        // Open end: R = −1 at DC, |R| ≈ 1 − ½(ka)²; radiate the transmitted part.
        let last = self.stages.len() - 1;
        let outgoing = self.arrivals[last][0].0;
        self.end_state += (outgoing - self.end_state) * self.end_coefficient;
        self.backward_in[last][0] = -self.end_state;
        for (k, stage) in self.stages.iter_mut().enumerate() {
            for (i, s) in stage.iter_mut().enumerate() {
                s.tube.launch(self.forward_in[k][i], self.backward_in[k][i]);
            }
        }
        self.arrivals_prepared = false;
        outgoing - self.end_state
    }
}

/// A lossy intake runner with a load-dependent throttle boundary.
pub struct Intake {
    tube: Tube,
    rate: f32,
    reflection: f32,
}
impl Intake {
    pub fn new(rate: f32, length: f32) -> Self {
        let mut this = Self {
            tube: Tube::new(rate),
            rate,
            reflection: 0.4,
        };
        this.tube.tune(length, 343., rate, 0.45, true);
        this
    }
    pub fn tune(&mut self, length: f32, load: f32, resonance: f32) {
        self.tube
            .tune(length, 343., self.rate, 0.6 - resonance * 0.4, false);
        self.reflection = (0.2 + resonance * 0.55) * (1. - load * 0.45);
    }
    pub fn next(&mut self, excitation: f32) -> f32 {
        let (outward, inward) = self.tube.arrivals();
        self.tube
            .launch(excitation + inward * self.reflection, -outward * 0.65);
        outward * 0.8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decay_fixture() -> Geometry {
        Geometry {
            header: 0.05,
            tail: 1.4,
            diameter_mm: 55.,
            chamber_litres: 6.,
            absorption: 0.4,
            resonance: 1.,
            temperature_c: 400.,
        }
    }

    #[test]
    fn exhaust_decay_orders_impulse_tails_at_native_and_double_rates() {
        let geometry = decay_fixture();
        for rate in [48000., 96000.] {
            let mut fractions = Vec::new();
            for decay in [10., 40., 120., 250.] {
                let mut network = ExhaustNetwork::new(
                    rate,
                    geometry,
                    ExhaustLayout {
                        catalyst: false,
                        muffler: 2,
                    },
                );
                network.set_decay_ms(decay);
                network.tune(geometry, true);
                let (mut total, mut late) = (0_f64, 0_f64);
                for i in 0..(rate * 0.5) as usize {
                    let y = network.next(f32::from(i == 0));
                    assert!(y.is_finite());
                    let energy = f64::from(y).powi(2);
                    total += energy;
                    if i >= (rate * 0.05) as usize {
                        late += energy;
                    }
                }
                assert!(total > 0.);
                fractions.push(late / total);
            }
            assert!(
                fractions.windows(2).all(|w| w[0] < w[1]),
                "{rate}: {fractions:?}"
            );
        }
    }

    #[test]
    fn exhaust_decay_live_retune_preserves_waves_and_ramps_losses() {
        let geometry = decay_fixture();
        let mut network = ExhaustNetwork::new(
            48000.,
            geometry,
            ExhaustLayout {
                catalyst: false,
                muffler: 2,
            },
        );
        for i in 0..4800 {
            network.next((i as f32 * 0.07).sin() * 0.01);
        }
        let before: Vec<_> = network
            .stages
            .iter()
            .flatten()
            .map(|s| {
                (
                    s.tube.loss,
                    s.tube.delay,
                    s.tube.forward.read_at(10.),
                    s.tube.backward.read_at(10.),
                )
            })
            .collect();
        let end_state = network.end_state;
        network.set_decay_ms(10.);
        network.tune(geometry, false);
        assert_eq!(network.end_state, end_state);
        for (s, &(loss, delay, forward, backward)) in network.stages.iter().flatten().zip(&before) {
            assert_eq!(s.tube.loss.to_bits(), loss.to_bits());
            assert_eq!(s.tube.delay.to_bits(), delay.to_bits());
            assert_eq!(s.tube.forward.read_at(10.).to_bits(), forward.to_bits());
            assert_eq!(s.tube.backward.read_at(10.).to_bits(), backward.to_bits());
            assert!(s.target_loss < loss);
        }
        let total_steps = network.loss_fade_remaining;
        network.next(0.);
        for (s, &(loss, ..)) in network.stages.iter().flatten().zip(&before) {
            assert!(s.tube.loss < loss && s.tube.loss > s.target_loss);
            assert!((loss - s.tube.loss) < (loss - s.target_loss) * 0.002);
        }
        for _ in 1..total_steps {
            assert!(network.next(0.).is_finite());
        }
        for s in network.stages.iter().flatten() {
            assert_eq!(s.tube.loss.to_bits(), s.target_loss.to_bits());
        }
    }

    #[test]
    fn explicit_default_decay_is_bit_exact_and_repeated_set_is_inert() {
        let geometry = decay_fixture();
        let layout = ExhaustLayout {
            catalyst: true,
            muffler: 3,
        };
        let mut a = ExhaustNetwork::new(48000., geometry, layout);
        let mut b = ExhaustNetwork::new(48000., geometry, layout);
        b.set_decay_ms(120.);
        assert_eq!(b.decay_seconds.to_bits(), NETWORK_T60.to_bits());
        assert_eq!(b.loss_fade_remaining, 0);
        for i in 0..12000 {
            if i % 128 == 0 {
                b.set_decay_ms(120.);
                b.tune(geometry, false);
            }
            let x = if i < 4800 {
                (i as f32 * 0.17).sin() * 0.01
            } else {
                0.
            };
            assert_eq!(a.next(x).to_bits(), b.next(x).to_bits());
        }
    }
    #[test]
    fn chamber_scaling_is_continuous_past_legacy_bounds_without_reset() {
        for diameter_mm in [35., 100.] {
            let g = Geometry {
                header: 0.05,
                tail: 1.4,
                diameter_mm,
                chamber_litres: 6.,
                absorption: 0.4,
                resonance: 1.,
                temperature_c: 400.,
            };
            let mut network = ExhaustNetwork::new(
                48000.,
                g,
                ExhaustLayout {
                    catalyst: true,
                    muffler: 2,
                },
            );
            for i in 0..1000 {
                network.next(if i == 0 { 0.01 } else { 0. });
            }
            let mut previous_target = 0.;
            for factor in [0.25, 0.26, 0.5, 0.51, 1., 1.01, 2.99, 3.] {
                let delay = network.stages[4][0].tube.delay;
                let end_state = network.end_state;
                network.set_chamber_length_scale(factor);
                network.tune(g, false);
                let target = network.stages[4][0].tube.target_delay;
                assert!(
                    target > previous_target,
                    "{diameter_mm} mm, factor {factor}"
                );
                assert_eq!(network.stages[4][0].tube.delay, delay);
                assert_eq!(network.end_state, end_state);
                previous_target = target;
            }
        }
    }

    #[test]
    fn prepared_external_reflection_matches_standalone_without_double_advance() {
        let geometry = Geometry {
            header: 0.05,
            tail: 1.4,
            diameter_mm: 55.,
            chamber_litres: 6.,
            absorption: 0.4,
            resonance: 1.2,
            temperature_c: 400.,
        };
        for rate in [48_000., 96_000.] {
            let mut g = geometry;
            let layout = ExhaustLayout {
                catalyst: true,
                muffler: 2,
            };
            let mut standalone = ExhaustNetwork::new(rate, g, layout);
            let mut coupled = ExhaustNetwork::new(rate, g, layout);
            let (mut early, mut late) = (0_f64, 0_f64);
            let mut nonzero_return = false;
            for frame in 0..rate as usize {
                if frame == 200 {
                    // Delay slewing and stateful loss filters must also advance
                    // exactly once, even if a caller inspects the port twice.
                    g.temperature_c = 550.;
                    standalone.tune(g, false);
                    coupled.tune(g, false);
                }
                let source = if frame < 100 {
                    (frame as f32 * 0.37).sin() * 0.01
                } else {
                    0.
                };
                let incoming = coupled.prepare_inlet();
                nonzero_return |= incoming.abs() > 1e-8;
                assert_eq!(incoming.to_bits(), coupled.prepare_inlet().to_bits());
                let expected = standalone.next(source);
                let actual = coupled.next_coupled(source + incoming * coupled.inlet_reflection);
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "frame {frame}, rate {rate}"
                );
                assert!(actual.is_finite());
                if frame < rate as usize / 4 {
                    early += f64::from(actual).powi(2);
                }
                if frame > rate as usize * 3 / 4 {
                    late += f64::from(actual).powi(2);
                }
            }
            assert!(nonzero_return && early > 0.);
            assert!(late < early * 1e-6, "tail failed to decay: {late}/{early}");
        }
    }
    #[test]
    fn junction_conserves_energy_for_different_pipe_areas() {
        for w in [0.001, 0.02, 0.1, 0.5, 0.9, 0.999] {
            for (a, b) in [(1., 0.), (0., 1.), (-0.4, 0.7)] {
                let (x, y) = scatter(a, b, w);
                let input = a * a * w + b * b * (1. - w);
                let output = x * x * w + y * y * (1. - w);
                assert!((input - output).abs() < 1e-6);
            }
        }
    }
    #[test]
    fn exhaust_networks_are_stable_and_mufflers_quieten_the_top_end() {
        let g = Geometry {
            header: 0.5,
            tail: 1.2,
            diameter_mm: 55.,
            chamber_litres: 6.,
            absorption: 0.4,
            resonance: 1.2,
            temperature_c: 600.,
        };
        let energy = |muffler: u8, freq: f32| {
            let mut line = ExhaustNetwork::new(
                48_000.,
                g,
                ExhaustLayout {
                    catalyst: true,
                    muffler,
                },
            );
            let mut out = 0f32;
            for i in 0..48_000 {
                let x = (std::f32::consts::TAU * freq * i as f32 / 48_000.).sin();
                let y = line.next(x);
                assert!(y.is_finite() && y.abs() < 20., "{muffler} {freq}");
                if i > 24_000 {
                    out += y * y;
                }
            }
            out
        };
        for muffler in 0..4 {
            for freq in [40., 300., 3000.] {
                energy(muffler, freq);
            }
        }
        assert!(energy(3, 3000.) < energy(0, 3000.) * 0.5);
        // An impulse dies away (no unstable loop through the junctions).
        let mut line = ExhaustNetwork::new(
            48_000.,
            g,
            ExhaustLayout {
                catalyst: true,
                muffler: 3,
            },
        );
        let tail: f32 = (0..96_000)
            .map(|i| line.next(f32::from(i == 0)))
            .skip(72_000)
            .map(|y| y * y)
            .sum();
        assert!(tail < 1e-6, "{tail}");
    }
    #[test]
    fn open_end_radiates_as_a_high_pass_and_stays_stable() {
        let g = Geometry {
            header: 0.5,
            tail: 1.8,
            diameter_mm: 55.,
            chamber_litres: 4.,
            absorption: 0.4,
            resonance: 1.2,
            temperature_c: 400.,
        };
        let gain = |freq: f32| {
            let mut line = ExhaustNetwork::new(
                48_000.,
                g,
                ExhaustLayout {
                    catalyst: false,
                    muffler: 0,
                },
            );
            let (mut input, mut output) = (0f32, 0f32);
            for i in 0..48_000 * 2 {
                let x = (std::f32::consts::TAU * freq * i as f32 / 48_000.).sin();
                let y = line.next(x);
                assert!(y.is_finite() && y.abs() < 10.);
                if i > 48_000 {
                    input += x * x;
                    output += y * y;
                }
            }
            (output / input).sqrt()
        };
        // Radiation efficiency rises with frequency (ka ≪ 1 radiates poorly).
        assert!(
            gain(2500.) > gain(60.) * 2.,
            "{} vs {}",
            gain(2500.),
            gain(60.)
        );
    }
    #[test]
    fn acoustic_tail_decays_at_extreme_geometries_and_device_rates() {
        for rate in [8000., 44100., 48000., 192000.] {
            for (diameter, volume) in [(30., 18.), (130., 0.3)] {
                let mut line = Exhaust::new(
                    rate,
                    Geometry {
                        header: 1.5,
                        tail: 5.,
                        diameter_mm: diameter,
                        chamber_litres: volume,
                        absorption: 0.,
                        resonance: 4.,
                        temperature_c: 150.,
                    },
                );
                let mut early = 0f32;
                let mut late = 0f32;
                for i in 0..(rate as usize * 3) {
                    let s = line.next(if i == 0 { 1. } else { 0. });
                    assert!(s.is_finite() && s.abs() < 2.);
                    if i < rate as usize {
                        early += s * s;
                    }
                    if i > rate as usize * 2 {
                        late += s * s;
                    }
                }
                assert!(
                    early > 1e-8 && late < early * 1e-4,
                    "{rate}: {early} {late}"
                );
            }
        }
    }
}
