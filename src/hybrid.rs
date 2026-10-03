//! Original Automation reference A and physical engine resynthesis B.
use crate::{bank::Bank, project::Parameters};
use bdsp::resample::{SincQuality, SincTable};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub enhanced: bool,
    /// Physical engine used for imported Automation resynthesis.
    #[serde(skip_serializing)]
    pub physical: bool,
    /// Complete, editable physical engine. None migrates the source metadata.
    pub engine: Option<crate::engine_definition::EngineDefinition>,
    /// Immutable imported reference, retained even when the mapper evolves.
    pub engine_baseline: Option<crate::engine_definition::EngineDefinition>,
    pub physical_sound: crate::scratch::SoundTuning,
    // Legacy project migration fields below are never used for synthesis.
    #[serde(skip_serializing)]
    pub procedural: bool,
    #[serde(skip_serializing)]
    pub generated_body: f32,
    #[serde(skip_serializing)]
    pub generated_edge: f32,
    #[serde(skip_serializing)]
    pub generated_flow: f32,
    #[serde(skip_serializing)]
    pub generated_mechanics: f32,
    pub level_match: bool,
    pub response: f32,
    #[serde(skip_serializing)]
    pub attack: f32,
    #[serde(skip_serializing)]
    pub body: f32,
    #[serde(skip_serializing)]
    pub rasp: f32,
    #[serde(skip_serializing)]
    pub texture: f32,
    #[serde(skip_serializing)]
    pub pipe: f32,
    #[serde(skip_serializing)]
    pub overrun: f32,
    #[serde(skip_serializing)]
    pub turbo: f32,
    #[serde(skip_serializing)]
    pub roughness: f32,
    #[serde(skip_serializing)]
    pub header_length: f32,
    #[serde(skip_serializing)]
    pub diameter: f32,
    #[serde(skip_serializing)]
    pub chamber: f32,
    #[serde(skip_serializing)]
    pub absorption: f32,
    #[serde(skip_serializing)]
    pub temperature: f32,
    #[serde(skip_serializing)]
    pub intake_length: f32,
    #[serde(skip_serializing)]
    pub airbox: f32,
    pub fuel_cut: f32,
    #[serde(skip_serializing)]
    pub pulse_gain: f32,
    #[serde(skip_serializing)]
    pub residual_gain: f32,
    #[serde(skip_serializing)]
    pub cycle_life: f32,
    #[serde(skip_serializing)]
    pub pulse_texture: f32,
    #[serde(skip_serializing)]
    pub pressure_shape: f32,
    #[serde(skip_serializing)]
    pub maps: crate::maps::Maps,
    #[serde(skip_serializing)]
    pub rpm_character: f32,
    #[serde(skip_serializing)]
    pub load_character: f32,
    #[serde(skip_serializing)]
    pub coloration: f32,
    #[serde(skip_serializing)]
    pub source_timbre: f32,
    /// Gain multiplier at idle RPM, fading to unity at higher RPM.
    pub idle_gain: f32,
    /// Independent intake/mechanical layer gain for physical scratch engines.
    pub engine_gain: f32,
    /// Physical scratch accessory loads, independent of acoustic controls.
    pub accessory_ac: bool,
    pub accessory_steering: bool,
    /// Momentary physical starter command; never restored from a saved project.
    #[serde(skip)]
    pub starter: bool,
    #[serde(skip_serializing)]
    pub combustion: crate::combustion::Combustion,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enhanced: true,
            physical: true,
            engine: None,
            engine_baseline: None,
            physical_sound: Default::default(),
            procedural: false,
            generated_body: 1.,
            generated_edge: 1.,
            generated_flow: 1.,
            generated_mechanics: 1.,
            idle_gain: 1.,
            engine_gain: 1.,
            accessory_ac: false,
            accessory_steering: false,
            starter: false,
            level_match: true,
            response: 0.14,
            attack: 0.4,
            body: 0.3,
            rasp: 0.25,
            texture: 0.22,
            pipe: 0.28,
            overrun: 0.12,
            turbo: 0.,
            roughness: 0.1,
            header_length: 0.55,
            diameter: 65.,
            chamber: 4.,
            absorption: 0.4,
            temperature: 550.,
            intake_length: 0.38,
            airbox: 0.35,
            fuel_cut: 0.4,
            pulse_gain: 1.,
            residual_gain: 1.,
            cycle_life: 0.45,
            pulse_texture: 0.45,
            pressure_shape: 0.5,
            maps: Default::default(),
            rpm_character: 0.,
            load_character: 0.,
            coloration: 1.,
            source_timbre: 0.45,
            combustion: Default::default(),
        }
    }
}
impl Settings {
    /// Export uses the same physical voice and sound controls as audition.
    pub fn for_beamng_export(self) -> Self {
        self
    }
    pub fn calibrated(_bank: &Bank) -> Self {
        Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(engine) = &self.engine {
            engine.validate()?;
        }
        if let Some(baseline) = &self.engine_baseline {
            baseline.validate()?;
        }
        self.physical_sound.validate()?;
        for (name, value, max) in [
            ("Response", self.response, 1.),
            ("Fuel cut", self.fuel_cut, 1.),
            ("Idle gain", self.idle_gain, 2.),
            ("Engine gain", self.engine_gain, 2.),
        ] {
            if !value.is_finite() || !(0.0..=max).contains(&value) {
                return Err(format!("{name} out of range"));
            }
        }
        Ok(())
    }
}

