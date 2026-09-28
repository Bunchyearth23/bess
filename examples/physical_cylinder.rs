//! Offline CSV probe; no sound output or physical-engine UI activation.
use bess::{
    engine_build::EngineBuild,
    physical::cylinder::{CylinderPrototype, PrototypeOptions},
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/physical-cylinder"));
    std::fs::create_dir_all(&output)?;
    let rate: u32 = std::env::args()
        .nth(2)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(96000);
    if !(96000..=1000000).contains(&rate) {
        return Err("Probe rate must be 96000–1000000 Hz".into());
    }
    let trace_stride = rate / 4000;
    println!(
        "Fixed speed 1500 RPM, 80 kPa / 310 K intake, 105 kPa / 650 K exhaust. Ideal heat-only combustion, no fuel species. {rate} Hz, 1 s; traces sampled every {trace_stride} steps."
    );
    for fired in [false, true] {
        let name = if fired { "fired" } else { "motored" };
        let path = output.join(format!("{name}.csv"));
        let mut csv = BufWriter::new(File::create(&path)?);
        writeln!(
            csv,
            "time_s,crank_deg,pressure_pa,temperature_k,mass_kg,volume_m3,intake_kg_s,exhaust_kg_s,heat_j,work_j,correction_j"
        )?;
        let mut p = CylinderPrototype::from_build(
            &EngineBuild::default(),
            PrototypeOptions {
                fired,
                ..Default::default()
            },
        )?;
        for step in 0..rate {
            let s = p
                .step(1.0 / f64::from(rate))
                .map_err(|e| format!("step failed: {e:?}"))?;
            if step % trace_stride == 0 {
                writeln!(
                    csv,
                    "{:.9},{:.5},{:.6},{:.6},{:.12},{:.12},{:.9},{:.9},{:.9},{:.9},{:.12}",
                    s.time_s,
                    s.crank_angle_rad.to_degrees(),
                    s.pressure_pa,
                    s.temperature_k,
                    s.mass_kg,
                    s.volume_m3,
                    s.intake_mass_flow_kg_s,
                    s.exhaust_mass_flow_kg_s,
                    s.ledger.heat_j,
                    s.ledger.boundary_work_j,
                    s.ledger.numerical_correction_j
                )?;
            }
        }
        csv.flush()?;
        let m = p.metrics();
        // Separate timing pass: no formatting or filesystem I/O in this loop.
        let mut bench = CylinderPrototype::from_build(
            &EngineBuild::default(),
            PrototypeOptions {
                fired,
                ..Default::default()
            },
        )?;
        for _ in 0..rate / 5 {
            std::hint::black_box(
                bench
                    .step(1.0 / f64::from(rate))
                    .map_err(|e| format!("warmup failed: {e:?}"))?,
            );
        }
        let started = std::time::Instant::now();
        for _ in 0..rate {
            std::hint::black_box(
                bench
                    .step(1.0 / f64::from(rate))
                    .map_err(|e| format!("benchmark failed: {e:?}"))?,
            );
        }
        let us_per_step = started.elapsed().as_secs_f64() * 1e6 / f64::from(rate);
        let summary = format!(
            "{name}: {m:#?}\nTrace: {}\nSeparate single-cylinder benchmark, no CSV I/O, 0.2 s simulated warmup + 1 s measured: {us_per_step:.6} us/step. Host-specific, not a V12 or audio-callback deadline measurement.\n",
            path.display()
        );
        print!("{summary}");
        std::fs::write(output.join(format!("{name}-metrics.txt")), summary)?;
    }
    Ok(())
}
