//! Shared transport for real-time listening and offline renders.
use crate::{
    bank::Bank,
    drive::{Controls, Mode, Simulator, State, TICK_RATE},
    export::{exhaust_level_gain, exhaust_level_gain_with_floor},
    hybrid::{Hybrid, HybridStems, Settings, audition},
    project::Parameters,
};
use bdsp::svf::{StateVariableFilter, SvfMode};
use std::sync::Arc;

/// Selects what the local listening bench plays. BeamNG applies additional
/// spatial filtering and mixing, so the second option is only a mono preview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AuditionMix {
    #[default]
    Live,
    BeamNgTwoEmitter,
}

/// Allocation-free block maxima over roughly one exported loop duration.
struct PeakWindow<const BLOCKS: usize> {
    blocks: [f32; BLOCKS],
    block_frames: u32,
    frame: u32,
    index: usize,
    max: f32,
}

impl<const BLOCKS: usize> PeakWindow<BLOCKS> {
    fn new(rate: u32) -> Self {
        Self {
            blocks: [0.; BLOCKS],
            // 48 blocks per second, covering the corresponding export WAV.
            block_frames: (rate as f32 / 48.).ceil().max(1.) as u32,
            frame: 0,
            index: 0,
            max: 0.,
        }
    }

    fn next(&mut self, value: f32) -> f32 {
        if self.frame == 0 {
            let old = self.blocks[self.index];
            self.blocks[self.index] = 0.;
            if old >= self.max {
                self.max = self.blocks.iter().copied().fold(0., f32::max);
            }
        }
        let peak = value.abs();
        self.blocks[self.index] = self.blocks[self.index].max(peak);
        self.max = self.max.max(peak);
        self.frame += 1;
        if self.frame >= self.block_frames {
            self.frame = 0;
            self.index = (self.index + 1) % BLOCKS;
        }
        self.max
    }
}

/// Online approximation of the per-knot balance applied by variant export.
/// Unlike export, this follows a changing RPM/load and cannot look ahead over
/// an entire WAV; the slow followers avoid turning individual impacts into gain
/// changes while the user listens.
struct TwoEmitterPreview {
    rate: f32,
    procedural: bool,
    source_power: f32,
    exhaust_power: f32,
    source_cut_power: f32,
    exhaust_cut_power: f32,
    source_cut: StateVariableFilter,
    exhaust_cut: StateVariableFilter,
    exhaust_peak: PeakWindow<96>,
    exhaust_gain: f32,
    engine_power: f32,
    engine_peak: PeakWindow<192>,
    engine_gain: f32,
    blend: f32,
}

impl TwoEmitterPreview {
    const NOMINAL_ENGINE_GAIN: f32 = 0.398_107_17; // -8 dB vs exhaust in the current JBeam.

    fn new(rate: u32, procedural: bool) -> Self {
        Self {
            rate: rate as f32,
            procedural,
            source_power: 0.,
            exhaust_power: 0.,
            source_cut_power: 0.,
            exhaust_cut_power: 0.,
            source_cut: StateVariableFilter::new(rate as f32, 80., 0.707, SvfMode::Highpass),
            exhaust_cut: StateVariableFilter::new(rate as f32, 80., 0.707, SvfMode::Highpass),
            exhaust_peak: PeakWindow::new(rate),
            exhaust_gain: 1.,
            engine_power: 0.,
            engine_peak: PeakWindow::new(rate),
            engine_gain: 1.,
            blend: 0.,
        }
    }

