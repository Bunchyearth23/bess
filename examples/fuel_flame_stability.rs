//! Synthetic attribution of optional fuel/flame models, without an audio device.
use bess::{
    dyno,
    physical::{
        combustion_model::PredictiveCombustionConfig,
        config::CylinderConfig,
        cycle::{CycleCylinder, CycleInput},
        engine::{Commands, Engine},
        fuel_transient::FuelTransientConfig,
    },
    scratch::Scratch,
};
use std::{error::Error, fs, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("output directory required")?,
    );
    fs::create_dir_all(&dir)?;
    if std::env::args().nth(2).as_deref() == Some("options") {
        return options(&dir);
    }
    if std::env::args().nth(2).as_deref() == Some("metering") {
        return metering(&dir);
    }
    if std::env::args().nth(2).as_deref() == Some("presets") {
        return presets(&dir);
    }
    if std::env::args().nth(2).as_deref() == Some("project") {
        let path = std::env::args_os().nth(3).ok_or("project path required")?;
        let project = bess::project::load_project(std::path::Path::new(&path))?;
        let spec = project.scratch.ok_or("project has no physical engine")?;
        return saved_project(&dir, &spec);
    }
    let mut curves = fs::File::create(dir.join("curves.csv"))?;
    writeln!(curves, "case,rate,rpm,torque_nm,power_kw,map_kpa")?;
    let mut steady = fs::File::create(dir.join("steady.csv"))?;
    writeln!(
        steady,
        "case,rpm,seconds,torque_nm,map_pa,heat_j,injected_kg,misfires,correction_j"
    )?;
    let mut sparks = fs::File::create(dir.join("cylinder-sparks.csv"))?;
    writeln!(
        sparks,
        "case,rpm,seconds,lambda,fresh_fraction,gas_k,wall_k,duration_ms,flame_speed,misfire,film_kg,droplets_kg"
    )?;
    let mut ca50 = fs::File::create(dir.join("cylinder-ca50.csv"))?;
    writeln!(ca50, "case,rpm,seconds,ca50_after_tdc_deg")?;
    let mut long_curve = fs::File::create(dir.join("long-curves.csv"))?;
    writeln!(long_curve, "case,rpm,torque_nm,power_kw")?;
    for (name, fuel, flame) in [
        ("baseline", false, false),
        ("fuel", true, false),
        ("flame", false, true),
        ("combined", true, true),
    ] {
        let mut spec = Scratch::default();
        spec.experimental.fuel_transients = fuel.then(FuelTransientConfig::default);
        spec.experimental.predictive_combustion = flame.then(PredictiveCombustionConfig::default);
        fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&spec)?,
        )?;
        for rate in [8000, 48000] {
            let curve = dyno::sweep_at_rate(&spec, 25, rate)?;
            for i in 0..curve.rpm.len() {
                writeln!(
                    curves,
                    "{name},{rate},{},{},{},{}",
                    curve.rpm[i], curve.torque_nm[i], curve.power_kw[i], curve.map_kpa[i]
                )?;
            }
            println!(
                "{name} @ {rate}: torque {:?}, power {:?}, negative points {}",
                curve.peak_torque,
                curve.peak_power,
                curve.torque_nm.iter().filter(|&&t| t < 0.).count()
            );
        }
        // Independent fixed minimum settling test, using whole-cycle windows.
        let rate = 8000;
        let mut engine = Engine::new(&dyno::key(&spec), rate)?;
        for point in 0..13 {
            let rpm = f64::from(spec.idle_rpm)
                + f64::from(spec.redline_rpm - spec.idle_rpm - 1.) * f64::from(point) / 12.;
            let command = Commands {
                imposed_rpm: Some(rpm),
                throttle: 1.,
                ..Default::default()
            };
            for _ in 0..rate * 2 {
                engine.next(command);
            }
            let cycle = 120. / rpm;
            let frames = (1. / cycle).ceil() * cycle * f64::from(rate);
            let mut frame = 0.;
            let mut torque = 0.;
            while frame < frames {
                let sample = engine.next(command);
                torque += (frames - frame).min(1.) * (sample.torque_nm - engine.friction_nm());
                frame += 1.;
            }
            if engine.failed() {
                return Err(format!("{name}: long dyno failed at {rpm}").into());
            }
            torque /= frames;
            writeln!(
                long_curve,
                "{name},{rpm},{torque},{}",
                torque * rpm * std::f64::consts::TAU / 60000.
            )?;
        }
        for rpm in [3000., 5500., f64::from(spec.redline_rpm) - 100.] {
            let rate = 8000;
            let mut engine = Engine::new(&spec, rate)?;
            let mut sums = [0.; 6];
            let mut last_misfires = 0;
            for frame in 0..rate * 5 {
                let s = engine.next(Commands {
                    imposed_rpm: Some(rpm),
                    throttle: 1.,
                    ..Default::default()
                });
                if engine.failed() {
                    return Err(format!("{name}: engine failed at {rpm}").into());
                }
                sums[0] += s.torque_nm - engine.friction_nm();
                sums[1] += s.map_pa;
                sums[2] += s.heat_j;
                sums[3] += s.fuel_injected_kg;
                sums[4] += s.misfires.saturating_sub(last_misfires) as f64;
                last_misfires = s.misfires;
                sums[5] += s.correction_j.abs();
                if (frame + 1) % (rate / 10) == 0 {
                    writeln!(
                        steady,
                        "{name},{rpm},{},{},{},{},{},{},{}",
                        f64::from(frame + 1) / f64::from(rate),
                        sums[0] / f64::from(rate / 10),
                        sums[1] / f64::from(rate / 10),
                        sums[2],
                        sums[3],
                        sums[4],
                        sums[5]
                    )?;
                    sums = [0.; 6];
                }
            }
            let config = CylinderConfig::from_build(&spec.build)?;
            let mut cylinder = CycleCylinder::new(config, 0., 123).map_err(|e| format!("{e:?}"))?;
            cylinder
                .set_fuel_transient(spec.experimental.fuel_transients)
                .map_err(|e| format!("{e:?}"))?;
            cylinder
                .set_predictive_combustion(spec.experimental.predictive_combustion)
                .map_err(|e| format!("{e:?}"))?;
            let rate = 96000;
            let mut last_burn_fraction = 0.;
            for frame in 1..=rate * 3 {
                let t = f64::from(frame) / f64::from(rate);
                let angular_speed = rpm * std::f64::consts::TAU / 60.;
                let angle = t * angular_speed;
                let previous = f64::from(frame - 1) / f64::from(rate) * angular_speed;
                let cycle = 2. * std::f64::consts::TAU;
                let spark = 35_f64.to_radians();
                let out = cylinder
                    .step(CycleInput {
                        angle_rad: angle,
                        rpm,
                        dt_s: 1. / f64::from(rate),
                        variation: 0.,
                        ..Default::default()
                    })
                    .map_err(|e| format!("{e:?}"))?;
                if ((angle + spark) / cycle).floor() > ((previous + spark) / cycle).floor() {
                    let lambda = out.fresh_air_mass_kg / (14.7 * out.fuel_mass_kg.max(1e-15));
                    let fresh = out.fresh_air_mass_kg / cylinder.gas_state().unwrap().mass_kg();
                    writeln!(
                        sparks,
                        "{name},{rpm},{t},{lambda},{fresh},{},{},{},{},{},{},{}",
                        out.temperature_k,
                        out.wall_temperature_k,
                        out.predicted_burn_duration_s.unwrap_or(0.) * 1000.,
                        out.predicted_flame_speed_m_s.unwrap_or(0.),
                        out.misfired,
                        out.fuel_transfer.film_kg,
                        out.fuel_transfer.droplets_kg
                    )?;
                }
                let fraction = out.burn_fraction.unwrap_or(0.);
                if last_burn_fraction < 0.5 && fraction >= 0.5 {
                    let after_tdc =
                        (angle + std::f64::consts::PI).rem_euclid(cycle) - std::f64::consts::PI;
                    writeln!(ca50, "{name},{rpm},{t},{}", after_tdc.to_degrees())?;
                }
                last_burn_fraction = fraction;
            }
        }
    }
    let mut matrix = fs::File::create(dir.join("fuel-matrix.csv"))?;
    writeln!(matrix, "engine,fuel,rpm,torque_nm,power_kw")?;
    for (name, bore, stroke, cam, compression, redline) in [
        ("compact", 60., 60., 0.3, 10.5, 7300.),
        ("long-stroke", 86., 100., 0.3, 10.5, 7300.),
        ("large-bore", 100., 86., 0.3, 10.5, 7300.),
        ("hot-cam", 86., 86., 0.8, 10.5, 8500.),
        ("mild-cam", 86., 86., 0., 10.5, 7300.),
        ("high-compression", 86., 86., 0.3, 14., 7300.),
    ] {
        for fuel in [false, true] {
            let mut spec = Scratch::default();
            spec.build.bore_mm = bore;
            spec.build.stroke_mm = stroke;
            spec.build.cam = cam;
            spec.build.compression = compression;
            spec.redline_rpm = redline;
            spec.experimental.fuel_transients = fuel.then(FuelTransientConfig::default);
            let curve = dyno::sweep(&spec, 25)?;
            for i in 0..curve.rpm.len() {
                writeln!(
                    matrix,
                    "{name},{fuel},{},{},{}",
                    curve.rpm[i], curve.torque_nm[i], curve.power_kw[i]
                )?;
            }
            println!(
                "{name}, fuel={fuel}: power {:?}, negative points {}",
                curve.peak_power,
                curve.torque_nm.iter().filter(|&&t| t < 0.).count()
            );
        }
    }
    Ok(())
}

