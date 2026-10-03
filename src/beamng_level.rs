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
    /// The exact complete physical engine used to render the measured files.
    pub engine: crate::engine_definition::EngineDefinition,
    /// Common linear gain applied to every exported exhaust WAV.
    pub safety_gain: f32,
    /// The primary exhaust difference uses an 80 Hz high-pass proxy for the
    /// physical path. Detailed AC RMS and peaks always describe raw files.
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
    /// Exhaust difference against Automation at this knot. In physical mode
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

    #[cfg(test)]
    fn from_pcm24(samples: &[f32], gain: f32) -> Result<Self, String> {
        Self::from_pcm24_with_master(samples, gain, 1.)
    }

    fn from_pcm24_with_master(
        samples: &[f32],
        gain: f32,
        master_gain: f32,
    ) -> Result<Self, String> {
        if !gain.is_finite() || gain < 0. {
            return Err("Invalid BeamNG export gain".into());
        }
        // Match hound's PCM24 write followed by Bank::decode_wav. The exhaust
        // is decoded by variant::build before the engine stem is balanced.
        Self::measure(samples.iter().map(|&sample| {
            let neutral = (sample * gain * PCM24_SCALE) as i32;
            ((neutral as f32 * master_gain) as i32) as f32 / PCM24_DECODE_SCALE
        }))
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
    source_post_low_cut_rms: f32,
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

fn measured_low_cut_rms(samples: &[f32]) -> Result<f32, String> {
    use bdsp::svf::{StateVariableFilter, SvfMode};
    let mut filter = StateVariableFilter::new(48_000., 80., 0.707, SvfMode::Highpass);
    // Unlike calibration, file measurement accepts intentionally quiet/silent
    // PCM after master attenuation, including values below the calibration floor.
    Ok(Stats::measure(samples.iter().map(|&sample| filter.next_sample(sample)))?.rms)
}

/// Render every original blend point using the variant export's audio path.
/// This is CPU-intensive and should be called from a worker thread.
pub fn analyze(bank: Arc<Bank>, p: Parameters, h: Settings) -> Result<Report, String> {
    let (h, _) = export::resolved_settings(&bank, h)?;
    p.validate()?;
    h.validate()?;
    let archive = Path::new(&bank.source.archive);
    // The source might have changed since the UI imported it. Export performs
    // the same fresh-bank check before it writes any output.
    let fresh = Bank::load(archive, Some(&bank.source.blend))?;
    export::verify_source_identity(&bank.source, &fresh.source)?;
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
            let (mut exhaust, engine, _) = export::loop_stems(bank.clone(), p, h, rpm, load)?;
            let exhaust_stats = Stats::from_samples(&exhaust)?;
            Stats::from_samples(&engine)?;
            if exhaust_stats.rms < 1e-7 || exhaust_stats.peak < 1e-7 {
                return Err(format!("Silent exhaust stem at {rpm:.0} rpm, load {load}"));
            }
            let level_gain = export::physical_exhaust_level_gain(
                &source_samples,
                source_rate,
                &exhaust,
                exhaust_stats.peak,
            )?;
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
                source_post_low_cut_rms: export::low_cut_rms(&source_samples, source_rate)?,
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
        // The physical comparison follows the same quantization and final
        // bank-wide gain as the written WAV, then applies the 80 Hz proxy.
        let exhaust_pcm = {
            item.exhaust
                .iter()
                .map(|&sample| {
                    ((sample * safety_gain * PCM24_SCALE) as i32) as f32 / PCM24_DECODE_SCALE
                })
                .collect::<Vec<_>>()
        };
        let neutral_exhaust = Stats::from_samples(&exhaust_pcm)?;
        let engine_raw = Stats::from_samples(&item.engine)?;
        let engine_gain = variant::engine_stem_gain(
            neutral_exhaust.raw_rms,
            engine_raw.raw_rms,
            engine_raw.peak,
            item.load,
            h.engine_gain,
        );
        // Export refuses samples outside this ceiling before PCM encoding.
        if engine_raw.peak * engine_gain > 0.951 {
            return Err("Engine stem would clip PCM24".into());
        }
        // Calibrate both emitters at unity, then attenuate their encoded PCM24
        // integers once, matching export::apply_master_gain exactly.
        let exhaust_pcm: Vec<_> = exhaust_pcm
            .iter()
            .map(|&sample| {
                ((sample * PCM24_DECODE_SCALE * p.master_gain) as i32) as f32 / PCM24_DECODE_SCALE
            })
            .collect();
        let exhaust = Stats::from_samples(&exhaust_pcm)?;
        let engine = Stats::from_pcm24_with_master(&item.engine, engine_gain, p.master_gain)?;
        let source_rms_dbfs = dbfs(item.source.rms)?;
        let exhaust_rms_dbfs = dbfs_allow_silence(exhaust.rms)?;
        let engine_rms_dbfs = dbfs_allow_silence(engine.rms)?;
        let exhaust_vs_source_db = dbfs_allow_silence(measured_low_cut_rms(&exhaust_pcm)?)?
            - dbfs(item.source_post_low_cut_rms)?;
        points.push(Point {
            rpm: item.rpm,
            load: item.load,
            source_rms_dbfs,
            exhaust_rms_dbfs,
            engine_rms_dbfs,
            exhaust_peak_dbfs: dbfs_allow_silence(exhaust.peak)?,
            engine_peak_dbfs: dbfs_allow_silence(engine.peak)?,
            exhaust_vs_source_db,
            engine_vs_exhaust_db: if exhaust.rms == 0. {
                f32::NEG_INFINITY
            } else {
                engine_rms_dbfs - exhaust_rms_dbfs
            },
        });
    }
    Ok(Report {
        engine: h.engine.expect("resolved export engine"),
        safety_gain,
        post_low_cut_estimate: true,
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
        let bytes = wav.into_inner();
        let (_, decoded) = bank::decode_wav(&bytes).unwrap();
        let actual = Stats::from_samples(&decoded).unwrap();
        assert!((measured.rms - actual.rms).abs() < 1e-8);
        assert_eq!(measured.peak, actual.peak);
        for master in [0., 0.000_001, 0.5, 1.] {
            let final_bytes = export::apply_master_gain(bytes.clone(), master).unwrap();
            let (_, decoded) = bank::decode_wav(&final_bytes).unwrap();
            let actual = Stats::from_samples(&decoded).unwrap();
            let measured = Stats::from_pcm24_with_master(&samples, gain, master).unwrap();
            assert_eq!(measured.rms, actual.rms);
            assert_eq!(measured.peak, actual.peak);
            assert!(measured_low_cut_rms(&decoded).unwrap().is_finite());
        }
    }

    #[test]
    fn ac_level_excludes_dc_while_peak_keeps_it() {
        let stats = Stats::from_samples(&[0.15, 0.05]).unwrap();
        assert!((stats.rms - 0.05).abs() < 1e-7);
        assert!((stats.raw_rms - 0.111_803_4).abs() < 1e-7);
        assert!((stats.peak - 0.15).abs() < 1e-7);
    }

    #[test]
    fn muted_engine_pcm24_has_valid_silent_metrics() {
        let samples = [0.1, -0.2, 0.3];
        let muted = Stats::from_pcm24(&samples, 0.).unwrap();
        assert_eq!((muted.rms, muted.raw_rms, muted.peak), (0., 0., 0.));
        assert_eq!(dbfs_allow_silence(muted.rms).unwrap(), f32::NEG_INFINITY);
        assert!(Stats::from_pcm24(&samples, -0.1).is_err());
        assert!(Stats::from_pcm24(&samples, f32::NAN).is_err());
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
    fn edited_engine_report_matches_actual_variant_wavs_and_preserves_source() {
        const UID: &str = "694E80154252F6189DE80988120C7F13";
        let fixture = crate::test_support::automation_fixture();
        let work = fixture.with_extension("unified-export");
        std::fs::create_dir(&work).unwrap();
        let source = work.join("source.zip");
        let mut original = zip::ZipArchive::new(File::open(&fixture).unwrap()).unwrap();
        let mut writer = zip::ZipWriter::new(File::create(&source).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for index in 0..original.len() {
            let mut entry = original.by_index(index).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            let mut name = entry.name().to_owned();
            if name.ends_with(".wav") {
                name = name.replace("art/sound/", &format!("art/sound/engine/{UID}/"));
            } else if name.ends_with(".sfxBlend2D.json") {
                bytes = String::from_utf8(bytes)
                    .unwrap()
                    .replace("art/sound/", &format!("art/sound/engine/{UID}/"))
                    .into_bytes();
            } else if name.ends_with(".jbeam") {
                bytes = format!(
                    r#"{{"Camso_Engine_694e8":{{
                        "slotType": "Camso_Engine",
                        "soundConfigExhaust":{{"sampleName":"{UID}"}},
                        "mainEngine":{{"soundConfigExhaust":"soundConfigExhaust",
                            "maxRPM":6000,"inertia":0.2,"afterFireAudioCoef":1.7,
                            "torque":[["rpm","torque"],[800,90],[4000,170]]}}
                    }}}}"#
                )
                .into_bytes();
            }
            writer.start_file(name, options).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer
            .start_file("vehicles/test/info_test.json", options)
            .unwrap();
        writer
            .write_all(br#"{"Configuration":"Original"}"#)
            .unwrap();
        writer.finish().unwrap();
        drop(original);
        std::fs::remove_file(fixture).unwrap();

        let before = std::fs::read(&source).unwrap();
        let bank = Arc::new(Bank::load(&source, None).unwrap());
        let mut engine = crate::automation_model::AutomationModel::from_bank(&bank)
            .unwrap()
            .baseline;
        // A prior importer may have used different estimates. Its original
        // reference must survive export and reload instead of being replaced.
        let mut saved_baseline = engine;
        saved_baseline.build.compression = 8.5;
        engine.build.compression = 11.;
        engine.tuning.cam.lift_mm = Some(11.5);
        let settings = Settings {
            engine: Some(engine),
            engine_baseline: Some(saved_baseline),
            fuel_cut: 1.,
            starter: true,
            ..Settings::default()
        };
        let output = work.join("variant");
        let params = Parameters {
            master_gain: 0.5,
            ..Parameters::default()
        };
        variant::package(&output, params, settings, bank.clone()).unwrap();
        let report = analyze(bank.clone(), params, settings).unwrap();
        assert_eq!(report.engine, engine);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["engine_definition"],
            serde_json::to_value(engine).unwrap()
        );
        assert_eq!(
            manifest["engine_baseline"],
            serde_json::to_value(saved_baseline).unwrap()
        );
        assert_eq!(manifest["settings"]["fuel_cut"], 0.);
        assert_eq!(
            manifest["source_engine_fingerprint"],
            serde_json::json!(bank.source.engine_fingerprint)
        );
        assert_eq!(manifest["loop_policy"]["mode"], "stationary_rpm_load_loops");
        let saved = crate::project::load_project(&output.join("settings.bess.json")).unwrap();
        assert_eq!(saved.parameters.master_gain, 0.5);
        assert_eq!(saved.hybrid.engine, Some(engine));
        assert_eq!(saved.hybrid.engine_baseline, Some(saved_baseline));

        let mut produced = zip::ZipArchive::new(
            File::open(output.join(manifest["zip_file"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        for (field, engine_stem) in [("wav_paths", false), ("engine_wav_paths", true)] {
            let paths = manifest[field].as_array().unwrap();
            assert_eq!(paths.len(), report.points.len());
            for (path, point) in paths.iter().zip(&report.points) {
                let wav =
                    read_limited(&mut produced, path.as_str().unwrap(), MAX_WAV_BYTES).unwrap();
                let (rate, pcm) = bank::decode_wav(&wav).unwrap();
                assert_eq!(rate, 48_000);
                let stats = Stats::from_samples(&pcm).unwrap();
                assert!(stats.peak <= 0.951);
                let (rms, peak) = if engine_stem {
                    (point.engine_rms_dbfs, point.engine_peak_dbfs)
                } else {
                    (point.exhaust_rms_dbfs, point.exhaust_peak_dbfs)
                };
                assert!((dbfs(stats.rms).unwrap() - rms).abs() < 0.001);
                assert!((dbfs(stats.peak).unwrap() - peak).abs() < 0.001);
            }
        }
        let engine_bytes = read_limited(
            &mut produced,
            manifest["engine_path"].as_str().unwrap(),
            MAX_BLEND_BYTES,
        )
        .unwrap();
        let jbeam: serde_json::Value = serde_json::from_slice(&engine_bytes).unwrap();
        let physical = &jbeam[manifest["engine_part"].as_str().unwrap()]["mainEngine"];
        assert_eq!(physical["maxRPM"], 6000);
        assert_eq!(physical["inertia"], 0.2);
        assert_eq!(physical["torque"][2][1], 170);
        assert_eq!(physical["afterFireAudioCoef"], 1.7);
        let muted = analyze(
            bank.clone(),
            Parameters::default(),
            Settings {
                engine_gain: 0.,
                ..settings
            },
        )
        .unwrap();
        for point in muted.points {
            assert_eq!(point.engine_rms_dbfs, f32::NEG_INFINITY);
            assert_eq!(point.engine_peak_dbfs, f32::NEG_INFINITY);
            assert_eq!(point.engine_vs_exhaust_db, f32::NEG_INFINITY);
        }
        let muted = analyze(
            bank,
            Parameters {
                master_gain: 0.,
                ..params
            },
            settings,
        )
        .unwrap();
        for point in muted.points {
            assert!(point.source_rms_dbfs.is_finite());
            assert_eq!(point.exhaust_rms_dbfs, f32::NEG_INFINITY);
            assert_eq!(point.engine_rms_dbfs, f32::NEG_INFINITY);
            assert_eq!(point.exhaust_peak_dbfs, f32::NEG_INFINITY);
            assert_eq!(point.engine_peak_dbfs, f32::NEG_INFINITY);
            assert_eq!(point.exhaust_vs_source_db, f32::NEG_INFINITY);
            assert_eq!(point.engine_vs_exhaust_db, f32::NEG_INFINITY);
        }
        assert_eq!(std::fs::read(&source).unwrap(), before);
        drop(produced);
        std::fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn physical_exports_reject_an_audio_only_bank_without_verified_metadata() {
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
        // Old project settings cannot select a retired generator on export.
        let h = Settings {
            physical: false,
            procedural: true,
            ..Settings::default()
        };
        let error = analyze(bank.clone(), p, h).unwrap_err();
        assert!(
            error.contains("metadata")
                || error.contains("Metadata")
                || error.contains("Automation"),
            "{error}"
        );
        let output = work.join("rendered");
        assert!(export::package_exhaust_stem(&output, p, h, bank.clone()).is_err());
        assert!(
            export::loop_stems(bank.clone(), p, h, 12_001., 0.)
                .unwrap_err()
                .contains("RPM")
        );
        assert!(export::loop_stems(bank, p, h, 800., 0.).is_err());
        std::fs::remove_dir_all(work).unwrap();
    }
}
