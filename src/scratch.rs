//! Engines designed from scratch, without an Automation sound bank.
//!
//! Two recording-free voices share the bench, driving model and WAV export:
//! the experimental descriptor voice, fed a descriptor grid generated from the
//! controls below instead of one measured from a ZIP, and the standalone
//! four-stroke event synth. Neither claims to model a specific real engine.
use crate::{
    drive::Controls,
    engine_build::{Aspiration, EngineBuild, Fuel, Head, Headers, Muffler, Throttle},
    hybrid::Settings,
    procedural::ProceduralBank,
    project::Parameters,
    standalone,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const BANDS: usize = 7;
/// Broad band centres shown for the advanced tone controls (Hz).
pub const BAND_LABELS: [&str; BANDS] = ["<80", "80–200", "200–500", "0.5–1.2k", "1.2–2.5k", "2.5–5k", ">5k"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScratchEngine {
    Experimental,
    Standalone,
}

/// Macro controls that generate the experimental voice's RPM × load grid.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExperimentalSpec {
    /// AC RMS at idle, off-load, on the bank's 0.65-peak processing scale.
    pub level: f32,
    pub rpm_rise_db: f32,
    pub load_rise_db: f32,
    /// Share of energy below the firing fundamental's upper bands; longer pressure release.
    pub body: f32,
    /// Narrower pressure pulses: more upper harmonics.
    pub sharpness: f32,
    /// Spectral tilt of both tone and flow noise, −1 dark to +1 bright.
    pub brightness: f32,
    /// Upper-band emphasis that grows with load.
    pub rasp: f32,
    /// Share of the level carried by periodic pulses rather than noise.
    pub tonal: f32,
    pub flow: f32,
    pub crackle: f32,
    pub variation: f32,
    /// Advanced per-band offsets in dB, `[off-load, full-load][band]`.
    pub tone_db: [[f32; BANDS]; 2],
    pub noise_db: [[f32; BANDS]; 2],
    /// Valve overlap, 0 mild to 1 race: idle instability, reversion puffs.
    pub cam: f32,
    /// COV of combustion strength at hot idle (production cars: 0.06–0.12).
    pub idle_cov: f32,
    pub direct_injection: bool,
    /// Lift-off pops and bangs, 0 = off. Never set by the engine builder.
    pub afterfire: f32,
}

impl Default for ExperimentalSpec {
    fn default() -> Self {
        Self {
            level: 0.06,
            rpm_rise_db: 4.,
            load_rise_db: 6.,
            body: 0.55,
            sharpness: 0.4,
            brightness: 0.,
            rasp: 0.35,
            tonal: 0.8,
            flow: 0.3,
            crackle: 0.25,
            variation: 0.08,
            tone_db: [[0.; BANDS]; 2],
            noise_db: [[0.; BANDS]; 2],
            cam: 0.3,
            idle_cov: 0.07,
            direct_injection: false,
            afterfire: 0.,
        }
    }
}

impl ExperimentalSpec {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, lo, hi) in [
            ("level", self.level, 0.01, 0.3),
            ("RPM rise", self.rpm_rise_db, -6., 12.),
            ("load rise", self.load_rise_db, 0., 12.),
            ("body", self.body, 0., 1.),
            ("sharpness", self.sharpness, 0., 1.),
            ("brightness", self.brightness, -1., 1.),
            ("rasp", self.rasp, 0., 1.),
            ("tonal share", self.tonal, 0.25, 1.),
            ("flow", self.flow, 0., 1.),
            ("crackle", self.crackle, 0., 1.),
            ("cycle variation", self.variation, 0., 0.25),
            ("cam overlap", self.cam, 0., 1.),
            ("idle combustion COV", self.idle_cov, 0.01, 0.2),
            ("afterfire", self.afterfire, 0., 1.),
        ] {
            if !value.is_finite() || value < lo || value > hi {
                return Err(format!("Scratch {name} out of range"));
            }
        }
        if self
            .tone_db
            .iter()
            .chain(&self.noise_db)
            .flatten()
            .any(|db| !db.is_finite() || db.abs() > 12.)
        {
            return Err("Scratch band offsets must be within ±12 dB".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    Inline,
    V,
    /// Horizontally opposed (boxer).
    Flat,
}

/// Crank used by the preset generators only; a design stores its pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crank {
    /// Every event 720/n degrees apart, in the firing order.
    Even,
    /// V engines whose opposite cylinders share a crankpin: pairs fire the
    /// bank angle apart (V6 90° odd-fire: 90/150°; V-twin 45°: 315/405°).
    SharedPin,
}

/// How the engine is built, cylinder by cylinder. Nothing forces it to be a
/// working engine: pins, banks and firing order are free, repeats and missing
/// cylinders included.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineDesign {
    pub layout: Layout,
    pub cylinders: u32,
    /// V angle: bank-2 pistons reach top dead centre this much later.
    pub bank_angle: f32,
    /// Crankpin angle of each cylinder, degrees in [0, 360).
    pub pins: [f32; 12],
    /// Bank of each cylinder (0 or 1).
    pub banks: [u8; 12],
    /// Firing sequence of 1-based cylinder numbers; `order_len` are used.
    pub firing_order: [u8; 12],
    pub order_len: u8,
    /// Level of bank 2 relative to bank 1.
    pub bank_gain_db: f32,
    /// Extra header delay on bank 2: unequal-length rumble.
    pub bank_delay_ms: f32,
}

