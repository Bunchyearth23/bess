//! Optional finite-volume primary pipe: ideal-gas 1D Euler, MUSCL/SSP-RK2,
//! HLLC flux with HLLE fallback. No cell clipping or hidden energy injection.
//!
//! EulerPipe/Primary1d: constant-area, inviscid, calorically perfect gas;
//! no heat transfer, wall friction or moving-wall work. SpeciesPipe is a separate
//! offline extension with frozen transported species and optional variable EOS.
//! See docs/reports/BESS-QUALITY-CALIBRATION-2026-10-03.md for sources and limits.
use super::thermo::Mixture;
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
    /// Conservative prescribed [mass, momentum, total-energy] flux per unit
    /// area, positive along the pipe axis at either end. Used for a shared
    /// cylinder transfer or a junction, never as an independent source copy.
    Flux([f64; 3]),
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
    /// Same integrated transfer separated into left/right (positive inward).
    pub boundary_by_side: [[f64; 3]; 2],
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
    /// One conservative HLLC face flux for both owners of a junction.
    pub fn interface_flux(
        left: Primitive,
        right: Primitive,
        gamma: f64,
    ) -> Result<[f64; 3], &'static str> {
        if !left.valid() || !right.valid() || !(1.01..=2.).contains(&gamma) {
            return Err("Invalid finite-volume interface state");
        }
        Ok(hllc(left, right, gamma, false).0)
    }

    /// Global network steps must respect the fastest cell before computing
    /// frozen shared interface fluxes. Internal recovery may still subdivide.
    pub fn stable_time_step(&self) -> f64 {
        let speed = self
            .state
            .iter()
            .map(|&u| {
                let q = Primitive::from_conserved(u, self.gamma);
                q.velocity.abs() + q.sound_speed(self.gamma)
            })
            .fold(0_f64, f64::max);
        CFL * self.dx() / speed
    }
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
                    Boundary::Transmissive | Boundary::Flux(_) => {
                        self.primitive[if side == 0 { 2 } else { n + 1 }]
                    }
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
        for (side, boundary) in boundaries.into_iter().enumerate() {
            if let Boundary::Flux(flux) = boundary {
                self.fluxes[if side == 0 { 0 } else { n }] = flux;
            }
        }
        (fallback, speed)
    }
    fn trial(
        &mut self,
        dt: f64,
        boundaries: [Boundary; 2],
        first_order: bool,
    ) -> Option<([[f64; 3]; 2], u64, f64)> {
        let ratio = dt / self.dx();
        let n = self.cells();
        let (mut fallback, a1) = self.fluxes(false, boundaries, first_order);
        let first_boundary = [self.fluxes[0], self.fluxes[n]];
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
        let exchange = std::array::from_fn(|side| {
            std::array::from_fn(|k| {
                let index = if side == 0 { 0 } else { n };
                let sign = if side == 0 { 1. } else { -1. };
                sign * 0.5 * dt * self.area * (first_boundary[side][k] + self.fluxes[index][k])
            })
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
            if let Boundary::Flux(flux) = boundary
                && flux.iter().any(|v| !v.is_finite())
            {
                return Err("Invalid prescribed finite-volume flux");
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
            for (side, values) in exchange.iter().enumerate() {
                for (k, value) in values.iter().enumerate() {
                    self.diagnostics.boundary_exchange[k] += value;
                    self.diagnostics.boundary_by_side[side][k] += value;
                }
            }
            self.record_bounds();
        }
        Ok(())
    }
}

/// Offline EOS selection. Fixed is a reduction/transport reference; SharedMixture
/// uses exactly the frozen species and sensible-energy reference of cylinders.
#[derive(Clone, Copy, Debug)]
pub enum GasEquation {
    Fixed { gamma: f64, gas_constant: f64 },
    SharedMixture,
}

/// Euler state plus transported mass fractions; products are the remainder.
#[derive(Clone, Copy, Debug)]
pub struct SpeciesState {
    pub gas: Primitive,
    pub fresh_air_fraction: f64,
    pub fuel_fraction: f64,
}

pub type SpeciesConserved = [f64; 5];

impl GasEquation {
    fn valid(self) -> bool {
        match self {
            Self::Fixed {
                gamma,
                gas_constant,
            } => (1.01..=1.67).contains(&gamma) && (50.0..=600.).contains(&gas_constant),
            Self::SharedMixture => true,
        }
    }
    fn mixture(q: SpeciesState) -> Mixture {
        Mixture::from_fractions(q.fresh_air_fraction, q.fuel_fraction)
    }
    pub fn temperature(self, q: SpeciesState) -> f64 {
        let r = match self {
            Self::Fixed { gas_constant, .. } => gas_constant,
            Self::SharedMixture => Self::mixture(q).gas_constant(),
        };
        q.gas.pressure / (q.gas.density * r)
    }
    pub fn gamma(self, q: SpeciesState) -> f64 {
        match self {
            Self::Fixed { gamma, .. } => gamma,
            Self::SharedMixture => Self::mixture(q).gamma(self.temperature(q)),
        }
    }
    pub fn sound_speed(self, q: SpeciesState) -> f64 {
        q.gas.sound_speed(self.gamma(q))
    }
    pub fn at_pressure(
        self,
        pressure: f64,
        temperature: f64,
        fresh: f64,
        fuel: f64,
    ) -> Result<SpeciesState, &'static str> {
        let r = match self {
            Self::Fixed { gas_constant, .. } => gas_constant,
            Self::SharedMixture => Mixture::from_fractions(fresh, fuel).gas_constant(),
        };
        let q = SpeciesState {
            gas: Primitive {
                density: pressure / (r * temperature),
                velocity: 0.,
                pressure,
            },
            fresh_air_fraction: fresh,
            fuel_fraction: fuel,
        };
        self.conserved(q)?;
        Ok(q)
    }
    pub fn conserved(self, q: SpeciesState) -> Result<SpeciesConserved, &'static str> {
        // A few ulps in the fractions are allowed solely for roundoff after
        // conservative operations. The conserved species are never clipped.
        if !self.valid()
            || !q.gas.valid()
            || !q.fresh_air_fraction.is_finite()
            || !q.fuel_fraction.is_finite()
            || q.fresh_air_fraction < 0.
            || q.fuel_fraction < 0.
            || q.fresh_air_fraction + q.fuel_fraction > 1. + 8. * f64::EPSILON
            || !(200. * (1. - 8. * f64::EPSILON)..=3500. * (1. + 8. * f64::EPSILON))
                .contains(&self.temperature(q))
        {
            return Err("Invalid species/EOS state (no cell repair)");
        }
        let internal = match self {
            Self::Fixed { gamma, .. } => q.gas.pressure / (q.gas.density * (gamma - 1.)),
            Self::SharedMixture => Self::mixture(q).internal_energy(self.temperature(q)),
        };
        let rho = q.gas.density;
        Ok([
            rho,
            rho * q.gas.velocity,
            rho * (internal + 0.5 * q.gas.velocity.powi(2)),
            rho * q.fresh_air_fraction,
            rho * q.fuel_fraction,
        ])
    }
    pub fn state(self, u: SpeciesConserved) -> Result<SpeciesState, &'static str> {
        if u.iter().any(|v| !v.is_finite()) || u[0] < RHO_MIN {
            return Err("Invalid conservative species state");
        }
        let (rho, velocity) = (u[0], u[1] / u[0]);
        let fresh = u[3] / rho;
        let fuel = u[4] / rho;
        let internal = u[2] / rho - 0.5 * velocity.powi(2);
        let pressure = match self {
            Self::Fixed { gamma, .. } => (gamma - 1.) * rho * internal,
            Self::SharedMixture => {
                let m = Mixture::from_fractions(fresh, fuel);
                // Same rationalized quadratic inversion as thermo::Mixture;
                // use its public caloric functions, no separate species table.
                let a = m.cv(0.);
                let b = (m.cv(1000.) - a) / 1000.;
                let e = internal - m.internal_energy(0.);
                let temperature = 2. * e / (a + (a * a + 2. * b * e).sqrt());
                rho * m.gas_constant() * temperature
            }
        };
        let q = SpeciesState {
            gas: Primitive {
                density: rho,
                velocity,
                pressure,
            },
            fresh_air_fraction: fresh,
            fuel_fraction: fuel,
        };
        self.conserved(q)?;
        Ok(q)
    }
}

