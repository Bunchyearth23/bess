//! Observation of intake flow and mechanical contacts. Never feeds the solver.
//! Airbox damping removes sharp flow corners; turbulence requires real flow.
//! Finite contact bursts replace identical one-sample mechanical impulses.
use bdsp::svf::{StateVariableFilter, SvfMode};

pub(crate) struct Radiation {
    intake_body: StateVariableFilter,
    breath: StateVariableFilter,
    breath_airbox: StateVariableFilter,
    flow_envelope: f32,
    contact_envelope: f32,
    flow_step: f32,
    contact_decay: f32,
    noise_scale: f32,
    seed: u64,
}
impl Radiation {
    pub fn new(rate: u32, seed: u64) -> Self {
        let rate = rate as f32;
        Self {
            intake_body: StateVariableFilter::new(rate, 750., 0.707, SvfMode::Lowpass),
            breath: StateVariableFilter::new(rate, 900., 0.8, SvfMode::Bandpass),
            breath_airbox: StateVariableFilter::new(
                rate,
                2800_f32.min(rate * 0.35),
                0.707,
                SvfMode::Lowpass,
            ),
            flow_envelope: 0.,
            contact_envelope: 0.,
            flow_step: 1. - (-1. / (rate * 0.002)).exp(),
            contact_decay: (-1. / (rate * 0.0015)).exp(),
            noise_scale: (rate / 48000.).sqrt(),
            seed: seed ^ 0x5e2d_908f_7531_b4a9,
        }
    }
    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / 8388608. - 1.
    }
    pub fn next(&mut self, flow_ac: f32, flow: f32, impact: f32) -> (f32, f32) {
        self.flow_envelope += (flow.abs().min(0.2) - self.flow_envelope) * self.flow_step;
        self.contact_envelope = self.contact_envelope * self.contact_decay + impact;
        let air_noise = self.noise() * self.noise_scale;
        let contact_noise = self.noise() * self.noise_scale;
        let breath = self
            .breath_airbox
            .next_sample(self.breath.next_sample(air_noise));
        let intake =
            self.intake_body.next_sample(flow_ac) * 0.06 + breath * self.flow_envelope * 0.4;
        // The existing material/pitch resonator receives a short contact burst,
        // with a small coherent onset. No free-running tone or independent hiss.
        let mechanical = impact * 0.12 + contact_noise * self.contact_envelope * 0.14;
        (intake, mechanical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_flow_or_contacts_cannot_generate_sound_and_tails_decay() {
        for rate in [8000, 48000, 96000, 384000] {
            let mut r = Radiation::new(rate, 13);
            for _ in 0..1000 {
                assert_eq!(r.next(0., 0., 0.), (0., 0.));
            }
            for _ in 0..1000 {
                let (a, b) = r.next(0.1, 0.1, 0.01);
                assert!(a.is_finite() && b.is_finite());
            }
            for _ in 0..rate {
                r.next(0., 0., 0.);
            }
            let (a, b) = r.next(0., 0., 0.);
            assert!(a.abs() < 1e-10 && b.abs() < 1e-10);
        }
    }
    #[test]
    fn seed_is_reproducible_and_periodic_events_do_not_repeat_an_identical_wave() {
        let mut a = Radiation::new(48000, 15);
        let mut b = Radiation::new(48000, 15);
        let mut previous = [0.; 480];
        let mut difference = 0.;
        let mut power = 0.;
        for i in 0..48000 {
            let phase = i % 480;
            let flow = (phase as f32 / 480. * std::f32::consts::TAU).sin() * 0.03;
            let impulse = if phase == 0 { 0.015 } else { 0. };
            let x = a.next(flow, 0.04 + flow, impulse);
            assert_eq!(x, b.next(flow, 0.04 + flow, impulse));
            if i > 24000 {
                difference += (x.1 - previous[phase]).powi(2);
                power += x.1 * x.1;
            }
            previous[phase] = x.1;
        }
        assert!(power > 1e-6 && difference > power * 0.5);
    }
}
