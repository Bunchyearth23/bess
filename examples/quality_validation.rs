//! Numerical quality-primary comparison and synthetic held-out calibration.
//! cargo run --release --example quality_validation -- <new-output-directory>
use bdsp::delay::DelayLine;
use bess::{
    calibration::{self, CalibrationConfig},
    physical::{
        engine::{Commands, Engine},
        finite_volume::Primary1d,
    },
    scratch::Scratch,
};
use serde_json::json;
use std::{f64::consts::TAU, fs, path::Path, time::Instant};

fn wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    bess::render::write_pcm(path, samples)
}

fn run(output: &Path) -> Result<(), String> {
    fs::create_dir(output).map_err(|e| format!("Choose a new report directory: {e}"))?;
    let rate = 48000;
    let c = (1.33_f64 * 287. * 673.).sqrt();
    let mut rows = Vec::new();
    let start = Instant::now();
    for max_cell_m in [0.02, 0.004] {
        for length in [0.23, 0.55, 0.7] {
            for hz in [80., 125., 250., 500., 1000., 2000., 4000.] {
                let mut primary = Primary1d::new(length, 0.001, rate, 101325., 673., 1.33, 287.)?
                    .with_cell_size(max_cell_m)?;
                let mut guide = DelayLine::new(rate as f32, 0.02);
                let delay = length * f64::from(rate) / c;
                let (mut fv, mut wg) = ([0.; 2], [0.; 2]);
                let frames = (f64::from(rate) * 0.18) as usize;
                let begin = (f64::from(rate) * 0.08) as usize;
                let case_start = Instant::now();
                for frame in 0..frames {
                    let phase = TAU * hz * frame as f64 / f64::from(rate);
                    let ramp = (frame as f64 / (f64::from(rate) * 0.01)).min(1.);
                    let incoming = 10. * phase.sin() * ramp;
                    let a = primary.arrivals()[0];
                    let b = f64::from(guide.read_at(delay as f32));
                    if frame >= begin {
                        fv[0] += a * phase.sin();
                        fv[1] += a * phase.cos();
                        wg[0] += b * phase.sin();
                        wg[1] += b * phase.cos();
                    }
                    primary.step(incoming, 0.)?;
                    guide.write(incoming as f32);
                }
                let fv_amplitude = fv[0].hypot(fv[1]);
                let wg_amplitude = wg[0].hypot(wg[1]);
                let error_db = 20. * (fv_amplitude / wg_amplitude).log10();
                let phase_error_degrees = (fv[1].atan2(fv[0]) - wg[1].atan2(wg[0])).to_degrees();
                rows.push(json!({"length_m":length,"frequency_hz":hz,"fv_vs_waveguide_db":error_db,
                "requested_max_cell_m":max_cell_m,"actual_cell_m":primary.cell_size_m(),
                "elapsed_seconds":case_start.elapsed().as_secs_f64(),"simulated_seconds":0.18,
                "phase_error_degrees":phase_error_degrees,"within_2_db":error_db.abs() < 2.,"diagnostics":primary.diagnostics()}));
                println!(
                    "primary grid {max_cell_m:.3} m, {length:.2} m {hz:4.0} Hz: {error_db:+.4} dB"
                );
            }
        }
    }
    let propagation_seconds = start.elapsed().as_secs_f64();
    let worst_db = rows
        .iter()
        .map(|v| v["fv_vs_waveguide_db"].as_f64().unwrap().abs())
        .fold(0_f64, f64::max);
    let mut engine_cases = Vec::new();
    for (rpm, throttle) in [(900., 0.1), (3000., 0.8)] {
        let mut peaks = [0_f32; 2];
        let mut energy = [0_f64; 2];
        let mut failures = [None; 2];
        let mut rendering_seconds = [0_f64; 2];
        let started = Instant::now();
        let frames = rate / 5;
        for (index, quality) in [false, true].into_iter().enumerate() {
            let mut scratch = Scratch::default();
            scratch.experimental.primary_1d = quality;
            let mut engine = Engine::new(&scratch, rate)?;
            let render_start = Instant::now();
            for frame in 0..frames {
                let sample = engine.next(Commands {
                    imposed_rpm: Some(rpm),
                    throttle,
                    overrun: 0.,
                    ..Default::default()
                });
                peaks[index] = peaks[index].max(sample.exhaust.abs());
                if engine.failed() {
                    failures[index] = Some(frame);
                    break;
                }
                if frame >= frames / 2 {
                    energy[index] += f64::from(sample.exhaust).powi(2);
                }
            }
            rendering_seconds[index] = render_start.elapsed().as_secs_f64();
        }
        let delta_db = if failures.iter().all(Option::is_none) && energy.iter().all(|e| *e > 0.) {
            Some(10. * (energy[1] / energy[0]).log10())
        } else {
            None
        };
        engine_cases.push(json!({"rpm":rpm,"throttle":throttle,"requested_frames":frames,
            "failure_frames_waveguide_fv":failures,"peak_raw_exhaust_waveguide_fv":peaks,
            "quality_vs_waveguide_rms_db":delta_db,"elapsed_seconds":started.elapsed().as_secs_f64(),
            "rendering_seconds_waveguide_fv":rendering_seconds,
            "wall_seconds_per_simulated_second_waveguide_fv":rendering_seconds.map(|t|t/0.2),
            "duration_seconds":0.2,"measurement_seconds":0.1,"settled_claim":false}));
        println!(
            "integrated engine {rpm:.0} rpm: failures={failures:?}, FV/guide RMS={delta_db:?} dB"
        );
    }
    let mut rng = 314159265_u32;
    let source: Vec<f32> = (0..rate * 2)
        .map(|_| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            ((f64::from(rng) / f64::from(u32::MAX) - 0.5) * 0.08) as f32
        })
        .collect();
    let reference: Vec<f32> = source
        .iter()
        .enumerate()
        .map(|(i, &x)| 0.8 * x + if i > 0 { 0.45 * source[i - 1] } else { 0. })
        .collect();
    let source_path = output.join("synthetic-generated.wav");
    let reference_path = output.join("synthetic-reference.wav");
    wav(&source_path, &source)?;
    wav(&reference_path, &reference)?;
    let calibration = calibration::calibrate_wavs(
        &source_path,
        &reference_path,
        &output.join("calibrated"),
        CalibrationConfig::default(),
    )?;
    let mesh_summaries = [0.02, 0.004].map(|cell| {
        let subset: Vec<_> = rows
            .iter()
            .filter(|v| v["requested_max_cell_m"].as_f64() == Some(cell))
            .collect();
        let maximum = |max_hz: f64| {
            subset
                .iter()
                .filter(|v| v["frequency_hz"].as_f64().unwrap() <= max_hz)
                .map(|v| v["fv_vs_waveguide_db"].as_f64().unwrap().abs())
                .fold(0_f64, f64::max)
        };
        let elapsed: f64 = subset
            .iter()
            .map(|v| v["elapsed_seconds"].as_f64().unwrap())
            .sum();
        json!({"requested_max_cell_m":cell,"low_order_worst_error_db":maximum(1000.),
            "all_tested_worst_error_db":maximum(4000.),"elapsed_seconds":elapsed,
            "wall_seconds_per_simulated_second":elapsed/(subset.len() as f64*0.18)})
    });
    let report = json!({
        "kind":"offline_numerical_and_synthetic_validation",
        "sample_rate":rate,"amplitude_pa":10.,"temperature_k":673.,"pressure_pa":101325.,
        "gamma":1.33,"gas_constant":287.,"primary_area_m2":0.001,
        "propagation_elapsed_seconds":propagation_seconds,"worst_primary_error_db":worst_db,
        "low_order_band_hz":[80,1000],"primary_mesh_summaries":mesh_summaries,
        "primary_all_within_2_db":worst_db < 2.,"primary_comparison":rows,
        "integrated_engine_cases":engine_cases,
        "calibration":calibration,
        "limits":"Small-signal single constant-area primary compared to a lossless fractional-delay waveguide. Identical downstream losses are omitted. Does not validate the complete collector/muffler network, combustion, large-amplitude coupling, wall losses or real vehicle sound. Calibration reference is an explicitly synthetic linear FIR transform, never an authentic engine recording."
    });
    fs::write(
        output.join("quality-validation.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "Worst primary magnitude difference: {worst_db:.4} dB; elapsed {propagation_seconds:.2} s"
    );
    println!(
        "Held-out synthetic calibration: {:.4} -> {:.4} dB RMSE",
        report["calibration"]["held_out"]["before_rmse_db"]
            .as_f64()
            .unwrap(),
        report["calibration"]["held_out"]["after_rmse_db"]
            .as_f64()
            .unwrap()
    );
    Ok(())
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: quality_validation <new-output-directory>");
        std::process::exit(2);
    };
    if let Err(error) = run(Path::new(&path)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
