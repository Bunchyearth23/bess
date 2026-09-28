//! Offline listening fixtures through the live scratch render path, without normalization.
use bess::{
    bench::BeamNgCamera,
    drive::{Controls, Mode},
    hybrid::Settings,
    project::Parameters,
    realtime::{DenormalGuard, RenderEngine},
    render::write_pcm,
    scratch::{PRESETS, Scratch},
};
use std::{fmt::Write as _, path::PathBuf};

const RATE: usize = 48_000;
const SECONDS: usize = 12;
const PHASES: [(&str, f32, f32); 6] = [
    ("Ralenti", 0., 2.),
    ("Accélération imposée", 2., 6.),
    ("Palier", 6., 7.),
    ("Lever de pied / DFCO demandé", 7., 9.),
    ("Retour au ralenti", 9., 11.),
    ("Pause et extinction", 11., 12.),
];

fn operating_point(t: f32, idle: f32) -> (f32, f32, bool) {
    if t < 2. {
        (idle, 0.05, true)
    } else if t < 6. {
        (idle + (4500. - idle) * (t - 2.) / 4., 0.85, true)
    } else if t < 7. {
        (4500., 0.85, true)
    } else if t < 9. {
        (4500. + (idle - 4500.) * (t - 7.) / 2., 0., true)
    } else {
        (idle, 0.05, t < 11.)
    }
}

fn levels(samples: &[f32]) -> (f64, f64) {
    let peak = samples
        .iter()
        .fold(0f64, |peak, x| peak.max(x.abs() as f64));
    let power =
        samples.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / samples.len().max(1) as f64;
    (20. * peak.log10(), 10. * power.log10())
}