impl Default for EngineDesign {
    fn default() -> Self {
        PRESETS[3].1
    }
}

const fn design(layout: Layout, cylinders: u32, bank_angle: f32, crank: Crank, order: &[u8]) -> EngineDesign {
    let n = cylinders as usize;
    let mut firing_order = [0; 12];
    let mut banks = [0; 12];
    let mut pins = [0.; 12];
    let pairs = if n / 2 > 1 { n / 2 } else { 1 };
    let mut k = 0;
    while k < order.len() {
        firing_order[k] = order[k];
        let i = order[k] as usize - 1;
        // Odd-numbered cylinders on bank 1, even on bank 2.
        let bank = if matches!(layout, Layout::Inline) { 0 } else { (i % 2) as u8 };
        banks[i] = bank;
        let angle = match crank {
            Crank::Even => k as f32 * 720. / n as f32,
            Crank::SharedPin => {
                let offset = bank_angle + if pairs == 1 { 360. } else { 0. };
                (k / 2) as f32 * 720. / pairs as f32 + (k % 2) as f32 * offset
            }
        };
        // Top dead centre = pin + bank offset; keep the pin in [0, 360).
        let mut pin = angle - if bank == 1 { bank_angle } else { 0. };
        while pin < 0. {
            pin += 360.;
        }
        while pin >= 360. {
            pin -= 360.;
        }
        pins[i] = pin;
        k += 1;
    }
    EngineDesign {
        layout,
        cylinders,
        bank_angle,
        pins,
        banks,
        firing_order,
        order_len: order.len() as u8,
        bank_gain_db: 0.,
        // Real headers are rarely equal-length: this offset is what separates
        // a cross-plane V8's burble or a boxer's rumble from a flat-plane scream.
        bank_delay_ms: match layout {
            Inline => 0.,
            V => 0.6,
            Flat => 1.,
        },
    }
}

