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

/// Acoustic connection between the two exhaust banks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Crossover {
    #[default]
    None,
    H,
    X,
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
    pub crossover: Crossover,
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
            crossover: Crossover::None,
            exhaust_mm: 55.,
            catalyst: Catalyst::Standard,
            muffler: Muffler::Baffled,
        }
    }
}

/// Optional overrides of quantities otherwise derived from the parts; `None` =
/// derived. Held by `Scratch`, not `EngineBuild`, so the per-design seed and
/// old projects stay bit-identical (W-007, D-037).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineTuning {
    pub cam: CamTuning,
    pub valves: ValveTuning,
    pub bottom: BottomTuning,
    pub intake: IntakeTuning,
    pub turbo: TurboTuning,
}

/// Independent valve profiles over a backwards-compatible shared baseline.
/// Old projects' duration/lift overrides remain the baseline for both cams;
/// separate overrides never change the opposite valve's profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CamTuning {
    /// Legacy shared duration at 0.050″ net lift, or the part-derived baseline.
    pub duration_deg: Option<f32>,
    /// Legacy shared lift, or the part-derived baseline.
    pub lift_mm: Option<f32>,
    pub intake_duration_deg: Option<f32>,
    pub intake_lift_mm: Option<f32>,
    pub exhaust_duration_deg: Option<f32>,
    pub exhaust_lift_mm: Option<f32>,
    pub lsa_deg: Option<f32>,
    /// Intake centreline = LSA − advance.
    pub intake_advance_deg: Option<f32>,
    /// Positive advances exhaust opening/closing (earlier crank angles).
    pub exhaust_advance_deg: Option<f32>,
}

impl CamTuning {
    /// Return intake to the inherited/shared profile without changing exhaust.
    pub fn reset_intake(&mut self) {
        self.intake_duration_deg = None;
        self.intake_lift_mm = None;
        self.intake_advance_deg = None;
    }

    /// Return exhaust to the inherited/shared profile without changing intake.
    pub fn reset_exhaust(&mut self) {
        self.exhaust_duration_deg = None;
        self.exhaust_lift_mm = None;
        self.exhaust_advance_deg = None;
    }
}

