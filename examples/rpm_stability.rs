//! Synthetic idle and acceleration with the same 1 ms drivetrain cadence as the UI.
use bess::{
    drive::{Controls, Simulator},
    physical::{
        combustion_model::PredictiveCombustionConfig,
        crank::TorsionConfig,
        engine::{Commands, Engine},
        fuel_transient::FuelTransientConfig,
    },
    scratch::Scratch,
};
use std::{error::Error, fs, io::Write};
fn main() -> Result<(), Box<dyn Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or("output/rpm-before".into());
    fs::create_dir_all(&dir)?;
    let rate: u32 = std::env::args().nth(2).unwrap_or("8000".into()).parse()?;
    let only = std::env::args().nth(3);
    let mut summary = fs::File::create(format!("{dir}/summary.csv"))?;
    writeln!(
        summary,
        "case,phase,shaft,mean_rpm,min_rpm,max_rpm,cuts,misfires,end_speed_kmh"
    )?;
    for name in [
        "baseline",
        "fuel",
        "flame",
        "both",
        "torsion",
        "all",
        "all-exhaust",
    ] {
        if only.as_deref().is_some_and(|x| x != name) {
            continue;
        }
        for driving in [false, true] {
            for shaft in [false, true] {
                if !driving && shaft {
                    continue;
                }
                let mut spec = Scratch::default();
                spec.experimental.fuel_transients =
                    matches!(name, "fuel" | "both" | "all" | "all-exhaust")
                        .then(FuelTransientConfig::default);
                spec.experimental.predictive_combustion =
                    matches!(name, "flame" | "both" | "all" | "all-exhaust")
                        .then(PredictiveCombustionConfig::default);
                spec.experimental.torsion =
                    matches!(name, "torsion" | "all" | "all-exhaust").then(TorsionConfig::default);
                spec.experimental.intake_wave_coupling = matches!(name, "all" | "all-exhaust");
                spec.experimental.wave_coupling = name == "all-exhaust";
                let mut engine = Engine::new(&spec, rate)?;
                let mut controls = Controls {
                    automatic: false,
                    gear: 1,
                    inertia: engine.inertia() as f32,
                    ..Default::default()
                };
                let mut sim = Simulator::new(spec.idle_rpm, spec.redline_rpm, controls);
                let mut command = Commands {
                    imposed_rpm: Some(1200.),
                    throttle: 0.,
                    ..Default::default()
                };
                let mut trace = fs::File::create(format!(
                    "{dir}/{name}-{}-{shaft}.csv",
                    if driving { "drive" } else { "idle" }
                ))?;
                writeln!(
                    trace,
                    "seconds,rpm,flywheel_rpm,map_pa,torque_nm,load_nm,throttle,bypass,fuel_cut,misfires,speed_kmh"
                )?;
                let (mut sum, mut count, mut min, mut max, mut cuts) =
                    (0., 0., f64::INFINITY, 0_f64, 0);
                for frame in 0..rate * 10 {
                    let time = f64::from(frame) / f64::from(rate);
                    if time >= 1. {
                        command.imposed_rpm = None;
                        if driving && frame % (rate / 1000) == 0 {
                            controls.throttle = if time < 1.2 { 0. } else { 0.65 };
                            let mut rpm = engine.state().rpm;
                            if shaft && let Some(t) = spec.experimental.torsion {
                                rpm = engine.torsion_ledger().flywheel_rpm;
                                controls.inertia =
                                    (engine.inertia() * (1. - t.crank_inertia_fraction)) as f32;
                            }
                            let (_, load, throttle) = sim.step_physical(controls, rpm as f32);
                            command.load_nm = load;
                            command.throttle = f64::from(throttle);
                        }
                    }
                    let s = engine.next(command);
                    if engine.failed() {
                        return Err(format!("{name} failed at {time}").into());
                    }
                    if time >= 8. {
                        sum += s.rpm;
                        count += 1.;
                        min = min.min(s.rpm);
                        max = max.max(s.rpm);
                        cuts += u32::from(s.fuel_cut);
                    }
                    if frame % (rate / 100) == 0 {
                        writeln!(
                            trace,
                            "{time},{},{},{},{},{},{},{},{},{},{}",
                            s.rpm,
                            engine.torsion_ledger().flywheel_rpm,
                            s.map_pa,
                            s.torque_nm,
                            command.load_nm,
                            command.throttle,
                            s.idle_bypass,
                            s.fuel_cut,
                            s.misfires,
                            sim.state().speed_kmh
                        )?;
                    }
                }
                let mean = sum / count;
                let s = engine.state();
                println!(
                    "{name} driving={driving} shaft={shaft}: RPM {mean:.1} [{min:.1},{max:.1}], cuts {cuts}, misfires {}, speed {:.1}",
                    s.misfires,
                    sim.state().speed_kmh
                );
                writeln!(
                    summary,
                    "{name},{driving},{shaft},{mean},{min},{max},{cuts},{},{}",
                    s.misfires,
                    sim.state().speed_kmh
                )?;
            }
        }
    }
    Ok(())
}