fn main() -> Result<(), String> {
    let _denormals = DenormalGuard::enter();
    let dir = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from("output/physical-listening-20260928"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut cards = String::new();
    let mut csv = String::from("preset,phase,start_s,end_s,peak_dbfs,rms_dbfs\n");
    let mut metadata = Vec::new();
    for (name, slug) in [
        ("Inline-4", "i4"),
        ("V8 cross-plane", "v8-cross-plane"),
        ("V12 60°", "v12-60"),
    ] {
        let mut scratch = Scratch {
            design: PRESETS
                .iter()
                .find(|(preset, _)| *preset == name)
                .ok_or_else(|| format!("Missing preset {name}"))?
                .1,
            ..Scratch::default()
        };
        let mut params = Parameters {
            volume: 0.8,
            ..Parameters::default()
        };
        let mut settings = Settings::default();
        let mut controls = Controls {
            mode: Mode::Direct,
            ..Controls::default()
        };
        scratch.derive_from_build(&mut settings, &mut params, &mut controls);
        params.rpm = scratch.idle_rpm;
        params.load = 0.05;
        settings.fuel_cut = 1.;
        let initial_params = params;
        let mut engine = RenderEngine::scratch(RATE as u32, params, settings, controls, &scratch)?;
        engine.bench.set_beamng_camera(BeamNgCamera::Orbit);
        let mut samples = Vec::with_capacity(RATE * SECONDS);
        let mut playing = true;
        for frame in 0..RATE * SECONDS {
            if frame.is_multiple_of(RATE / 1000) {
                (params.rpm, params.load, playing) =
                    operating_point(frame as f32 / RATE as f32, scratch.idle_rpm);
                engine.bench.set(params, settings, controls, 0);
            }
            let sample = engine.next_sample(playing);
            if !sample.is_finite() || engine.bench.failed() {
                return Err(format!("Physical render failed: {name}, frame {frame}"));
            }
            samples.push(sample);
        }
        let tail_peak = samples[samples.len() - 480..]
            .iter()
            .fold(0f32, |peak, x| peak.max(x.abs()));
        if tail_peak > 0.0001 {
            return Err(format!("Pause did not fade out {name}: peak {tail_peak}"));
        }
        let wav = format!("{slug}.wav");
        write_pcm(&dir.join(&wav), &samples)?;
        let (peak, rms) = levels(&samples);
        let mut rows = String::new();
        for (label, start, end) in PHASES {
            let (phase_peak, phase_rms) =
                levels(&samples[(start as usize * RATE)..(end as usize * RATE)]);
            writeln!(
                csv,
                "{name},{label},{start},{end},{phase_peak:.2},{phase_rms:.2}"
            )
            .map_err(|e| e.to_string())?;
            write!(rows, "<tr><td>{label}</td><td>{start}–{end} s</td><td>{phase_peak:.2}</td><td>{phase_rms:.2}</td></tr>")
                .map_err(|e| e.to_string())?;
        }
        writeln!(csv, "{name},Clip complet,0,{SECONDS},{peak:.2},{rms:.2}")
            .map_err(|e| e.to_string())?;
        write!(cards, r#"<article><h2>{name}</h2><p>Ralenti : {idle:.0} tr/min · accélération jusqu’à 4 500 tr/min · volume moteur 0,8 · caméra Orbit.</p><audio controls preload="metadata" src="{wav}"></audio><p><a href="{wav}" download>Télécharger le WAV PCM 24 bits</a> · crête {peak:.2} dBFS · RMS {rms:.2} dBFS (clip entier, pause incluse).</p><h3>Mesures par séquence</h3><table><thead><tr><th>Séquence</th><th>Temps</th><th>Crête dBFS</th><th>RMS dBFS</th></tr></thead><tbody>{rows}</tbody></table></article>"#, idle = scratch.idle_rpm)
            .map_err(|e| e.to_string())?;
        metadata.push(serde_json::json!({
            "preset": name, "wav": wav, "sample_rate": RATE, "internal_rate": engine.synth_rate,
            "seconds": SECONDS, "camera": "Orbit", "mode": "Direct / imposed RPM",
            "normalized": false, "physical_failed": false, "finite": true,
            "peak_dbfs": peak, "rms_dbfs": rms, "tail_peak": tail_peak,
            "initial_parameters": initial_params, "settings": settings, "scratch": scratch,
        }));
        println!("{name}: peak {peak:.2} dBFS; RMS {rms:.2} dBFS; finite; no physical fault");
    }
    let html = format!(
        r#"<!doctype html><html lang="fr"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>BESS · écoute du moteur physique</title><style>body{{margin:0;background:#10151d;color:#e9edf3;font:17px/1.6 system-ui,sans-serif}}main{{max-width:960px;margin:auto;padding:40px 24px}}h1{{font-size:2.3rem;line-height:1.15}}h2{{margin-top:0}}.tag{{color:#96dcd3;letter-spacing:.08em;font-size:.8rem;text-transform:uppercase}}article{{margin:24px 0;padding:24px;border:1px solid #344053;border-radius:16px;background:#19212d}}a{{color:#a7d3ff}}audio{{width:100%}}table{{width:100%;text-align:left;border-collapse:collapse;font-size:.9rem}}td,th{{padding:8px;border-bottom:1px solid #344053}}small{{color:#b9c4d2}}summary{{cursor:pointer}}code{{font-size:.85em}}</style><main><p class="tag">BESS · rendu physique · 28 septembre 2026</p><h1>Trois moteurs à écouter</h1><p>Rendus synthétiques à <strong>régime imposé</strong>, exportés avec le même moteur audio que l’écoute de l’application : simulation à 96 kHz, décimation à 48 kHz, WAV mono PCM 24 bits. Volume d’écoute réglé à 0,8, caméra Orbit, construction de série par défaut.</p><p>Chaque clip dure 12 secondes : 0–2 s ralenti (papillon 5 %), 2–6 s accélération progressive vers 4 500 tr/min (85 %), 6–7 s palier, 7–9 s lever de pied (0 %, DFCO activé) avec régime ramené au ralenti, 9–11 s ralenti, 11–12 s pause avec extinction du son. Le régime est commandé ; ces clips ne démontrent pas une accélération libre du véhicule.</p><p>Les niveaux d’origine sont conservés, <strong>sans normalisation</strong>. Le limiteur habituel de l’application reste actif. La mesure DFCO décrit la commande ; cette page n’expose pas de télémétrie de coupure carburant. Le volume du lecteur et du système s’ajoute au réglage exporté.</p>{cards}<p><a href="metrics.csv" download>Mesures CSV</a> · <a href="parameters.json" download>Paramètres complets JSON</a></p><small>Validation automatique : échantillons finis, absence de panne du solveur et extinction finale. Ces vérifications ne constituent ni une appréciation à l’oreille ni une validation du réalisme sonore. Aucune lecture automatique.</small></main></html>"#
    );
    std::fs::write(dir.join("index.html"), html).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("metrics.csv"), csv).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("parameters.json"),
        serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Listening page: {}", dir.join("index.html").display());
    Ok(())
}
