use crate::{
    acoustics::{Exhaust, Geometry, Intake},
    bank::Bank,
    engine::Engine,
    procedural::ProceduralVoice,
    project::Parameters,
};
use bdsp::{
    noise::{Noise, NoiseColor},
    resample::{SincQuality, SincTable},
    svf::{StateVariableFilter, SvfMode},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub enhanced: bool,
    /// Use measured descriptors to generate B without replaying source PCM.
    pub procedural: bool,
    /// Neutral-at-one controls for the descriptor-driven B sound.
    pub generated_body: f32,
    pub generated_edge: f32,
    pub generated_flow: f32,
    pub generated_mechanics: f32,
    pub level_match: bool,
    pub response: f32,
    pub attack: f32,
    pub body: f32,
    pub rasp: f32,
    pub texture: f32,
    pub pipe: f32,
    pub overrun: f32,
    pub turbo: f32,
    pub roughness: f32,
    pub header_length: f32,
    pub diameter: f32,
    pub chamber: f32,
    pub absorption: f32,
    pub temperature: f32,
    pub intake_length: f32,
    pub airbox: f32,
    pub fuel_cut: f32,
    pub pulse_gain: f32,
    pub residual_gain: f32,
    /// Source driven cycle-to-cycle pressure variation; never infers cylinders.
    pub cycle_life: f32,
    /// Couples the recorded texture to the recorded pressure envelope.
    pub pulse_texture: f32,
    /// Blend of recorded cycle pressure with its band-limited pressure edge.
    pub pressure_shape: f32,
    pub maps: crate::maps::Maps,
    pub rpm_character: f32,
    pub load_character: f32,
    pub coloration: f32,
    pub combustion: crate::combustion::Combustion,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enhanced: true,
            procedural: false,
            generated_body: 1.,
            generated_edge: 1.,
            generated_flow: 1.,
            generated_mechanics: 1.,
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
            combustion: Default::default(),
        }
    }
}
impl Settings {
    /// BeamNG receives the source-guided sound, even when the live bench is
    /// auditioning the experimental generator.
    pub fn for_beamng_export(self) -> Self {
        Self {
            procedural: false,
            generated_body: 1.,
            generated_edge: 1.,
            generated_flow: 1.,
            generated_mechanics: 1.,
            ..self
        }
    }

