//! Piston slap and opt-in knock observation. Never feeds the gas or crank
//! solver, except the knock-control spark retard (`Knock::retard`).
//! Inline `[_; 12]` state only: no heap on the audio path.
use crate::engine_build::BlockMaterial;
use std::f64::consts::{PI, TAU};

fn xorshift(seed: &mut u64) -> f64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed >> 11) as f64 / (1_u64 << 52) as f64 - 1.
}

/// Piston side thrust reverses at TDC/BDC and where rod load changes sign;
/// each reversal knocks the skirt across its clearance into the bore.
pub(crate) struct Slap {
    previous: [f64; 12],
    pending: [f64; 12],
    delay: [u32; 12],
    radius_m: f64,
    ratio: f64,
    area_m2: f64,
    mass_kg: f64,
    clearance: f64,
    delay_steps: f64,
    impact: f64,
    envelope: f32,
    decay: f32,
    noise_scale: f32,
    seed: u64,
}
impl Slap {
    /// `rate` is the output rate; `step_rate` the physics substep rate.
    pub fn new(
        rate: f64,
        step_rate: f64,
        bore_m: f64,
        stroke_m: f64,
        rod_m: f64,
        block: BlockMaterial,
        seed: u64,
    ) -> Self {
        let material = match block {
            BlockMaterial::Aluminium => 1.15,
            BlockMaterial::CastIron => 1.,
        };
        Self {
            previous: [f64::NAN; 12],
            pending: [0.; 12],
            delay: [0; 12],
            radius_m: stroke_m * 0.5,
            ratio: stroke_m * 0.5 / rod_m,
            area_m2: PI * bore_m * bore_m / 4.,
            // Same moving-mass estimate as the crank's reciprocating torque.
            mass_kg: 0.45 * (bore_m / 0.086).powi(2),
            // Clearance proxy: bigger bores and aluminium run looser skirts.
            clearance: material * (bore_m / 0.086).sqrt(),
            // Onset timing scatter up to 0.15 ms.
            delay_steps: 0.00015 * step_rate,
            impact: 0.,
            envelope: 0.,
            decay: (-1. / (rate * 0.003)).exp() as f32,
            noise_scale: (rate / 48000.).sqrt() as f32,
            seed: (seed ^ 0x51a9_c0de_7f3e_2b41) | 1,
        }
    }
    /// One cylinder, one physics substep. `phase` is crank angle from its TDC.
    pub fn step(&mut self, i: usize, phase: f64, pressure_pa: f64, rpm: f64, dt: f64) {
        let (sine, cosine) = phase.sin_cos();
        let omega = rpm * TAU / 60.;
        let lateral = self.ratio * sine;
        let tangent = lateral / (1. - lateral * lateral).sqrt();
        let acceleration =
            self.radius_m * omega * omega * (cosine + self.ratio * (cosine * cosine - sine * sine));
        let side = ((pressure_pa - 101325.) * self.area_m2 - self.mass_kg * acceleration) * tangent;
        let turned = omega * dt;
        if self.delay[i] > 0 {
            self.delay[i] -= 1;
            if self.delay[i] == 0 {
                self.impact += std::mem::take(&mut self.pending[i]);
            }
        }
        if side * self.previous[i] < 0. && turned > 0. {
            // Side-load rate across the reversal, N/rad: gas load dominates at
            // firing TDC, so the hit grows with load. Same sqrt(rpm) law as the
            // valve contacts. ±15 % amplitude and small timing scatter.
            // Fixed gain: −7 dB vs contacts at 3000 rpm / 0.7, −12.6 dB at idle
            // (`slap_sits_below_valve_contacts_and_drops_at_idle`).
            let rate = (side - self.previous[i]).abs() / turned;
            let amplitude = 7.5e-7
                * rate
                * self.clearance
                * (rpm / 3000.).sqrt()
                * (1. + 0.15 * xorshift(&mut self.seed));
            if self.delay[i] > 0 {
                self.impact += std::mem::take(&mut self.pending[i]);
            }
            let delay = ((xorshift(&mut self.seed) * 0.5 + 0.5) * self.delay_steps) as u32;
            if delay == 0 {
                self.impact += amplitude;
            } else {
                self.pending[i] = amplitude;
                self.delay[i] = delay;
            }
        }
        self.previous[i] = side;
    }
    /// Block excitation for one output sample: coherent onset plus a few ms of
    /// decaying textured noise, like the valve contacts but longer and duller.
    pub fn next(&mut self) -> f32 {
        let impact = std::mem::take(&mut self.impact) as f32;
        self.envelope = self.envelope * self.decay + impact;
        let noise = xorshift(&mut self.seed) as f32 * self.noise_scale;
        impact * 0.1 + noise * self.envelope * 0.12
    }
}

