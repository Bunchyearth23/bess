//! Display-only spectrum of the live output: log-spaced Goertzel bins over a
//! Hann-windowed block, smoothed in time so it reads like an analyser.
pub const WINDOW: usize = 8192;
pub const BINS: usize = 240;
pub const LOW_HZ: f32 = 20.;
pub const HIGH_HZ: f32 = 20_000.;
pub const FLOOR_DB: f32 = -100.;

pub struct Spectrum {
    pub db: [f32; BINS],
    window: Vec<f32>,
    coefficients: [f32; BINS],
}

pub fn bin_hz(bin: usize) -> f32 {
    LOW_HZ * (HIGH_HZ / LOW_HZ).powf(bin as f32 / (BINS - 1) as f32)
}

impl Spectrum {
    pub fn new(rate: f32) -> Self {
        let window: Vec<f32> = (0..WINDOW)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (WINDOW - 1) as f32).cos())
            .collect();
        let mut this = Self {
            db: [FLOOR_DB; BINS],
            window,
            coefficients: [0.; BINS],
        };
        this.set_rate(rate);
        this
    }
    pub fn set_rate(&mut self, rate: f32) {
        self.coefficients = std::array::from_fn(|bin| {
            2. * (std::f32::consts::TAU * bin_hz(bin).min(rate * 0.49) / rate).cos()
        });
    }
    /// Analyse `block` (oldest sample first) and move the display toward it:
    /// fast rise, slow fall, like an analyser's ballistics.
    pub fn update(&mut self, block: &[f32; WINDOW]) {
        let gain = 2. / self.window.iter().sum::<f32>();
        for bin in 0..BINS {
            let c = self.coefficients[bin];
            let (mut s1, mut s2) = (0f32, 0f32);
            for (x, w) in block.iter().zip(&self.window) {
                let s0 = x * w + c * s1 - s2;
                s2 = s1;
                s1 = s0;
            }
            let magnitude = (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.).sqrt() * gain;
            let db = (20. * magnitude.max(1e-6).log10()).max(FLOOR_DB);
            let rate = if db > self.db[bin] { 0.8 } else { 0.35 };
            self.db[bin] += (db - self.db[bin]) * rate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_peaks_in_its_bin_at_its_level() {
        let mut spectrum = Spectrum::new(48_000.);
        let target = 120;
        let hz = bin_hz(target);
        let block: [f32; WINDOW] = std::array::from_fn(|i| {
            0.5 * (std::f32::consts::TAU * hz * i as f32 / 48_000.).sin()
        });
        for _ in 0..40 {
            spectrum.update(&block);
        }
        let loudest = (0..BINS).max_by(|&a, &b| spectrum.db[a].total_cmp(&spectrum.db[b]));
        assert_eq!(loudest, Some(target));
        assert!((spectrum.db[target] - 20. * 0.5f32.log10()).abs() < 1., "{}", spectrum.db[target]);
    }
}
