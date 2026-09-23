use crate::{
    bank::Bank,
    bench::Bench,
    drive::{Controls, Mode},
    hybrid::Settings,
};
use crate::{engine::Engine, project::Parameters};
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
    params.validate()?;
    settings.validate()?;
    driving.validate()?;
    if !seconds.is_finite() || !(1.0..=60.0).contains(&seconds) {
        return Err("Duration: 1–60 seconds".into());
    }
    let frames = (seconds * 48000.) as usize;
    let mut engine = Bench::new(48000, params, settings, driving, Some(bank));
    engine.set_cycle_seconds(seconds);
    let mut samples = Vec::with_capacity(frames);
    for i in 0..frames {
        let fade = ((frames - i) as f32 / 2400.).min(1.);
        samples.push(engine.next(true) * fade);
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
    let mode = if settings.procedural {
        "independent procedural synthesis guided by measured characteristics"
    } else {
        "source-guided resynthesis"
    };
    let report = format!(
        "Automation source playback and BESS {mode}, using the same 16-second scenario.\nSource RMS: {:.6}\nBESS RMS: {:.6}\nBESS level adjustment: {:.3} dB\nRMS is not a LUFS measurement. The source is reconstructed from the bank, not recorded from Automation gameplay.\n",
        rms(&a),
        rms(&b),
        20. * gain.log10()
    );
    std::fs::write(dir.join("comparison.txt"), &report).map_err(|e| e.to_string())?;
    Ok(report)
}