/// Head diameters as fractions of the bore, so they follow bore changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValveTuning {
    pub intake_to_bore: Option<f32>,
    pub exhaust_to_bore: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BottomTuning {
    pub rod_to_stroke: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IntakeTuning {
    /// Equivalent single throttle, all individual throttles combined.
    pub throttle_mm: Option<f32>,
    /// Plenum volume / total displacement.
    pub plenum_ratio: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TurboTuning {
    /// Compressor swept volume × size; rotor inertia × size^(5/3).
    pub size: Option<f32>,
}

/// Concrete SI values, derived or fixed, consumed by the physical engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedTuning {
    pub intake_valves: u32,
    pub exhaust_valves: u32,
    pub duration_at_050_deg: f64,
    pub lift_m: f64,
    pub exhaust_duration_at_050_deg: f64,
    pub exhaust_lift_m: f64,
    /// Four-stroke crank degrees, firing TDC = 0, overlap TDC = 360.
    pub intake_center_deg: f64,
    pub exhaust_center_deg: f64,
    pub intake_diameter_m: f64,
    pub exhaust_diameter_m: f64,
    pub rod_m: f64,
    pub throttle_area_m2: f64,
    pub plenum_volume_m3: f64,
    pub compressor_displacement_m3: f64,
    pub turbo_inertia_kg_m2: f64,
    /// Effective turbine nozzle area; zero without a turbocharger.
    pub turbine_area_m2: f64,
    /// Wastegate valve flow area in parallel with the nozzle.
    pub wastegate_area_m2: f64,
    /// Per-cylinder intake runner, plenum to valve seat (port included).
    pub runner_length_m: f64,
    pub runner_area_m2: f64,
}

impl EngineTuning {
    pub fn is_derived(&self) -> bool {
        *self == Self::default()
    }

    /// The only home of the part → quantity estimates (design choices, not
    /// measurements). `cylinders` sizes intake and turbo to total displacement.
    pub fn resolve(&self, build: &EngineBuild, cylinders: u32) -> ResolvedTuning {
        use std::f64::consts::PI;
        let or = |value: Option<f32>, derived: f64| value.map_or(derived, f64::from);
        let cam = f64::from(build.cam);
        let bore_m = f64::from(build.bore_mm) * 0.001;
        let stroke_m = f64::from(build.stroke_mm) * 0.001;
        let intake_valves = u32::from(build.valves).div_ceil(2);
        let exhaust_valves = u32::from(build.valves) / 2;
        let lsa = or(self.cam.lsa_deg, 114. - 8. * cam);
        let shared_duration = or(self.cam.duration_deg, 200. + 60. * cam);
        let shared_lift = self
            .cam
            .lift_mm
            .map_or(0.009 + 0.004 * cam, |mm| f64::from(mm) * 0.001);
        let displacement = PI * bore_m.powi(2) * stroke_m / 4. * f64::from(cylinders);
        let individual = build.throttle == Throttle::Individual;
        let turbo = or(self.turbo.size, 1.);
        let rotor = if build.aspiration == Aspiration::TwinTurbo {
            1.2e-5
        } else {
            2.0e-5
        };
        ResolvedTuning {
            intake_valves,
            exhaust_valves,
            duration_at_050_deg: or(self.cam.intake_duration_deg, shared_duration),
            lift_m: self
                .cam
                .intake_lift_mm
                .map_or(shared_lift, |mm| f64::from(mm) * 0.001),
            exhaust_duration_at_050_deg: or(self.cam.exhaust_duration_deg, shared_duration),
            exhaust_lift_m: self
                .cam
                .exhaust_lift_mm
                .map_or(shared_lift, |mm| f64::from(mm) * 0.001),
            intake_center_deg: 360. + lsa - or(self.cam.intake_advance_deg, 0.),
            exhaust_center_deg: 360. - lsa - or(self.cam.exhaust_advance_deg, 0.),
            intake_diameter_m: bore_m
                * or(
                    self.valves.intake_to_bore,
                    if intake_valves == 1 { 0.43 } else { 0.36 },
                ),
            exhaust_diameter_m: bore_m
                * or(
                    self.valves.exhaust_to_bore,
                    if exhaust_valves == 1 { 0.37 } else { 0.31 },
                ),
            rod_m: stroke_m * or(self.bottom.rod_to_stroke, 1.75),
            throttle_area_m2: match self.intake.throttle_mm {
                Some(mm) => PI * (f64::from(mm) * 0.001).powi(2) / 4.0,
                // 55 mm equivalent throttle at 2 L, with area scaled to displacement.
                None => {
                    PI * 0.055_f64.powi(2) / 4.0
                        * (displacement / 0.002).powf(2.0 / 3.0)
                        * if individual { 1.35 } else { 1.0 }
                }
            },
            plenum_volume_m3: displacement
                * or(
                    self.intake.plenum_ratio,
                    if individual { 0.35 } else { 1.25 },
                ),
            // ponytail: geometric similarity (swept volume ∝ r³, inertia ∝ r⁵),
            // not a turbo family map; replace with measured frames if needed.
            // Matched to the engine's WOT line at the target boost: with the
            // 4 × target head (Induction) the target is reached at half the
            // speed limit, where one swept volume per revolution is ≈0.054
            // kg/s at 2 L (surge sits at 0.6 × that, ≈0.033 kg/s, after the
            // X-027 exit-density scaling) and the choke side at the limit
            // ≈3 × that (≈0.165 kg/s, ≈6300 rpm). The former 0.005
            // choked at ≈0.10 kg/s, so boost collapsed above 4400 rpm.
            compressor_displacement_m3: displacement * 0.008 * turbo,
            turbo_inertia_kg_m2: rotor * turbo.powf(5. / 3.),
            // ≈27 mm equivalent nozzle at 2 L: builds turbine backpressure
            // (inlet ≈1.6–2 bar abs at full boost) so the wastegate cracks
            // near 3000 rpm, the usual small-frame match.
            turbine_area_m2: if build.aspiration == Aspiration::Natural {
                0.
            } else {
                5e-4 * (displacement / 0.002) * turbo.powf(2. / 3.)
            },
            // ≈29 mm swing valve at 2 L.
            wastegate_area_m2: if build.aspiration == Aspiration::Natural {
                0.
            } else {
                6.5e-4 * (displacement / 0.002)
            },
            // 0.40 m plenum-to-valve at an 86 mm stroke (typical stock 2 L
            // DOHC runners 0.3–0.45 m), scaled with stroke so the Helmholtz
            // tune stays at a similar fraction of the piston-speed-limited
            // redline; area = the intake valve heads (≈44 mm at 2 L).
            runner_length_m: 0.40 * stroke_m / 0.086,
            runner_area_m2: PI / 4.
                * (bore_m
                    * or(
                        self.valves.intake_to_bore,
                        if intake_valves == 1 { 0.43 } else { 0.36 },
                    ))
                .powi(2)
                * f64::from(intake_valves),
        }
    }

    /// Numeric ranges only (D-028): a valve that cannot fit is a warning, not an error.
    pub fn validate(&self) -> Result<(), String> {
        for (label, value, min, max) in [
            ("Cam duration", self.cam.duration_deg, 180., 300.),
            ("Cam lift", self.cam.lift_mm, 7., 16.),
            ("Intake duration", self.cam.intake_duration_deg, 180., 300.),
            ("Intake lift", self.cam.intake_lift_mm, 7., 16.),
            (
                "Exhaust duration",
                self.cam.exhaust_duration_deg,
                180.,
                300.,
            ),
            ("Exhaust lift", self.cam.exhaust_lift_mm, 7., 16.),
            ("Lobe separation", self.cam.lsa_deg, 102., 120.),
            ("Intake cam advance", self.cam.intake_advance_deg, -4., 10.),
            (
                "Exhaust cam advance",
                self.cam.exhaust_advance_deg,
                -10.,
                10.,
            ),
            (
                "Intake valve / bore",
                self.valves.intake_to_bore,
                0.25,
                0.55,
            ),
            (
                "Exhaust valve / bore",
                self.valves.exhaust_to_bore,
                0.17,
                0.53,
            ),
            ("Rod / stroke", self.bottom.rod_to_stroke, 1.4, 2.2),
            ("Throttle diameter", self.intake.throttle_mm, 30., 110.),
            ("Plenum / displacement", self.intake.plenum_ratio, 0.3, 3.),
            ("Turbo size", self.turbo.size, 0.5, 2.),
        ] {
            if let Some(value) = value
                && !(min..=max).contains(&value)
            {
                return Err(format!("{label}: expected {min}–{max}"));
            }
        }
        Ok(())
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
        let idle_rpm =
            (700. + self.cam * 500. + (self.aspiration != Aspiration::Natural) as u8 as f32 * 50.)
                .min(redline_rpm - 1500.);
        let boost = match self.aspiration {
            Aspiration::Natural => 0.,
            _ => self.boost_bar,
        };
        let bmep_bar = (10.5
            + (self.compression - 10.) * 0.35
            + self.cam * 1.2
            + if self.vvt { 0.6 } else { 0. }
            + if self.valves >= 4 { 0.5 } else { 0. })
            * (1. + boost * 0.85);
        let peak_torque_nm = bmep_bar * 1e5 * displacement_l * 1e-3 / (4. * std::f32::consts::PI);
        // Torque falls away above its peak; racier cams keep it higher.
        let power_rpm = redline_rpm * (0.85 + self.cam * 0.08).min(0.95);
        let peak_power_kw =
            peak_torque_nm * (0.78 + self.cam * 0.12) * power_rpm * std::f32::consts::TAU
                / 60.
                / 1000.;
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
        assert!(
            (p.displacement_l - 1.998).abs() < 0.01,
            "{}",
            p.displacement_l
        );
        assert!(
            (6800. ..=7800.).contains(&p.redline_rpm),
            "{}",
            p.redline_rpm
        );
        assert!(
            (170. ..=220.).contains(&p.peak_torque_nm),
            "{}",
            p.peak_torque_nm
        );
        assert!(
            (90. ..=130.).contains(&p.peak_power_kw),
            "{}",
            p.peak_power_kw
        );
        let turbo = EngineBuild {
            aspiration: Aspiration::Turbo,
            ..Default::default()
        }
        .performance(4);
        assert!(turbo.peak_torque_nm > p.peak_torque_nm * 1.5);
        let race = EngineBuild {
            cam: 1.,
            crank: Crankshaft::Billet,
            ..Default::default()
        }
        .performance(4);
        assert!(race.redline_rpm > p.redline_rpm && race.idle_rpm > p.idle_rpm);
    }
}
