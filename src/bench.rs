//! Shared transport for real-time listening and offline renders.
use crate::{
    bank::Bank,
    drive::{Controls, Mode, Simulator, State, TICK_RATE},
    hybrid::{Hybrid, Settings, audition},
    project::Parameters,
};
use std::sync::Arc;

pub struct Bench {
    engine: Hybrid,
    sim: Simulator,
    params: Parameters,
    settings: Settings,
    controls: Controls,
    rate: u32,
    max: f32,
    frames: u64,
    physics_accumulator: u32,
    reset_token: u64,
    cycle_seconds: f32,
}
impl Bench {
    pub fn new(
        rate: u32,
        mut params: Parameters,
        settings: Settings,
        controls: Controls,
        bank: Option<Arc<Bank>>,
    ) -> Self {
        let rate = rate.max(8000);
        let (min, max) = bank
            .as_ref()
            .map(|b| (b.min_rpm, b.max_rpm))
            .unwrap_or((300., 8000.));
        if controls.mode == Mode::Simulated {
            params.rpm = min;
            params.load = 0.1;
        }
        Self {
            engine: Hybrid::new(rate, params, settings, bank),
            sim: Simulator::new(min, max, controls),
            params,
            settings,
            controls,
            rate,
            max,
            frames: 0,
            physics_accumulator: rate,
            reset_token: 0,
            cycle_seconds: 16.,
        }
    }
    pub fn set_cycle_seconds(&mut self, seconds: f32) {
        self.cycle_seconds = seconds;
    }
    pub fn set(&mut self, p: Parameters, h: Settings, c: Controls, reset_token: u64) {
        if p.validate().is_err() || h.validate().is_err() || c.validate().is_err() {
            return;
        }
        if c.mode != self.controls.mode || reset_token != self.reset_token {
            self.sim.reset(c);
            self.frames = 0;
            self.physics_accumulator = self.rate;
        }
        self.params = p;
        self.settings = h;
        self.controls = c;
        self.reset_token = reset_token;
        if c.mode == Mode::Direct {
            self.engine.set(p, h);
        }
    }
    pub fn next(&mut self, playing: bool) -> f32 {
        if playing {
            if self.physics_accumulator >= self.rate {
                self.physics_accumulator -= self.rate;
                let p = match self.controls.mode {
                    Mode::Direct => self.params,
                    Mode::Simulated => {
                        let s = self.sim.step(self.controls);
                        Parameters {
                            rpm: s.rpm,
                            load: s.load,
                            ..self.params
                        }
                    }
                    Mode::Cycle => audition(
                        (self.frames as f32 / self.rate as f32 * 16. / self.cycle_seconds) % 16.,
                        self.params,
                        self.max,
                    ),
                };
                self.engine.set(p, self.settings);
            }
            self.physics_accumulator += TICK_RATE;
            self.frames += 1;
        }
        self.engine.next(playing)
    }
    pub fn state(&self) -> State {
        let mut state = if self.controls.mode == Mode::Simulated {
            self.sim.state()
        } else {
            State::default()
        };
        state.rpm = self.engine.rpm();
        state.load = self.engine.load();
        state
    }
}
