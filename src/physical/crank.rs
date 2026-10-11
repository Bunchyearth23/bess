//! One-degree-of-freedom forward-rotating crankshaft, SI units and f64 state.
//! Gas and reciprocating torques come from the caller's cylinder mechanics.
//! No RPM target, torque curve or idle-restoring force is present here.
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

/// Two-inertia engineering model; inertia fraction partitions the existing
/// total inertia, so enabling it never adds hidden rotational energy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TorsionConfig {
    pub crank_inertia_fraction: f64,
    pub stiffness_nm_rad: f64,
    pub damping_nm_s_rad: f64,
}
impl Default for TorsionConfig {
    fn default() -> Self {
        Self {
            crank_inertia_fraction: 0.35,
            stiffness_nm_rad: 18000.0,
            damping_nm_s_rad: 2.0,
        }
    }
}
impl TorsionConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.05..=0.95).contains(&self.crank_inertia_fraction)
            || !(1.0..=1e7).contains(&self.stiffness_nm_rad)
            || !(0.0..=1e4).contains(&self.damping_nm_s_rad)
        {
            return Err("Invalid torsion inertia split, stiffness or damping".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub struct TorsionLedger {
    pub spring_energy_j: f64,
    pub damping_loss_j: f64,
    /// Inelastic forward-stop projection, explicitly accounted, never clipped silently.
    pub stop_loss_j: f64,
    pub flywheel_rpm: f64,
    pub twist_rad: f64,
    pub transmitted_torque_nm: f64,
    pub stop_events: u32,
    pub event_solver_iterations: u32,
}
struct Torsion {
    config: TorsionConfig,
    flywheel_omega: f64,
    twist: f64,
    ledger: TorsionLedger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrankError {
    InvalidGeometry,
    InvalidInput,
}

impl std::fmt::Display for CrankError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CrankError {}

/// Estimated Chen-Flynn FMEP coefficients, not a measured friction map.
/// FMEP = a + b * peak cylinder pressure + c * mean piston speed + d * speed².
/// Total four-stroke friction torque is FMEP * total displacement / (4π).
/// Sized to the middle of the published scatter band for modern 4-valve SI
/// engines (≈0.8 bar at 2000 rpm, ≈2.0 bar at 6000 rpm for an 86 mm stroke;
/// Sandoval & Heywood SAE 2003-01-0725, FEV friction scatter band). The former
/// 400 Pa/(m/s)² speed² term gave 3.0 bar at 20 m/s, double those references.
#[derive(Debug, Clone, Copy)]
pub struct FrictionModel {
    pub a_pa: f64,
    pub b: f64,
    pub c_pa_per_m_s: f64,
    pub d_pa_per_m2_s2: f64,
}
impl Default for FrictionModel {
    fn default() -> Self {
        Self {
            a_pa: 30_000.,
            b: 0.005,
            c_pa_per_m_s: 6000.,
            d_pa_per_m2_s2: 200.,
        }
    }
}
impl FrictionModel {
    pub const ZERO: Self = Self {
        a_pa: 0.,
        b: 0.,
        c_pa_per_m_s: 0.,
        d_pa_per_m2_s2: 0.,
    };

    fn valid(self) -> bool {
        (0.0..=1e6).contains(&self.a_pa)
            && (0.0..=1.).contains(&self.b)
            && (0.0..=1e6).contains(&self.c_pa_per_m_s)
            && (0.0..=1e6).contains(&self.d_pa_per_m2_s2)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CrankState {
    pub angle_unwrapped: f64,
    pub omega_rad_s: f64,
    pub rpm: f64,
    /// Below 40 rpm: a reporting state, never an artificial speed clamp.
    pub stalled: bool,
    pub friction_nm: f64,
    pub accessory_nm: f64,
    pub starter_nm: f64,
    pub net_torque_nm: f64,
    pub kinetic_energy_j: f64,
    /// Work ledger for this step. Loss fields are positive for dissipation.
    pub gas_work_j: f64,
    pub reciprocating_work_j: f64,
    pub starter_work_j: f64,
    pub load_work_j: f64,
    pub friction_work_j: f64,
    pub accessory_work_j: f64,
}

pub struct Crank {
    inertia_kg_m2: f64,
    stroke_m: f64,
    displacement_m3: f64,
    friction: FrictionModel,
    peak_pressure_pa: f64,
    accessory_nm: f64,
    state: CrankState,
    torsion: Option<Torsion>,
}

impl Crank {
    pub fn new(
        inertia_kg_m2: f64,
        stroke_m: f64,
        displacement_m3: f64,
        initial_rpm: f64,
    ) -> Result<Self, CrankError> {
        if !(1e-4..=100.).contains(&inertia_kg_m2)
            || !(0.005..=0.5).contains(&stroke_m)
            || !(1e-7..=0.5).contains(&displacement_m3)
            || !(0.0..=100_000.).contains(&initial_rpm)
        {
            return Err(CrankError::InvalidGeometry);
        }
        let omega_rad_s = initial_rpm * TAU / 60.;
        Ok(Self {
            inertia_kg_m2,
            stroke_m,
            displacement_m3,
            friction: FrictionModel::default(),
            peak_pressure_pa: 0.,
            accessory_nm: 0.,
            torsion: None,
            state: CrankState {
                omega_rad_s,
                rpm: initial_rpm,
                stalled: initial_rpm < 40.,
                kinetic_energy_j: 0.5 * inertia_kg_m2 * omega_rad_s * omega_rad_s,
                ..Default::default()
            },
        })
    }

    pub fn state(&self) -> CrankState {
        self.state
    }

    pub fn set_torsion(&mut self, config: Option<TorsionConfig>) -> Result<(), CrankError> {
        if let Some(config) = config {
            config.validate().map_err(|_| CrankError::InvalidGeometry)?;
            self.torsion = Some(Torsion {
                config,
                flywheel_omega: self.state.omega_rad_s,
                twist: 0.0,
                ledger: TorsionLedger {
                    flywheel_rpm: self.state.rpm,
                    ..Default::default()
                },
            });
        } else {
            self.torsion = None;
        }
        Ok(())
    }
    pub fn torsion_ledger(&self) -> TorsionLedger {
        self.torsion
            .as_ref()
            .map_or(TorsionLedger::default(), |t| t.ledger)
    }

    pub fn set_friction(&mut self, friction: FrictionModel) -> Result<(), CrankError> {
        if !friction.valid() {
            return Err(CrankError::InvalidInput);
        }
        self.friction = friction;
        Ok(())
    }

    /// Caller supplies a cycle peak (absolute pressure) for the load-sensitive
    /// friction term. The default zero omits that term until pressure is known.
    pub fn set_peak_pressure_pa(&mut self, pressure_pa: f64) -> Result<(), CrankError> {
        if !(0.0..=1e9).contains(&pressure_pa) {
            return Err(CrankError::InvalidInput);
        }
        self.peak_pressure_pa = pressure_pa;
        Ok(())
    }

    /// Illustrative compressor and electrical loads from the design report.
    pub fn set_accessories(&mut self, compressor: bool, electrical: bool) {
        self.accessory_nm = if compressor { 16. } else { 0. } + if electrical { 22. } else { 0. };
    }

    /// Friction torque at `rpm` with the last supplied peak pressure.
    pub fn friction_nm(&self, rpm: f64) -> f64 {
        let speed = 2. * self.stroke_m * rpm / 60.;
        let model = self.friction;
        let fmep = model.a_pa
            + model.b * self.peak_pressure_pa
            + model.c_pa_per_m_s * speed
            + model.d_pa_per_m2_s2 * speed * speed;
        fmep * self.displacement_m3 / (4. * PI)
    }

    pub fn set_angle_unwrapped(&mut self, angle: f64) -> Result<(), CrankError> {
        if !angle.is_finite() || angle.abs() > 1e15 {
            return Err(CrankError::InvalidInput);
        }
        self.state.angle_unwrapped = angle;
        Ok(())
    }

    pub fn step(
        &mut self,
        dt: f64,
        gas_torque_nm: f64,
        reciprocating_torque_nm: f64,
        load_nm: f64,
        starter: bool,
    ) -> Result<CrankState, CrankError> {
        if !(1e-9..=0.02).contains(&dt)
            || [gas_torque_nm, reciprocating_torque_nm, load_nm]
                .iter()
                .any(|x| !x.is_finite() || x.abs() > 1e6)
        {
            return Err(CrankError::InvalidInput);
        }
        let friction_nm = self.friction_nm(self.state.rpm);
        // Approximate DC starter torque-speed line with a freewheel. It cannot
        // brake a running engine, and contributes no torque unless commanded.
        let starter_nm = if starter {
            (self.displacement_m3 * 75_000.).clamp(30., 600.)
                * (1. - self.state.rpm / 400.).clamp(0., 1.)
        } else {
            0.
        };
        let net_torque_nm = gas_torque_nm + reciprocating_torque_nm + starter_nm
            - load_nm
            - friction_nm
            - self.accessory_nm;
        if let Some(t) = &mut self.torsion {
            let j1 = self.inertia_kg_m2 * t.config.crank_inertia_fraction;
            let j2 = self.inertia_kg_m2 - j1;
            let omega1 = self.state.omega_rad_s;
            let omega2 = t.flywheel_omega;
            let drive = gas_torque_nm + reciprocating_torque_nm + starter_nm
                - friction_nm
                - self.accessory_nm;
            let response = -load_nm;
            let k = t.config.stiffness_nm_rad;
            let c = t.config.damping_nm_s_rad;
            let mut w1 = omega1;
            let mut w2 = omega2;
            let mut q = t.twist;
            let mut remaining = dt;
            let mut angle1 = 0.0;
            let mut angle2 = 0.0;
            let mut damping = 0.0;
            let mut stop_loss = 0.0;
            let mut transmitted = 0.0;
            let mut stop_events = 0;
            let mut event_solver_iterations = 0;
            // Resolve an intra-step zero-speed event before changing its static
            // constraint. This prevents backwards angles/negative drag work.
            for _ in 0..64 {
                if remaining <= dt * 1e-14 {
                    break;
                }
                let mut h = remaining;
                let mut trial = torsion_midpoint(h, [j1, j2, k, c], [w1, w2, q], [drive, response]);
                if trial[0] < 0.0 || trial[1] < 0.0 {
                    stop_events += 1;
                    let mut lo = 0.0;
                    let mut hi = h;
                    for _ in 0..52 {
                        event_solver_iterations += 1;
                        let mid = 0.5 * (lo + hi);
                        let candidate =
                            torsion_midpoint(mid, [j1, j2, k, c], [w1, w2, q], [drive, response]);
                        if candidate[0] >= 0.0 && candidate[1] >= 0.0 {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    h = hi;
                    trial = torsion_midpoint(h, [j1, j2, k, c], [w1, w2, q], [drive, response]);
                }
                let a1 = h * 0.5 * (w1 + trial[0]);
                let a2 = h * 0.5 * (w2 + trial[1]);
                let vm = 0.5 * (w1 + trial[0] - w2 - trial[1]);
                angle1 += a1;
                angle2 += a2;
                damping += h * c * vm * vm;
                transmitted += h * (k * 0.5 * (q + trial[2]) + c * vm);
                stop_loss +=
                    0.5 * j1 * trial[0].min(0.0).powi(2) + 0.5 * j2 * trial[1].min(0.0).powi(2);
                w1 = trial[0].max(0.0);
                w2 = trial[1].max(0.0);
                q = trial[2];
                remaining -= h;
            }
            // The supported physics cadence resolves at most a few events.
            // Failure is explicit rather than silently omitting elapsed time.
            if remaining > dt * 1e-12 {
                return Err(CrankError::InvalidInput);
            }
            let next1 = w1;
            let next2 = w2;
            t.twist = q;
            t.flywheel_omega = next2;
            t.ledger = TorsionLedger {
                spring_energy_j: 0.5 * k * q * q,
                damping_loss_j: damping,
                stop_loss_j: stop_loss,
                flywheel_rpm: next2 * 60.0 / TAU,
                twist_rad: q,
                transmitted_torque_nm: transmitted / dt,
                stop_events,
                event_solver_iterations,
            };
            self.state = CrankState {
                angle_unwrapped: self.state.angle_unwrapped + angle1,
                omega_rad_s: next1,
                rpm: next1 * 60.0 / TAU,
                stalled: next1 * 60.0 / TAU < 40.0,
                friction_nm,
                accessory_nm: self.accessory_nm,
                starter_nm,
                net_torque_nm: drive + response,
                kinetic_energy_j: 0.5 * j1 * next1 * next1 + 0.5 * j2 * next2 * next2,
                gas_work_j: gas_torque_nm * angle1,
                reciprocating_work_j: reciprocating_torque_nm * angle1,
                starter_work_j: starter_nm * angle1,
                load_work_j: load_nm * angle2,
                friction_work_j: friction_nm * angle1,
                accessory_work_j: self.accessory_nm * angle1,
            };
            return Ok(self.state);
        }
        let alpha = net_torque_nm / self.inertia_kg_m2;
        let omega = self.state.omega_rad_s;
        // Integrate constant torque exactly for this short step. If braking
        // reaches rest mid-step, integrate only to rest. This forward-engine
        // prototype does not simulate reverse kickback, nor create negative RPM.
        let active_dt = if alpha < 0. {
            dt.min(omega / -alpha)
        } else {
            dt
        };
        let next_omega = (omega + alpha * active_dt).max(0.);
        let angle_step = (omega + next_omega) * 0.5 * active_dt;
        let rpm = next_omega * 60. / TAU;
        self.state = CrankState {
            angle_unwrapped: self.state.angle_unwrapped + angle_step,
            omega_rad_s: next_omega,
            rpm,
            stalled: rpm < 40.,
            friction_nm,
            accessory_nm: self.accessory_nm,
            starter_nm,
            net_torque_nm,
            kinetic_energy_j: 0.5 * self.inertia_kg_m2 * next_omega * next_omega,
            gas_work_j: gas_torque_nm * angle_step,
            reciprocating_work_j: reciprocating_torque_nm * angle_step,
            starter_work_j: starter_nm * angle_step,
            load_work_j: load_nm * angle_step,
            friction_work_j: friction_nm * angle_step,
            accessory_work_j: self.accessory_nm * angle_step,
        };
        Ok(self.state)
    }
}

/// [next crank speed,next flywheel speed,next relative angle]. A static
/// constraint has zero angle/work and disappears as soon as torque releases it.
fn torsion_midpoint(h: f64, parameters: [f64; 4], state: [f64; 3], torques: [f64; 2]) -> [f64; 3] {
    let [j1, j2, k, c] = parameters;
    let [w1, w2, q] = state;
    let [drive, response] = torques;
    let spring = k * q + c * (w1 - w2);
    let held1 = w1 == 0.0 && drive - spring <= 0.0;
    let held2 = w2 == 0.0 && response + spring <= 0.0;
    let (mid1, mid2) = if held1 && held2 {
        (0.0, 0.0)
    } else if held1 {
        (
            0.0,
            (w2 + 0.5 * h * (response + k * q) / j2)
                / (1.0 + 0.5 * h * c / j2 + 0.25 * h * h * k / j2),
        )
    } else if held2 {
        (
            (w1 + 0.5 * h * (drive - k * q) / j1)
                / (1.0 + 0.5 * h * c / j1 + 0.25 * h * h * k / j1),
            0.0,
        )
    } else {
        let a = 1.0 / j1 + 1.0 / j2;
        let vm = (w1 - w2 + 0.5 * h * (drive / j1 - response / j2 - a * k * q))
            / (1.0 + 0.5 * h * a * c + 0.25 * h * h * a * k);
        let torque = k * (q + 0.5 * h * vm) + c * vm;
        (
            w1 + 0.5 * h * (drive - torque) / j1,
            w2 + 0.5 * h * (response + torque) / j2,
        )
    };
    [2.0 * mid1 - w1, 2.0 * mid2 - w2, q + h * (mid1 - mid2)]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn crank(rpm: f64) -> Crank {
        Crank::new(0.22, 0.086, 0.002, rpm).unwrap()
    }
    #[test]
    fn torsion_static_load_holds_both_rotors_and_one_blocked_rotor_has_zero_work() {
        let mut parked = crank(0.0);
        parked.set_torsion(Some(TorsionConfig::default())).unwrap();
        for _ in 0..9600 {
            let s = parked.step(1.0 / 96000.0, 0.0, 0.0, 400.0, false).unwrap();
            let t = parked.torsion_ledger();
            assert_eq!(s.angle_unwrapped, 0.0);
            assert_eq!(s.kinetic_energy_j, 0.0);
            assert_eq!(t.spring_energy_j, 0.0);
            assert_eq!(t.stop_events, 0);
            assert_eq!(t.event_solver_iterations, 0);
        }
        let mut one = crank(0.0);
        one.set_friction(FrictionModel::ZERO).unwrap();
        one.set_torsion(Some(TorsionConfig::default())).unwrap();
        let mut total_events = 0;
        for _ in 0..9600 {
            let s = one.step(1.0 / 96000.0, 40.0, 0.0, 400.0, false).unwrap();
            let t = one.torsion_ledger();
            assert_eq!(t.flywheel_rpm, 0.0);
            assert_eq!(s.load_work_j, 0.0);
            assert!(t.stop_events <= 2);
            assert_eq!(t.event_solver_iterations, 52 * t.stop_events);
            total_events += t.stop_events;
        }
        assert!(one.state().angle_unwrapped > 0.0);
        assert!(total_events > 0);
    }
    #[test]
    fn torsion_stall_grid_never_reverses_or_turns_drag_into_a_source() {
        for rate in [48000, 96000, 192000] {
            for rpm in [0.0, 0.01, 40.0, 850.0] {
                for load in [0.0, 80.0, 400.0] {
                    let mut c = crank(rpm);
                    c.set_torsion(Some(TorsionConfig::default())).unwrap();
                    c.set_accessories(true, true);
                    let initial = c.state().kinetic_energy_j;
                    let mut work = 0.0;
                    let mut loss = 0.0;
                    let mut previous_angle = 0.0;
                    for _ in 0..rate / 20 {
                        let s = c
                            .step(1.0 / f64::from(rate), 0.0, 0.0, load, false)
                            .unwrap();
                        let t = c.torsion_ledger();
                        assert!(
                            s.angle_unwrapped >= previous_angle - 1e-15
                                && s.rpm >= 0.0
                                && t.flywheel_rpm >= 0.0
                        );
                        assert!(
                            s.friction_work_j >= -1e-14
                                && s.accessory_work_j >= -1e-14
                                && s.load_work_j >= -1e-14
                        );
                        previous_angle = s.angle_unwrapped;
                        work -= s.friction_work_j + s.accessory_work_j + s.load_work_j;
                        loss += t.damping_loss_j + t.stop_loss_j;
                        assert!(
                            (s.kinetic_energy_j + t.spring_energy_j - initial - work + loss).abs()
                                < 1e-7,
                            "rate={rate} rpm={rpm} load={load}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn torsion_conserves_work_and_damping_over_load_transient() {
        let mut c = crank(3000.0);
        c.set_friction(FrictionModel::ZERO).unwrap();
        c.set_torsion(Some(TorsionConfig::default())).unwrap();
        let initial = c.state().kinetic_energy_j;
        let mut work = 0.0;
        let mut damping = 0.0;
        let mut peak_twist: f64 = 0.0;
        for i in 0..9600 {
            let s = c
                .step(
                    1.0 / 96000.0,
                    if i < 4800 { 80.0 } else { 20.0 },
                    0.0,
                    40.0,
                    false,
                )
                .unwrap();
            let t = c.torsion_ledger();
            work += s.gas_work_j - s.load_work_j;
            damping += t.damping_loss_j + t.stop_loss_j;
            peak_twist = peak_twist.max(t.twist_rad.abs());
            assert!(
                (s.kinetic_energy_j + t.spring_energy_j - initial - work + damping).abs() < 1e-7
            );
        }
        assert!(peak_twist > 0.0);
        assert!(damping > 0.0);
    }
    #[test]
    fn torsion_energy_converges_and_stopping_loss_is_reported() {
        let run = |rate: f64| {
            let mut c = crank(1000.0);
            c.set_friction(FrictionModel::ZERO).unwrap();
            let config = TorsionConfig {
                damping_nm_s_rad: 0.0,
                ..Default::default()
            };
            c.set_torsion(Some(config)).unwrap();
            for _ in 0..(rate / 10.0) as usize {
                c.step(1.0 / rate, 80.0, 0.0, 40.0, false).unwrap();
            }
            c.state().rpm
        };
        let a = run(96000.0);
        let b = run(192000.0);
        assert!((a - b).abs() < 0.02);
        let mut c = crank(0.01);
        c.set_torsion(Some(TorsionConfig::default())).unwrap();
        let initial = c.state().kinetic_energy_j;
        let s = c.step(1.0 / 96000.0, 0.0, 0.0, 400.0, false).unwrap();
        let t = c.torsion_ledger();
        assert!(t.stop_loss_j >= 0.0);
        assert!(s.angle_unwrapped >= 0.0 && s.friction_work_j >= 0.0 && s.load_work_j >= 0.0);
        assert!(
            (s.kinetic_energy_j + t.spring_energy_j - initial
                + s.load_work_j
                + s.friction_work_j
                + t.stop_loss_j
                + t.damping_loss_j)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn applied_torque_matches_analytic_speed_and_work() {
        let mut c = crank(0.);
        c.set_friction(FrictionModel::ZERO).unwrap();
        let mut work = 0.;
        for _ in 0..9600 {
            let s = c.step(1. / 96000., 60., -5., 10., false).unwrap();
            work += s.gas_work_j + s.reciprocating_work_j - s.load_work_j;
        }
        let s = c.state();
        assert!((s.omega_rad_s - 45. * 0.1 / 0.22).abs() < 1e-8);
        assert!((s.kinetic_energy_j - work).abs() < 1e-8);
    }

    #[test]
    fn zero_gas_coasts_to_rest_without_idle_support_or_reverse_spin() {
        let mut c = crank(900.);
        let initial_energy = c.state().kinetic_energy_j;
        let mut losses = 0.;
        let mut last_rpm = 900.;
        for _ in 0..2000 {
            let s = c.step(0.005, 0., 0., 8., false).unwrap();
            assert!(s.rpm <= last_rpm && s.rpm >= 0.);
            losses += s.friction_work_j + s.load_work_j;
            last_rpm = s.rpm;
        }
        assert_eq!(c.state().rpm, 0.);
        assert!(c.state().stalled);
        assert!((initial_energy - losses).abs() < 1e-8);
    }

    #[test]
    fn starter_turns_a_stopped_engine_and_freewheels_above_its_speed() {
        let mut c = crank(0.);
        for _ in 0..1000 {
            c.step(0.001, 0., 0., 0., true).unwrap();
        }
        assert!(c.state().rpm > 200. && c.state().rpm < 400.);
        assert!(!c.state().stalled);
        let mut running = crank(900.);
        assert_eq!(
            running.step(0.001, 0., 0., 0., true).unwrap().starter_nm,
            0.
        );
    }

    #[test]
    fn accessories_and_pressure_friction_are_real_energy_losses() {
        let mut c = crank(1200.);
        c.set_accessories(true, true);
        c.set_peak_pressure_pa(5e6).unwrap();
        let before = c.state().kinetic_energy_j;
        let s = c.step(0.001, 0., 0., 0., false).unwrap();
        assert_eq!(s.accessory_nm, 38.);
        assert!(s.friction_nm > 10.);
        assert!(
            (before - s.kinetic_energy_j - s.friction_work_j - s.accessory_work_j).abs() < 1e-9
        );
    }

    #[test]
    fn invalid_steps_do_not_mutate_state() {
        let mut c = crank(850.);
        let angle = c.state().angle_unwrapped;
        for bad in [f64::NAN, f64::INFINITY, -1.] {
            assert!(c.step(bad, 0., 0., 0., false).is_err());
        }
        assert!(c.step(0.001, f64::NAN, 0., 0., false).is_err());
        assert_eq!(c.state().angle_unwrapped, angle);
    }
}