/// Three level-matched direct-mode clips at one operating point for source,
/// source-guided and independent procedural listening.
pub fn steady_procedural_comparison(
    dir: &Path,
    params: Parameters,
    bank: Arc<Bank>,
    seconds: f32,
) -> Result<String, String> {
    params.validate()?;
    if !seconds.is_finite() || !(2.0..=20.0).contains(&seconds) {
        return Err("Steady comparison duration must be 2–20 seconds".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let base = Settings {
        level_match: false,
        ..Settings::calibrated(&bank)
    };
    let variants = [
        (
            "01-source-automation.wav",
            Settings {
                enhanced: false,
                ..base
            },
        ),
        ("02-source-guided.wav", base),
        (
            "03-generated.wav",
            Settings {
                procedural: true,
                ..base
            },
        ),
    ];
    let mut rendered = Vec::with_capacity(variants.len());
    let mut target_rms = 0.;
    for (name, settings) in variants {
        let mut samples = hybrid_samples(params, settings, bank.clone(), seconds, false)?;
        let settled = &samples[(samples.len() / 10).max(1)..];
        let mean = settled.iter().map(|x| *x as f64).sum::<f64>() / settled.len() as f64;
        let rms = (settled
            .iter()
            .map(|x| (*x as f64 - mean).powi(2))
            .sum::<f64>()
            / settled.len() as f64)
            .sqrt() as f32;
        if rendered.is_empty() {
            target_rms = rms;
        } else {
            let gain = target_rms / rms.max(1e-9);
            if gain > 8. {
                return Err(format!("{name} is too quiet for a reliable level match"));
            }
            for sample in &mut samples {
                *sample *= gain;
            }
        }
        rendered.push((name, samples));
    }
    let peak = rendered
        .iter()
        .flat_map(|(_, samples)| samples)
        .fold(0f32, |max, sample| max.max(sample.abs()));
    let safety = (0.95 / peak.max(1e-9)).min(1.);
    for (name, mut samples) in rendered {
        for sample in &mut samples {
            *sample *= safety;
        }
        write_pcm(&dir.join(name), &samples)?;
    }
    let report = format!(
        "Steady comparison at {:.0} rpm and {:.2} load, {:.1} s, 48 kHz / PCM24. Clips were matched by settled AC RMS, then shared the same peak safety gain. Equal level does not establish naturalness. The generated clip uses measured descriptors, not Automation PCM playback.\n",
        params.rpm, params.load, seconds
    );
    std::fs::write(dir.join("comparison.txt"), &report).map_err(|e| e.to_string())?;
    Ok(report)
}

/// Same source and trajectory for all characters, matched by integrated RMS.
pub fn characters(dir: &Path, params: Parameters, bank: Arc<Bank>) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let variants = [
        (
            "00-source-automation",
            Settings {
                enhanced: false,
                ..Settings::default()
            },
        ),
        ("01-balanced", Settings::character_for_bank(0, &bank)),
        ("02-muted", Settings::character_for_bank(1, &bank)),
        ("03-open", Settings::character_for_bank(2, &bank)),
        ("04-warm", Settings::character_for_bank(3, &bank)),
        ("05-mechanical", Settings::character_for_bank(4, &bank)),
        ("06-grit", Settings::character_for_bank(5, &bank)),
    ];
    let mut rendered = Vec::new();
    let mut reference = 0.;
    for (name, h) in variants {
        let mut audio = hybrid_samples(params, h, bank.clone(), 16., true)?;
        let rms =
            (audio.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / audio.len() as f64).sqrt();
        if rendered.is_empty() {
            reference = rms;
        }
        let gain = (reference / rms.max(1e-9)) as f32;
        for s in &mut audio {
            *s *= gain;
        }
        rendered.push((name, h, audio));
    }
    let peak = rendered
        .iter()
        .flat_map(|(_, _, a)| a)
        .fold(0f32, |m, s| m.max(s.abs()));
    let safety = (0.95 / peak.max(1e-9)).min(1.);
    for (name, h, mut audio) in rendered {
        for s in &mut audio {
            *s *= safety;
        }
        write_pcm(&dir.join(format!("{name}.wav")), &audio)?;
        crate::project::save_project(
            &dir.join(format!("{name}.bess.json")),
            &crate::project::Project {
                version: 2,
                parameters: params,
                hybrid: h,
                source: Some(bank.source.clone()),
                driving: Controls {
                    mode: Mode::Cycle,
                    ..Default::default()
                },
            },
        )?;
    }
    let report = format!(
        "BESS {} — the same vehicle and 16-second cycle, with six generated characters matched to the source by RMS.\nCommon RMS: {:.6}. Common safety gain: {:.6}.\nThe associated projects retain their settings before final WAV RMS matching.\nA is reconstructed from the source bank, not recorded from gameplay. RMS matching is not LUFS matching.\n",
        env!("CARGO_PKG_VERSION"),
        reference * safety as f64,
        safety
    );
    std::fs::write(dir.join("listening-notes.txt"), &report).map_err(|e| e.to_string())?;
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
        let mut audio = Vec::with_capacity(48000 * 16);
        let mut csv =
            "seconds,rpm,load,speed_kmh,gear,wheel_torque_nm,resistance_nm,shifting\n".to_owned();
        for i in 0..48000 * 16 {
            audio.push(engine.next(true) * ((48000 * 16 - i) as f32 / 2400.).min(1.));
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
            },
        )?;
    }
    Ok("Three 16-second runs at 85% throttle: open road, an additional 900 Nm of resistance at the wheels, and neutral gear. Output level is unchanged, with no WAV normalization. The bench settings are generic and are not identified from the vehicle.".into())
}

pub fn wav(path: &Path, params: Parameters, seconds: f32, sweep: bool) -> Result<(), String> {
    params.validate()?;
    if !seconds.is_finite() || !(1.0..=60.0).contains(&seconds) {
        return Err("Duration: 1 to 60 seconds".into());
    }
    let rate = 48000;
    let frames = (seconds * rate as f32) as usize;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    let mut engine = Engine::new(rate, params);
    for i in 0..frames {
        if sweep && i % 256 == 0 {
            let rpm = params.rpm + (8000. - params.rpm) * i as f32 / frames as f32;
            engine.set_parameters(Parameters { rpm, ..params });
        }
        let fade = ((frames - i) as f32 / (rate as f32 * 0.05)).min(1.);
        let sample = (engine.next_sample(true) * fade * 8_388_607.) as i32;
        writer.write_sample(sample).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
}