struct TransitionLevel {
    a2: f32,
    b2: f32,
    ab: f32,
}
impl TransitionLevel {
    fn new() -> Self {
        Self {
            a2: 1e-6,
            b2: 1e-6,
            ab: 0.,
        }
    }
    fn mix(&mut self, a: f32, b: f32, blend: f32, rate: f32) -> f32 {
        // A few firing pulses at idle are needed for a useful local estimate.
        let follow = 1. / (rate * 0.08);
        self.a2 += (a * a - self.a2) * follow;
        self.b2 += (b * b - self.b2) * follow;
        self.ab += (a * b - self.ab) * follow;
        if blend <= 0. {
            return a;
        }
        if blend >= 1. {
            return b;
        }
        let left = 1. - blend;
        let mixed = a * left + b * blend;
        let a_rms = self.a2.max(0.).sqrt();
        let b_rms = self.b2.max(0.).sqrt();
        let expected = a_rms * left + b_rms * blend;
        let mixed_energy =
            (self.a2 * left * left + self.b2 * blend * blend + 2. * self.ab * left * blend).max(0.);
        // If the branches oppose each other, a plain linear fade can nearly
        // vanish. Bound the correction so estimation errors cannot create a
        // burst, and leave the final B waveform and phase entirely intact.
        let floor = (expected * 0.05).powi(2).max(1e-12);
        let correction = (expected / mixed_energy.max(floor).sqrt()).clamp(0.5, 4.);
        mixed * correction
    }
}
pub struct Hybrid {
    bank: Option<Arc<Bank>>,
    physical_voice: Option<crate::automation_voice::AutomationVoice>,
    physical_error: Option<String>,
    physical_limiter: crate::output_limiter::OutputLimiter,
    target: Parameters,
    p: Parameters,
    h: Settings,
    current: Settings,
    rate: f32,
    cycle: f64,
    gain: f32,
    blend: f32,
    fast_load: f32,
    tick: u64,
    sinc: SincTable,
    transition_level: TransitionLevel,
    raw_energy: f32,
    wet_energy: f32,
    compensation: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HybridStems {
    pub exhaust: f32,
    pub engine: f32,
    /// Original Automation exhaust at the processed Bank gain, before export
    /// normalization. It does not enter either audible B stem.
    pub source_reference: f32,
    pub mixed: f32,
}
impl Hybrid {
    pub fn new(rate: u32, mut p: Parameters, h: Settings, bank: Option<Arc<Bank>>) -> Self {
        if let Some(bank) = &bank {
            p.rpm = p.rpm.clamp(bank.min_rpm, bank.max_rpm);
        }
        let (physical_voice, physical_error) = match bank.as_deref() {
            Some(bank) => {
                match crate::automation_voice::AutomationVoice::from_settings(rate, bank, h) {
                    Ok(voice) => (Some(voice), None),
                    Err(error) => (None, Some(error)),
                }
            }
            None => (None, None),
        };
        Self {
            bank,
            physical_voice,
            physical_error,
            physical_limiter: crate::output_limiter::OutputLimiter::new(rate),
            target: p,
            p,
            h,
            current: h,
            rate: rate.max(8000) as f32,
            cycle: 0.,
            gain: 0.,
            blend: if h.enhanced { 1. } else { 0. },
            fast_load: p.load,
            tick: 0,
            sinc: SincTable::for_quality(SincQuality::Realtime),
            transition_level: TransitionLevel::new(),
            raw_energy: 0.001,
            wet_energy: 0.001,
            compensation: 1.,
        }
    }
    pub fn set(&mut self, mut p: Parameters, h: Settings) {
        if p.validate().is_err() || h.validate().is_err() {
            return;
        }
        if let Some(bank) = &self.bank {
            p.rpm = p.rpm.clamp(bank.min_rpm, bank.max_rpm);
        }
        let sound = h.engine.map_or(h.physical_sound, |engine| engine.sound);
        let previous_sound = self
            .h
            .engine
            .map_or(self.h.physical_sound, |engine| engine.sound);
        if sound != previous_sound
            && let Some(voice) = &mut self.physical_voice
        {
            voice.set_sound_tuning(&sound);
        }
        self.target = p;
        self.h = h;
    }
    fn rpm_range(&self) -> (f32, f32) {
        self.physical_range().unwrap_or_else(|| {
            self.bank
                .as_ref()
                .map_or((300., 8000.), |b| (b.min_rpm, b.max_rpm))
        })
    }
    pub fn inferred_layer_weight(&self) -> f32 {
        1.
    }
    pub fn next_stems(&mut self, playing: bool) -> HybridStems {
        self.next_command_stems(playing, None)
    }
    /// The listening bench supplies the same complete commands for both origins.
    pub fn next_command_stems(
        &mut self,
        playing: bool,
        commands: Option<crate::physical::engine::Commands>,
    ) -> HybridStems {
        if self.bank.is_some() {
            self.next_bank(playing, commands)
        } else {
            HybridStems {
                exhaust: 0.,
                engine: 0.,
                source_reference: 0.,
                mixed: 0.,
            }
        }
    }
    pub fn rpm(&self) -> f32 {
        self.p.rpm
    }
    pub fn load(&self) -> f32 {
        self.fast_load
    }
    pub(crate) fn export_stem_normalizer(&self) -> f32 {
        self.gain * self.compensation * self.blend * self.bank.as_ref().map_or(1., |bank| bank.gain)
    }
    pub(crate) fn exhaust_export_normalizer(&self) -> f32 {
        self.gain * self.bank.as_ref().map_or(1., |bank| bank.gain)
    }
    pub fn next(&mut self, playing: bool) -> f32 {
        self.next_stems(playing).mixed
    }
    pub fn initialization_error(&self) -> Option<&str> {
        self.physical_error.as_deref()
    }
    pub(crate) fn physical_state(&self) -> Option<(crate::physical::engine::Sample, f64)> {
        self.physical_voice
            .as_ref()
            .map(|voice| (voice.state(), voice.inertia()))
    }
    pub(crate) fn physical_range(&self) -> Option<(f32, f32)> {
        self.physical_voice.as_ref().map(|voice| voice.rpm_range())
    }
    pub(crate) fn reset_physical(&mut self) {
        if let Some(voice) = &mut self.physical_voice {
            voice.reset();
        }
    }
    pub(crate) fn swap_model(
        &mut self,
        model: crate::scratch::ScratchModel,
    ) -> Option<crate::scratch::ScratchVoice> {
        if let Some(voice) = &mut self.physical_voice {
            voice.swap_model(model)
        } else {
            Some(crate::scratch::ScratchVoice::Prepared {
                physical: model.physical,
            })
        }
    }
    pub(crate) fn take_retired(&mut self) -> Option<crate::scratch::ScratchVoice> {
        self.physical_voice
            .as_mut()
            .and_then(|voice| voice.take_retired())
    }
    pub fn failed(&self) -> bool {
        self.bank.is_some()
            && self.h.enhanced
            && self
                .physical_voice
                .as_ref()
                .is_none_or(|voice| voice.failed())
    }
    fn next_bank(
        &mut self,
        playing: bool,
        commands: Option<crate::physical::engine::Commands>,
    ) -> HybridStems {
        let smooth = 1. / (self.rate * 0.025);
        self.gain += (if playing { self.target.volume } else { 0. } - self.gain) * smooth;
        self.blend += (if self.h.enhanced { 1. } else { 0. } - self.blend) * smooth;
        let rpm_smooth = 1. / (self.rate * (0.025 + self.h.response * 0.35));
        self.p.rpm += (self.target.rpm - self.p.rpm) * rpm_smooth;
        let response = if self.target.load > self.fast_load {
            0.015 + self.h.response * 0.14
        } else {
            0.025 + self.h.response * 0.2
        };
        self.fast_load += (self.target.load - self.fast_load) / (self.rate * response);
        if let Some(commands) = commands {
            self.p.rpm = commands.imposed_rpm.unwrap_or_else(|| {
                self.physical_voice
                    .as_ref()
                    .map_or(f64::from(self.p.rpm), |voice| voice.state().rpm)
            }) as f32;
            self.fast_load = commands.throttle as f32;
        }
        self.cycle += self.p.rpm as f64 / (120. * self.rate as f64);
        if self.tick.is_multiple_of(64) {
            let s = 64. / (self.rate * 0.05);
            self.current.idle_gain += (self.h.idle_gain - self.current.idle_gain) * s;
            self.current.engine_gain += (self.h.engine_gain - self.current.engine_gain) * s;
            self.p.intake += (self.target.intake - self.p.intake) * s;
            self.p.exhaust += (self.target.exhaust - self.p.exhaust) * s;
            self.p.mechanical += (self.target.mechanical - self.p.mechanical) * s;
        }
        self.tick = self.tick.wrapping_add(1);
        let bank = self.bank.as_ref().expect("bank path");
        let raw = bank.read_original(
            self.cycle,
            self.p.rpm,
            self.fast_load,
            self.rate,
            &self.sinc,
        );
        let source_scale = bank.original_to_processed_gain();
        // Same fixed pressure-to-listening calibration as physical scratch.
        // This is not an RMS normalizer; only the output limiter attenuates peaks.
        let bank_gain = bank.gain * 16.;
        let generated = if playing || self.gain > 1e-6 {
            self.physical_voice.as_mut().map_or_else(
                crate::automation_voice::Stems::default,
                |voice| {
                    voice.next_commands(commands.unwrap_or(crate::physical::engine::Commands {
                        imposed_rpm: Some(f64::from(self.p.rpm)),
                        throttle: f64::from(self.fast_load),
                        overrun: f64::from(self.h.fuel_cut),
                        starter: self.h.starter,
                        ac: self.h.accessory_ac,
                        steering: self.h.accessory_steering,
                        ..Default::default()
                    }))
                },
            )
        } else {
            crate::automation_voice::Stems::default()
        };
        let exhaust = generated.exhaust * self.p.exhaust * bank_gain;
        let engine = (generated.intake * self.p.intake + generated.mechanical * self.p.mechanical)
            * self.current.engine_gain
            * bank_gain;
        let wet = exhaust + engine * 0.25;
        let energy_smooth = 1. / (self.rate * 1.5);
        self.raw_energy += (raw * raw - self.raw_energy) * energy_smooth;
        self.wet_energy += (wet * wet - self.wet_energy) * energy_smooth;
        if self.tick.is_multiple_of(64) {
            let target = if self.h.level_match {
                (self.raw_energy / self.wet_energy.max(1e-9))
                    .sqrt()
                    .clamp(0.02, 2.)
            } else {
                1.
            };
            self.compensation += (target - self.compensation) * (64. / (self.rate * 0.2));
        }
        let b_gain = self.compensation * self.idle_gain_mult();
        // Only audition is limited. Export receives physical stems at the same
        // volume*bank.gain scale as its existing normalization contract.
        let b = self.physical_limiter.next(wet * b_gain * self.gain);
        let out = self
            .transition_level
            .mix(raw * self.gain, b, self.blend, self.rate);
        // This is the original A safety characteristic, unchanged at blend=0.
        let mixed = if out.abs() > 0.95 {
            out.signum() * (0.95 + 0.049 * ((out.abs() - 0.95) / 0.049).tanh())
        } else {
            out
        };
        HybridStems {
            exhaust: exhaust * b_gain * self.blend * self.gain,
            engine: engine * b_gain * self.blend * self.gain,
            source_reference: raw * source_scale * self.gain,
            mixed,
        }
    }
    fn idle_gain_mult(&self) -> f32 {
        let idle_ref = if self.bank.is_some() {
            self.rpm_range().0
        } else {
            800.
        };
        let idle_span = 1400.0f32;
        let idle_t = ((idle_ref + idle_span - self.p.rpm) / idle_span).clamp(0., 1.);
        let idle_weight = idle_t * idle_t * (3. - 2. * idle_t);
        1.0 + (self.current.idle_gain - 1.0) * idle_weight
    }
}
pub fn audition(t: f32, base: Parameters, max: f32) -> Parameters {
    let idle = base.rpm.clamp(500., 1500.);
    let high = max.min(8000.);
    let (rpm, load) = if t < 2. {
        (idle, 0.12)
    } else if t < 4. {
        (idle + (2200. - idle) * (t - 2.) / 2., (t - 2.) / 2.)
    } else if t < 9. {
        (2200. + (high - 2200.) * (t - 4.) / 5., 1.)
    } else if t < 12. {
        (high - (high - 2400.) * (t - 9.) / 3., 0.02)
    } else if t < 15. {
        (2400. + (high - 2400.) * (t - 12.) / 3., 0.85)
    } else {
        (idle, 0.1)
    };
    Parameters { rpm, load, ..base }
}

#[cfg(test)]
mod transition_tests {
    use super::TransitionLevel;

