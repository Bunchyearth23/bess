//! Synthetic true free-crank cold/hot transients, no imposed RPM at any point.
//! cargo run --example fuel_startup -- <directory> [48000|96000|192000]
use bess::{
    physical::{
        combustion_model::PredictiveCombustionConfig,
        engine::{Commands, Engine, StartupThermalState},
        fuel_transient::FuelTransientConfig,
    },
    scratch::Scratch,
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

fn run(dir: &Path, rate: u32, cold: bool) -> Result<(), Box<dyn Error>> {
    let name = if cold { "cold" } else { "hot" };
    let mut scratch = Scratch::default();
    scratch.experimental.fuel_transients = Some(FuelTransientConfig::default());
    scratch.experimental.predictive_combustion = Some(PredictiveCombustionConfig::default());
    let path = dir.join(format!("{name}-{rate}.bess.json"));
    bess::project::save_project(
        &path,
        &bess::project::Project {
            version: 4,
            parameters: Default::default(),
            hybrid: Default::default(),
            source: None,
            driving: Default::default(),
            profile_name: format!("Synthetic free startup {name}"),
            scratch: Some(scratch.clone()),
        },
    )?;
    let saved = bess::project::load_project(&path)?
        .scratch
        .ok_or("lost scratch")?;
    if saved != scratch {
        return Err("project roundtrip mismatch".into());
    }
    let mut engine = Engine::new(&saved, rate)?;
    let thermal = if cold {
        StartupThermalState::uniform(280.)
    } else {
        StartupThermalState {
            gas_temperature_k: 300.,
            cylinder_wall_temperature_k: 450.,
            coolant_temperature_k: 360.,
            aftertreatment_temperature_k: 450.,
        }
    };
    engine.initialize_stopped_with_thermal(thermal)?;
    let initial = engine.transient_diagnostics().ok_or("missing observer")?;
    let mut csv = BufWriter::new(File::create(dir.join(format!("{name}-{rate}.csv")))?);
    writeln!(
        csv,
        "synthetic_seconds,phase,rpm,starter_nm,throttle,load_nm,map_pa,fuel_cut,heat_j,injected_kg,burned_kg,film_kg,droplets_kg,vapor_kg,latent_j,gas_min_k,gas_max_k,wall_mean_k,fuel_residual_kg,energy_residual_j,gas_correction_abs_j,active_gas_correction_abs_j"
    )?;
    let mut wav = hound::WavWriter::create(
        dir.join(format!("{name}-{rate}.wav")),
        hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    let (mut fuel, mut energy) = (0., initial.cylinder_stored_energy_j);
    let (mut heat, mut injected, mut latent, mut correction, mut active_correction) =
        (0., 0., 0., 0., 0.);
    let (mut max_fuel, mut max_energy, mut max_gas, mut peak_rpm, mut peak_audio) =
        (0_f64, 0_f64, 0_f64, 0_f64, 0_f32);
    let (mut dfco_frames, mut restart_heat) = (0_u64, 0.);
    let (mut numerical_energy, mut wall_correction) = (0., 0.);
    let mut phase_corrections = [0.; 7];
    let mut phase_rpm_min = [f64::INFINITY; 7];
    let mut correction_csv = BufWriter::new(File::create(
        dir.join(format!("{name}-{rate}-corrections.csv")),
    )?);
    writeln!(
        correction_csv,
        "synthetic_seconds,phase,rpm,minimum_gas_k,injection_kg,latent_j,gas_correction_j,gas_correction_after_injection_j,wall_correction_j"
    )?;
    let mut checkpoints = Vec::new();
    for frame in 0..rate * 8 {
        let t = f64::from(frame) / f64::from(rate);
        let (phase, throttle, starter, load, overrun) = if t < 1.2 {
            ("start", 0.2, true, 0., 1.)
        } else if t < 3. {
            ("tip-in", 0.75, false, 0., 1.)
        } else if t < 4. {
            ("closed", 0., false, 0., 1.)
        } else if t < 4.8 {
            ("resume", 0.4, false, 0., 1.)
        } else if t < 5.8 {
            ("stop-load", 0., false, 300., 0.)
        } else if t < 7. {
            ("restart", 0.2, true, 0., 1.)
        } else {
            ("release", 0.2, false, 0., 1.)
        };
        let s = engine.next(Commands {
            imposed_rpm: None,
            throttle,
            starter,
            load_nm: load,
            overrun,
            ..Default::default()
        });
        if engine.failed() {
            return Err(format!("{name}/{rate} failed at {t}").into());
        }
        let d = engine.transient_diagnostics().ok_or("observer lost")?;
        let l = d.ledger;
        let phase_index = match phase {
            "start" => 0,
            "tip-in" => 1,
            "closed" => 2,
            "resume" => 3,
            "stop-load" => 4,
            "restart" => 5,
            _ => 6,
        };
        phase_corrections[phase_index] += l.gas_correction_abs_j;
        phase_rpm_min[phase_index] = phase_rpm_min[phase_index].min(s.rpm);
        numerical_energy += l.numerical_energy_added_j;
        wall_correction += l.wall_correction_abs_j;
        if l.gas_correction_abs_j > 1e-8 || l.wall_correction_abs_j > 1e-8 {
            writeln!(
                correction_csv,
                "{t:.9},{phase},{:.9},{:.9},{:.12},{:.12},{:.12},{:.12},{:.12}",
                s.rpm,
                d.cylinder_temperature_min_k,
                l.injected_fuel_kg,
                l.latent_heat_j,
                l.gas_correction_abs_j,
                l.gas_correction_after_injection_abs_j,
                l.wall_correction_abs_j
            )?;
        }
        fuel += l.injected_fuel_kg + l.fuel_boundary_net_kg - l.fuel_burned_kg;
        energy += l.combined_energy_input_j;
        let fr = fuel - d.fuel_vapor_kg - d.film_kg - d.droplets_kg;
        let er = energy - d.cylinder_stored_energy_j;
        max_fuel = max_fuel.max(fr.abs());
        max_energy = max_energy.max(er.abs());
        max_gas = max_gas.max(l.gas_residual_max_j);
        heat += s.heat_j;
        injected += l.injected_fuel_kg;
        latent += l.latent_heat_j;
        correction += l.gas_correction_abs_j;
        active_correction += l.gas_correction_after_injection_abs_j;
        peak_rpm = peak_rpm.max(s.rpm);
        dfco_frames += u64::from(s.fuel_cut);
        if phase == "restart" {
            restart_heat += s.heat_j;
        }
        let audio = 0.25 * (s.exhaust + s.intake + s.mechanical);
        if !audio.is_finite() {
            return Err("nonfinite audio".into());
        }
        peak_audio = peak_audio.max(audio.abs());
        wav.write_sample(audio)?;
        if (frame + 1).is_multiple_of(rate) {
            checkpoints.push(s.rpm);
        }
        if frame.is_multiple_of((rate / 1000).max(1)) {
            writeln!(
                csv,
                "{t:.6},{phase},{:.6},{:.6},{throttle},{load},{:.6},{},{:.9},{:.12},{:.12},{:.12},{:.12},{:.12},{:.9},{:.6},{:.6},{:.6},{fr:.15},{er:.12},{:.12},{:.12}",
                s.rpm,
                d.starter_nm,
                s.map_pa,
                s.fuel_cut,
                s.heat_j,
                l.injected_fuel_kg,
                l.fuel_burned_kg,
                d.film_kg,
                d.droplets_kg,
                d.fuel_vapor_kg,
                l.latent_heat_j,
                d.cylinder_temperature_min_k,
                d.cylinder_temperature_max_k,
                d.cylinder_wall_mean_k,
                l.gas_correction_abs_j,
                l.gas_correction_after_injection_abs_j
            )?;
        }
    }
    csv.flush()?;
    correction_csv.flush()?;
    wav.finalize()?;
    let summary = format!(
        "SYNTHETIC {name}/{rate}: initial gas={}K wall={}K coolant={}K treatment={}K; no imposed RPM; peak_rpm={peak_rpm:.6} checkpoints={checkpoints:?} heat={heat:.9}J injected={injected:.12}kg latent={latent:.9}J DFCO_seconds={:.6} restart_heat={restart_heat:.9}J fuel_residual_max={max_fuel:.3e}kg energy_residual_max={max_energy:.3e}J gas_residual_max={max_gas:.3e}J gas_correction_abs={correction:.9}J active_gas_correction_abs={active_correction:.9}J numerical_energy_added={numerical_energy:.9}J wall_correction_abs={wall_correction:.9}J phase_gas_corrections={phase_corrections:?} phase_min_rpm={phase_rpm_min:?} audio_peak={peak_audio:.6}\n",
        thermal.gas_temperature_k,
        thermal.cylinder_wall_temperature_k,
        thermal.coolant_temperature_k,
        thermal.aftertreatment_temperature_k,
        dfco_frames as f64 / f64::from(rate)
    );
    print!("{summary}");
    fs::write(dir.join(format!("{name}-{rate}.txt")), summary)?;
    if max_fuel > 1e-11 || max_energy > 1e-4 || max_gas > 1e-7 {
        return Err("cylinder conservation ledger failed".into());
    }
    if peak_audio > 1. {
        return Err("common audio observation gain clips".into());
    }
    // Corrections are explicitly reported, never silently treated as measured heat.
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let dir = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "artifacts/simulation-startup".into());
    let rate = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(48000);
    if ![48000, 96000, 192000].contains(&rate) {
        return Err("expected rate48/96/192k".into());
    }
    fs::create_dir_all(&dir)?;
    let result = std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            for cold in [true, false] {
                run(Path::new(&dir), rate, cold).map_err(|e| e.to_string())?;
            }
            Ok::<_, String>(())
        })?
        .join()
        .map_err(|_| "simulation worker panicked")?;
    result.map_err(Into::into)
}
