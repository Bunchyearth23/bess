//! Offline linear spectral calibration against a reference WAV.
//!
//! A bounded magnitude response is fitted on the first 2/3 of each recording,
//! realised as a symmetric FIR, and measured on the held-out final 1/3.
//! This does not infer combustion physics, identify a vehicle, align phases,
//! or demonstrate authenticity. Use stationary recordings at matching RPM,
//! load and microphone conditions. Original WAVs are never modified.
use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{f64::consts::TAU, fs, path::Path};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CalibrationConfig {
    pub fft_size: usize,
    pub taps: usize,
    pub max_gain_db: f64,
    pub low_hz: f64,
    pub high_hz: f64,
    /// Spectral power floor relative to the largest fitted source/reference bin.
    pub regularization: f64,
}
impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            fft_size: 4096,
            taps: 257,
            max_gain_db: 12.,
            low_hz: 40.,
            high_hz: 12000.,
            regularization: 1e-6,
        }
    }
}
impl CalibrationConfig {
    fn validate(self, rate: u32) -> Result<(), String> {
        if !(8000..=192000).contains(&rate)
            || !self.fft_size.is_power_of_two()
            || !(256..=16384).contains(&self.fft_size)
            || !(3..=1025).contains(&self.taps)
            || self.taps.is_multiple_of(2)
            || self.taps >= self.fft_size
            || !(0.0..=24.).contains(&self.max_gain_db)
            || !self.low_hz.is_finite()
            || !self.high_hz.is_finite()
            || !(0.0..f64::from(rate) * 0.5).contains(&self.low_hz)
            || self.high_hz <= self.low_hz
            || self.high_hz > f64::from(rate) * 0.5
            || !(1e-10..=0.01).contains(&self.regularization)
        {
            return Err(
                "Invalid spectral calibration rate, FIR length, frequency band or regularization"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalibrationProfile {
    pub version: u32,
    pub sample_rate: u32,
    pub delay_samples: usize,
    pub coefficients: Vec<f64>,
    /// Fixed attenuation chosen once from the generated calibration recording.
    pub output_safety_gain: f64,
    pub config: CalibrationConfig,
}
impl CalibrationProfile {
    pub fn validate(&self) -> Result<(), String> {
        self.config.validate(self.sample_rate)?;
        if self.version != 1
            || self.coefficients.len() != self.config.taps
            || self.delay_samples != self.coefficients.len() / 2
            || self
                .coefficients
                .iter()
                .any(|x| !x.is_finite() || x.abs() > 32.)
            || self.coefficients.iter().map(|x| x.abs()).sum::<f64>()
                > 10_f64.powf(self.config.max_gain_db / 20.) * (1. + 1e-10)
            || !(0.0..=1.).contains(&self.output_safety_gain)
            || self.output_safety_gain == 0.
        {
            return Err("Invalid linear calibration profile".into());
        }
        Ok(())
    }
    /// Causal linear FIR, including its declared delay. No limiter or AGC.
    pub fn apply(&self, input: &[f32]) -> Result<Vec<f32>, String> {
        self.validate()?;
        validate_samples(input)?;
        let mut output = vec![0.; input.len()];
        for (i, out) in output.iter_mut().enumerate() {
            let sum: f64 = self.coefficients[..self.coefficients.len().min(i + 1)]
                .iter()
                .enumerate()
                .map(|(tap, coefficient)| coefficient * f64::from(input[i - tap]))
                .sum();
            let value = sum * self.output_safety_gain;
            if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
                return Err("Non-finite calibrated output".into());
            }
            *out = value as f32;
        }
        Ok(output)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpectralMetrics {
    pub active_bins: usize,
    pub before_rmse_db: f64,
    pub after_rmse_db: f64,
    pub source_rms: f64,
    pub reference_rms: f64,
    pub calibrated_rms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub profile: CalibrationProfile,
    pub training: SpectralMetrics,
    pub held_out: SpectralMetrics,
    pub generated_sha256: Option<String>,
    pub reference_sha256: Option<String>,
    pub generated_path: Option<String>,
    pub reference_path: Option<String>,
    pub note: String,
}

fn validate_samples(samples: &[f32]) -> Result<(), String> {
    if samples.is_empty() || samples.iter().any(|x| !x.is_finite()) {
        return Err("Calibration needs nonempty finite audio".into());
    }
    Ok(())
}

fn rms(samples: &[f32]) -> f64 {
    let mean = samples.iter().map(|x| f64::from(*x)).sum::<f64>() / samples.len() as f64;
    (samples
        .iter()
        .map(|x| (f64::from(*x) - mean).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt()
}

fn spectrum(samples: &[f32], size: usize) -> Result<Vec<f64>, String> {
    validate_samples(samples)?;
    if samples.len() < size {
        return Err("Calibration recording is too short for its FFT size".into());
    }
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(size);
    let mut scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
    let window: Vec<f64> = (0..size)
        .map(|i| 0.5 - 0.5 * (TAU * i as f64 / size as f64).cos())
        .collect();
    let window_power: f64 = window.iter().map(|x| x * x).sum();
    let mut buffer = vec![Complex::default(); size];
    let mut power = vec![0.; size / 2 + 1];
    let mut frames = 0;
    for start in (0..=samples.len() - size).step_by(size / 2) {
        let frame = &samples[start..start + size];
        let mean = frame.iter().map(|x| f64::from(*x)).sum::<f64>() / size as f64;
        for ((out, sample), win) in buffer.iter_mut().zip(frame).zip(&window) {
            *out = Complex::new((f64::from(*sample) - mean) * win, 0.);
        }
        fft.process_with_scratch(&mut buffer, &mut scratch);
        for (p, value) in power.iter_mut().zip(&buffer) {
            *p += value.norm_sqr() / window_power;
        }
        frames += 1;
    }
    for p in &mut power {
        *p /= f64::from(frames);
    }
    if power.iter().copied().fold(0_f64, f64::max) < 1e-14 {
        return Err("Calibration rejects silent recordings".into());
    }
    Ok(power)
}

fn metric(
    source: &[f32],
    reference: &[f32],
    calibrated: &[f32],
    rate: u32,
    config: CalibrationConfig,
) -> Result<SpectralMetrics, String> {
    let (a, b, c) = (
        spectrum(source, config.fft_size)?,
        spectrum(reference, config.fft_size)?,
        spectrum(calibrated, config.fft_size)?,
    );
    let floor = a.iter().chain(&b).copied().fold(0_f64, f64::max) * 1e-7;
    let mut before = 0.;
    let mut after = 0.;
    let mut bins = 0;
    for i in 1..a.len() {
        let hz = i as f64 * f64::from(rate) / config.fft_size as f64;
        if hz < config.low_hz || hz > config.high_hz || a[i] < floor || b[i] < floor {
            continue;
        }
        before += (10. * (a[i] / b[i]).log10()).powi(2);
        after += (10. * (c[i].max(floor * 1e-6) / b[i]).log10()).powi(2);
        bins += 1;
    }
    if bins == 0 {
        return Err("Source/reference have no jointly excited calibration bins".into());
    }
    Ok(SpectralMetrics {
        active_bins: bins,
        before_rmse_db: (before / bins as f64).sqrt(),
        after_rmse_db: (after / bins as f64).sqrt(),
        source_rms: rms(source),
        reference_rms: rms(reference),
        calibrated_rms: rms(calibrated),
    })
}

pub fn fit(
    generated: &[f32],
    reference: &[f32],
    rate: u32,
    config: CalibrationConfig,
) -> Result<CalibrationReport, String> {
    config.validate(rate)?;
    validate_samples(generated)?;
    validate_samples(reference)?;
    // Both independent recordings supply training and held-out material.
    if generated.len().min(reference.len()) < config.fft_size * 6 + config.taps * 3 {
        return Err(
            "Calibration needs at least six FFT windows plus FIR settling in each recording".into(),
        );
    }
    let (split_g, split_r) = (generated.len() * 2 / 3, reference.len() * 2 / 3);
    let source = spectrum(&generated[..split_g], config.fft_size)?;
    let target = spectrum(&reference[..split_r], config.fft_size)?;
    let floor = source.iter().chain(&target).copied().fold(0_f64, f64::max) * config.regularization;
    let bound = 10_f64.powf(config.max_gain_db / 20.);
    let raw: Vec<f64> = source
        .iter()
        .zip(&target)
        .enumerate()
        .map(|(i, (&a, &b))| {
            let hz = i as f64 * f64::from(rate) / config.fft_size as f64;
            if hz < config.low_hz || hz > config.high_hz || a < floor {
                1.
            } else {
                ((b + floor) / (a + floor)).sqrt().clamp(1. / bound, bound)
            }
        })
        .collect();
    // Local log-frequency smoothing discourages overfitting isolated engine
    // harmonics or noise. DC/Nyquist remain explicit real-valued bins.
    let mut gains = raw.clone();
    for (i, gain) in gains.iter_mut().enumerate().skip(1) {
        let lo = ((i as f64 / 2_f64.powf(1. / 12.)).floor() as usize).max(1);
        let hi = ((i as f64 * 2_f64.powf(1. / 12.)).ceil() as usize).min(raw.len() - 1);
        *gain = (raw[lo..=hi].iter().map(|x| x.ln()).sum::<f64>() / (hi - lo + 1) as f64).exp();
    }
    let mut bins = vec![Complex::default(); config.fft_size];
    for (i, &gain) in gains.iter().enumerate() {
        bins[i].re = gain;
        if i > 0 && i < config.fft_size / 2 {
            bins[config.fft_size - i].re = gain;
        }
    }
    FftPlanner::<f64>::new()
        .plan_fft_inverse(config.fft_size)
        .process(&mut bins);
    let center = config.taps / 2;
    let mut coefficients: Vec<f64> = (0..config.taps)
        .map(|i| {
            let index = (i + config.fft_size - center) % config.fft_size;
            let window = 0.5 - 0.5 * (TAU * i as f64 / (config.taps - 1) as f64).cos();
            bins[index].re / config.fft_size as f64 * window
        })
        .collect();
    // The l1 bound guarantees bounded gain at every frequency and for every
    // input, including between fitted FFT bins and after window truncation.
    let l1: f64 = coefficients.iter().map(|x| x.abs()).sum();
    if l1 > bound {
        for coefficient in &mut coefficients {
            *coefficient *= bound / l1;
        }
    }
    let mut profile = CalibrationProfile {
        version: 1,
        sample_rate: rate,
        delay_samples: center,
        coefficients,
        output_safety_gain: 1.,
        config,
    };
    let mut output = profile.apply(generated)?;
    let peak = output.iter().map(|x| x.abs()).fold(0_f32, f32::max);
    profile.output_safety_gain = (0.95 / f64::from(peak)).min(1.);
    for value in &mut output {
        *value *= profile.output_safety_gain as f32;
    }
    let skip = config.taps;
    let training = metric(
        &generated[skip..split_g - center],
        &reference[skip..split_r - center],
        &output[skip + center..split_g],
        rate,
        config,
    )?;
    let held_out = metric(
        &generated[split_g + skip..generated.len() - center],
        &reference[split_r + skip..reference.len() - center],
        &output[split_g + skip + center..],
        rate,
        config,
    )?;
    Ok(CalibrationReport {
        profile, training, held_out, generated_sha256: None, reference_sha256: None, generated_path: None, reference_path: None,
        note: "Linear magnitude matching at the supplied operating point only; final third held out. No phase identification, combustion identification or authenticity claim. Fixed PCM headroom attenuation is reported separately.".into(),
    })
}

/// Creates a new output directory containing the profile/report and a PCM24
/// calibrated copy. SHA256 identifies the two original unmodified WAV files.
pub fn calibrate_wavs(
    generated: &Path,
    reference: &Path,
    output: &Path,
    config: CalibrationConfig,
) -> Result<CalibrationReport, String> {
    let read = |path: &Path| -> Result<Vec<u8>, String> {
        if fs::metadata(path).map_err(|e| e.to_string())?.len() > 256_000_000 {
            return Err("Calibration WAV exceeds 256 MB".into());
        }
        fs::read(path).map_err(|e| e.to_string())
    };
    let (source_bytes, reference_bytes) = (read(generated)?, read(reference)?);
    let (rate, source) = crate::bank::decode_wav(&source_bytes)?;
    let (reference_rate, target) = crate::bank::decode_wav(&reference_bytes)?;
    if rate != reference_rate {
        return Err("Calibration WAV sample rates must match; resample explicitly first".into());
    }
    let mut report = fit(&source, &target, rate, config)?;
    report.generated_sha256 = Some(format!("{:x}", Sha256::digest(&source_bytes)));
    report.reference_sha256 = Some(format!("{:x}", Sha256::digest(&reference_bytes)));
    report.generated_path = Some(generated.display().to_string());
    report.reference_path = Some(reference.display().to_string());
    let calibrated = report.profile.apply(&source)?;
    fs::create_dir(output)
        .map_err(|e| format!("Choose a new calibration output directory: {e}"))?;
    // Keep the recording's sample rate; render::write_pcm always uses 48 kHz.
    let mut writer = hound::WavWriter::create(
        output.join("calibrated.wav"),
        hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|e| e.to_string())?;
    for value in calibrated {
        if !value.is_finite() || value.abs() > 0.951 {
            return Err("Calibrated output exceeds its measured PCM headroom".into());
        }
        writer
            .write_sample((value * 8_388_607.) as i32)
            .map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    fs::write(
        output.join("calibration.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn noise() -> Vec<f32> {
        let mut state = 1234567_u32;
        (0..48000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f64 / u32::MAX as f64 - 0.5) as f32 * 0.08
            })
            .collect()
    }
    #[test]
    fn unity_fit_is_identity_with_declared_linear_phase_delay() {
        let samples = noise();
        let report = fit(&samples, &samples, 48000, CalibrationConfig::default()).unwrap();
        assert!(report.held_out.after_rmse_db < 0.2, "{:?}", report.held_out);
        for (i, &coefficient) in report.profile.coefficients.iter().enumerate() {
            assert!(
                (coefficient
                    - if i == report.profile.delay_samples {
                        1.
                    } else {
                        0.
                    })
                .abs()
                    < 1e-12
            );
        }
    }
    #[test]
    fn held_out_broadband_reference_improves_without_unbounded_gain() {
        let source = noise();
        let reference: Vec<f32> = source
            .iter()
            .enumerate()
            .map(|(i, &x)| 0.8 * x + if i > 0 { 0.45 * source[i - 1] } else { 0. })
            .collect();
        let report = fit(&source, &reference, 48000, CalibrationConfig::default()).unwrap();
        assert!(
            report.held_out.after_rmse_db < report.held_out.before_rmse_db * 0.4,
            "{:?}",
            report.held_out
        );
        assert!(report.profile.coefficients.iter().all(|v| v.is_finite()));
        let encoded = serde_json::to_vec(&report.profile).unwrap();
        let recovered: CalibrationProfile = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            recovered.apply(&source).unwrap(),
            report.profile.apply(&source).unwrap()
        );
    }
    #[test]
    fn silence_nonfinite_and_inadequate_recordings_are_rejected() {
        let valid = noise();
        assert!(
            fit(
                &vec![0.; valid.len()],
                &valid,
                48000,
                CalibrationConfig::default()
            )
            .is_err()
        );
        assert!(
            fit(
                &[f32::NAN; 48000],
                &valid,
                48000,
                CalibrationConfig::default()
            )
            .is_err()
        );
        assert!(fit(&valid[..1000], &valid, 48000, CalibrationConfig::default()).is_err());
    }
}
