//! Source-independent engine rendering from bounded, import-time descriptors.
//!
//! The Automation PCM is analyzed when `ProceduralBank` is built, then kept out
//! of `ProceduralVoice`. The latter receives only levels and coarse spectral
//! shapes. It is an exhaust-guided approximation, not an identified intake or
//! a reconstruction of the real engine's firing order.

use crate::bank::{Bank, Sample};
use bdsp::svf::{StateVariableFilter, SvfMode};
use std::{f32::consts::PI, sync::Arc};

// Keep the bass above BeamNG's 80 Hz exhaust cut distinct from the firing
// fundamental below it. A single 100-350 Hz band obscured this distinction.
const BANDS: usize = 7;
const LOW_ORDERS: usize = 4;
const PHASE_BINS: usize = 256;
const PULSE_BINS: usize = 512;
const PRESSURE_BODY: f32 = 1.5;
const FREQS: [f32; BANDS - 1] = [80., 200., 500., 1200., 2500., 5000.];

#[derive(Clone, Copy)]
struct Descriptor {
    rpm: f32,
    source_rms: f32,
    pulse_width_ms: f32,
    bass_share: f32,
    pulse_level: f32,
    tone_gain: [f32; BANDS],
    order_correction: [(f32, f32); LOW_ORDERS],
    noise_color: [f32; BANDS],
    flow_level: f32,
    transient_level: f32,
    event_variation: f32,
}

impl Descriptor {
    fn blend(a: Self, b: Self, t: f32) -> Self {
        let mix = |x: f32, y: f32| x + (y - x) * t;
        let mut out = a;
        out.source_rms = mix(a.source_rms, b.source_rms);
        out.pulse_width_ms = mix(a.pulse_width_ms, b.pulse_width_ms);
        out.bass_share = mix(a.bass_share, b.bass_share);
        out.pulse_level = mix(a.pulse_level, b.pulse_level);
        out.flow_level = mix(a.flow_level, b.flow_level);
        out.transient_level = mix(a.transient_level, b.transient_level);
        out.event_variation = mix(a.event_variation, b.event_variation);
        for i in 0..BANDS {
            out.tone_gain[i] = mix(a.tone_gain[i], b.tone_gain[i]);
            out.noise_color[i] = mix(a.noise_color[i], b.noise_color[i]);
        }
        for i in 0..LOW_ORDERS {
            out.order_correction[i] = (
                mix(a.order_correction[i].0, b.order_correction[i].0),
                mix(a.order_correction[i].1, b.order_correction[i].1),
            );
        }
        out
    }
}

/// Scalar and coarse-band measurements of the imported exhaust bank. No PCM is
/// retained here, so a ProceduralVoice cannot replay the Automation waveform.
pub struct ProceduralBank {
    layers: [Vec<Descriptor>; 2],
}

impl ProceduralBank {
    pub fn from_bank(bank: &Bank) -> Self {
        let cylinders = bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders);
        let (lut, shape_rms) = pulse_table();
        let layers = std::array::from_fn(|load| {
            bank.layers[load]
                .iter()
                .map(|sample| {
                    analyze_sample(sample, load as f32, cylinders, bank.gain, &lut, shape_rms)
                })
                .collect()
        });
        Self { layers }
    }

    fn at(&self, rpm: f32, load: f32) -> Descriptor {
        let layer = |descriptors: &Vec<Descriptor>| {
            let upper = descriptors
                .partition_point(|point| point.rpm < rpm)
                .min(descriptors.len() - 1);
            let lower = upper.saturating_sub(1);
            if upper == lower {
                return descriptors[upper];
            }
            let t = ((rpm - descriptors[lower].rpm)
                / (descriptors[upper].rpm - descriptors[lower].rpm))
                .clamp(0., 1.);
            Descriptor::blend(
                descriptors[lower],
                descriptors[upper],
                t * t * (3. - 2. * t),
            )
        };
        Descriptor::blend(
            layer(&self.layers[0]),
            layer(&self.layers[1]),
            load.clamp(0., 1.),
        )
    }

    /// AC RMS of the imported WAV at the processed Bank gain. This is a level
    /// reference for diagnostics, not an audio sample read by the voice.
    pub fn mean_source_rms(&self, rpm: f32, load: f32) -> f32 {
        self.at(rpm, load).source_rms
    }
}

struct BroadBands {
    pole: [f32; BANDS - 1],
    state: [f32; BANDS - 1],
}

impl BroadBands {
    fn new(rate: f32) -> Self {
        Self {
            pole: FREQS.map(|freq| (-2. * PI * freq.min(rate * 0.42) / rate).exp()),
            state: [0.; BANDS - 1],
        }
    }

    fn next(&mut self, sample: f32) -> [f32; BANDS] {
        for i in 0..BANDS - 1 {
            self.state[i] = sample + self.pole[i] * (self.state[i] - sample);
        }
        std::array::from_fn(|i| match i {
            0 => self.state[0],
            i if i == BANDS - 1 => sample - self.state[BANDS - 2],
            i => self.state[i] - self.state[i - 1],
        })
    }
}

