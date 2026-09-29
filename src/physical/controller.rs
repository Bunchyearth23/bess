//! Sampled engine controls: idle air/ignition, injection overrun and rev limiting.
//! This module changes air, fuel and ignition requests; it never writes RPM or
//! supplies crank torque. Gains and bypass areas need engine-specific calibration.
use crate::engine_build::Fuel;

const ECU_PERIOD_S: f64 = 0.030;
const IDLE_BYPASS_BASE: f64 = 0.060;
const IDLE_BYPASS_MAX: f64 = 0.35;
const STARTER_BYPASS: f64 = 0.10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlError;
impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid engine controller input")
    }
}
impl std::error::Error for ControlError {}

#[derive(Debug, Clone, Copy)]
pub struct ControlInput {
    pub rpm: f64,
    /// Driver accelerator fraction, not an engine torque request.
    pub throttle: f64,
    /// 1 = full injected-fuel overrun cut; 0 = retain fuel on lift-off.
    pub overrun_strength: f64,
    pub starter: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ControlOutput {
    pub throttle: f64,
    /// Idle-bypass valve position, independently mapped to an additional area
    /// by Manifolds. It is not a main-butterfly angle or a torque request.
    pub bypass: f64,
    pub fuel_multiplier: f64,
    /// Positive means RETARD (later crank angle). Idle retains a nominal 10°
    /// reserve; low RPM advances toward the ordinary MBT setting (zero shift).
    pub spark_shift_rad: f64,
    pub spark_enabled: bool,
    pub dfco: bool,
    pub rev_limited: bool,
}

pub struct Controller {
    idle_rpm: f64,
    redline_rpm: f64,
    fuel: Fuel,
    elapsed: f64,
    integral: f64,
    bypass: f64,
    spark_shift_rad: f64,
    dfco: bool,
    rev_limited: bool,
}

impl Controller {
    pub fn new(idle_rpm: f64, redline_rpm: f64, fuel: Fuel) -> Result<Self, ControlError> {
        if !(300.0..=2000.).contains(&idle_rpm)
            || !(idle_rpm + 1000.0..=20_000.).contains(&redline_rpm)
        {
            return Err(ControlError);
        }
        Ok(Self {
            idle_rpm,
            redline_rpm,
            fuel,
            elapsed: 0.,
            integral: 0.,
            bypass: IDLE_BYPASS_BASE,
            spark_shift_rad: 10_f64.to_radians(),
            dfco: false,
            rev_limited: false,
        })
    }

    pub fn step(
        &mut self,
        dt: f64,
        rpm: f64,
        throttle: f64,
        overrun_strength: f64,
        starter: bool,
    ) -> Result<ControlOutput, ControlError> {
        self.step_input(
            dt,
            ControlInput {
                rpm,
                throttle,
                overrun_strength,
                starter,
            },
        )
    }

    pub fn step_input(
        &mut self,
        dt: f64,
        input: ControlInput,
    ) -> Result<ControlOutput, ControlError> {
        if !(1e-9..=0.1).contains(&dt)
            || !(0.0..=100_000.).contains(&input.rpm)
            || !(0.0..=1.).contains(&input.throttle)
            || !(0.0..=1.).contains(&input.overrun_strength)
        {
            return Err(ControlError);
        }
        self.elapsed += dt;
        while self.elapsed + 1e-12 >= ECU_PERIOD_S {
            self.elapsed = (self.elapsed - ECU_PERIOD_S).max(0.);
            self.idle_tick(input);
        }

        // Fuel/spark protection reacts at the physical step, with RPM hysteresis.
        // It cuts energy input only; a drivetrain can mechanically over-rev.
        if input.rpm >= self.redline_rpm {
            self.rev_limited = true;
        } else if input.rpm <= self.redline_rpm - 150. {
            self.rev_limited = false;
        }
        if self.fuel == Fuel::Carburettor
            || input.starter
            || input.throttle > 0.04
            || input.rpm <= self.idle_rpm + 200.
            || input.overrun_strength == 0.
        {
            self.dfco = false;
        } else if input.throttle <= 0.02 && input.rpm >= self.idle_rpm * 1.8 {
            self.dfco = true;
        }
        let fuel_multiplier = if self.rev_limited && self.fuel != Fuel::Carburettor {
            0.
        } else if self.dfco {
            1. - input.overrun_strength
        } else if input.starter && input.rpm < self.idle_rpm * 0.65 {
            1.12
        } else {
            1.
        };
        let idling = input.throttle <= 0.04 && input.rpm < self.idle_rpm * 1.8;
        // The report's overrun setting also delays any retained combustion;
        // it does not inject arbitrary torque pulses or force an audio gain.
        let overrun_retard = if input.throttle <= 0.02 && input.rpm > self.idle_rpm * 1.5 {
            (12. * input.overrun_strength).to_radians()
        } else {
            0.
        };
        Ok(ControlOutput {
            throttle: input.throttle,
            bypass: if idling {
                self.bypass
            } else if input.throttle <= 0.04 {
                0.
            } else {
                IDLE_BYPASS_BASE
            },
            fuel_multiplier,
            spark_shift_rad: if idling {
                self.spark_shift_rad
            } else {
                overrun_retard
            },
            spark_enabled: !self.rev_limited && input.rpm >= 40. && fuel_multiplier > 0.,
            dfco: self.dfco,
            rev_limited: self.rev_limited,
        })
    }