/// Boundary includes species in every imposed conservative flux.
#[derive(Clone, Copy, Debug)]
pub enum SpeciesBoundary {
    Transmissive,
    Reflective,
    Reservoir(SpeciesState),
    Flux(SpeciesConserved),
    Periodic,
}

/// First-order spatial HLLC/HLLE, SSP-RK2 in time. This separate offline solver
/// leaves the production calorically-perfect MUSCL path byte unchanged. Five
/// conserved unknowns per cell prevent instantaneous whole-pipe scalar mixing.
pub struct SpeciesPipe {
    state: Vec<SpeciesConserved>,
    stage: Vec<SpeciesConserved>,
    next: Vec<SpeciesConserved>,
    faces: Vec<SpeciesConserved>,
    length: f64,
    area: f64,
    eos: GasEquation,
    boundary_by_side: [SpeciesConserved; 2],
    rejected_trials: u64,
}

impl SpeciesPipe {
    pub fn new(
        length: f64,
        area: f64,
        cells: usize,
        eos: GasEquation,
        initial: SpeciesState,
    ) -> Result<Self, &'static str> {
        if !(0.02..=10.).contains(&length)
            || !(1e-6..=1.).contains(&area)
            || !(8..=2048).contains(&cells)
        {
            return Err("Invalid species pipe geometry");
        }
        let u = eos.conserved(initial)?;
        Ok(Self {
            state: vec![u; cells],
            stage: vec![u; cells],
            next: vec![u; cells],
            faces: vec![[0.; 5]; cells + 1],
            length,
            area,
            eos,
            boundary_by_side: [[0.; 5]; 2],
            rejected_trials: 0,
        })
    }
    pub fn cells(&self) -> usize {
        self.state.len()
    }
    pub fn dx(&self) -> f64 {
        self.length / self.cells() as f64
    }
    pub fn state(&self, cell: usize) -> Option<SpeciesState> {
        self.state.get(cell).and_then(|u| self.eos.state(*u).ok())
    }
    pub fn totals(&self) -> SpeciesConserved {
        let mut sum = [0.; 5];
        for u in &self.state {
            for k in 0..5 {
                sum[k] += u[k] * self.area * self.dx();
            }
        }
        sum
    }
    pub fn boundary_by_side(&self) -> [SpeciesConserved; 2] {
        self.boundary_by_side
    }
    pub fn rejected_trials(&self) -> u64 {
        self.rejected_trials
    }
    pub fn initialize(
        &mut self,
        mut initial: impl FnMut(f64) -> SpeciesState,
    ) -> Result<(), &'static str> {
        let dx = self.dx();
        for (i, u) in self.next.iter_mut().enumerate() {
            *u = self.eos.conserved(initial((i as f64 + 0.5) * dx))?;
        }
        std::mem::swap(&mut self.next, &mut self.state);
        self.boundary_by_side = [[0.; 5]; 2];
        Ok(())
    }
    pub fn stable_time_step(&self) -> f64 {
        let speed = self
            .state
            .iter()
            .map(|u| {
                let q = self.eos.state(*u).expect("accepted species state");
                q.gas.velocity.abs() + self.eos.sound_speed(q)
            })
            .fold(0., f64::max);
        CFL * self.dx() / speed
    }
    pub fn interface_flux(
        eos: GasEquation,
        left: SpeciesState,
        right: SpeciesState,
    ) -> Result<SpeciesConserved, &'static str> {
        Self::flux(eos, left, right, false).map(|(flux, _)| flux)
    }
    fn flux(
        eos: GasEquation,
        l: SpeciesState,
        r: SpeciesState,
        low_order: bool,
    ) -> Result<(SpeciesConserved, f64), &'static str> {
        let ul = eos.conserved(l)?;
        let ur = eos.conserved(r)?;
        let physical = |q: SpeciesState, u: SpeciesConserved| {
            [
                u[1],
                u[1] * q.gas.velocity + q.gas.pressure,
                (u[2] + q.gas.pressure) * q.gas.velocity,
                u[3] * q.gas.velocity,
                u[4] * q.gas.velocity,
            ]
        };
        let fl = physical(l, ul);
        let fr = physical(r, ur);
        let sl = (l.gas.velocity - eos.sound_speed(l)).min(r.gas.velocity - eos.sound_speed(r));
        let sr = (l.gas.velocity + eos.sound_speed(l)).max(r.gas.velocity + eos.sound_speed(r));
        let speed = sl.abs().max(sr.abs());
        if sl >= 0. {
            return Ok((fl, speed));
        }
        if sr <= 0. {
            return Ok((fr, speed));
        }
        let hlle = || {
            let mut f = [0.; 5];
            for k in 0..5 {
                f[k] = (sr * fl[k] - sl * fr[k] + sl * sr * (ur[k] - ul[k])) / (sr - sl);
            }
            f
        };
        if low_order {
            return Ok((hlle(), speed));
        }
        let dl = l.gas.density * (sl - l.gas.velocity);
        let dr = r.gas.density * (sr - r.gas.velocity);
        let sm = (r.gas.pressure - l.gas.pressure + dl * l.gas.velocity - dr * r.gas.velocity)
            / (dl - dr);
        let (q, u, f, s) = if sm >= 0. {
            (l, ul, fl, sl)
        } else {
            (r, ur, fr, sr)
        };
        let rho_star = q.gas.density * (s - q.gas.velocity) / (s - sm);
        let pressure_star =
            q.gas.pressure + q.gas.density * (s - q.gas.velocity) * (sm - q.gas.velocity);
        let energy_star = ((s - q.gas.velocity) * u[2] - q.gas.pressure * q.gas.velocity
            + pressure_star * sm)
            / (s - sm);
        let star = [
            rho_star,
            rho_star * sm,
            energy_star,
            rho_star * q.fresh_air_fraction,
            rho_star * q.fuel_fraction,
        ];
        if !sm.is_finite()
            || sm <= sl
            || sm >= sr
            || pressure_star <= 0.
            || eos.state(star).is_err()
        {
            return Ok((hlle(), speed));
        }
        let mut flux = f;
        for k in 0..5 {
            flux[k] += s * (star[k] - u[k]);
        }
        // A contact transports the donor fractions with exactly the computed
        // mass flux. Separate subtractions at vanishing flow can otherwise
        // create a negative trace scalar despite a positive hydro state.
        flux[3] = flux[0] * q.fresh_air_fraction;
        flux[4] = flux[0] * q.fuel_fraction;
        Ok((flux, speed))
    }
    fn fill_faces(
        &mut self,
        staged: bool,
        boundaries: [SpeciesBoundary; 2],
        low_order: bool,
    ) -> Result<f64, &'static str> {
        let u = if staged { &self.stage } else { &self.state };
        let ghost = |side: usize| -> Result<SpeciesState, &'static str> {
            let q = self.eos.state(u[if side == 0 { 0 } else { u.len() - 1 }])?;
            Ok(match boundaries[side] {
                SpeciesBoundary::Reservoir(q) => q,
                SpeciesBoundary::Periodic => {
                    self.eos.state(u[if side == 0 { u.len() - 1 } else { 0 }])?
                }
                SpeciesBoundary::Reflective => SpeciesState {
                    gas: Primitive {
                        velocity: -q.gas.velocity,
                        ..q.gas
                    },
                    ..q
                },
                _ => q,
            })
        };
        let mut maximum = 0_f64;
        for face in 0..=u.len() {
            let l = if face == 0 {
                ghost(0)?
            } else {
                self.eos.state(u[face - 1])?
            };
            let r = if face == u.len() {
                ghost(1)?
            } else {
                self.eos.state(u[face])?
            };
            let (f, speed) = Self::flux(self.eos, l, r, low_order)?;
            self.faces[face] = f;
            maximum = maximum.max(speed);
        }
        if let SpeciesBoundary::Flux(f) = boundaries[0] {
            self.faces[0] = f;
        }
        if let SpeciesBoundary::Flux(f) = boundaries[1] {
            self.faces[u.len()] = f;
        }
        Ok(maximum)
    }
    fn trial(
        &mut self,
        dt: f64,
        boundaries: [SpeciesBoundary; 2],
        low_order: bool,
    ) -> Result<[SpeciesConserved; 2], &'static str> {
        let ratio = dt / self.dx();
        if self.fill_faces(false, boundaries, low_order)? * ratio > 0.48 {
            return Err("Species CFL trial");
        }
        let first = [self.faces[0], self.faces[self.cells()]];
        for i in 0..self.cells() {
            for k in 0..5 {
                self.stage[i][k] =
                    self.state[i][k] - ratio * (self.faces[i + 1][k] - self.faces[i][k]);
            }
            self.eos.state(self.stage[i])?;
        }
        if self.fill_faces(true, boundaries, low_order)? * ratio > 0.48 {
            return Err("Species CFL stage");
        }
        for i in 0..self.cells() {
            for k in 0..5 {
                self.next[i][k] = 0.5
                    * (self.state[i][k] + self.stage[i][k]
                        - ratio * (self.faces[i + 1][k] - self.faces[i][k]));
            }
            self.eos.state(self.next[i])?;
        }
        let mut exchange = [[0.; 5]; 2];
        for side in 0..2 {
            for k in 0..5 {
                let last = self.faces[if side == 0 { 0 } else { self.cells() }][k];
                exchange[side][k] = (if side == 0 { 1. } else { -1. })
                    * self.area
                    * dt
                    * 0.5
                    * (first[side][k] + last);
            }
        }
        Ok(exchange)
    }
    pub fn advance(
        &mut self,
        seconds: f64,
        boundaries: [SpeciesBoundary; 2],
    ) -> Result<(), &'static str> {
        if !(0.0..=0.05).contains(&seconds)
            || matches!(boundaries[0], SpeciesBoundary::Periodic)
                != matches!(boundaries[1], SpeciesBoundary::Periodic)
        {
            return Err("Invalid species pipe interval/boundaries");
        }
        for b in boundaries {
            match b {
                SpeciesBoundary::Reservoir(q) => {
                    self.eos.conserved(q)?;
                }
                SpeciesBoundary::Flux(f) if f.iter().any(|v| !v.is_finite()) => {
                    return Err("Nonfinite species flux");
                }
                _ => (),
            }
        }
        let mut remaining = seconds;
        let mut substeps = 0;
        while remaining > seconds * 1e-12 {
            substeps += 1;
            if substeps > MAX_SUBSTEPS {
                return Err("Species substep budget exhausted");
            }
            let mut dt = remaining.min(self.stable_time_step());
            let mut accepted = None;
            for attempt in 0..14 {
                match self.trial(dt, boundaries, attempt > 0) {
                    Ok(exchange) => {
                        accepted = Some(exchange);
                        break;
                    }
                    Err(_) => {
                        self.rejected_trials += 1;
                        dt *= 0.5;
                    }
                }
            }
            let exchange = accepted.ok_or("Species positivity/EOS recovery exhausted")?;
            std::mem::swap(&mut self.state, &mut self.next);
            for (side, values) in exchange.iter().enumerate() {
                for (k, value) in values.iter().enumerate() {
                    self.boundary_by_side[side][k] += value;
                }
            }
            remaining -= dt;
        }
        Ok(())
    }
}

