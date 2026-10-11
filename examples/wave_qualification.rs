//! Three separate X-028 diagnostics. None replaces the historical 10% gate.
use bdsp::delay::DelayLine;
use bess::{
    dyno,
    engine_build::Headers,
    physical::{
        gas::{GasProperties, Orifice},
        wave_junction::{ValveFlow, WavePort},
    },
    scratch::Scratch,
};
use serde::Serialize;
use std::{fmt::Write, path::Path};

#[derive(Debug, Serialize)]
struct Fit {
    factor: f64,
    error_pct: f64,
    rms_mismatch: f64,
    pairs: usize,
    max_gain: f64,
    passes_historical_limits: bool,
}

fn interpolate(rpm: &[f64], values: &[f64], x: f64) -> Option<f64> {
    let i = rpm.partition_point(|&v| v < x);
    (i > 0 && i < rpm.len()).then(|| {
        values[i - 1] + (values[i] - values[i - 1]) * (x - rpm[i - 1]) / (rpm[i] - rpm[i - 1])
    })
}

fn fit(rpm: &[f64], reference: &[f64], response: &[f64], scale: f64, common: bool) -> Fit {
    let (mut best, mut error, mut pairs) = (0., f64::INFINITY, 0);
    for step in 0..=400 {
        let (lo, hi) = if common {
            (scale * 0.8, scale * 1.2)
        } else {
            (0.4, 2.5)
        };
        let factor = lo * (hi / lo).powf(f64::from(step) / 400.);
        let (mut sum, mut count) = (0., 0);
        for (&n, &v) in rpm.iter().zip(response) {
            // Diagnostic only: use a fixed domain shared by all tested factors.
            // Keep the historical variable-overlap estimator available intact.
            if common && (n * lo <= rpm[0] || n * hi > rpm[rpm.len() - 1]) {
                continue;
            }
            if let Some(w) = interpolate(rpm, reference, n * factor) {
                sum += (v - w).powi(2);
                count += 1;
            }
        }
        if count >= 10 && sum / (count as f64) < error {
            (best, error, pairs) = (factor, sum / count as f64, count);
        }
    }
    let gain = response.iter().copied().fold(0_f64, f64::max);
    let error_pct = 100. * (best / scale - 1.);
    Fit {
        factor: best,
        error_pct,
        rms_mismatch: error.sqrt(),
        pairs,
        max_gain: gain,
        passes_historical_limits: !common
            && pairs >= 10
            && gain > 0.1
            && error_pct.abs() < 10.
            && error.sqrt() < 0.5 * gain,
    }
}

#[derive(Serialize)]
struct Echo {
    rate: u32,
    length_m: f64,
    predicted_roundtrip_s: f64,
    measured_roundtrip_s: f64,
    timing_error_pct: f64,
}

fn echo(rate: u32, length: f64) -> Echo {
    let speed = (1.33_f64 * 287. * 673.).sqrt();
    // A closed valve and a pressure-release end: the first negative return
    // traverses exactly two production DelayLine objects, read before write.
    let mut forward = DelayLine::new(rate as f32, 0.03);
    let mut backward = DelayLine::new(rate as f32, 0.03);
    let delay = (length * f64::from(rate) / speed) as f32;
    let n = (2. * f64::from(delay)).ceil() as usize + 8;
    let (mut moment, mut amplitude) = (0., 0.);
    for i in 0..n {
        let incident = forward.read_at(delay);
        let returning = backward.read_at(delay);
        forward.write(if i == 0 { 1. } else { 0. });
        backward.write(-incident);
        moment += i as f64 * f64::from(returning.abs());
        amplitude += f64::from(returning.abs());
    }
    assert!(amplitude > 0.99 && amplitude < 1.01);
    let measured = moment / amplitude / f64::from(rate);
    let predicted = 2. * length / speed;
    Echo {
        rate,
        length_m: length,
        predicted_roundtrip_s: predicted,
        measured_roundtrip_s: measured,
        timing_error_pct: 100. * (measured / predicted - 1.),
    }
}

#[derive(Serialize)]
struct Valve {
    cylinder_pa: f64,
    impedance_pa_s_kg: f64,
    reference_pa: f64,
    reflection: f64,
    flow_residual_kg_s: f64,
    pressure_residual_pa: f64,
}