fn pulse_table() -> ([f32; PULSE_BINS], f32) {
    let table = std::array::from_fn(|i| {
        let u = i as f32 / (PULSE_BINS - 1) as f32;
        let window = (PI * u).sin().powi(2);
        // A short pressure edge plus a broader mass-flow body. The latter is
        // de-meaned over the firing interval by `pulse_value`, keeping the
        // fundamental audible without a DC-heavy or metallic narrow pulse.
        window * ((2. * PI * u).sin() + PRESSURE_BODY)
    });
    let rms = (table.iter().map(|x| x * x).sum::<f32>() / PULSE_BINS as f32).sqrt();
    (table, rms.max(1e-6))
}

#[derive(Clone, Copy)]
struct PulseEvent {
    gain: f32,
    delay: f32,
}

fn pulse_value(
    cycle: f64,
    rpm: f32,
    cylinders: u32,
    width_ms: f32,
    table: &[f32; PULSE_BINS],
    shape_rms: f32,
    event: PulseEvent,
) -> f32 {
    let cylinders = cylinders.clamp(1, 12);
    let event_phase = (cycle * cylinders as f64).rem_euclid(1.) as f32 - event.delay;
    let width = (width_ms * 0.001 * rpm * cylinders as f32 / 120.).clamp(0.025, 0.82);
    let body_mean = PRESSURE_BODY * 0.5 * width;
    let normalizer = (width * shape_rms * shape_rms - body_mean * body_mean)
        .max(1e-7)
        .sqrt();
    if !(0. ..width).contains(&event_phase) {
        return -body_mean / normalizer;
    }
    let position = event_phase / width * (PULSE_BINS - 1) as f32;
    let index = position as usize;
    let frac = position - index as f32;
    let sample = table[index] * (1. - frac) + table[(index + 1).min(PULSE_BINS - 1)] * frac;
    (sample * event.gain - body_mean) / normalizer
}

struct SourceAnalysis {
    tonal_bands: [f32; BANDS],
    tonal_rms: f32,
    order_amplitudes: [f32; LOW_ORDERS],
    residual_bands: [f32; BANDS],
}

fn source_bands(
    pcm: &[f32],
    rate: u32,
    analysis_cycles: usize,
    cylinders: u32,
    accepted: bool,
) -> SourceAnalysis {
    let cycles = analysis_cycles.clamp(1, 8);
    let cycle_len = pcm.len() as f64 / analysis_cycles.max(1) as f64;
    let frames = ((cycles as f64 * cycle_len).floor() as usize).min(pcm.len());
    let mut phase_template = [0.; PHASE_BINS];
    if accepted {
        for (bin, value) in phase_template.iter_mut().enumerate() {
            for cycle in 0..cycles {
                let pos = (cycle as f64 + bin as f64 / PHASE_BINS as f64) * cycle_len;
                let index = pos as usize;
                let frac = (pos - index as f64) as f32;
                let next = (index + 1).min(pcm.len() - 1);
                *value += pcm[index] * (1. - frac) + pcm[next] * frac;
            }
            *value /= cycles as f32;
        }
    }
    // Magnitudes only: the imported phase and waveform are deliberately not
    // stored. The model retains its own pressure-pulse phase while matching
    // only the first few firing-order strengths.
    let order_amplitudes = std::array::from_fn(|order| {
        if !accepted {
            return 0.;
        }
        let harmonic = (order + 1) as f32 * cylinders.clamp(1, 12) as f32;
        let (cosine, sine) =
            phase_template
                .iter()
                .enumerate()
                .fold((0., 0.), |(cosine, sine), (index, value)| {
                    let angle = 2. * PI * harmonic * index as f32 / PHASE_BINS as f32;
                    (cosine + value * angle.cos(), sine + value * angle.sin())
                });
        cosine.hypot(sine) * 2. / PHASE_BINS as f32
    });
    let mut tonal_filter = BroadBands::new(rate as f32);
    let mut residual_filter = BroadBands::new(rate as f32);
    let mut tonal_energy = [0.; BANDS];
    let mut residual_energy = [0.; BANDS];
    let mut tonal_total = 0.;
    let skip = (rate as usize / 40).min(frames / 4);
    for (i, &raw) in pcm[..frames].iter().enumerate() {
        let phase = (i as f64 / cycle_len).fract() as f32 * PHASE_BINS as f32;
        let bin = phase as usize % PHASE_BINS;
        let frac = phase - bin as f32;
        let periodic =
            phase_template[bin] * (1. - frac) + phase_template[(bin + 1) % PHASE_BINS] * frac;
        let tonal = tonal_filter.next(periodic);
        let residual = residual_filter.next(raw - periodic);
        if i >= skip {
            tonal_total += periodic * periodic;
            for band in 0..BANDS {
                tonal_energy[band] += tonal[band] * tonal[band];
                residual_energy[band] += residual[band] * residual[band];
            }
        }
    }
    let denom = (frames - skip).max(1) as f32;
    SourceAnalysis {
        tonal_bands: tonal_energy.map(|x| (x / denom).sqrt()),
        tonal_rms: (tonal_total / denom).sqrt(),
        order_amplitudes,
        residual_bands: residual_energy.map(|x| (x / denom).sqrt()),
    }
}