#[cfg(test)]
mod species_tests {
    use super::*;
    use crate::physical::thermo::GasState;

    #[test]
    fn shared_eos_matches_cylinder_energy_pressure_and_temperature() {
        let eos = GasEquation::SharedMixture;
        for (fresh, fuel) in [(1., 0.), (0., 0.), (0.2, 0.03), (0.1, 0.7)] {
            for temperature in [200., 298.15, 673., 1800., 3500.] {
                let q = eos.at_pressure(220000., temperature, fresh, fuel).unwrap();
                let u = eos.conserved(q).unwrap();
                let cylinder = GasState::new(
                    q.gas.density * 0.001,
                    temperature,
                    0.001,
                    Mixture::from_fractions(fresh, fuel),
                )
                .unwrap();
                assert!((u[2] * 0.001 - cylinder.internal_energy_j()).abs() < 1e-9);
                let back = eos.state(u).unwrap();
                assert!((eos.temperature(back) - cylinder.temperature_k()).abs() < 1e-8);
                assert!((back.gas.pressure - cylinder.pressure_pa()).abs() < 1e-6);
            }
        }
        assert!(eos.at_pressure(101325., 673., 0.8, 0.3).is_err());
        assert!(eos.at_pressure(101325., 199., 0.5, 0.).is_err());
    }