    #[test]
    fn anti_correlated_ab_transition_keeps_level_without_changing_endpoints() {
        let rate = 48_000.;
        let mut meter = TransitionLevel::new();
        // Equal-level engine-order components with a local correlation of -0.8.
        let signal = |i: usize| {
            let phase = std::f32::consts::TAU * 200. * i as f32 / rate;
            let a = phase.sin() * 0.4;
            let b = (-0.8 * phase.sin() + 0.6 * phase.cos()) * 0.4;
            (a, b)
        };
        for i in 0..rate as usize {
            let (a, b) = signal(i);
            assert_eq!(meter.mix(a, b, 0., rate).to_bits(), a.to_bits());
            assert_eq!(meter.mix(a, b, 1., rate).to_bits(), b.to_bits());
        }
        let mut corrected = 0.;
        let mut plain = 0.;
        let mut reference = 0.;
        let mut count = 0;
        let mut blend = 0.;
        for i in rate as usize..rate as usize + 12_000 {
            blend += (1. - blend) / (rate * 0.025);
            let (a, b) = signal(i);
            let mixed = meter.mix(a, b, blend, rate);
            if (0.45..=0.55).contains(&blend) {
                corrected += mixed * mixed;
                plain += (a * (1. - blend) + b * blend).powi(2);
                reference += (a * a + b * b) * 0.5;
                count += 1;
            }
        }
        assert!(count > 100);
        let corrected_ratio = (corrected / reference).sqrt();
        let plain_ratio = (plain / reference).sqrt();
        assert!(
            plain_ratio < 0.55,
            "plain blend unexpectedly loud: {plain_ratio}"
        );
        assert!(
            (0.85..=1.15).contains(&corrected_ratio),
            "transition level {corrected_ratio}"
        );
    }
}
