//! Offline physical-engine audit; no audio device is opened.
//! Usage: physical_engine [output-dir] [measured-seconds=1] [rate=48000] [case=all]
use bess::{
    physical::engine::{Commands, Engine, Sample},
    scratch::{PRESETS, Scratch},
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

fn preset(name: &str) -> Scratch {
    let mut s = Scratch {
        design: PRESETS.iter().find(|p| p.0 == name).unwrap().1,
        ..Default::default()
    };
    s.apply_design();
    s
}

fn order_power(x: &[f32], hz: f64, rate: u32) -> f64 {
    let omega = std::f64::consts::TAU * hz / f64::from(rate);
    let mean = x.iter().map(|&x| f64::from(x)).sum::<f64>() / x.len() as f64;
    let (mut re, mut im) = (0., 0.);
    for (i, &sample) in x.iter().enumerate() {
        let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / x.len() as f64).cos();
        let (s, c) = (omega * i as f64).sin_cos();
        re += (f64::from(sample) - mean) * window * c;
        im -= (f64::from(sample) - mean) * window * s;
    }
    (re * re + im * im) * 4. / (x.len() as f64).powi(2)
}

fn run(
    dir: &Path,
    name: &str,
    scratch: &Scratch,
    seconds: f64,
    rate: u32,
    scenario: &str,
) -> Result<(), Box<dyn Error>> {
    let mut engine = Engine::new(scratch, rate)?;
    let samples = (seconds * f64::from(rate)) as usize;
    let warmup = rate as usize / 2;
    let mut audio = Vec::with_capacity(samples);
    let mut banks = [Vec::with_capacity(samples), Vec::with_capacity(samples)];
    let mut csv = BufWriter::new(File::create(dir.join(format!("{name}.csv")))?);
    writeln!(
        csv,
        "seconds,rpm,map_pa,torque_nm,heat_j,correction_j,exhaust,intake,mechanical,fuel_cut,misfires,idle_bypass,fresh_supply_kg_s,fresh_tailpipe_in_kg_s,fuel_injected_kg,window_frames"
    )?;
    let (mut heat, mut correction, mut energy, mut peak, mut rpm_min, mut rpm_max) =
        (0., 0., 0., 0_f64, f64::MAX, 0_f64);
    let mut final_sample = Sample::default();
    let (mut fuel_total, mut supply_total, mut tailpipe_total) = (0., 0., 0.);
    let mut diagnostic_sum = [0.; 5];
    let mut diagnostic_frames = 0_u32;
    let mut cycle_angle = 0.;
    let mut cycle_work = 0.;
    let mut completed_work = Vec::new();
    let mut cycle_csv = BufWriter::new(File::create(dir.join(format!("{name}-cycles.csv")))?);
    writeln!(cycle_csv, "end_seconds,cycle_work_j,rpm")?;
    let mut failed_at = None;
    let started = Instant::now();
    for i in 0..warmup + samples {
        let t = (i as f64 - warmup as f64) / f64::from(rate);
        let commands = Commands {
            imposed_rpm: if scenario == "idle" && i >= warmup {
                None
            } else {
                Some(if scenario == "dfco" { 3000. } else { 1200. })
            },
            throttle: if scenario == "dfco" && t > seconds * 0.25 || scenario == "idle" {
                0.
            } else {
                0.35
            },
            load_nm: if scenario == "idle" && t > seconds * 0.5 {
                16.
            } else {
                0.
            },
            ..Default::default()
        };
        let s = engine.next(commands);
        if engine.failed() && failed_at.is_none() {
            failed_at = Some(t);
        }
        if [
            s.rpm,
            s.map_pa,
            s.torque_nm,
            s.heat_j,
            s.correction_j,
            f64::from(s.exhaust),
        ]
        .iter()
        .any(|x| !x.is_finite())
        {
            return Err(format!("{name}: nonfinite state at {t}s").into());
        }
        if i >= warmup {
            let delta_angle = s.rpm * std::f64::consts::TAU / (60. * f64::from(rate));
            let remaining = 2. * std::f64::consts::TAU - cycle_angle;
            if delta_angle >= remaining {
                cycle_work += s.torque_nm * remaining;
                completed_work.push(cycle_work);
                writeln!(cycle_csv, "{t:.6},{cycle_work:.9},{:.6}", s.rpm)?;
                cycle_angle = delta_angle - remaining;
                cycle_work = s.torque_nm * cycle_angle;
            } else {
                cycle_angle += delta_angle;
                cycle_work += s.torque_nm * delta_angle;
            }
            audio.push(s.exhaust);
            banks[0].push(s.bank_pressure[0]);
            banks[1].push(s.bank_pressure[1]);
            heat += s.heat_j;
            fuel_total += s.fuel_injected_kg;
            supply_total += s.fresh_supply_kg_s / f64::from(rate);
            tailpipe_total += s.fresh_tailpipe_in_kg_s / f64::from(rate);
            for (sum, value) in diagnostic_sum.iter_mut().zip([
                s.heat_j,
                s.idle_bypass,
                s.fresh_supply_kg_s,
                s.fresh_tailpipe_in_kg_s,
                s.fuel_injected_kg,
            ]) {
                *sum += value;
            }
            diagnostic_frames += 1;
            correction += s.correction_j.abs();
            energy += f64::from(s.exhaust).powi(2);
            peak = peak.max(f64::from(s.exhaust.abs()));
            rpm_min = rpm_min.min(s.rpm);
            rpm_max = rpm_max.max(s.rpm);
            if (i - warmup).is_multiple_of((rate as usize / 1000).max(1)) {
                writeln!(
                    csv,
                    "{t:.6},{:.6},{:.6},{:.6},{:.9},{:.9},{:.9},{:.9},{:.9},{},{},{:.9},{:.12},{:.12},{:.15},{}",
                    s.rpm,
                    s.map_pa,
                    s.torque_nm,
                    diagnostic_sum[0] / f64::from(diagnostic_frames),
                    s.correction_j,
                    s.exhaust,
                    s.intake,
                    s.mechanical,
                    s.fuel_cut,
                    s.misfires,
                    diagnostic_sum[1] / f64::from(diagnostic_frames),
                    diagnostic_sum[2] / f64::from(diagnostic_frames),
                    diagnostic_sum[3] / f64::from(diagnostic_frames),
                    diagnostic_sum[4] / f64::from(diagnostic_frames),
                    diagnostic_frames,
                )?;
                diagnostic_sum = [0.; 5];
                diagnostic_frames = 0;
            }
        }
        final_sample = s;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let rms = (energy / samples as f64).sqrt();
    let recent = &completed_work[completed_work.len().saturating_sub(20)..];
    let mean_work = recent.iter().sum::<f64>() / recent.len().max(1) as f64;
    let work_std = (recent.iter().map(|x| (x - mean_work).powi(2)).sum::<f64>()
        / recent.len().max(1) as f64)
        .sqrt();
    let cov_work = if recent.len() >= 2 && mean_work.abs() > 1e-12 {
        work_std / mean_work.abs() * 100.
    } else {
        f64::NAN
    };
    // IEEE float preserves the raw source and any over-range values for auditing.
    let mut wav = hound::WavWriter::create(
        dir.join(format!("{name}.wav")),
        hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for &s in &audio {
        wav.write_sample(s)?;
    }
    wav.finalize()?;
    let mut orders = BufWriter::new(File::create(dir.join(format!("{name}-orders.csv")))?);
    writeln!(orders, "order,hz,exhaust_power,bank0_power,bank1_power")?;
    if scenario == "direct" {
        for half in 1..=24 {
            let order = f64::from(half) * 0.5;
            let hz = order * 1200. / 60.;
            writeln!(
                orders,
                "{order},{hz},{:.12},{:.12},{:.12}",
                order_power(&audio, hz, rate),
                order_power(&banks[0], hz, rate),
                order_power(&banks[1], hz, rate)
            )?;
        }
    }
    let summary = format!(
        "{name}: failed={failed_at:?}; RMS={rms:.7} ({:.2} dBFS), peak={peak:.7}, heat={heat:.3}J, absolute_correction={correction:.6}J ({:.6}%), RPM={rpm_min:.2}..{rpm_max:.2}, final_RPM={:.2}, final_MAP={:.2}kPa, misfires={}, rolling_work_COV={cov_work:.3}%({}cycles), wall={elapsed:.3}s, ns/output_step={:.1}, realtime_ratio={:.3}",
        20. * rms.max(1e-30).log10(),
        100. * correction / heat.max(1e-30),
        final_sample.rpm,
        final_sample.map_pa / 1000.,
        final_sample.misfires,
        recent.len(),
        elapsed * 1e9 / (warmup + samples) as f64,
        elapsed / (seconds + 0.5)
    );
    let summary = format!(
        "{summary}; heat_power_W={:.3}, fuel_rate_kg_s={:.9}, fresh_supply_rate_kg_s={:.9}, fresh_tailpipe_in_rate_kg_s={:.9}",
        heat / seconds,
        fuel_total / seconds,
        supply_total / seconds,
        tailpipe_total / seconds
    );
    println!("{summary}");
    fs::write(dir.join(format!("{name}-summary.txt")), summary)?;
    if let Some(time) = failed_at {
        return Err(format!(
            "{name}: engine entered failure state at {time}s; audit files preserved"
        )
        .into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(
        args.get(1)
            .map(String::as_str)
            .unwrap_or("outputs/physical-engine"),
    );
    let seconds = args
        .get(2)
        .map(|s| s.parse::<f64>())
        .transpose()?
        .unwrap_or(1.);
    let rate = args
        .get(3)
        .map(|s| s.parse::<u32>())
        .transpose()?
        .unwrap_or(48000);
    if !(0.25..=600.).contains(&seconds) || !(8000..=192000).contains(&rate) {
        return Err("seconds .25–600, rate 8000–192000".into());
    }
    fs::create_dir_all(&dir)?;
    let case = args.get(4).map(String::as_str).unwrap_or("all");
    if ![
        "all",
        "i4",
        "v8",
        "v12",
        "i4-dfco",
        "i4-idle-load",
        "i4-wrong-wiring",
    ]
    .contains(&case)
    {
        return Err(format!("Unknown case: {case}").into());
    }
    for (label, name) in [
        ("i4", "Inline-4"),
        ("v8", "V8 cross-plane"),
        ("v12", "V12 60°"),
    ] {
        if case == "all" || case == label {
            run(&dir, label, &preset(name), seconds, rate, "direct")?;
        }
    }
    let i4 = preset("Inline-4");
    if case == "all" || case == "i4-dfco" {
        run(&dir, "i4-dfco", &i4, seconds, rate, "dfco")?;
    }
    if case == "all" || case == "i4-idle-load" {
        run(&dir, "i4-idle-load", &i4, seconds, rate, "idle")?;
    }
    let mut wrong = i4.clone();
    wrong.design.firing_order[..4].copy_from_slice(&[1, 1, 1, 1]);
    wrong.apply_design();
    if case == "all" || case == "i4-wrong-wiring" {
        run(&dir, "i4-wrong-wiring", &wrong, seconds, rate, "direct")?;
    }
    Ok(())
}