    #[test]
    fn mixing_species_at_common_reference_does_not_release_chemical_energy() {
        let eos = GasEquation::SharedMixture;
        let air = eos
            .conserved(eos.at_pressure(101325., 298.15, 1., 0.).unwrap())
            .unwrap();
        let fuel = eos
            .conserved(eos.at_pressure(101325., 298.15, 0., 1.).unwrap())
            .unwrap();
        let mixed = std::array::from_fn(|k| 0.3 * air[k] + 0.7 * fuel[k]);
        let q = eos.state(mixed).unwrap();
        assert!((eos.temperature(q) - 298.15).abs() < 1e-9);
        assert!((mixed[2] - (0.3 * air[2] + 0.7 * fuel[2])).abs() < 1e-9);
    }

    #[test]
    fn fixed_eos_reduces_to_legacy_euler_energy_and_uniform_flux() {
        let eos = GasEquation::Fixed {
            gamma: 1.4,
            gas_constant: 287.,
        };
        let mut q = eos.at_pressure(101325., 673., 0.2, 0.03).unwrap();
        q.gas.velocity = 125.;
        let u = eos.conserved(q).unwrap();
        let old = q.gas.conserved(1.4);
        for k in 0..3 {
            assert!((u[k] - old[k]).abs() < 1e-9);
        }
        let flux = SpeciesPipe::interface_flux(eos, q, q).unwrap();
        let legacy = EulerPipe::interface_flux(q.gas, q.gas, 1.4).unwrap();
        for k in 0..3 {
            assert!((flux[k] - legacy[k]).abs() < 1e-7);
        }
        assert!((flux[3] - flux[0] * 0.2).abs() < 1e-12);
        assert!((flux[4] - flux[0] * 0.03).abs() < 1e-12);
        let mut pipe = SpeciesPipe::new(1., 0.01, 64, eos, q).unwrap();
        let initial = pipe.totals();
        pipe.advance(0.002, [SpeciesBoundary::Periodic; 2]).unwrap();
        for (after, before) in pipe.totals().iter().zip(initial) {
            assert!((after - before).abs() < 1e-9);
        }
    }

