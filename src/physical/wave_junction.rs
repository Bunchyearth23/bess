//! Passive valve/waveguide junction (X-017 prototype, opt-in; X-028).
//!
//! The valve is a nonlinear orifice between the cylinder and a pipe port of
//! characteristic impedance Z = c/A (Pa per kg/s). Waves w are stored as
//! linear-equivalent pressures (Pa): w = G7·p_r·(X − 1), with X the Benson /
//! Blair pressure-amplitude ratio (Blair, *Design and Simulation of
//! Four-Stroke Engines*, ch. 2), G7 = 2γ/(γ−1), G5 = 2/(γ−1), p_r the mean
//! pipe pressure. With the wave w⁻ arriving at the valve, the superposition
//! X = X⁺ + X⁻ − 1 has the wave sum σ = w⁺ + w⁻ and gives the port pressure
//!
//!   p = p̄ + P(σ),  P(σ) = p_r·(X^G7 − 1),  X = 1 + σ/(G7·p_r),
//!
//! and the particle velocity u = G5·c·(X⁺ − X⁻), i.e. w⁺ − w⁻ = Z·ρ_r·A·u.
//! The pipe carries the high-passed flow (the 0D volumes hold the mean): the
//! valve outflow m enters it as m/X^G5 (Blair's mass flux ρ_r·X^G5·A·u; the
//! filter memory h and any backflow enter linearly), so
//!
//!   w⁺ − w⁻ = Z·(g + h),  g = m·X^(−G5) for m > 0, g = m otherwise.
//!
//! For small σ both reduce to linear acoustics (p_r = 0 selects it exactly).
//! At tens of kPa the linear sum doubled every rarefaction at the valve (WOT
//! port pressures reached the 2 kPa floor) and overstated blowdown
//! compressions by X^G5 ≈ 1.3–1.9 (X-028). The orifice gives the reservoir →
//! cylinder flow F(p), nondecreasing in p; the valve flow and the port are
//! solved together, never with the previous step's pressure.
//!
//! Passivity: for a fixed cylinder state, dw⁺ − dw⁻ = −c·(dw⁺ + dw⁻) with
//! c = Z·X^(−G5)·(F′·P′ + g·G5·X^(G5−1)/(G7·p_r)) ≥ 0 for outflow (g ≥ 0),
//! and c = Z·F′·P′ ≥ 0 otherwise, so the incremental reflection
//! r = (1 − c)/(1 + c) ∈ (−1, 1] and |Δw⁺| ≤ |Δw⁻| between any two arrivals:
//! the junction never returns more perturbation power than arrives. The
//! density factor must stay inside the joint solve: lagged by one sample (and
//! applied to h) it made the loop active, +13 to +17 dB at 6650 rpm, like the
//! rejected explicit coupling, r(z) = 1 − a·z⁻¹ with gain 1 + a at Nyquist.
//! Also tried and left out (X-028): a quadratic Borda–Carnot collector loss
//! (≤ 3 points of tuning gain, −1…−4 dB WOT level, worse 1/L law; the tail's
//! lumped damping already stands for junction losses). Not modelled:
//! amplitude-dependent propagation speed (steepening, shocks).

/// Lowest port pressure handed to the orifice, a safety floor (Pa). It is
/// monotone (keeps the contraction) but breaks the port relation where it
/// acts; with the ratio superposition it no longer acts at WOT (X-028).
const FLOOR_PA: f64 = 2000.;
/// Lower clamp of the pressure-amplitude ratio X (p ≈ 1e-8·p_r), monotone.
const MIN_RATIO: f64 = 0.1;
const MAX_ITERATIONS: usize = 60;
/// Bracket width relative to the flow: the flow error when bisecting. Mass
/// stays exact regardless; cylinder and pipe use the same returned flow.
const TOLERANCE: f64 = 1e-7;
/// Last Newton step relative to the flow. Convergence is quadratic, so the
/// returned (unevaluated) step leaves an error of order STEP² ≈ 1e-10.
const STEP_TOLERANCE: f64 = 1e-5;

