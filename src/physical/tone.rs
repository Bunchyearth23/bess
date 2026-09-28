//! Explicit sound shaping of physical stems, never feedback into the gas model.
//! Neutral settings bypass every operation, preserving samples bit for bit.
use crate::scratch::SoundTuning;
use bdsp::{
    core::InplaceProcessor,
    filters::BiquadFilter,
    svf::{StateVariableFilter, SvfMode},
};
use std::f32::consts::TAU;

struct Channel {
    eq: [Option<BiquadFilter>; 3],
    brightness: Option<StateVariableFilter>,
    high_split_low: f32,
}

impl Channel {
    fn new(rate: f32, tuning: &SoundTuning) -> Self {
        let frequency = |hz: f32| hz.min(rate * 0.35);
        Self {
            eq: [
                (tuning.bass_db != 0.).then(|| {
                    BiquadFilter::new_low_shelf(rate, frequency(120.), 0.707, tuning.bass_db)
                }),
                (tuning.presence_db != 0.).then(|| {
                    BiquadFilter::new_peaking(rate, frequency(1200.), 0.8, tuning.presence_db)
                }),
                (tuning.treble_db != 0.).then(|| {
                    BiquadFilter::new_high_shelf(rate, frequency(4000.), 0.707, tuning.treble_db)
                }),
            ],
            brightness: (tuning.brightness_hz < 20_000.).then(|| {
                StateVariableFilter::new(
                    rate,
                    tuning.brightness_hz.clamp(500., rate * 0.45),
                    0.707,
                    SvfMode::Lowpass,
                )
            }),
            high_split_low: 0.,
        }
    }

    fn process(&mut self, input: f32, dynamic: bool, split: f32, gain: f32) -> f32 {
        let mut sample = [input];
        for filter in self.eq.iter_mut().flatten() {
            filter.process_inplace(&mut sample);
        }
        if dynamic {
            self.high_split_low += split * (sample[0] - self.high_split_low);
            // First-order complementary split: low + gain * high. Fixed pole,
            // smoothed gain, no moving resonator or coefficient discontinuity.
            sample[0] += (gain - 1.) * (sample[0] - self.high_split_low);
        }
        if let Some(filter) = &mut self.brightness {
            sample[0] = filter.next_sample(sample[0]);
        }
        sample[0]
    }
}

pub struct Tone {
    neutral: bool,
    exhaust: Channel,
    intake: Channel,
    intake_resonator: Option<BiquadFilter>,
    exhaust_eq: [Option<BiquadFilter>; 3],
    exhaust_low_cut: Option<StateVariableFilter>,
    exhaust_high_cut: Option<StateVariableFilter>,
    exhaust_drive: f32,
    drive: f32,
    texture: f32,
    texture_filter: StateVariableFilter,
    seed: u64,
    dynamic: bool,
    rpm_db: f32,
    load_db: f32,
    target_gain: f32,
    gain: f32,
    split: f32,
    slew: f32,
    tick: u32,
    control_period: u32,
}