    fn advected_species_error(cells: usize) -> f64 {
        let eos = GasEquation::Fixed {
            gamma: 1.4,
            gas_constant: 287.,
        };
        let mut base = eos.at_pressure(101325., 673., 0.2, 0.05).unwrap();
        base.gas.velocity = 100.;
        let mut pipe = SpeciesPipe::new(1., 0.01, cells, eos, base).unwrap();
        let profile = |x: f64| 0.2 + 0.1 * (std::f64::consts::TAU * x).sin();
        pipe.initialize(|x| SpeciesState {
            fresh_air_fraction: profile(x),
            ..base
        })
        .unwrap();
        let initial = pipe.totals();
        pipe.advance(0.003, [SpeciesBoundary::Periodic; 2]).unwrap();
        let mut error = 0.;
        for i in 0..cells {
            let q = pipe.state(i).unwrap();
            error += (q.fresh_air_fraction - profile((i as f64 + 0.5) * pipe.dx() - 0.3)).abs()
                / cells as f64;
            assert!((q.gas.pressure - 101325.).abs() < 1e-6);
        }
        for (after, before) in pipe.totals().iter().zip(initial) {
            assert!((after - before).abs() < 1e-8);
        }
        error
    }

    #[test]
    fn conservative_species_front_advects_and_converges_with_refinement() {
        let coarse = advected_species_error(64);
        let medium = advected_species_error(128);
        let fine = advected_species_error(256);
        eprintln!("species L1 errors 64/128/256: {coarse:.8e}/{medium:.8e}/{fine:.8e}");
        assert!(medium < coarse * 0.6 && fine < medium * 0.6);
    }