    pub fn calibrated(bank: &Bank) -> Self {
        let c = bank.character();
        let bright = (c.edge_ratio * 4.).clamp(0., 1.);
        let periodic = c.cycle_similarity.clamp(0., 1.);
        Self {
            procedural: false,
            body: 0.08 + 0.12 * (1. - bright),
            rasp: 0.03 + 0.05 * (1. - bright),
            pipe: 0.03 + 0.08 * (1. - periodic),
            airbox: 0.08,
            texture: 0.06,
            attack: 0.25,
            roughness: 0.,
            overrun: 0.,
            turbo: 0.,
            // The acoustic path now carries a clear part of the output.
            coloration: 0.6 + 0.1 * (1. - periodic),
            cycle_life: 0.5,
            pulse_texture: 0.55,
            pressure_shape: 0.5 - 0.3 * bright,
            ..Self::default()
        }
    }
    pub fn character_for_bank(index: usize, bank: &Bank) -> Self {
        let base = Self::calibrated(bank);
        match index {
            1 => Self {
                chamber: 9.,
                absorption: 0.75,
                pipe: 0.18,
                rasp: 0.02,
                body: 0.18,
                generated_body: 0.9,
                generated_edge: 0.72,
                generated_flow: 0.7,
                generated_mechanics: 0.75,
                ..base
            },
            2 => Self {
                chamber: 1.2,
                absorption: 0.22,
                pipe: 0.15,
                rasp: 0.25,
                attack: 0.5,
                coloration: 0.5,
                generated_body: 1.05,
                generated_edge: 1.2,
                generated_flow: 1.15,
                generated_mechanics: 1.05,
                ..base
            },
            3 => Self {
                chamber: 6.,
                absorption: 0.6,
                pipe: 0.1,
                body: 0.25,
                rasp: 0.04,
                generated_body: 1.3,
                generated_edge: 0.78,
                generated_flow: 0.8,
                generated_mechanics: 0.85,
                ..base
            },
            4 => Self {
                chamber: 2.5,
                absorption: 0.42,
                texture: 0.08,
                generated_body: 1.0,
                generated_edge: 0.9,
                generated_flow: 0.75,
                generated_mechanics: 1.5,
                ..base
            },
            5 => Self {
                chamber: 1.7,
                absorption: 0.3,
                rasp: 0.2,
                texture: 0.14,
                generated_body: 0.95,
                generated_edge: 1.15,
                generated_flow: 1.35,
                generated_mechanics: 1.05,
                ..base
            },
            _ => base,
        }
    }
    /// Timbre presets must retain explicit engine timing and transport choices.
    pub fn character_preserving_engine(self, index: usize, bank: Option<&Bank>) -> Self {
        let preset = bank.map_or_else(
            || Self::character(index),
            |b| Self::character_for_bank(index, b),
        );
        Self {
            enhanced: self.enhanced,
            procedural: self.procedural,
            level_match: self.level_match,
            response: self.response,
            combustion: self.combustion,
            ..preset
        }
    }
    /// Replace detailed maps only when the user edits these simplified controls.
    pub fn rebuild_character_maps(&mut self) {
        let make = |rpm_amount: f32, load_amount: f32| {
            let mut map = crate::maps::Map::default();
            for (j, row) in map.0.iter_mut().enumerate() {
                for (i, v) in row.iter_mut().enumerate() {
                    *v = (1.
                        + rpm_amount * self.rpm_character * i as f32 / 2.
                        + load_amount * self.load_character * j as f32 / 2.)
                        .clamp(0., 2.);
                }
            }
            map
        };
        self.maps = crate::maps::Maps {
            pulse: make(0.15, 0.25),
            texture: make(0.35, 0.45),
            intake: make(0.25, 0.55),
            exhaust: make(0.05, 0.1),
        };
    }
    pub fn validate(&self) -> Result<(), String> {
        self.combustion.validate()?;
        self.maps.validate()?;
        for v in [
            self.response,
            self.attack,
            self.body,
            self.rasp,
            self.texture,
            self.pipe,
            self.overrun,
            self.turbo,
            self.roughness,
            self.absorption,
            self.airbox,
            self.fuel_cut,
            self.coloration,
            self.cycle_life,
            self.pulse_texture,
            self.pressure_shape,
        ] {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err("Hybrid setting out of range".into());
            }
        }
        for value in [
            self.generated_body,
            self.generated_edge,
            self.generated_flow,
            self.generated_mechanics,
        ] {
            if !value.is_finite() || !(0.0..=2.0).contains(&value) {
                return Err("Generated sound setting out of range".into());
            }
        }
        for (v, min, max) in [
            (self.rpm_character, -1., 1.),
            (self.load_character, -1., 1.),
            (self.pulse_gain, 0., 2.),
            (self.residual_gain, 0., 2.),
            (self.header_length, 0.15, 1.5),
            (self.diameter, 30., 130.),
            (self.chamber, 0.3, 18.),
            (self.temperature, 150., 950.),
            (self.intake_length, 0.12, 1.2),
        ] {
            if !v.is_finite() || !(min..=max).contains(&v) {
                return Err("Acoustic dimension out of range".into());
            }
        }
        Ok(())
    }
    /// Starting points for the same imported engine, never cylinder-count presets.
    pub fn character(index: usize) -> Self {
        match index {
            1 => Self {
                body: 0.35,
                rasp: 0.08,
                attack: 0.2,
                overrun: 0.02,
                pipe: 0.5,
                chamber: 11.,
                absorption: 0.8,
                diameter: 52.,
                header_length: 0.85,
                airbox: 0.65,
                texture: 0.1,
                fuel_cut: 0.7,
                generated_body: 0.9,
                generated_edge: 0.72,
                generated_flow: 0.7,
                generated_mechanics: 0.75,
                ..Self::default()
            },
            2 => Self {
                body: 0.45,
                rasp: 0.65,
                attack: 0.75,
                overrun: 0.38,
                pipe: 0.42,
                chamber: 0.6,
                absorption: 0.12,
                diameter: 90.,
                header_length: 0.35,
                intake_length: 0.22,
                airbox: 0.65,
                texture: 0.5,
                fuel_cut: 0.15,
                generated_body: 1.05,
                generated_edge: 1.2,
                generated_flow: 1.15,
                generated_mechanics: 1.05,
                ..Self::default()
            },
            3 => Self {
                body: 0.55,
                rasp: 0.12,
                chamber: 8.,
                absorption: 0.65,
                generated_body: 1.3,
                generated_edge: 0.78,
                generated_flow: 0.8,
                generated_mechanics: 0.85,
                ..Self::default()
            },
            4 => Self {
                body: 0.3,
                rasp: 0.2,
                chamber: 2.5,
                texture: 0.18,
                generated_body: 1.,
                generated_edge: 0.9,
                generated_flow: 0.75,
                generated_mechanics: 1.5,
                ..Self::default()
            },
            5 => Self {
                body: 0.3,
                rasp: 0.55,
                chamber: 1.5,
                texture: 0.4,
                generated_body: 0.95,
                generated_edge: 1.15,
                generated_flow: 1.35,
                generated_mechanics: 1.05,
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}
fn geometry(p: Parameters, h: Settings) -> Geometry {
    Geometry {
        header: h.header_length,
        tail: p.pipe_length,
        diameter_mm: h.diameter,
        chamber_litres: h.chamber,
        absorption: h.absorption,
        resonance: p.resonance,
        temperature_c: h.temperature,
    }
}

fn mechanical_cylinders(bank: Option<&Bank>, h: Settings) -> u32 {
    (h.combustion.cylinders > 0)
        .then_some(h.combustion.cylinders)
        .or_else(|| bank.and_then(|bank| bank.engine_meta.as_ref().map(|meta| meta.cylinders)))
        .unwrap_or(0)
}

pub(crate) fn inferred_layer_weight(reliability: f32) -> f32 {
    0.15 + 0.85 * reliability.clamp(0., 1.)
}

/// Learns only the source texture that persists at the same crank phase.
/// Unlike a one-cycle delay subtraction, it does not boost alternating cycles.
struct PhaseLockedTexture {
    bins: [f32; 1024],
}

impl PhaseLockedTexture {
    fn new() -> Self {
        Self { bins: [0.; 1024] }
    }

    fn next(&mut self, phase: f32, sample: f32, rpm: f32, rate: f32) -> f32 {
        let position = phase * self.bins.len() as f32;
        let first = (position as usize).min(self.bins.len() - 1);
        let second = (first + 1) % self.bins.len();
        let t = position - first as f32;
        let estimate = self.bins[first] * (1. - t) + self.bins[second] * t;
        let detail = sample - estimate;
        // Each bin receives approximately 0.5 of a cycle's observations,
        // independent of RPM. The cap avoids overfitting a single high-RPM hit.
        let per_sample = (0.5 * self.bins.len() as f32 * rpm / (120. * rate)).min(0.65);
        self.bins[first] += detail * per_sample * (1. - t);
        self.bins[second] += detail * per_sample * t;
        detail
    }
}

/// Short-time level and covariance for a phase-safe A/B audition transition.
/// This only changes the intermediate blend: pure A and pure B are untouched.
struct TransitionLevel {
    a2: f32,
    b2: f32,
    ab: f32,
}

/// A quiet, irregular valve-cover/engine-block texture. The impact rate follows
/// known cylinder metadata, but this does not assign a firing order or add an
/// exhaust pulse. Fixed resonances and small timing/strength changes prevent an
/// identical click from repeating in phase with the recorded exhaust.
struct MechanicalImpacts {
    slots: u32,
    previous_slot: i64,
    seed: u64,
    delay: u32,
    pending: f32,
    cover: StateVariableFilter,
    block: StateVariableFilter,
}

impl MechanicalImpacts {
    fn new(rate: f32, slots: u32) -> Self {
        Self {
            slots,
            previous_slot: -1,
            seed: 0x4D45_4348_414E_4943,
            delay: 0,
            pending: 0.,
            cover: StateVariableFilter::new(rate, 2200., 0.65, SvfMode::Bandpass),
            block: StateVariableFilter::new(rate, 850., 0.65, SvfMode::Bandpass),
        }
    }

    fn set_slots(&mut self, slots: u32) {
        if self.slots != slots {
            self.slots = slots;
            self.previous_slot = -1;
            self.delay = 0;
            self.pending = 0.;
        }
    }

    fn next(&mut self, cycle: f64, source_level: f32, load: f32) -> f32 {
        if self.slots == 0 {
            return 0.;
        }
        let slot = (cycle * self.slots as f64 + 0.13).floor() as i64;
        if slot != self.previous_slot {
            self.previous_slot = slot;
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 7;
            self.seed ^= self.seed << 17;
            // At 48 kHz the maximum offset is 0.33 ms. Strength changes remain
            // small enough to preserve the engine's steady operating level.
            self.delay = ((self.seed >> 32) % 17) as u32;
            let strength = 0.78 + ((self.seed >> 48) as u16 as f32 / 65535.) * 0.44;
            self.pending = source_level * (0.8 + 0.2 * load) * strength;
        }
        let impulse = if self.delay == 0 {
            std::mem::take(&mut self.pending)
        } else {
            self.delay -= 1;
            0.
        };
        self.cover.next_sample(impulse) + self.block.next_sample(impulse) * 0.38
    }
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
    fallback: Option<Engine>,
    sinc: SincTable,
    target: Parameters,
    p: Parameters,
    h: Settings,
    current: Settings,
    rate: f32,
    cycle: f64,
    wet_cycle: f64,
    gain: f32,
    blend: f32,
    transition_level: TransitionLevel,
    tick: u64,
    fast_load: f32,
    slow_load: f32,
    boost: f32,
    flutter: f32,
    noise: Noise,
    low: StateVariableFilter,
    high: StateVariableFilter,
    intake: StateVariableFilter,
    muffler: StateVariableFilter,
    exhaust_line: Exhaust,
    intake_runner: Intake,
    cut: f32,
    dc: StateVariableFilter,
    engine_dc: StateVariableFilter,
    engine_texture: StateVariableFilter,
    engine_air_band: StateVariableFilter,
    engine_stem_dc: StateVariableFilter,
    mechanical_impacts: MechanicalImpacts,
    mechanical_source_energy: f32,
    inferred_weight: f32,
    phase_locked_texture: PhaseLockedTexture,
    procedural_voice: Option<ProceduralVoice>,
    raw_energy: f32,
    wet_energy: f32,
    compensation: f32,
    transient: f32,
    crackle: f32,
    crackle_age: f32,
    pop_clock: f32,
    pop_interval: f32,
    crackle_phase: f32,
    crackle_frequency: f32,
    pop_filter: StateVariableFilter,
    turbo_phase: f32,
    event_gain: f32,
    cycle_number: i64,
    cycle_from: f32,
    cycle_to: f32,
    cycle_seed: u64,
    pulse_envelope: f32,
    pulse_average: f32,
    previous_pulse: f32,
    pressure_edge: StateVariableFilter,
    pulse_energy: f32,
    edge_energy: f32,
    pressure_ready: f32,
}

/// BeamNG exhaust and engine emitters derived from one Automation recording.
/// `mixed` remains the normal audition output. The engine stem also contains
/// subtle modeled mechanical impacts, so the two export stems do not sum to it.
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
        let rate = rate.max(8000) as f32;
        let filter = |cut, q, mode| StateVariableFilter::new(rate, cut, q, mode);
        let fallback = if bank.is_none() {
            Some(Engine::new(rate as u32, p))
        } else {
            None
        };
        let mechanical_cylinders = mechanical_cylinders(bank.as_deref(), h);
        let procedural_cylinders = bank
            .as_ref()
            .and_then(|bank| bank.engine_meta.as_ref().map(|meta| meta.cylinders))
            .unwrap_or(p.cylinders);
        let inferred_weight = bank.as_ref().map_or(1., |bank| {
            inferred_layer_weight(bank.residual_reliability(p.rpm, p.load))
        });
        let procedural_voice = bank
            .as_ref()
            .map(|bank| ProceduralVoice::new(rate, bank.procedural_model(), procedural_cylinders));
        Self {
            bank,
            fallback,
            sinc: SincTable::for_quality(SincQuality::Realtime),
            target: p,
            p,
            h,
            current: h,
            rate,
            cycle: 0.,
            wet_cycle: 0.,
            gain: 0.,
            blend: if h.enhanced { 1. } else { 0. },
            transition_level: TransitionLevel::new(),
            tick: 0,
            fast_load: p.load,
            slow_load: p.load,
            boost: 0.,
            flutter: 0.,
            noise: Noise::with_seed(NoiseColor::White, 0xB355),
            low: filter(240., 0.707, SvfMode::Lowpass),
            high: filter(1800., 0.707, SvfMode::Highpass),
            intake: filter(1300., 0.8, SvfMode::Bandpass),
            muffler: filter(10000., 0.707, SvfMode::Lowpass),
            exhaust_line: Exhaust::new(rate, geometry(p, h)),
            intake_runner: Intake::new(rate, h.intake_length),
            cut: 0.,
            dc: filter(18., 0.707, SvfMode::Highpass),
            engine_dc: filter(18., 0.707, SvfMode::Highpass),
            engine_texture: filter(200., 0.707, SvfMode::Highpass),
            engine_air_band: filter(3600., 0.707, SvfMode::Lowpass),
            engine_stem_dc: filter(18., 0.707, SvfMode::Highpass),
            mechanical_impacts: MechanicalImpacts::new(rate, mechanical_cylinders),
            mechanical_source_energy: 0.,
            inferred_weight,
            phase_locked_texture: PhaseLockedTexture::new(),
            procedural_voice,
            raw_energy: 0.001,
            wet_energy: 0.001,
            compensation: 1.,
            transient: 0.,
            crackle: 0.,
            crackle_age: 1.,
            pop_clock: 0.,
            pop_interval: 1.,
            crackle_phase: 0.,
            crackle_frequency: 130.,
            pop_filter: filter(1800., 0.707, SvfMode::Lowpass),
            turbo_phase: 0.,
            event_gain: 0.,
            cycle_number: -1,
            cycle_from: 0.,
            cycle_to: 0.,
            cycle_seed: 0xB355_7200_5EED,
            pulse_envelope: 0.,
            pulse_average: 0.01,
            previous_pulse: 0.,
            pressure_edge: filter(1800., 0.707, SvfMode::Lowpass),
            pulse_energy: 1e-6,
            edge_energy: 1e-6,
            pressure_ready: 0.,
        }
    }
    pub fn set(&mut self, mut p: Parameters, h: Settings) {
        if p.validate().is_err() || h.validate().is_err() {
            return;
        }
        // The same exported limits apply to manual control, audition and WAV rendering.
        if let Some(bank) = &self.bank {
            p.rpm = p.rpm.clamp(bank.min_rpm, bank.max_rpm);
        }
        self.target = p;
        self.h = h;
        let mechanical_cylinders = mechanical_cylinders(self.bank.as_deref(), h);
        self.mechanical_impacts.set_slots(mechanical_cylinders);
        if let Some(voice) = &mut self.procedural_voice {
            voice.set_cylinders(
                self.bank
                    .as_ref()
                    .and_then(|bank| bank.engine_meta.as_ref().map(|meta| meta.cylinders))
                    .unwrap_or(p.cylinders),
            );
        }
        if let Some(e) = &mut self.fallback {
            e.set_parameters(p);
        }
    }
    pub fn rpm(&self) -> f32 {
        self.p.rpm
    }
    pub fn load(&self) -> f32 {
        self.fast_load
    }
    pub fn inferred_layer_weight(&self) -> f32 {
        if self.h.procedural {
            1.
        } else {
            self.inferred_weight
        }
    }
    /// Divide a live engine stem by this factor to compare its peak with an
    /// exported stem, which removes playback level and the Bank gain.
    pub(crate) fn export_stem_normalizer(&self) -> f32 {
        self.gain * self.compensation * self.blend * self.bank.as_ref().map_or(1., |bank| bank.gain)
    }
    /// Divide the live B exhaust and source reference by this factor to
    /// compare their levels at the same scale as the exported WAVs.
    pub(crate) fn exhaust_export_normalizer(&self) -> f32 {
        self.gain * self.bank.as_ref().map_or(1., |bank| bank.gain)
    }
    pub fn next(&mut self, playing: bool) -> f32 {
        self.next_stems(playing).mixed
    }
    fn next_procedural(&mut self, raw: f32, source_scale: f32) -> HybridStems {
        let bank = self.bank.as_ref().expect("procedural voice has a bank");
        let position = (self.p.rpm - bank.min_rpm) / (bank.max_rpm - bank.min_rpm).max(1.);
        let generated = self
            .procedural_voice
            .as_mut()
            .expect("procedural voice was prepared before playback")
            .next(self.p.rpm, self.fast_load, self.wet_cycle);
        // The neutral preset leaves the R8 generator unchanged. Other presets
        // adjust broad components, not the measured firing-order phases.
        let low_tone = self.low.next_sample(generated.exhaust_tone);
        let base_exhaust = generated.exhaust_tone
            + (self.current.generated_body - 1.) * low_tone
            + generated.exhaust_texture * self.current.generated_flow;
        let upper = self.high.next_sample(base_exhaust);
        let exhaust_source = base_exhaust + (self.current.generated_edge - 1.) * upper;
        let exhaust = self.dc.next_sample(
            exhaust_source
                * self.p.exhaust
                * self.current.maps.exhaust.at(position, self.fast_load),
        );
        let engine = self.engine_stem_dc.next_sample(
            generated.intake
                * self.p.intake
                * self.current.maps.intake.at(position, self.fast_load)
                + generated.mechanical * self.p.mechanical * self.current.generated_mechanics,
        );
        let wet = exhaust + engine * 0.25;
        let energy_smooth = 1. / (self.rate * 1.5);
        // Keep the level reference in the descriptor domain. Following the
        // source waveform here would imprint its slow amplitude motion onto B.
        let measured_level = self
            .procedural_voice
            .as_ref()
            .expect("procedural voice was prepared before playback")
            .mean_source_rms(self.p.rpm, self.fast_load);
        self.raw_energy += (measured_level * measured_level - self.raw_energy) * energy_smooth;
        self.wet_energy += (wet * wet - self.wet_energy) * energy_smooth;
        if self.tick.is_multiple_of(64) {
            let target = if self.h.level_match {
                (self.raw_energy / self.wet_energy.max(1e-9))
                    .sqrt()
                    .clamp(0.4, 2.)
            } else {
                1.
            };
            self.compensation += (target - self.compensation) * (64. / (self.rate * 0.2));
        }
        let out = self
            .transition_level
            .mix(raw, wet * self.compensation, self.blend, self.rate)
            * self.gain;
        let mixed = if out.abs() > 0.95 {
            out.signum() * (0.95 + 0.049 * ((out.abs() - 0.95) / 0.049).tanh())
        } else {
            out
        };
        let engine_stem = engine * self.compensation * self.blend * self.gain;
        HybridStems {
            exhaust: mixed - engine_stem * 0.25,
            engine: engine_stem,
            source_reference: raw * source_scale * self.gain,
            mixed,
        }
    }
    pub fn next_stems(&mut self, playing: bool) -> HybridStems {
        if let Some(e) = &mut self.fallback {
            let sample = e.next_sample(playing);
            return HybridStems {
                exhaust: sample,
                engine: 0.,
                source_reference: 0.,
                mixed: sample,
            };
        }
        let smooth = 1. / (self.rate * 0.025);
        self.gain += (if playing { self.target.volume } else { 0. } - self.gain) * smooth;
        self.blend += (if self.h.enhanced { 1. } else { 0. } - self.blend) * smooth;
        // Dynamics are shared by A/B; neither branch resets its phase or its state.
        let rpm_smooth = 1. / (self.rate * (0.025 + self.h.response * 0.35));
        self.p.rpm += (self.target.rpm - self.p.rpm) * rpm_smooth;
        let response = if self.target.load > self.fast_load {
            0.015 + self.h.response * 0.14
        } else {
            0.025 + self.h.response * 0.2
        };
        self.fast_load += (self.target.load - self.fast_load) / (self.rate * response);
        self.slow_load += (self.fast_load - self.slow_load) / (self.rate * 0.24);
        self.transient = (self.fast_load - self.slow_load).max(0.);
        let lift = (self.slow_load - self.fast_load).max(0.);
        let noise = self.noise.next_sample();
        self.flutter += (noise - self.flutter) / (self.rate * 0.025);
        let rough = 1. + self.current.roughness * self.flutter * 0.3 * (1. - self.fast_load);
        self.cycle += self.p.rpm as f64 / (120. * self.rate as f64);
        self.wet_cycle += self.p.rpm as f64 / (120. * self.rate as f64) * rough as f64;
        if self.tick.is_multiple_of(64) {
            let s = (64. / (self.rate * 0.04)).min(1.);
            self.current.maps.follow(self.h.maps, s);
            macro_rules! follow {($($field:ident),*)=>{$(self.current.$field+=(self.h.$field-self.current.$field)*s;)*};}
            follow!(
                attack,
                body,
                rasp,
                texture,
                pipe,
                overrun,
                turbo,
                roughness,
                header_length,
                diameter,
                chamber,
                absorption,
                temperature,
                intake_length,
                airbox,
                fuel_cut,
                pulse_gain,
                residual_gain,
                cycle_life,
                pulse_texture,
                pressure_shape,
                coloration,
                generated_body,
                generated_edge,
                generated_flow,
                generated_mechanics
            );
            macro_rules! follow_p {($($field:ident),*)=>{$(self.p.$field+=(self.target.$field-self.p.$field)*s;)*};}
            follow_p!(
                intake,
                exhaust,
                mechanical,
                brightness,
                resonance,
                pipe_length,
                uneven
            );
            self.low.set_cutoff(110. + self.p.rpm * 0.025);
            self.engine_texture.set_cutoff(140. + self.p.rpm * 0.025);
            self.engine_air_band.set_cutoff(3200. + self.p.rpm * 0.18);
            self.intake
                .set_cutoff(500. + self.fast_load * 900. + self.p.rpm * 0.1);
            self.muffler
                .set_cutoff(self.p.brightness * (0.55 + 0.45 * self.fast_load));
            self.exhaust_line
                .tune(geometry(self.p, self.current), false);
            self.intake_runner.tune(
                self.current.intake_length,
                self.fast_load,
                self.current.airbox,
            );
            if let Some(bank) = &self.bank {
                let target =
                    inferred_layer_weight(bank.residual_reliability(self.p.rpm, self.fast_load));
                self.inferred_weight += (target - self.inferred_weight) * s;
            }
        }
        self.tick = self.tick.wrapping_add(1);
        let bank = self
            .bank
            .as_ref()
            .expect("constructed with bank or fallback");
        let raw = bank.read_original(
            self.cycle,
            self.p.rpm,
            self.fast_load,
            self.rate,
            &self.sinc,
        );
        if self.h.procedural {
            let source_scale = bank.original_to_processed_gain();
            return self.next_procedural(raw, source_scale);
        }
        let (periodic, residual) = bank.read_components(
            self.wet_cycle,
            self.p.rpm,
            self.fast_load,
            self.rate,
            &self.sinc,
        );
        let phase = self.wet_cycle.fract() as f32;
        // Reuse the measured recording at a different complete 720-degree
        // cycle for the engine-side texture. Its pressure phase still aligns
        // with the current exhaust, but irregular detail is not copied at the
        // same instant into both spatial emitters.
        let inferred_source = bank.read(
            self.wet_cycle + 7.,
            self.p.rpm,
            self.fast_load,
            self.rate,
            &self.sinc,
        );
        let inferred_residual = self.phase_locked_texture.next(
            phase,
            inferred_source - periodic,
            self.p.rpm,
            self.rate,
        ) * self.inferred_weight;
        let position = (self.p.rpm - bank.min_rpm) / (bank.max_rpm - bank.min_rpm).max(1.);
        let pulse_gain =
            self.current.pulse_gain * self.current.maps.pulse.at(position, self.fast_load);
        let texture_gain =
            self.current.residual_gain * self.current.maps.texture.at(position, self.fast_load);
        // Keep cycle variation at the 720-degree rate and interpolate through
        // the cycle. A fresh random number for every sample would sound like
        // broadband flutter and would destroy the source's engine orders.
        let cycle_number = self.wet_cycle.floor() as i64;
        if cycle_number != self.cycle_number {
            self.cycle_number = cycle_number;
            self.cycle_from = self.cycle_to;
            self.cycle_seed ^= self.cycle_seed << 13;
            self.cycle_seed ^= self.cycle_seed >> 7;
            self.cycle_seed ^= self.cycle_seed << 17;
            let random = (self.cycle_seed >> 40) as f32 / 16_777_215. * 2. - 1.;
            self.cycle_to = (self.cycle_to * 0.55 + random * 0.45).clamp(-1., 1.);
        }
        let fade = phase * phase * (3. - 2. * phase);
        let variation = self.cycle_from + (self.cycle_to - self.cycle_from) * fade;
        let idle_factor = (1700. / self.p.rpm).clamp(0.35, 1.);
        // The cycle average contains the pressure rhythm already present in
        // the recording. A normalized, band-limited time derivative gives a
        // pressure-release edge, replacing part of that rhythm instead of
        // stacking an unrelated oscillator or guessed cylinder event on it.
        let edge = self
            .pressure_edge
            .next_sample(periodic - self.previous_pulse);
        self.previous_pulse = periodic;
        let energy_smooth = 1. / (self.rate * 0.3);
        self.pulse_energy += (periodic * periodic - self.pulse_energy) * energy_smooth;
        self.edge_energy += (edge * edge - self.edge_energy) * energy_smooth;
        let edge_scale = (self.pulse_energy / self.edge_energy.max(1e-12))
            .sqrt()
            .min(350.);
        self.pressure_ready += (1. - self.pressure_ready) / (self.rate * 0.12);
        // Give a middle setting a meaningful pressure-front contribution while
        // preserving the zero and full-scale endpoints of the control.
        let shape = self.current.pressure_shape.sqrt() * self.pressure_ready;
        let modeled_pulse = periodic * (1. - shape) + edge * edge_scale * shape;
        let living_pulse = modeled_pulse
            * pulse_gain
            * (1. + variation * self.current.cycle_life * 0.32 * idle_factor)
            * (1. + self.transient * self.current.attack * 0.7);
        // The texture is strongest around pressure activity already present in
        // the WAV. No unknown cylinder count or synthetic firing order is used.
        let level = living_pulse.abs();
        let follower = if level > self.pulse_envelope {
            0.0015
        } else {
            0.012
        };
        self.pulse_envelope += (level - self.pulse_envelope) / (self.rate * follower);
        self.pulse_average += (self.pulse_envelope - self.pulse_average) / (self.rate * 0.35);
        let activity = (self.pulse_envelope / self.pulse_average.max(0.002)).clamp(0., 2.5);
        let texture_shape = 1. + self.current.pulse_texture * (activity - 1.) * 0.6;
        // Smooth fuel cutoff above idle. Only the enhanced path is affected; the
        // off-load recording remains audible, retaining mechanical/flow texture.
        let coasting = ((0.1 - self.fast_load) / 0.08).clamp(0., 1.)
            * ((self.p.rpm - bank.min_rpm * 1.35) / 500.).clamp(0., 1.);
        self.cut += (coasting * self.current.fuel_cut - self.cut) / (self.rate * 0.06);
        // During fuel cut, pressure pulses fall much more than the recorded
        // mechanical/flow residual. Suppressing both together sounds like a
        // volume fade rather than an engine being driven by its wheels.
        let source = living_pulse * (1. - self.cut * 0.85)
            + residual
                * texture_gain
                * texture_shape
                * (1. + self.transient * self.current.attack * 1.2)
                * (1. - self.cut * 0.12);
        let low = self.low.next_sample(source);
        let high = self.high.next_sample(source);
        let burst = self.transient * self.current.attack;
        let body = low * self.current.body * (0.18 + self.fast_load * 0.75 + burst * 1.2);
        let rasp = high * self.current.rasp * (0.12 + self.fast_load * 0.65 + burst * 2.);
        // The intake duct receives only the source's irregular texture. Feeding
        // the periodic exhaust waveform into it recreates an exhaust note at the
        // engine emitter, rather than an independently responding intake.
        let air_excitation = inferred_residual
            * texture_gain
            * (0.4 + self.fast_load * 0.35 + self.current.texture * 0.3)
            * (0.7 + activity * 0.15);
        let runner = self.intake_runner.next(air_excitation);
        let air = self
            .intake
            .next_sample(air_excitation + runner * self.current.airbox * 2.);
        let intake = air
            * self.p.intake
            * self.current.maps.intake.at(position, self.fast_load)
            * (0.25 + self.fast_load + burst * 2.);
        let mechanical = (high * 0.13 + residual * texture_gain * 0.08) * self.p.mechanical;
        // Discrete lift-off pressure events excite the same exhaust network.
        // Refractory time avoids clusters becoming continuous white-noise hiss.
        self.crackle_age += 1. / self.rate;
        if lift > 0.035 && self.fast_load < 0.25 && self.p.rpm > 1200. {
            self.pop_clock += 28. * lift / self.rate;
        } else {
            self.pop_clock = 0.;
        }
        if self.pop_clock >= self.pop_interval && self.crackle_age > 0.055 {
            self.pop_clock -= self.pop_interval;
            self.pop_interval = 0.7 + (noise + 1.) * 0.35;
            self.crackle = lift * self.current.overrun * 0.35 * (1. - self.cut * 0.7);
            self.crackle_age = 0.;
            self.crackle_phase = 0.;
            self.crackle_frequency = 95. + self.p.rpm * 0.018;
        }
        self.crackle *= 1. - 1. / (self.rate * 0.035);
        self.crackle_phase = (self.crackle_phase + self.crackle_frequency / self.rate).fract();
        let overrun = self.pop_filter.next_sample(
            self.crackle * (0.8 * (self.crackle_phase * std::f32::consts::TAU).sin() + noise * 0.2),
        );
        let event_target = if self.h.combustion.cylinders == 0 {
            0.
        } else {
            self.h.combustion.amount
        };
        self.event_gain += (event_target - self.event_gain) / (self.rate * 0.04);
        let events = self
            .h
            .combustion
            .pressure(self.wet_cycle, self.p.rpm, self.fast_load)
            * self.event_gain
            * (1. - self.cut);
        let excitation = (source + body + rasp) * (1. + burst * 0.5) + overrun + events;
        let propagated = self.exhaust_line.next(excitation);
        // Even a restrained Pipe setting must let the modeled outlet matter.
        // Previously a calibrated value near 0.04 sent about 97% of the
        // excitation straight to the output, leaving the acoustic network
        // almost inaudible.
        let acoustic_mix = 0.5 + self.current.pipe * 0.4;
        let exhaust = self
            .muffler
            .next_sample(excitation * (1. - acoustic_mix * 0.65) + propagated * acoustic_mix * 1.3);
        self.boost +=
            (self.fast_load * (self.p.rpm / 5000.).min(1.) - self.boost) / (self.rate * 0.4);
        self.turbo_phase = (self.turbo_phase + (1700. + self.boost * 4300.) / self.rate).fract();
        let turbo = self.current.turbo
            * (0.009 * self.boost * (self.turbo_phase * std::f32::consts::TAU).sin()
                + lift * noise * 0.03);
        let shaped =
            exhaust * self.p.exhaust * self.current.maps.exhaust.at(position, self.fast_load)
                + intake
                + mechanical
                + turbo;
        // This emitter has no independent intake microphone. Use a warm,
        // bounded band of the source residual. The old one-cycle subtraction
        // made regularly spaced comb notches and a hollow artificial timbre.
        let upper_residual = self.engine_texture.next_sample(inferred_residual);
        let engine_texture = self.engine_air_band.next_sample(upper_residual);
        self.mechanical_source_energy +=
            (engine_texture * engine_texture - self.mechanical_source_energy) / (self.rate * 0.06);
        let engine_air = engine_texture * self.p.intake * (0.7 + 0.3 * self.fast_load);
        let impacts = self.mechanical_impacts.next(
            self.wet_cycle,
            self.mechanical_source_energy.max(0.).sqrt() * self.p.mechanical * 4.,
            self.fast_load,
        );
        let engine_channel = self
            .engine_stem_dc
            .next_sample((engine_air + impacts + (intake + turbo) * 0.1) * self.current.coloration);
        // Added coloration remains exactly off at zero, but a calibrated middle
        // value now favors the reconstructed path over the source reference.
        let path_gain = 1. - (1. - self.current.coloration).powi(3);
        let mixed_engine = self
            .engine_dc
            .next_sample((intake + mechanical + turbo) * path_gain);
        // The original recording is the A reference and a small B anchor at
        // middle settings. A calibrated coloration near 0.6 leaves only about
        // six percent of that dry waveform in B.
        let wet = self
            .dc
            .next_sample(shaped * path_gain + raw * (1. - path_gain));
        let energy_smooth = 1. / (self.rate * 1.5);
        self.raw_energy += (raw * raw - self.raw_energy) * energy_smooth;
        self.wet_energy += (wet * wet - self.wet_energy) * energy_smooth;
        if self.tick.is_multiple_of(64) {
            let target = if self.h.level_match {
                (self.raw_energy / self.wet_energy.max(1e-9))
                    .sqrt()
                    .clamp(0.4, 2.)
            } else {
                1.
            };
            self.compensation += (target - self.compensation) * (64. / (self.rate * 0.2));
        }
        let out = self
            .transition_level
            .mix(raw, wet * self.compensation, self.blend, self.rate)
            * self.gain;
        // Safety ceiling only: normal operating levels do not hit this branch.
        let mixed = if out.abs() > 0.95 {
            out.signum() * (0.95 + 0.049 * ((out.abs() - 0.95) / 0.049).tanh())
        } else {
            out
        };
        let engine = engine_channel * self.compensation * self.blend * self.gain;
        HybridStems {
            exhaust: mixed - mixed_engine * self.compensation * self.blend * self.gain,
            engine,
            source_reference: raw * bank.original_to_processed_gain() * self.gain,
            mixed,
        }
    }
}

/// Repeatable audition: idle, progressive load, acceleration, release, reapplication.
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
