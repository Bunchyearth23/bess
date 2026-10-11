//! Reproducible P5/P6 demonstration, entirely synthetic. No audio hardware.
use bess::{
    engine_build::{Aspiration, BlowOff},
    physical::{
        crank::TorsionConfig,
        engine::{Commands, Engine, MechanicalProbe},
        structural::StructuralResponse,
        turbo_map::TurboModel,
    },
    scratch::Scratch,
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn Error>> {
    let _denormals = bess::realtime::DenormalGuard::enter();
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "work/simulation-p5-p6".into()),
    );
    fs::create_dir_all(&dir)?;
    let turbo = TurboModel::synthetic_example();
    let response = StructuralResponse::synthetic_example();
    fs::write(dir.join("synthetic-turbo.json"), turbo.to_import_json()?)?;
    fs::write(
        dir.join("synthetic-structural.json"),
        response.to_import_json()?,
    )?;
    fs::write(
        dir.join("estimated-torsion.json"),
        serde_json::to_string_pretty(&TorsionConfig::default())?,
    )?;
    let mut summary = Vec::new();
    for enhanced in [false, true] {
        let name = if enhanced { "mapped" } else { "baseline" };
        let mut settings = Scratch::default();
        settings.build.aspiration = Aspiration::Turbo;
        settings.build.blow_off = BlowOff::Atmospheric;
        if enhanced {
            settings.experimental.turbo_model = Some(turbo);
            settings.experimental.torsion = Some(TorsionConfig::default());
            settings.experimental.mechanical_transfer = Some(response);
        }
        let rate = 48000;
        let mut engine = Engine::new(&settings, rate)?;
        let mut probe = MechanicalProbe::new(&engine);
        let mut csv = BufWriter::new(File::create(dir.join(format!("{name}.csv")))?);
        writeln!(
            csv,
            "seconds,rpm,map_pa,charge_pa,turbo_rpm,torque_nm,exhaust,intake,mechanical,contacts,combustion,slap,flywheel_rpm,twist_rad,spring_energy_j,substep_damping_loss_j,substep_stop_loss_j,turbo_outside_map,substep_charge_wall_heat_j"
        )?;
        let wav_spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut wav =
            hound::WavWriter::create(dir.join(format!("{name}-mechanical.wav")), wav_spec)?;
        let mut isolated: [hound::WavWriter<BufWriter<File>>; 3] = [
            hound::WavWriter::create(dir.join(format!("{name}-contacts.wav")), wav_spec)?,
            hound::WavWriter::create(dir.join(format!("{name}-combustion.wav")), wav_spec)?,
            hound::WavWriter::create(dir.join(format!("{name}-slap.wav")), wav_spec)?,
        ];
        let mut mix = hound::WavWriter::create(dir.join(format!("{name}-mix.wav")), wav_spec)?;
        let mut peak_charge: f64 = 0.0;
        let mut peak_shaft: f64 = 0.0;
        let mut mech_energy = 0.0;
        let start = std::time::Instant::now();
        for i in 0..rate * 2 {
            let t = f64::from(i) / f64::from(rate);
            let s = engine.next(Commands {
                imposed_rpm: if t < 1.0 { Some(3000.0) } else { None },
                throttle: if t < 0.25 {
                    0.2
                } else if t < 1.5 {
                    0.8
                } else {
                    0.0
                },
                load_nm: if t >= 1.0 { 35.0 } else { 0.0 },
                ..Default::default()
            });
            if engine.failed() {
                return Err(format!("{name} failed at {t}s").into());
            }
            let sources = probe.next(engine.mechanical_excitation());
            wav.write_sample(s.mechanical)?;
            for (writer, sample) in isolated.iter_mut().zip(sources) {
                writer.write_sample(sample)?;
            }
            // Fixed common presentation gains; no automatic level matching.
            mix.write_sample(s.exhaust * 0.7 + s.intake * 0.3 + s.mechanical * 0.0)?;
            peak_charge = peak_charge.max(s.charge_pa);
            peak_shaft = peak_shaft.max(s.turbo_rpm);
            mech_energy += f64::from(s.mechanical).powi(2);
            if i % 48 == 0 {
                let torsion = engine.torsion_ledger();
                let turbo = engine.turbo_step().unwrap_or_default();
                writeln!(
                    csv,
                    "{t},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    s.rpm,
                    s.map_pa,
                    s.charge_pa,
                    s.turbo_rpm,
                    s.torque_nm,
                    s.exhaust,
                    s.intake,
                    s.mechanical,
                    sources[0],
                    sources[1],
                    sources[2],
                    torsion.flywheel_rpm,
                    torsion.twist_rad,
                    torsion.spring_energy_j,
                    torsion.damping_loss_j,
                    torsion.stop_loss_j,
                    turbo.map_outside_domain,
                    turbo.charge_wall_heat_j
                )?;
            }
        }
        wav.finalize()?;
        for writer in isolated {
            writer.finalize()?;
        }
        mix.finalize()?;
        let raw_rms = (mech_energy / f64::from(rate * 2)).sqrt();
        // A separate, explicit offline listening copy. Raw outputs/physics stay
        // auditable; this gain is never installed in a project or live voice.
        let source = hound::WavReader::open(dir.join(format!("{name}-mechanical.wav")))?;
        let raw = source
            .into_samples::<f32>()
            .collect::<Result<Vec<_>, _>>()?;
        let peak = raw.iter().copied().map(f32::abs).fold(0.0_f32, f32::max);
        let listening_gain = (0.05 / raw_rms.max(1e-20)).min(0.9 / f64::from(peak).max(1e-20));
        for stem in ["mechanical", "contacts", "combustion", "slap"] {
            let source = hound::WavReader::open(dir.join(format!("{name}-{stem}.wav")))?;
            let mut audition = hound::WavWriter::create(
                dir.join(format!("{name}-{stem}-audition.wav")),
                wav_spec,
            )?;
            for sample in source.into_samples::<f32>() {
                audition.write_sample((f64::from(sample?) * listening_gain) as f32)?;
            }
            audition.finalize()?;
        }
        summary.push(serde_json::json!({"case":name,"build_profile":if cfg!(debug_assertions) {"dev"}else{"release"},"measured":false,"seed":settings.life().seed,"rate":rate,"seconds":2.0,"render_seconds":start.elapsed().as_secs_f64(),"mechanical_listening_gain":listening_gain,"audition_mechanical_rms":raw_rms*listening_gain,"peak_charge_pa":peak_charge,"peak_turbo_rpm":peak_shaft,"mechanical_rms":raw_rms,"default_mix_mechanical_gain":0.0}));
    }
    fs::write(
        dir.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    println!("Synthetic turbo/mechanical demo saved to {}", dir.display());
    Ok(())
}