    #[test]
    fn species_ssp_rk2_converges_in_time_against_semidiscrete_advection() {
        let eos = GasEquation::Fixed {
            gamma: 1.4,
            gas_constant: 287.,
        };
        let mut base = eos.at_pressure(101325., 673., 0.2, 0.05).unwrap();
        base.gas.velocity = 100.;
        let mut errors = Vec::new();
        for factor in [1, 2, 4] {
            let mut pipe = SpeciesPipe::new(1., 0.01, 32, eos, base).unwrap();
            pipe.initialize(|x| SpeciesState {
                fresh_air_fraction: 0.2 + 0.1 * (std::f64::consts::TAU * x).sin(),
                ..base
            })
            .unwrap();
            let duration = 0.001;
            let steps = (duration / pipe.stable_time_step()).ceil() as usize * factor;
            let dt = duration / steps as f64;
            for _ in 0..steps {
                pipe.advance(dt, [SpeciesBoundary::Periodic; 2]).unwrap();
            }
            let theta = std::f64::consts::TAU * pipe.dx();
            let real = -100. / pipe.dx() * (1. - theta.cos());
            let imaginary = -100. / pipe.dx() * theta.sin();
            let error = (0..pipe.cells())
                .map(|i| {
                    let x = (i as f64 + 0.5) * pipe.dx();
                    let exact = 0.2
                        + 0.1
                            * (real * duration).exp()
                            * (std::f64::consts::TAU * x + imaginary * duration).sin();
                    (pipe.state(i).unwrap().fresh_air_fraction - exact).abs() / pipe.cells() as f64
                })
                .sum::<f64>();
            errors.push(error);
        }
        eprintln!("species temporal RK2 errors dt/dt2/dt4: {errors:?}");
        assert!(errors[1] < errors[0] * 0.3 && errors[2] < errors[1] * 0.3);
    }

