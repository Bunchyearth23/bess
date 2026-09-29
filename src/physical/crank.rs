//! One-degree-of-freedom forward-rotating crankshaft, SI units and f64 state.
//! Gas and reciprocating torques come from the caller's cylinder mechanics.
//! No RPM target, torque curve or idle-restoring force is present here.
use std::f64::consts::{PI, TAU};

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
            c_pa_per_m_s: 5000.,
            d_pa_per_m2_s2: 400.,
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
        let speed = 2. * self.stroke_m * self.state.rpm / 60.;
        let model = self.friction;
        let fmep = model.a_pa
            + model.b * self.peak_pressure_pa
            + model.c_pa_per_m_s * speed
            + model.d_pa_per_m2_s2 * speed * speed;
        let friction_nm = fmep * self.displacement_m3 / (4. * PI);
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

#[cfg(test)]
mod tests {
    use super::*;
    fn crank(rpm: f64) -> Crank {
        Crank::new(0.22, 0.086, 0.002, rpm).unwrap()
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