/// Douaud–Eyzat pre-exponential multiplier, 1 = published (CFR engine, PRF).
/// Calibration point: set from a measured knock-limited spark map. With 1,
/// the default build at WOT and ON 95 (knock → 0) needs ≈ 11° retard at
/// 3000 rpm, 6° at 4000, 2.4° at 5000, none at 6000, and exceeds the 12°
/// authority at ≤ 2000 rpm: likely pessimistic at low speed (X-025).
const DELAY_CALIBRATION: f64 = 1.;

/// Livengood–Wu end-gas autoignition with the Douaud–Eyzat delay, then an
/// observation-only first circumferential chamber mode. Opt-in (intensity > 0).
/// A knock-sensor ECU loop retards each knocking cylinder's spark (`retard`),
/// the only path by which knock reaches the gas solver.
pub(crate) struct Knock {
    /// Per-cylinder knock-control retard, rad, positive = later spark.
    pub retard: [f64; 12],
    /// Retard added per detected event, rad (tests set 0 to disable control).
    pub retard_step: f64,
    retard_limit: f64,
    /// Knock-free advance recovery, rad/s.
    recovery: f64,
    /// Detected events since construction, all cylinders.
    pub events: u64,
    integral: [f64; 12],
    /// Unburned-charge reference at compression BDC (K, Pa); NaN outside.
    reference: [(f64, f64); 12],
    knocked: [bool; 12],
    amplitude: [f64; 12],
    phase: [f64; 12],
    omega: [f64; 12],
    bore_m: f64,
    decay: f64,
    omega_limit: f64,
    octane: f64,
    intensity: f64,
}
impl Knock {
    pub fn new(rate: f64, step_rate: f64, bore_m: f64, intensity: f64) -> Self {
        Self {
            // Typical knock-sensor ECU loop: 2° per event, 12° authority,
            // 1°/s recovery (sawtooth: one light event per 2 s at the limit).
            retard: [0.; 12],
            retard_step: 2_f64.to_radians(),
            retard_limit: 12_f64.to_radians(),
            recovery: 1_f64.to_radians(),
            events: 0,
            integral: [0.; 12],
            reference: [(f64::NAN, f64::NAN); 12],
            knocked: [false; 12],
            amplitude: [0.; 12],
            phase: [0.; 12],
            omega: [0.; 12],
            bore_m,
            // Ringing decays with a 1.5 ms time constant.
            decay: (-1. / (step_rate * 0.0015)).exp(),
            // Modes past 0.4 fs of the output are not rendered (no aliasing).
            omega_limit: TAU * 0.4 * rate,
            // No fuel octane in the build: RON 95, lowered so the user can
            // provoke knock at high load / advanced spark.
            octane: 95. - 25. * intensity,
            intensity,
        }
    }
    /// One cylinder, one substep. `phase` is crank angle from its firing TDC;
    /// `burn` the Wiebe burned fraction while a burn is active;
    /// `temperature_k` the single-zone gas (all unburned charge before spark).
    pub fn step(
        &mut self,
        i: usize,
        phase: f64,
        burn: Option<f64>,
        pressure_pa: f64,
        temperature_k: f64,
        dt: f64,
    ) {
        self.amplitude[i] *= self.decay;
        self.retard[i] = (self.retard[i] - self.recovery * dt).max(0.);
        // Speed of sound falls as the burned gas expands: the ring drifts down.
        self.omega[i] =
            TAU * 1.84 * (super::thermo::gamma(temperature_k) * 287. * temperature_k).sqrt()
                / (PI * self.bore_m);
        self.phase[i] = (self.phase[i] + self.omega[i] * dt) % TAU;
        // Livengood–Wu integrates from intake closing. Here from compression
        // BDC; the BDC–IVC share is < 1e-3 (charge < 450 K, < 2 bar).
        let compressing = (phase + PI).rem_euclid(2. * TAU) < PI;
        if burn.is_none() && !compressing {
            self.integral[i] = 0.;
            self.reference[i] = (f64::NAN, f64::NAN);
            self.knocked[i] = false;
            return;
        }
        // Unburned end gas: isentropic from the BDC charge (residuals and
        // intake heating included), γ 1.32 of a gasoline–air mixture. Not the
        // single-zone T: its air cv compresses at γ ≈ 1.39 (+55 K at spark).
        // A spark after TDC (no BDC state this cycle) references the spark.
        // No end-gas wall loss (slightly hot).
        if self.reference[i].0.is_nan() {
            self.reference[i] = (temperature_k, pressure_pa);
        }
        let (t0, p0) = self.reference[i];
        let end_gas = t0 * (pressure_pa / p0).max(1e-3).powf(0.32 / 1.32);
        let burned = burn.unwrap_or(0.);
        if self.knocked[i] || burned >= 0.98 {
            return;
        }
        let delay_s = DELAY_CALIBRATION
            * 17.68e-3
            * (self.octane / 100.).powf(3.402)
            * (pressure_pa / 101325.).max(1e-3).powf(-1.7)
            * (3800. / end_gas).exp();
        self.integral[i] += dt / delay_s;
        // Autoignition before spark would be preignition: not modelled.
        if self.integral[i] >= 1. && burn.is_some() {
            self.knocked[i] = true;
            self.events += 1;
            self.retard[i] = (self.retard[i] + self.retard_step).min(self.retard_limit);
            if self.omega[i] < self.omega_limit {
                // Ring amplitude ∝ unburned fraction; 5 % of p at full intensity
                // (≈ +11 dB over the mechanical layer at 3000 rpm / 0.7).
                self.amplitude[i] = 0.05 * self.intensity * (1. - burned) * pressure_pa;
                self.phase[i] = 0.;
            }
        }
    }
    /// Summed chamber ring, Pa, for the combustion excitation only.
    pub fn pressure(&self) -> f64 {
        (0..12)
            .map(|i| self.amplitude[i] * self.phase[i].sin())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slap_is_silent_without_rotation_and_finite() {
        let mut s = Slap::new(
            48000.,
            96000.,
            0.086,
            0.086,
            0.15,
            BlockMaterial::Aluminium,
            3,
        );
        for k in 0..960 {
            s.step(0, 0.3, 5e6, 0., 1. / 96000.);
            if k % 2 == 1 {
                assert_eq!(s.next(), 0.);
            }
        }
        let mut k = Knock::new(48000., 96000., 0.086, 1.);
        k.step(0, 0.1, Some(0.2), 4e6, 2500., 1. / 96000.);
        assert!(k.pressure().is_finite());
    }
    #[test]
    fn knock_control_retards_per_event_clamps_and_recovers() {
        let dt = 1. / 96000.;
        let mut k = Knock::new(48000., 96000., 0.086, 1.);
        // One burn per "cycle" in a hot, 60 bar end gas: τ ≈ 0.2 ms, so each
        // burn knocks once; then out of the compression window it resets.
        let cycle = |k: &mut Knock| {
            for _ in 0..200 {
                k.step(0, 0.1, Some(0.2), 6e6, 1000., dt);
            }
            k.step(0, 3., None, 1e5, 1000., dt);
        };
        cycle(&mut k);
        assert_eq!(k.events, 1);
        let step = 2_f64.to_radians();
        assert!((k.retard[0] - step).abs() < 1e-3 * step, "{}", k.retard[0]);
        assert_eq!(k.retard[1], 0.);
        for _ in 0..10 {
            cycle(&mut k);
        }
        assert_eq!(k.events, 11);
        assert!(k.retard[0] <= 12_f64.to_radians() && k.retard[0] > 11.9_f64.to_radians());
        // Knock-free: 1°/s back towards the base spark, never past it.
        let before = k.retard[0];
        for _ in 0..96000 {
            k.step(0, 3., None, 1e5, 400., dt);
        }
        let recovered = (before - k.retard[0]).to_degrees();
        assert!((recovered - 1.).abs() < 1e-6, "{recovered}°");
        for _ in 0..96000 * 12 {
            k.step(0, 3., None, 1e5, 400., dt);
        }
        assert_eq!(k.retard[0], 0.);
        assert_eq!(k.events, 11);
    }
}