/// Port state at wave sum σ.
struct State {
    pressure: f64,
    /// dp/dσ.
    pressure_slope: f64,
    /// Reservoir → cylinder flow the pipe side implies (−m), kg/s.
    inflow: f64,
    /// d(inflow)/dσ < 0.
    inflow_slope: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavePort {
    /// 0D mean pressure at the port, Pa.
    pub mean_pa: f64,
    /// Wave sum σ₀ at zero inflow: 2·arriving wave + filter memory, Pa.
    pub wave_pa: f64,
    /// Reference p_r of the pressure-amplitude ratio (Pa); 0 = linear sum.
    pub reference_pa: f64,
    /// γ of the pipe gas, for G7 = 2γ/(γ−1) (> 1).
    pub gamma: f64,
    /// Characteristic impedance for mass flow, Pa/(kg/s), ≥ 0.
    pub impedance: f64,
    /// Starting inflow estimate (kg/s), e.g. the previous substep's; any value.
    pub guess_inflow_kg_s: f64,
}

/// Open-valve invariants prepared once per cylinder substep, outside Newton.
/// Flow and analytic derivative share the same pressure-ratio exponentials.
/// The caller supplies already validated cylinder/reservoir states.
pub struct ValveFlow {
    cylinder_pa: f64,
    backflow: Donor,
    outflow: Donor,
    closed: bool,
}

struct Donor {
    gamma: f64,
    coefficient: f64,
    critical: std::cell::Cell<f64>,
    choked_factor: std::cell::Cell<f64>,
}

impl ValveFlow {
    pub fn new(
        orifice: crate::physical::gas::Orifice,
        reservoir_k: f64,
        cylinder_pa: f64,
        cylinder_k: f64,
        backflow: crate::physical::gas::GasProperties,
        outflow: crate::physical::gas::GasProperties,
    ) -> Self {
        let donor = |gas: crate::physical::gas::GasProperties, temperature: f64| {
            let g = gas.gamma;
            Donor {
                gamma: g,
                coefficient: orifice.discharge_coefficient * orifice.area_m2
                    / (gas.gas_constant_j_kg_k * temperature).sqrt(),
                critical: std::cell::Cell::new(f64::NAN),
                choked_factor: std::cell::Cell::new(f64::NAN),
            }
        };
        Self {
            cylinder_pa,
            backflow: donor(backflow, reservoir_k),
            outflow: donor(outflow, cylinder_k),
            closed: orifice.area_m2 == 0. || orifice.discharge_coefficient == 0.,
        }
    }

