//! Deterministic, allocation-free 0D gas exchange primitives (SI units).
//!
//! `GasVolume` is a calorically perfect gas test bed. The state-based orifice
//! API also accepts an instantaneous gamma from the variable-cv cylinder model.
//! Neither API models pipe-wave propagation or claims measured valve calibration.

use std::f64::consts::{PI, TAU};

pub const MIN_TEMPERATURE_K: f64 = 200.0;
pub const MAX_TEMPERATURE_K: f64 = 3500.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GasProperties {
    pub gas_constant_j_kg_k: f64,
    pub gamma: f64,
}

impl Default for GasProperties {
    fn default() -> Self {
        Self {
            gas_constant_j_kg_k: 287.0,
            gamma: 1.4,
        }
    }
}

impl GasProperties {
    fn valid(self) -> bool {
        (1.0..=10_000.0).contains(&self.gas_constant_j_kg_k) && (1.01..=2.0).contains(&self.gamma)
    }

    pub fn cv_j_kg_k(self) -> f64 {
        self.gas_constant_j_kg_k / (self.gamma - 1.0)
    }

    pub fn cp_j_kg_k(self) -> f64 {
        self.gamma * self.cv_j_kg_k()
    }
}

/// A fixed, finite volume; creation rejects invalid values rather than hiding
/// added or removed mass/energy with state clamps.
#[derive(Clone, Copy, Debug)]
pub struct GasVolume {
    volume_m3: f64,
    mass_kg: f64,
    internal_energy_j: f64,
    properties: GasProperties,
}

impl GasVolume {
    pub fn new(
        volume_m3: f64,
        pressure_pa: f64,
        temperature_k: f64,
        properties: GasProperties,
    ) -> Option<Self> {
        if !properties.valid()
            || !(1e-9..=1000.0).contains(&volume_m3)
            || !(1.0..=1e9).contains(&pressure_pa)
            || !(MIN_TEMPERATURE_K..=MAX_TEMPERATURE_K).contains(&temperature_k)
        {
            return None;
        }
        let mass_kg = pressure_pa * volume_m3 / (properties.gas_constant_j_kg_k * temperature_k);
        Some(Self {
            volume_m3,
            mass_kg,
            internal_energy_j: mass_kg * properties.cv_j_kg_k() * temperature_k,
            properties,
        })
    }

