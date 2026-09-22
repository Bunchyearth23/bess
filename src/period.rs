//! Import-time local period refinement. This never identifies a firing order.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Period {
    pub nominal: f64,
    pub measured: f64,
    pub confidence: f64,
    pub accepted: bool,
}

fn peak(raw: &[f32], nominal: f64) -> Option<(f64, f64)> {
    let lo = (nominal * 0.97).floor().max(2.) as usize;
    let hi = (nominal * 1.03).ceil() as usize;
    if raw.len() < hi * 3 || hi <= lo + 2 {
        return None;
    }
    // Bounded reference positions over the whole window, without allocating
    // in proportion to its length. This is only called by the import worker.
    let count = (raw.len() - hi).min(4096);
    let score = |lag: usize| {
        let (mut xy, mut xx, mut yy) = (0., 0., 0.);
        for j in 0..count {
            let i = j * (raw.len() - hi) / count;
            let (x, y) = (raw[i] as f64, raw[i + lag] as f64);
            xy += x * y;
            xx += x * x;
            yy += y * y;
        }
        xy / (xx * yy).sqrt().max(1e-20)
    };
    let scores: Vec<_> = (lo..=hi).map(score).collect();
    let (index, &confidence) = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))?;
    if index == 0 || index + 1 == scores.len() || confidence < 0.65 {
        return None;
    }
    let (a, b, c) = (scores[index - 1], scores[index], scores[index + 1]);
    let curvature = a - 2. * b + c;
    if curvature >= -1e-10 {
        return None;
    }
    let delta = (0.5 * (a - c) / curvature).clamp(-0.5, 0.5);
    Some(((lo + index) as f64 + delta, confidence))
}

pub fn estimate(raw: &[f32], nominal: f64) -> Period {
    let fallback = Period {
        nominal,
        measured: nominal,
        confidence: 0.,
        accepted: false,
    };
    if !nominal.is_finite() || nominal < 4. || (raw.len() as f64) < nominal * 8. {
        return fallback;
    }
    let Some((a, qa)) = peak(&raw[..raw.len() / 2], nominal) else {
        return fallback;
    };
    let Some((b, qb)) = peak(&raw[raw.len() / 2..], nominal) else {
        return fallback;
    };
    // Reject changing/ambiguous periods; do not force an unstable source onto
    // a guessed engine cycle. Half-window estimates must agree within 0.15%.
    if (a - b).abs() / nominal > 0.0015 {
        return fallback;
    }
    let Some((period, q)) = peak(raw, nominal) else {
        return fallback;
    };
    Period {
        nominal,
        measured: period,
        confidence: q.min(qa).min(qb),
        accepted: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn signal(period: f64) -> Vec<f32> {
        (0..96000)
            .map(|i| {
                let p = std::f64::consts::TAU * i as f64 / period;
                (0.3 * p.sin() + 0.17 * (4. * p + 0.7).sin() + 0.08 * (11. * p).sin()) as f32
            })
            .collect()
    }
    #[test]
    fn finds_fractional_period_without_harmonic_octave_jump() {
        for actual in [790.3, 804.7, 1608.2] {
            let nominal = if actual > 1000. { 1600. } else { 800. };
            let p = estimate(&signal(actual), nominal);
            assert!(p.accepted, "{p:?}");
            assert!((p.measured - actual).abs() < 0.1, "{p:?}");
        }
    }
    #[test]
    fn rejects_silence_drift_and_out_of_range_period() {
        assert!(!estimate(&vec![0.; 96000], 800.).accepted);
        assert!(!estimate(&signal(900.), 800.).accepted);
        let mut drifting = signal(790.);
        drifting[48000..].copy_from_slice(&signal(810.)[..48000]);
        assert!(!estimate(&drifting, 800.).accepted);
        let mut seed = 37u32;
        let noise: Vec<_> = (0..96000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                seed as f32 / u32::MAX as f32 - 0.5
            })
            .collect();
        assert!(!estimate(&noise, 800.).accepted);
    }
}