// A short import-time power-spectrum measurement distinguishes a raspy
// exhaust with strong high-frequency turbulence from one whose upper residual
// is weak. The result is one scalar per WAV; no transform bins or PCM enter the
// real-time voice.
fn source_spectral_shares(pcm: &[f32], rate: u32) -> (f32, f32) {
    const FFT: usize = 4096;
    if pcm.len() < FFT {
        return (0.6, 0.05);
    }
    let window: [f32; FFT] =
        std::array::from_fn(|i| 0.5 - 0.5 * (2. * PI * i as f32 / (FFT - 1) as f32).cos());
    let mut spectrum = vec![(0., 0.); FFT];
    let mut audible = 0.;
    let mut bass = 0.;
    let mut high = 0.;
    for start in (0..=pcm.len() - FFT).step_by(FFT / 2).take(24) {
        for i in 0..FFT {
            spectrum[i] = (pcm[start + i] * window[i], 0.);
        }
        let mut reversed = 0;
        for i in 1..FFT {
            let mut bit = FFT >> 1;
            while reversed & bit != 0 {
                reversed ^= bit;
                bit >>= 1;
            }
            reversed ^= bit;
            if i < reversed {
                spectrum.swap(i, reversed);
            }
        }
        let mut length = 2;
        while length <= FFT {
            let (step_sine, step_cosine) = (-2. * PI / length as f32).sin_cos();
            for base in (0..FFT).step_by(length) {
                let (mut cosine, mut sine) = (1., 0.);
                for offset in 0..length / 2 {
                    let (ar, ai) = spectrum[base + offset];
                    let (br, bi) = spectrum[base + offset + length / 2];
                    let tr = cosine * br - sine * bi;
                    let ti = sine * br + cosine * bi;
                    spectrum[base + offset] = (ar + tr, ai + ti);
                    spectrum[base + offset + length / 2] = (ar - tr, ai - ti);
                    let next_cosine = cosine * step_cosine - sine * step_sine;
                    sine = sine * step_cosine + cosine * step_sine;
                    cosine = next_cosine;
                }
            }
            length *= 2;
        }
        for (bin, (real, imag)) in spectrum.iter().enumerate().take(FFT / 2).skip(1) {
            let frequency = bin as f32 * rate as f32 / FFT as f32;
            let power = real * real + imag * imag;
            if (80. ..10_000.).contains(&frequency) {
                audible += power;
                if frequency < 500. {
                    bass += power;
                }
                if (2000. ..5000.).contains(&frequency) {
                    high += power;
                }
            }
        }
    }
    (
        (bass / audible.max(1e-12)).clamp(0., 1.),
        (high / audible.max(1e-12)).clamp(0., 1.),
    )
}

// Variation of whole engine cycles is measured at import time. The residual
// ratio supplies a conservative floor when a source WAV contains only one
// cycle, which cannot reveal cycle-to-cycle motion by itself.
fn source_cycle_variation(pcm: &[f32], cycles: usize) -> f32 {
    if cycles < 2 || pcm.len() < cycles * 128 {
        return 0.;
    }
    let cycle_len = pcm.len() / cycles;
    let mut levels = [0.; 8];
    let count = cycles.min(levels.len());
    for (i, level) in levels.iter_mut().enumerate().take(count) {
        let start = i * cycle_len;
        let end = start + cycle_len;
        let energy = pcm[start..end].iter().map(|x| x * x).sum::<f32>();
        *level = (energy / cycle_len as f32).sqrt();
    }
    let mean = levels[..count].iter().sum::<f32>() / count as f32;
    let variance = levels[..count]
        .iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f32>()
        / count as f32;
    (variance.sqrt() / mean.max(1e-6)).clamp(0., 0.2)
}

fn pressure_duration_scale(bass_share: f32) -> f32 {
    (1. + 3.2 * (bass_share - 0.58).max(0.)).clamp(1., 2.4)
}

fn light_load_pressure_scale(bass_share: f32, load: f32) -> f32 {
    let moderate_bass = ((0.9 - bass_share) / 0.2).clamp(0., 1.);
    1. - 0.35 * (1. - load.clamp(0., 1.)).powi(2) * moderate_bass
}

fn full_load_pressure_scale(bass_share: f32, load: f32) -> f32 {
    let mid_heavy = ((0.64 - bass_share) / 0.12).clamp(0., 1.);
    1. + 0.3 * load.clamp(0., 1.).powi(8) * mid_heavy
}

fn full_load_mid_gain(bass_share: f32, load: f32) -> f32 {
    let mid_heavy = ((0.64 - bass_share) / 0.12).clamp(0., 1.);
    1. - 0.55 * load.clamp(0., 1.).powi(8) * mid_heavy
}