    pub fn mass_kg(&self) -> f64 {
        self.mass_kg
    }
    pub fn internal_energy_j(&self) -> f64 {
        self.internal_energy_j
    }
    pub fn volume_m3(&self) -> f64 {
        self.volume_m3
    }
    pub fn temperature_k(&self) -> f64 {
        self.internal_energy_j / (self.mass_kg * self.properties.cv_j_kg_k())
    }
    pub fn pressure_pa(&self) -> f64 {
        self.internal_energy_j * (self.properties.gamma - 1.0) / self.volume_m3
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Orifice {
    pub area_m2: f64,
    pub discharge_coefficient: f64,
}

impl Orifice {
    /// Positive flow travels A -> B. The high-pressure side supplies temperature
    /// and gas properties, including during reverse flow. Inputs outside finite
    /// engineering bounds close the orifice. Zero downstream pressure is valid.
    pub fn mass_flow_from_states(
        self,
        pressure_a_pa: f64,
        temperature_a_k: f64,
        pressure_b_pa: f64,
        temperature_b_k: f64,
        properties: GasProperties,
    ) -> f64 {
        if self.area_m2 == 0.0 || self.discharge_coefficient == 0.0 {
            return 0.0;
        }
        if !properties.valid()
            || !(0.0..=100.0).contains(&self.area_m2)
            || !(0.0..=1.0).contains(&self.discharge_coefficient)
            || !(0.0..=1e9).contains(&pressure_a_pa)
            || !(0.0..=1e9).contains(&pressure_b_pa)
            || !(MIN_TEMPERATURE_K..=MAX_TEMPERATURE_K).contains(&temperature_a_k)
            || !(MIN_TEMPERATURE_K..=MAX_TEMPERATURE_K).contains(&temperature_b_k)
            // Roundoff in p = U(γ-1)/V must not create flux between equal states.
            || (pressure_a_pa - pressure_b_pa).abs()
                <= 8.0 * f64::EPSILON * pressure_a_pa.max(pressure_b_pa)
        {
            return 0.0;
        }
        let (upstream, downstream, temperature, sign) = if pressure_a_pa > pressure_b_pa {
            (pressure_a_pa, pressure_b_pa, temperature_a_k, 1.0)
        } else {
            (pressure_b_pa, pressure_a_pa, temperature_b_k, -1.0)
        };
        let g = properties.gamma;
        let ratio = downstream / upstream;
        // For the valid 1.01 <= gamma <= 2 the critical ratio lies in
        // 0.444..0.607: decide clear cases without its powf (same branch).
        let choked = if ratio > 0.61 {
            false
        } else if ratio < 0.44 {
            true
        } else {
            ratio <= (2.0 / (g + 1.0)).powf(g / (g - 1.0))
        };
        let factor = if choked {
            g.sqrt() * (2.0 / (g + 1.0)).powf((g + 1.0) / (2.0 * (g - 1.0)))
        } else {
            // expm1 avoids cancellation as downstream pressure approaches upstream.
            let log_ratio = ratio.ln();
            (2.0 * g / (g - 1.0)
                * (2.0 / g * log_ratio).exp()
                * -((g - 1.0) / g * log_ratio).exp_m1())
            .sqrt()
        };
        sign * self.discharge_coefficient * self.area_m2 * upstream
            / (properties.gas_constant_j_kg_k * temperature).sqrt()
            * factor
    }

    pub fn mass_flow_kg_s(self, a: &GasVolume, b: &GasVolume) -> f64 {
        let properties = if a.pressure_pa() >= b.pressure_pa() {
            a.properties
        } else {
            b.properties
        };
        self.mass_flow_from_states(
            a.pressure_pa(),
            a.temperature_k(),
            b.pressure_pa(),
            b.temperature_k(),
            properties,
        )
    }
}

/// Actual signed transfer A -> B, including transported upstream enthalpy.
#[derive(Clone, Copy, Debug, Default)]
pub struct Transfer {
    pub mass_kg: f64,
    pub energy_j: f64,
}

/// Conservative explicit exchange between equal-species, constant-cv volumes.
/// Limits the requested flux before applying it: at most 90% donor mass, no
/// crossing of pressure equilibrium, and both final temperatures in 200–3500 K.
/// This keeps large steps bounded without silently creating energy by clamping.
/// It is a stability guard, not an accuracy substitute for small time steps.
pub fn transfer(a: &mut GasVolume, b: &mut GasVolume, orifice: Orifice, dt_s: f64) -> Transfer {
    if !dt_s.is_finite() || dt_s <= 0.0 || a.properties != b.properties {
        return Transfer::default();
    }
    let rate = orifice.mass_flow_kg_s(a, b);
    if rate == 0.0 {
        return Transfer::default();
    }
    let (donor, receiver, sign) = if rate > 0.0 {
        (a, b, 1.0)
    } else {
        (b, a, -1.0)
    };
    let td = donor.temperature_k();
    let tr = receiver.temperature_k();
    let g = donor.properties.gamma;
    let h = donor.properties.cp_j_kg_k() * td;
    let equilibrium = (donor.pressure_pa() - receiver.pressure_pa())
        / (h * (g - 1.0) * (1.0 / donor.volume_m3 + 1.0 / receiver.volume_m3));
    let thermal_donor =
        donor.mass_kg * (td - MIN_TEMPERATURE_K).max(0.0) / (g * td - MIN_TEMPERATURE_K);
    let thermal_receiver = if g * td > MAX_TEMPERATURE_K {
        receiver.mass_kg * (MAX_TEMPERATURE_K - tr).max(0.0) / (g * td - MAX_TEMPERATURE_K)
    } else {
        f64::INFINITY
    };
    let mass = (rate.abs() * dt_s)
        .min(0.9 * donor.mass_kg)
        .min(equilibrium.max(0.0))
        .min(thermal_donor)
        .min(thermal_receiver);
    let energy = mass * h;
    donor.mass_kg -= mass;
    receiver.mass_kg += mass;
    donor.internal_energy_j -= energy;
    receiver.internal_energy_j += energy;
    Transfer {
        mass_kg: sign * mass,
        energy_j: sign * energy,
    }
}

/// All angles are crank radians; a four-stroke cam repeats every 4π radians.
#[derive(Clone, Copy, Debug)]
pub struct HarmonicCam {
    pub center_rad: f64,
    pub duration_rad: f64,
    pub peak_lift_m: f64,
    pub shape_exponent: f64,
    pub lash_m: f64,
}

impl HarmonicCam {
    pub fn lift_m(self, crank_angle_rad: f64) -> f64 {
        if !crank_angle_rad.is_finite()
            || !self.center_rad.is_finite()
            || !(0.0..=2.0 * TAU).contains(&self.duration_rad)
            || self.duration_rad == 0.0
            || !(0.0..=0.1).contains(&self.peak_lift_m)
            || !(0.1..=20.0).contains(&self.shape_exponent)
            || !(0.0..=0.1).contains(&self.lash_m)
        {
            return 0.0;
        }
        // Cheap conservative reject of the closed part of the cycle, before
        // three fmod calls: this unwrapped estimate is within a few ulp of
        // |angle| of the exact wrap below, far inside `margin`, and the window
        // keeps clear of the ±2π wrap, so it only returns where that would.
        let margin = 1e-9 * (1.0 + crank_angle_rad.abs() + self.center_rad.abs());
        let half = self.duration_rad * 0.5;
        if half < TAU - 2.0 * margin {
            let offset = crank_angle_rad - self.center_rad;
            let mut estimate = offset - (offset * (0.5 / TAU)) as i64 as f64 * (2.0 * TAU);
            if estimate > TAU {
                estimate -= 2.0 * TAU;
            } else if estimate < -TAU {
                estimate += 2.0 * TAU;
            }
            if estimate.abs() >= half + margin {
                return 0.0;
            }
        }
        let delta = (crank_angle_rad.rem_euclid(2.0 * TAU) - self.center_rad.rem_euclid(2.0 * TAU)
            + TAU)
            .rem_euclid(2.0 * TAU)
            - TAU;
        if delta.abs() >= self.duration_rad * 0.5 {
            return 0.0;
        }
        let harmonic = 0.5 + 0.5 * (TAU * delta / self.duration_rad).cos();
        let profile = if self.shape_exponent == 1.0 {
            harmonic
        } else {
            harmonic.powf(self.shape_exponent)
        };
        (self.peak_lift_m * profile - self.lash_m).max(0.0)
    }
}

/// Borrowed, validated lift/diameter vs Cd table, linearly interpolated.
#[derive(Clone, Copy, Debug)]
pub struct DischargeCurve<'a> {
    points: &'a [(f64, f64)],
}

impl<'a> DischargeCurve<'a> {
    pub fn new(points: &'a [(f64, f64)]) -> Option<Self> {
        if points.is_empty()
            || points
                .iter()
                .any(|&(x, y)| !(0.0..=10.0).contains(&x) || !(0.0..=1.0).contains(&y))
            || points.windows(2).any(|p| p[0].0 >= p[1].0)
        {
            return None;
        }
        Some(Self { points })
    }

