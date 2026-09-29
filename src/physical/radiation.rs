//! Observation of intake flow and mechanical contacts. Never feeds the solver.
//! Airbox damping removes sharp flow corners; turbulence requires real flow.
//! Finite contact bursts replace identical one-sample mechanical impulses.
//! A small inharmonic modal bank, not one resonator, rings for every contact.
use crate::engine_build::BlockMaterial;
use bdsp::svf::{StateVariableFilter, SvfMode};

// Shell/plate-like inharmonic ratios around the mechanical pitch control.
const RATIOS: [f32; 8] = [0.45, 0.68, 1., 1.32, 1.74, 2.21, 2.79, 3.5];
// Sets the neutral-default level near the former single 2.4 kHz / Q 2 SVF,
// measured by `neutral_level_matches_the_former_single_resonator`.
const LEVEL: f32 = 0.21;

/// Inline modal bank. Clone/retune never allocate.
#[derive(Clone)]
pub(crate) struct Modes {
    modes: [StateVariableFilter; 8],
    ratio: [f32; 8],
    q_scale: [f32; 8],
    head: [f32; 8],
    block: [f32; 8],
    gain: [f32; 8],
    limit_hz: f32,
}
impl Modes {
    pub fn new(
        rate: f32,
        pitch_hz: f32,
        resonance: f32,
        block: BlockMaterial,
        bore_mm: f32,
        seed: u64,
    ) -> Self {
        // Cast iron: slower waves, better damped, weaker low modes.
        let (stiffness, damping, low) = match block {
            BlockMaterial::CastIron => (0.9, 0.7, 0.8),
            BlockMaterial::Aluminium => (1., 1., 1.),
        };
        let size = (86. / bore_mm.max(1.)).powf(0.25);
        let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
        let mut jitter = || {
            // splitmix64 → ±6 % per mode, fixed per engine seed.
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            1. + 0.06 * (((z ^ (z >> 31)) >> 40) as f32 / 8388608. - 1.)
        };
        let mut modes = Self {
            modes: std::array::from_fn(|_| {
                StateVariableFilter::new(rate, 1000., 1., SvfMode::Bandpass)
            }),
            ratio: RATIOS.map(|r| r * jitter() * size * stiffness),
            // Higher modes lose energy faster: Q falls with ratio.
            q_scale: RATIOS.map(|r| 1.5 * damping * r.powf(-0.35)),
            // Band-pass skirts already add brightness per mode: on white input
            // head spans ~3.8 kHz centroid (legacy SVF 3.3 kHz), block ~2.7 kHz.
            // Valvetrain/injector contacts are small and hard: upper modes.
            head: RATIOS.map(|r| r.powf(-1.2)),
            // Structure-borne combustion drives the large low/mid modes.
            block: RATIOS.map(|r| r.powf(-1.8) * if r < 1.1 { low } else { 1. }),
            gain: [LEVEL; 8],
            limit_hz: rate * 0.4,
        };
        modes.retune(pitch_hz, resonance);
        modes
    }
    pub fn retune(&mut self, pitch_hz: f32, resonance: f32) {
        for i in 0..8 {
            let hz = pitch_hz * self.ratio[i];
            // Modes past 0.4 fs are muted, not piled up on the limit.
            self.gain[i] = if hz <= self.limit_hz { LEVEL } else { 0. };
            self.modes[i].set_cutoff(hz.min(self.limit_hz));
            self.modes[i].set_q(resonance * self.q_scale[i]);
        }
    }
    pub fn next(&mut self, head: f32, block: f32) -> f32 {
        let mut out = 0.;
        for i in 0..8 {
            out += self.modes[i].next_sample(head * self.head[i] + block * self.block[i])
                * self.gain[i];
        }
        out
    }
}

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

    fn modes(seed: u64) -> Modes {
        Modes::new(48000., 2400., 2., BlockMaterial::Aluminium, 86., seed)
    }
    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }
    // Mean-square frequency proxy: E[dx²] / E[x²] rises with spectral centroid.
    fn brightness(x: &[f32]) -> f32 {
        x.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum::<f32>()
            / x.iter().map(|v| v * v).sum::<f32>()
    }
    fn impulse(m: &mut Modes, head: f32, block: f32) -> Vec<f32> {
        (0..4800)
            .map(|i| {
                if i == 0 {
                    m.next(head, block)
                } else {
                    m.next(0., 0.)
                }
            })
            .collect()
    }
    #[test]
    fn modes_silence_decay_seed_and_excitation_differ() {
        let mut m = modes(1);
        for _ in 0..1000 {
            assert_eq!(m.next(0., 0.), 0.);
        }
        let h = impulse(&mut m, 1., 0.);
        assert!(rms(&h[4000..]) < rms(&h[..400]) * 1e-4);
        assert_ne!(h, impulse(&mut modes(2), 1., 0.));
        let b = impulse(&mut modes(1), 0., 1.);
        assert!(brightness(&h) > brightness(&b) * 1.3);
        // Low sample rates mute modes above 0.4 fs instead of stacking them.
        let mut low = Modes::new(8000., 2400., 2., BlockMaterial::CastIron, 86., 1);
        assert!(impulse(&mut low, 1., 1.).iter().all(|v| v.is_finite()));
    }
    #[test]
    fn neutral_level_matches_the_former_single_resonator() {
        let mut r = Radiation::new(48000, 15);
        let mut old = StateVariableFilter::new(48000., 2400., 2., SvfMode::Bandpass);
        let mut new = modes(15);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for i in 0..96000 {
            let impact = if i % 480 == 0 { 0.015 } else { 0. };
            let contact = r.next(0., 0., impact).1;
            a.push(old.next_sample(contact));
            b.push(new.next(contact, 0.));
        }
        let db = 20. * (rms(&b) / rms(&a)).log10();
        // Measured: -0.1 dB, mean-square frequency x1.6 (RMS frequency x1.27).
        // The bound guards against a return of the reported upper-band hiss.
        let bright = brightness(&b) / brightness(&a);
        assert!(db.abs() < 2. && bright < 2., "{db} dB, x{bright}");
    }
}
