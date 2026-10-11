//! Offline engine-reference acquisition and bounded identification.
//! Synthetic demonstrations can exercise the pipeline, never qualify realism.
use crate::engine_definition::EngineDefinition;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Measured,
    ManufacturerPublished,
    Synthetic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    pub origin: Origin,
    pub engine_id: String,
    pub source_uri: String,
    pub instrument_and_conditions: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Identification,
    Validation,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Channels {
    /// Mean brake torque, Nm; not instantaneous gas torque.
    pub torque_nm: Option<f64>,
    /// Absolute intake manifold pressure, Pa.
    pub map_pa: Option<f64>,
    /// Air/fuel equivalence ratio; absent if not observed by the simulation.
    pub lambda: Option<f64>,
}
impl Channels {
    fn values(self) -> [(&'static str, Option<f64>); 3] {
        [
            ("torque_nm", self.torque_nm),
            ("map_pa", self.map_pa),
            ("lambda", self.lambda),
        ]
    }
    fn valid(self) -> bool {
        self.values()
            .iter()
            .all(|(_, value)| value.is_none_or(f64::is_finite))
            && self.map_pa.is_none_or(|p| p > 0.)
            && self.lambda.is_none_or(|l| l > 0.)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StationaryPoint {
    pub id: String,
    pub split: Split,
    pub rpm: f64,
    /// Imposed throttle opening, not inferred from measured torque.
    pub throttle: f64,
    pub settle_s: f64,
    pub average_s: f64,
    pub observed: Channels,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Acceptance {
    /// Nonempty record of when/how tolerances were declared before fitting.
    pub declared_before_fitting: String,
    /// Absolute tolerances in the channel's SI unit; missing means not scored.
    pub absolute: Channels,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceSet {
    pub version: u32,
    pub provenance: Provenance,
    pub engine: EngineDefinition,
    pub acceptance: Acceptance,
    pub points: Vec<StationaryPoint>,
}
impl ReferenceSet {
    pub fn validate(&self) -> Result<(), String> {
        self.engine.validate()?;
        if self.version != 1 || !(2..=10_000).contains(&self.points.len()) {
            return Err("Reference version 1 requires 2–10000 points".into());
        }
        if [
            &self.provenance.engine_id,
            &self.provenance.source_uri,
            &self.provenance.instrument_and_conditions,
            &self.acceptance.declared_before_fitting,
        ]
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > 4096)
        {
            return Err("Reference provenance and predeclared acceptance are required".into());
        }
        for (name, tolerance) in self.acceptance.absolute.values() {
            if tolerance.is_some_and(|v| !v.is_finite() || v <= 0.) {
                return Err(format!("{name} tolerance must be positive and finite"));
            }
        }
        let mut ids = BTreeSet::new();
        let mut splits = [false; 2];
        for point in &self.points {
            if point.id.trim().is_empty()
                || point.id.len() > 128
                || !ids.insert(&point.id)
                || !point.rpm.is_finite()
                || !(300.0..=12_000.).contains(&point.rpm)
                || !point.throttle.is_finite()
                || !(0.0..=1.).contains(&point.throttle)
                || !point.settle_s.is_finite()
                || !(0.0..=60.).contains(&point.settle_s)
                || !point.average_s.is_finite()
                || !(0.01..=60.).contains(&point.average_s)
                || !point.observed.valid()
                || point.observed.values().iter().all(|(_, v)| v.is_none())
            {
                return Err(format!(
                    "Invalid or duplicate stationary reference {}",
                    point.id
                ));
            }
            for ((name, observed), (_, tolerance)) in point
                .observed
                .values()
                .into_iter()
                .zip(self.acceptance.absolute.values())
            {
                if observed.is_some() && tolerance.is_none() {
                    return Err(format!("Predeclare {name} tolerance before fitting"));
                }
            }
            splits[usize::from(point.split == Split::Validation)] = true;
        }
        if !splits.into_iter().all(|s| s) {
            return Err("Both identification and reserved validation points are required".into());
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if metadata.len() > 10 * 1024 * 1024 {
            return Err("Reference file exceeds 10 MiB".into());
        }
        let set: Self = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        set.validate()?;
        Ok(set)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prediction {
    pub id: String,
    pub channels: Channels,
}
#[derive(Clone, Debug, Serialize)]
pub struct ChannelError {
    pub point_id: String,
    pub split: Split,
    pub channel: String,
    pub observed: f64,
    pub predicted: f64,
    pub absolute_error: f64,
    pub tolerance: f64,
    pub passed: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    pub origin: Origin,
    pub errors: Vec<ChannelError>,
    pub validation_passed: bool,
    /// Requires genuine measurements as well as all reserved points passing.
    pub measured_qualification: bool,
}
pub fn compare(set: &ReferenceSet, predictions: &[Prediction]) -> Result<Comparison, String> {
    set.validate()?;
    if predictions.len() != set.points.len() {
        return Err("One prediction per reference point is required".into());
    }
    let mut ids = BTreeSet::new();
    if predictions
        .iter()
        .any(|p| !ids.insert(&p.id) || !p.channels.valid())
    {
        return Err("Duplicate prediction or nonfinite channel".into());
    }
    let mut errors = Vec::new();
    for point in &set.points {
        let predicted = predictions
            .iter()
            .find(|p| p.id == point.id)
            .ok_or_else(|| format!("Missing prediction {}", point.id))?;
        for (((name, observed), (_, value)), (_, tolerance)) in point
            .observed
            .values()
            .into_iter()
            .zip(predicted.channels.values())
            .zip(set.acceptance.absolute.values())
        {
            if let Some(observed) = observed {
                let predicted =
                    value.ok_or_else(|| format!("Missing predicted {name} at {}", point.id))?;
                let tolerance = tolerance.expect("validated");
                let absolute_error = (predicted - observed).abs();
                errors.push(ChannelError {
                    point_id: point.id.clone(),
                    split: point.split,
                    channel: name.into(),
                    observed,
                    predicted,
                    absolute_error,
                    tolerance,
                    passed: absolute_error <= tolerance,
                });
            }
        }
    }
    let validation_passed = errors
        .iter()
        .filter(|e| e.split == Split::Validation)
        .all(|e| e.passed);
    let measured_qualification =
        set.provenance.origin == Origin::Measured && errors.iter().all(|e| e.passed);
    Ok(Comparison {
        origin: set.provenance.origin,
        errors,
        validation_passed,
        measured_qualification,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Identification {
    pub parameters: Vec<f64>,
    pub identification_loss: f64,
    pub evaluations: usize,
    pub reserved: Comparison,
}
/// Bounded coordinate search. The objective NEVER reads validation observations.
/// `evaluate` must return physical predictions, with no reference-data fitting.
pub fn identify<F>(
    set: &ReferenceSet,
    bounds: &[(f64, f64)],
    initial: &[f64],
    max_evaluations: usize,
    mut evaluate: F,
) -> Result<Identification, String>
where
    F: FnMut(&[f64], &[StationaryPoint]) -> Result<Vec<Prediction>, String>,
{
    set.validate()?;
    if bounds.len() != initial.len()
        || !(1..=16).contains(&bounds.len())
        || !(3..=1024).contains(&max_evaluations)
        || bounds.iter().zip(initial).any(|(&(lo, hi), &x)| {
            !lo.is_finite() || !hi.is_finite() || !x.is_finite() || lo >= hi || x < lo || x > hi
        })
    {
        return Err("Invalid bounded identification search".into());
    }
    let training: Vec<_> = set
        .points
        .iter()
        .filter(|p| p.split == Split::Identification)
        .cloned()
        .collect();
    let objective = |predictions: Vec<Prediction>| -> Result<f64, String> {
        if predictions.len() != training.len() {
            return Err("Incomplete training predictions".into());
        }
        let mut seen = BTreeSet::new();
        if predictions
            .iter()
            .any(|p| !seen.insert(&p.id) || !p.channels.valid())
        {
            return Err("Invalid training predictions".into());
        }
        let mut sum = 0.;
        let mut count = 0.;
        for point in &training {
            let predicted = predictions
                .iter()
                .find(|p| p.id == point.id)
                .ok_or_else(|| "Missing training prediction".to_string())?;
            for (((_, observed), (name, value)), (_, tolerance)) in point
                .observed
                .values()
                .into_iter()
                .zip(predicted.channels.values())
                .zip(set.acceptance.absolute.values())
            {
                if let Some(observed) = observed {
                    let value = value.ok_or_else(|| format!("Missing training {name}"))?;
                    sum += ((value - observed) / tolerance.expect("validated")).powi(2);
                    count += 1.;
                }
            }
        }
        let loss = sum / count;
        if !loss.is_finite() {
            return Err("Nonfinite identification loss".into());
        }
        Ok(loss)
    };
    let mut parameters = initial.to_vec();
    let mut steps: Vec<_> = bounds.iter().map(|(lo, hi)| (hi - lo) * 0.25).collect();
    let mut loss = objective(evaluate(&parameters, &training)?)?;
    let mut evaluations = 1;
    while evaluations + 1 < max_evaluations && steps.iter().any(|s| *s > 1e-8) {
        let mut improved = false;
        for axis in 0..parameters.len() {
            for direction in [-1., 1.] {
                if evaluations + 1 >= max_evaluations {
                    break;
                }
                let mut candidate = parameters.clone();
                candidate[axis] = (candidate[axis] + direction * steps[axis])
                    .clamp(bounds[axis].0, bounds[axis].1);
                if candidate[axis] == parameters[axis] {
                    continue;
                }
                let next = objective(evaluate(&candidate, &training)?)?;
                evaluations += 1;
                if next < loss {
                    parameters = candidate;
                    loss = next;
                    improved = true;
                }
            }
        }
        if !improved {
            for step in &mut steps {
                *step *= 0.5;
            }
        }
    }
    let reserved = compare(set, &evaluate(&parameters, &set.points)?)?;
    evaluations += 1;
    Ok(Identification {
        parameters,
        identification_loss: loss,
        evaluations,
        reserved,
    })
}

/// Constant-RPM order amplitudes using a Hann window and coherent-gain correction.
/// These are crankshaft orders (frequency = order × rpm/60), not cylinder guesses.
pub fn order_amplitudes(
    samples: &[f32],
    sample_rate: u32,
    rpm: f64,
    orders: &[f64],
) -> Result<Vec<f64>, String> {
    if samples.len() < 64
        || samples.iter().any(|s| !s.is_finite())
        || sample_rate < 1000
        || !rpm.is_finite()
        || rpm <= 0.
        || orders.len() > 128
        || orders
            .iter()
            .any(|o| !o.is_finite() || *o <= 0. || o * rpm / 60. >= f64::from(sample_rate) * 0.5)
    {
        return Err("Invalid stationary order-analysis input".into());
    }
    let mean = samples.iter().map(|s| f64::from(*s)).sum::<f64>() / samples.len() as f64;
    let mut amplitudes = Vec::with_capacity(orders.len());
    for order in orders {
        let omega = std::f64::consts::TAU * order * rpm / (60. * f64::from(sample_rate));
        let (mut re, mut im, mut weight) = (0., 0., 0.);
        for (i, sample) in samples.iter().enumerate() {
            let window =
                0.5 * (1. - (std::f64::consts::TAU * i as f64 / (samples.len() - 1) as f64).cos());
            let angle = omega * i as f64;
            let value = (f64::from(*sample) - mean) * window;
            re += value * angle.cos();
            im += value * angle.sin();
            weight += window;
        }
        amplitudes.push(2. * re.hypot(im) / weight);
    }
    Ok(amplitudes)
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Alignment {
    pub candidate_lag_samples: isize,
    pub correlation: f64,
}
/// Positive lag means the candidate signal arrives later. No clipping or gain matching.
pub fn align(reference: &[f32], candidate: &[f32], max_lag: usize) -> Result<Alignment, String> {
    if reference.len().min(candidate.len()) < 64
        || max_lag > 4096
        || max_lag + 32 >= reference.len().min(candidate.len())
        || reference.iter().chain(candidate).any(|s| !s.is_finite())
    {
        return Err("Invalid bounded alignment input".into());
    }
    let mut best = Alignment {
        candidate_lag_samples: 0,
        correlation: -2.,
    };
    for lag in -(max_lag as isize)..=max_lag as isize {
        let a0 = (-lag).max(0) as usize;
        let b0 = lag.max(0) as usize;
        let n = (reference.len() - a0).min(candidate.len() - b0);
        let a = &reference[a0..a0 + n];
        let b = &candidate[b0..b0 + n];
        let ma = a.iter().map(|v| f64::from(*v)).sum::<f64>() / n as f64;
        let mb = b.iter().map(|v| f64::from(*v)).sum::<f64>() / n as f64;
        let (mut dot, mut aa, mut bb) = (0., 0., 0.);
        for (&x, &y) in a.iter().zip(b) {
            let x = f64::from(x) - ma;
            let y = f64::from(y) - mb;
            dot += x * y;
            aa += x * x;
            bb += y * y;
        }
        if aa > 1e-20 && bb > 1e-20 {
            let correlation = dot / (aa * bb).sqrt();
            if correlation > best.correlation {
                best = Alignment {
                    candidate_lag_samples: lag,
                    correlation,
                };
            }
        }
    }
    if best.correlation < -1. {
        return Err("Alignment requires nonsilent varying signals".into());
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference() -> ReferenceSet {
        ReferenceSet {
            version: 1,
            engine: EngineDefinition::default(),
            provenance: Provenance {
                origin: Origin::Synthetic,
                engine_id: "test".into(),
                source_uri: "synthetic:test".into(),
                instrument_and_conditions: "Explicit tool fixture".into(),
            },
            acceptance: Acceptance {
                declared_before_fitting: "fixture contract".into(),
                absolute: Channels {
                    torque_nm: Some(0.01),
                    ..Channels::default()
                },
            },
            points: [Split::Identification, Split::Validation]
                .into_iter()
                .enumerate()
                .map(|(i, split)| StationaryPoint {
                    id: i.to_string(),
                    split,
                    rpm: 2000.,
                    throttle: 0.5,
                    settle_s: 0.2,
                    average_s: 0.1,
                    observed: Channels {
                        torque_nm: Some(2.),
                        ..Channels::default()
                    },
                })
                .collect(),
        }
    }
    #[test]
    fn heldout_data_cannot_steer_identification_and_synthetic_never_qualifies() {
        let set = reference();
        let evaluator = |p: &[f64], points: &[StationaryPoint]| {
            Ok(points
                .iter()
                .map(|point| Prediction {
                    id: point.id.clone(),
                    channels: Channels {
                        torque_nm: Some(p[0]),
                        ..Channels::default()
                    },
                })
                .collect())
        };
        let a = identify(&set, &[(0., 4.)], &[0.], 24, evaluator).unwrap();
        let mut altered = set.clone();
        altered.points[1].observed.torque_nm = Some(999.);
        let b = identify(&altered, &[(0., 4.)], &[0.], 24, evaluator).unwrap();
        assert_eq!(a.parameters, b.parameters);
        assert_eq!(a.identification_loss, b.identification_loss);
        assert!(a.reserved.validation_passed && !a.reserved.measured_qualification);
        assert!(!b.reserved.validation_passed);
    }
    #[test]
    fn missing_channels_duplicates_and_posthoc_tolerances_are_rejected() {
        let mut set = reference();
        set.acceptance.absolute.torque_nm = None;
        assert!(set.validate().is_err());
        let mut set = reference();
        set.points[1].id = "0".into();
        assert!(set.validate().is_err());
        let set = reference();
        assert!(
            compare(
                &set,
                &[
                    Prediction {
                        id: "0".into(),
                        channels: Channels::default()
                    },
                    Prediction {
                        id: "1".into(),
                        channels: Channels::default()
                    }
                ]
            )
            .is_err()
        );
    }
    #[test]
    fn published_ratings_are_not_a_measured_multichannel_qualification() {
        let mut set = reference();
        set.provenance.origin = Origin::ManufacturerPublished;
        let predictions: Vec<_> = set
            .points
            .iter()
            .map(|p| Prediction {
                id: p.id.clone(),
                channels: p.observed,
            })
            .collect();
        let result = compare(&set, &predictions).unwrap();
        assert!(result.validation_passed);
        assert!(!result.measured_qualification);
        set.provenance.origin = Origin::Measured;
        let mut predictions = predictions;
        predictions[0].channels.torque_nm = Some(99.);
        let result = compare(&set, &predictions).unwrap();
        assert!(result.validation_passed);
        assert!(!result.measured_qualification);
    }
    #[test]
    fn order_tracking_recovers_half_orders_and_rejects_aliasing() {
        let rate = 8000;
        let rpm = 1200.;
        let samples: Vec<_> = (0..8000)
            .map(|i| {
                (0.3 * (std::f64::consts::TAU * 30. * i as f64 / rate as f64).sin()
                    + 0.1 * (std::f64::consts::TAU * 40. * i as f64 / rate as f64).sin()
                    + 0.2) as f32
            })
            .collect();
        let amplitudes = order_amplitudes(&samples, rate, rpm, &[1.5, 2., 3.]).unwrap();
        assert!((amplitudes[0] - 0.3).abs() < 1e-4);
        assert!((amplitudes[1] - 0.1).abs() < 1e-4);
        assert!(amplitudes[2] < 1e-5);
        assert!(order_amplitudes(&samples, rate, rpm, &[200.]).is_err());
    }
    #[test]
    fn alignment_finds_delay_and_rejects_silence() {
        let mut state = 11_u32;
        let reference: Vec<_> = (0..512)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                state as f32 / u32::MAX as f32 - 0.5
            })
            .collect();
        let mut candidate = vec![0.; 17];
        candidate.extend_from_slice(&reference);
        let found = align(&reference, &candidate, 32).unwrap();
        assert_eq!(found.candidate_lag_samples, 17);
        assert!(found.correlation > 0.999);
        assert!(align(&[0.; 128], &[0.; 128], 10).is_err());
    }
}
