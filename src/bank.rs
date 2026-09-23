//! Immutable Automation assets. All decoding/loop preparation is outside audio.
use bdsp::resample::SincTable;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::Path,
    sync::{Arc, OnceLock},
};

const PAD: usize = 128;
const PROFILE: usize = 256;
const MAX_FILE: u64 = 16_000_000;
const MAX_TOTAL: usize = 24_000_000;
#[derive(Clone, Copy)]
enum ReadMode {
    Original,
    Source,
    Periodic,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SourceRef {
    pub archive: String,
    pub blend: String,
    pub fingerprint: String,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Character {
    pub cycle_similarity: f32,
    pub edge_ratio: f32,
}
pub struct Sample {
    pub rpm: f32,
    pub rate: u32,
    pcm: Vec<f32>,
    frames: usize,
    cycles: usize,
    offset: f64,
    pub rms: f32,
    pub peak: f32,
    pub period: crate::period::Period,
    legacy: Option<Box<Sample>>,
    periodic: Vec<f32>,
}
pub struct Bank {
    pub source: SourceRef,
    pub engine_meta: Option<crate::engine_meta::EngineMeta>,
    pub layers: [Vec<Sample>; 2],
    pub gain: f32,
    original_gain: f32,
    pub min_rpm: f32,
    pub max_rpm: f32,
    procedural: OnceLock<Arc<crate::procedural::ProceduralBank>>,
}
fn read_entry(zip: &mut zip::ZipArchive<File>, name: &str, limit: u64) -> Result<Vec<u8>, String> {
    let entry = zip.by_name(name).map_err(|e| format!("{name} : {e}"))?;
    if entry.size() > limit {
        return Err(format!("File too large: {name}"));
    }
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Decode limit exceeded".into());
    }
    Ok(bytes)
}
pub fn decode_wav(bytes: &[u8]) -> Result<(u32, Vec<f32>), String> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if !(8000..=192000).contains(&spec.sample_rate)
        || !(1..=2).contains(&spec.channels)
        || reader.len() > 4_000_000
    {
        return Err(
            "WAV outside supported limits (8–192 kHz, mono/stereo, 4 million samples)".into(),
        );
    }
    let data: Result<Vec<f32>, _> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().collect()
    } else {
        let scale = 2f32.powi(spec.bits_per_sample as i32 - 1);
        reader
            .samples::<i32>()
            .map(|s| s.map(|n| n as f32 / scale))
            .collect()
    };
    let data = data.map_err(|e| e.to_string())?;
    if data.iter().any(|n| !n.is_finite() || n.abs() > 16.) {
        return Err("WAV contains invalid values".into());
    }
    let mono: Vec<f32> = data
        .chunks_exact(spec.channels as usize)
        .map(|s| s.iter().sum::<f32>() / spec.channels as f32)
        .collect();
    if mono.len() < 512 {
        return Err("WAV is too short".into());
    }
    Ok((spec.sample_rate, mono))
}
impl Sample {
    /// Prepared recording for offline descriptor extraction. The audio thread
    /// must not use this slice for procedural playback.
    pub(crate) fn analysis_pcm(&self) -> &[f32] {
        &self.pcm[PAD..PAD + self.frames]
    }

    pub(crate) fn analysis_cycles(&self) -> usize {
        self.cycles
    }