use Crank::{Even, SharedPin};
use Layout::{Flat, Inline, V};
/// Common layouts. Orders are typical examples, not a specific manufacturer's engine.
pub const PRESETS: [(&str, EngineDesign); 14] = [
    ("Single", design(Inline, 1, 0., Even, &[1])),
    ("V-twin 45°", design(V, 2, 45., SharedPin, &[1, 2])),
    ("Inline-3", design(Inline, 3, 0., Even, &[1, 2, 3])),
    ("Inline-4", design(Inline, 4, 0., Even, &[1, 3, 4, 2])),
    ("Flat-4 boxer", design(Flat, 4, 180., Even, &[1, 3, 2, 4])),
    ("Inline-5", design(Inline, 5, 0., Even, &[1, 2, 4, 5, 3])),
    ("Inline-6", design(Inline, 6, 0., Even, &[1, 5, 3, 6, 2, 4])),
    ("V6 60°", design(V, 6, 60., Even, &[1, 2, 3, 4, 5, 6])),
    ("V6 90° odd-fire", design(V, 6, 90., SharedPin, &[1, 2, 3, 4, 5, 6])),
    ("Flat-6", design(Flat, 6, 180., Even, &[1, 6, 2, 4, 3, 5])),
    ("V8 cross-plane", design(V, 8, 90., Even, &[1, 8, 4, 3, 6, 5, 7, 2])),
    ("V8 flat-plane", design(V, 8, 90., Even, &[1, 2, 3, 4, 5, 6, 7, 8])),
    ("V10 72°", design(V, 10, 72., Even, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10])),
    ("V12 60°", design(V, 12, 60., Even, &[1, 12, 5, 8, 3, 10, 6, 7, 2, 11, 4, 9])),
];

/// Firing events resolved from an `EngineDesign`, in firing-order sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Firing {
    pub events: usize,
    /// Crank degrees in [0, 720) of event `k`.
    pub angles: [f32; 12],
    /// Cylinder index (0-based) of event `k`.
    pub cylinder: [u8; 12],
    /// Bank of each cylinder index.
    pub banks: [u8; 12],
    pub bank_gain: [f32; 2],
    pub bank_delay_ms: [f32; 2],
}

