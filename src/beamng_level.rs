//! File-level measurements for the selectable BeamNG configuration export.
//!
//! These values describe the WAVs before BeamNG applies JBeam sound gains,
//! filtering, spatial emitters, cabin treatment, and the rest of its mix.

use crate::{
    bank::{self, Bank},
    export,
    hybrid::Settings,
    project::Parameters,
    variant,
};
use std::{collections::HashSet, fs::File, io::Read, path::Path, sync::Arc};

const MAX_BLEND_BYTES: u64 = 1_000_000;
const MAX_WAV_BYTES: u64 = 16_000_000;
const PCM24_SCALE: f32 = 8_388_607.;
const PCM24_DECODE_SCALE: f32 = 8_388_608.;

#[derive(Clone, Debug)]
pub struct Report {
    /// Common linear gain applied to every exported exhaust WAV.
    pub safety_gain: f32,
    /// The primary exhaust difference uses an 80 Hz high-pass proxy for the
    /// procedural path. Detailed AC RMS and peaks always describe raw files.
    pub post_low_cut_estimate: bool,
    /// One point per original Automation RPM knot and load row.
    pub points: Vec<Point>,
}

#[derive(Clone, Debug)]
pub struct Point {
    pub rpm: f32,
    /// Original blend row: 0 = off load, 1 = on load.
    pub load: f32,
    /// AC RMS, with each WAV's mean removed before measuring its energy.
    pub source_rms_dbfs: f32,
    pub exhaust_rms_dbfs: f32,
    pub engine_rms_dbfs: f32,
    pub exhaust_peak_dbfs: f32,
    pub engine_peak_dbfs: f32,
    /// Exhaust difference against Automation at this knot. In procedural mode
    /// this uses an 80 Hz low-cut proxy on exact exported PCM24; otherwise it
    /// uses unfiltered AC RMS. See `Report::post_low_cut_estimate`.
    pub exhaust_vs_source_db: f32,
    /// Relative AC RMS of the two exported WAV files before BeamNG's JBeam gains.
    pub engine_vs_exhaust_db: f32,
}

impl Point {
    /// Estimated RMS level (in dBFS) for a given BeamNG camera perspective.
    pub fn camera_rms_dbfs(&self, camera: crate::bench::BeamNgCamera) -> f32 {
        let exhaust_rms = 10f32.powf(self.exhaust_rms_dbfs / 20.);
        let engine_rms = 10f32.powf(self.engine_rms_dbfs / 20.);
        let (ex_mult, eng_mult, overall, is_cabin) = camera.gains();
        let ex_level = exhaust_rms * ex_mult;
        let eng_level = engine_rms * eng_mult;
        let total = (ex_level * ex_level + eng_level * eng_level).sqrt() * overall;
        let total = if is_cabin { total * 0.75 } else { total };
        if total > 1e-6 {
            20. * total.log10()
        } else {
            f32::NEG_INFINITY
        }
    }

    /// Estimated peak level (in dBFS) for a given BeamNG camera perspective.
    pub fn camera_peak_dbfs(&self, camera: crate::bench::BeamNgCamera) -> f32 {
        let exhaust_peak = 10f32.powf(self.exhaust_peak_dbfs / 20.);
        let engine_peak = 10f32.powf(self.engine_peak_dbfs / 20.);
        let (ex_mult, eng_mult, overall, is_cabin) = camera.gains();
        let total = (exhaust_peak * ex_mult + engine_peak * eng_mult) * overall;
        let total = if is_cabin { total * 0.78 } else { total };
        if total > 1e-6 {
            20. * total.log10()
        } else {
            f32::NEG_INFINITY
        }
    }

    /// Balance between engine and exhaust in percentage (engine %)
    pub fn camera_engine_share(&self, camera: crate::bench::BeamNgCamera) -> f32 {
        let exhaust_rms = 10f32.powf(self.exhaust_rms_dbfs / 20.);
        let engine_rms = 10f32.powf(self.engine_rms_dbfs / 20.);
        let (ex_mult, eng_mult, _, _) = camera.gains();
        let ex_power = (exhaust_rms * ex_mult).powi(2);
        let eng_power = (engine_rms * eng_mult).powi(2);
        let sum = ex_power + eng_power;
        if sum > 1e-12 {
            (eng_power / sum) * 100.
        } else {
            50.
        }
    }
}

