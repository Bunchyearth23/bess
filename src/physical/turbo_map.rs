//! Versioned corrected-SI turbo tables. Synthetic fixtures are never measurements.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceText(pub [[u8; 32]; 8]);
impl ProvenanceText {
    pub fn new(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() || text.len() > 256 || text.contains('\0') {
            return Err("Provenance text requires 1..256 UTF-8 bytes".into());
        }
        let mut result = Self([[0; 32]; 8]);
        for (i, b) in text.bytes().enumerate() {
            result.0[i / 32][i % 32] = b;
        }
        Ok(result)
    }
    pub fn text(&self) -> String {
        String::from_utf8(
            self.0
                .iter()
                .flatten()
                .copied()
                .take_while(|b| *b != 0)
                .collect(),
        )
        .unwrap_or_default()
    }
    pub fn valid(&self) -> bool {
        let mut bytes = [0; 256];
        for (i, b) in self.0.iter().flatten().enumerate() {
            bytes[i] = *b;
        }
        let len = bytes.iter().position(|b| *b == 0).unwrap_or(256);
        bytes[len..].iter().all(|b| *b == 0)
            && std::str::from_utf8(&bytes[..len]).is_ok_and(|s| !s.trim().is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MapProvenance {
    pub hardware: ProvenanceText,
    pub source: ProvenanceText,
    pub measured: bool,
    pub reference_temperature_k: f64,
    pub reference_pressure_pa: f64,
}
impl MapProvenance {
    pub fn validate(&self) -> Result<(), String> {
        if !self.hardware.valid()
            || !self.source.valid()
            || !(200.0..=1500.0).contains(&self.reference_temperature_k)
            || !(1000.0..=1e7).contains(&self.reference_pressure_pa)
        {
            return Err(
                "Map requires hardware, provenance and valid SI reference conditions".into(),
            );
        }
        Ok(())
    }
    pub fn corrected_speed(&self, rpm: f64, temperature_k: f64) -> f64 {
        rpm / (temperature_k / self.reference_temperature_k).sqrt()
    }
    pub fn corrected_flow(&self, kg_s: f64, temperature_k: f64, pressure_pa: f64) -> f64 {
        kg_s * (temperature_k / self.reference_temperature_k).sqrt()
            / (pressure_pa / self.reference_pressure_pa)
    }
    pub fn actual_flow(&self, corrected_kg_s: f64, temperature_k: f64, pressure_pa: f64) -> f64 {
        corrected_kg_s * (pressure_pa / self.reference_pressure_pa)
            / (temperature_k / self.reference_temperature_k).sqrt()
    }
}

/// Rectangular speed rows, second axis flow for compressor, expansion ratio for
/// turbine. Row-major arrays; boundary hold is explicit and never extrapolated.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TurboTable {
    pub speed_count: usize,
    pub coordinate_count: usize,
    pub corrected_speed_rpm: [f64; 8],
    pub coordinate: [f64; 16],
    pub values: [[f64; 16]; 8],
    pub efficiency: [[f64; 16]; 8],
}
#[derive(Debug, Clone, Copy)]
pub struct MapPoint {
    pub value: f64,
    pub efficiency: f64,
    pub outside: bool,
}
impl TurboTable {
    pub fn validate(&self, compressor: bool) -> Result<(), String> {
        if !(2..=8).contains(&self.speed_count) || !(2..=16).contains(&self.coordinate_count) {
            return Err("Turbo grid supports 2..8 speed rows and 2..16 coordinates".into());
        }
        let speed = &self.corrected_speed_rpm[..self.speed_count];
        let coord = &self.coordinate[..self.coordinate_count];
        let ordered = |axis: &[f64]| {
            axis.iter().all(|v| v.is_finite() && *v >= 0.0) && axis.windows(2).all(|v| v[0] < v[1])
        };
        if !ordered(speed)
            || !ordered(coord)
            || speed[0] <= 0.0
            || speed[self.speed_count - 1] > 1e6
            || (!compressor && coord[0] < 1.0)
        {
            return Err("Invalid turbo grid axes".into());
        }
        for i in 0..self.speed_count {
            for j in 0..self.coordinate_count {
                let v = self.values[i][j];
                let e = self.efficiency[i][j];
                if !(0.05..=1.0).contains(&e)
                    || if compressor {
                        !(1.0..=20.0).contains(&v)
                    } else {
                        !(0.0..=100.0).contains(&v)
                    }
                {
                    return Err("Invalid turbo grid efficiency/value".into());
                }
            }
        }
        Ok(())
    }
    pub fn sample(&self, speed: f64, coordinate: f64) -> MapPoint {
        let interval = |axis: &[f64], x: f64| {
            let i = axis
                .partition_point(|v| *v <= x)
                .saturating_sub(1)
                .min(axis.len() - 2);
            (i, ((x - axis[i]) / (axis[i + 1] - axis[i])).clamp(0.0, 1.0))
        };
        let (i, t) = interval(&self.corrected_speed_rpm[..self.speed_count], speed);
        let (j, u) = interval(&self.coordinate[..self.coordinate_count], coordinate);
        let interp = |v: &[[f64; 16]; 8]| {
            let row = |n: usize| v[n][j] * (1.0 - u) + v[n][j + 1] * u;
            row(i) * (1.0 - t) + row(i + 1) * t
        };
        MapPoint {
            value: interp(&self.values),
            efficiency: interp(&self.efficiency),
            outside: speed < self.corrected_speed_rpm[0]
                || speed > self.corrected_speed_rpm[self.speed_count - 1]
                || coordinate < self.coordinate[0]
                || coordinate > self.coordinate[self.coordinate_count - 1],
        }
    }
    pub fn from_rows(
        speed: &[f64],
        coordinate: &[f64],
        values: &[f64],
        efficiency: &[f64],
    ) -> Result<Self, String> {
        if !(2..=8).contains(&speed.len())
            || !(2..=16).contains(&coordinate.len())
            || values.len() != speed.len() * coordinate.len()
            || efficiency.len() != values.len()
        {
            return Err("Turbo grid exceeds fixed capacities or inconsistent dimensions".into());
        }
        let mut table = Self {
            speed_count: speed.len(),
            coordinate_count: coordinate.len(),
            corrected_speed_rpm: [0.0; 8],
            coordinate: [0.0; 16],
            values: [[0.0; 16]; 8],
            efficiency: [[0.0; 16]; 8],
        };
        table.corrected_speed_rpm[..speed.len()].copy_from_slice(speed);
        table.coordinate[..coordinate.len()].copy_from_slice(coordinate);
        for i in 0..speed.len() {
            for j in 0..coordinate.len() {
                table.values[i][j] = values[i * coordinate.len() + j];
                table.efficiency[i][j] = efficiency[i * coordinate.len() + j];
            }
        }
        Ok(table)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TurboModel {
    pub schema: u32,
    pub provenance: MapProvenance,
    pub compressor: TurboTable,
    pub turbine: TurboTable,
    pub wastegate_tau_s: f64,
    pub bov_tau_s: f64,
    /// Linear shaft drag torque = coefficient * omega; dissipated to housing.
    pub bearing_nm_per_rad_s: f64,
    pub charge_wall_temperature_k: f64,
    pub charge_wall_conductance_w_k: f64,
}
impl TurboModel {
    pub fn validate(&self) -> Result<(), String> {
        self.provenance.validate()?;
        self.compressor.validate(true)?;
        self.turbine.validate(false)?;
        if self.schema != 1
            || !(1e-5..=10.0).contains(&self.wastegate_tau_s)
            || !(1e-5..=10.0).contains(&self.bov_tau_s)
            || !(0.0..=0.1).contains(&self.bearing_nm_per_rad_s)
            || !(200.0..=1000.0).contains(&self.charge_wall_temperature_k)
            || !(0.0..=1e4).contains(&self.charge_wall_conductance_w_k)
        {
            return Err("Invalid turbo actuators/loss/thermal configuration".into());
        }
        Ok(())
    }
    pub fn from_json(json: &str) -> Result<Self, String> {
        let result: Self = match serde_json::from_str(json) {
            Ok(saved) => saved,
            Err(_) => TurboImport::parse(json)?,
        };
        result.validate()?;
        Ok(result)
    }
    /// Human-readable interchange form; conversion happens only off the audio path.
    pub fn to_import_json(&self) -> Result<String, String> {
        self.validate()?;
        let table = |t: &TurboTable| {
            serde_json::json!({
            "corrected_speed_rpm":&t.corrected_speed_rpm[..t.speed_count],
            "coordinate":&t.coordinate[..t.coordinate_count],
            "values":t.values[..t.speed_count].iter().flat_map(|r|r[..t.coordinate_count].iter().copied()).collect::<Vec<_>>(),
            "efficiency":t.efficiency[..t.speed_count].iter().flat_map(|r|r[..t.coordinate_count].iter().copied()).collect::<Vec<_>>()})
        };
        serde_json::to_string_pretty(&serde_json::json!({"schema":self.schema,"provenance":{"hardware":self.provenance.hardware.text(),"source":self.provenance.source.text(),"measured":self.provenance.measured,"reference_temperature_k":self.provenance.reference_temperature_k,"reference_pressure_pa":self.provenance.reference_pressure_pa},"compressor":table(&self.compressor),"turbine":table(&self.turbine),"wastegate_tau_s":self.wastegate_tau_s,"bov_tau_s":self.bov_tau_s,"bearing_nm_per_rad_s":self.bearing_nm_per_rad_s,"charge_wall_temperature_k":self.charge_wall_temperature_k,"charge_wall_conductance_w_k":self.charge_wall_conductance_w_k})).map_err(|e|e.to_string())
    }
    /// Runnable engineering fixture, explicitly synthetic, not a fitted turbo.
    pub fn synthetic_example() -> Self {
        Self {
            schema: 1,
            provenance: MapProvenance {
                hardware: ProvenanceText::new("synthetic demonstration turbo").unwrap(),
                source: ProvenanceText::new("BESS analytical test fixture; no measured map")
                    .unwrap(),
                measured: false,
                reference_temperature_k: 300.0,
                reference_pressure_pa: 101325.0,
            },
            compressor: TurboTable::from_rows(
                &[30000.0, 90000.0, 180000.0],
                &[0.0, 0.04, 0.1, 0.2],
                &[
                    1.02, 1.04, 1.01, 1.0, 1.4, 1.9, 1.5, 1.02, 2.0, 3.0, 3.5, 2.1,
                ],
                &[
                    0.3, 0.55, 0.4, 0.2, 0.4, 0.7, 0.68, 0.4, 0.4, 0.65, 0.75, 0.6,
                ],
            )
            .unwrap(),
            turbine: TurboTable::from_rows(
                &[30000.0, 180000.0],
                &[1.0, 2.0, 4.0],
                &[0.0, 0.02, 0.03, 0.0, 0.04, 0.05],
                &[0.5, 0.6, 0.55, 0.5, 0.72, 0.65],
            )
            .unwrap(),
            wastegate_tau_s: 0.04,
            bov_tau_s: 0.015,
            bearing_nm_per_rad_s: 1e-7,
            charge_wall_temperature_k: 300.0,
            charge_wall_conductance_w_k: 2.0,
        }
    }
}

#[derive(Deserialize)]
struct TurboImport {
    schema: u32,
    provenance: RawProvenance,
    compressor: RawTable,
    turbine: RawTable,
    wastegate_tau_s: f64,
    bov_tau_s: f64,
    bearing_nm_per_rad_s: f64,
    charge_wall_temperature_k: f64,
    charge_wall_conductance_w_k: f64,
}
#[derive(Deserialize)]
struct RawProvenance {
    hardware: String,
    source: String,
    measured: bool,
    reference_temperature_k: f64,
    reference_pressure_pa: f64,
}
#[derive(Deserialize)]
struct RawTable {
    corrected_speed_rpm: Vec<f64>,
    coordinate: Vec<f64>,
    values: Vec<f64>,
    efficiency: Vec<f64>,
}
impl TurboImport {
    fn parse(json: &str) -> Result<TurboModel, String> {
        let raw: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let compile = |r: RawTable| {
            TurboTable::from_rows(
                &r.corrected_speed_rpm,
                &r.coordinate,
                &r.values,
                &r.efficiency,
            )
        };
        Ok(TurboModel {
            schema: raw.schema,
            provenance: MapProvenance {
                hardware: ProvenanceText::new(&raw.provenance.hardware)?,
                source: ProvenanceText::new(&raw.provenance.source)?,
                measured: raw.provenance.measured,
                reference_temperature_k: raw.provenance.reference_temperature_k,
                reference_pressure_pa: raw.provenance.reference_pressure_pa,
            },
            compressor: compile(raw.compressor)?,
            turbine: compile(raw.turbine)?,
            wastegate_tau_s: raw.wastegate_tau_s,
            bov_tau_s: raw.bov_tau_s,
            bearing_nm_per_rad_s: raw.bearing_nm_per_rad_s,
            charge_wall_temperature_k: raw.charge_wall_temperature_k,
            charge_wall_conductance_w_k: raw.charge_wall_conductance_w_k,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interpolation_is_continuous_and_correction_invertible() {
        let m = TurboModel::synthetic_example();
        m.validate().unwrap();
        let a = m.compressor.sample(90000.0 - 1e-5, 0.07);
        let b = m.compressor.sample(90000.0 + 1e-5, 0.07);
        assert!((a.value - b.value).abs() < 1e-8);
        assert!((m.compressor.sample(90000.0, 0.07).value - 1.7).abs() < 1e-12);
        let corrected = m.provenance.corrected_flow(0.08, 450.0, 180000.0);
        assert!((m.provenance.actual_flow(corrected, 450.0, 180000.0) - 0.08).abs() < 1e-12);
        assert!(m.compressor.sample(1e6, 5.0).outside);
    }
    #[test]
    fn rejects_bad_grid_and_roundtrips_provenance() {
        let mut m = TurboModel::synthetic_example();
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(TurboModel::from_json(&json).unwrap(), m);
        assert_eq!(
            TurboModel::from_json(&m.to_import_json().unwrap()).unwrap(),
            m
        );
        m.compressor.efficiency[0][0] = 1.01;
        assert!(m.validate().is_err());
        m.compressor.efficiency[0][0] = 0.5;
        m.compressor.coordinate[1] = 0.0;
        assert!(m.validate().is_err());
    }
}