    pub fn evaluate(&self, p: f64) -> (f64, f64) {
        if self.closed {
            return (0., 0.);
        }
        let pc = self.cylinder_pa;
        if (p - pc).abs() <= 8. * f64::EPSILON * p.max(pc) {
            return (0., f64::INFINITY);
        }
        let (donor, upstream, downstream, sign) = if p > pc {
            (&self.backflow, p, pc, 1.)
        } else {
            (&self.outflow, pc, p, -1.)
        };
        let ratio = downstream / upstream;
        let g = donor.gamma;
        let choked = if ratio > 0.61 {
            false
        } else if ratio < 0.44 {
            true
        } else {
            let mut critical = donor.critical.get();
            if critical.is_nan() {
                critical = (2. / (g + 1.)).powf(g / (g - 1.));
                donor.critical.set(critical);
            }
            ratio <= critical
        };
        let (factor, phi_slope_ratio) = if choked {
            let mut factor = donor.choked_factor.get();
            if factor.is_nan() {
                factor = g.sqrt() * (2. / (g + 1.)).powf((g + 1.) / (2. * (g - 1.)));
                donor.choked_factor.set(factor);
            }
            (factor, 0.)
        } else {
            let log_ratio = ratio.ln();
            let minus_one = ((g - 1.) / g * log_ratio).exp_m1();
            let factor = (2. * g / (g - 1.) * (2. / g * log_ratio).exp() * -minus_one).sqrt();
            // r^(1-1/g) = 1+expm1(...) is already needed by the flow.
            let slope = (2. - (g + 1.) * (1. + minus_one)) / (-2. * g * ratio * minus_one);
            (factor, slope)
        };
        let flow = sign * donor.coefficient * upstream * factor;
        let slope = if p > pc {
            flow * (1. - ratio * phi_slope_ratio) / upstream
        } else {
            flow * phi_slope_ratio / upstream
        };
        (flow, slope)
    }
}

/// dF/dp of an orifice flow F (reservoir p → cylinder at `cylinder_pa`) from
/// F itself: F = ±K·p_u·φ(r) makes F′/F a function of the pressures and γ
/// only. Subsonic φ′/φ = (2/r − (γ+1)·r^(−1/γ)) / (2γ·(1 − r^((γ−1)/γ)));
/// choked φ′ = 0. `gamma` must be the upstream γ the orifice used.
pub fn orifice_slope(flow: f64, p: f64, cylinder_pa: f64, gamma: f64) -> f64 {
    if flow == 0. {
        // Closed, or exactly at equilibrium: the √Δp cusp, slope unbounded.
        return if p == cylinder_pa { f64::INFINITY } else { 0. };
    }
    let (r, upstream) = if p > cylinder_pa {
        (cylinder_pa / p, p)
    } else {
        (p / cylinder_pa, cylinder_pa)
    };
    // One power: t = r^(1/γ), r^((γ−1)/γ) = r/t; choked iff r/t ≤ 2/(γ+1).
    // Below 0.48 every γ ≤ 5/3 is choked (blowdown): no power needed.
    let ratio = if r < 0.48 {
        0.
    } else {
        let t = r.powf(1. / gamma);
        if r / t <= 2. / (gamma + 1.) {
            0.
        } else {
            (2. / r - (gamma + 1.) / t) / (2. * gamma * (1. - r / t))
        }
    };
    if p > cylinder_pa {
        // F = K·p·φ(p_c/p): dF/dp = F·(1 − r·φ′/φ)/p.
        flow * (1. - r * ratio) / upstream
    } else {
        // F = −K·p_c·φ(p/p_c): dF/dp = F·(φ′/φ)/p_c.
        flow * ratio / upstream
    }
}

impl WavePort {
    fn exponents(&self) -> (f64, f64) {
        (2. * self.gamma / (self.gamma - 1.), 2. / (self.gamma - 1.))
    }

    fn state(&self, sigma: f64) -> State {
        let z = self.impedance;
        let g = (sigma - self.wave_pa) / z;
        if self.reference_pa <= 0. {
            let p = self.mean_pa + sigma;
            let (pressure, pressure_slope) = if p > FLOOR_PA {
                (p, 1.)
            } else {
                (FLOOR_PA, 0.)
            };
            return State {
                pressure,
                pressure_slope,
                inflow: -g,
                inflow_slope: -1. / z,
            };
        }
        let (g7, g5) = self.exponents();
        let scale = g7 * self.reference_pa;
        let x = 1. + sigma / scale;
        let (x, clamped) = if x > MIN_RATIO {
            (x, false)
        } else {
            (MIN_RATIO, true)
        };
        let power = x.powf(g5); // X^G5; X^G7 = X^G5·X²
        let p = self.mean_pa + self.reference_pa * (power * x * x - 1.);
        let (pressure, pressure_slope) = if p > FLOOR_PA && !clamped {
            (p, power * x)
        } else {
            (p.max(FLOOR_PA), 0.)
        };
        let (m, dm) = if g > 0. {
            let dx = if clamped { 0. } else { 1. / scale };
            (g * power, power / z + g * g5 * power / x * dx)
        } else {
            (g, 1. / z)
        };
        State {
            pressure,
            pressure_slope,
            inflow: -m,
            inflow_slope: -dm,
        }
    }

