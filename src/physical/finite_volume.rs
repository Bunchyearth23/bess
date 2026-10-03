//! Optional finite-volume primary pipe: ideal-gas 1D Euler, MUSCL/SSP-RK2,
//! HLLC flux with HLLE fallback. No cell clipping or hidden energy injection.
//!
//! Validity: constant-area, inviscid, calorically perfect gas; no heat transfer,
//! wall friction, chemical species or moving-wall work. This is an acoustic
//! primary extension of the shared engine, not a replacement combustion model.
//! See docs/reports/BESS-QUALITY-CALIBRATION-2026-10-03.md for sources and limits.
use serde::Serialize;

type Conserved = [f64; 3];
const RHO_MIN: f64 = 1e-8;
const P_MIN: f64 = 1e-6;
const CFL: f64 = 0.4;
const MAX_SUBSTEPS: usize = 4096;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Primitive {
    pub density: f64,
    pub velocity: f64,
    pub pressure: f64,
}

impl Primitive {
    pub fn sound_speed(self, gamma: f64) -> f64 {
        (gamma * self.pressure / self.density).sqrt()
    }
    fn valid(self) -> bool {
        self.density.is_finite()
            && (RHO_MIN..=1e4).contains(&self.density)
            && self.velocity.is_finite()
            && self.velocity.abs() <= 1e5
            && self.pressure.is_finite()
            && (P_MIN..=1e12).contains(&self.pressure)
    }
    fn conserved(self, gamma: f64) -> Conserved {
        [
            self.density,
            self.density * self.velocity,
            self.pressure / (gamma - 1.) + 0.5 * self.density * self.velocity.powi(2),
        ]
    }
    fn from_conserved(u: Conserved, gamma: f64) -> Self {
        Self {
            density: u[0],
            velocity: u[1] / u[0],
            pressure: (gamma - 1.) * (u[2] - 0.5 * u[1] * u[1] / u[0]),
        }
    }
    fn array(self) -> [f64; 3] {
        [self.density, self.velocity, self.pressure]
    }
    fn from_array(q: [f64; 3]) -> Self {
        Self {
            density: q[0],
            velocity: q[1],
            pressure: q[2],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Boundary {
    Transmissive,
    Reflective,
    Reservoir(Primitive),
    /// Must be selected at both ends.
    Periodic,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Diagnostics {
    pub accepted_substeps: u64,
    pub rejected_trials: u64,
    pub first_order_steps: u64,
    pub hlle_faces: u64,
    pub max_cfl: f64,
    pub min_density: f64,
    pub min_pressure: f64,
    /// Integrated [mass, axial momentum, energy] entering through boundaries.
    pub boundary_exchange: [f64; 3],
    /// Explicit inventory change from retargeting geometry, not Euler flux.
    pub geometry_exchange: [f64; 3],
}

/// All working storage is allocated at construction; advance allocates nothing.
pub struct EulerPipe {
    state: Vec<Conserved>,
    stage: Vec<Conserved>,
    next: Vec<Conserved>,
    primitive: Vec<Primitive>,
    slopes: Vec<[f64; 3]>,
    fluxes: Vec<Conserved>,
    length: f64,
    area: f64,
    gamma: f64,
    diagnostics: Diagnostics,
}

fn physical_flux(q: Primitive, gamma: f64) -> Conserved {
    let u = q.conserved(gamma);
    [
        u[1],
        u[1] * q.velocity + q.pressure,
        (u[2] + q.pressure) * q.velocity,
    ]
}

fn minmod(a: f64, b: f64, c: f64) -> f64 {
    if a > 0. && b > 0. && c > 0. {
        a.min(b).min(c)
    } else if a < 0. && b < 0. && c < 0. {
        a.max(b).max(c)
    } else {
        0.
    }
}

fn hllc(left: Primitive, right: Primitive, gamma: f64, low_order: bool) -> (Conserved, bool, f64) {
    let (ul, ur) = (left.conserved(gamma), right.conserved(gamma));
    let (fl, fr) = (physical_flux(left, gamma), physical_flux(right, gamma));
    let (cl, cr) = (left.sound_speed(gamma), right.sound_speed(gamma));
    let (rl, rr) = (left.density.sqrt(), right.density.sqrt());
    let roe_u = (rl * left.velocity + rr * right.velocity) / (rl + rr);
    let roe_h = (rl * (ul[2] + left.pressure) / left.density
        + rr * (ur[2] + right.pressure) / right.density)
        / (rl + rr);
    let roe_c = ((gamma - 1.) * (roe_h - 0.5 * roe_u * roe_u))
        .max(0.)
        .sqrt();
    let sl = (left.velocity - cl)
        .min(right.velocity - cr)
        .min(roe_u - roe_c);
    let sr = (left.velocity + cl)
        .max(right.velocity + cr)
        .max(roe_u + roe_c);
    let speed = sl.abs().max(sr.abs());
    if sl >= 0. {
        return (fl, false, speed);
    }
    if sr <= 0. {
        return (fr, false, speed);
    }
    let hlle = || {
        std::array::from_fn(|k| (sr * fl[k] - sl * fr[k] + sl * sr * (ur[k] - ul[k])) / (sr - sl))
    };
    let denominator = left.density * (sl - left.velocity) - right.density * (sr - right.velocity);
    let sm = (right.pressure - left.pressure + left.density * left.velocity * (sl - left.velocity)
        - right.density * right.velocity * (sr - right.velocity))
        / denominator;
    if low_order || !sm.is_finite() || sm <= sl || sm >= sr {
        return (hlle(), true, speed);
    }
    let star = |q: Primitive, u: Conserved, s: f64| -> Conserved {
        let density = q.density * (s - q.velocity) / (s - sm);
        [
            density,
            density * sm,
            density
                * (u[2] / q.density
                    + (sm - q.velocity) * (sm + q.pressure / (q.density * (s - q.velocity)))),
        ]
    };
    let (ls, rs) = (star(left, ul, sl), star(right, ur, sr));
    if !Primitive::from_conserved(ls, gamma).valid()
        || !Primitive::from_conserved(rs, gamma).valid()
    {
        return (hlle(), true, speed);
    }
    let flux = if sm >= 0. {
        std::array::from_fn(|k| fl[k] + sl * (ls[k] - ul[k]))
    } else {
        std::array::from_fn(|k| fr[k] + sr * (rs[k] - ur[k]))
    };
    (flux, false, speed)
}

impl EulerPipe {
    pub fn new(
        length_m: f64,
        area_m2: f64,
        cells: usize,
        gamma: f64,
        initial: Primitive,
    ) -> Result<Self, &'static str> {
        if !(0.02..=10.).contains(&length_m)
            || !(1e-6..=1.).contains(&area_m2)
            || !(8..=2048).contains(&cells)
            || !(1.01..=1.67).contains(&gamma)
            || !initial.valid()
        {
            return Err("Invalid finite-volume pipe geometry, gas or initial state");
        }
        let u = initial.conserved(gamma);
        Ok(Self {
            state: vec![u; cells],
            stage: vec![u; cells],
            next: vec![u; cells],
            primitive: vec![initial; cells + 4],
            slopes: vec![[0.; 3]; cells + 4],
            fluxes: vec![[0.; 3]; cells + 1],
            length: length_m,
            area: area_m2,
            gamma,
            diagnostics: Diagnostics {
                min_density: initial.density,
                min_pressure: initial.pressure,
                ..Default::default()
            },
        })
    }
    pub fn cells(&self) -> usize {
        self.state.len()
    }
    pub fn dx(&self) -> f64 {
        self.length / self.cells() as f64
    }
    pub fn primitive(&self, cell: usize) -> Option<Primitive> {
        self.state
            .get(cell)
            .map(|&u| Primitive::from_conserved(u, self.gamma))
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.diagnostics
    }
    pub fn totals(&self) -> [f64; 3] {
        let scale = self.area * self.dx();
        std::array::from_fn(|k| self.state.iter().map(|u| u[k]).sum::<f64>() * scale)
    }
    pub fn initialize(
        &mut self,
        mut initial: impl FnMut(f64) -> Primitive,
    ) -> Result<(), &'static str> {
        let dx = self.dx();
        for (i, u) in self.next.iter_mut().enumerate() {
            let p = initial((i as f64 + 0.5) * dx);
            if !p.valid() {
                return Err("Invalid finite-volume initial profile");
            }
            *u = p.conserved(self.gamma);
        }
        std::mem::swap(&mut self.state, &mut self.next);
        self.diagnostics = Diagnostics {
            min_density: f64::INFINITY,
            min_pressure: f64::INFINITY,
            ..Default::default()
        };
        self.record_bounds();
        Ok(())
    }
    /// Sound-only geometry edit: preserve local primitive state, record the
    /// resulting inventory exchange. This is not a moving-wall work solver.
    pub fn retune(&mut self, length_m: f64) -> Result<(), &'static str> {
        if !(0.02..=10.).contains(&length_m) {
            return Err("Invalid finite-volume pipe length");
        }
        let old = self.totals();
        self.length = length_m;
        let new = self.totals();
        for (k, exchange) in self.diagnostics.geometry_exchange.iter_mut().enumerate() {
            *exchange += new[k] - old[k];
        }
        Ok(())
    }
    fn record_bounds(&mut self) {
        for &u in &self.state {
            let p = Primitive::from_conserved(u, self.gamma);
            self.diagnostics.min_density = self.diagnostics.min_density.min(p.density);
            self.diagnostics.min_pressure = self.diagnostics.min_pressure.min(p.pressure);
        }
    }
    fn fluxes(&mut self, staged: bool, boundaries: [Boundary; 2], first_order: bool) -> (u64, f64) {
        let state = if staged { &self.stage } else { &self.state };
        let n = state.len();
        for (q, &u) in self.primitive[2..n + 2].iter_mut().zip(state) {
            *q = Primitive::from_conserved(u, self.gamma);
        }
        for (side, boundary) in boundaries.into_iter().enumerate() {
            for ghost in 0..2 {
                let inside = if side == 0 { 2 + ghost } else { n + 1 - ghost };
                let out = if side == 0 { 1 - ghost } else { n + 2 + ghost };
                self.primitive[out] = match boundary {
                    Boundary::Periodic => {
                        self.primitive[if side == 0 { n + 1 - ghost } else { 2 + ghost }]
                    }
                    Boundary::Transmissive => self.primitive[if side == 0 { 2 } else { n + 1 }],
                    Boundary::Reservoir(q) => q,
                    Boundary::Reflective => Primitive {
                        velocity: -self.primitive[inside].velocity,
                        ..self.primitive[inside]
                    },
                };
            }
        }
        self.slopes.fill([0.; 3]);
        if !first_order {
            for i in 1..n + 3 {
                let (a, b, c) = (
                    self.primitive[i - 1].array(),
                    self.primitive[i].array(),
                    self.primitive[i + 1].array(),
                );
                let slope = std::array::from_fn(|k| {
                    minmod(
                        1.5 * (b[k] - a[k]),
                        0.5 * (c[k] - a[k]),
                        1.5 * (c[k] - b[k]),
                    )
                });
                let lo = Primitive::from_array(std::array::from_fn(|k| b[k] - 0.5 * slope[k]));
                let hi = Primitive::from_array(std::array::from_fn(|k| b[k] + 0.5 * slope[k]));
                if lo.valid() && hi.valid() {
                    self.slopes[i] = slope;
                }
            }
        }
        let mut fallback = 0;
        let mut speed: f64 = 0.;
        for face in 0..=n {
            let (li, ri) = (face + 1, face + 2);
            let (left, right) = (self.primitive[li].array(), self.primitive[ri].array());
            let left =
                Primitive::from_array(std::array::from_fn(|k| left[k] + 0.5 * self.slopes[li][k]));
            let right =
                Primitive::from_array(std::array::from_fn(|k| right[k] - 0.5 * self.slopes[ri][k]));
            let (flux, used_hlle, a) = hllc(left, right, self.gamma, first_order);
            self.fluxes[face] = flux;
            fallback += u64::from(used_hlle);
            speed = speed.max(a);
        }
        (fallback, speed)
    }
    fn trial(
        &mut self,
        dt: f64,
        boundaries: [Boundary; 2],
        first_order: bool,
    ) -> Option<([f64; 3], u64, f64)> {
        let ratio = dt / self.dx();
        let n = self.cells();
        let (mut fallback, a1) = self.fluxes(false, boundaries, first_order);
        let first_boundary: [f64; 3] =
            std::array::from_fn(|k| self.fluxes[0][k] - self.fluxes[n][k]);
        if ratio * a1 > 0.48 {
            return None;
        }
        for (i, stage) in self.stage.iter_mut().enumerate() {
            *stage = std::array::from_fn(|k| {
                self.state[i][k] - ratio * (self.fluxes[i + 1][k] - self.fluxes[i][k])
            });
            if !Primitive::from_conserved(*stage, self.gamma).valid() {
                return None;
            }
        }
        let (second_fallback, a2) = self.fluxes(true, boundaries, first_order);
        fallback += second_fallback;
        if ratio * a2 > 0.48 {
            return None;
        }
        for (i, next) in self.next.iter_mut().enumerate() {
            *next = std::array::from_fn(|k| {
                0.5 * (self.state[i][k] + self.stage[i][k]
                    - ratio * (self.fluxes[i + 1][k] - self.fluxes[i][k]))
            });
            if !Primitive::from_conserved(*next, self.gamma).valid() {
                return None;
            }
        }
        let exchange = std::array::from_fn(|k| {
            0.5 * dt * self.area * (first_boundary[k] + self.fluxes[0][k] - self.fluxes[n][k])
        });
        Some((exchange, fallback, ratio * a1.max(a2)))
    }
    pub fn advance(&mut self, seconds: f64, boundaries: [Boundary; 2]) -> Result<(), &'static str> {
        if !(0.0..=0.05).contains(&seconds) {
            return Err("Finite-volume step must be finite and within 0–50 ms");
        }
        if matches!(boundaries[0], Boundary::Periodic)
            != matches!(boundaries[1], Boundary::Periodic)
        {
            return Err("Periodic finite-volume boundaries must be paired");
        }
        for boundary in boundaries {
            if let Boundary::Reservoir(q) = boundary
                && !q.valid()
            {
                return Err("Invalid finite-volume reservoir");
            }
        }
        let mut remaining = seconds;
        let mut substeps = 0;
        while remaining > seconds * 1e-12 {
            substeps += 1;
            if substeps > MAX_SUBSTEPS {
                return Err("Finite-volume adaptive work budget exceeded");
            }
            let max_speed = self
                .state
                .iter()
                .map(|&u| {
                    let p = Primitive::from_conserved(u, self.gamma);
                    p.velocity.abs() + p.sound_speed(self.gamma)
                })
                .fold(0_f64, f64::max);
            let mut dt = remaining.min(CFL * self.dx() / max_speed);
            let mut accepted = None;
            for retry in 0..14 {
                if let Some(result) = self.trial(dt, boundaries, retry > 0) {
                    accepted = Some((result, retry > 0));
                    break;
                }
                self.diagnostics.rejected_trials += 1;
                dt *= 0.5;
            }
            let Some(((exchange, fallback, cfl), first_order)) = accepted else {
                return Err("Finite-volume positivity/CFL recovery exhausted");
            };
            std::mem::swap(&mut self.state, &mut self.next);
            remaining = (remaining - dt).max(0.);
            self.diagnostics.accepted_substeps += 1;
            self.diagnostics.first_order_steps += u64::from(first_order);
            self.diagnostics.hlle_faces += fallback;
            self.diagnostics.max_cfl = self.diagnostics.max_cfl.max(cfl);
            for (total, value) in self.diagnostics.boundary_exchange.iter_mut().zip(exchange) {
                *total += value;
            }
            self.record_bounds();
        }
        Ok(())
    }
}

