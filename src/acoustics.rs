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
