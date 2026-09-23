//! Standalone, recording-free four-stroke instrument. All allocation and topology
//! preparation happen in `new`; `next_sample` and `render_block` are callback-safe.
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const DEFAULT_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cycle {
    FourStroke,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cylinder {
    pub name: String,
    /// Crank degrees in [0, 720); zero is the beginning of the cycle.
    pub firing_deg: f32,
    pub bank: usize,
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bank {
    pub exhaust_length_m: f32,
    pub intake_length_m: f32,
    pub sound_speed_m_s: f32,
    /// Signed reflection coefficients, magnitude at most 0.8.
    pub exhaust_reflection: f32,
    pub intake_reflection: f32,
    pub loss: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calibration {
    /// Sound controls, not dimensions measured on an engine.
    pub pulse_ms: f32,
    pub event_variation: f32,
    pub residual: f32,
    pub block_hz: f32,
    pub block_decay_ms: f32,
    pub exhaust_level: f32,
    pub intake_level: f32,
    pub block_level: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub cycle: Cycle,
    pub cylinders: Vec<Cylinder>,
    pub banks: Vec<Bank>,
    /// Opening offsets after the cylinder's firing reference, modulo 720 degrees.
    pub exhaust_open_after_deg: f32,
    pub intake_open_after_deg: f32,
    pub calibration: Calibration,
    pub seed: u64,
}

impl Config {
    pub fn even(cylinders: usize) -> Self {
        let count = cylinders.clamp(1, 12);
        Self {
            version: 1,
            cycle: Cycle::FourStroke,
            cylinders: (0..count)
                .map(|i| Cylinder {
                    name: format!("C{}", i + 1),
                    firing_deg: i as f32 * 720. / count as f32,
                    bank: 0,
                    strength: 1.,
                })
                .collect(),
            banks: vec![Bank {
                exhaust_length_m: 1.6,
                intake_length_m: 0.7,
                sound_speed_m_s: 420.,
                exhaust_reflection: -0.42,
                intake_reflection: 0.28,
                loss: 0.68,
            }],
            exhaust_open_after_deg: 140.,
            intake_open_after_deg: 500.,
            calibration: Calibration {
                pulse_ms: 2.2,
                event_variation: 0.08,
                residual: 0.,
                block_hz: 125.,
                block_decay_ms: 12.,
                exhaust_level: 0.7,
                intake_level: 0.3,
                block_level: 0.025,
            },
            seed: 0xB355,
        }
    }

    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "single" => Some(Self::even(1)),
            "four-even" => Some(Self::even(4)),
            "four-split" => {
                let mut c = Self::even(4);
                c.banks.push(Bank {
                    exhaust_length_m: 2.4,
                    intake_length_m: 1.1,
                    sound_speed_m_s: 420.,
                    exhaust_reflection: -0.32,
                    intake_reflection: 0.24,
                    loss: 0.7,
                });
                c.cylinders[1].bank = 1;
                c.cylinders[3].bank = 1;
                c.cylinders[1].firing_deg = 205.;
                c.cylinders[2].firing_deg = 360.;
                c.cylinders[3].firing_deg = 565.;
                Some(c)
            }
            _ => None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported standalone configuration version".into());
        }
        if !(1..=12).contains(&self.cylinders.len()) || !(1..=4).contains(&self.banks.len()) {
            return Err("expected 1–12 cylinders and 1–4 banks".into());
        }
        for (name, value, lo, hi) in [
            ("exhaust opening", self.exhaust_open_after_deg, 0., 720.),
            ("intake opening", self.intake_open_after_deg, 0., 720.),
            ("pulse width", self.calibration.pulse_ms, 0.5, 5.),
            (
                "event variation",
                self.calibration.event_variation,
                0.,
                0.25,
            ),
            ("residual", self.calibration.residual, 0., 0.5),
            ("block resonance", self.calibration.block_hz, 50., 2_000.),
            ("block decay", self.calibration.block_decay_ms, 10., 300.),
            ("exhaust level", self.calibration.exhaust_level, 0., 1.),
            ("intake level", self.calibration.intake_level, 0., 1.),
            ("block level", self.calibration.block_level, 0., 1.),
        ] {
            if !value.is_finite() || value < lo || value > hi || (hi == 720. && value == hi) {
                return Err(format!("invalid {name}"));
            }
        }
        for cylinder in &self.cylinders {
            if !cylinder.firing_deg.is_finite()
                || !(0.0..720.0).contains(&cylinder.firing_deg)
                || cylinder.bank >= self.banks.len()
                || !cylinder.strength.is_finite()
                || !(0.1..=2.).contains(&cylinder.strength)
                || cylinder.name.trim().is_empty()
            {
                return Err("invalid cylinder phase, bank, strength or name".into());
            }
        }
        for bank in &self.banks {
            for value in [bank.exhaust_length_m, bank.intake_length_m] {
                if !value.is_finite() || !(0.1..=6.).contains(&value) {
                    return Err("duct length must be 0.1–6 m".into());
                }
            }
            if !bank.sound_speed_m_s.is_finite()
                || !(250.0..=700.0).contains(&bank.sound_speed_m_s)
                || !bank.loss.is_finite()
                || !(0.0..=0.85).contains(&bank.loss)
                || !bank.exhaust_reflection.is_finite()
                || bank.exhaust_reflection.abs() > 0.8
                || !bank.intake_reflection.is_finite()
                || bank.intake_reflection.abs() > 0.8
            {
                return Err("invalid duct speed, loss or reflection".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombustionState {
    Firing,
    Motoring,
    FuelCut,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commands {
    pub rpm: f32,
    pub load: f32,
    pub volume: f32,
    pub combustion: CombustionState,
}

impl Commands {
    pub fn validate(self) -> Result<(), String> {
        if !self.rpm.is_finite()
            || !(0.0..=12_000.).contains(&self.rpm)
            || !self.load.is_finite()
            || !(0.0..=1.).contains(&self.load)
            || !self.volume.is_finite()
            || !(0.0..=1.).contains(&self.volume)
        {
            return Err("invalid rpm, load or volume".into());
        }
        Ok(())
    }
}

impl Default for Commands {
    fn default() -> Self {
        Self {
            rpm: 3_000.,
            load: 0.65,
            volume: 0.6,
            combustion: CombustionState::Firing,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventCounts {
    pub combustion: u64,
    pub exhaust: u64,
    pub intake: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    pub exhaust: f32,
    pub intake: f32,
    pub block: f32,
}
impl Levels {
    pub fn validate(self) -> Result<(), String> {
        if [self.exhaust, self.intake, self.block]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            Ok(())
        } else {
            Err("source levels must be 0–1".into())
        }
    }
}

struct Pulse {
    pos: f32,
    width: f32,
    gain: f32,
}
impl Pulse {
    fn new() -> Self {
        Self {
            pos: f32::INFINITY,
            width: 1.,
            gain: 0.,
        }
    }
    fn trigger(&mut self, fraction: f32, width: f32, gain: f32) {
        self.pos = fraction.clamp(0., 1.);
        self.width = width;
        self.gain = gain;
    }
    fn next(&mut self) -> f32 {
        if self.pos >= self.width {
            return 0.;
        }
        let phase = (self.pos / self.width).clamp(0., 1.);
        self.pos += 1.;
        self.gain * (PI * phase).sin().powi(2)
    }
}

struct CylinderState {
    firing: Pulse,
    exhaust: Pulse,
    intake: Pulse,
}

struct Duct {
    samples: Vec<f32>,
    position: usize,
    delay: f32,
    feedback: f32,
    loss: f32,
}
impl Duct {
    fn new(rate: u32, length: f32, speed: f32, reflection: f32, loss: f32) -> Self {
        let delay = rate as f32 * length / speed;
        Self {
            samples: vec![0.; delay.ceil() as usize + 3],
            position: 0,
            delay,
            feedback: reflection * loss,
            loss,
        }
    }
    fn next(&mut self, input: f32) -> f32 {
        let len = self.samples.len() as f32;
        let read = (self.position as f32 - self.delay).rem_euclid(len);
        let first = read.floor() as usize;
        let fraction = read.fract();
        let delayed = self.samples[first] * (1. - fraction)
            + self.samples[(first + 1) % self.samples.len()] * fraction;
        self.samples[self.position] = input + self.feedback * delayed;
        self.position = (self.position + 1) % self.samples.len();
        delayed * self.loss
    }
}

struct BankState {
    exhaust: Duct,
    intake: Duct,
}

struct Resonator {
    a1: f32,
    a2: f32,
    y1: f32,
    y2: f32,
}
impl Resonator {
    fn new(rate: u32, hz: f32, decay_ms: f32) -> Self {
        let radius = (-1. / (rate as f32 * decay_ms * 0.001)).exp();
        Self {
            a1: 2. * radius * (2. * PI * hz / rate as f32).cos(),
            a2: -radius * radius,
            y1: 0.,
            y2: 0.,
        }
    }
    fn next(&mut self, input: f32) -> f32 {
        let y = self.a1 * self.y1 + self.a2 * self.y2 + input;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Owns the prepared topology; command changes never rebuild it.
pub struct Synth {
    config: Config,
    rate: u32,
    control_smooth: f32,
    dc_radius: f32,
    target: Commands,
    rpm: f32,
    load: f32,
    volume: f32,
    levels: Levels,
    target_levels: Levels,
    cycle: f64,
    cylinders: Vec<CylinderState>,
    banks: Vec<BankState>,
    block: Resonator,
    residual_lp: f32,
    dc_input: f32,
    dc_output: f32,
    rng: u64,
    counts: EventCounts,
}
impl Synth {
    pub fn new(rate: u32, config: Config, commands: Commands) -> Result<Self, String> {
        config.validate()?;
        commands.validate()?;
        if !(8_000..=192_000).contains(&rate) {
            return Err("sample rate must be 8–192 kHz".into());
        }
        let cylinders = (0..config.cylinders.len())
            .map(|_| CylinderState {
                firing: Pulse::new(),
                exhaust: Pulse::new(),
                intake: Pulse::new(),
            })
            .collect();
        let banks = config
            .banks
            .iter()
            .map(|b| BankState {
                exhaust: Duct::new(
                    rate,
                    b.exhaust_length_m,
                    b.sound_speed_m_s,
                    b.exhaust_reflection,
                    b.loss,
                ),
                intake: Duct::new(
                    rate,
                    b.intake_length_m,
                    b.sound_speed_m_s,
                    b.intake_reflection,
                    b.loss,
                ),
            })
            .collect();
        let block = Resonator::new(
            rate,
            config.calibration.block_hz,
            config.calibration.block_decay_ms,
        );
        let levels = Levels {
            exhaust: config.calibration.exhaust_level,
            intake: config.calibration.intake_level,
            block: config.calibration.block_level,
        };
        let rng = config.seed.max(1);
        Ok(Self {
            config,
            rate,
            control_smooth: 1. - (-1. / (rate as f32 * 0.025)).exp(),
            dc_radius: (-2. * PI * 25. / rate as f32).exp(),
            target: commands,
            rpm: commands.rpm,
            load: commands.load,
            volume: commands.volume,
            levels,
            target_levels: levels,
            cycle: 0.,
            cylinders,
            banks,
            block,
            residual_lp: 0.,
            dc_input: 0.,
            dc_output: 0.,
            rng,
            counts: EventCounts::default(),
        })
    }
    pub fn set_commands(&mut self, commands: Commands) -> Result<(), String> {
        commands.validate()?;
        self.target = commands;
        Ok(())
    }
    pub fn counts(&self) -> EventCounts {
        self.counts
    }
    pub fn set_levels(&mut self, levels: Levels) -> Result<(), String> {
        levels.validate()?;
        self.target_levels = levels;
        Ok(())
    }
    pub fn cycle_position(&self) -> f64 {
        self.cycle
    }
    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u32 << 24) as f32 * 2. - 1.
    }
    fn crossing(start: f64, end: f64, offset: f64) -> Option<f32> {
        // Floating accumulation can place an exact block-end event a few ulps
        // before the boundary. Assign it to the following sample consistently.
        const EPS: f64 = 1e-8;
        let event = (start - offset - EPS).ceil() + offset;
        if event >= start - EPS && event < end - EPS {
            Some(((event - start) / (end - start)) as f32)
        } else {
            None
        }
    }
    pub fn next_sample(&mut self) -> f32 {
        let smooth = self.control_smooth;
        // An explicit zero command freezes phase immediately; existing paths keep decaying.
        self.rpm = if self.target.rpm == 0. {
            0.
        } else {
            self.rpm + (self.target.rpm - self.rpm) * smooth
        };
        self.load += (self.target.load - self.load) * smooth;
        self.volume += (self.target.volume - self.volume) * smooth;
        self.levels.exhaust += (self.target_levels.exhaust - self.levels.exhaust) * smooth;
        self.levels.intake += (self.target_levels.intake - self.levels.intake) * smooth;
        self.levels.block += (self.target_levels.block - self.levels.block) * smooth;
        let next_cycle = self.cycle + self.rpm as f64 / (120. * self.rate as f64);
        let width = (self.config.calibration.pulse_ms * self.rate as f32 * 0.001).max(8.);
        if next_cycle > self.cycle {
            for i in 0..self.config.cylinders.len() {
                let firing_deg = self.config.cylinders[i].firing_deg;
                let strength = self.config.cylinders[i].strength;
                let base = firing_deg as f64 / 720.;
                let exhaust =
                    ((firing_deg + self.config.exhaust_open_after_deg) % 720.) as f64 / 720.;
                let intake =
                    ((firing_deg + self.config.intake_open_after_deg) % 720.) as f64 / 720.;
                if let Some(f) = Self::crossing(self.cycle, next_cycle, base) {
                    match self.target.combustion {
                        CombustionState::Firing => {
                            let random = self.random();
                            let gain =
                                strength * (1. + random * self.config.calibration.event_variation);
                            self.cylinders[i].firing.trigger(f, width, gain);
                            self.counts.combustion += 1;
                        }
                        CombustionState::Motoring => {
                            // Compression/mechanical excitation without a fired charge.
                            self.cylinders[i]
                                .firing
                                .trigger(f, width * 1.5, strength * 0.12);
                        }
                        CombustionState::FuelCut => {}
                    }
                }
                if let Some(f) = Self::crossing(self.cycle, next_cycle, exhaust) {
                    self.cylinders[i].exhaust.trigger(f, width * 1.5, strength);
                    self.counts.exhaust += 1;
                }
                if let Some(f) = Self::crossing(self.cycle, next_cycle, intake) {
                    self.cylinders[i].intake.trigger(f, width * 1.8, strength);
                    self.counts.intake += 1;
                }
            }
        }
        self.cycle = next_cycle;
        let mut bank_exhaust = [0.; 4];
        let mut bank_intake = [0.; 4];
        let mut block_input = 0.;
        for (i, state) in self.cylinders.iter_mut().enumerate() {
            let cylinder = &self.config.cylinders[i];
            let firing = state.firing.next();
            let exhaust = state.exhaust.next();
            let intake = state.intake.next();
            let combustion = if self.target.combustion == CombustionState::Firing {
                firing
            } else {
                0.
            };
            bank_exhaust[cylinder.bank] +=
                exhaust * (0.08 + self.load * 0.09) + combustion * (0.15 + self.load * 0.22);
            bank_intake[cylinder.bank] += intake * (0.06 + self.load * 0.11);
            block_input += firing * 0.00012 + exhaust * 0.000016;
        }
        // Optional roughness is carried by the event itself. Filtering the
        // random component before the duct prevents a continuous output hiss.
        let random = self.random();
        self.residual_lp +=
            (random - self.residual_lp) * (2. * PI * 1_500. / self.rate as f32).min(0.8);
        let roughness = self.residual_lp * self.config.calibration.residual;
        let mut exhaust = 0.;
        let mut intake = 0.;
        for (i, bank) in self.banks.iter_mut().enumerate() {
            exhaust += bank.exhaust.next(bank_exhaust[i] * (1. + roughness));
            intake += bank.intake.next(bank_intake[i]);
        }
        let block = self.block.next(block_input);
        let mixed =
            self.levels.exhaust * exhaust + self.levels.intake * intake + self.levels.block * block;
        let ac = mixed - self.dc_input + self.dc_radius * self.dc_output;
        self.dc_input = mixed;
        self.dc_output = ac;
        let output = self.volume * ac;
        // Emergency ceiling only; preset ranges should remain below it.
        if output.is_finite() {
            output.clamp(-0.98, 0.98)
        } else {
            0.
        }
    }
    pub fn render_block(&mut self, output: &mut [f32]) {
        for sample in output {
            *sample = self.next_sample();
        }
    }
}
