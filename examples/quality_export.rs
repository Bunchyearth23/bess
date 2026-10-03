//! Four-second 1D export-path qualification: load, lift-off fuel cut, recovery.
//! cargo run --release --example quality_export -- <new-output-directory>
use bess::{
    drive::Controls, engine_definition::EngineDefinition, hybrid::Settings, project::Parameters,
    realtime::RenderEngine, scratch::Scratch,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Instant};

fn run(directory: &Path) -> Result<(), String> {
    fs::create_dir(directory).map_err(|e| format!("Choose a new directory: {e}"))?;
    let mut scratch = Scratch::default();
    scratch.experimental.primary_1d = true;
    let definition = EngineDefinition::from_scratch(&scratch);
    let settings = Settings {
        fuel_cut: 1.,
        engine: Some(definition),
        ..Default::default()
    };
    let driving = Controls::default();
    let params = Parameters::default();
    let mut renderer = RenderEngine::scratch(48000, params, settings, driving, &scratch)?;
    let phases = [(1500., 0.25), (3000., 0.8), (3000., 0.), (3000., 0.7)];
    let mut samples = Vec::with_capacity(48000 * 4);
    let mut rows = Vec::new();
    let start = Instant::now();
    for (phase, (rpm, load)) in phases.into_iter().enumerate() {
        renderer.bench.set(
            Parameters {
                rpm,
                load,
                ..params
            },
            settings,
            driving,
            0,
        );
        let (mut peak, mut power, mut cuts) = (0_f32, 0_f64, 0_usize);
        let phase_start = Instant::now();
        for frame in 0..48000 {
            let sample = renderer.next_sample(true);
            if renderer.bench.failed() || !sample.is_finite() {
                return Err(format!(
                    "1D RenderEngine failed at phase {phase}, frame {frame}"
                ));
            }
            let telemetry = renderer
                .bench
                .physical_state()
                .ok_or("Missing physical telemetry")?;
            cuts += usize::from(telemetry.fuel_cut);
            peak = peak.max(sample.abs());
            power += f64::from(sample).powi(2);
            // Same 50 ms final presentation fade as scratch_samples.
            let fade = if phase == 3 {
                ((48000 - frame) as f32 / 2400.).min(1.)
            } else {
                1.
            };
            samples.push(sample * fade);
        }
        if peak <= 1e-8 || power <= 1e-14 {
            return Err(format!("Silent 1D RenderEngine output in phase {phase}"));
        }
        rows.push(
            json!({"start_seconds":phase,"duration_seconds":1,"rpm":rpm,"load":load,
            "peak":peak,"rms":(power/48000.).sqrt(),"fuel_cut_frames":cuts,
            "elapsed_seconds":phase_start.elapsed().as_secs_f64()}),
        );
    }
    let rendering_seconds = start.elapsed().as_secs_f64();
    if rows[2]["fuel_cut_frames"].as_u64().unwrap_or(0) == 0 {
        return Err("Lift-off phase did not exercise physical fuel cut".into());
    }
    if rows[3]["fuel_cut_frames"].as_u64().unwrap_or(48000) > 4800 {
        return Err("Physical fuel cut did not release during recovery".into());
    }
    let path = directory.join("quality-1d-load-cut-recovery.wav");
    bess::render::write_pcm(&path, &samples)?;
    let reader = hound::WavReader::open(&path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if spec.bits_per_sample != 24
        || spec.sample_rate != 48000
        || spec.channels != 1
        || reader.duration() != 192000
    {
        return Err(
            "Rendered WAV format or duration differs from the expected PCM24 output".into(),
        );
    }
    let wav_sha256 = format!(
        "{:x}",
        Sha256::digest(fs::read(&path).map_err(|e| e.to_string())?)
    );
    let report = json!({"kind":"offline_1d_RenderEngine_transient_export_qualification",
        "definition":definition,"settings":settings,"parameters":params,
        "duration_seconds":4,"sample_rate":spec.sample_rate,"synthesis_rate":renderer.synth_rate,
        "bits_per_sample":spec.bits_per_sample,"channels":spec.channels,"wav_sha256":wav_sha256,
        "rendering_seconds":rendering_seconds,"wall_seconds_per_simulated_second":rendering_seconds/4.,
        "phases":rows,"finite":true,"non_silent":true,
        "limits":"One synthetic Inline-4, imposed RPM, default acoustic grid. Includes the same RenderEngine/Bench/listener/decimation used by scratch WAV export, with its normal limiter and final 50ms fade. No extra normalization. Not a game or real-engine recording and not a realtime claim."});
    fs::write(
        directory.join("quality-export.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "PCM24 export: four seconds, finite and non-silent; {rendering_seconds:.3}s elapsed ({:.3} wall seconds per simulated second)",
        rendering_seconds / 4.
    );
    Ok(())
}

fn main() {
    let Some(directory) = std::env::args().nth(1) else {
        eprintln!("usage: quality_export <new-output-directory>");
        std::process::exit(2);
    };
    if let Err(error) = run(Path::new(&directory)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