impl EngineDesign {
    pub fn order(&self) -> &[u8] {
        &self.firing_order[..(self.order_len as usize).clamp(1, 12)]
    }
    /// Top dead centre of cylinder `i` within one crank revolution.
    pub fn tdc(&self, i: usize) -> f32 {
        let offset = if self.banks[i] == 1 { self.bank_angle } else { 0. };
        (self.pins[i] + offset).rem_euclid(360.)
    }
    /// Change the count: an evenly firing inline-style crank, sequential order.
    pub fn set_cylinders(&mut self, cylinders: u32) {
        let cylinders = cylinders.clamp(1, 12);
        let order: Vec<u8> = (1..=cylinders as u8).collect();
        let layout = if cylinders < 2 { Inline } else { self.layout };
        let bank_angle = self.bank_angle;
        *self = EngineDesign {
            bank_gain_db: self.bank_gain_db,
            bank_delay_ms: self.bank_delay_ms,
            ..design(layout, cylinders, bank_angle, Even, &order)
        };
    }
    /// Only numeric sanity: a design may be mechanically nonsensical.
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=12).contains(&self.cylinders) {
            return Err("Cylinders: 1 to 12".into());
        }
        if !(1..=12).contains(&self.order_len) {
            return Err("The firing order needs 1 to 12 entries".into());
        }
        if self.order().iter().any(|&c| c == 0 || c as u32 > self.cylinders) {
            return Err(format!("Firing order entries must be cylinders 1–{}", self.cylinders));
        }
        if self.pins.iter().any(|p| !p.is_finite() || !(0.0..360.0).contains(p))
            || self.banks.iter().any(|&b| b > 1)
        {
            return Err("Pin angles must be 0–360° and banks 1 or 2".into());
        }
        for (name, value, lo, hi) in [
            ("bank angle", self.bank_angle, 0., 180.),
            ("bank level", self.bank_gain_db, -12., 12.),
            ("bank delay", self.bank_delay_ms, 0., 5.),
        ] {
            if !value.is_finite() || value < lo || value > hi {
                return Err(format!("Engine {name} out of range"));
            }
        }
        Ok(())
    }
    /// Walk the firing order. Each event can fire at either of its cylinder's
    /// two top dead centres per cycle (the cam chooses which); pick the one
    /// nearest an even spacing without going backwards.
    pub fn firing(&self) -> Firing {
        let order = self.order();
        let events = order.len();
        let mut angles = [0.; 12];
        let mut cylinder = [0; 12];
        let mut previous = f32::NEG_INFINITY;
        let start = self.tdc(order[0] as usize - 1);
        for (k, &number) in order.iter().enumerate() {
            let i = (number as usize).clamp(1, self.cylinders as usize) - 1;
            let target = start + k as f32 * 720. / events as f32;
            let mut best = f32::NAN;
            let base = self.tdc(i) + 720. * ((previous.max(start) - self.tdc(i)) / 720.).floor();
            for candidate in [base, base + 360., base + 720., base + 1080.] {
                if candidate + 1e-3 >= previous
                    && (best.is_nan() || (candidate - target).abs() < (best - target).abs())
                {
                    best = candidate;
                }
            }
            previous = best;
            angles[k] = (best - start).rem_euclid(720.);
            cylinder[k] = i as u8;
        }
        Firing {
            events,
            angles,
            cylinder,
            banks: self.banks,
            bank_gain: [1., 10f32.powf(self.bank_gain_db / 20.)],
            bank_delay_ms: [0., self.bank_delay_ms],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scratch {
    pub engine: ScratchEngine,
    #[serde(default)]
    pub design: EngineDesign,
    #[serde(default)]
    pub build: EngineBuild,
    pub idle_rpm: f32,
    pub redline_rpm: f32,
    #[serde(default)]
    pub experimental: ExperimentalSpec,
    #[serde(default = "default_standalone")]
    pub standalone: standalone::Config,
    /// Rotating inertia (kg·m²) that turns combustion scatter into crank-speed ripple.
    #[serde(default = "default_inertia")]
    pub inertia: f32,
}

fn default_inertia() -> f32 {
    0.22
}

/// Scratch-only engine behaviour the descriptor voice needs at run time:
/// combustion statistics, crank dynamics, valve events and overrun. Built
/// engines get their own cylinder imbalance from `seed`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Life {
    pub seed: u64,
    pub cylinders: u32,
    pub idle_rpm: f32,
    pub redline_rpm: f32,
    pub inertia: f32,
    pub idle_cov: f32,
    pub cam: f32,
    pub afterfire: f32,
    pub direct_injection: bool,
    pub exhaust: crate::acoustics::ExhaustLayout,
    /// Boost dump on lift-off; `None` for naturally aspirated engines.
    pub blow_off: Option<crate::engine_build::BlowOff>,
}

fn default_standalone() -> standalone::Config {
    standalone::Config::even(4)
}

impl Default for Scratch {
    fn default() -> Self {
        let mut scratch = Self {
            engine: ScratchEngine::Experimental,
            design: EngineDesign::default(),
            build: EngineBuild::default(),
            idle_rpm: 850.,
            redline_rpm: 7000.,
            experimental: ExperimentalSpec::default(),
            standalone: default_standalone(),
            inertia: default_inertia(),
        };
        scratch.apply_design();
        scratch
    }
}

impl Scratch {
    pub fn validate(&self) -> Result<(), String> {
        self.design.validate()?;
        self.build.validate()?;
        if self.standalone.cylinders.len() != self.design.order().len() {
            return Err("Standalone events do not match the engine design".into());
        }
        if !self.idle_rpm.is_finite()
            || !self.redline_rpm.is_finite()
            || !(300.0..=2000.0).contains(&self.idle_rpm)
            || !(self.idle_rpm + 1000.0..=12_000.0).contains(&self.redline_rpm)
        {
            return Err("Idle 300–2000 rpm; redline at least 1000 rpm above idle, up to 12000".into());
        }
        self.experimental.validate()?;
        self.standalone.validate()
    }
    /// Derive every sound and bench setting from the built engine, as a
    /// part change does in Automation. Manual fine-tuning after this call
    /// stays until the next part change.
    pub fn derive_from_build(
        &mut self,
        settings: &mut Settings,
        params: &mut Parameters,
        controls: &mut Controls,
    ) {
        use crate::engine_build::{BlockMaterial, Catalyst};
        let b = self.build;
        let perf = b.performance(self.design.cylinders);
        let cylinder = b.cylinder_litres();
        let turbo = b.aspiration != Aspiration::Natural;
        let cam = b.cam;
        self.idle_rpm = perf.idle_rpm;
        self.redline_rpm = perf.redline_rpm.max(perf.idle_rpm + 1000.);
        let (muffler_bright, chamber, absorption): (f32, f32, f32) = match b.muffler {
            Muffler::None => (0.35, 0.3, 0.05),
            Muffler::StraightThrough => (0.2, 2.5, 0.25),
            Muffler::Baffled => (0., 6., 0.4),
            Muffler::ReverseFlow => (-0.25, 10., 0.6),
        };
        let (cat_bright, cat_absorption): (f32, f32) = match b.catalyst {
            Catalyst::None => (0.1, 0.),
            Catalyst::Standard => (-0.05, 0.1),
            Catalyst::HighFlow => (0., 0.04),
        };
        let (header_length, header_bright, bank_delay_ms): (f32, f32, f32) = match b.headers {
            Headers::CastManifold => (0.3, -0.2, 0.9),
            Headers::Tubular => (0.55, 0., 0.6),
            // Equal runner lengths remove the bank-to-bank offset (no rumble).
            Headers::EqualLength => (0.75, 0.1, 0.),
        };
        // A turbine absorbs and smooths the blowdown pulses.
        let (turbo_gain, turbo_bright): (f32, f32) = match b.aspiration {
            Aspiration::Natural => (1., 0.),
            Aspiration::Turbo => (0.72, -0.3),
            Aspiration::TwinTurbo => (0.78, -0.25),
        };
        let open_exhaust = matches!(b.muffler, Muffler::None | Muffler::StraightThrough);
        let e = &mut self.experimental;
        // Bigger engines are louder, but the level saturates: bore and stroke are free.
        // The exhaust network itself makes an open system louder than a
        // muffled one; only the turbine's damping is added here.
        e.level = ((0.035 + 0.05 * (1. - (-perf.displacement_l / 4.).exp())) * turbo_gain)
            .clamp(0.01, 0.3);
        e.body = (0.25 + 0.45 * (cylinder / 0.6)).clamp(0., 1.);
        e.sharpness = (0.25 + 0.05 * (b.compression - 10.) + 0.25 * cam - 0.15 * turbo as u8 as f32)
            .clamp(0., 1.);
        e.brightness = (header_bright + muffler_bright + cat_bright + turbo_bright).clamp(-1., 1.);
        e.rasp = (0.2 + 0.3 * cam + if open_exhaust { 0.2 } else { 0. }).clamp(0., 1.);
        e.flow = (0.25 + 0.15 * turbo as u8 as f32 + if b.muffler == Muffler::None { 0.1 } else { 0. })
            .clamp(0., 1.);
        e.crackle = (0.15 + 0.3 * cam + if open_exhaust { 0.15 } else { 0. }).clamp(0., 1.);
        // Valve overlap makes low-speed combustion irregular: the lumpy race idle.
        e.variation = (0.05 + 0.13 * cam).clamp(0., 0.25);
        e.tonal = (0.85 - 0.1 * turbo as u8 as f32).clamp(0.25, 1.);
        e.cam = cam;
        // Production idle: COV 6–12 %; overlap and residual gas push it up.
        e.idle_cov = 0.06 + 0.06 * cam;
        e.direct_injection = b.fuel == Fuel::DirectInjection;
        if self.design.layout != Layout::Inline {
            self.design.bank_delay_ms = bank_delay_ms;
        }

        settings.header_length = header_length;
        settings.diameter = b.exhaust_mm.clamp(30., 130.);
        settings.chamber = chamber;
        settings.absorption = (absorption + cat_absorption).min(1.);
        settings.pipe = 0.35;
        settings.turbo = match b.aspiration {
            Aspiration::Natural => 0.,
            Aspiration::Turbo => 0.5,
            Aspiration::TwinTurbo => 0.4,
        };
        settings.roughness = (0.08 + 0.5 * cam).min(1.);
        settings.cycle_life = (0.35 + 0.4 * cam).min(1.);
        settings.overrun = (0.1 + 0.3 * cam + if b.catalyst == Catalyst::None { 0.2 } else { 0. }).min(1.);
        settings.fuel_cut = 0.5;
        self.inertia = perf.inertia.clamp(0.05, 2.);
        let itb = b.throttle == Throttle::Individual;
        settings.intake_length = if itb { 0.18 } else { 0.38 };
        settings.airbox = if itb { 0.7 } else if b.fuel == Fuel::Carburettor { 0.5 } else { 0.3 };
        settings.texture = if itb { 0.45 } else if b.fuel == Fuel::Carburettor { 0.35 } else { 0.2 };
        settings.attack = (0.35 + 0.03 * (b.compression - 10.)).clamp(0., 1.);

        params.cylinders = self.design.cylinders;
        params.exhaust = 1.;
        let intake: f32 = if itb { 0.55 } else if b.fuel == Fuel::Carburettor { 0.4 } else { 0.25 };
        params.intake = (intake - if turbo { 0.1 } else { 0. }).clamp(0., 1.);
        let valvetrain: f32 = match b.head {
            Head::Pushrod => 0.2,
            Head::Sohc => 0.14,
            Head::Dohc => 0.1,
        };
        // Direct injectors tick audibly at idle. A cast-iron block is heavier
        // and better damped: its structure-borne mechanical noise is lower.
        let block = if b.block == BlockMaterial::CastIron { 0.85 } else { 1. };
        params.mechanical =
            ((valvetrain + if b.fuel == Fuel::DirectInjection { 0.08 } else { 0. }) * block)
                .clamp(0., 1.);
        params.brightness = 10000.;
        params.pipe_length = if open_exhaust { 1.2 } else { 1.8 };

        controls.peak_torque_nm = perf.peak_torque_nm.clamp(30., 2000.);
        controls.inertia = perf.inertia.clamp(0.1, 2.);

        let c = &mut self.standalone.calibration;
        c.pulse_ms = (1.4 + cylinder * 1.2).clamp(0.5, 5.);
        c.event_variation = self.experimental.variation;
        c.residual = (0.05 + 0.15 * cam).min(0.5);
        c.block_hz = match b.block {
            BlockMaterial::CastIron => 110.,
            BlockMaterial::Aluminium => 150.,
        };
        c.block_level = if b.head == Head::Pushrod { 0.035 } else { 0.022 };
        c.intake_level = if itb { 0.45 } else { 0.3 };
        for bank in &mut self.standalone.banks {
            bank.exhaust_length_m = (header_length + params.pipe_length).clamp(0.1, 6.);
        }
        self.apply_design();
    }
    pub fn life(&self) -> Life {
        // FNV-1a over the engine's construction: same parts, same imbalance.
        let key = serde_json::to_string(&(self.design, self.build)).unwrap_or_default();
        let seed = key.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
        });
        Life {
            seed: seed | 1,
            cylinders: self.design.cylinders,
            idle_rpm: self.idle_rpm,
            redline_rpm: self.redline_rpm,
            inertia: self.inertia.clamp(0.05, 2.),
            idle_cov: self.experimental.idle_cov,
            cam: self.experimental.cam,
            afterfire: self.experimental.afterfire,
            blow_off: (self.build.aspiration != Aspiration::Natural).then_some(self.build.blow_off),
            exhaust: crate::acoustics::ExhaustLayout {
                catalyst: self.build.catalyst != crate::engine_build::Catalyst::None,
                muffler: match self.build.muffler {
                    Muffler::None => 0,
                    Muffler::StraightThrough => 1,
                    Muffler::Baffled => 2,
                    Muffler::ReverseFlow => 3,
                },
            },
            direct_injection: self.experimental.direct_injection,
        }
    }
    pub fn cylinders(&self) -> u32 {
        self.design.cylinders
    }
    /// Rewrite the standalone cylinders and banks from the engine design,
    /// keeping bank 1's ducts, the calibration and valve timing.
    pub fn apply_design(&mut self) {
        let firing = self.design.firing();
        let config = &mut self.standalone;
        let first = config.banks[0].clone();
        let mut second = first.clone();
        // Bank 2's extra header delay becomes duct length at the same speed of sound.
        second.exhaust_length_m = (first.exhaust_length_m
            + firing.bank_delay_ms[1] * 0.001 * first.sound_speed_m_s)
            .min(6.);
        let two_banks = self.design.banks[..self.design.cylinders as usize].contains(&1);
        config.banks = if two_banks { vec![first, second] } else { vec![first] };
        // One explicit event per firing-order entry: repeats fire twice,
        // cylinders left out of the order never fire.
        config.cylinders = (0..firing.events)
            .map(|k| {
                let cylinder = usize::from(firing.cylinder[k]);
                let bank = usize::from(firing.banks[cylinder]);
                standalone::Cylinder {
                    name: format!("C{}", cylinder + 1),
                    firing_deg: firing.angles[k],
                    bank: bank.min(config.banks.len() - 1),
                    strength: firing.bank_gain[bank].clamp(0.1, 2.),
                }
            })
            .collect();
    }
}