#[derive(Clone, Copy)]
struct Stats {
    /// Energy after removing the sample mean, used for level reporting and
    /// exhaust calibration.
    rms: f32,
    /// Full waveform energy, retained to mirror variant::build's engine gain.
    raw_rms: f32,
    /// Absolute PCM peak, including any DC offset.
    peak: f32,
}

impl Stats {
    fn from_samples(samples: &[f32]) -> Result<Self, String> {
        Self::measure(samples.iter().copied())
    }

    fn from_pcm24(samples: &[f32], gain: f32) -> Result<Self, String> {
        if !gain.is_finite() || gain <= 0. {
            return Err("Invalid BeamNG export gain".into());
        }
        // Match hound's PCM24 write followed by Bank::decode_wav. The exhaust
        // is decoded by variant::build before the engine stem is balanced.
        Self::measure(
            samples
                .iter()
                .map(|&sample| ((sample * gain * PCM24_SCALE) as i32) as f32 / PCM24_DECODE_SCALE),
        )
    }

    fn measure(samples: impl Iterator<Item = f32>) -> Result<Self, String> {
        let mut count = 0usize;
        let mut raw_power = 0f64;
        let mut mean = 0f64;
        let mut centered_power = 0f64;
        let mut peak = 0f32;
        for sample in samples {
            if !sample.is_finite() {
                return Err("Non-finite WAV sample in BeamNG level analysis".into());
            }
            count += 1;
            let sample64 = sample as f64;
            raw_power += sample64 * sample64;
            let delta = sample64 - mean;
            mean += delta / count as f64;
            centered_power += delta * (sample64 - mean);
            peak = peak.max(sample.abs());
        }
        if count == 0 || !raw_power.is_finite() || !centered_power.is_finite() {
            return Err("Empty or invalid WAV in BeamNG level analysis".into());
        }
        Ok(Self {
            rms: (centered_power / count as f64).sqrt() as f32,
            raw_rms: (raw_power / count as f64).sqrt() as f32,
            peak,
        })
    }
}

struct PendingPoint {
    rpm: f32,
    load: f32,
    source: Stats,
    source_post_low_cut_rms: Option<f32>,
    exhaust: Vec<f32>,
    engine: Vec<f32>,
}

fn read_limited(
    zip: &mut zip::ZipArchive<File>,
    name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let entry = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    if entry.size() > max_bytes {
        return Err(format!("File too large for BeamNG level analysis: {name}"));
    }
    let mut bytes = Vec::new();
    entry
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!("File too large for BeamNG level analysis: {name}"));
    }
    Ok(bytes)
}

fn dbfs(value: f32) -> Result<f32, String> {
    if !value.is_finite() || value <= 0. {
        return Err("Silent WAV in BeamNG level analysis".into());
    }
    Ok(20. * value.log10())
}

fn dbfs_allow_silence(value: f32) -> Result<f32, String> {
    if value == 0. {
        Ok(f32::NEG_INFINITY)
    } else {
        dbfs(value)
    }
}