fn pulse_bands(
    rpm: f32,
    rate: f32,
    cylinders: u32,
    width_ms: f32,
    table: &[f32; PULSE_BINS],
    shape_rms: f32,
    tone_gain: [f32; BANDS],
) -> ([f32; BANDS], f32, [(f32, f32); LOW_ORDERS]) {
    let cycles = 6.;
    let frames = (cycles * 120. * rate / rpm).ceil() as usize;
    let mut bands = BroadBands::new(rate);
    let mut energy = [0.; BANDS];
    let mut mixed_energy = 0.;
    let mut orders = [(0., 0.); LOW_ORDERS];
    let skip = (rate as usize / 40).min(frames / 4);
    for i in 0..frames {
        let cycle = i as f64 * rpm as f64 / (120. * rate as f64);
        let pulse = pulse_value(
            cycle,
            rpm,
            cylinders,
            width_ms,
            table,
            shape_rms,
            PulseEvent {
                gain: 1.,
                delay: 0.,
            },
        );
        let split = bands.next(pulse);
        if i >= skip {
            let mut mixed = 0.;
            for band in 0..BANDS {
                energy[band] += split[band] * split[band];
                mixed += split[band] * tone_gain[band];
            }
            mixed_energy += mixed * mixed;
            let event_phase = (cycle * cylinders.clamp(1, 12) as f64).fract() as f32;
            for (index, (cosine, sine)) in orders.iter_mut().enumerate() {
                let angle = 2. * PI * (index + 1) as f32 * event_phase;
                *cosine += mixed * angle.cos();
                *sine += mixed * angle.sin();
            }
        }
    }
    let denom = (frames - skip).max(1) as f32;
    for (cosine, sine) in &mut orders {
        *cosine *= 2. / denom;
        *sine *= 2. / denom;
    }
    (
        energy.map(|x| (x / denom).sqrt()),
        (mixed_energy / denom).sqrt(),
        orders,
    )
}

fn analyze_sample(
    sample: &Sample,
    load: f32,
    cylinders: u32,
    bank_gain: f32,
    table: &[f32; PULSE_BINS],
    shape_rms: f32,
) -> Descriptor {
    let source_rms = (sample.rms * bank_gain).max(1e-6);
    let analysis = source_bands(
        sample.analysis_pcm(),
        sample.rate,
        sample.analysis_cycles(),
        cylinders,
        sample.period.accepted,
    );
    let crest = (sample.peak / sample.rms.max(1e-6)).clamp(1., 12.);
    let (bass_share, high_share) = source_spectral_shares(sample.analysis_pcm(), sample.rate);
    // Recordings specify broad timbre and level. The exact source cycle is not
    // copied. A bass-dominant reference calls for a longer pressure release,
    // suppressing the artificial row of upper harmonics from a narrow pulse.
    let pulse_width_ms = (1.8 - (crest - 3.) * 0.09).clamp(1.1, 2.1);
    let fitted_width_ms = (pulse_width_ms * pressure_duration_scale(bass_share)).min(4.8);
    let synth_cylinders = cylinders.clamp(1, 12);
    let (model_bands, _, _) = pulse_bands(
        sample.rpm,
        48_000.,
        synth_cylinders,
        fitted_width_ms,
        table,
        shape_rms,
        [1.; BANDS],
    );
    let tonal_total = analysis
        .tonal_bands
        .iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt()
        .max(1e-9);
    let model_total = model_bands
        .iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt()
        .max(1e-9);
    let raw_gain: [f32; BANDS] = if sample.period.accepted && analysis.tonal_rms > 1e-6 {
        std::array::from_fn(|i| {
            ((analysis.tonal_bands[i] / tonal_total) / (model_bands[i] / model_total).max(1e-4))
                .clamp(0.12, 8.)
        })
    } else {
        // A rejected period cannot distinguish a tonal engine order from
        // source noise. Never fit a periodic synth to its raw spectrum.
        [1.15, 1.12, 1.08, 0.96, 0.82, 0.68, 0.5]
    };
    // Smooth adjacent broad bands so isolated source peaks do not become
    // narrow resonators or a hard metallic spectral edge.
    let mut tone_gain: [f32; BANDS] = std::array::from_fn(|i| {
        let left = raw_gain[i.saturating_sub(1)];
        let right = raw_gain[(i + 1).min(BANDS - 1)];
        (raw_gain[i] * 0.72 + (left + right) * 0.14).clamp(0.15, 7.)
    });
    // The fixed-width pressure release has too much 350–1200 Hz energy when
    // an off-load source is dominated by its lower firing orders. A modest
    // off-load shelf keeps the coarse band fit from sounding nasal at cruise.
    let off_load = 1. - load;
    tone_gain[0] *= 1. + 0.25 * off_load;
    tone_gain[1] *= 1. + 0.35 * off_load;
    tone_gain[2] *= 1. + 0.15 * off_load;
    tone_gain[3] *= 1. - 0.18 * off_load;
    // The broad-band pulse fit systematically overstates 500-2000 Hz when
    // the reference is dominated by firing orders below 500 Hz. Use the
    // source's audible bass share to constrain this ratio at every knot.
    let bass_weight = (1. / (1. - bass_share).max(0.015))
        .powf(0.35)
        .clamp(1., 4.5);
    tone_gain[1] *= bass_weight;
    tone_gain[2] *= bass_weight;
    let (_, mixed_rms, model_orders) = pulse_bands(
        sample.rpm,
        48_000.,
        synth_cylinders,
        fitted_width_ms,
        table,
        shape_rms,
        tone_gain,
    );
    let residual_total = analysis
        .residual_bands
        .iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt();
    let mut noise_color = if sample.period.accepted && residual_total > 1e-9 {
        analysis.residual_bands.map(|x| x / residual_total)
    } else {
        [0.02, 0.08, 0.22, 0.48, 0.62, 0.6, 0.38]
    };
    // The source recording is exhaust-only. Its residual gives a conservative
    // coloration cue, never evidence for a measured intake or a loud hiss.
    noise_color[0] *= 0.3;
    let color_norm = noise_color
        .iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt()
        .max(1e-6);
    for color in &mut noise_color {
        *color /= color_norm;
    }
    // The non-periodic part of a high-load exhaust can carry most of its
    // audible rasp, especially when bass below the game's 80 Hz cutoff is
    // strong. Match its measured broad-band energy with independently drawn
    // noise rather than forcing it into phase-locked cylinder harmonics.
    let texture_fit = (high_share / 0.057).powf(0.85).clamp(0.3, 3.5);
    let stochastic_fit = (high_share / 0.09).powf(0.45).clamp(0.55, 1.7);
    let flow_level =
        (residual_total * bank_gain * 2.2 * texture_fit * stochastic_fit).min(source_rms * 1.7);
    let transient_level = ((analysis.residual_bands[4] * 1.1
        + analysis.residual_bands[5] * 1.1
        + analysis.residual_bands[6] * 0.2)
        * bank_gain)
        .min(source_rms * 0.28)
        * stochastic_fit;
    let residual_fraction = residual_total / (analysis.tonal_rms + residual_total).max(1e-6);
    let cycle_motion = source_cycle_variation(sample.analysis_pcm(), sample.analysis_cycles());
    // Whole-cycle source motion is a stronger clue for pressure irregularity
    // than the residual spectrum alone. Preserve it in the independent pulse
    // generator instead of repeating a nearly identical pressure event.
    let event_variation = (0.04 + residual_fraction * 0.05 + cycle_motion * 1.25).clamp(0.04, 0.18);
    let tonal_level = if sample.period.accepted {
        (analysis.tonal_rms * bank_gain).max(source_rms * 0.25)
    } else {
        source_rms * 0.5
    };
    let pulse_level = tonal_level / mixed_rms.max(1e-6);
    let order_correction = std::array::from_fn(|index| {
        if !sample.period.accepted {
            return (0., 0.);
        }
        let (cosine, sine) = model_orders[index];
        let model_amplitude = cosine.hypot(sine) * pulse_level;
        if model_amplitude < source_rms * 0.002 {
            return (0., 0.);
        }
        let target_amplitude = analysis.order_amplitudes[index] * bank_gain;
        let multiplier = (target_amplitude / model_amplitude).clamp(0.02, 2.);
        (
            cosine * pulse_level * (multiplier - 1.),
            sine * pulse_level * (multiplier - 1.),
        )
    });
    Descriptor {
        rpm: sample.rpm,
        source_rms,
        pulse_width_ms,
        bass_share,
        pulse_level,
        tone_gain,
        order_correction,
        noise_color,
        flow_level,
        transient_level,
        event_variation,
    }
}

