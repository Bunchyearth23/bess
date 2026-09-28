//! Automation-style engine builder: mechanical parts, from which the scratch
//! engine's sound design and bench performance are derived.
//!
//! The part → sound mappings are sound-design heuristics grounded in how each
//! part changes gas exchange and radiation, not a gas-dynamics simulation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockMaterial {
    CastIron,
    Aluminium,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Head {
    Pushrod,
    Sohc,
    Dohc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Crankshaft {
    Cast,
    Forged,
    Billet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspiration {
    Natural,
    Turbo,
    TwinTurbo,
}

/// What happens to boost when the throttle shuts on a turbo engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlowOff {
    /// Recirculates to the intake: a soft whoosh.
    Recirculating,
    /// Vents to air: the "pssh".
    Atmospheric,
    /// No valve: compressor surge flutter.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fuel {
    Carburettor,
    PortInjection,
    DirectInjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Throttle {
    Single,
    Individual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Headers {
    CastManifold,
    Tubular,
    EqualLength,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Catalyst {
    None,
    Standard,
    HighFlow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Muffler {
    None,
    StraightThrough,
    Baffled,
    ReverseFlow,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineBuild {
    pub bore_mm: f32,
    pub stroke_mm: f32,
    pub block: BlockMaterial,
    pub head: Head,
    pub valves: u8,
    pub crank: Crankshaft,
    pub compression: f32,
    /// 0 = mild economy cam, 1 = race cam with large overlap (lumpy idle).
    pub cam: f32,
    pub vvt: bool,
    pub aspiration: Aspiration,
    pub boost_bar: f32,
    pub blow_off: BlowOff,
    pub fuel: Fuel,
    pub throttle: Throttle,
    pub headers: Headers,
    pub exhaust_mm: f32,
    pub catalyst: Catalyst,
    pub muffler: Muffler,
}

impl Default for EngineBuild {
    fn default() -> Self {
        Self {
            bore_mm: 86.,
            stroke_mm: 86.,
            block: BlockMaterial::Aluminium,
            head: Head::Dohc,
            valves: 4,
            crank: Crankshaft::Cast,
            compression: 10.5,
            cam: 0.3,
            vvt: true,
            aspiration: Aspiration::Natural,
            boost_bar: 0.8,
            blow_off: BlowOff::Recirculating,
            fuel: Fuel::PortInjection,
            throttle: Throttle::Single,
            headers: Headers::CastManifold,
            exhaust_mm: 55.,
            catalyst: Catalyst::Standard,
            muffler: Muffler::Baffled,
        }
    }
}

/// Figures shown on the builder's summary and fed to the driving bench.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Performance {
    pub displacement_l: f32,
    pub redline_rpm: f32,
    pub idle_rpm: f32,
    pub peak_torque_nm: f32,
    pub peak_power_kw: f32,
    /// Rotating inertia for the bench, kg·m².
    pub inertia: f32,
}

impl EngineBuild {
    pub fn validate(&self) -> Result<(), String> {
        // Bore and stroke are free: any positive size is a valid design.
        for (name, value) in [("bore", self.bore_mm), ("stroke", self.stroke_mm)] {
            if !value.is_finite() || value <= 0. {
                return Err(format!("Engine {name} must be a positive size"));
            }
        }
        if !self.cylinder_litres().is_finite() {
            return Err("Engine displacement is too large to compute".into());
        }
        for (name, value, lo, hi) in [
            ("compression", self.compression, 7., 14.),
            ("cam profile", self.cam, 0., 1.),
            ("boost", self.boost_bar, 0.2, 2.5),
            ("exhaust diameter", self.exhaust_mm, 35., 100.),
        ] {
            if !value.is_finite() || value < lo || value > hi {
                return Err(format!("Engine {name} out of range"));
            }
        }
        if !(2..=5).contains(&self.valves) {
            return Err("Valves per cylinder: 2 to 5".into());
        }
        Ok(())
    }

    pub fn cylinder_litres(&self) -> f32 {
        std::f32::consts::PI / 4. * (self.bore_mm * 0.1).powi(2) * self.stroke_mm * 0.1 / 1000.
    }

    pub fn performance(&self, cylinders: u32) -> Performance {
        let displacement_l = self.cylinder_litres() * cylinders as f32;
        // Mean piston speed limit set by the valvetrain and bottom end.
        let piston_speed = match self.head {
            Head::Pushrod => 17.,
            Head::Sohc => 19.,
            Head::Dohc => 20.5,
        } + match self.crank {
            Crankshaft::Cast => 0.,
            Crankshaft::Forged => 1.5,
            Crankshaft::Billet => 3.,
        } + self.cam * 1.5;
        let redline_rpm = (piston_speed * 30_000. / self.stroke_mm / 100.).round() * 100.;
        let redline_rpm = redline_rpm.clamp(3000., 12_000.);
        // Overlap destabilises low-speed combustion; racier cams idle higher.
        let idle_rpm = (700. + self.cam * 500. + (self.aspiration != Aspiration::Natural) as u8 as f32 * 50.)
            .min(redline_rpm - 1500.);
        let boost = match self.aspiration {
            Aspiration::Natural => 0.,
            _ => self.boost_bar,
        };
        let bmep_bar = (10.5 + (self.compression - 10.) * 0.35 + self.cam * 1.2
            + if self.vvt { 0.6 } else { 0. }
            + if self.valves >= 4 { 0.5 } else { 0. })
            * (1. + boost * 0.85);
        let peak_torque_nm = bmep_bar * 1e5 * displacement_l * 1e-3 / (4. * std::f32::consts::PI);
        // Torque falls away above its peak; racier cams keep it higher.
        let power_rpm = redline_rpm * (0.85 + self.cam * 0.08).min(0.95);
        let peak_power_kw =
            peak_torque_nm * (0.78 + self.cam * 0.12) * power_rpm * std::f32::consts::TAU / 60. / 1000.;
        let inertia = (0.08 + displacement_l * 0.07)
            * match self.crank {
                Crankshaft::Cast => 1.,
                Crankshaft::Forged => 0.85,
                Crankshaft::Billet => 0.7,
            };
        // Estimates are reported as computed; the bench clamps what it uses.
        Performance {
            displacement_l,
            redline_rpm,
            idle_rpm,
            peak_torque_nm,
            peak_power_kw,
            inertia,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_two_litre_four_has_plausible_figures() {
        let p = EngineBuild::default().performance(4);
        assert!((p.displacement_l - 1.998).abs() < 0.01, "{}", p.displacement_l);
        assert!((6800. ..=7800.).contains(&p.redline_rpm), "{}", p.redline_rpm);
        assert!((170. ..=220.).contains(&p.peak_torque_nm), "{}", p.peak_torque_nm);
        assert!((90. ..=130.).contains(&p.peak_power_kw), "{}", p.peak_power_kw);
        let turbo = EngineBuild { aspiration: Aspiration::Turbo, ..Default::default() }.performance(4);
        assert!(turbo.peak_torque_nm > p.peak_torque_nm * 1.5);
        let race = EngineBuild { cam: 1., crank: Crankshaft::Billet, ..Default::default() }.performance(4);
        assert!(race.redline_rpm > p.redline_rpm && race.idle_rpm > p.idle_rpm);
    }
}
