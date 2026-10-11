//! W-016 runnable offline qualification pipeline (not production Engine mode).
//! cargo run --release --example coupled_1d -- <out_dir> [seconds] [cell_mm] [cancel_file|-] [legacy|fixed-species|mixture]
use bess::{
    physical::{
        config::CylinderConfig,
        coupled_1d::{Exhaust1d, Geometry, PipePort, ReferenceGas, SpeciesExhaust1d, Transfer},
        cycle::{CycleCylinder, CycleInput},
        cylinder::Reservoir,
        finite_volume::GasEquation,
        wave_junction::WavePort,
    },
    scratch::Scratch,
};
use std::{f64::consts::TAU, fmt::Write, path::PathBuf, time::Instant};

enum Network {
    Legacy(Exhaust1d),
    Species(SpeciesExhaust1d),
}
impl Network {
    fn inventory(&self) -> Transfer {
        match self {
            Self::Legacy(n) => n.inventory(),
            Self::Species(n) => n.inventory(),
        }
    }
    fn inlet_total(&self) -> Transfer {
        match self {
            Self::Legacy(n) => n.inlet_total(),
            Self::Species(n) => n.inlet_total(),
        }
    }
    fn outlet_total(&self) -> Transfer {
        match self {
            Self::Legacy(n) => n.outlet_total(),
            Self::Species(n) => n.outlet_total(),
        }
    }
    fn port(&self, i: usize) -> PipePort {
        match self {
            Self::Legacy(n) => n.port(i),
            Self::Species(n) => n.port(i),
        }
    }
    fn cells(&self) -> usize {
        match self {
            Self::Legacy(n) => n.cells(),
            Self::Species(n) => n.cells(),
        }
    }
    fn collector_pressure_pa(&self) -> f64 {
        match self {
            Self::Legacy(n) => n.collector_pressure_pa(),
            Self::Species(n) => n.collector_pressure_pa(),
        }
    }
    fn tail_pressure_pa(&self) -> f64 {
        match self {
            Self::Legacy(n) => n.tail_pressure_pa(),
            Self::Species(n) => n.tail_pressure_pa(),
        }
    }
    fn stable_time_step(&self) -> f64 {
        match self {
            Self::Legacy(n) => n.stable_time_step(),
            Self::Species(n) => n.stable_time_step(),
        }
    }
    fn advance(&mut self, dt: f64, s: &[Transfer], ambient: Reservoir) -> Result<(), &'static str> {
        match self {
            Self::Legacy(n) => n.advance(dt, s, ambient),
            Self::Species(n) => n.advance(dt, s, ambient),
        }
    }
    fn rejected_trials(&self) -> Option<u64> {
        match self {
            Self::Legacy(_) => None,
            Self::Species(n) => Some(n.rejected_trials()),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "out/coupled-1d".into()));
    let seconds: f64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(0.16);
    let cell_mm: f64 = args.next().map(|s| s.parse()).transpose()?.unwrap_or(4.);
    let cancel_file = args.next().filter(|s| s != "-").map(PathBuf::from);
    let eos_mode = args.next().unwrap_or_else(|| "legacy".into());
    if !(0.04..=30.).contains(&seconds) || !(2.0..=40.).contains(&cell_mm) {
        return Err("duration 0.04–30 s; cells 2–40 mm".into());
    }
    std::fs::create_dir_all(&out)?;
    let scratch = Scratch::default();
    let tuning = scratch.tuning.resolve(&scratch.build, 4);
    let config = CylinderConfig::from_tuning(&scratch.build, &tuning)
        .map_err(|e| format!("Cylinder config: {e:?}"))?;
    let gas = ReferenceGas::default();
    let geometry = Geometry {
        primary_lengths_m: vec![0.7; 4],
        primary_area_m2: std::f64::consts::PI
            * (f64::from(scratch.build.bore_mm) * 0.00021).powi(2),
        collector_volume_m3: 0.003,
        tail_length_m: 1.2,
        tail_area_m2: 0.003,
        cell_size_m: cell_mm * 0.001,
    };
    let mut network = match eos_mode.as_str() {
        "legacy" => Network::Legacy(Exhaust1d::new(&geometry, gas)?),
        "fixed-species" => Network::Species(SpeciesExhaust1d::new(
            &geometry,
            gas,
            GasEquation::Fixed {
                gamma: gas.gamma,
                gas_constant: gas.gas_constant,
            },
        )?),
        "mixture" => Network::Species(SpeciesExhaust1d::new(
            &geometry,
            gas,
            GasEquation::SharedMixture,
        )?),
        _ => return Err("EOS mode: legacy, fixed-species, mixture".into()),
    };
    let initial = network.inventory();
    let initial_port = network.port(0);
    let mut cylinders = (0..4)
        .map(|i| CycleCylinder::new(config, i as f64 * TAU * 0.5, 17 + i as u64 * 7919))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Cylinder init: {e:?}"))?;
    let mut previous_flow = [0.; 4];
    let intake = Reservoir {
        pressure_pa: 85000.,
        temperature_k: 310.,
    };
    let ambient = Reservoir {
        pressure_pa: 101325.,
        temperature_k: 305.,
    };
    let rate = 48000_u32;
    let rpm = 2400.;
    let frames = (seconds * f64::from(rate)).round() as usize;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(out.join("physical-cylinder-branched-1d.wav"), spec)?;
    let mut csv = String::from(
        "time_s,torque_nm,collector_pa,tail_pa,network_mass_residual_kg,network_energy_residual_j,fresh_residual_kg,fuel_residual_kg,port0_temperature_k,port0_gamma,port0_fresh_fraction,port0_fuel_fraction\n",
    );
    let start = Instant::now();
    let mut time = 0.;
    let mut max_residual = [0_f64; 4];
    let mut steps = 0_u64;
    let mut sum_torque = 0.;
    let mut completed_frames = 0;
    let mut observed_mean_pa = ambient.pressure_pa;
    let observation_slew = 1. - (-std::f64::consts::TAU * 12. / f64::from(rate)).exp();
    for frame in 0..frames {
        if frame.is_multiple_of(480) {
            if cancel_file.as_ref().is_some_and(|p| p.exists()) {
                break;
            }
            if frame.is_multiple_of(4800) {
                eprintln!("Offline 1D: {frame}/{frames} frames");
            }
        }
        let target = (frame + 1) as f64 / f64::from(rate);
        let mut torque_integral = 0.;
        while time < target - 1e-12 {
            let stable_dt = network.stable_time_step();
            if stable_dt < 1e-9 {
                return Err("CFL requires a step outside the cylinder's supported domain".into());
            }
            let dt = (target - time).min(stable_dt).min(1. / 96000.);
            // Carry a sub-nanosecond remainder into the next frame; never
            // skip a CFL-constrained physical interval to reach audio time.
            if dt < 1e-9 {
                break;
            }
            let angle = (time + dt) * rpm * TAU / 60.;
            let mut sources = [Transfer::default(); 4];
            let mut torque = 0.;
            for i in 0..4 {
                let port = network.port(i);
                cylinders[i].set_exhaust_port(Some(WavePort {
                    mean_pa: port.reservoir.pressure_pa,
                    reference_pa: port.reservoir.pressure_pa,
                    gamma: port.gamma,
                    wave_pa: 0.,
                    impedance: port.sound_speed_m_s / geometry.primary_area_m2,
                    guess_inflow_kg_s: -previous_flow[i],
                }));
                let sample = cylinders[i]
                    .step(CycleInput {
                        angle_rad: angle,
                        dt_s: dt,
                        rpm,
                        intake,
                        exhaust: port.reservoir,
                        exhaust_mass_limit_kg: port.incoming_budget_kg,
                        exhaust_fresh_air_fraction: port.fresh_air_fraction,
                        exhaust_fuel_fraction: port.fuel_fraction,
                        variation: 0.,
                        ..Default::default()
                    })
                    .map_err(|e| format!("Cylinder {i} at {time:.8} s: {e:?}"))?;
                sources[i] = Transfer {
                    mass_kg: sample.exhaust_mass_kg,
                    energy_j: sample.exhaust_enthalpy_j,
                    fresh_air_kg: sample.exhaust_fresh_air_kg,
                    fuel_kg: sample.exhaust_fuel_kg,
                };
                previous_flow[i] = sample.exhaust_mass_flow_kg_s;
                torque += sample.gas_torque_nm;
            }
            network
                .advance(dt, &sources, ambient)
                .map_err(|e| format!("1D at {time:.8} s: {e}"))?;
            time += dt;
            steps += 1;
            torque_integral += torque * dt;
            let residual = network.inventory().residual(
                initial,
                network.inlet_total(),
                network.outlet_total(),
            );
            for (maximum, value) in max_residual.iter_mut().zip([
                residual.mass_kg,
                residual.energy_j,
                residual.fresh_air_kg,
                residual.fuel_kg,
            ]) {
                *maximum = maximum.max(value.abs());
            }
        }
        let torque = torque_integral * f64::from(rate);
        sum_torque += torque;
        completed_frames += 1;
        // Fixed observation gain and 12 Hz DC removal; neither feeds physics.
        let observed_pressure = network.tail_pressure_pa();
        observed_mean_pa += observation_slew * (observed_pressure - observed_mean_pa);
        wav.write_sample(((observed_pressure - observed_mean_pa) / 30000.) as f32)?;
        if frame.is_multiple_of(48) {
            let r = network.inventory().residual(
                initial,
                network.inlet_total(),
                network.outlet_total(),
            );
            writeln!(
                csv,
                "{time:.8},{torque:.6},{:.6},{:.6},{:.3e},{:.3e},{:.3e},{:.3e},{:.6},{:.6},{:.8},{:.8}",
                network.collector_pressure_pa(),
                network.tail_pressure_pa(),
                r.mass_kg,
                r.energy_j,
                r.fresh_air_kg,
                r.fuel_kg,
                network.port(0).reservoir.temperature_k,
                network.port(0).gamma,
                network.port(0).fresh_air_fraction,
                network.port(0).fuel_fraction
            )?;
        }
    }
    wav.finalize()?;
    std::fs::write(out.join("physical-cylinder-branched-1d.csv"), csv)?;
    let report = serde_json::json!({"provenance":"synthetic engineering qualification; no real-engine measurements",
        "scope":"physical CycleCylinder; 4 Euler primaries + compliant conservative collector + Euler tail; prescribed intake/RPM",
        "eos_mode":eos_mode,
        "spatial_scheme":if eos_mode == "legacy" { "MUSCL order 2; branch species well mixed" } else { "first-order HLLC/HLLE; five conservative cell unknowns; SSP-RK2 time" },
        "thermodynamics":if eos_mode == "mixture" { "same frozen Mixture caloric tables/common energy reference as CycleCylinder; gamma(T,Y), R(Y)" } else { "fixed gamma/R; reduction reference differs from variable cylinder EOS" },
        "limitations":["first-order spatial distributed-species bench requires refinement qualification", "collector perfectly mixed; frozen chemistry", "no catalyst/turbo/wall losses", "not production Engine primary_1d path", "no measured accuracy or listening qualification", "conservative multispecies contact pressure errors require dedicated qualification"],
        "scratch":scratch,"rpm":rpm,"sample_rate":rate,"seconds":seconds,"cell_mm":cell_mm,
        "completed":completed_frames == frames,"rendered_seconds":completed_frames as f64 / f64::from(rate),
        "physical_time_s":time,
        "fixed_gamma":(eos_mode != "mixture").then_some(gas.gamma),
        "fixed_gas_constant":(eos_mode != "mixture").then_some(gas.gas_constant),
        "initial_temperature_k":gas.temperature_k,
        "initial_port_gamma":initial_port.gamma,"initial_port_sound_speed_m_s":initial_port.sound_speed_m_s,
        "primary_lengths_m":geometry.primary_lengths_m,"primary_area_m2":geometry.primary_area_m2,
        "collector_volume_m3":geometry.collector_volume_m3,"tail_length_m":geometry.tail_length_m,"tail_area_m2":geometry.tail_area_m2,
        "cells":network.cells(),"steps":steps,"render_seconds":start.elapsed().as_secs_f64(),
        "rejected_species_trials":network.rejected_trials(),"cylinder_seeds":[17,7936,15855,23774],
        "max_network_residuals":{"mass_kg":max_residual[0],"energy_j":max_residual[1],"fresh_air_kg":max_residual[2],"fuel_kg":max_residual[3]},
        "mean_gas_torque_nm":sum_torque / completed_frames.max(1) as f64,"fixed_audio_pa_per_sample":30000.,"observation_highpass_hz":12.});
    std::fs::write(
        out.join("physical-cylinder-branched-1d.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
