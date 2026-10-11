//! Reproducible SYNTHETIC W-012/W-013 comparison. No audio device required.
//! cargo run --release --example fuel_combustion -- <output-dir>
//! Equal fixed observation gain, identical commands/seeds and saved Scratch
//! definitions. Cylinder CSVs additionally expose cold/hot liquid ledgers.
use bess::{
    physical::{
        combustion_model::PredictiveCombustionConfig,
        config::CylinderConfig,
        cycle::{CycleCylinder, CycleInput, Reservoir},
        engine::{Commands, Engine},
        fuel_transient::FuelTransientConfig,
    },
    scratch::Scratch,
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

fn engine_demo(dir: &Path, name: &str, fuel: bool, flame: bool) -> Result<(), Box<dyn Error>> {
    let mut scratch = Scratch::default();
    scratch.experimental.fuel_transients = fuel.then(FuelTransientConfig::default);
    scratch.experimental.predictive_combustion = flame.then(PredictiveCombustionConfig::default);
    fs::write(
        dir.join(format!("{name}.scratch.json")),
        serde_json::to_vec_pretty(&scratch)?,
    )?;
    let project_path = dir.join(format!("{name}.bess.json"));
    bess::project::save_project(
        &project_path,
        &bess::project::Project {
            version: 4,
            parameters: Default::default(),
            hybrid: Default::default(),
            source: None,
            driving: Default::default(),
            profile_name: format!("Synthetic {name}"),
            scratch: Some(scratch.clone()),
        },
    )?;
    let reloaded = bess::project::load_project(&project_path)?;
    let loaded_scratch = reloaded.scratch.ok_or("saved free-engine origin lost")?;
    if loaded_scratch.experimental != scratch.experimental {
        return Err("experimental options changed during project save/reload".into());
    }
    // Render the actual reloaded project definition, not an unsaved copy.
    scratch = loaded_scratch;
    let mut engine = Engine::new(&scratch, 48000)?;
    let mut wav = hound::WavWriter::create(
        dir.join(format!("{name}.wav")),
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    let mut csv = BufWriter::new(File::create(dir.join(format!("{name}.csv")))?);
    writeln!(
        csv,
        "synthetic_seconds,rpm,throttle,map_pa,torque_nm,heat_j,fuel_cut,misfires,correction_j"
    )?;
    let mut heat = 0.;
    let mut correction = 0.;
    let mut injected = 0.;
    let mut peak = 0_f32;
    for frame in 0_u32..57600 {
        let t = f64::from(frame) / 48000.;
        let rpm = if t < 0.4 { 1500. } else { 3500. };
        let throttle = if t < 0.2 {
            0.15
        } else if t < 0.6 {
            0.7
        } else if t < 0.9 {
            0.
        } else {
            0.45
        };
        let sample = engine.next(Commands {
            imposed_rpm: Some(rpm),
            throttle,
            ..Default::default()
        });
        if engine.failed() {
            return Err(format!("{name}: engine failed at {t}").into());
        }
        heat += sample.heat_j;
        correction += sample.correction_j.abs();
        injected += sample.fuel_injected_kg;
        let audio = 0.25 * (sample.exhaust + sample.intake + sample.mechanical);
        peak = peak.max(audio.abs());
        if !audio.is_finite() {
            return Err(format!("{name}: nonfinite output").into());
        }
        wav.write_sample(audio)?;
        if frame.is_multiple_of(48) {
            writeln!(
                csv,
                "{t:.6},{:.4},{throttle:.4},{:.6},{:.6},{:.9},{},{},{:.12}",
                sample.rpm,
                sample.map_pa,
                sample.torque_nm,
                sample.heat_j,
                sample.fuel_cut,
                sample.misfires,
                sample.correction_j
            )?;
        }
    }
    wav.finalize()?;
    csv.flush()?;
    println!(
        "{name}: synthetic heat={heat:.6} J injected={injected:.9} kg correction_abs={correction:.9} J peak={peak:.6}"
    );
    if peak > 1. {
        return Err(format!(
            "{name}: fixed-gain WAV exceeds unity; reduce the COMMON gain for all variants"
        )
        .into());
    }
    Ok(())
}

fn cylinder_demo(dir: &Path, temperature: f64) -> Result<(), Box<dyn Error>> {
    let mut config = CylinderConfig::from_build(&Default::default())?;
    config.wall_temperature_k = temperature;
    let mut cylinder = CycleCylinder::new(config, 0., 123).map_err(|e| format!("{e:?}"))?;
    cylinder
        .set_fuel_transient(Some(FuelTransientConfig::default()))
        .map_err(|e| format!("{e:?}"))?;
    cylinder
        .set_predictive_combustion(Some(PredictiveCombustionConfig::default()))
        .map_err(|e| format!("{e:?}"))?;
    let mut csv = BufWriter::new(File::create(
        dir.join(format!("cylinder-{temperature:.0}K.csv")),
    )?);
    writeln!(
        csv,
        "synthetic_seconds,angle_deg,pressure_pa,temperature_k,torque_nm,lambda,heat_j,injector_kg,vapor_added_kg,film_kg,droplets_kg,latent_j,fuel_balance_kg,fuel_transfer_energy_residual_j,gas_residual_j,gas_correction_j"
    )?;
    let (mut balance, mut max_fuel_residual, mut max_transfer_residual, mut max_gas_residual) =
        (0., 0_f64, 0_f64, 0_f64);
    let (mut injected, mut burned, mut latent) = (0., 0., 0.);
    for frame in 1_u32..=115200 {
        let t = f64::from(frame) / 96000.;
        let fuel_multiplier = if (0.6..0.9).contains(&t) { 0. } else { 1. };
        let angle = t * 1500. * std::f64::consts::TAU / 60.;
        let out = cylinder
            .step(CycleInput {
                angle_rad: angle,
                fuel_multiplier,
                intake: Reservoir {
                    temperature_k: temperature,
                    pressure_pa: if t < 0.2 { 45000. } else { 80000. },
                },
                ..Default::default()
            })
            .map_err(|e| format!("cylinder-{temperature} at {t}: {e:?}"))?;
        balance +=
            out.intake_fuel_kg - out.exhaust_fuel_kg + out.injected_fuel_kg - out.fuel_burned_kg;
        let residual =
            balance - out.fuel_mass_kg - out.fuel_transfer.film_kg - out.fuel_transfer.droplets_kg;
        max_fuel_residual = max_fuel_residual.max(residual.abs());
        max_transfer_residual =
            max_transfer_residual.max(out.fuel_transfer.energy_residual_j().abs());
        max_gas_residual = max_gas_residual.max(out.ledger.residual_j().abs());
        injected += out.injected_fuel_kg;
        burned += out.fuel_burned_kg;
        latent += out.fuel_transfer.latent_heat_j;
        if frame.is_multiple_of(48) {
            writeln!(
                csv,
                "{t:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.9},{:.15},{:.15},{:.15},{:.15},{:.12},{:.15},{:.15},{:.15},{:.12}",
                angle.to_degrees(),
                out.pressure_pa,
                out.temperature_k,
                out.gas_torque_nm,
                out.fresh_air_mass_kg / (14.7 * out.fuel_mass_kg.max(1e-15)),
                out.heat_j,
                out.injected_fuel_kg,
                out.gas_fuel_added_kg,
                out.fuel_transfer.film_kg,
                out.fuel_transfer.droplets_kg,
                out.fuel_transfer.latent_heat_j,
                residual,
                out.fuel_transfer.energy_residual_j(),
                out.ledger.residual_j(),
                out.ledger.numerical_correction_j
            )?;
        }
    }
    csv.flush()?;
    println!(
        "cylinder-{temperature}K: injected={injected:.9} kg burned={burned:.9} kg latent={latent:.6} J max_fuel_residual={max_fuel_residual:.3e} kg max_transfer_energy={max_transfer_residual:.3e} J max_gas_residual={max_gas_residual:.3e} J"
    );
    Ok(())
}

fn generate() -> Result<(), Box<dyn Error>> {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/simulation-p3-p4"));
    fs::create_dir_all(&dir)?;
    for (name, fuel, flame) in [
        ("legacy", false, false),
        ("fuel", true, false),
        ("flame", false, true),
        ("fuel-flame", true, true),
    ] {
        correction_audit(&dir, name, fuel, flame)?;
    }
    for (name, fuel, flame) in [
        ("legacy", false, false),
        ("fuel", true, false),
        ("flame", false, true),
        ("fuel-flame", true, true),
    ] {
        engine_demo(&dir, name, fuel, flame)?;
    }
    cylinder_demo(&dir, 280.)?;
    cylinder_demo(&dir, 450.)?;
    Ok(())
}

fn correction_audit(dir: &Path, name: &str, fuel: bool, flame: bool) -> Result<(), Box<dyn Error>> {
    let config = CylinderConfig::from_build(&Default::default())?;
    let mut cylinder = CycleCylinder::new(config, 0., 123).map_err(|e| format!("{e:?}"))?;
    cylinder
        .set_fuel_transient(fuel.then(FuelTransientConfig::default))
        .map_err(|e| format!("{e:?}"))?;
    cylinder
        .set_predictive_combustion(flame.then(PredictiveCombustionConfig::default))
        .map_err(|e| format!("{e:?}"))?;
    let mut csv = BufWriter::new(File::create(dir.join(format!("{name}-corrections.csv")))?);
    writeln!(
        csv,
        "synthetic_seconds,angle_deg,previous_k,temperature_k,gas_correction_j,wall_correction_j,heat_j,latent_j,wall_heat_j"
    )?;
    let (mut correction, mut heat, mut cold, mut hot, mut latent) = (0., 0., 0_u32, 0_u32, 0.);
    for frame in 1..=48000 {
        let previous = cylinder.gas_state().map_or(310., |g| g.temperature_k());
        let t = f64::from(frame) / 96000.;
        let angle = t * 1500. * std::f64::consts::TAU / 60.;
        let out = cylinder
            .step(CycleInput {
                angle_rad: angle,
                variation: 0.5,
                ..Default::default()
            })
            .map_err(|e| format!("{e:?}"))?;
        correction +=
            out.ledger.numerical_correction_j.abs() + out.wall_numerical_correction_j.abs();
        heat += out.heat_j;
        latent += out.fuel_transfer.latent_heat_j;
        if out.ledger.numerical_correction_j.abs() > 1e-8 {
            cold += u32::from(out.temperature_k <= 200. + 1e-8);
            hot += u32::from(out.temperature_k >= 3500. - 1e-8);
            writeln!(
                csv,
                "{t:.9},{:.6},{previous:.9},{:.9},{:.12},{:.12},{:.12},{:.12},{:.12}",
                angle.to_degrees(),
                out.temperature_k,
                out.ledger.numerical_correction_j,
                out.wall_numerical_correction_j,
                out.heat_j,
                out.fuel_transfer.latent_heat_j,
                out.ledger.wall_heat_j
            )?;
        }
    }
    csv.flush()?;
    println!(
        "{name} cylinder-correction-audit: heat={heat:.6}J correction_abs={correction:.9}J guard_200K={cold} guard_3500K={hot} latent={latent:.6}J"
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Debug builds keep large prepared-engine copies on the stack. This
    // offline worker has an explicit stack; the realtime path is unchanged.
    std::thread::Builder::new()
        .name("fuel-combustion-demo".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| generate().map_err(|e| e.to_string()))?
        .join()
        .map_err(|_| "fuel/combustion demo worker panicked")??;
    Ok(())
}
