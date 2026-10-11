//! Synthetic component qualification: rates, loads, thermal and stop boundaries.
//! No output is a measured turbo/crank reference or a real-time qualification.
use bess::{
    engine_build::{Aspiration, BlowOff, EngineBuild, EngineTuning},
    physical::{
        crank::{Crank, FrictionModel, TorsionConfig},
        cylinder::Reservoir,
        gas::Orifice,
        induction::Induction,
        manifolds::{ATMOSPHERE, Exchange},
        structural::{StructuralObserver, StructuralResponse},
        thermo::Mixture,
        turbo_map::TurboModel,
    },
};
use std::{error::Error, fs, path::PathBuf, time::Instant};

fn turbo_case(
    rate: u32,
    temperature: f64,
    demand: f64,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let build = EngineBuild {
        aspiration: Aspiration::Turbo,
        blow_off: BlowOff::Atmospheric,
        ..Default::default()
    };
    let tuning = EngineTuning::default().resolve(&build, 4);
    let mut induction = Induction::new(&build, &tuning, 0.002)?;
    let model = TurboModel::from_json(&TurboModel::synthetic_example().to_import_json()?)?;
    induction.set_model(Some(model))?;
    induction
        .initialize_temperature(temperature)
        .map_err(|e| format!("{e:?}"))?;
    let initial_u = induction.charge_internal_energy_j();
    let initial_m = induction.charge_mass_kg();
    let mut max_charge_residual: f64 = 0.0;
    let mut max_shaft_residual: f64 = 0.0;
    let mut mass_change = 0.0;
    let mut energy_change = 0.0;
    let mut wall_heat = 0.0;
    let mut correction = 0.0;
    let mut reverse = 0.0;
    let mut outside = 0;
    let mut peak_pressure: f64 = 0.0;
    let exhaust = Reservoir {
        pressure_pa: 220000.0,
        temperature_k: 1000.0,
    };
    let dt = 1.0 / f64::from(rate);
    let start = Instant::now();
    for i in 0..rate / 5 {
        let lift = i >= rate / 10;
        let area = induction.mapped_turbine_area_m2(exhaust).unwrap()
            + induction.wastegate() * tuning.wastegate_area_m2;
        let exhaust_flow = Orifice {
            area_m2: area,
            discharge_coefficient: 0.9,
        }
        .mass_flow_from_states(
            exhaust.pressure_pa,
            exhaust.temperature_k,
            ATMOSPHERE.pressure_pa,
            300.0,
            Mixture::PRODUCTS.properties(exhaust.temperature_k),
        );
        let step = induction
            .step(
                dt,
                3000.0,
                if lift { 0.0 } else { 0.8 },
                exhaust,
                if lift { 0.0 } else { exhaust_flow },
                Exchange {
                    mass_out_kg: if lift {
                        0.0
                    } else {
                        (demand * dt).min(induction.outgoing_budget_kg())
                    },
                    ..Default::default()
                },
            )
            .map_err(|e| format!("{e:?}"))?;
        let l = step.charge_ledger;
        max_charge_residual = max_charge_residual.max(l.residual_j().abs());
        max_shaft_residual = max_shaft_residual.max(
            (step.shaft_energy_change_j - step.turbine_energy_j
                + step.compressor_energy_j
                + step.bearing_loss_j
                + step.overspeed_loss_j)
                .abs(),
        );
        mass_change += l.mass_in_kg - l.mass_out_kg;
        energy_change +=
            l.enthalpy_in_j - l.enthalpy_out_j - l.wall_heat_j + l.numerical_correction_j;
        wall_heat += step.charge_wall_heat_j;
        correction += l.numerical_correction_j.abs();
        reverse += step.surge_reverse_mass_kg;
        outside += u32::from(step.map_outside_domain);
        peak_pressure = peak_pressure.max(induction.supply().pressure_pa);
    }
    let mass_residual = induction.charge_mass_kg() - initial_m - mass_change;
    let energy_residual = induction.charge_internal_energy_j() - initial_u - energy_change;
    if mass_residual.abs() > 1e-10
        || energy_residual.abs() > 1e-6
        || max_shaft_residual > 1e-8
        || correction > 1e-7
    {
        return Err("Synthetic turbo grid violates its mass/energy/correction budget".into());
    }
    Ok(
        serde_json::json!({"rate":rate,"initial_temperature_k":temperature,"demand_kg_s":demand,"final_pressure_pa":induction.supply().pressure_pa,"peak_pressure_pa":peak_pressure,"final_temperature_k":induction.supply().temperature_k,"shaft_rpm":induction.shaft_rpm(),"wastegate":induction.wastegate(),"bov_opening":induction.bov_opening(),"wall_heat_j":wall_heat,"reverse_mass_kg":reverse,"outside_steps":outside,"total_steps":rate/5,"mass_residual_kg":mass_residual,"energy_residual_j":energy_residual,"max_charge_residual_j":max_charge_residual,"max_shaft_residual_j":max_shaft_residual,"correction_j":correction,"wall_seconds":start.elapsed().as_secs_f64()}),
    )
}