fn xorshift(seed: &mut u64) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed >> 40) as f32 / 16_777_215. * 2. - 1.
}

fn white_band_reference(rate: f32) -> [f32; BANDS] {
    let mut bands = BroadBands::new(rate);
    let mut seed = 0xBA5E_1EAF_CAFE_139Du64;
    let mut energy = [0.; BANDS];
    // A bounded calibration at the actual device rate. This is constructed
    // with the voice and never runs inside its sample callback.
    let frames = 16_384;
    for i in 0..frames {
        let split = bands.next(xorshift(&mut seed));
        if i >= frames / 10 {
            for band in 0..BANDS {
                energy[band] += split[band] * split[band];
            }
        }
    }
    energy.map(|x| (x / (frames - frames / 10) as f32).sqrt().max(1e-5))
}

/// Runs only on measured descriptors. The two exhaust components can be
/// balanced independently without recovering the imported PCM waveform.
/// Intake and mechanical detail remain estimated, independently excited paths.
#[derive(Clone, Copy, Debug)]
pub struct ProceduralStems {
    pub exhaust_tone: f32,
    pub exhaust_texture: f32,
    pub intake: f32,
    pub mechanical: f32,
}

pub struct ProceduralVoice {
    descriptors: Arc<ProceduralBank>,
    white_band_rms: [f32; BANDS],
    cylinders: u32,
    pulse_table: [f32; PULSE_BINS],
    pulse_rms: f32,
    last_event: i64,
    event_gain: f32,
    event_delay: f32,
    event_width: f32,
    cylinder_gain: [f32; 12],
    event_seed: u64,
    flow_seed: u64,
    transient_seed: u64,
    engine_seed: u64,
    mechanical_seed: u64,
    last_mechanical_event: i64,
    mechanical_envelope: f32,
    mechanical_decay: f32,
    flow_envelope: f32,
    flow_attack: f32,
    flow_release: f32,
    tone_bands: BroadBands,
    flow_bands: BroadBands,
    transient_bands: BroadBands,
    engine_bands: BroadBands,
    mechanical_bands: BroadBands,
    flow_warm: StateVariableFilter,
    intake_warm: StateVariableFilter,
    mechanical_warm: StateVariableFilter,
    low_resonator: StateVariableFilter,
    mid_resonator: StateVariableFilter,
}