    fn next(
        &mut self,
        stems: HybridStems,
        load: f32,
        inferred_weight: f32,
        export_stem_normalizer: f32,
        exhaust_export_normalizer: f32,
        selected: bool,
    ) -> f32 {
        let energy_step = 1. / (self.rate * 0.35);
        self.source_power +=
            (stems.source_reference * stems.source_reference - self.source_power) * energy_step;
        self.exhaust_power += (stems.exhaust * stems.exhaust - self.exhaust_power) * energy_step;
        let source_cut = self.source_cut.next_sample(stems.source_reference);
        let exhaust_cut = self.exhaust_cut.next_sample(stems.exhaust);
        self.source_cut_power += (source_cut * source_cut - self.source_cut_power) * energy_step;
        self.exhaust_cut_power +=
            (exhaust_cut * exhaust_cut - self.exhaust_cut_power) * energy_step;
        self.engine_power += (stems.engine * stems.engine - self.engine_power) * energy_step;
        let export_exhaust_peak = stems.exhaust.abs() / exhaust_export_normalizer.max(1e-6);
        let exhaust_peak = self.exhaust_peak.next(export_exhaust_peak);
        let source_power = if self.procedural {
            self.source_cut_power
        } else {
            self.source_power
        };
        let exhaust_power = if self.procedural {
            self.exhaust_cut_power
        } else {
            self.exhaust_power
        };
        let exhaust_target = if source_power > 1e-12 && exhaust_power > 1e-12 {
            if self.procedural {
                exhaust_level_gain_with_floor(
                    source_power.sqrt(),
                    exhaust_power.sqrt(),
                    exhaust_peak,
                    0.25,
                )
            } else {
                exhaust_level_gain(source_power.sqrt(), exhaust_power.sqrt(), exhaust_peak)
            }
        } else {
            1.
        };
        self.exhaust_gain += (exhaust_target - self.exhaust_gain) / (self.rate * 0.3);
        // Export removes playback volume, bank normalization and live level
        // compensation before enforcing its PCM ceiling. Track that same peak
        // so turning the listening volume cannot change the preview balance.
        let export_peak = stems.engine.abs() / export_stem_normalizer.max(1e-6);
        let engine_peak = self.engine_peak.next(export_peak);

        let target_ratio = (0.35 + 0.20 * load.clamp(0., 1.)) * inferred_weight;
        let target_gain = if self.engine_power > 1e-12 && self.exhaust_power > 1e-12 {
            (target_ratio * self.exhaust_gain * (self.exhaust_power / self.engine_power).sqrt())
                .min(12.)
                .min(0.94 / engine_peak.max(1e-6))
        } else {
            1.
        };
        self.engine_gain += (target_gain - self.engine_gain) / (self.rate * 0.3);
        self.blend += (f32::from(u8::from(selected)) - self.blend) / (self.rate * 0.06);

        let two_emitters = stems.exhaust * self.exhaust_gain
            + stems.engine * self.engine_gain * Self::NOMINAL_ENGINE_GAIN;
        let two_emitters = if two_emitters.abs() > 0.95 {
            two_emitters.signum() * (0.95 + 0.049 * ((two_emitters.abs() - 0.95) / 0.049).tanh())
        } else {
            two_emitters
        };
        stems.mixed + (two_emitters - stems.mixed) * self.blend
    }
}

pub struct Bench {
    engine: Hybrid,
    sim: Simulator,
    params: Parameters,
    settings: Settings,
    controls: Controls,
    rate: u32,
    max: f32,
    frames: u64,
    physics_accumulator: u32,
    reset_token: u64,
    cycle_seconds: f32,
    audition_mix: AuditionMix,
    two_emitter_preview: TwoEmitterPreview,
}
impl Bench {
    fn effective_settings(&self) -> Settings {
        let mut settings = self.settings;
        if self.audition_mix == AuditionMix::BeamNgTwoEmitter {
            settings = settings.for_beamng_export();
            settings.level_match = false;
        }
        settings
    }

