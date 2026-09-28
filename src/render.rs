use crate::project::Parameters;
use crate::{
    bank::Bank,
    bench::Bench,
    drive::{Controls, Mode},
    hybrid::Settings,
};
use std::path::Path;
use std::sync::Arc;

pub fn hybrid_wav(
    path: &Path,
    params: Parameters,
    settings: Settings,
    bank: Arc<Bank>,
    seconds: f32,
    drive: bool,
) -> Result<(), String> {
    let audio = hybrid_samples(params, settings, bank, seconds, drive)?;
    write_pcm(path, &audio)
}
pub fn hybrid_samples(
    params: Parameters,
    settings: Settings,
    bank: Arc<Bank>,
    seconds: f32,
    drive: bool,
) -> Result<Vec<f32>, String> {
    bench_samples(
        params,
        settings,
        bank,
        seconds,
        Controls {
            mode: if drive { Mode::Cycle } else { Mode::Direct },
            ..Default::default()
        },
    )
}
pub fn bench_wav(
    path: &Path,
    params: Parameters,
    settings: Settings,
    bank: Arc<Bank>,
    seconds: f32,
    driving: Controls,
) -> Result<(), String> {
    write_pcm(
        path,
        &bench_samples(params, settings, bank, seconds, driving)?,
    )
}
pub fn bench_samples(
    params: Parameters,
    settings: Settings,
    bank: Arc<Bank>,
    seconds: f32,
    driving: Controls,
) -> Result<Vec<f32>, String> {
    render_bench(params, settings, seconds, driving, |params| {
        Ok(Bench::new(48000, params, settings, driving, Some(bank)))
    })
}
/// WAV of an engine designed from scratch, with the same transport as listening.
pub fn scratch_wav(
    path: &Path,
    params: Parameters,
    settings: Settings,
    scratch: &crate::scratch::Scratch,
    seconds: f32,
    driving: Controls,
) -> Result<(), String> {
    let samples = scratch_samples(params, settings, scratch, seconds, driving)?;
    write_pcm(path, &samples)
}
/// Scratch uses exactly the same 2x synthesis and decimation as live audio.
/// The final 50 ms file fade is presentation only, applied after that shared path.
pub fn scratch_samples(
    params: Parameters,
    settings: Settings,
    scratch: &crate::scratch::Scratch,
    seconds: f32,
    driving: Controls,
) -> Result<Vec<f32>, String> {
    let _denormals = crate::realtime::DenormalGuard::enter();
    if !seconds.is_finite() || !(1.0..=60.0).contains(&seconds) {
        return Err("Duration: 1–60 seconds".into());
    }
    let mut engine =
        crate::realtime::RenderEngine::scratch(48000, params, settings, driving, scratch)?;
    engine.bench.set_cycle_seconds(seconds);
    let frames = (seconds * 48000.) as usize;
    let mut samples = Vec::with_capacity(frames);
    for i in 0..frames {
        samples.push(engine.next_sample(true) * ((frames - i) as f32 / 2400.).min(1.));
        if engine.bench.failed() {
            return Err("Physical engine could not render this operating point".into());
        }
    }
    Ok(samples)
}
fn render_bench(
    params: Parameters,
    settings: Settings,
    seconds: f32,
    driving: Controls,
    make: impl FnOnce(Parameters) -> Result<Bench, String>,
) -> Result<Vec<f32>, String> {
    params.validate()?;
    settings.validate()?;
    driving.validate()?;
    if !seconds.is_finite() || !(1.0..=60.0).contains(&seconds) {
        return Err("Duration: 1–60 seconds".into());
    }
    let frames = (seconds * 48000.) as usize;
    let mut engine = make(params)?;
    if settings.enhanced
        && let Some(error) = engine.initialization_error()
    {
        return Err(error.to_owned());
    }
    engine.set_cycle_seconds(seconds);
    let mut samples = Vec::with_capacity(frames);
    for i in 0..frames {
        let fade = ((frames - i) as f32 / 2400.).min(1.);
        samples.push(engine.next(true) * fade);
        if engine.failed() {
            return Err("Physical engine could not render this operating point".into());
        }
    }
    Ok(samples)
}
pub fn write_pcm(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &s in samples {
        if !s.is_finite() || s.abs() > 1. {
            return Err("Invalid output signal".into());
        }
        writer
            .write_sample((s * 8388607.) as i32)
            .map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
}
pub fn comparison(
    dir: &Path,
    params: Parameters,
    settings: Settings,
    bank: Arc<Bank>,
) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let a = hybrid_samples(
        params,
        Settings {
            enhanced: false,
            ..settings
        },
        bank.clone(),
        16.,
        true,
    )?;
    let mut b = hybrid_samples(
        params,
        Settings {
            enhanced: true,
            ..settings
        },
        bank,
        16.,
        true,
    )?;
    let rms =
        |v: &[f32]| (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
    let (ra, rb) = (rms(&a), rms(&b));
    let gain = (ra / rb.max(1e-9)) as f32;
    for s in &mut b {
        *s *= gain;
    }
    // Same safety gain applied to both recordings so RMS matching is retained.
    let peak = a.iter().chain(&b).fold(0f32, |m, s| m.max(s.abs()));
    let safe = (0.95 / peak.max(1e-9)).min(1.);
    let a: Vec<f32> = a.iter().map(|s| s * safe).collect();
    for s in &mut b {
        *s *= safe;
    }
    write_pcm(&dir.join("01-source-automation.wav"), &a)?;
    write_pcm(&dir.join("02-bess-enhanced.wav"), &b)?;
    let mode = "physical engine resynthesis";
    let report = format!(
        "Automation source playback and BESS {mode}, using the same 16-second scenario.\nSource RMS: {:.6}\nBESS RMS: {:.6}\nBESS level adjustment: {:.3} dB\nRMS is not a LUFS measurement. The source is reconstructed from the bank, not recorded from Automation gameplay.\n",
        rms(&a),
        rms(&b),
        20. * gain.log10()
    );
    std::fs::write(dir.join("comparison.txt"), &report).map_err(|e| e.to_string())?;
    Ok(report)
}

/// Reproducible listening examples and their actual audio-transport telemetry.
pub fn drive_demo(dir: &Path, params: Parameters, bank: Arc<Bank>) -> Result<String, String> {
    let settings = Settings::calibrated(&bank);
    use std::fmt::Write;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let base = Controls {
        mode: Mode::Simulated,
        throttle: 0.85,
        ..Default::default()
    };
    for (name, c) in [
        ("01-open-road", base),
        (
            "02-load-900Nm",
            Controls {
                resistance_nm: 900.,
                ..base
            },
        ),
        (
            "03-neutral",
            Controls {
                automatic: false,
                gear: 0,
                ..base
            },
        ),
    ] {
        let mut engine = Bench::new(48000, params, settings, c, Some(bank.clone()));
        if let Some(error) = engine.initialization_error() {
            return Err(error.to_owned());
        }
        let mut audio = Vec::with_capacity(48000 * 16);
        let mut csv =
            "seconds,rpm,load,speed_kmh,gear,wheel_torque_nm,resistance_nm,shifting\n".to_owned();
        for i in 0..48000 * 16 {
            audio.push(engine.next(true) * ((48000 * 16 - i) as f32 / 2400.).min(1.));
            if engine.failed() {
                return Err("Physical simulation failed".into());
            }
            if i % 2400 == 0 {
                let s = engine.state();
                writeln!(
                    csv,
                    "{:.3},{:.3},{:.4},{:.3},{},{:.3},{:.3},{}",
                    i as f32 / 48000.,
                    s.rpm,
                    s.load,
                    s.speed_kmh,
                    s.gear,
                    s.wheel_torque,
                    s.resisting_torque,
                    u8::from(s.shifting)
                )
                .unwrap();
            }
        }
        write_pcm(&dir.join(format!("{name}.wav")), &audio)?;
        std::fs::write(dir.join(format!("{name}.csv")), csv).map_err(|e| e.to_string())?;
        crate::project::save_project(
            &dir.join(format!("{name}.bess.json")),
            &crate::project::Project {
                version: 3,
                parameters: params,
                hybrid: settings,
                source: Some(bank.source.clone()),
                driving: c,
                profile_name: crate::project::default_profile_name(),
                scratch: None,
            },
        )?;
    }
    Ok("Three 16-second runs at 85% throttle: open road, an additional 900 Nm of resistance at the wheels, and neutral gear. Output level is unchanged, with no WAV normalization. The bench settings are generic and are not identified from the vehicle.".into())
}

pub fn wav(path: &Path, params: Parameters, seconds: f32, sweep: bool) -> Result<(), String> {
    scratch_wav(
        path,
        params,
        Settings::default(),
        &crate::scratch::Scratch::default(),
        seconds,
        Controls {
            mode: if sweep { Mode::Cycle } else { Mode::Direct },
            ..Default::default()
        },
    )
}