/// Something the audio thread displaced, to be dropped elsewhere.
pub enum ScratchVoice {
    Descriptors(Arc<ProceduralBank>),
    Synth(Box<standalone::Synth>),
    Exhaust(Box<crate::acoustics::ExhaustNetwork>),
}

/// A scratch engine prepared off the audio thread and moved into it. The
/// descriptor grid always supplies flow, intake and mechanical texture; the
/// optional event synth replaces its pressure pulses.
pub struct ScratchModel {
    pub descriptors: Arc<ProceduralBank>,
    pub firing: Firing,
    pub life: Life,
    pub synth: Option<Box<standalone::Synth>>,
    /// Built from the parts; the bench retunes lengths and temperatures live.
    pub exhaust: Box<crate::acoustics::ExhaustNetwork>,
    pub idle_rpm: f32,
    pub redline_rpm: f32,
}

impl ScratchModel {
    /// Allocates and analyses; never call from an audio callback.
    pub fn build(scratch: &Scratch, rate: u32) -> Result<Self, String> {
        scratch.validate()?;
        let synth = match scratch.engine {
            ScratchEngine::Experimental => None,
            ScratchEngine::Standalone => Some(Box::new(standalone::Synth::new(
                rate.clamp(8_000, 192_000),
                scratch.standalone.clone(),
                standalone::Commands {
                    rpm: scratch.idle_rpm,
                    load: 0.1,
                    volume: 1.,
                    combustion: standalone::CombustionState::Firing,
                },
            )?)),
        };
        Ok(Self {
            descriptors: Arc::new(ProceduralBank::from_spec(
                &scratch.experimental,
                scratch.cylinders(),
                scratch.idle_rpm,
                scratch.redline_rpm,
            )),
            firing: scratch.design.firing(),
            life: scratch.life(),
            synth,
            exhaust: Box::new(crate::acoustics::ExhaustNetwork::new(
                rate.clamp(8_000, 192_000) as f32,
                crate::acoustics::Geometry {
                    header: 0.5,
                    tail: 1.5,
                    diameter_mm: scratch.build.exhaust_mm,
                    chamber_litres: 6.,
                    absorption: 0.4,
                    resonance: 1.2,
                    temperature_c: 400.,
                },
                scratch.life().exhaust,
            )),
            idle_rpm: scratch.idle_rpm,
            redline_rpm: scratch.redline_rpm,
        })
    }
}