    pub fn new(
        rate: u32,
        mut params: Parameters,
        settings: Settings,
        controls: Controls,
        bank: Option<Arc<Bank>>,
    ) -> Self {
        let rate = rate.max(8000);
        let (min, max) = bank
            .as_ref()
            .map(|b| (b.min_rpm, b.max_rpm))
            .unwrap_or((300., 8000.));
        if controls.mode == Mode::Simulated {
            params.rpm = min;
            params.load = 0.1;
        }
        Self {
            engine: Hybrid::new(rate, params, settings, bank),
            sim: Simulator::new(min, max, controls),
            params,
            settings,
            controls,
            rate,
            max,
            frames: 0,
            physics_accumulator: rate,
            reset_token: 0,
            cycle_seconds: 16.,
            audition_mix: AuditionMix::Live,
            two_emitter_preview: TwoEmitterPreview::new(rate, settings.procedural),
        }
    }
    pub fn set_cycle_seconds(&mut self, seconds: f32) {
        self.cycle_seconds = seconds;
    }
    pub fn set_audition_mix(&mut self, mix: AuditionMix) {
        self.audition_mix = mix;
    }
    pub fn set(&mut self, p: Parameters, h: Settings, c: Controls, reset_token: u64) {
        if p.validate().is_err() || h.validate().is_err() || c.validate().is_err() {
            return;
        }
        if c.mode != self.controls.mode || reset_token != self.reset_token {
            self.sim.reset(c);
            self.frames = 0;
            self.physics_accumulator = self.rate;
        }
        self.params = p;
        self.settings = h;
        self.two_emitter_preview.procedural =
            h.procedural && self.audition_mix != AuditionMix::BeamNgTwoEmitter;
        self.controls = c;
        self.reset_token = reset_token;
        if c.mode == Mode::Direct {
            self.engine.set(p, self.effective_settings());
        }
    }
    pub fn next(&mut self, playing: bool) -> f32 {
        if playing {
            if self.physics_accumulator >= self.rate {
                self.physics_accumulator -= self.rate;
                let p = match self.controls.mode {
                    Mode::Direct => self.params,
                    Mode::Simulated => {
                        let s = self.sim.step(self.controls);
                        Parameters {
                            rpm: s.rpm,
                            load: s.load,
                            ..self.params
                        }
                    }
                    Mode::Cycle => audition(
                        (self.frames as f32 / self.rate as f32 * 16. / self.cycle_seconds) % 16.,
                        self.params,
                        self.max,
                    ),
                };
                self.engine.set(p, self.effective_settings());
            }
            self.physics_accumulator += TICK_RATE;
            self.frames += 1;
        }
        let stems = self.engine.next_stems(playing);
        self.two_emitter_preview.next(
            stems,
            self.engine.load(),
            self.engine.inferred_layer_weight(),
            self.engine.export_stem_normalizer(),
            self.engine.exhaust_export_normalizer(),
            self.audition_mix == AuditionMix::BeamNgTwoEmitter && self.settings.enhanced,
        )
    }
    pub fn state(&self) -> State {
        let mut state = if self.controls.mode == Mode::Simulated {
            self.sim.state()
        } else {
            State::default()
        };
        state.rpm = self.engine.rpm();
        state.load = self.engine.load();
        state
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    #[test]
    fn two_emitter_preview_uses_export_load_targets_and_gain_cap() {
        let run = |load, engine, inferred_weight| {
            let mut preview = TwoEmitterPreview::new(48_000, false);
            let stems = HybridStems {
                source_reference: 0.1,
                exhaust: 0.1,
                engine,
                mixed: 0.1,
            };
            let mut output = 0.;
            for _ in 0..96_000 {
                output = preview.next(stems, load, inferred_weight, 1., 1., true);
            }
            (preview.exhaust_gain, preview.engine_gain, output)
        };
        let (exhaust_gain, off_gain, _) = run(0., 0.02, 1.);
        let (_, on_gain, output) = run(1., 0.02, 1.);
        assert!((exhaust_gain - 0.891_250_9).abs() < 0.01);
        assert!((off_gain - 1.75 * exhaust_gain).abs() < 0.01);
        assert!((on_gain - 2.75 * exhaust_gain).abs() < 0.01);
        assert!((output - (0.1 * exhaust_gain + 0.02 * on_gain * 0.398_107_17)).abs() < 0.001);
        let (_, capped_gain, _) = run(1., 0.0001, 1.);
        assert!((capped_gain - 12.).abs() < 0.03);
        let (_, rejected_gain, _) = run(1., 0.02, 0.15);
        assert!(rejected_gain < on_gain * 0.2);
    }

    #[test]
    fn preview_selection_fades_without_changing_default_live_mix() {
        let mut preview = TwoEmitterPreview::new(48_000, false);
        let stems = HybridStems {
            source_reference: 0.2,
            exhaust: 0.2,
            engine: 0.1,
            mixed: 0.4,
        };
        assert_eq!(preview.next(stems, 1., 1., 1., 1., false), stems.mixed);
        let first = preview.next(stems, 1., 1., 1., 1., true);
        assert!((first - stems.mixed).abs() < 0.001);
        for _ in 0..48_000 {
            preview.next(stems, 1., 1., 1., 1., true);
        }
        assert!((preview.blend - 1.).abs() < 1e-4);
    }

    #[test]
    fn two_emitter_preview_has_a_mono_safety_ceiling() {
        let mut preview = TwoEmitterPreview::new(48_000, false);
        let stems = HybridStems {
            source_reference: 0.95,
            exhaust: 0.95,
            engine: 0.9,
            mixed: 0.8,
        };
        let mut output = 0.;
        for _ in 0..48_000 {
            output = preview.next(stems, 1., 1., 1., 1., true);
        }
        assert!(output.is_finite() && output <= 0.999);
    }

    #[test]
    fn preview_engine_peak_cap_does_not_follow_listening_volume() {
        let run = |volume: f32| {
            let mut preview = TwoEmitterPreview::new(48_000, false);
            for i in 0..96_000 {
                let engine = if i % 10_000 == 0 { 0.5 } else { 0.0001 };
                let stems = HybridStems {
                    source_reference: 0.1 * volume,
                    exhaust: 0.1 * volume,
                    engine: engine * volume,
                    mixed: 0.1 * volume,
                };
                preview.next(stems, 1., 1., volume, volume, true);
            }
            (preview.exhaust_gain, preview.engine_gain)
        };
        let full = run(1.);
        let quiet = run(0.1);
        assert!((full.0 - quiet.0).abs() < 0.001);
        assert!((full.1 - quiet.1).abs() < 0.001);
        assert!(full.1 < 3.);
    }

    #[test]
    fn preview_exhaust_calibration_tracks_original_level_with_a_peak_ceiling() {
        let mut preview = TwoEmitterPreview::new(48_000, false);
        let stems = HybridStems {
            source_reference: 0.2,
            exhaust: 0.1,
            engine: 0.,
            mixed: 0.2,
        };
        for _ in 0..96_000 {
            preview.next(stems, 0., 1., 1., 1., true);
        }
        assert!((preview.exhaust_gain - 1.782_501_8).abs() < 0.01);

        let mut capped = TwoEmitterPreview::new(48_000, false);
        let loud = HybridStems {
            source_reference: 1.2,
            exhaust: 0.8,
            engine: 0.,
            mixed: 1.2,
        };
        for _ in 0..96_000 {
            capped.next(loud, 0., 1., 1., 1., true);
        }
        assert!((capped.exhaust_gain - 1.175).abs() < 0.01);
    }

    #[test]
    fn procedural_preview_uses_the_export_low_cut_basis() {
        let run = |procedural| {
            let mut preview = TwoEmitterPreview::new(48_000, procedural);
            for frame in 0..144_000 {
                let time = frame as f32 / 48_000.;
                let source = 0.1 * (std::f32::consts::TAU * 40. * time).sin();
                let exhaust = 0.1 * (std::f32::consts::TAU * 400. * time).sin();
                preview.next(
                    HybridStems {
                        source_reference: source,
                        exhaust,
                        engine: 0.,
                        mixed: exhaust,
                    },
                    1.,
                    1.,
                    1.,
                    1.,
                    true,
                );
            }
            preview.exhaust_gain
        };
        let original_basis = run(false);
        let low_cut_basis = run(true);
        assert!((original_basis - 0.891_250_9).abs() < 0.02);
        assert!((low_cut_basis - 0.25).abs() < 0.02);
    }
}