    fn idle_tick(&mut self, input: ControlInput) {
        if input.throttle > 0.04 {
            // Release accumulated idle compensation gradually after tip-in;
            // high-RPM coast must not wind the integrator to an extreme.
            self.integral *= (-ECU_PERIOD_S / 0.5).exp();
            return;
        }
        if input.rpm >= self.idle_rpm * 1.8 {
            // Closing idle air on coast must never reopen the baseline valve
            // when RPM crosses the idle-control region. That discontinuity
            // previously drove a self-sustaining overspeed/DFCO limit cycle.
            self.bypass = 0.;
            self.integral = self.integral.min(0.);
            return;
        }
        let error = ((self.idle_rpm - input.rpm) / self.idle_rpm).clamp(-1., 1.);
        let proportional = 0.10 * error;
        let candidate = self.integral + 0.20 * error * ECU_PERIOD_S;
        let raw = IDLE_BYPASS_BASE + proportional + candidate;
        // Conditional integration prevents saturation from storing a long
        // throttle surge. No measured-engine claims for these initial PI gains.
        // The starter's air floor is a saturation too: winding down beneath it
        // during the start flare dropped the bypass to zero on release.
        let floor = if input.starter { STARTER_BYPASS } else { 0. };
        if (floor..=IDLE_BYPASS_MAX).contains(&raw)
            || (raw > IDLE_BYPASS_MAX && error < 0.)
            || (raw < floor && error > 0.)
        {
            self.integral = candidate.clamp(-IDLE_BYPASS_BASE, IDLE_BYPASS_MAX - IDLE_BYPASS_BASE);
        }
        self.bypass = (IDLE_BYPASS_BASE + proportional + self.integral).clamp(0., IDLE_BYPASS_MAX);
        if input.starter {
            self.bypass = self.bypass.max(STARTER_BYPASS);
        }
        self.spark_shift_rad = (10. - 30. * error).clamp(0., 20.).to_radians();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn controller(fuel: Fuel) -> Controller {
        Controller::new(850., 7000., fuel).unwrap()
    }

    #[test]
    fn idle_pi_updates_every_thirty_ms_and_releases_spark_reserve() {
        let mut c = controller(Fuel::PortInjection);
        let baseline = c.step(0.01, 650., 0., 1., false).unwrap();
        let held = c.step(0.01, 650., 0., 1., false).unwrap();
        assert_eq!(baseline.bypass, held.bypass);
        let adjusted = c.step(0.01, 650., 0., 1., false).unwrap();
        assert!(adjusted.bypass > held.bypass);
        assert!(adjusted.spark_shift_rad < held.spark_shift_rad);
        assert_eq!(adjusted.throttle, 0.);
    }

    #[test]
    fn injected_overrun_has_rpm_and_throttle_hysteresis() {
        for fuel in [Fuel::PortInjection, Fuel::DirectInjection] {
            let mut c = controller(fuel);
            assert!(!c.step(0.001, 1500., 0., 1., false).unwrap().dfco);
            let cut = c.step(0.001, 1600., 0., 1., false).unwrap();
            assert!(cut.dfco && !cut.spark_enabled);
            assert_eq!(cut.fuel_multiplier, 0.);
            assert!(c.step(0.001, 1200., 0.03, 1., false).unwrap().dfco);
            assert!(!c.step(0.001, 1050., 0., 1., false).unwrap().dfco);
            c.step(0.001, 3000., 0., 1., false).unwrap();
            assert!(!c.step(0.001, 3000., 0.05, 1., false).unwrap().dfco);
        }
    }

    #[test]
    fn carb_keeps_fuel_on_coast_and_cuts_spark_at_redline() {
        let mut c = controller(Fuel::Carburettor);
        let coast = c.step(0.001, 3000., 0., 1., false).unwrap();
        assert!(!coast.dfco && coast.spark_enabled);
        assert_eq!(coast.fuel_multiplier, 1.);
        let limit = c.step(0.001, 7100., 1., 1., false).unwrap();
        assert!(limit.rev_limited && !limit.spark_enabled);
        assert_eq!(limit.fuel_multiplier, 1.);
    }

    #[test]
    fn limiter_cuts_energy_without_overriding_accelerator() {
        let mut c = controller(Fuel::DirectInjection);
        let limit = c.step(0.001, 7001., 0.8, 1., false).unwrap();
        assert!(limit.rev_limited);
        assert_eq!(limit.fuel_multiplier, 0.);
        assert_eq!(limit.throttle, 0.8);
        assert!(c.step(0.001, 6900., 0.8, 1., false).unwrap().rev_limited);
        assert!(!c.step(0.001, 6849., 0.8, 1., false).unwrap().rev_limited);
    }

    #[test]
    fn idle_saturation_does_not_wind_up_and_open_throttle_is_preserved() {
        let mut c = controller(Fuel::PortInjection);
        for _ in 0..1000 {
            c.step(0.03, 0., 0., 1., false).unwrap();
        }
        let open = c.step(0.03, 850., 0.7, 1., false).unwrap();
        assert_eq!(open.throttle, 0.7);
        assert_eq!(open.bypass, IDLE_BYPASS_BASE);
        for _ in 0..400 {
            c.step(0.03, 1200., 0., 1., false).unwrap();
        }
        let recovered = c.step(0.03, 850., 0., 1., false).unwrap();
        assert!(recovered.bypass <= IDLE_BYPASS_BASE);
    }
    #[test]
    fn closed_driver_throttle_never_reopens_idle_air_at_overspeed_boundary() {
        let mut c = controller(Fuel::PortInjection);
        for _ in 0..300 {
            c.step(0.03, 1200.0, 0.0, 1.0, false).unwrap();
        }
        let before = c.step(0.03, 1200.0, 0.0, 1.0, false).unwrap();
        let old_boundary = c.step(0.03, 1300.0, 0.0, 1.0, false).unwrap();
        let coast = c.step(0.03, 1800.0, 0.0, 1.0, false).unwrap();
        // Conditional anti-windup can leave less than one 30 ms integration
        // quantum above the closed stop. The regression is the old 0.06 jump.
        assert!(before.bypass < 0.005);
        assert!(old_boundary.bypass <= before.bypass);
        assert_eq!(coast.bypass, 0.0);
    }

    #[test]
    fn stationary_engine_gets_no_magic_ignition_or_torque() {
        let mut c = controller(Fuel::PortInjection);
        let stopped = c.step(0.03, 0., 0., 1., false).unwrap();
        assert!(!stopped.spark_enabled);
        let cranking = c.step(0.03, 200., 0., 1., true).unwrap();
        assert!(cranking.spark_enabled && !cranking.dfco);
        assert!(cranking.fuel_multiplier > 1.);
    }

    #[test]
    fn partial_overrun_and_invalid_inputs_are_explicit() {
        let mut c = controller(Fuel::PortInjection);
        let partial = c.step(0.001, 3000., 0., 0.6, false).unwrap();
        assert!((partial.fuel_multiplier - 0.4).abs() < 1e-12);
        assert!(partial.spark_enabled);
        assert!(!c.step(0.001, 3000., 0., 0., false).unwrap().dfco);
        assert!(c.step(0.001, f64::NAN, 0., 1., false).is_err());
        assert!(c.step(-0.1, 850., 0., 1., false).is_err());
    }
}