impl ProceduralVoice {
    pub fn new(rate: f32, descriptors: Arc<ProceduralBank>, cylinders: u32) -> Self {
        let rate = rate.max(8000.);
        let (pulse_table, pulse_rms) = pulse_table();
        let mut cylinder_seed = 0x9C17_83D5_26A0_F21Bu64;
        let cylinder_gain = std::array::from_fn(|_| 1. + 0.035 * xorshift(&mut cylinder_seed));
        Self {
            descriptors,
            white_band_rms: white_band_reference(rate),
            cylinders: cylinders.clamp(1, 12),
            pulse_table,
            pulse_rms,
            last_event: i64::MIN,
            event_gain: 1.,
            event_delay: 0.,
            event_width: 1.,
            cylinder_gain,
            event_seed: 0xA19E_5C45_21DA_7777,
            flow_seed: 0xF10F_8A7E_4251_1055,
            transient_seed: 0xC04B_0571_0AEE_2175,
            engine_seed: 0xE11E_515E_4A41_1010,
            mechanical_seed: 0x4EEC_AA15_3C75_474B,
            last_mechanical_event: i64::MIN,
            mechanical_envelope: 0.,
            mechanical_decay: (-1. / (rate * 0.0025)).exp(),
            flow_envelope: 0.,
            flow_attack: 1. - (-1. / (rate * 0.003)).exp(),
            flow_release: 1. - (-1. / (rate * 0.012)).exp(),
            tone_bands: BroadBands::new(rate),
            flow_bands: BroadBands::new(rate),
            transient_bands: BroadBands::new(rate),
            engine_bands: BroadBands::new(rate),
            mechanical_bands: BroadBands::new(rate),
            flow_warm: StateVariableFilter::new(rate, 2200., 0.707, SvfMode::Lowpass),
            intake_warm: StateVariableFilter::new(rate, 950., 0.707, SvfMode::Lowpass),
            mechanical_warm: StateVariableFilter::new(rate, 1300., 0.707, SvfMode::Lowpass),
            // Low Q and restrained wet gain bound metallic ringing.
            low_resonator: StateVariableFilter::new(rate, 260., 0.65, SvfMode::Bandpass),
            mid_resonator: StateVariableFilter::new(rate, 760., 0.65, SvfMode::Bandpass),
        }
    }

    pub fn set_cylinders(&mut self, cylinders: u32) {
        self.cylinders = cylinders.clamp(1, 12);
    }

    pub fn mean_source_rms(&self, rpm: f32, load: f32) -> f32 {
        self.descriptors.mean_source_rms(rpm, load)
    }