/// Render every original blend point using the variant export's audio path.
/// This is CPU-intensive and should be called from a worker thread.
pub fn analyze(bank: Arc<Bank>, p: Parameters, h: Settings) -> Result<Report, String> {
    p.validate()?;
    h.validate()?;
    let archive = Path::new(&bank.source.archive);
    // The source might have changed since the UI imported it. Export performs
    // the same fresh-bank check before it writes any output.
    let fresh = Bank::load(archive, Some(&bank.source.blend))?;
    if fresh.source.fingerprint != bank.source.fingerprint {
        return Err("Source archive changed since import".into());
    }
    drop(fresh);
    let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let blend_bytes = read_limited(&mut zip, &bank.source.blend, MAX_BLEND_BYTES)?;
    let blend: serde_json::Value =
        serde_json::from_slice(&blend_bytes).map_err(|e| e.to_string())?;
    let rows = blend["samples"]
        .as_array()
        .filter(|rows| rows.len() == 2)
        .ok_or("Original blend must have two load layers")?;

    let mut pending = Vec::new();
    let mut max_exhaust_peak = 0f32;
    let mut seen_wavs = HashSet::new();
    for (layer, row) in rows.iter().enumerate() {
        let points = row.as_array().ok_or("Invalid blend load layer")?;
        if points.len() != bank.layers[layer].len() {
            return Err("Imported bank differs from the source blend".into());
        }
        for entry in points {
            let path = entry[0].as_str().ok_or("Missing source WAV path")?;
            if !seen_wavs.insert(path) {
                return Err("A WAV shared by multiple RPM points cannot be replaced".into());
            }
            let rpm = entry[1].as_f64().ok_or("Missing source RPM")? as f32;
            if !bank.layers[layer].iter().any(|sample| sample.rpm == rpm) {
                return Err("Imported bank differs from the source blend".into());
            }
            let wav_bytes = read_limited(&mut zip, path, MAX_WAV_BYTES)?;
            let (source_rate, source_samples) = bank::decode_wav(&wav_bytes)?;
            let source = Stats::from_samples(&source_samples)?;
            if source.rms < 1e-7 || source.peak < 1e-7 {
                return Err(format!("Silent source WAV at {rpm:.0} rpm, load {layer}"));
            }
            dbfs(source.rms)?;

            // The four-second engine render contains the same first two-second
            // exhaust stem as export::package_exhaust_stem. Apply its per-knot
            // source-level calibration before the bank-wide safety gain.
            let load = layer as f32;
            let (mut exhaust, engine, _) = export::loop_stems(bank.clone(), p, h, rpm, load);
            let exhaust_stats = Stats::from_samples(&exhaust)?;
            Stats::from_samples(&engine)?;
            if exhaust_stats.rms < 1e-7 || exhaust_stats.peak < 1e-7 {
                return Err(format!("Silent exhaust stem at {rpm:.0} rpm, load {load}"));
            }
            let level_gain = if h.procedural {
                export::procedural_exhaust_level_gain(
                    &source_samples,
                    source_rate,
                    &exhaust,
                    exhaust_stats.peak,
                )?
            } else {
                export::exhaust_level_gain(source.rms, exhaust_stats.rms, exhaust_stats.peak)
            };
            if !level_gain.is_finite() || level_gain <= 0. {
                return Err(format!(
                    "Invalid exhaust level gain at {rpm:.0} rpm, load {load}"
                ));
            }
            for sample in &mut exhaust {
                *sample *= level_gain;
            }
            // An untrusted period can intentionally yield a silent inferred
            // engine stem; this remains a valid PCM24 file in the add-on.
            max_exhaust_peak = max_exhaust_peak.max(exhaust_stats.peak * level_gain);
            pending.push(PendingPoint {
                rpm,
                load,
                source,
                source_post_low_cut_rms: h
                    .procedural
                    .then(|| export::low_cut_rms(&source_samples, source_rate))
                    .transpose()?,
                exhaust,
                engine,
            });
        }
    }
    if pending.is_empty() || !max_exhaust_peak.is_finite() || max_exhaust_peak < 1e-8 {
        return Err("Silent or invalid BeamNG export".into());
    }
    let safety_gain = export::exhaust_safety_gain(max_exhaust_peak);
    let mut points = Vec::with_capacity(pending.len());
    pending.sort_by(|a, b| a.load.total_cmp(&b.load).then(a.rpm.total_cmp(&b.rpm)));
    for item in pending {
        // The procedural comparison follows the same quantization and final
        // bank-wide gain as the written WAV, then applies the 80 Hz proxy.
        let exhaust_pcm = h.procedural.then(|| {
            item.exhaust
                .iter()
                .map(|&sample| {
                    ((sample * safety_gain * PCM24_SCALE) as i32) as f32 / PCM24_DECODE_SCALE
                })
                .collect::<Vec<_>>()
        });
        let exhaust = match &exhaust_pcm {
            Some(samples) => Stats::from_samples(samples)?,
            None => Stats::from_pcm24(&item.exhaust, safety_gain)?,
        };
        let engine_raw = Stats::from_samples(&item.engine)?;
        let engine_gain = variant::engine_stem_gain(
            exhaust.raw_rms,
            engine_raw.raw_rms,
            engine_raw.peak,
            item.load,
            if h.procedural {
                1.
            } else {
                bank.residual_reliability(item.rpm, item.load)
            },
        );
        // Export refuses samples outside this ceiling before PCM encoding.
        if engine_raw.peak * engine_gain > 0.951 {
            return Err("Engine stem would clip PCM24".into());
        }
        let engine = Stats::from_pcm24(&item.engine, engine_gain)?;
        let source_rms_dbfs = dbfs(item.source.rms)?;
        let exhaust_rms_dbfs = dbfs(exhaust.rms)?;
        let engine_rms_dbfs = dbfs_allow_silence(engine.rms)?;
        let exhaust_vs_source_db = if let Some(samples) = &exhaust_pcm {
            let source_post_low_cut = item
                .source_post_low_cut_rms
                .ok_or("Missing procedural source low-cut level")?;
            dbfs(export::low_cut_rms(samples, 48_000)?)? - dbfs(source_post_low_cut)?
        } else {
            exhaust_rms_dbfs - source_rms_dbfs
        };
        points.push(Point {
            rpm: item.rpm,
            load: item.load,
            source_rms_dbfs,
            exhaust_rms_dbfs,
            engine_rms_dbfs,
            exhaust_peak_dbfs: dbfs(exhaust.peak)?,
            engine_peak_dbfs: dbfs_allow_silence(engine.peak)?,
            exhaust_vs_source_db,
            engine_vs_exhaust_db: engine_rms_dbfs - exhaust_rms_dbfs,
        });
    }
    Ok(Report {
        safety_gain,
        post_low_cut_estimate: h.procedural,
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn pcm24_metric_matches_written_and_decoded_samples() {
        let samples: Vec<f32> = (0..512).map(|i| (i as f32 / 256. - 1.) * 0.55).collect();
        let gain = 0.67;
        let measured = Stats::from_pcm24(&samples, gain).unwrap();
        let mut wav = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(
                &mut wav,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 48_000,
                    bits_per_sample: 24,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for sample in &samples {
                writer
                    .write_sample((sample * gain * PCM24_SCALE) as i32)
                    .unwrap();
            }
            writer.finalize().unwrap();
        }
        let (_, decoded) = bank::decode_wav(&wav.into_inner()).unwrap();
        let actual = Stats::from_samples(&decoded).unwrap();
        assert!((measured.rms - actual.rms).abs() < 1e-8);
        assert_eq!(measured.peak, actual.peak);
    }

    #[test]
    fn ac_level_excludes_dc_while_peak_keeps_it() {
        let stats = Stats::from_samples(&[0.15, 0.05]).unwrap();
        assert!((stats.rms - 0.05).abs() < 1e-7);
        assert!((stats.raw_rms - 0.111_803_4).abs() < 1e-7);
        assert!((stats.peak - 0.15).abs() < 1e-7);
    }

    #[test]
    fn db_differences_match_linear_ratios() {
        let source = dbfs(0.1).unwrap();
        let exhaust = dbfs(0.2).unwrap();
        let engine = dbfs(0.05).unwrap();
        assert!(((exhaust - source) - 6.0206).abs() < 0.001);
        assert!(((engine - exhaust) + 12.0412).abs() < 0.001);
    }

    #[test]
    fn level_report_matches_rendered_exhaust_archive() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let work = std::env::temp_dir().join(format!("bess-level-{unique}"));
        std::fs::create_dir(&work).unwrap();
        let source = work.join("automation.zip");
        let mut zip = zip::ZipWriter::new(File::create(&source).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        let mut layers = [Vec::new(), Vec::new()];
        for (layer, list) in layers.iter_mut().enumerate() {
            let name = format!("art/sound/engine/test/{layer}.wav");
            let mut cursor = std::io::Cursor::new(Vec::new());
            {
                let mut wav = hound::WavWriter::new(
                    &mut cursor,
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: 16_000,
                        bits_per_sample: 32,
                        sample_format: hound::SampleFormat::Float,
                    },
                )
                .unwrap();
                for i in 0..32_000u32 {
                    let phase = std::f32::consts::TAU * i as f32 * 800. / (16_000. * 120.);
                    let grain = (i.wrapping_mul(17_321).wrapping_add(layer as u32 * 317) % 997)
                        as f32
                        / 997.
                        - 0.5;
                    wav.write_sample(
                        0.08 + 0.04 * layer as f32
                            + (0.06 * (phase * 4.).sin() + 0.01 * grain)
                                * (1. + layer as f32 * 0.3),
                    )
                    .unwrap();
                }
                wav.finalize().unwrap();
            }
            zip.start_file(&name, options).unwrap();
            zip.write_all(&cursor.into_inner()).unwrap();
            list.push(serde_json::json!([name, 800]));
        }
        zip.start_file("art/sound/blends/test.sfxBlend2D.json", options)
            .unwrap();
        zip.write_all(
            serde_json::to_string(&serde_json::json!({"samples":layers}))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        zip.start_file("vehicles/test/info.json", options).unwrap();
        zip.write_all(br#"{"Name":"Test Vehicle"}"#).unwrap();
        zip.finish().unwrap();

        let bank = Arc::new(Bank::load(&source, None).unwrap());
        let p = Parameters::default();
        let h = Settings::default();
        let report = analyze(bank.clone(), p, h).unwrap();
        assert!(!report.post_low_cut_estimate);
        assert_eq!(report.points.len(), 2);
        let output = work.join("rendered");
        export::package_exhaust_stem(&output, p, h, bank.clone()).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            report.safety_gain,
            manifest["gain"].as_f64().unwrap() as f32
        );
        let archive = output.join(export::package_name(&bank));
        let mut rendered = zip::ZipArchive::new(File::open(archive).unwrap()).unwrap();
        let mut original = zip::ZipArchive::new(File::open(&source).unwrap()).unwrap();
        for (layer, point) in report.points.iter().enumerate() {
            let path = format!("art/sound/engine/test/{layer}.wav");
            let wav = read_limited(&mut rendered, &path, MAX_WAV_BYTES).unwrap();
            let (_, samples) = bank::decode_wav(&wav).unwrap();
            let actual = Stats::from_samples(&samples).unwrap();
            assert!((point.exhaust_rms_dbfs - dbfs(actual.rms).unwrap()).abs() < 0.00001);
            assert!((point.exhaust_peak_dbfs - dbfs(actual.peak).unwrap()).abs() < 0.00001);

            let source_wav = read_limited(&mut original, &path, MAX_WAV_BYTES).unwrap();
            let (_, source_samples) = bank::decode_wav(&source_wav).unwrap();
            let source_stats = Stats::from_samples(&source_samples).unwrap();
            assert!(source_stats.raw_rms > source_stats.rms * 1.5);
            assert!((point.source_rms_dbfs - dbfs(source_stats.rms).unwrap()).abs() < 0.00001);
            let (raw_exhaust, _, _) = export::loop_stems(bank.clone(), p, h, 800., layer as f32);
            let raw_stats = Stats::from_samples(&raw_exhaust).unwrap();
            let expected_gain =
                export::exhaust_level_gain(source_stats.rms, raw_stats.rms, raw_stats.peak);
            let documented_gain = manifest["loops"]
                .as_array()
                .unwrap()
                .iter()
                .find(|loop_info| loop_info["path"] == path)
                .unwrap()["exhaust_level_gain"]
                .as_f64()
                .unwrap() as f32;
            assert!((documented_gain - expected_gain).abs() < 1e-6);
            assert!((actual.rms - raw_stats.rms * expected_gain * report.safety_gain).abs() < 1e-6);
        }
        drop(rendered);

        // The procedural report uses the same per-knot low-cut calibration as
        // export and measures the actual quantized PCM24 after safety gain.
        let mut procedural = h;
        procedural.procedural = true;
        let procedural_report = analyze(bank.clone(), p, procedural).unwrap();
        assert!(procedural_report.post_low_cut_estimate);
        let procedural_output = work.join("procedural-rendered");
        export::package_exhaust_stem(&procedural_output, p, procedural, bank.clone()).unwrap();
        let procedural_manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(procedural_output.join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            procedural_report.safety_gain,
            procedural_manifest["gain"].as_f64().unwrap() as f32
        );
        let mut procedural_zip = zip::ZipArchive::new(
            File::open(procedural_output.join(export::package_name(&bank))).unwrap(),
        )
        .unwrap();
        for (layer, point) in procedural_report.points.iter().enumerate() {
            let path = format!("art/sound/engine/test/{layer}.wav");
            let source_wav = read_limited(&mut original, &path, MAX_WAV_BYTES).unwrap();
            let (source_rate, source_samples) = bank::decode_wav(&source_wav).unwrap();
            let exported_wav = read_limited(&mut procedural_zip, &path, MAX_WAV_BYTES).unwrap();
            let (rate, exported_samples) = bank::decode_wav(&exported_wav).unwrap();
            assert_eq!(rate, 48_000);
            let actual = Stats::from_samples(&exported_samples).unwrap();
            assert!((point.exhaust_rms_dbfs - dbfs(actual.rms).unwrap()).abs() < 0.00001);
            assert!((point.exhaust_peak_dbfs - dbfs(actual.peak).unwrap()).abs() < 0.00001);
            let expected_difference = dbfs(export::low_cut_rms(&exported_samples, rate).unwrap())
                .unwrap()
                - dbfs(export::low_cut_rms(&source_samples, source_rate).unwrap()).unwrap();
            assert!((point.exhaust_vs_source_db - expected_difference).abs() < 0.00001);

            let (raw_exhaust, _, _) =
                export::loop_stems(bank.clone(), p, procedural, 800., layer as f32);
            let raw_peak = Stats::from_samples(&raw_exhaust).unwrap().peak;
            let expected_gain = export::procedural_exhaust_level_gain(
                &source_samples,
                source_rate,
                &raw_exhaust,
                raw_peak,
            )
            .unwrap();
            let documented_gain = procedural_manifest["loops"]
                .as_array()
                .unwrap()
                .iter()
                .find(|loop_info| loop_info["path"] == path)
                .unwrap()["exhaust_level_gain"]
                .as_f64()
                .unwrap() as f32;
            assert!((documented_gain - expected_gain).abs() < 1e-6);
        }
        drop(procedural_zip);

        // The legacy full-replacement route still writes its original mixed
        // channel with only the bank-wide safety gain.
        let legacy_output = work.join("legacy");
        export::package(&legacy_output, p, h, bank.clone()).unwrap();
        let legacy_manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(legacy_output.join("manifest.json")).unwrap())
                .unwrap();
        assert!(
            legacy_manifest["loops"]
                .as_array()
                .unwrap()
                .iter()
                .all(|loop_info| loop_info.get("exhaust_level_gain").is_none())
        );
        let mut legacy = zip::ZipArchive::new(
            File::open(legacy_output.join(export::package_name(&bank))).unwrap(),
        )
        .unwrap();
        let legacy_wav =
            read_limited(&mut legacy, "art/sound/engine/test/0.wav", MAX_WAV_BYTES).unwrap();
        let (_, legacy_samples) = bank::decode_wav(&legacy_wav).unwrap();
        let legacy_stats = Stats::from_samples(&legacy_samples).unwrap();
        let (_, _, raw_mixed) = export::loop_stems(bank.clone(), p, h, 800., 0.);
        let mixed_stats = Stats::from_samples(&raw_mixed).unwrap();
        let legacy_safety = legacy_manifest["gain"].as_f64().unwrap() as f32;
        assert!((legacy_stats.rms - mixed_stats.rms * legacy_safety).abs() < 1e-6);
        drop(legacy);
        std::fs::remove_dir_all(&work).unwrap();
    }
}