    /// Wave sum σ = w⁺ + w⁻ at which the pipe side carries reservoir →
    /// cylinder inflow f (kg/s): the port relation solved for σ.
    pub fn wave_sum(&self, inflow_kg_s: f64) -> f64 {
        self.wave_sum_within(inflow_kg_s, MAX_ITERATIONS)
    }

    fn wave_sum_within(&self, inflow_kg_s: f64, iterations: usize) -> f64 {
        let z = self.impedance.max(0.);
        let linear = self.wave_pa - z * inflow_kg_s;
        if self.reference_pa <= 0. || inflow_kg_s >= 0. || z == 0. {
            return linear;
        }
        // Outflow m = −f: σ − Z·m·X(σ)^(−G5) = σ₀, increasing in σ, from the
        // linear estimate.
        let (g7, g5) = self.exponents();
        let (scale, m) = (g7 * self.reference_pa, -inflow_kg_s);
        let mut sigma = linear;
        for _ in 0..iterations {
            let x = (1. + sigma / scale).max(MIN_RATIO);
            let inverse = x.powf(-g5);
            let residual = sigma - z * m * inverse - self.wave_pa;
            let slope = 1. + z * m * g5 * inverse / x / scale;
            let step = residual / slope;
            sigma -= step;
            if step.abs() <= 1e-9 * scale {
                break;
            }
        }
        sigma
    }

    /// Port pressure (Pa) at reservoir→cylinder inflow f (kg/s).
    pub fn port_pressure(&self, inflow_kg_s: f64) -> f64 {
        if self.impedance <= 0. {
            return self.state_without_pipe().0;
        }
        self.state(self.wave_sum(inflow_kg_s)).pressure
    }

    /// Z = 0: the port pressure p̄ + P(σ₀) whatever the flow.
    fn state_without_pipe(&self) -> (f64, f64) {
        let port = WavePort {
            impedance: 1.,
            ..*self
        };
        let state = port.state(self.wave_pa);
        (state.pressure, state.pressure_slope)
    }