fn metering(dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let mut csv = fs::File::create(dir.join("metering.csv"))?;
    writeln!(csv, "case,fuel,rpm,torque_nm,power_kw")?;
    for name in [
        "normal",
        "restrictive-valve",
        "low-lift",
        "late-intake",
        "intake-feedback",
        "exhaust-feedback",
        "turbo",
    ] {
        for fuel in [false, true] {
            let mut spec = Scratch {
                redline_rpm: 7300.,
                ..Scratch::default()
            };
            match name {
                "restrictive-valve" => spec.tuning.valves.intake_to_bore = Some(0.25),
                "low-lift" => spec.tuning.cam.intake_lift_mm = Some(7.),
                "late-intake" => spec.tuning.cam.intake_advance_deg = Some(-4.),
                "intake-feedback" => spec.experimental.intake_wave_coupling = true,
                "exhaust-feedback" => spec.experimental.wave_coupling = true,
                "turbo" => spec.build.aspiration = bess::engine_build::Aspiration::Turbo,
                _ => (),
            }
            spec.experimental.fuel_transients = fuel.then(FuelTransientConfig::default);
            let curve = dyno::sweep(&spec, 25)?;
            for i in 0..curve.rpm.len() {
                writeln!(
                    csv,
                    "{name},{fuel},{},{},{}",
                    curve.rpm[i], curve.torque_nm[i], curve.power_kw[i]
                )?;
            }
            println!(
                "{name}, fuel={fuel}: power {:?}, negative points {}",
                curve.peak_power,
                curve.torque_nm.iter().filter(|&&t| t < 0.).count()
            );
        }
    }
    Ok(())
}

