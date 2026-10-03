//! Event-conditioned mechanical excitations. These are perceptual source models,
//! not calibrated measurements of block acceleration or contact force. They only
//! observe the solver: their random streams must never be shared with the gas,
//! combustion-cycle or intake observers.
//!
//! Each packet has a finite attack and a short tail. The texture has a broad,
//! non-resonant spectrum; no oscillator or noise source runs without excitation.

const QUIET: f32 = 1.0e-12;
const PRESSURE_RATE_GAIN: f64 = 2.0e-12;
const CONTACT_DOMAIN: u64 = 0x8794_17f1_9c32_681b;
const COMBUSTION_DOMAIN: u64 = 0xd173_bac5_9086_2ef3;

fn seed_for(seed: u64, stream: u64) -> u64 {
    let mut z = seed.wrapping_add(stream.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)).max(1)
}

fn random_signed(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 * (1.0 / 8_388_608.0) - 1.0
}

fn decay(rate: f32, seconds: f32) -> f32 {
    (-1.0 / (rate * seconds)).exp()
}

/// A one-pole low-pass followed by DC/low-frequency removal. Neither filter
/// resonates. Fixed-Hz filters and the square-root rate factor keep the texture
/// power approximately independent of the synthesis sample rate (48/96 kHz).
#[derive(Clone)]
struct Texture {
    initial_seed: u64,
    seed: u64,
    low: f32,
    dc: f32,
    low_step: f32,
    dc_step: f32,
    rate_gain: f32,
}

impl Texture {
    fn new(rate: f32, seed: u64, low_hz: f32, high_hz: f32) -> Self {
        let step = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz.min(rate * 0.4) / rate).exp();
        Self {
            initial_seed: seed,
            seed,
            low: 0.0,
            dc: 0.0,
            low_step: step(high_hz),
            dc_step: step(low_hz),
            rate_gain: (rate / 48_000.0).sqrt(),
        }
    }

    fn next(&mut self) -> f32 {
        let white = random_signed(&mut self.seed) * self.rate_gain;
        self.low += self.low_step * (white - self.low);
        self.dc += self.dc_step * (self.low - self.dc);
        self.low - self.dc
    }

    fn clear_tail(&mut self) {
        self.low = 0.0;
        self.dc = 0.0;
    }

    fn reset(&mut self) {
        self.seed = self.initial_seed;
        self.clear_tail();
    }
}

/// Damped contact packets driven by positive impact impulses. A new impact adds
/// to the current tail; it does not reset the envelope or abruptly replace it.
/// The input is an impulse magnitude, not a force sampled for a whole dwell.
#[derive(Clone)]
pub(crate) struct Contacts {
    tail: f32,
    envelope: f32,
    decay: f32,
    attack_step: f32,
    initial_event_seed: u64,
    event_seed: u64,
    texture: Texture,
}

impl Contacts {
    pub fn new(rate: u32, seed: u64) -> Self {
        let rate = rate.max(1) as f32;
        let event_seed = seed_for(seed ^ CONTACT_DOMAIN, 1);
        Self {
            tail: 0.0,
            envelope: 0.0,
            decay: decay(rate, 0.0008),
            attack_step: 1.0 - decay(rate, 0.00015),
            initial_event_seed: event_seed,
            event_seed,
            texture: Texture::new(rate, seed_for(seed ^ CONTACT_DOMAIN, 2), 250.0, 6000.0),
        }
    }

    pub fn next(&mut self, impact: f32) -> f32 {
        if impact.is_finite() && impact > 0.0 {
            // Event scatter is separate from the per-sample texture RNG, so it
            // depends on the impact sequence rather than the synthesis rate.
            let variation = 1.0 + 0.1 * random_signed(&mut self.event_seed);
            self.tail += impact * variation;
        }
        if self.tail < QUIET && self.envelope < QUIET {
            self.tail = 0.0;
            self.envelope = 0.0;
            self.texture.clear_tail();
            return 0.0;
        }
        self.envelope += self.attack_step * (self.tail - self.envelope);
        self.tail *= self.decay;
        // A small coherent displacement remains. The higher texture coefficient
        // compensates approximately for the shorter, bandwidth-limited packet
        // relative to the old 0.14 x white-noise burst; this is a fixed starting
        // calibration, not level normalization or an automatic gain control.
        self.envelope * (0.025 + 0.28 * self.texture.next())
    }

    /// Reproduce the initial state without reallocating or changing the tuning.
    #[allow(dead_code)] // Also useful to callers that explicitly restart a scene.
    pub fn reset(&mut self) {
        self.tail = 0.0;
        self.envelope = 0.0;
        self.event_seed = self.initial_event_seed;
        self.texture.reset();
    }
}

