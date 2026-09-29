//! Passive valve/waveguide junction (X-017 prototype, opt-in).
//!
//! The valve is a nonlinear orifice between the cylinder and a pipe port of
//! characteristic impedance Z = c/A (Pa per kg/s of mass flow). With the wave
//! p⁻ arriving at the valve, the port pressure obeys p = p̄ + 2p⁻ + Z·q, q the
//! (high-passed) outflow. Everything but the current outflow m is known before
//! the step, so p = p₀ + Z·m. The orifice gives the reservoir→cylinder flow
//! F(p), nondecreasing in the reservoir pressure p, and m = −F(p). Both are
//! solved together, never with the previous step's pressure:
//!
//!   f = F(p₀ − Z·f),  f = −m.
//!
//! h(f) = f − F(p₀ − Z f) is strictly increasing, so the root is unique and
//! bracketed by 0 and F(p₀). Passivity: for two arriving waves with the same
//! cylinder state, Δp·Δm ≤ 0 (monotone orifice), and the outgoing wave
//! p⁺ = p − p⁻ satisfies (Δp⁺)² − (Δp⁻)² = Z·Δm·Δp ≤ 0. The junction is a
//! contraction on waves: it never returns more perturbation power than
//! arrives. Linearised, a = −Z·F′ ≥ 0 gives r = (1 − a)/(1 + a) ∈ (−1, 1].
//! The rejected explicit coupling used the previous step's wave,
//! r(z) = 1 − a·z⁻¹, whose gain 1 + a > 1 at Nyquist is active.

/// Lowest port pressure handed to the orifice, a safety floor (Pa). It is
/// monotone (keeps the contraction) but breaks p = p₀ + Z·m where it acts;
/// how often it acts at WOT is not yet instrumented.
const FLOOR_PA: f64 = 2000.;
const MAX_ITERATIONS: usize = 60;
/// Final bracket width or secant step, relative to |F(p₀)|: the flow error.
/// Mass stays exact regardless; cylinder and pipe use the same returned flow.
const TOLERANCE: f64 = 1e-7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavePort {
    /// Port pressure at zero outflow: 0D mean + 2·arriving wave + filter memory, Pa.
    pub pressure_at_zero_flow_pa: f64,
    /// Characteristic impedance for mass flow, Pa/(kg/s), ≥ 0.
    pub impedance: f64,
    /// Starting inflow estimate (kg/s), e.g. the previous sample's; any value.
    pub guess_inflow_kg_s: f64,
}