    fn prepare(rpm: f32, rate: u32, mut raw: Vec<f32>) -> Result<Self, String> {
        let mean = raw.iter().map(|x| *x as f64).sum::<f64>() / raw.len() as f64;
        for x in &mut raw {
            *x -= mean as f32;
        }
        let period = rate as f64 * 120. / rpm as f64;
        let analysis = crate::period::estimate(&raw, period);
        let legacy = Self::prepare_at(rpm, rate, &raw, period, analysis)?;
        // Keep the RPM-labelled cycle length as the common timebase for every
        // neighbouring recording. Independently fitted lengths drift against
        // one another during interpolation and make a held RPM pulse in level.
        let mut sample = Self::prepare_at(rpm, rate, &raw, period, analysis)?;
        sample.prepare_periodic();
        sample.legacy = Some(Box::new(legacy));
        Ok(sample)
    }
    fn prepare_at(
        rpm: f32,
        rate: u32,
        raw: &[f32],
        period: f64,
        analysis: crate::period::Period,
    ) -> Result<Self, String> {
        let available = (raw.len() as f64 / period).floor() as usize;
        if available < 4 {
            return Err("At least four engine cycles are required".into());
        }
        let cycles = available - 2;
        let frames = (period * cycles as f64).round() as usize;
        let overlap = (period * 1.5).round() as usize;
        let mut looped = raw[..frames].to_vec();
        // Tail after the chosen loop point overlaps the head. The loop joins
        // contiguous source samples, with an integer number of 720-degree cycles.
        for i in 0..overlap.min(frames) {
            let w = 0.5 - 0.5 * (std::f32::consts::PI * i as f32 / overlap as f32).cos();
            looped[i] = raw[frames + i] * (1. - w) + raw[i] * w;
        }
        let rms = (looped.iter().map(|s| s * s).sum::<f32>() / frames as f32).sqrt();
        let peak = looped.iter().fold(0f32, |a, s| a.max(s.abs()));
        let mut pcm = Vec::with_capacity(frames + 2 * PAD);
        pcm.extend((0..PAD).map(|i| looped[(frames + i - PAD % frames) % frames]));
        pcm.extend_from_slice(&looped);
        pcm.extend((0..PAD).map(|i| looped[i % frames]));
        Ok(Self {
            rpm,
            rate,
            pcm,
            frames,
            cycles,
            offset: 0.,
            rms,
            peak,
            period: analysis,
            legacy: None,
            periodic: Vec::new(),
        })
    }
    fn prepare_periodic(&mut self) {
        // A cycle-synchronous average, at approximately the original sample
        // density. The complementary residual is defined by subtraction.
        // Unreliable periods keep all source energy in the residual path.
        if !self.period.accepted {
            return;
        }
        let size = (self.frames as f64 / self.cycles as f64).round() as usize;
        let mut template = vec![0.; size];
        for (i, v) in template.iter_mut().enumerate() {
            for c in 0..self.cycles {
                let pos =
                    (c as f64 + i as f64 / size as f64) * self.frames as f64 / self.cycles as f64;
                let j = pos as usize;
                let f = (pos - j as f64) as f32;
                *v += self.pcm[PAD + j] * (1. - f) + self.pcm[PAD + (j + 1) % self.frames] * f;
            }
            *v /= self.cycles as f32;
        }
        self.periodic
            .extend((0..PAD).map(|i| template[(size + i - PAD % size) % size]));
        self.periodic.extend_from_slice(&template);
        self.periodic.extend((0..PAD).map(|i| template[i % size]));
    }
    fn read_periodic(&self, cycle: f64, rpm: f32, output_rate: f32, sinc: &SincTable) -> f32 {
        if self.periodic.is_empty() {
            return 0.;
        }
        let size = self.periodic.len() - 2 * PAD;
        let pos = (cycle + self.offset).rem_euclid(1.) * size as f64;
        let ratio = rpm as f64 / 120. * size as f64 / output_rate as f64;
        sinc.interpolate(&self.periodic, PAD as f64 + pos, ratio)
    }
    fn profile(&self) -> [f32; PROFILE] {
        let mut profile = [0.; PROFILE];
        for (i, v) in profile.iter_mut().enumerate() {
            for c in 0..self.cycles {
                let p = (c as f64 + i as f64 / PROFILE as f64) * self.frames as f64
                    / self.cycles as f64;
                let j = p as usize;
                let frac = (p - j as f64) as f32;
                *v +=
                    self.pcm[PAD + j] * (1. - frac) + self.pcm[PAD + (j + 1) % self.frames] * frac;
            }
            *v /= self.cycles as f32;
        }
        profile
    }
    pub fn read(&self, cycle: f64, rpm: f32, output_rate: f32, sinc: &SincTable) -> f32 {
        // The prepared loop joins contiguous source samples. Randomly jumping
        // between cycle-aligned but locally out-of-phase excerpts modulated the
        // level at every handover, most audibly between high-RPM source knots.
        self.read_loop(cycle, rpm, output_rate, sinc)
    }
    fn read_loop(&self, cycle: f64, rpm: f32, output_rate: f32, sinc: &SincTable) -> f32 {
        let pos = (cycle + self.offset).rem_euclid(self.cycles as f64) * self.frames as f64
            / self.cycles as f64;
        let ratio =
            (rpm as f64 / 120.) * self.frames as f64 / self.cycles as f64 / output_rate as f64;
        sinc.interpolate(&self.pcm, PAD as f64 + pos, ratio)
    }
    fn original(&self) -> &Sample {
        self.legacy.as_deref().unwrap_or(self)
    }
}
impl Bank {
    /// Prepare the generated sound model on the import worker before playback.
    pub fn prepare_procedural(&self) {
        let _ = self.procedural_model();
    }