#[derive(Clone)]
struct Cylinder {
    envelope: f32,
    coherent: f32,
    texture: Texture,
}

/// Combustion-only observation. Inputs are nonnegative thermal pressure rates
/// in Pa/s, one per cylinder, after summing heat increments over physics substeps
/// and dividing by the output-frame duration. Do not pass total dP/dt: its
/// compression and pumping components are deliberately absent from this source.
#[derive(Clone)]
pub(crate) struct Combustion {
    cylinders: [Cylinder; 12],
    attack_step: f32,
    release_step: f32,
}

impl Combustion {
    pub fn new(rate: u32, seed: u64) -> Self {
        let rate = rate.max(1) as f32;
        Self {
            cylinders: std::array::from_fn(|i| Cylinder {
                envelope: 0.0,
                coherent: 0.0,
                texture: Texture::new(
                    rate,
                    seed_for(seed ^ COMBUSTION_DOMAIN, i as u64 + 1),
                    150.0,
                    4000.0,
                ),
            }),
            attack_step: 1.0 - decay(rate, 0.00015),
            release_step: 1.0 - decay(rate, 0.0007),
        }
    }

    pub fn next(&mut self, heat_pressure_rate: [f64; 12]) -> f32 {
        let mut out = 0.0;
        for (cylinder, heat_rate) in self.cylinders.iter_mut().zip(heat_pressure_rate) {
            let scaled = (heat_rate * PRESSURE_RATE_GAIN) as f32;
            // Invalid/negative observer input cannot create an excitation. This
            // is not a validity check or correction of the underlying gas state.
            let drive = if scaled.is_finite() && scaled > 0.0 {
                scaled
            } else {
                0.0
            };
            if drive == 0.0 && cylinder.envelope < QUIET && cylinder.coherent < QUIET {
                cylinder.envelope = 0.0;
                cylinder.coherent = 0.0;
                cylinder.texture.clear_tail();
                continue;
            }
            let step = if drive > cylinder.envelope {
                self.attack_step
            } else {
                self.release_step
            };
            cylinder.envelope += step * (drive - cylinder.envelope);
            cylinder.coherent += self.attack_step * (drive - cylinder.coherent);
            // Existing packets may ring briefly after heat stops, but no new
            // packet, autonomous noise floor or oscillator is excited then.
            out += 0.1 * cylinder.coherent + 0.7 * cylinder.envelope * cylinder.texture.next();
        }
        out
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        for cylinder in &mut self.cylinders {
            cylinder.envelope = 0.0;
            cylinder.coherent = 0.0;
            cylinder.texture.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contacts_have_a_finite_attack_and_no_autonomous_tail() {
        for rate in [48_000, 96_000] {
            let mut contacts = Contacts::new(rate, 0);
            for _ in 0..100 {
                assert_eq!(contacts.next(0.0), 0.0);
            }
            let first = contacts.next(1.0);
            let first_envelope = contacts.envelope;
            let mut peak_envelope = first_envelope;
            let mut energy = f64::from(first).powi(2);
            let mut peak_frame = 0;
            for frame in 1..rate / 100 {
                let sample = contacts.next(0.0);
                assert!(sample.is_finite());
                energy += f64::from(sample).powi(2);
                if contacts.envelope > peak_envelope {
                    peak_envelope = contacts.envelope;
                    peak_frame = frame;
                }
            }
            assert!(peak_envelope > first_envelope * 3.0);
            assert!(peak_frame > 0 && peak_frame < rate / 1000);
            assert!(energy > 0.0);
            for _ in 0..rate / 10 {
                contacts.next(0.0);
            }
            assert_eq!(contacts.next(0.0), 0.0);
            assert_eq!(contacts.envelope, 0.0);
        }
    }

    #[test]
    fn contact_packets_are_reproducible_and_seeded() {
        let mut a = Contacts::new(48_000, 43);
        let mut same = Contacts::new(48_000, 43);
        let mut other = Contacts::new(48_000, 44);
        let mut first = [0.0; 1024];
        let mut difference = 0.0;
        for (i, sample) in first.iter_mut().enumerate() {
            let impact = if i % 300 == 0 { 0.015 } else { 0.0 };
            *sample = a.next(impact);
            assert_eq!(*sample, same.next(impact));
            difference += (*sample - other.next(impact)).abs();
        }
        assert!(difference > 0.0);
        a.reset();
        for (i, expected) in first.iter().enumerate() {
            let impact = if i % 300 == 0 { 0.015 } else { 0.0 };
            assert_eq!(a.next(impact), *expected);
        }
    }

    #[test]
    fn combustion_requires_heat_and_decays_after_cut() {
        for rate in [48_000, 96_000] {
            let mut combustion = Combustion::new(rate, 5);
            for _ in 0..rate / 100 {
                assert_eq!(combustion.next([0.0; 12]), 0.0);
            }
            let mut heat = [0.0; 12];
            heat[3] = 8.0e9;
            let mut energy = 0.0;
            for _ in 0..rate / 200 {
                energy += f64::from(combustion.next(heat)).powi(2);
            }
            assert!(energy > 0.0);
            // A heat cut allows the already excited packet to finish.
            assert!(combustion.next([0.0; 12]).is_finite());
            let mut early = 0.0;
            let mut late = 0.0;
            for i in 0..rate / 50 {
                let sample = combustion.next([0.0; 12]);
                assert!(sample.is_finite());
                if i < rate / 1000 {
                    early += f64::from(sample).powi(2);
                } else if i >= rate / 100 {
                    late += f64::from(sample).powi(2);
                }
            }
            assert!(early > 0.0 && late < early * 1.0e-8);
            for _ in 0..rate / 10 {
                combustion.next([0.0; 12]);
            }
            assert_eq!(combustion.next([0.0; 12]), 0.0);
            assert!(combustion.cylinders.iter().all(|c| c.envelope == 0.0));
        }
    }

    #[test]
    fn combustion_reset_and_cylinder_streams_are_independent() {
        let mut a = Combustion::new(48_000, 23);
        let mut same = Combustion::new(48_000, 23);
        let mut other = Combustion::new(48_000, 24);
        let mut first = [0.0; 1024];
        let mut difference = 0.0;
        for (i, sample) in first.iter_mut().enumerate() {
            let heat = if i < 400 { [5.0e9; 12] } else { [0.0; 12] };
            *sample = a.next(heat);
            assert_eq!(*sample, same.next(heat));
            difference += (*sample - other.next(heat)).abs();
        }
        assert!(difference > 0.0);
        a.reset();
        for (i, expected) in first.iter().enumerate() {
            let heat = if i < 400 { [5.0e9; 12] } else { [0.0; 12] };
            assert_eq!(a.next(heat), *expected);
        }
        let mut first_cylinder = Combustion::new(48_000, 23);
        let mut second_cylinder = first_cylinder.clone();
        let mut only_first = [0.0; 12];
        only_first[0] = 5.0e9;
        let mut only_second = [0.0; 12];
        only_second[1] = 5.0e9;
        assert_ne!(
            first_cylinder.next(only_first),
            second_cylinder.next(only_second)
        );
    }

    fn variance(sum: f64, squares: f64, frames: u32) -> f64 {
        squares / f64::from(frames) - (sum / f64::from(frames)).powi(2)
    }

    fn contact_variance(rate: u32) -> f64 {
        let mut contacts = Contacts::new(rate, 9);
        let frames = rate * 3;
        let (mut sum, mut squares) = (0.0, 0.0);
        for frame in 0..frames {
            let impact = if frame % (rate / 100) == 0 {
                0.015
            } else {
                0.0
            };
            let sample = f64::from(contacts.next(impact));
            sum += sample;
            squares += sample * sample;
        }
        variance(sum, squares, frames)
    }

    fn combustion_variance(rate: u32) -> f64 {
        let mut combustion = Combustion::new(rate, 9);
        let frames = rate * 3;
        let (mut sum, mut squares) = (0.0, 0.0);
        for frame in 0..frames {
            let mut heat = [0.0; 12];
            for (cylinder, value) in heat.iter_mut().enumerate().take(4) {
                let phase = (frame + cylinder as u32 * rate / 100) % (rate / 25);
                if phase < rate / 200 {
                    *value = 8.0e9;
                }
            }
            let sample = f64::from(combustion.next(heat));
            sum += sample;
            squares += sample * sample;
        }
        variance(sum, squares, frames)
    }

    #[test]
    fn packet_power_is_consistent_at_48_and_96_khz() {
        let contacts = contact_variance(96_000) / contact_variance(48_000);
        let combustion = combustion_variance(96_000) / combustion_variance(48_000);
        // Stochastic finite records need not match sample-for-sample, but doubling
        // the synthesis rate must not double or halve the observed source power.
        assert!(
            (0.75..1.25).contains(&contacts),
            "contact variance ratio {contacts}"
        );
        assert!(
            (0.75..1.25).contains(&combustion),
            "combustion variance ratio {combustion}"
        );
    }
}