    pub fn coefficient(self, lift_over_diameter: f64) -> f64 {
        if !lift_over_diameter.is_finite() || lift_over_diameter < 0.0 {
            return 0.0;
        }
        if lift_over_diameter <= self.points[0].0 {
            return self.points[0].1;
        }
        for pair in self.points.windows(2) {
            let (x0, y0) = pair[0];
            let (x1, y1) = pair[1];
            if lift_over_diameter <= x1 {
                return y0 + (y1 - y0) * (lift_over_diameter - x0) / (x1 - x0);
            }
        }
        self.points[self.points.len() - 1].1
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Valve {
    pub diameter_m: f64,
    pub port_area_m2: f64,
    pub count: u8,
}

impl Valve {
    /// Seat-normal area approximation πDL per valve, capped by the port area.
    /// Port area is per valve; count scales both curtain and port capacity.
    pub fn orifice(self, lift_m: f64, curve: DischargeCurve<'_>) -> Orifice {
        if !(1e-4..=0.5).contains(&self.diameter_m)
            || !(0.0..=1.0).contains(&self.port_area_m2)
            || !(0.0..=0.1).contains(&lift_m)
            || self.count > 8
        {
            return Orifice {
                area_m2: 0.0,
                discharge_coefficient: 0.0,
            };
        }
        Orifice {
            area_m2: (PI * self.diameter_m * lift_m).min(self.port_area_m2) * f64::from(self.count),
            discharge_coefficient: curve.coefficient(lift_m / self.diameter_m),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const PORT: Orifice = Orifice {
        area_m2: 0.001,
        discharge_coefficient: 0.7,
    };
    fn volume(p: f64, t: f64) -> GasVolume {
        GasVolume::new(0.001, p, t, GasProperties::default()).unwrap()
    }
    fn close(a: f64, b: f64) {
        assert!(
            (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1e-15),
            "{a} != {b}"
        );
    }

    #[test]
    fn equal_pressure_has_no_flow_even_with_different_temperature() {
        close(
            PORT.mass_flow_kg_s(&volume(1e5, 300.0), &volume(1e5, 1500.0)),
            0.0,
        );
    }

    #[test]
    fn reversal_uses_upstream_temperature_and_exactly_reverses_sign() {
        let a = volume(2e5, 900.0);
        let b = volume(1.8e5, 300.0);
        let flow = PORT.mass_flow_kg_s(&a, &b);
        assert!(flow > 0.0);
        close(flow, -PORT.mass_flow_kg_s(&b, &a));
        close(flow, PORT.mass_flow_kg_s(&a, &volume(1.8e5, 2500.0)));
    }

    #[test]
    fn choking_matches_analytic_air_flux_and_subsonic_branch_is_continuous() {
        let props = GasProperties::default();
        let critical = (2.0 / (props.gamma + 1.0)).powf(props.gamma / (props.gamma - 1.0));
        let flow = |ratio| PORT.mass_flow_from_states(2e5, 300.0, 2e5 * ratio, 300.0, props);
        close(flow(0.1), flow(0.0));
        close(flow(critical), flow(0.1));
        let reference =
            0.7 * 0.001 * 2e5 * (1.4_f64 / (287.0 * 300.0)).sqrt() * (2.0_f64 / 2.4).powf(3.0);
        close(flow(0.1), reference);
        assert!((flow(critical + 1e-7) - flow(critical)).abs() < 1e-10);
        assert!(flow(0.99) < flow(0.8) && flow(0.8) < flow(0.1));
    }

    #[test]
    fn transfer_conserves_mass_energy_and_transports_enthalpy() {
        let mut a = volume(3e5, 900.0);
        let mut b = volume(1e5, 300.0);
        let mass = a.mass_kg() + b.mass_kg();
        let energy = a.internal_energy_j() + b.internal_energy_j();
        let h = a.properties.cp_j_kg_k() * a.temperature_k();
        let step = transfer(&mut a, &mut b, PORT, 1.0 / 96000.0);
        assert!(step.mass_kg > 0.0);
        close(step.energy_j, step.mass_kg * h);
        close(a.mass_kg() + b.mass_kg(), mass);
        close(a.internal_energy_j() + b.internal_energy_j(), energy);
        assert!(a.temperature_k() < 900.0 && b.temperature_k() > 300.0);
    }

    #[test]
    fn oversized_steps_remain_conservative_bounded_and_do_not_cross_equilibrium() {
        for pd in [1e5, 1e7, 1e9] {
            for td in [200.0, 300.0, 3500.0] {
                for tr in [200.0, 3500.0] {
                    let mut a = volume(pd, td);
                    let mut b = volume(1.0, tr);
                    let mass = a.mass_kg() + b.mass_kg();
                    let energy = a.internal_energy_j() + b.internal_energy_j();
                    let original_mass = a.mass_kg();
                    for _ in 0..100 {
                        transfer(&mut a, &mut b, PORT, f64::MAX);
                        close(a.mass_kg() + b.mass_kg(), mass);
                        close(a.internal_energy_j() + b.internal_energy_j(), energy);
                        for v in [&a, &b] {
                            assert!(v.mass_kg() > 0.0);
                            assert!((199.999999..=3500.000001).contains(&v.temperature_k()));
                        }
                    }
                    assert!(a.mass_kg() >= original_mass * 0.1);
                    assert!(a.pressure_pa() >= b.pressure_pa() - pd * 1e-12);
                }
            }
        }
    }

    #[test]
    fn invalid_data_is_rejected_and_invalid_steps_do_nothing() {
        for bad in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(GasVolume::new(bad, 1e5, 300.0, GasProperties::default()).is_none());
            close(
                PORT.mass_flow_from_states(bad, 300.0, 1e5, 300.0, GasProperties::default()),
                0.0,
            );
            let mut a = volume(2e5, 300.0);
            let mut b = volume(1e5, 300.0);
            close(transfer(&mut a, &mut b, PORT, bad).mass_kg, 0.0);
        }
    }

    #[test]
    fn cam_is_periodic_with_closed_seat_lash_and_peak() {
        let cam = HarmonicCam {
            center_rad: 0.1,
            duration_rad: 4.0,
            peak_lift_m: 0.01,
            shape_exponent: 1.5,
            lash_m: 0.0002,
        };
        close(cam.lift_m(0.1), 0.0098);
        close(cam.lift_m(2.1), 0.0);
        close(cam.lift_m(-1.0), cam.lift_m(-1.0 + 2.0 * TAU));
        close(cam.lift_m(0.1 + TAU), 0.0);
        close(cam.lift_m(1.0), cam.lift_m(-0.8));
        assert!(cam.lift_m(2.05) == 0.0);
    }

    #[test]
    fn fast_rejects_match_the_exact_arithmetic() {
        // Pre-X-014 arithmetic, kept as the bit-exact reference.
        let wrap = |cam: HarmonicCam, angle: f64| {
            let delta = (angle.rem_euclid(2.0 * TAU) - cam.center_rad.rem_euclid(2.0 * TAU) + TAU)
                .rem_euclid(2.0 * TAU)
                - TAU;
            if delta.abs() >= cam.duration_rad * 0.5 {
                return 0.0;
            }
            let harmonic = 0.5 + 0.5 * (TAU * delta / cam.duration_rad).cos();
            (cam.peak_lift_m * harmonic.powf(cam.shape_exponent) - cam.lash_m).max(0.0)
        };
        for (center, duration) in [(-1.9, 4.4), (4.3, 4.4), (-40.0, 0.3), (1.0, 12.4)] {
            let cam = HarmonicCam {
                center_rad: center,
                duration_rad: duration,
                peak_lift_m: 0.01,
                shape_exponent: 1.5,
                lash_m: 0.0002,
            };
            for base in [0.0, 1e3, -1e3, 1e6, 3e8] {
                for i in 0..20_000 {
                    let edge = center + 0.5 * duration + (i % 7) as f64 * 1e-12;
                    for angle in [base + i as f64 * 7e-3, base + edge, base - edge] {
                        assert_eq!(cam.lift_m(angle).to_bits(), wrap(cam, angle).to_bits());
                    }
                }
            }
        }
        for g in (0..=990).map(|i| 1.01 + i as f64 * 1e-3) {
            let critical = (2.0 / (g + 1.0)).powf(g / (g - 1.0));
            assert!((0.44..=0.61).contains(&critical), "{g}: {critical}");
        }
    }

    #[test]
    fn valve_curtain_port_cap_and_cd_interpolate() {
        let curve = DischargeCurve::new(&[(0.0, 0.4), (0.1, 0.6), (0.3, 0.8)]).unwrap();
        close(curve.coefficient(0.05), 0.5);
        close(curve.coefficient(1.0), 0.8);
        let valve = Valve {
            diameter_m: 0.04,
            port_area_m2: 0.001,
            count: 2,
        };
        close(valve.orifice(0.0, curve).area_m2, 0.0);
        close(valve.orifice(0.001, curve).area_m2, 2.0 * PI * 0.04 * 0.001);
        close(valve.orifice(0.1, curve).area_m2, 0.002);
        assert!(DischargeCurve::new(&[(0.1, 0.5), (0.1, 0.8)]).is_none());
        assert!(DischargeCurve::new(&[]).is_none());
    }
}