fn valve(cylinder_pa: f64, impedance: f64, reference_pa: f64) -> Valve {
    let gas = GasProperties {
        gas_constant_j_kg_k: 287.,
        gamma: 1.33,
    };
    let flow = ValveFlow::new(
        Orifice {
            area_m2: 6e-4,
            discharge_coefficient: 0.7,
        },
        673.,
        cylinder_pa,
        900.,
        gas,
        gas,
    );
    let solve = |incoming: f64| {
        let port = WavePort {
            mean_pa: 105000.,
            wave_pa: 2. * incoming,
            reference_pa,
            gamma: 1.33,
            impedance,
            guess_inflow_kg_s: 0.,
        };
        let (mass, pressure) = port.solve(cylinder_pa, |p| flow.evaluate(p));
        (port.wave_sum(mass) - incoming, mass, pressure, port)
    };
    let (outgoing, mass, pressure, port) = solve(0.);
    let reflection = (solve(1.).0 - solve(-1.).0) / 2.;
    assert!(outgoing.is_finite());
    Valve {
        cylinder_pa,
        impedance_pa_s_kg: impedance,
        reference_pa,
        reflection,
        flow_residual_kg_s: mass - flow.evaluate(pressure).0,
        pressure_residual_pa: pressure - port.port_pressure(mass),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = bess::realtime::DenormalGuard::enter();
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "output/wave-qualification".into());
    let dir = Path::new(&directory);
    std::fs::create_dir_all(dir)?;
    let echoes: Vec<_> = [48000, 96000]
        .into_iter()
        .flat_map(|rate| {
            [0.49, 0.7, 0.98]
                .into_iter()
                .map(move |length| echo(rate, length))
        })
        .collect();
    let mut valves = Vec::new();
    for pressure in [104000., 106000., 130000., 600000.] {
        for impedance in [1e4, 1e5, 1e6] {
            for reference in [0., 105000.] {
                valves.push(valve(pressure, impedance, reference));
            }
        }
    }
    let mut csv = String::from(
        "rate,scale,rpm,torque_0d_nm,torque_anechoic_nm,torque_coupled_nm,ratio_0d,ratio_anechoic,map_kpa\n",
    );
    let mut fits = Vec::new();
    let mut configurations = Vec::new();
    for rate in [48000, 96000] {
        let mut scratch = Scratch::default();
        scratch.build.headers = Headers::EqualLength;
        let baseline = dyno::sweep_at_rate(&scratch, 31, rate)?;
        scratch.experimental.wave_coupling = true;
        let anechoic = dyno::sweep_anechoic_reference(&scratch, 31, rate)?;
        let mut curves = Vec::new();
        for scale in [0.7, 1., 1.4] {
            scratch.sound.primary_length_scale = scale;
            configurations.push(serde_json::json!({"rate":rate,"scratch":scratch}));
            curves.push(dyno::sweep_at_rate(&scratch, 31, rate)?);
        }
        for (label, base) in [
            ("historical_0d", &baseline),
            ("diagnostic_anechoic", &anechoic),
        ] {
            let ratios: Vec<Vec<f64>> = curves
                .iter()
                .map(|c| {
                    c.torque_nm
                        .iter()
                        .zip(&base.torque_nm)
                        .map(|(a, b)| a / b - 1.)
                        .collect()
                })
                .collect();
            for (index, scale) in [(0, f64::from(0.7_f32)), (2, f64::from(1.4_f32))] {
                let result = fit(&base.rpm, &ratios[1], &ratios[index], scale, false);
                println!(
                    "{rate} {label} x{scale:.1}: {:.3}% error, RMS {:.5}, pass {}",
                    result.error_pct, result.rms_mismatch, result.passes_historical_limits
                );
                fits.push(serde_json::json!({"rate":rate,"reference":label,"scale":scale,
                    "historical_estimator":result,
                    "fixed_overlap_diagnostic":fit(&base.rpm,&ratios[1],&ratios[index],scale,true)}));
            }
        }
        for (scale, curve) in [0.7, 1., 1.4].iter().zip(&curves) {
            for i in 0..curve.rpm.len() {
                writeln!(
                    csv,
                    "{rate},{scale},{},{},{},{},{},{},{}",
                    curve.rpm[i],
                    baseline.torque_nm[i],
                    anechoic.torque_nm[i],
                    curve.torque_nm[i],
                    curve.torque_nm[i] / baseline.torque_nm[i] - 1.,
                    curve.torque_nm[i] / anechoic.torque_nm[i] - 1.,
                    curve.map_kpa[i]
                )?;
            }
        }
    }
    std::fs::write(dir.join("curves.csv"), csv)?;
    std::fs::write(
        dir.join("diagnostics.json"),
        serde_json::to_string_pretty(&serde_json::json!({
        "historical_gate_unchanged":true,"measurement_origin":"synthetic model diagnostic",
        "configurations":configurations,
            "tube_first_echo":echoes,"constant_state_valves":valves,"full_engine":fits,
            "limits":"Tube timing and fixed-state valve passivity do not establish whole-engine 1/L or measured fidelity. Fixed-overlap uses a restricted 0.8*scale..1.2*scale scan with a constant domain; it and anechoic results are diagnostics only."
        }))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_delay_first_echo_obeys_length_at_both_rates() {
        for rate in [48000, 96000] {
            for length in [0.49, 0.7, 0.98] {
                assert!(echo(rate, length).timing_error_pct.abs() < 0.01);
            }
        }
    }
    #[test]
    fn fixed_valve_is_incrementally_passive_and_closes_the_two_relations() {
        for pressure in [104000., 106000., 130000., 600000.] {
            for impedance in [1e4, 1e5, 1e6] {
                for reference in [0., 105000.] {
                    let v = valve(pressure, impedance, reference);
                    assert!(v.reflection.abs() <= 1. + 1e-6);
                    assert!(v.flow_residual_kg_s.abs() < 1e-8);
                    assert!(v.pressure_residual_pa.abs() < 0.01);
                }
            }
        }
    }
    #[test]
    fn historical_estimator_recovers_a_known_shift_without_changing_its_limits() {
        let rpm: Vec<_> = (0..31).map(|i| 850. + f64::from(i) * 200.).collect();
        let reference: Vec<_> = rpm.iter().map(|n| 0.15 * (n / 700.).sin()).collect();
        let scale = 0.7;
        let response: Vec<_> = rpm
            .iter()
            .map(|n| interpolate(&rpm, &reference, n * scale).unwrap_or(0.))
            .collect();
        let result = fit(&rpm, &reference, &response, scale, false);
        assert!(result.passes_historical_limits, "{result:?}");
        assert!(result.error_pct.abs() < 1.);
        let diagnostic = fit(&rpm, &reference, &response, scale, true);
        assert!(diagnostic.pairs >= 10);
        assert!(diagnostic.error_pct.abs() < 1.);
        assert!(!diagnostic.passes_historical_limits);
    }
}