    pub fn next(&mut self, rpm: f32, load: f32, cycle: f64) -> ProceduralStems {
        let rpm = rpm.clamp(200., 20_000.);
        let load = load.clamp(0., 1.);
        let descriptor = self.descriptors.at(rpm, load);
        let event = (cycle * self.cylinders as f64).floor() as i64;
        if event != self.last_event {
            self.last_event = event;
            // Stable cylinder-to-cylinder imbalance combines with bounded
            // cycle variation. A delayed onset changes timing without moving
            // the global crank phase or discontinuously cutting off a pulse.
            let cylinder = event.rem_euclid(self.cylinders as i64) as usize;
            self.event_gain = self.cylinder_gain[cylinder]
                * (1. + xorshift(&mut self.event_seed) * descriptor.event_variation);
            self.event_delay = (0.012 + xorshift(&mut self.event_seed) * 0.012).max(0.);
            self.event_width = 1. + xorshift(&mut self.event_seed) * 0.06;
        }
        let pulse = pulse_value(
            cycle,
            rpm,
            self.cylinders,
            (descriptor.pulse_width_ms
                * pressure_duration_scale(descriptor.bass_share)
                * light_load_pressure_scale(descriptor.bass_share, load)
                * full_load_pressure_scale(descriptor.bass_share, load))
            .min(4.8)
                * self.event_width,
            &self.pulse_table,
            self.pulse_rms,
            PulseEvent {
                gain: self.event_gain,
                delay: self.event_delay,
            },
        );
        let tone = self.tone_bands.next(pulse);
        let mid_gain = full_load_mid_gain(descriptor.bass_share, load);
        let mut tonal = tone
            .iter()
            .zip(descriptor.tone_gain)
            .enumerate()
            .map(|(i, (band, gain))| band * gain * if i >= 3 { mid_gain } else { 1. })
            .sum::<f32>()
            * descriptor.pulse_level;
        let event_phase = (cycle * self.cylinders as f64).rem_euclid(1.) as f32;
        let (base_sine, base_cosine) = (2. * PI * event_phase).sin_cos();
        let (mut sine, mut cosine) = (base_sine, base_cosine);
        for (correction_cosine, correction_sine) in descriptor.order_correction {
            tonal += correction_cosine * cosine + correction_sine * sine;
            let next_cosine = cosine * base_cosine - sine * base_sine;
            sine = sine * base_cosine + cosine * base_sine;
            cosine = next_cosine;
        }
        let resonant = tonal
            + self.low_resonator.next_sample(tonal) * 0.075
            + self.mid_resonator.next_sample(tonal) * 0.04;

        let flow = self.flow_bands.next(xorshift(&mut self.flow_seed));
        let mut colored_flow = 0.;
        for (i, band) in flow.iter().enumerate() {
            colored_flow += band * descriptor.noise_color[i] / self.white_band_rms[i];
        }
        let pressure_activity = (pulse.abs() * 0.35).min(1.);
        let follow = if pressure_activity > self.flow_envelope {
            self.flow_attack
        } else {
            self.flow_release
        };
        self.flow_envelope += (pressure_activity - self.flow_envelope) * follow;
        // Airflow follows pressure events. A large load-only floor made a
        // continuous hiss during acceleration even between those events.
        let flow_gate = (0.012 + load * (0.035 + 0.75 * self.flow_envelope)).min(1.);
        let burst_gate = (pulse.abs() * 0.75).min(1.);
        let transient_noise = self
            .transient_bands
            .next(xorshift(&mut self.transient_seed));
        let combustion_texture = (transient_noise[4] / self.white_band_rms[4] * 0.45
            + transient_noise[5] / self.white_band_rms[5] * 0.6
            + transient_noise[6] / self.white_band_rms[6] * 0.1)
            * descriptor.transient_level
            * burst_gate
            * (0.55 + 0.45 * load);
        let exhaust_texture =
            self.flow_warm.next_sample(colored_flow) * descriptor.flow_level * flow_gate
                + combustion_texture;

        let engine_noise = self.engine_bands.next(xorshift(&mut self.engine_seed));
        // Keep valve airflow in the lower bands. Broad upper noise reads as
        // constant electrical hiss when the intake is heard in isolation.
        let air = engine_noise[1] / self.white_band_rms[1] * 0.26
            + engine_noise[2] / self.white_band_rms[2] * 0.32
            + engine_noise[3] / self.white_band_rms[3] * 0.04;
        let valve_phase = (cycle * self.cylinders as f64 + 0.47).rem_euclid(1.) as f32;
        let valve_window = if valve_phase < 0.24 {
            (PI * valve_phase / 0.24).sin().powi(2)
        } else {
            0.
        };
        let intake = self.intake_warm.next_sample(
            descriptor.source_rms
                * air
                * valve_window
                * (0.65 + 0.25 * load)
                * (0.25 + 0.45 * load),
        );

        let mechanical_event = (cycle * self.cylinders as f64 + 0.47).floor() as i64;
        if mechanical_event != self.last_mechanical_event {
            self.last_mechanical_event = mechanical_event;
            self.mechanical_envelope = 0.9 + 0.1 * xorshift(&mut self.mechanical_seed);
        }
        self.mechanical_envelope *= self.mechanical_decay;
        let mechanical_noise = self
            .mechanical_bands
            .next(xorshift(&mut self.mechanical_seed));
        // Every mechanical contribution follows an event. The previous
        // ungated band made a constant noise floor between valve impacts.
        let mechanical = self.mechanical_warm.next_sample(
            descriptor.source_rms
                * (mechanical_noise[1] / self.white_band_rms[1] * 0.12
                    + mechanical_noise[2] / self.white_band_rms[2] * 0.16
                    + mechanical_noise[3] / self.white_band_rms[3] * 0.045)
                * self.mechanical_envelope,
        );
        ProceduralStems {
            exhaust_tone: resonant,
            exhaust_texture,
            intake,
            mechanical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_bank() -> Arc<ProceduralBank> {
        let point = Descriptor {
            rpm: 4000.,
            source_rms: 0.1,
            pulse_width_ms: 1.6,
            bass_share: 0.6,
            pulse_level: 0.1,
            tone_gain: [1.; BANDS],
            order_correction: [(0., 0.); LOW_ORDERS],
            noise_color: [0.02, 0.08, 0.22, 0.48, 0.62, 0.6, 0.38],
            flow_level: 0.004,
            transient_level: 0.012,
            event_variation: 0.09,
        };
        Arc::new(ProceduralBank {
            layers: [vec![point], vec![point]],
        })
    }

    #[test]
    fn broad_band_split_reconstructs_sample() {
        let mut filter = BroadBands::new(48_000.);
        for i in 0..1000 {
            let x = (i as f32 * 0.41).sin();
            let bands = filter.next(x);
            assert!((bands.iter().sum::<f32>() - x).abs() < 1e-6);
        }
    }

    #[test]
    fn pulse_is_smooth_and_has_no_raw_audio_dependency() {
        let (table, rms) = pulse_table();
        assert!(table[0].abs() < 1e-7);
        assert!(table[PULSE_BINS - 1].abs() < 1e-7);
        let mut voice = ProceduralVoice::new(48_000., test_bank(), 6);
        let mut total = 0.;
        for i in 0..48_000 {
            let cycle = i as f64 * 4000. / (120. * 48_000.);
            let stems = voice.next(4000., 0.8, cycle);
            assert!(
                stems.exhaust_tone.is_finite()
                    && stems.exhaust_texture.is_finite()
                    && stems.intake.is_finite()
                    && stems.mechanical.is_finite()
            );
            total += (stems.exhaust_tone + stems.exhaust_texture).powi(2);
        }
        assert!(total > 0.);
        assert!(rms > 0.);
    }

    #[test]
    fn voice_accepts_live_cylinder_changes_without_blowing_up() {
        let mut voice = ProceduralVoice::new(48_000., test_bank(), 4);
        let mut cycle = 0.;
        for cylinders in [4, 6, 12, 1] {
            voice.set_cylinders(cylinders);
            for _ in 0..1000 {
                cycle += 5200. / (120. * 48_000.);
                let stems = voice.next(5200., 1., cycle);
                let exhaust = stems.exhaust_tone + stems.exhaust_texture;
                assert!(
                    exhaust.is_finite() && stems.intake.is_finite() && stems.mechanical.is_finite()
                );
                assert!(
                    exhaust.abs() < 10. && stems.intake.abs() < 10. && stems.mechanical.abs() < 10.
                );
            }
        }
    }

    #[test]
    fn mechanical_noise_dies_away_without_a_new_event() {
        let mut voice = ProceduralVoice::new(48_000., test_bank(), 4);
        let _ = voice.next(4000., 0.8, 0.);
        let mut tail_energy = 0.;
        for i in 0..12_000 {
            let mechanical = voice.next(4000., 0.8, 0.).mechanical;
            if i >= 11_000 {
                tail_energy += mechanical * mechanical;
            }
        }
        assert!(
            tail_energy < 1e-12,
            "ungated mechanical floor: {tail_energy}"
        );
    }

    #[test]
    fn rejected_noise_cannot_become_a_tonal_profile() {
        let mut seed = 0x37AB_3214_7809_0042;
        let noise: Vec<_> = (0..24_000).map(|_| xorshift(&mut seed)).collect();
        let analysis = source_bands(&noise, 48_000, 4, 4, false);
        assert_eq!(analysis.tonal_rms, 0.);
        assert!(analysis.tonal_bands.iter().all(|value| *value == 0.));
        assert!(analysis.residual_bands[3] > 0.);
    }

    #[test]
    fn spectral_descriptor_distinguishes_high_rasp_from_low_orders() {
        let rate = 48_000;
        let low: Vec<_> = (0..8192)
            .map(|i| (2. * PI * 120. * i as f32 / rate as f32).sin())
            .collect();
        let high: Vec<_> = (0..8192)
            .map(|i| (2. * PI * 3000. * i as f32 / rate as f32).sin())
            .collect();
        assert!(source_spectral_shares(&low, rate).1 < 0.01);
        assert!(source_spectral_shares(&high, rate).1 > 0.99);
        assert!(source_spectral_shares(&low, rate).0 > 0.99);
    }

    #[test]
    fn pressure_duration_and_cycle_motion_follow_source_evidence() {
        assert_eq!(pressure_duration_scale(0.4), 1.);
        assert!(pressure_duration_scale(0.98) > pressure_duration_scale(0.7));
        assert!(pressure_duration_scale(1.) <= 2.4);
        assert!(light_load_pressure_scale(0.76, 0.15) < light_load_pressure_scale(0.76, 0.7));
        assert_eq!(full_load_pressure_scale(0.8, 1.), 1.);
        assert!(full_load_pressure_scale(0.52, 1.) > full_load_pressure_scale(0.52, 0.7));
        assert!(full_load_mid_gain(0.52, 1.) < full_load_mid_gain(0.52, 0.7));
        let steady = vec![0.1; 2048];
        let alternating = [vec![0.1; 1024], vec![0.2; 1024]].concat();
        assert!(source_cycle_variation(&steady, 2) < 1e-5);
        assert!(source_cycle_variation(&alternating, 2) > 0.1);
    }

    #[test]
    fn device_rates_keep_stems_finite_and_similarly_scaled() {
        let mut levels = Vec::new();
        for rate in [44_100., 48_000., 96_000., 192_000.] {
            let mut voice = ProceduralVoice::new(rate, test_bank(), 6);
            let frames = rate as usize / 2;
            let mut energy = [0.; 3];
            for i in 0..frames {
                let cycle = i as f64 * 4000. / (120. * rate as f64);
                let stems = voice.next(4000., 1., cycle);
                let exhaust = stems.exhaust_tone + stems.exhaust_texture;
                let (intake, mechanical) = (stems.intake, stems.mechanical);
                for (sum, sample) in energy.iter_mut().zip([exhaust, intake, mechanical]) {
                    assert!(sample.is_finite());
                    *sum += sample * sample;
                }
            }
            let rms = energy.map(|sum| (sum / frames as f32).sqrt());
            assert!(rms.iter().all(|value| *value > 0.));
            levels.push(rms);
        }
        for level in &levels[1..] {
            for (a, b) in level.iter().zip(levels[0]) {
                assert!((a / b).clamp(0., 10.) > 0.35 && a / b < 2., "{levels:?}");
            }
        }
    }
}