    /// Joint solve with `inflow(p)` = (F, dF/dp), the reservoir→cylinder kg/s
    /// through the valve at reservoir pressure p (nondecreasing in p), for a
    /// cylinder at `cylinder_pa`. Returns (inflow, p). The residual
    /// h(σ) = f_pipe(σ) − F(p(σ)) falls strictly in σ; the root lies between
    /// σ₀ (no pipe flow) and the σ where p = cylinder_pa (no valve flow), both
    /// known without evaluating F. Safeguarded Newton from the guess:
    /// ≈2–3 orifice evaluations per open-valve substep (the secant: ≈5).
    pub fn solve(self, cylinder_pa: f64, inflow: impl Fn(f64) -> (f64, f64)) -> (f64, f64) {
        if self.impedance <= 0. {
            let p = self.state_without_pipe().0;
            return (inflow(p).0, p);
        }
        // The root lies between σ₀ and the σ where p = cylinder_pa; that end
        // costs a power, so it is found only if a step needs the bracket.
        let equilibrium = || {
            if self.reference_pa > 0. {
                let (g7, _) = self.exponents();
                let ratio = ((cylinder_pa - self.mean_pa) / self.reference_pa + 1.).max(0.);
                g7 * self.reference_pa * (ratio.powf(1. / g7).max(MIN_RATIO) - 1.)
            } else {
                cylinder_pa - self.mean_pa
            }
        };
        let (mut low, mut high) = (f64::NEG_INFINITY, f64::INFINITY);
        let guess = self.guess_inflow_kg_s;
        // One Newton step of the port relation is start enough.
        let start = if guess.is_finite() {
            self.wave_sum_within(guess, 1)
        } else {
            self.wave_pa
        };
        let mut sigma = start;
        let mut state = self.state(sigma);
        let mut bounded = false;
        for _ in 0..MAX_ITERATIONS {
            let (flow, slope) = inflow(state.pressure);
            let h = state.inflow - flow;
            if h == 0. {
                break;
            }
            if h > 0. {
                low = sigma;
            } else {
                high = sigma;
            }
            // Orifice coupling F′·P′ against the pipe's |df/dσ|. Weak: Newton
            // on h. Strong: on k = f|f| − F|F|, the same sign and root;
            // subsonic F ≈ ±C√|Δp| makes F|F| nearly linear in p, and Newton
            // on h would crawl along the square-root cusp at p = p_cylinder.
            let orifice = slope * state.pressure_slope;
            let pipe = state.inflow_slope; // < 0
            let f = state.inflow;
            let step = if orifice <= -pipe {
                -h / (pipe - orifice)
            } else {
                -(f * f.abs() - flow * flow.abs())
                    / (2. * f.abs() * pipe - 2. * flow.abs() * orifice)
            };
            let mut next = sigma + step;
            let scale = f.abs().max(flow.abs());
            if (step * pipe).abs() <= STEP_TOLERANCE * scale {
                // Converged; a last step of a few ulps may touch the bracket.
                sigma = next.clamp(low, high);
                state = self.state(sigma);
                break;
            }
            // Also catches a NaN step and a zero step at the cusp (F′ = ∞).
            if !(next > low && next < high) {
                if !bounded {
                    let end = equilibrium();
                    low = low.max(self.wave_pa.min(end));
                    high = high.min(self.wave_pa.max(end));
                    bounded = true;
                }
                next = 0.5 * (low + high);
            }
            let done = ((high - low) * pipe).abs() <= TOLERANCE * scale;
            sigma = next;
            state = self.state(sigma);
            if done {
                break;
            }
        }
        (state.inflow, state.pressure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::gas::{GasProperties, Orifice};

    const GAS: GasProperties = GasProperties {
        gas_constant_j_kg_k: 287.,
        gamma: 1.33,
    };

    /// Exhaust valve, 30 mm × 8 mm lift class, cylinder at `cylinder_pa`/1100 K
    /// against a reservoir at p/700 K; positive = reservoir → cylinder.
    fn valve(cylinder_pa: f64) -> impl Fn(f64) -> f64 {
        let orifice = Orifice {
            area_m2: 6e-4,
            discharge_coefficient: 0.7,
        };
        move |p| orifice.mass_flow_from_states(p, 700., cylinder_pa, 1100., GAS)
    }

    /// `valve` with its analytic slope, as handed to the solver.
    fn sloped(cylinder_pa: f64) -> impl Fn(f64) -> (f64, f64) {
        move |p| {
            let f = valve(cylinder_pa)(p);
            (f, orifice_slope(f, p, cylinder_pa, GAS.gamma))
        }
    }

    /// Outgoing wave w⁺ = w⁻ − Z·f for an arriving wave, cylinder held fixed.
    fn outgoing(cylinder_pa: f64, mean_pa: f64, reference_pa: f64, z: f64, incoming: f64) -> f64 {
        let port = WavePort {
            mean_pa,
            wave_pa: 2. * incoming,
            reference_pa,
            gamma: 1.33,
            impedance: z,
            guess_inflow_kg_s: 0.,
        };
        port.wave_sum(port.solve(cylinder_pa, sloped(cylinder_pa)).0) - incoming
    }

    #[test]
    fn prepared_valve_shares_exponents_without_changing_flow_or_slope() {
        let orifice = Orifice {
            area_m2: 6e-4,
            discharge_coefficient: 0.7,
        };
        for gamma in [1.1, 1.33, 1.4, 1.67] {
            let properties = GasProperties { gamma, ..GAS };
            for cylinder in [30e3, 105e3, 600e3] {
                let prepared =
                    ValveFlow::new(orifice, 700., cylinder, 1100., properties, properties);
                for n in 0..400 {
                    let p = 2e3 + f64::from(n) * 2000.;
                    let expected =
                        orifice.mass_flow_from_states(p, 700., cylinder, 1100., properties);
                    let (flow, slope) = prepared.evaluate(p);
                    assert!((flow - expected).abs() < 1e-13 * expected.abs().max(1e-8));
                    let derivative = orifice_slope(expected, p, cylinder, gamma);
                    if derivative.is_infinite() {
                        assert_eq!(slope, derivative);
                    } else {
                        assert!((slope - derivative).abs() < 1e-10 * derivative.abs().max(1e-8));
                    }
                }
            }
        }
    }

    #[test]
    fn orifice_slope_matches_finite_differences() {
        for cylinder in [600e3, 250e3, 130e3, 106e3, 80e3] {
            for k in 0..200 {
                let p = 30e3 + 700e3 * f64::from(k) / 199.;
                if (p - cylinder).abs() < 2e3 {
                    continue; // √Δp cusp
                }
                let f = valve(cylinder)(p);
                let analytic = orifice_slope(f, p, cylinder, GAS.gamma);
                let h = 1e-3 * (p - cylinder).abs().min(p);
                let numeric = (valve(cylinder)(p + h) - valve(cylinder)(p - h)) / (2. * h);
                // Choked flow has a kink at the critical ratio; skip its neighbourhood.
                let scale = numeric.abs().max(analytic.abs()).max(1e-12);
                let critical = (2. / 2.33_f64).powf(1.33 / 0.33);
                let r = (p / cylinder).min(cylinder / p);
                if (r - critical).abs() > 0.01 {
                    assert!(
                        (analytic - numeric).abs() <= 1e-4 * scale,
                        "{cylinder} {p}: {analytic} vs {numeric}"
                    );
                }
                assert!(analytic >= 0.);
            }
        }
        assert_eq!(orifice_slope(0., 1e5, 2e5, 1.33), 0.);
        assert_eq!(orifice_slope(0., 1e5, 1e5, 1.33), f64::INFINITY);
    }

    #[test]
    fn ratio_superposition_is_blair_and_linear_for_small_waves() {
        let port = |wave_pa| WavePort {
            mean_pa: 110e3,
            wave_pa,
            reference_pa: 105e3,
            gamma: 1.33,
            impedance: 5e5,
            guess_inflow_kg_s: 0.,
        };
        let g7: f64 = 2. * 1.33 / 0.33;
        for w in [-60e3_f64, -20e3, 20e3, 60e3] {
            // Closed valve, σ = w: X = 1 + σ/(G7·p_r), p = p̄ + p_r·(X^G7 − 1).
            let blair = 110e3 + 105e3 * ((1. + w / (g7 * 105e3)).powf(g7) - 1.);
            assert!((port(w).port_pressure(0.) - blair).abs() < 1e-6 * blair);
            // Superposed rarefactions are shallower, compressions steeper.
            assert!(port(w).port_pressure(0.) - 110e3 > w);
        }
        for w in [-10_f64, 10.] {
            let p = port(w).port_pressure(0.) - 110e3;
            assert!((p - w).abs() < 1e-3 * w.abs(), "{p} vs {w}");
        }
        // Blair's mass flux: outflow m launches σ = w⁺ (no arrival) with
        // σ·X^G5 = Z·m, a weaker compression than the linear Z·m.
        let g5 = 2. / 0.33;
        for m in [1e-4, 0.03, 0.1, 0.3] {
            let sigma = port(0.).wave_sum(-m);
            let x = 1. + sigma / (g7 * 105e3);
            assert!((sigma / 5e5 * x.powf(g5) - m).abs() < 1e-9 * m, "{m}");
            assert!(sigma < 5e5 * m && sigma > 0.);
            // Backflow stays linear.
            assert_eq!(port(0.).wave_sum(m), -5e5 * m);
        }
    }

    #[test]
    fn joint_solution_satisfies_both_orifice_and_port() {
        // Blowdown (choked), mid-stroke, near-equal and reverse-flow states.
        let mut evaluations = (0, 0);
        for cylinder in [600e3, 250e3, 130e3, 106e3, 105e3, 101e3, 80e3] {
            for z in [0., 1e4, 5e5, 5e6] {
                for (mean, guess, reference) in [60e3, 105e3, 160e3]
                    .into_iter()
                    .flat_map(|p| [(p, 0.), (p, 0.05), (p, -0.3), (p, f64::NAN)])
                    .flat_map(|(p, g)| [(p, g, 0.), (p, g, 105e3)])
                {
                    let port = WavePort {
                        mean_pa: mean,
                        wave_pa: 0.,
                        reference_pa: reference,
                        gamma: 1.33,
                        impedance: z,
                        guess_inflow_kg_s: guess,
                    };
                    let count = std::cell::Cell::new(0);
                    let (f, p) = port.solve(cylinder, |p| {
                        count.set(count.get() + 1);
                        sloped(cylinder)(p)
                    });
                    evaluations = (evaluations.0 + count.get(), evaluations.1 + 1);
                    // The exact root lies within 2e-7·|F(p0)| of f: the
                    // residual changes sign across that interval. At an
                    // equilibrium root (F(p0) = 0) the ratio 1 + σ/(G7·p_r)
                    // rounds to 1 for |f| < 1e-17: allow 1e-15 kg/s there.
                    let p0 = port.port_pressure(0.);
                    let residual = |f: f64| f - valve(cylinder)(port.port_pressure(f));
                    let d = 2e-7 * valve(cylinder)(p0).abs() + 1e-15;
                    assert!(
                        residual(f - d) <= 0. && residual(f + d) >= 0.,
                        "{cylinder} {z} {mean} {guess} {reference}: f {f:e} p {p} r- {:e} r+ {:e} evals {}",
                        residual(f - d),
                        residual(f + d),
                        count.get()
                    );
                    assert!((p - port.port_pressure(f)).abs() <= 1e-9 * p);
                    // The port pressure lies between p0 and the cylinder.
                    assert!(p >= p0.min(cylinder) - 1e-6 && p <= p0.max(cylinder) + 1e-6);
                }
            }
        }
        println!(
            "{:.2} orifice evaluations per solve",
            f64::from(evaluations.0) / f64::from(evaluations.1)
        );
    }

    #[test]
    fn junction_never_returns_more_wave_power_than_arrives() {
        // Contraction: |Δp⁺| ≤ |Δp⁻| for any pair of arriving waves, which is
        // the discrete acoustic power balance of the port (Z-normalised).
        let mut seed = 0x2545f4914f6cdd1d_u64;
        let mut uniform = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1_u64 << 53) as f64
        };
        let mut worst: f64 = 0.;
        for i in 0..40000 {
            // Linear sum and pressure-amplitude-ratio superposition alike.
            let reference = if i % 2 == 0 { 0. } else { 105e3 };
            let cylinder = 90e3 + 500e3 * uniform().powi(3);
            let z = 10_f64.powf(4. + 3. * uniform());
            let (a, b) = (80e3 * (uniform() - 0.5), 80e3 * (uniform() - 0.5));
            let (pa, pb) = (
                outgoing(cylinder, 105e3, reference, z, a),
                outgoing(cylinder, 105e3, reference, z, b),
            );
            let gain = (pa - pb).abs() / (a - b).abs().max(1e-9);
            worst = worst.max(gain);
            assert!(
                gain <= 1. + 1e-6,
                "active junction: gain {gain} ({cylinder} {z} {reference} {a} {b} -> {pa} {pb})"
            );
        }
        println!("largest incremental wave gain {worst}");
    }

