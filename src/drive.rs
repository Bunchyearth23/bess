//! Forward-only listening-bench driveline, not a reconstruction of the imported car.
//! A fixed 1 ms step couples engine and wheel inertias through a slipping clutch.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Direct,
    Simulated,
    Cycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Controls {
    pub mode: Mode,
    pub throttle: f32,
    pub brake: f32,
    pub gear: u8,
    pub automatic: bool,
    /// Additional resisting torque at the wheels, after gearbox and final drive.
    pub resistance_nm: f32,
    pub grade_percent: f32,
    pub mass_kg: f32,
    pub peak_torque_nm: f32,
    pub inertia: f32,
    pub wheel_radius: f32,
    pub final_drive: f32,
    pub ratios: [f32; 6],
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            mode: Mode::Direct,
            throttle: 0.,
            brake: 0.,
            gear: 1,
            automatic: true,
            resistance_nm: 0.,
            grade_percent: 0.,
            mass_kg: 1300.,
            peak_torque_nm: 280.,
            inertia: 0.35,
            wheel_radius: 0.32,
            final_drive: 3.7,
            ratios: [3.3, 2.1, 1.5, 1.15, 0.95, 0.78],
        }
    }
}
impl Controls {
    pub fn validate(&self) -> Result<(), String> {
        if self.gear > 6 {
            return Err("Gear must be N or 1–6".into());
        }
        for (value, min, max) in [
            (self.throttle, 0., 1.),
            (self.brake, 0., 1.),
            (self.resistance_nm, 0., 4000.),
            (self.grade_percent, 0., 25.),
            (self.mass_kg, 300., 6000.),
            (self.peak_torque_nm, 30., 2000.),
            (self.inertia, 0.1, 2.),
            (self.wheel_radius, 0.2, 0.6),
            (self.final_drive, 1.5, 6.),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err("Driving setting out of range".into());
            }
        }
        if self
            .ratios
            .iter()
            .any(|r| !r.is_finite() || !(0.4..=5.).contains(r))
            || self.ratios.windows(2).any(|r| r[0] <= r[1])
        {
            return Err("The six gear ratios must decrease and stay between 0.4 and 5".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct State {
    pub rpm: f32,
    pub load: f32,
    pub speed_kmh: f32,
    pub gear: u8,
    pub wheel_torque: f32,
    pub resisting_torque: f32,
    pub shifting: bool,
    pub limited: bool,
    pub shift_rejected: bool,
}

pub const TICK_RATE: u32 = 1000;
pub const STEP: f32 = 1. / TICK_RATE as f32;
const RPM_PER_RAD: f32 = 60. / std::f32::consts::TAU;
pub struct Simulator {
    state: State,
    omega: f32,
    wheel: f32,
    idle: f32,
    maximum: f32,
    throttle: f32,
    shift_left: f32,
    shift_cooldown: f32,
    requested_gear: u8,
    was_auto: bool,
}
impl Simulator {
    pub fn new(min: f32, max: f32, controls: Controls) -> Self {
        let mut result = Self {
            state: State::default(),
            omega: 0.,
            wheel: 0.,
            idle: min / RPM_PER_RAD,
            maximum: max / RPM_PER_RAD,
            throttle: 0.,
            shift_left: 0.,
            shift_cooldown: 0.,
            requested_gear: controls.gear,
            was_auto: controls.automatic,
        };
        result.reset(controls);
        result
    }
    pub fn reset(&mut self, c: Controls) {
        self.omega = self.idle;
        self.wheel = 0.;
        self.throttle = 0.;
        self.shift_left = 0.;
        self.shift_cooldown = 0.;
        self.requested_gear = c.gear;
        self.was_auto = c.automatic;
        self.state = State {
            rpm: self.idle * RPM_PER_RAD,
            load: 0.1,
            gear: if c.automatic { 1 } else { c.gear },
            ..State::default()
        };
    }
    pub fn state(&self) -> State {
        self.state
    }
    fn ratio(c: Controls, gear: u8) -> f32 {
        if gear == 0 {
            0.
        } else {
            c.ratios[(gear - 1) as usize] * c.final_drive
        }
    }
    fn shift(&mut self, c: Controls, gear: u8) {
        if gear == self.state.gear {
            return;
        }
        if self.wheel * Self::ratio(c, gear) > self.maximum * 0.98 {
            self.state.shift_rejected = true;
            return;
        }
        self.state.shift_rejected = false;
        self.state.gear = gear;
        self.shift_left = 0.32;
        self.shift_cooldown = 0.9;
    }
    /// Controls have been validated at the transport boundary. No allocation.
    pub fn step(&mut self, c: Controls) -> State {
        let pending_safe = self.state.shift_rejected
            && c.gear != self.state.gear
            && self.wheel * Self::ratio(c, c.gear) <= self.maximum * 0.98;
        if !c.automatic && (c.gear != self.requested_gear || self.was_auto || pending_safe) {
            self.shift(c, c.gear);
        }
        if c.automatic && !self.was_auto && self.state.gear == 0 {
            self.shift(c, 1);
        }
        self.requested_gear = c.gear;
        self.was_auto = c.automatic;
        self.shift_left = (self.shift_left - STEP).max(0.);
        self.shift_cooldown = (self.shift_cooldown - STEP).max(0.);
        let ratio = Self::ratio(c, self.state.gear);
        if c.automatic && self.shift_cooldown == 0. {
            let coupled = self.wheel * ratio;
            if self.omega > self.maximum * 0.87 && coupled > self.omega * 0.8 && self.state.gear < 6
            {
                self.shift(c, self.state.gear + 1);
            } else if self.omega < self.maximum * 0.34 && self.state.gear > 1 {
                let lower = self.state.gear - 1;
                if self.wheel * Self::ratio(c, lower) < self.maximum * 0.68 {
                    self.shift(c, lower);
                }
            }
        }
        let ratio = Self::ratio(c, self.state.gear);
        let shift_open = (self.shift_left / 0.2).clamp(0., 1.);
        self.throttle += (c.throttle * (1. - shift_open * 0.95) - self.throttle) * STEP / 0.06;
        let position =
            ((self.omega - self.idle) / (self.maximum - self.idle).max(1.)).clamp(0., 1.);
        let available = c.peak_torque_nm * (0.65 + 0.35 * (std::f32::consts::PI * position).sin());
        let drag = c.peak_torque_nm * (0.045 + 0.055 * position);
        let idle_throttle =
            (drag / available + (self.idle * 1.015 - self.omega) * 0.012).clamp(0., 0.3);
        let limiter = ((self.maximum - self.omega) / (self.maximum * 0.035).max(1.)).clamp(0., 1.);
        let combustion = available * self.throttle.max(idle_throttle) * limiter;
        let free_engine = self.omega + (combustion - drag) / c.inertia * STEP;
        let speed = self.wheel * c.wheel_radius;
        let grade = c.grade_percent / (10000. + c.grade_percent * c.grade_percent).sqrt();
        let road_force = c.mass_kg * 9.81 * (0.015 + grade) + 0.5 * 1.225 * 0.65 * speed * speed;
        let resistance =
            c.resistance_nm + (road_force + c.brake * c.mass_kg * 9.81 * 0.8) * c.wheel_radius;
        let wheel_inertia = c.mass_kg * c.wheel_radius * c.wheel_radius;
        let free_wheel = self.wheel - resistance / wheel_inertia * STEP;
        // Automatic launch clutch and anti-stall disengagement. Impulse transfer
        // equalizes shaft speeds when within capacity, dissipating slip energy.
        let engagement = ((self.omega - self.idle) / (self.idle * 0.55).max(1.)).clamp(0., 1.)
            * (1. - shift_open);
        let capacity = c.peak_torque_nm * 1.5 * engagement;
        let impulse = if ratio > 0. {
            ((free_engine - free_wheel * ratio) / (1. / c.inertia + ratio * ratio / wheel_inertia))
                .clamp(-capacity * STEP, capacity * STEP)
        } else {
            0.
        };
        self.omega = (free_engine - impulse / c.inertia).clamp(self.idle, self.maximum);
        self.wheel = (free_wheel + impulse * ratio / wheel_inertia).max(0.);
        // Keep road speed compatible with the imported acoustic ceiling, including
        // live ratio edits. This virtual ceiling is not the car's actual redline.
        let overspeed = ratio > 0. && self.wheel * ratio > self.maximum;
        if overspeed {
            self.wheel = self.maximum / ratio;
        }
        self.state.rpm = self.omega * RPM_PER_RAD;
        self.state.load = (combustion / available).clamp(0., 1.);
        self.state.speed_kmh = self.wheel * c.wheel_radius * 3.6;
        self.state.wheel_torque = impulse / STEP * ratio;
        self.state.resisting_torque = resistance;
        self.state.shifting = self.shift_left > 0.;
        self.state.limited = limiter < 0.95 || overspeed;
        self.state
    }
}
