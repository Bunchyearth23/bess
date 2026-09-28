//! Reproducible sound-control audition through the application's live renderer.
use bess::{
    bench::BeamNgCamera,
    drive::{Controls, Mode},
    hybrid::Settings,
    project::{Parameters, Project, load_project, save_project},
    realtime::{DenormalGuard, RenderEngine},
    render::write_pcm,
    scratch::{PRESETS, Scratch, SoundTuning},
};
use std::{fmt::Write as _, path::PathBuf};

const RATE: usize = 48_000;
const SECONDS: usize = 8;

fn operating_point(time: f32) -> (f32, f32) {
    if time < 2. {
        (850., 0.05)
    } else if time < 4. {
        (850. + (3500. - 850.) * (time - 2.) / 2., 0.75)
    } else if time < 5. {
        (3500., 0.75)
    } else if time < 7. {
        (3500. - (3500. - 850.) * (time - 5.) / 2., 0.)
    } else {
        (850., 0.05)
    }
}

fn main() -> Result<(), String> {
    let _denormals = DenormalGuard::enter();
    let directory = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("output/physical-sound-controls-20260928/listening"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut metrics = Vec::new();
    let mut readme = String::from(
        "# Trois réglages du même I4\n\n\
         WAV mono PCM24 à 48 kHz, RenderEngine (simulation interne 96 kHz), caméra Orbit, volume 0,8. \
         Aucun ajustement de niveau entre variantes ni normalisation ; limiteur habituel de l'application actif.\n\n\
         Trajectoire identique à régime imposé : 0–2 s à 850 tr/min (papillon 5 %), \
         2–4 s montée à 3500 (75 %), 4–5 s palier (75 %), 5–7 s descente à 850 (0 %, DFCO demandé), \
         7–8 s ralenti (5 %). Le régime est commandé, pas une accélération libre du véhicule.\n\n\
         Chaque JSON est un projet version 3 rechargeable dans BESS, positionné au ralenti initial. \
         La trajectoire de huit secondes se reproduit avec `cargo run --release --example sound_controls`. \
         Les changements de timbre sont dans `scratch.sound` ; moteur, graine et autres paramètres sont identiques.\n\n\
         | Variante | WAV / projet | Réglages | Crête dBFS | RMS dBFS |\n\
         |---|---|---|---:|---:|\n",
    );
    for (name, slug, description, tuning) in [
        (
            "Neutre",
            "neutre",
            "Réglages par défaut",
            SoundTuning::default(),
        ),
        (
            "Grave",
            "grave",
            "Basses +6 dB, aigus −4 dB, sortie 2,5 m",
            SoundTuning {
                bass_db: 6.,
                treble_db: -4.,
                tail_length_m: 2.5,
                ..Default::default()
            },
        ),
        (
            "Mordant",
            "mordant",
            "Présence +5 dB, texture débit 0,6, saturation 0,35, résonance admission 1,5",
            SoundTuning {
                presence_db: 5.,
                flow_texture: 0.6,
                drive: 0.35,
                intake_resonance: 1.5,
                ..Default::default()
            },
        ),
    ] {
        let mut scratch = Scratch {
            design: PRESETS
                .iter()
                .find(|(name, _)| *name == "Inline-4")
                .ok_or("Inline-4 preset missing")?
                .1,
            sound: tuning,
            ..Default::default()
        };
        let mut parameters = Parameters::default();
        let mut settings = Settings::default();
        let mut driving = Controls::default();
        scratch.derive_from_build(&mut settings, &mut parameters, &mut driving);
        scratch.idle_rpm = 850.;
        scratch.sound = tuning;
        parameters.volume = 0.8;
        parameters.rpm = 850.;
        parameters.load = 0.05;
        settings.fuel_cut = 1.;
        driving.mode = Mode::Direct;
        let project = Project {
            version: 3,
            parameters,
            hybrid: settings,
            driving,
            source: None,
            profile_name: name.into(),
            scratch: Some(scratch),
        };
        let project_path = directory.join(format!("{slug}.json"));
        save_project(&project_path, &project)?;
        let restored = load_project(&project_path)?;
        if serde_json::to_value(&project).map_err(|e| e.to_string())?
            != serde_json::to_value(&restored).map_err(|e| e.to_string())?
        {
            return Err(format!("Project did not round-trip: {name}"));
        }
        // Render the actual reloaded project, not a separate reconstruction.
        let mut parameters = restored.parameters;
        let settings = restored.hybrid;
        let driving = restored.driving;
        let mut engine = RenderEngine::scratch(
            RATE as u32,
            parameters,
            settings,
            driving,
            restored
                .scratch
                .as_ref()
                .ok_or("Reloaded project has no engine")?,
        )?;
        engine.bench.set_beamng_camera(BeamNgCamera::Orbit);
        let mut samples = Vec::with_capacity(RATE * SECONDS);
        for frame in 0..RATE * SECONDS {
            if frame.is_multiple_of(RATE / 1000) {
                (parameters.rpm, parameters.load) = operating_point(frame as f32 / RATE as f32);
                engine.bench.set(parameters, settings, driving, 0);
            }
            let value = engine.next_sample(true);
            if !value.is_finite() || value.abs() > 1. || engine.bench.failed() {
                return Err(format!(
                    "Invalid signal/physical failure: {name}, frame {frame}"
                ));
            }
            samples.push(value);
        }
        let energy: f64 = samples.iter().map(|x| f64::from(*x).powi(2)).sum();
        let peak = samples
            .iter()
            .map(|x| f64::from(x.abs()))
            .fold(0., f64::max);
        if energy <= 1e-12 || !energy.is_finite() {
            return Err(format!("Silent/invalid output: {name}"));
        }
        let rms_dbfs = 10. * (energy / samples.len() as f64).log10();
        let peak_dbfs = 20. * peak.log10();
        let wav_path = directory.join(format!("{slug}.wav"));
        write_pcm(&wav_path, &samples)?;
        let mut decoded = hound::WavReader::open(&wav_path).map_err(|e| e.to_string())?;
        let spec = decoded.spec();
        if spec.channels != 1
            || spec.sample_rate != RATE as u32
            || spec.bits_per_sample != 24
            || spec.sample_format != hound::SampleFormat::Int
            || decoded.duration() as usize != RATE * SECONDS
        {
            return Err(format!("Unexpected WAV format/duration: {name}"));
        }
        let mut pcm_energy = 0.;
        for value in decoded.samples::<i32>() {
            pcm_energy += (f64::from(value.map_err(|e| e.to_string())?) / 8388608.).powi(2);
        }
        if pcm_energy <= 1e-12 || (pcm_energy / energy - 1.).abs() > 0.001 {
            return Err(format!("PCM energy verification failed: {name}"));
        }
        metrics.push(serde_json::json!({
            "variant": name, "wav": format!("{slug}.wav"), "project": format!("{slug}.json"),
            "project_version": 3, "sample_rate": RATE, "internal_rate": engine.synth_rate,
            "seconds": SECONDS, "volume": 0.8, "camera": "Orbit", "normalized": false,
            "finite": true, "physical_failed": false, "project_round_trip": true,
            "peak_dbfs": peak_dbfs, "rms_dbfs": rms_dbfs,
            "energy_sum": energy, "pcm_energy_sum": pcm_energy,
        }));
        writeln!(readme, "| {name} | [{slug}.wav]({slug}.wav) / [{slug}.json]({slug}.json) | {description} | {peak_dbfs:.2} | {rms_dbfs:.2} |")
            .map_err(|e| e.to_string())?;
        println!(
            "{name}: peak {peak_dbfs:.2} dBFS, RMS {rms_dbfs:.2} dBFS; finite, PCM24 energy verified, v3 project reloaded"
        );
    }
    readme.push_str("\n`metrics.json` contient les mesures. Les vérifications numériques ne remplacent pas l'écoute ; aucune appréciation subjective du réalisme n'est déduite de ces chiffres.\n");
    std::fs::write(directory.join("README.md"), readme).map_err(|e| e.to_string())?;
    std::fs::write(
        directory.join("metrics.json"),
        serde_json::to_vec_pretty(&metrics).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
