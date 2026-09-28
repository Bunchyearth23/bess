//! Explicit SI translation of builder parts for the first cylinder prototype.
//!
//! Rod ratio, valve diameters/lift and cam mapping are design estimates, not
//! measurements of a real engine. They remain visible here for calibration.
use crate::engine_build::EngineBuild;

#[derive(Clone, Copy, Debug)]
pub struct CylinderConfig {
    pub bore_m: f64,
    pub stroke_m: f64,
    pub rod_m: f64,
    pub compression_ratio: f64,
    pub intake_valves: u32,
    pub exhaust_valves: u32,
    pub intake_diameter_m: f64,
    pub exhaust_diameter_m: f64,
    pub lift_m: f64,
    pub lash_m: f64,
    pub seat_duration_deg: f64,
    /// Four-stroke crank degrees, firing TDC = 0, overlap TDC = 360.
    pub intake_center_deg: f64,
    pub exhaust_center_deg: f64,
    pub nominal_duration_at_050_deg: f64,
    pub wall_temperature_k: f64,
}

impl CylinderConfig {
    pub fn from_build(build: &EngineBuild) -> Result<Self, String> {
        build.validate()?;
        let cam = f64::from(build.cam);
        let bore_m = f64::from(build.bore_mm) * 0.001;
        let stroke_m = f64::from(build.stroke_mm) * 0.001;
        let intake_valves = u32::from(build.valves).div_ceil(2);
        let exhaust_valves = u32::from(build.valves) / 2;
        let lift_m = 0.009 + 0.004 * cam;
        let lash_m = 0.0002;
        let nominal_duration_at_050_deg = 200. + 60. * cam;
        // For harmonic lift L/2*(1+cos(2π*offset/duration)), convert the
        // duration at 0.050 inches net lift to a duration at the cam seat.
        let half_width = (2. * (0.00127 + lash_m) / lift_m - 1.).acos();
        let seat_duration_deg = nominal_duration_at_050_deg * std::f64::consts::PI / half_width;
        let lsa = 114. - 8. * cam;
        let setup = Self {
            bore_m,
            stroke_m,
            rod_m: stroke_m * 1.75,
            compression_ratio: f64::from(build.compression),
            intake_valves,
            exhaust_valves,
            intake_diameter_m: bore_m * if intake_valves == 1 { 0.43 } else { 0.36 },
            exhaust_diameter_m: bore_m * if exhaust_valves == 1 { 0.37 } else { 0.31 },
            lift_m,
            lash_m,
            seat_duration_deg,
            intake_center_deg: 360. + lsa,
            exhaust_center_deg: 360. - lsa,
            nominal_duration_at_050_deg,
            wall_temperature_k: 450.,
        };
        if !setup.displacement_m3().is_finite() || setup.displacement_m3() <= 0. {
            return Err("Physical cylinder displacement is not representable".into());
        }
        Ok(setup)
    }

    pub fn displacement_m3(&self) -> f64 {
        std::f64::consts::PI * self.bore_m.powi(2) * self.stroke_m / 4.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_dimensions_become_si_geometry() {
        let config = CylinderConfig::from_build(&EngineBuild::default()).unwrap();
        assert!((config.bore_m - 0.086).abs() < 1e-12);
        assert!((config.displacement_m3() * 1000. - 0.499557).abs() < 1e-5);
        assert!(config.rod_m > config.stroke_m / 2.);
        assert_eq!((config.intake_valves, config.exhaust_valves), (2, 2));
    }

    #[test]
    fn racing_cam_changes_duration_and_overlap_not_an_audio_gain() {
        let mild = CylinderConfig::from_build(&EngineBuild {
            cam: 0.,
            ..Default::default()
        })
        .unwrap();
        let race = CylinderConfig::from_build(&EngineBuild {
            cam: 1.,
            ..Default::default()
        })
        .unwrap();
        assert!(race.seat_duration_deg > mild.seat_duration_deg);
        assert!(race.lift_m > mild.lift_m);
        let duration = |c: CylinderConfig| {
            c.seat_duration_deg / std::f64::consts::PI
                * (2. * (0.00127 + c.lash_m) / c.lift_m - 1.).acos()
        };
        assert!((duration(mild) - 200.).abs() < 1e-10);
        assert!((duration(race) - 260.).abs() < 1e-10);
        assert!(race.intake_center_deg < mild.intake_center_deg);
        assert!(race.exhaust_center_deg > mild.exhaust_center_deg);
    }
}