fn options(dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let mut csv = fs::File::create(dir.join("options.csv"))?;
    writeln!(csv, "case,rpm,torque_nm,power_kw,map_kpa")?;
    for (name, fuel, flame, intake, torsion, exhaust) in [
        ("baseline", false, false, false, false, false),
        ("fuel", true, false, false, false, false),
        ("flame", false, true, false, false, false),
        ("combined", true, true, false, false, false),
        ("fuel-intake", true, false, true, false, false),
        ("fuel-torsion", true, false, false, true, false),
        ("all-visible", true, true, true, true, false),
        ("all-with-exhaust", true, true, true, true, true),
    ] {
        let mut spec = Scratch::default();
        spec.experimental.fuel_transients = fuel.then(FuelTransientConfig::default);
        spec.experimental.predictive_combustion = flame.then(PredictiveCombustionConfig::default);
        spec.experimental.intake_wave_coupling = intake;
        spec.experimental.torsion = torsion.then(Default::default);
        spec.experimental.wave_coupling = exhaust;
        let curve = dyno::sweep(&spec, 25)?;
        for i in 0..curve.rpm.len() {
            writeln!(
                csv,
                "{name},{},{},{},{}",
                curve.rpm[i], curve.torque_nm[i], curve.power_kw[i], curve.map_kpa[i]
            )?;
        }
        println!(
            "{name}: power {:?}, minimum torque {}, negative points {}",
            curve.peak_power,
            curve
                .torque_nm
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min),
            curve.torque_nm.iter().filter(|&&t| t < 0.).count()
        );
    }
    Ok(())
}

fn presets(dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let mut csv = fs::File::create(dir.join("presets.csv"))?;
    writeln!(csv, "case,fuel,rpm,torque_nm,power_kw")?;
    for (name, design) in bess::scratch::PRESETS {
        for fuel in [false, true] {
            let mut spec = Scratch {
                design,
                redline_rpm: 7300.,
                ..Default::default()
            };
            spec.experimental.fuel_transients = fuel.then(FuelTransientConfig::default);
            let curve = dyno::sweep(&spec, 25)?;
            for i in 0..curve.rpm.len() {
                writeln!(
                    csv,
                    "{name},{fuel},{},{},{}",
                    curve.rpm[i], curve.torque_nm[i], curve.power_kw[i]
                )?;
            }
            println!(
                "{name}, fuel={fuel}: power {:?}, negative points {}",
                curve.peak_power,
                curve.torque_nm.iter().filter(|&&t| t < 0.).count()
            );
        }
    }
    Ok(())
}

fn saved_project(dir: &std::path::Path, saved: &Scratch) -> Result<(), Box<dyn Error>> {
    let mut csv = fs::File::create(dir.join("project.csv"))?;
    writeln!(csv, "case,rate,rpm,torque_nm,power_kw,map_kpa")?;
    // Preserve every other saved setting, including custom model coefficients.
    for name in ["saved", "without-fuel", "without-flame", "without-both"] {
        let mut spec = saved.clone();
        if matches!(name, "without-fuel" | "without-both") {
            spec.experimental.fuel_transients = None;
        }
        if matches!(name, "without-flame" | "without-both") {
            spec.experimental.predictive_combustion = None;
        }
        for rate in [8000, 48000] {
            let curve = dyno::sweep_at_rate(&spec, 25, rate)?;
            for i in 0..curve.rpm.len() {
                writeln!(
                    csv,
                    "{name},{rate},{},{},{},{}",
                    curve.rpm[i], curve.torque_nm[i], curve.power_kw[i], curve.map_kpa[i]
                )?;
            }
            println!(
                "{name} @ {rate}: power {:?}, negative points {}",
                curve.peak_power,
                curve.torque_nm.iter().filter(|&&t| t < 0.).count()
            );
        }
    }
    Ok(())
}