/// Acoustic adapter: incoming pressure waves at either end, outgoing waves
/// for the existing collector/valve network. Incoming/outgoing values are
/// Riemann-invariant amplitudes in equivalent small-signal pascals; the adapter
/// reconstructs positive isentropic reservoir states, not a linear pressure sum.
pub struct Primary1d {
    pipe: EulerPipe,
    reference: Primitive,
    gamma: f64,
    rate: f64,
    failed: bool,
}

impl Primary1d {
    pub fn new(
        length_m: f64,
        area_m2: f64,
        rate: u32,
        mean_pressure_pa: f64,
        temperature_k: f64,
        gamma: f64,
        gas_constant: f64,
    ) -> Result<Self, &'static str> {
        if !(8000..=384000).contains(&rate)
            || !(200.0..=2500.).contains(&temperature_k)
            || !(150.0..=500.).contains(&gas_constant)
            || !(10000.0..=2e6).contains(&mean_pressure_pa)
        {
            return Err("Invalid quality-primary operating reference");
        }
        let reference = Primitive {
            density: mean_pressure_pa / (gas_constant * temperature_k),
            velocity: 0.,
            pressure: mean_pressure_pa,
        };
        // Refined from 6 mm after its measured -3.08 dB at 4 kHz / 0.70 m.
        // The quality_validation example measures the actual dispersion.
        let cells = (length_m / 0.004).ceil().clamp(32., 512.) as usize;
        Ok(Self {
            pipe: EulerPipe::new(length_m, area_m2, cells, gamma, reference)?,
            reference,
            gamma,
            rate: f64::from(rate),
            failed: false,
        })
    }
    /// Select the numerical grid during offline preparation only. The requested
    /// maximum spacing is 2–40 mm; 8–512 cells bound storage and per-step work.
    /// Refining a running pipe is rejected rather than resetting its waves.
    pub fn with_cell_size(mut self, max_cell_m: f64) -> Result<Self, &'static str> {
        if !(0.002..=0.04).contains(&max_cell_m)
            || self.failed
            || self.pipe.diagnostics.accepted_substeps != 0
        {
            return Err("Invalid quality-primary grid or pipe already running");
        }
        let cells = (self.pipe.length / max_cell_m).ceil().max(8.) as usize;
        if cells > 512 {
            return Err("Requested quality-primary spacing exceeds the 512-cell budget");
        }
        self.pipe = EulerPipe::new(
            self.pipe.length,
            self.pipe.area,
            cells,
            self.gamma,
            self.reference,
        )?;
        Ok(self)
    }
    pub fn cell_size_m(&self) -> f64 {
        self.pipe.dx()
    }
    /// [forward arrival at the collector, backward arrival at the valve].
    pub fn arrivals(&self) -> [f64; 2] {
        let c = self.reference.sound_speed(self.gamma);
        let z = self.reference.density * c;
        let left = self.pipe.primitive(0).expect("nonempty pipe");
        let right = self
            .pipe
            .primitive(self.pipe.cells() - 1)
            .expect("nonempty pipe");
        [
            0.5 * z
                * (2. * (right.sound_speed(self.gamma) - c) / (self.gamma - 1.) + right.velocity),
            0.5 * z * (2. * (left.sound_speed(self.gamma) - c) / (self.gamma - 1.) - left.velocity),
        ]
    }
    pub fn step(
        &mut self,
        left_outgoing_pa: f64,
        right_outgoing_pa: f64,
    ) -> Result<(), &'static str> {
        if self.failed {
            return Err("Quality primary has stopped after a numerical/domain failure");
        }
        let incoming = [left_outgoing_pa, right_outgoing_pa];
        if incoming
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 2. * self.reference.pressure)
        {
            self.failed = true;
            return Err(
                "Quality-primary characteristic amplitude exceeds twice reference pressure",
            );
        }
        let c = self.reference.sound_speed(self.gamma);
        let z = self.reference.density * c;
        let arrivals = self.arrivals();
        let boundaries = std::array::from_fn(|side| {
            let (plus, minus) = if side == 0 {
                (incoming[0], arrivals[1])
            } else {
                (arrivals[0], incoming[1])
            };
            let sound_ratio = 1. + (self.gamma - 1.) * (plus + minus) / (2. * z * c);
            Boundary::Reservoir(Primitive {
                pressure: self.reference.pressure
                    * sound_ratio.powf(2. * self.gamma / (self.gamma - 1.)),
                density: self.reference.density * sound_ratio.powf(2. / (self.gamma - 1.)),
                velocity: (plus - minus) / z,
            })
        });
        let result = self.pipe.advance(1. / self.rate, boundaries);
        self.failed |= result.is_err();
        result
    }
    pub fn retune(&mut self, length_m: f64) -> Result<(), &'static str> {
        self.pipe.retune(length_m)
    }
    pub fn failed(&self) -> bool {
        self.failed
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.pipe.diagnostics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quality_grid_is_bounded_and_only_prepared_before_running() {
        let make = || Primary1d::new(0.7, 0.001, 48000, 101325., 673., 1.33, 287.);
        let mut pipe = make().unwrap().with_cell_size(0.02).unwrap();
        assert!((pipe.cell_size_m() - 0.02).abs() < 1e-12);
        pipe.step(10., 0.).unwrap();
        assert!(pipe.with_cell_size(0.004).is_err());
        assert!(make().unwrap().with_cell_size(f64::NAN).is_err());
        assert!(make().unwrap().with_cell_size(0.001).is_err());
    }
    #[test]
    fn uniform_state_is_preserved_without_cell_clipping() {
        let q = Primitive {
            density: 1.2,
            velocity: 12.,
            pressure: 101325.,
        };
        let mut pipe = EulerPipe::new(1., 0.001, 64, 1.4, q).unwrap();
        pipe.advance(0.01, [Boundary::Periodic; 2]).unwrap();
        for i in 0..pipe.cells() {
            let actual = pipe.primitive(i).unwrap();
            assert!((actual.density - q.density).abs() < 1e-12);
            assert!((actual.pressure - q.pressure).abs() < 1e-8);
        }
        assert_eq!(pipe.diagnostics.rejected_trials, 0);
    }
    #[test]
    fn sod_shock_tube_stays_positive_and_balances_boundary_flux() {
        let left = Primitive {
            density: 1.,
            velocity: 0.,
            pressure: 1.,
        };
        let mut pipe = EulerPipe::new(1., 1., 256, 1.4, left).unwrap();
        pipe.initialize(|x| {
            if x < 0.5 {
                left
            } else {
                Primitive {
                    density: 0.125,
                    pressure: 0.1,
                    ..left
                }
            }
        })
        .unwrap();
        let before = pipe.totals();
        for _ in 0..4 {
            pipe.advance(0.05, [Boundary::Transmissive; 2]).unwrap();
        }
        let after = pipe.totals();
        for ((end, initial), exchange) in after
            .into_iter()
            .zip(before)
            .zip(pipe.diagnostics.boundary_exchange)
        {
            assert!((end - initial - exchange).abs() < 1e-10);
        }
        let star = pipe.primitive(145).unwrap();
        assert!((star.pressure - 0.30313).abs() < 0.03, "{star:?}");
        assert!((star.velocity - 0.92745).abs() < 0.04, "{star:?}");
        assert!(pipe.diagnostics.min_density > 0.);
        assert!(pipe.diagnostics.min_pressure > 0.);
        assert!(pipe.diagnostics.max_cfl <= 0.48);
    }
    #[test]
    fn periodic_density_wave_conserves_mass_momentum_and_energy() {
        let q = Primitive {
            density: 1.,
            velocity: 0.7,
            pressure: 1.,
        };
        let mut pipe = EulerPipe::new(1., 0.01, 128, 1.4, q).unwrap();
        pipe.initialize(|x| Primitive {
            density: 1. + 0.2 * (std::f64::consts::TAU * x).sin(),
            ..q
        })
        .unwrap();
        let before = pipe.totals();
        for _ in 0..10 {
            pipe.advance(0.02, [Boundary::Periodic; 2]).unwrap();
        }
        let after = pipe.totals();
        for (end, initial) in after.into_iter().zip(before) {
            assert!((end - initial).abs() < 1e-12);
        }
    }
    #[test]
    fn strong_expansion_remains_positive_with_bounded_adaptive_work() {
        let left = Primitive {
            density: 1.,
            velocity: -2.,
            pressure: 0.4,
        };
        let mut pipe = EulerPipe::new(1., 1., 128, 1.4, left).unwrap();
        pipe.initialize(|x| Primitive {
            velocity: if x < 0.5 { -2. } else { 2. },
            ..left
        })
        .unwrap();
        pipe.advance(0.05, [Boundary::Transmissive; 2]).unwrap();
        assert!(pipe.diagnostics.min_pressure > 0.);
        assert!(pipe.diagnostics.accepted_substeps < MAX_SUBSTEPS as u64);
        assert!(pipe.advance(f64::NAN, [Boundary::Transmissive; 2]).is_err());
    }
}
