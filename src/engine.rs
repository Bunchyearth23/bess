use crate::project::Parameters;
use bdsp::{
    delay::DelayLine,
    noise::{Noise, NoiseColor},
    svf::{StateVariableFilter, SvfMode},
};
use std::f32::consts::TAU;

const HARMONICS: usize = 96;

/// Stylised four-stroke excitation. Harmonics encode firing events over 720 degrees.
/// This is an acoustic synthesizer, not a thermodynamic solver.
pub struct Engine {
    params: Parameters,
    rate: f32,
    phase: f32,
    rpm: f32,
    load: f32,
    gain: f32,
    coeff: [(f32, f32); HARMONICS],
    noise: Noise,
    exhaust: StateVariableFilter,
    intake: StateVariableFilter,
    dc: StateVariableFilter,
    pipe: DelayLine,
}
impl Engine {
    pub fn new(rate: u32, params: Parameters) -> Self {
        let rate = rate.max(8000) as f32;
        let mut s = Self {
            params,
            rate,
            phase: 0.,
            rpm: params.rpm,
            load: params.load,
            gain: 0.,
            coeff: [(0., 0.); HARMONICS],
            noise: Noise::with_seed(NoiseColor::White, 12345),
            exhaust: StateVariableFilter::new(
                rate,
                params.brightness,
                params.resonance,
                SvfMode::Lowpass,
            ),
            intake: StateVariableFilter::new(rate, 900., 0.8, SvfMode::Bandpass),
            dc: StateVariableFilter::new(rate, 25., 0.707, SvfMode::Highpass),
            pipe: DelayLine::new(rate, 0.04),
        };
        s.set_parameters(params);
        s
    }
    pub fn set_parameters(&mut self, p: Parameters) {
        if p.validate().is_err() {
            return;
        }
        self.params = p;
        self.exhaust.set_cutoff(p.brightness);
        self.exhaust.set_q(p.resonance);
        self.pipe.set_delay_seconds(2. * p.pipe_length / 500.);
        for (h, coeff) in self.coeff.iter_mut().enumerate() {
            let harmonic = (h + 1) as f32;
            let mut re = 0.;
            let mut im = 0.;
            for c in 0..p.cylinders {
                // Alternating gap variation, explicitly a sound-design control.
                let offset = if c % 2 == 1 { p.uneven * 0.5 } else { 0. };
                let angle = TAU * harmonic * (c as f32 + offset) / p.cylinders as f32;
                let (sn, cs) = angle.sin_cos();
                re += cs;
                im += sn;
            }
            let weight = (-harmonic / (p.cylinders as f32 * 9.)).exp() / p.cylinders as f32;
            *coeff = (re * weight, im * weight);
        }
    }
    pub fn next_sample(&mut self, playing: bool) -> f32 {
        let smooth = 1. - (-1. / (self.rate * 0.035)).exp();
        self.rpm += (self.params.rpm - self.rpm) * smooth;
        self.load += (self.params.load - self.load) * smooth;
        self.gain += (if playing { self.params.volume } else { 0. } - self.gain) * smooth;
        let cycle_hz = self.rpm / 120.;
        self.phase = (self.phase + cycle_hz / self.rate).fract();
        let (sn, cs) = (self.phase * TAU).sin_cos();
        let (mut hs, mut hc) = (sn, cs);
        let mut excitation = 0.;
        for (h, &(re, im)) in self.coeff.iter().enumerate() {
            let frequency = (h + 1) as f32 * cycle_hz;
            // Smooth rolloff before Nyquist, including during RPM sweeps.
            let aa = ((0.47 * self.rate - frequency) / (0.07 * self.rate)).clamp(0., 1.);
            excitation += (hc * re + hs * im) * aa;
            (hs, hc) = (hs * cs + hc * sn, hc * cs - hs * sn);
        }
        excitation *= 0.16 * (0.2 + self.load * 0.8);
        let noise = self.noise.next_sample();
        let delayed = self.pipe.read();
        self.pipe.write((excitation + delayed * 0.45).tanh());
        let ex = self.exhaust.next_sample(excitation + delayed * 0.55);
        let intake = self
            .intake
            .next_sample(noise * (0.04 + self.load * 0.14) + excitation * 0.4);
        let mechanical = noise * 0.045 + sn * 0.04;
        let mixed = ex * self.params.exhaust
            + intake * self.params.intake
            + mechanical * self.params.mechanical;
        self.dc.next_sample(mixed).tanh() * self.gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_finite_bounded_and_audible() {
        for preset in 0..4 {
            for rpm in [300., 900., 6000., 12000.] {
                let p = Parameters {
                    rpm,
                    ..Parameters::preset(preset)
                };
                let mut e = Engine::new(48000, p);
                let mut energy = 0.;
                for _ in 0..24000 {
                    let s = e.next_sample(true);
                    assert!(s.is_finite() && s.abs() <= p.volume);
                    energy += s * s;
                }
                assert!(energy > 0.001);
            }
        }
    }
    #[test]
    fn deterministic_and_stop_fades() {
        let mut a = Engine::new(48000, Parameters::default());
        let mut b = Engine::new(48000, Parameters::default());
        for _ in 0..4800 {
            assert_eq!(a.next_sample(true), b.next_sample(true));
        }
        for _ in 0..24000 {
            a.next_sample(false);
        }
        assert!(a.next_sample(false).abs() < 0.00001);
    }
    #[test]
    fn evenly_spaced_firing_cancels_non_engine_orders() {
        let e = Engine::new(48000, Parameters::default());
        for h in 1..=24 {
            let (re, im) = e.coeff[h - 1];
            if h % 4 == 0 {
                assert!(re > 0.4);
            } else {
                assert!(re.abs() + im.abs() < 0.00002);
            }
        }
    }
}