impl WavePort {
    /// Joint solve of `inflow(p)` (reservoir→cylinder kg/s at reservoir
    /// pressure p, nondecreasing in p) with the port. Returns (inflow, p).
    /// ponytail: ≈5 orifice evaluations per open-valve substep (+20–28 %
    /// engine CPU); an analytic dF/dp Newton step would cut that if needed.
    pub fn solve(self, inflow: impl Fn(f64) -> f64) -> (f64, f64) {
        let p0 = self.pressure_at_zero_flow_pa.max(FLOOR_PA);
        let z = self.impedance.max(0.);
        let pressure = |f: f64| (p0 - z * f).max(FLOOR_PA);
        let f0 = inflow(p0);
        if f0 == 0. || z == 0. {
            return (f0, p0);
        }
        // h(f) = f − F(p(f)) rises with slope ≥ 1 through [min(0,f0), max(0,f0)].
        // Iterate on k(f) = f|f| − F|F|(p(f)), same sign and root: subsonic
        // F ≈ ±C√|Δp| makes F|F| nearly linear in p, so a secant converges in
        // a few steps where it would crawl along the square-root cusp of h.
        let square = |f: f64| f * f.abs();
        let residual = |f: f64| square(f) - square(inflow(pressure(f)));
        let tolerance = TOLERANCE * f0.abs();
        let (mut low, mut high) = (f0.min(0.), f0.max(0.));
        // k(0) = −f0|f0| needs no evaluation.
        let (mut previous, mut k_previous) = (0., -square(f0));
        let guess = self.guess_inflow_kg_s;
        let mut x = if guess > low && guess < high {
            guess
        } else {
            f0
        };
        for _ in 0..MAX_ITERATIONS {
            let k = residual(x);
            if k == 0. {
                break;
            }
            if k > 0. {
                high = x;
            } else {
                low = x;
            }
            if high - low <= tolerance {
                break;
            }
            let secant = x - k * (x - previous) / (k - k_previous);
            (previous, k_previous) = (x, k);
            // A root on a bracket end (choked: F independent of p, root f0)
            // is reached by clamping; bisect only when the secant stalls.
            let middle = 0.5 * (low + high);
            x = if secant.is_nan() {
                middle
            } else {
                secant.clamp(low, high)
            };
            if x == previous {
                x = middle;
            }
            if (x - previous).abs() <= 0.25 * tolerance {
                break;
            }
        }
        (x, pressure(x))
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

    /// Outgoing wave for an arriving wave, cylinder state held fixed.
    fn outgoing(cylinder_pa: f64, mean_pa: f64, z: f64, incoming: f64) -> f64 {
        let port = WavePort {
            pressure_at_zero_flow_pa: mean_pa + 2. * incoming,
            impedance: z,
            guess_inflow_kg_s: 0.,
        };
        let (_, p) = port.solve(valve(cylinder_pa));
        p - mean_pa - incoming
    }

    #[test]
    fn joint_solution_satisfies_both_orifice_and_port() {
        // Blowdown (choked), mid-stroke, near-equal and reverse-flow states.
        for cylinder in [600e3, 250e3, 130e3, 106e3, 105e3, 101e3, 80e3] {
            for z in [0., 1e4, 5e5, 5e6] {
                for (p0, guess) in [60e3, 105e3, 160e3]
                    .into_iter()
                    .flat_map(|p| [(p, 0.), (p, 0.05), (p, -0.3), (p, f64::NAN)])
                {
                    let port = WavePort {
                        pressure_at_zero_flow_pa: p0,
                        impedance: z,
                        guess_inflow_kg_s: guess,
                    };
                    let (f, p) = port.solve(valve(cylinder));
                    // The exact root lies within 2e-7·|F(p0)| of f: the
                    // residual changes sign across that interval.
                    let residual = |f: f64| f - valve(cylinder)((p0 - z * f).max(FLOOR_PA));
                    let d = 2e-7 * valve(cylinder)(p0).abs() + 1e-300;
                    assert!(
                        residual(f - d) <= 0. && residual(f + d) >= 0.,
                        "{cylinder} {z} {p0} {guess}"
                    );
                    assert!((p - (p0 - z * f).max(FLOOR_PA)).abs() <= 1e-6 * p0);
                    // The port pressure lies between p0 and the cylinder.
                    assert!(p >= p0.min(cylinder) - 1e-6 && p <= p0.max(cylinder) + 1e-6);
                }
            }
        }
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
        for _ in 0..20000 {
            let cylinder = 90e3 + 500e3 * uniform().powi(3);
            let z = 10_f64.powf(4. + 3. * uniform());
            let (a, b) = (40e3 * (uniform() - 0.5), 40e3 * (uniform() - 0.5));
            let (pa, pb) = (
                outgoing(cylinder, 105e3, z, a),
                outgoing(cylinder, 105e3, z, b),
            );
            let gain = (pa - pb).abs() / (a - b).abs().max(1e-9);
            worst = worst.max(gain);
            assert!(gain <= 1. + 1e-6, "active junction: gain {gain}");
        }
        println!("largest incremental wave gain {worst}");
    }

    #[test]
    fn small_signal_reflection_is_passive_and_explicit_delay_is_not() {
        for cylinder in [600e3, 130e3, 106e3, 104e3] {
            for z in [1e4, 1e5, 1e6] {
                let h = 1.;
                let r =
                    (outgoing(cylinder, 105e3, z, h) - outgoing(cylinder, 105e3, z, -h)) / (2. * h);
                assert!(
                    (-1. - 1e-6..=1. + 1e-6).contains(&r),
                    "{cylinder} {z}: r {r}"
                );
                // Explicit one-step-delayed coupling: p⁺ = p⁻ + Z·m(p̄ + 2p⁻[n−1]).
                // Linearise the orifice at the solved operating point.
                let (_, operating) = WavePort {
                    pressure_at_zero_flow_pa: 105e3,
                    impedance: z,
                    guess_inflow_kg_s: 0.,
                }
                .solve(valve(cylinder));
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
        let r = outgoing(600e3, 105e3, 1e5, 1.) - outgoing(600e3, 105e3, 1e5, 0.);
        assert!((r - 1.).abs() < 1e-6, "choked r {r}");
        let closed = WavePort {
            pressure_at_zero_flow_pa: 105e3,
            impedance: 1e5,
            guess_inflow_kg_s: 0.,
        };
        assert_eq!(closed.solve(|_| 0.), (0., 105e3));
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
                pressure_at_zero_flow_pa: input.exhaust.pressure_pa
                    + 30e3 * (TAU * 300. * time).sin(),
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