impl Tone {
    /// Internal synthesis rates 8–384 kHz. Tuning is fixed for this instance;
    /// callers rebuild off-thread and crossfade when the user edits controls.
    pub fn new(rate: u32, tuning: &SoundTuning) -> Self {
        let rate = rate.clamp(8000, 384000) as f32;
        let dynamic = tuning.rpm_brightness_db != 0. || tuning.load_brightness_db != 0.;
        let neutral = tuning.bass_db == 0.
            && tuning.presence_db == 0.
            && tuning.treble_db == 0.
            && tuning.brightness_hz >= 20_000.
            && tuning.drive == 0.
            && tuning.flow_texture == 0.
            && tuning.intake_resonance == 0.
            && !dynamic
            && tuning.exhaust_bass_db == 0.
            && tuning.exhaust_body_db == 0.
            && tuning.exhaust_rasp_db == 0.
            && tuning.exhaust_low_cut_hz <= 20.
            && tuning.exhaust_high_cut_hz >= 20000.
            && tuning.exhaust_drive == 0.;
        Self {
            neutral,
            exhaust: Channel::new(rate, tuning),
            exhaust_eq: [
                (tuning.exhaust_bass_db != 0.).then(|| {
                    BiquadFilter::new_low_shelf(rate, 120., 0.707, tuning.exhaust_bass_db)
                }),
                (tuning.exhaust_body_db != 0.).then(|| {
                    BiquadFilter::new_peaking(
                        rate,
                        tuning.exhaust_body_hz.min(rate * 0.35),
                        tuning.exhaust_body_q,
                        tuning.exhaust_body_db,
                    )
                }),
                (tuning.exhaust_rasp_db != 0.).then(|| {
                    BiquadFilter::new_high_shelf(
                        rate,
                        3000_f32.min(rate * 0.35),
                        0.707,
                        tuning.exhaust_rasp_db,
                    )
                }),
            ],
            exhaust_low_cut: (tuning.exhaust_low_cut_hz > 20.).then(|| {
                StateVariableFilter::new(rate, tuning.exhaust_low_cut_hz, 0.707, SvfMode::Highpass)
            }),
            exhaust_high_cut: (tuning.exhaust_high_cut_hz < 20000.).then(|| {
                StateVariableFilter::new(
                    rate,
                    tuning.exhaust_high_cut_hz.min(rate * 0.45),
                    0.707,
                    SvfMode::Lowpass,
                )
            }),
            exhaust_drive: tuning.exhaust_drive,
            intake: Channel::new(rate, tuning),
            // A quarter-wave observation filter, not a second physical intake.
            // Resonance 0–3 maps to 0–24 dB and Q 0.7–4.9. Length only changes
            // the frequency when this explicit resonance is enabled.
            intake_resonator: (tuning.intake_resonance > 0.).then(|| {
                let amount = tuning.intake_resonance.clamp(0., 3.);
                BiquadFilter::new_peaking(
                    rate,
                    343. / (4. * tuning.intake_length_m.clamp(0.15, 1.5)),
                    0.7 + 1.4 * amount,
                    8. * amount,
                )
            }),
            drive: tuning.drive.clamp(0., 1.),
            texture: tuning.flow_texture.clamp(0., 1.),
            texture_filter: StateVariableFilter::new(
                rate,
                3000_f32.min(rate * 0.3),
                0.7,
                SvfMode::Bandpass,
            ),
            seed: 0x72e4_c196_9b35_a8df,
            dynamic,
            rpm_db: tuning.rpm_brightness_db.clamp(-12., 12.),
            load_db: tuning.load_brightness_db.clamp(-12., 12.),
            target_gain: 1.,
            gain: 1.,
            split: 1. - (-TAU * 1500. / rate).exp(),
            slew: 1. - (-1. / (rate * 0.03)).exp(),
            tick: 0,
            control_period: (rate as u32 / 1000).max(1),
        }
    }

    /// No allocation, level tracking, automatic makeup or physical state changes.
    /// RPM must be normalized to the engine's redline; throttle is 0–1.
    pub fn process(
        &mut self,
        exhaust: f32,
        intake: f32,
        rpm_normalized: f32,
        throttle: f32,
    ) -> (f32, f32) {
        if self.neutral {
            return (exhaust, intake);
        }
        if self.dynamic {
            if self.tick == 0 {
                let rpm = unit(rpm_normalized);
                let load = unit(throttle);
                self.target_gain = 10_f32.powf((self.rpm_db * rpm + self.load_db * load) / 20.);
            }
            self.tick = (self.tick + 1) % self.control_period;
            self.gain += self.slew * (self.target_gain - self.gain);
        }
        let mut exhaust = self
            .exhaust
            .process(exhaust, self.dynamic, self.split, self.gain);
        let mut intake = self
            .intake
            .process(intake, self.dynamic, self.split, self.gain);
        if let Some(filter) = &mut self.intake_resonator {
            let mut sample = [intake];
            filter.process_inplace(&mut sample);
            intake = sample[0];
        }
        if self.texture > 0. {
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            let white = (self.seed >> 40) as f32 / 8_388_608. - 1.;
            let noise = self.texture_filter.next_sample(white);
            // Multiplicative texture: silent input cannot create hiss/pop
            // excitation; its amplitude follows the physical signal and tails.
            let modulation = 1. + 0.35 * self.texture * noise;
            exhaust *= modulation;
            intake *= modulation;
        }
        if self.drive > 0. {
            let k = 1. + 15. * self.drive;
            // Unit small-signal slope; saturation only removes peak magnitude.
            // No tanh(k) normalization or automatic loudness compensation.
            exhaust = (exhaust * k).tanh() / k;
            intake = (intake * k).tanh() / k;
        }
        // Shape only the physical exhaust outlet. No parallel excitation or
        // gain normalization; intake/mechanics and gas state stay untouched.
        let mut outlet = [exhaust];
        for filter in self.exhaust_eq.iter_mut().flatten() {
            filter.process_inplace(&mut outlet);
        }
        exhaust = outlet[0];
        if let Some(filter) = &mut self.exhaust_low_cut {
            exhaust = filter.next_sample(exhaust);
        }
        if let Some(filter) = &mut self.exhaust_high_cut {
            exhaust = filter.next_sample(exhaust);
        }
        if self.exhaust_drive > 0. {
            let k = 1. + 15. * self.exhaust_drive;
            exhaust = (exhaust * k).tanh() / k;
        }
        (exhaust, intake)
    }
}