    #[test]
    fn shared_eos_periodic_contact_closes_all_five_inventories() {
        let eos = GasEquation::SharedMixture;
        let base = eos.at_pressure(101325., 673., 0.2, 0.03).unwrap();
        let mut pipe = SpeciesPipe::new(1., 0.01, 96, eos, base).unwrap();
        pipe.initialize(|x| {
            let fresh = 0.25 + 0.2 * (std::f64::consts::TAU * x).sin();
            let mut q = eos.at_pressure(101325., 673., fresh, 0.03).unwrap();
            q.gas.velocity = 80.;
            q
        })
        .unwrap();
        let initial = pipe.totals();
        pipe.advance(0.002, [SpeciesBoundary::Periodic; 2]).unwrap();
        for (after, before) in pipe.totals().iter().zip(initial) {
            assert!((after - before).abs() < 1e-8);
        }
        let maximum = (0..pipe.cells())
            .map(|i| (pipe.state(i).unwrap().gas.pressure - 101325.).abs())
            .fold(0., f64::max);
        eprintln!("variable EOS conservative contact pressure error: {maximum:.6} Pa");
        // Conservation is not an exact-pressure multi-EOS contact guarantee.
        assert!(maximum < 200.);
    }

    #[test]
    fn material_contact_quantifies_variable_eos_pressure_error_against_fixed_reduction() {
        for eos in [
            GasEquation::Fixed {
                gamma: 1.4,
                gas_constant: 287.,
            },
            GasEquation::SharedMixture,
        ] {
            let base = eos.at_pressure(101325., 673., 0.2, 0.03).unwrap();
            let mut pipe = SpeciesPipe::new(1., 0.01, 128, eos, base).unwrap();
            pipe.initialize(|x| {
                let (t, a, f) = if x < 0.5 {
                    (500., 0.8, 0.02)
                } else {
                    (1100., 0.1, 0.12)
                };
                let mut q = eos.at_pressure(101325., t, a, f).unwrap();
                q.gas.velocity = 100.;
                q
            })
            .unwrap();
            let initial = pipe.totals();
            pipe.advance(0.001, [SpeciesBoundary::Periodic; 2]).unwrap();
            for (after, before) in pipe.totals().iter().zip(initial) {
                assert!((after - before).abs() < 1e-8);
            }
            let maximum = (0..pipe.cells())
                .map(|i| (pipe.state(i).unwrap().gas.pressure - 101325.).abs())
                .fold(0., f64::max);
            eprintln!("material contact {eos:?} max pressure departure: {maximum:.6} Pa");
            match eos {
                GasEquation::Fixed { .. } => assert!(maximum < 1e-6),
                // An explicit known limitation, not a measured accuracy gate:
                // conservative mixing across different T/cv produces pressure
                // errors. Do not erase them with nonconservative double-flux.
                GasEquation::SharedMixture => assert!(maximum > 1e-3 && maximum < 20000.),
            }
        }
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
