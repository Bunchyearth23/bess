//! Attenuation-only lookahead output control. This stage does not clip samples:
//! its transient headroom comes from a fast gain attack,
//! three milliseconds of anticipation and a conservative compression threshold.
//! BDSP currently caps ratios at 100:1; this is not an infinite-ratio limiter or
//! a mathematical ceiling guarantee for arbitrarily large finite input.
use bdsp::{agc::AgcDetector, delay::DelayLine, dynamics::Compressor};

pub struct OutputLimiter {
    compressor: Compressor,
    delay: DelayLine,
    reduction_db: f32,
}

impl OutputLimiter {
    pub fn new(rate: u32) -> Self {
        let rate = rate.clamp(8000, 384000) as f32;
        let mut compressor = Compressor::new(rate);
        compressor.set_detector(AgcDetector::Peak);
        compressor.set_threshold_db(-3.);
        // This is the maximum supported by the pinned BDSP revision.
        compressor.set_ratio(100.);
        compressor.set_knee_db(1.);
        compressor.set_attack_ms(0.05);
        compressor.set_release_ms(150.);
        compressor.set_makeup_db(0.);
        let mut delay = DelayLine::new(rate, 0.004);
        delay.set_delay_samples(rate * 0.003);
        Self {
            compressor,
            delay,
            reduction_db: 0.,
        }
    }

    /// Original input controls gain; delayed audio receives it. No gain boost,
    /// sample clamp, allocation or reset occurs during ordinary processing.
    pub fn next(&mut self, input: f32) -> f32 {
        let input = if input.is_finite() { input } else { 0. };
        let gain = self.compressor.gain_for(input).min(1.);
        self.reduction_db = self.compressor.gain_reduction_db().min(0.);
        self.delay.process(input) * gain
    }

    pub fn reduction_db(&self) -> f32 {
        self.reduction_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CEILING: f32 = 0.891;

    #[test]
    fn native_peak_stays_below_safety_ceiling_for_tones_impulses_and_steps() {
        for rate in [8000, 44100, 48000, 96000, 192000, 384000] {
            for amplitude in [0.1, 1., 4., 16., 64.] {
                for waveform in 0..3 {
                    let mut limiter = OutputLimiter::new(rate);
                    let mut peak = 0_f32;
                    for i in 0..rate / 5 {
                        let t = i as f32 / rate as f32;
                        let input = match waveform {
                            0 => amplitude * (std::f32::consts::TAU * 70. * t).sin(),
                            1 => {
                                if i == rate / 50 || i == rate / 10 {
                                    amplitude
                                } else {
                                    0.
                                }
                            }
                            _ => {
                                if i > rate / 50 && i < rate / 10 {
                                    amplitude
                                } else {
                                    0.1
                                }
                            }
                        };
                        let output = limiter.next(input);
                        assert!(output.is_finite());
                        assert!(limiter.reduction_db() <= 0.);
                        peak = peak.max(output.abs());
                    }
                    assert!(
                        peak <= CEILING,
                        "{rate} Hz, amp {amplitude}, waveform {waveform}: {peak}"
                    );
                    if rate == 48000 {
                        println!("amplitude {amplitude}, waveform {waveform}: native peak {peak}");
                    }
                }
            }
        }
    }

    #[test]
    fn quiet_signal_is_only_delayed_and_silence_is_not_raised() {
        let mut limiter = OutputLimiter::new(48000);
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.019).sin() * 0.1).collect();
        for i in 0..input.len() {
            let actual = limiter.next(input[i]);
            let expected = if i >= 144 { input[i - 144] } else { 0. };
            assert!((actual - expected).abs() < 1e-6);
            assert_eq!(limiter.reduction_db(), 0.);
        }
        for _ in 0..480 {
            limiter.next(0.);
        }
        for _ in 0..4800 {
            assert_eq!(limiter.next(0.), 0.);
        }
    }

    #[test]
    fn anticipated_gain_has_less_settled_tone_distortion_than_instant_envelope() {
        let rate = 48000_u32;
        let release = (-1. / (rate as f32 * 0.15)).exp();
        for frequency in [28., 70., 440.] {
            let mut old_envelope = 0_f32;
            let mut limiter = OutputLimiter::new(rate);
            let (mut old, mut new) = (Vec::new(), Vec::new());
            for i in 0..rate {
                let input = 4. * (std::f32::consts::TAU * frequency * i as f32 / rate as f32).sin();
                old_envelope = (old_envelope * release).max(input.abs());
                let reference = if old_envelope > CEILING {
                    input * CEILING / old_envelope
                } else {
                    input
                };
                let output = limiter.next(input);
                if i >= rate / 2 {
                    old.push(reference);
                    new.push(output);
                }
            }
            // Fit sine and cosine at the fundamental, then measure all residual
            // energy. Phase/level differences do not masquerade as distortion.
            let residual_ratio = |samples: &[f32]| {
                let (mut sine, mut cosine, mut energy) = (0_f64, 0_f64, 0_f64);
                for (i, &sample) in samples.iter().enumerate() {
                    let phase =
                        std::f64::consts::TAU * f64::from(frequency) * i as f64 / f64::from(rate);
                    sine += f64::from(sample) * phase.sin();
                    cosine += f64::from(sample) * phase.cos();
                    energy += f64::from(sample).powi(2);
                }
                let fundamental = 2. * (sine * sine + cosine * cosine) / samples.len() as f64;
                ((energy - fundamental).max(0.) / energy).sqrt()
            };
            let previous = residual_ratio(&old);
            let anticipated = residual_ratio(&new);
            println!("{frequency} Hz: residual old {previous:.8}, anticipated {anticipated:.8}");
            assert!(
                anticipated < previous,
                "{frequency}: {anticipated} vs {previous}"
            );
        }
    }
}