fn unit(x: f32) -> f32 {
    if x.is_finite() { x.clamp(0., 1.) } else { 0. }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(tuning: SoundTuning, hz: f32, rpm: f32, load: f32, intake: bool) -> f64 {
        let mut tone = Tone::new(48_000, &tuning);
        let mut power = 0.;
        for frame in 0..24_000 {
            let x = 0.05 * (TAU * hz * frame as f32 / 48_000.).sin();
            let y = tone.process(x, x, rpm, load);
            if frame >= 12_000 {
                power += f64::from(if intake { y.1 } else { y.0 }).powi(2);
            }
        }
        (power / 12_000.).sqrt()
    }

    #[test]
    fn exhaust_bands_filters_and_drive_have_measured_effects_only_on_the_outlet() {
        let flat = SoundTuning::default();
        for (band, hz) in [(0, 25.), (1, 250.), (2, 15000.)] {
            for db in [-6., 6.] {
                let mut tuned = flat;
                match band {
                    0 => tuned.exhaust_bass_db = db,
                    1 => tuned.exhaust_body_db = db,
                    _ => tuned.exhaust_rasp_db = db,
                }
                let measured = 20.
                    * (response(tuned, hz, 0., 0., false) / response(flat, hz, 0., 0., false))
                        .log10();
                assert!(
                    (measured - f64::from(db)).abs() < 0.5,
                    "band {band}: {measured}"
                );
                assert_eq!(
                    response(tuned, hz, 0., 0., true),
                    response(flat, hz, 0., 0., true)
                );
            }
        }
        let low = SoundTuning {
            exhaust_low_cut_hz: 200.,
            ..flat
        };
        let high = SoundTuning {
            exhaust_high_cut_hz: 800.,
            ..flat
        };
        assert!(response(low, 25., 0., 0., false) < response(flat, 25., 0., 0., false) * 0.03);
        assert!(response(high, 6000., 0., 0., false) < response(flat, 6000., 0., 0., false) * 0.03);
        let saturated = SoundTuning {
            exhaust_drive: 1.,
            ..flat
        };
        assert!(
            response(saturated, 500., 0., 0., false) < response(flat, 500., 0., 0., false) * 0.9
        );
    }

    #[test]
    fn extreme_exhaust_tone_is_finite_and_cannot_emit_without_input() {
        for rate in [8000, 44100, 96000, 384000] {
            let tuning = SoundTuning {
                exhaust_bass_db: 12.,
                exhaust_body_db: 12.,
                exhaust_body_hz: 2000.,
                exhaust_body_q: 8.,
                exhaust_rasp_db: 12.,
                exhaust_low_cut_hz: 300.,
                exhaust_high_cut_hz: 500.,
                exhaust_drive: 1.,
                ..Default::default()
            };
            let mut tone = Tone::new(rate, &tuning);
            for _ in 0..500 {
                assert_eq!(tone.process(0., 0., 1., 1.), (0., 0.));
            }
            for i in 0..24000 {
                let input = (i as f32 * 0.25).sin() * 4.;
                let (out, intake) = tone.process(input, input, 1., 1.);
                assert!(out.is_finite() && out.abs() <= 1. / 16.);
                assert_eq!(intake.to_bits(), input.to_bits());
            }
        }
    }

    #[test]
    fn neutral_preserves_every_bit_at_all_supported_rate_extremes() {
        for rate in [8000, 44100, 96000, 384000] {
            let mut tone = Tone::new(rate, &SoundTuning::default());
            for x in [0., -0., 1e-25, -1e-25, 0.002, -0.92, 3.] {
                let (a, b) = tone.process(x, -x, 0.8, 0.6);
                assert_eq!(a.to_bits(), x.to_bits());
                assert_eq!(b.to_bits(), (-x).to_bits());
            }
        }
    }

    #[test]
    fn each_eq_band_changes_its_target_region_in_both_directions() {
        for (band, hz) in [(0, 30.), (1, 1200.), (2, 14000.)] {
            for db in [-6., 6.] {
                let mut tuning = SoundTuning::default();
                match band {
                    0 => tuning.bass_db = db,
                    1 => tuning.presence_db = db,
                    _ => tuning.treble_db = db,
                }
                let measured = 20.
                    * (response(tuning, hz, 0., 0., false)
                        / response(SoundTuning::default(), hz, 0., 0., false))
                    .log10();
                assert!((measured - f64::from(db)).abs() < 0.5, "{band}: {measured}");
            }
        }
    }

    #[test]
    fn brightness_cutoff_removes_high_frequency_energy() {
        let dark = response(
            SoundTuning {
                brightness_hz: 500.,
                ..Default::default()
            },
            5000.,
            0.,
            0.,
            false,
        );
        let neutral = response(SoundTuning::default(), 5000., 0., 0., false);
        assert!(dark < neutral * 0.03);
    }

    #[test]
    fn drive_rounds_peaks_without_any_makeup_gain() {
        let mut tone = Tone::new(
            48_000,
            &SoundTuning {
                drive: 1.,
                ..Default::default()
            },
        );
        for i in -1000..=1000 {
            let x = i as f32 / 1000.;
            let y = tone.process(x, x, 0., 0.).0;
            assert!(y.abs() <= x.abs() + 1e-7);
            assert!(y.abs() <= 1. / 16.);
        }
        assert!(tone.process(0.5, 0., 0., 0.).0 < 0.1);
    }

    #[test]
    fn physical_texture_is_reproducible_and_cannot_create_sound_from_silence() {
        let tuning = SoundTuning {
            flow_texture: 1.,
            ..Default::default()
        };
        let mut a = Tone::new(48_000, &tuning);
        let mut b = Tone::new(48_000, &tuning);
        let mut difference = 0.;
        for i in 0..10_000 {
            let x = if i < 5000 { 0.02 } else { 0. };
            let y = a.process(x, x, 0., 0.);
            assert_eq!(y, b.process(x, x, 0., 0.));
            difference += (y.0 - x).abs();
            if x == 0. {
                assert_eq!(y, (0., 0.));
            }
        }
        assert!(difference > 0.1);
    }

    #[test]
    fn intake_length_moves_the_resonance_and_does_not_color_exhaust() {
        let short = SoundTuning {
            intake_length_m: 0.2,
            intake_resonance: 2.,
            ..Default::default()
        };
        let long = SoundTuning {
            intake_length_m: 0.8,
            intake_resonance: 2.,
            ..Default::default()
        };
        let hz = 343. / (4. * 0.2);
        assert!(response(short, hz, 0., 0., true) > response(long, hz, 0., 0., true) * 4.);
        assert_eq!(
            response(short, hz, 0., 0., false),
            response(SoundTuning::default(), hz, 0., 0., false)
        );
    }

    #[test]
    fn rpm_and_load_independently_control_smoothed_brightness() {
        for rpm_control in [false, true] {
            let tuning = SoundTuning {
                rpm_brightness_db: if rpm_control { 9. } else { 0. },
                load_brightness_db: if rpm_control { 0. } else { 9. },
                ..Default::default()
            };
            let off = response(tuning, 8000., 0., 0., false);
            let on = response(
                tuning,
                8000.,
                if rpm_control { 1. } else { 0. },
                if rpm_control { 0. } else { 1. },
                false,
            );
            assert!(on > off * 2.2);
            let mut tone = Tone::new(48_000, &tuning);
            let first = tone.process(0.1, 0.1, 1., 1.).0;
            assert!((first - 0.1).abs() < 0.001, "control jump was not smoothed");
        }
    }

    #[test]
    fn extreme_tuning_remains_finite_at_minimum_and_maximum_rates() {
        for rate in [8000, 96000, 384000] {
            for sign in [-1., 1.] {
                let tuning = SoundTuning {
                    bass_db: 12. * sign,
                    presence_db: 12. * sign,
                    treble_db: 12. * sign,
                    brightness_hz: 500.,
                    drive: 1.,
                    flow_texture: 1.,
                    intake_length_m: 1.5,
                    intake_resonance: 3.,
                    rpm_brightness_db: 12. * sign,
                    load_brightness_db: 12. * sign,
                    ..Default::default()
                };
                let mut tone = Tone::new(rate, &tuning);
                for i in 0..rate / 4 {
                    let x = (TAU * 83. * i as f32 / rate as f32).sin();
                    let (a, b) = tone.process(x, x, 1., 1.);
                    assert!(a.is_finite() && b.is_finite());
                    assert!(a.abs() <= 1. / 16. && b.abs() <= 1. / 16.);
                }
            }
        }
    }
}