fn torsion_case(rate: u32, rpm: f64, load: f64) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut crank = Crank::new(0.22, 0.086, 0.002, rpm)?;
    crank.set_friction(FrictionModel::ZERO)?;
    crank.set_torsion(Some(TorsionConfig::default()))?;
    let initial = crank.state().kinetic_energy_j;
    let mut work = 0.0;
    let mut loss = 0.0;
    let mut residual: f64 = 0.0;
    let mut peak_twist: f64 = 0.0;
    let mut events = 0;
    let mut iterations = 0;
    let mut max_events = 0;
    let mut steps_with_event = 0;
    let dt = 1.0 / f64::from(rate);
    let start = Instant::now();
    for i in 0..rate / 5 {
        let torque = if i < rate / 10 { 40.0 } else { 0.0 };
        let s = crank.step(dt, torque, 0.0, load, false)?;
        let t = crank.torsion_ledger();
        work += s.gas_work_j - s.load_work_j;
        loss += t.damping_loss_j + t.stop_loss_j;
        residual =
            residual.max((s.kinetic_energy_j + t.spring_energy_j - initial - work + loss).abs());
        peak_twist = peak_twist.max(t.twist_rad.abs());
        events += t.stop_events;
        iterations += t.event_solver_iterations;
        max_events = max_events.max(t.stop_events);
        steps_with_event += u32::from(t.stop_events > 0);
    }
    if residual > 1e-6 {
        return Err("Synthetic torsion grid violates its work budget".into());
    }
    let t = crank.torsion_ledger();
    Ok(
        serde_json::json!({"rate":rate,"initial_rpm":rpm,"load_nm":load,"final_crank_rpm":crank.state().rpm,"final_flywheel_rpm":t.flywheel_rpm,"peak_twist_rad":peak_twist,"work_j":work,"dissipation_j":loss,"max_energy_residual_j":residual,"stop_events":events,"solver_iterations":iterations,"max_events_in_step":max_events,"steps_with_event":steps_with_event,"total_steps":rate/5,"wall_seconds":start.elapsed().as_secs_f64()}),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let _denormals = bess::realtime::DenormalGuard::enter();
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "work/simulation-p5-p6-grid".into()),
    );
    fs::create_dir_all(&dir)?;
    let mut turbo = Vec::new();
    let mut torsion = Vec::new();
    let mut structural = Vec::new();
    for rate in [96000, 192000, 384000] {
        for temperature in [260.0, 300.0, 450.0] {
            for demand in [0.0, 0.04, 0.12] {
                turbo.push(turbo_case(rate, temperature, demand)?);
            }
        }
    }
    for rate in [48000, 96000, 192000] {
        for rpm in [0.0, 40.0, 3000.0] {
            for load in [0.0, 40.0, 400.0] {
                torsion.push(torsion_case(rate, rpm, load)?);
            }
        }
    }
    let response =
        StructuralResponse::from_json(&StructuralResponse::synthetic_example().to_import_json()?)?;
    for rate in [48000, 96000, 192000] {
        let mut observer = StructuralObserver::new(response, rate)?;
        let mut energy = 0.0;
        let mut work = 0.0;
        let mut loss = 0.0;
        let mut max_residual: f64 = 0.0;
        for i in 0..rate / 5 {
            let before = observer.energy_j();
            let input =
                (std::f64::consts::TAU * 200.0 * f64::from(i) / f64::from(rate)).sin() as f32;
            let sample = observer.next(input, 0.0);
            work += observer.last_work_j();
            loss += observer.last_damping_loss_j();
            max_residual = max_residual.max(
                (observer.energy_j() - before - observer.last_work_j()
                    + observer.last_damping_loss_j())
                .abs(),
            );
            if i >= rate / 10 {
                energy += f64::from(sample).powi(2);
            }
        }
        structural.push(serde_json::json!({"rate":rate,"rms":(energy/f64::from(rate/10)).sqrt(),"max_residual_j":max_residual,"integrated_residual_j":observer.energy_j()-work+loss}));
    }
    let summary = serde_json::json!({"schema":1,"build_profile":if cfg!(debug_assertions) {"dev"}else{"release"},"measured":false,"profile":if cfg!(debug_assertions) {"dev; component grids only, no runtime performance acceptance"}else{"release; component grids only, no runtime performance acceptance"},"hardware":std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_|"CPU not recorded".into()),"turbo":turbo,"torsion":torsion,"structural":structural});
    fs::write(
        dir.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    println!(
        "Synthetic maps/torsion/structural grid saved to {}",
        dir.display()
    );
    Ok(())
}