    #[test]
    fn small_signal_reflection_is_passive_and_explicit_delay_is_not() {
        for cylinder in [600e3, 130e3, 106e3, 104e3] {
            for z in [1e4, 1e5, 1e6] {
                let h = 1.;
                let r = (outgoing(cylinder, 105e3, 0., z, h)
                    - outgoing(cylinder, 105e3, 0., z, -h))
                    / (2. * h);
                assert!(
                    (-1. - 1e-6..=1. + 1e-6).contains(&r),
                    "{cylinder} {z}: r {r}"
                );
                // Explicit one-step-delayed coupling: p⁺ = p⁻ + Z·m(p̄ + 2p⁻[n−1]).
                // Linearise the orifice at the solved operating point.
                let (_, operating) = WavePort {
                    mean_pa: 105e3,
                    wave_pa: 0.,
                    reference_pa: 0.,
                    gamma: 1.33,
                    impedance: z,
                    guess_inflow_kg_s: 0.,
                }
                .solve(cylinder, sloped(cylinder));
                let slope =
                    (valve(cylinder)(operating + h) - valve(cylinder)(operating - h)) / (2. * h);
                let a = z * slope; // = −Z·dm/dp ≥ 0
                let (implicit, explicit_nyquist) = ((1. - a) / (1. + a), 1. + 2. * a);
                assert!((r - implicit).abs() < 1e-3 * (1. + a), "{r} vs {implicit}");
                if a > 1e-3 {
                    assert!(
                        explicit_nyquist > 1.,
                        "delayed coupling must be the active one"
                    );
                }
            }
        }
        // A choked or closed valve is a rigid, lossless end (r = 1).
        let r = outgoing(600e3, 105e3, 0., 1e5, 1.) - outgoing(600e3, 105e3, 0., 1e5, 0.);
        assert!((r - 1.).abs() < 1e-6, "choked r {r}");
        let closed = WavePort {
            mean_pa: 105e3,
            wave_pa: 0.,
            reference_pa: 0.,
            gamma: 1.33,
            impedance: 1e5,
            guess_inflow_kg_s: 0.,
        };
        assert_eq!(closed.solve(2e5, |_| (0., 0.)), (0., 105e3));
    }
    #[test]
    fn ported_cylinder_keeps_mass_energy_and_fuel_ledgers_closed() {
        use crate::{
            engine_build::EngineBuild,
            physical::{
                config::CylinderConfig,
                cycle::{CycleCylinder, CycleInput},
            },
        };
        use std::f64::consts::TAU;
        let config = CylinderConfig::from_build(&EngineBuild::default()).unwrap();
        let mut plain = CycleCylinder::new(config, 0., 123).unwrap();
        let mut ported = CycleCylinder::new(config, 0., 123).unwrap();
        let (mut injected, mut burned, mut escaped, mut changed, mut heat) = (0., 0., 0., 0., 0.);
        for i in 1..=48000 {
            let time = f64::from(i) / 96000.;
            let input = CycleInput {
                angle_rad: time * 1500. * TAU / 60.,
                ..Default::default()
            };
            // A strong 300 Hz standing wave on the exhaust port.
            ported.set_exhaust_port(Some(WavePort {
                mean_pa: input.exhaust.pressure_pa,
                wave_pa: 30e3 * (TAU * 300. * time).sin(),
                reference_pa: 0.,
                gamma: 1.33,
                impedance: 4e5,
                guess_inflow_kg_s: 0.,
            }));
            let out = ported.step(input).unwrap();
            let reference = plain.step(input).unwrap();
            changed += (out.exhaust_mass_kg - reference.exhaust_mass_kg).abs();
            heat += out.heat_j;
            injected += out.injected_fuel_kg;
            burned += out.fuel_burned_kg;
            escaped += out.unburned_exhaust_kg + out.unburned_intake_kg;
            assert!((injected - burned - escaped - out.fuel_mass_kg).abs() < 1e-12);
            assert!(out.ledger.residual_j().abs() < 1e-8);
            assert!(
                (out.ledger.enthalpy_in_j - out.ledger.enthalpy_out_j - out.intake_enthalpy_j
                    + out.exhaust_enthalpy_j
                    - out.injected_fuel_enthalpy_j)
                    .abs()
                    < 1e-8
            );
            assert!(
                (out.ledger.mass_in_kg - out.ledger.mass_out_kg - out.intake_mass_kg
                    + out.exhaust_mass_kg
                    - out.injected_fuel_kg)
                    .abs()
                    < 1e-14
            );
        }
        assert!(heat > 1000., "no combustion: {heat} J");
        assert!(
            changed > 1e-4,
            "the port did not change valve flow: {changed} kg"
        );
    }
}