    /// The descriptor analysis runs once per imported bank, before audio starts.
    pub(crate) fn procedural_model(&self) -> Arc<crate::procedural::ProceduralBank> {
        self.procedural
            .get_or_init(|| Arc::new(crate::procedural::ProceduralBank::from_bank(self)))
            .clone()
    }

    /// Import-time descriptors, not an identification of physical engine parts.
    pub fn character(&self) -> Character {
        let mut confidence = 0.;
        let mut edge = 0.;
        let mut count: f32 = 0.;
        for sample in self.layers.iter().flatten() {
            let data = &sample.pcm[PAD..PAD + sample.frames];
            let energy = data.iter().map(|x| (*x as f64).powi(2)).sum::<f64>();
            let changes = data
                .windows(2)
                .map(|w| (w[1] as f64 - w[0] as f64).powi(2))
                .sum::<f64>();
            // Approximate derivative ratio normalised to a 44.1 kHz source.
            edge += (changes / energy.max(1e-20)).sqrt() as f32 * sample.rate as f32 / 44100.;
            confidence += sample.period.confidence as f32;
            count += 1.;
        }
        Character {
            cycle_similarity: confidence / count.max(1.),
            edge_ratio: edge / count.max(1.),
        }
    }
    pub fn load(path: &Path, selected_blend: Option<&str>) -> Result<Self, String> {
        let mut zip = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if zip.len() > 20000 {
            return Err("Too many files in the ZIP archive".into());
        }
        let blends: Vec<String> = zip
            .file_names()
            .filter(|s| s.ends_with(".sfxBlend2D.json"))
            .map(str::to_owned)
            .collect();
        let blend = match selected_blend {
            Some(s) if blends.iter().any(|b| b == s) => s.to_owned(),
            None if blends.len() == 1 => blends[0].clone(),
            _ => {
                return Err(
                    "Select one blend explicitly when the archive contains multiple blends".into(),
                );
            }
        };
        let engine_meta = crate::engine_meta::inspect(path, &blend);
        let bytes = read_entry(&mut zip, &blend, 1_000_000)?;
        let mut hash = Sha256::new();
        hash.update(&bytes);
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let lists = value["samples"]
            .as_array()
            .filter(|a| a.len() == 2)
            .ok_or("The blend must contain two load layers")?;
        let mut layers: [Vec<Sample>; 2] = [Vec::new(), Vec::new()];
        let mut total = 0;
        for (layer, entries) in lists.iter().enumerate() {
            let entries = entries
                .as_array()
                .filter(|a| !a.is_empty() && a.len() <= 128)
                .ok_or("Unsupported number of RPM points")?;
            for row in entries {
                let name = row[0].as_str().ok_or("Missing WAV path")?;
                let rpm = row[1]
                    .as_f64()
                    .filter(|r| r.is_finite() && (200.0..=20000.0).contains(r))
                    .ok_or("Invalid source RPM")? as f32;
                let bytes = read_entry(&mut zip, name, MAX_FILE)?;
                hash.update(name.as_bytes());
                hash.update(&bytes);
                let (rate, raw) = decode_wav(&bytes)?;
                total += raw.len();
                if total > MAX_TOTAL {
                    return Err("Bank exceeds the 24 million sample limit".into());
                }
                layers[layer].push(Sample::prepare(rpm, rate, raw)?);
            }
            layers[layer].sort_by(|a, b| a.rpm.total_cmp(&b.rpm));
            if layers[layer].windows(2).any(|w| w[0].rpm == w[1].rpm) {
                return Err("Duplicate RPM points in the blend".into());
            }
        }
        let reference = layers[0][0].profile();
        // Correlate cycle profiles. A shared crank phase then keeps neighbouring
        // sources aligned while their independent residual textures remain intact.
        for s in layers.iter_mut().flatten() {
            let profile = s.profile();
            let shift = (0..PROFILE)
                .max_by(|&a, &b| {
                    let corr = |shift: usize| {
                        reference
                            .iter()
                            .enumerate()
                            .map(|(i, r)| r * profile[(i + shift) % PROFILE])
                            .sum::<f32>()
                    };
                    corr(a).total_cmp(&corr(b))
                })
                .unwrap_or(0);
            s.offset = shift as f64 / PROFILE as f64;
        }
        // Retain the exact previous preparation/alignment for source A.
        let reference = layers[0][0].original().profile();
        for s in layers.iter_mut().flatten() {
            if let Some(old) = &mut s.legacy {
                let profile = old.profile();
                let shift = (0..PROFILE)
                    .max_by(|&a, &b| {
                        let corr = |shift: usize| {
                            reference
                                .iter()
                                .enumerate()
                                .map(|(i, r)| r * profile[(i + shift) % PROFILE])
                                .sum::<f32>()
                        };
                        corr(a).total_cmp(&corr(b))
                    })
                    .unwrap_or(0);
                old.offset = shift as f64 / PROFILE as f64;
            }
        }
        let max_peak = layers.iter().flatten().fold(0f32, |v, s| v.max(s.peak));
        if max_peak < 1e-6 {
            return Err("Silent sound bank".into());
        }
        let gain = (0.65 / max_peak).min(40.);
        let original_gain = (0.65
            / layers
                .iter()
                .flatten()
                .map(|s| s.original().peak)
                .fold(0., f32::max))
        .min(40.);
        let min_rpm = layers
            .iter()
            .map(|l| l[0].rpm)
            .fold(f32::INFINITY, f32::min);
        let max_rpm = layers
            .iter()
            .map(|l| l.last().unwrap().rpm)
            .fold(0., f32::max);
        Ok(Self {
            source: SourceRef {
                archive: path
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned(),
                blend,
                fingerprint: format!("{:x}", hash.finalize()),
            },
            engine_meta,
            layers,
            gain,
            original_gain,
            min_rpm,
            max_rpm,
            procedural: OnceLock::new(),
        })
    }
    pub fn read(&self, cycle: f64, rpm: f32, load: f32, rate: f32, sinc: &SincTable) -> f32 {
        self.read_mode(cycle, rpm, load, rate, sinc, ReadMode::Source)
    }
    /// Approximate cycle component and its exact complementary residual.
    pub fn read_components(
        &self,
        cycle: f64,
        rpm: f32,
        load: f32,
        rate: f32,
        sinc: &SincTable,
    ) -> (f32, f32) {
        let source = self.read(cycle, rpm, load, rate, sinc);
        let periodic = self.read_mode(cycle, rpm, load, rate, sinc, ReadMode::Periodic);
        (periodic, source - periodic)
    }
    /// Interpolated share of source knots with a usable cycle template.
    /// A rejected knot has no template, so its residual is the entire source
    /// recording and should not drive a separate inferred engine emitter.
    pub fn residual_reliability(&self, rpm: f32, load: f32) -> f32 {
        let read_layer = |layer: &Vec<Sample>| {
            let accepted = |i: usize| f32::from(layer[i].period.accepted);
            let upper = layer.partition_point(|s| s.rpm < rpm).min(layer.len() - 1);
            let lower = upper.saturating_sub(1);
            if upper == lower {
                return accepted(upper);
            }
            let t =
                ((rpm - layer[lower].rpm) / (layer[upper].rpm - layer[lower].rpm)).clamp(0., 1.);
            let t = t * t * (3. - 2. * t);
            accepted(lower) * (1. - t) + accepted(upper) * t
        };
        let load = load.clamp(0., 1.);
        read_layer(&self.layers[0]) * (1. - load) + read_layer(&self.layers[1]) * load
    }
    pub fn read_original(
        &self,
        cycle: f64,
        rpm: f32,
        load: f32,
        rate: f32,
        sinc: &SincTable,
    ) -> f32 {
        self.read_mode(cycle, rpm, load, rate, sinc, ReadMode::Original)
    }
    /// Rescale the unchanged A reference to the processed bank's gain for a
    /// live comparison against the exhaust stem before export normalization.
    pub(crate) fn original_to_processed_gain(&self) -> f32 {
        self.gain / self.original_gain
    }
    fn read_mode(
        &self,
        cycle: f64,
        rpm: f32,
        load: f32,
        rate: f32,
        sinc: &SincTable,
        mode: ReadMode,
    ) -> f32 {
        let read_layer = |layer: &Vec<Sample>| {
            let sample = |i: usize| match mode {
                ReadMode::Original => layer[i].original().read(cycle, rpm, rate, sinc),
                ReadMode::Source => layer[i].read(cycle, rpm, rate, sinc),
                ReadMode::Periodic => layer[i].read_periodic(cycle, rpm, rate, sinc),
            };
            let upper = layer.partition_point(|s| s.rpm < rpm).min(layer.len() - 1);
            let lower = upper.saturating_sub(1);
            if upper == lower {
                return sample(upper);
            }
            let t =
                ((rpm - layer[lower].rpm) / (layer[upper].rpm - layer[lower].rpm)).clamp(0., 1.);
            let t = if matches!(mode, ReadMode::Original) {
                t
            } else {
                t * t * (3. - 2. * t)
            };
            sample(lower) * (1. - t) + sample(upper) * t
        };
        // Linear blend for correlated engine recordings: equal-power can add 3 dB.
        (read_layer(&self.layers[0]) * (1. - load) + read_layer(&self.layers[1]) * load)
            * if matches!(mode, ReadMode::Original) {
                self.original_gain
            } else {
                self.gain
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residual_reliability_follows_rpm_and_load_interpolation() {
        let sample = |rpm: f32, accepted: bool| Sample {
            rpm,
            rate: 48_000,
            pcm: Vec::new(),
            frames: 0,
            cycles: 0,
            offset: 0.,
            rms: 0.,
            peak: 0.,
            period: crate::period::Period {
                nominal: 1.,
                measured: 1.,
                confidence: f64::from(accepted),
                accepted,
            },
            legacy: None,
            periodic: Vec::new(),
        };
        let bank = Bank {
            source: SourceRef {
                archive: String::new(),
                blend: String::new(),
                fingerprint: String::new(),
            },
            engine_meta: None,
            layers: [
                vec![sample(1000., true), sample(2000., false)],
                vec![sample(1000., false), sample(2000., true)],
            ],
            gain: 1.,
            original_gain: 1.,
            min_rpm: 1000.,
            max_rpm: 2000.,
            procedural: OnceLock::new(),
        };
        assert_eq!(bank.residual_reliability(1000., 0.), 1.);
        assert_eq!(bank.residual_reliability(1000., 1.), 0.);
        assert_eq!(bank.residual_reliability(2000., 0.), 0.);
        assert_eq!(bank.residual_reliability(2000., 1.), 1.);
        assert!((bank.residual_reliability(1250., 0.25) - 0.671_875).abs() < 1e-6);
        assert!((bank.residual_reliability(1500., 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn steady_source_envelope_does_not_jump_between_distant_excerpts() {
        let raw: Vec<_> = (0..96000)
            .map(|i| {
                let p = std::f64::consts::TAU * i as f64 / 804.7;
                ((0.8 + 0.4 * i as f64 / 96000.) * (0.2 * p.sin() + 0.1 * (7. * p).sin())) as f32
            })
            .collect();
        let sample = Sample::prepare(7200., 48000, raw).unwrap();
        assert!(sample.period.accepted && sample.cycles >= 8);
        let sinc = SincTable::for_quality(bdsp::resample::SincQuality::Realtime);
        let mut levels = Vec::new();
        // The first 1.5 cycles contain the one-time seam overlap at import.
        for grain in 4..44 {
            let energy = (0..1600)
                .map(|i| {
                    let cycle = grain as f64 * 2. + i as f64 * 7200. / (120. * 48000.);
                    (sample.read(cycle, 7200., 48000., &sinc) as f64).powi(2)
                })
                .sum::<f64>();
            levels.push((energy / 1600.).sqrt());
        }
        let max_jump = levels
            .windows(2)
            .map(|w| (20. * (w[1] / w[0]).log10()).abs())
            .fold(0f64, f64::max);
        assert!(
            max_jump < 0.3,
            "Steady-RPM envelope changed by {max_jump:.2} dB over two cycles"
        );
    }

    #[test]
    fn prepared_loop_is_continuous_at_wrap() {
        let raw: Vec<_> = (0..96000)
            .map(|i| {
                let p = std::f64::consts::TAU * i as f64 / 804.7;
                (0.2 * p.sin() + 0.1 * (7. * p).sin() + 0.02 * (i as f64 * 0.37).sin()) as f32
            })
            .collect();
        let sample = Sample::prepare(7200., 48000, raw).unwrap();
        assert!(sample.period.accepted);
        let sinc = SincTable::for_quality(bdsp::resample::SincQuality::Realtime);
        for i in 1..50 {
            let cycle = i as f64 * sample.cycles as f64;
            let a = sample.read(cycle - 1e-7, 7200., 48000., &sinc);
            let b = sample.read(cycle + 1e-7, 7200., 48000., &sinc);
            assert!((a - b).abs() < 1e-5, "loop wrap {i}: {a} -> {b}");
        }
    }
    #[test]
    fn rpm_label_keeps_neighbouring_samples_on_shared_cycle_timebase() {
        let raw: Vec<_> = (0..96000)
            .map(|i| {
                let p = std::f64::consts::TAU * i as f64 / 804.7;
                (0.3 * p.sin() + 0.12 * (7. * p).sin()) as f32
            })
            .collect();
        let s = Sample::prepare(7200., 48000, raw).unwrap();
        let sinc = SincTable::for_quality(bdsp::resample::SincQuality::Realtime);
        assert!(s.period.accepted);
        assert!((s.period.measured - s.period.nominal).abs() > 1.);
        let max_difference = (0..4096)
            .map(|i| {
                let cycle = 3. + i as f64 * 110. / 4096.;
                (s.read(cycle, 7200., 48000., &sinc)
                    - s.original().read(cycle, 7200., 48000., &sinc))
                .abs()
            })
            .fold(0f32, f32::max);
        assert!(max_difference < 1e-7, "Drift from source: {max_difference}");
    }
    #[test]
    fn source_components_reconstruct_and_keep_nonperiodic_energy() {
        let mut seed = 93u32;
        let raw: Vec<_> = (0..96000)
            .map(|i| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                0.3 * (std::f64::consts::TAU * i as f64 / 800.).sin() as f32
                    + (seed as f32 / u32::MAX as f32 - 0.5) * 0.04
            })
            .collect();
        let s = Sample::prepare(7200., 48000, raw).unwrap();
        assert!(s.period.accepted);
        let sinc = SincTable::for_quality(bdsp::resample::SincQuality::Realtime);
        let (mut pe, mut re) = (0., 0.);
        for i in 0..48000 {
            let cycle = i as f64 / 799.3;
            let source = s.read(cycle, 7200., 48000., &sinc);
            let periodic = s.read_periodic(cycle, 7200., 48000., &sinc);
            let residual = source - periodic;
            assert!((periodic + residual - source).abs() < 3e-8);
            pe += periodic * periodic;
            re += residual * residual;
        }
        assert!(pe > re * 20. && re > 0.1, "periodic={pe} residual={re}");
    }
    #[test]
    fn loop_join_keeps_waveform_continuity() {
        let raw: Vec<_> = (0..96000)
            .map(|i| (std::f32::consts::TAU * i as f32 * 50. / 48000.).sin() * 0.2 + 0.1)
            .collect();
        let s = Sample::prepare(1500., 48000, raw).unwrap();
        assert!((s.pcm[PAD] - s.pcm[PAD + s.frames - 1]).abs() < 0.005);
        assert!(s.pcm.iter().all(|v| v.is_finite()));
    }
}
