//! Imported reduced structural response; observes forces, never drives the gas.
//! Implicit midpoint preserves modal work and accounts positive damping losses.
use super::turbo_map::ProvenanceText;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StructuralMode {
    pub frequency_hz: f64,
    pub damping_ratio: f64,
    pub mass_kg: f64,
    pub head_participation: f64,
    pub block_participation: f64,
    /// At the documented listening position; explicitly an observation transfer.
    pub radiation_pa_per_m_s: f64,
}
impl Default for StructuralMode {
    fn default() -> Self {
        Self {
            frequency_hz: 400.0,
            damping_ratio: 0.1,
            mass_kg: 1.0,
            head_participation: 0.0,
            block_participation: 0.0,
            radiation_pa_per_m_s: 0.0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StructuralResponse {
    pub schema: u32,
    pub hardware: ProvenanceText,
    pub source: ProvenanceText,
    pub measured: bool,
    pub sensor_position_m: [f64; 3],
    pub listener_position_m: [f64; 3],
    /// Existing BESS packets are relative excitations; conversion requires its
    /// own calibration. An imported mode alone never authenticates this scale.
    pub excitation_scale_measured: bool,
    pub head_force_n_per_unit: f64,
    pub block_force_n_per_unit: f64,
    pub pa_to_sample: f64,
    pub count: usize,
    pub modes: [StructuralMode; 32],
}
impl StructuralResponse {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || !(1..=32).contains(&self.count)
            || !self.hardware.valid()
            || !self.source.valid()
            || self
                .sensor_position_m
                .iter()
                .chain(self.listener_position_m.iter())
                .any(|v| !v.is_finite() || v.abs() > 1000.0)
            || !(0.0..=1e6).contains(&self.head_force_n_per_unit)
            || !(0.0..=1e6).contains(&self.block_force_n_per_unit)
            || !(1e-12..=1.0).contains(&self.pa_to_sample)
        {
            return Err(
                "Structural response requires bounded SI scales, positions and provenance".into(),
            );
        }
        for m in &self.modes[..self.count] {
            if !(1.0..=40000.0).contains(&m.frequency_hz)
                || !(0.0001..=10.0).contains(&m.damping_ratio)
                || !(1e-6..=1e4).contains(&m.mass_kg)
                || [
                    m.head_participation,
                    m.block_participation,
                    m.radiation_pa_per_m_s,
                ]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1e6)
            {
                return Err("Invalid structural modal mass/frequency/damping/transfer".into());
            }
        }
        Ok(())
    }
    pub fn from_json(json: &str) -> Result<Self, String> {
        let result = match serde_json::from_str(json) {
            Ok(saved) => saved,
            Err(_) => {
                let raw: ResponseImport = serde_json::from_str(json).map_err(|e| e.to_string())?;
                if raw.modes.is_empty() || raw.modes.len() > 32 {
                    return Err("Structural import supports 1..32 modes".into());
                }
                let mut modes = [StructuralMode::default(); 32];
                modes[..raw.modes.len()].copy_from_slice(&raw.modes);
                Self {
                    schema: raw.schema,
                    hardware: ProvenanceText::new(&raw.hardware)?,
                    source: ProvenanceText::new(&raw.source)?,
                    measured: raw.measured,
                    sensor_position_m: raw.sensor_position_m,
                    listener_position_m: raw.listener_position_m,
                    excitation_scale_measured: raw.excitation_scale_measured,
                    head_force_n_per_unit: raw.head_force_n_per_unit,
                    block_force_n_per_unit: raw.block_force_n_per_unit,
                    pa_to_sample: raw.pa_to_sample,
                    count: raw.modes.len(),
                    modes,
                }
            }
        };
        result.validate()?;
        Ok(result)
    }
    pub fn synthetic_example() -> Self {
        let mut modes = [StructuralMode::default(); 32];
        modes[0] = StructuralMode {
            frequency_hz: 350.0,
            damping_ratio: 0.12,
            mass_kg: 8.0,
            head_participation: 0.3,
            block_participation: 1.0,
            radiation_pa_per_m_s: 2.0,
        };
        modes[1] = StructuralMode {
            frequency_hz: 1400.0,
            damping_ratio: 0.16,
            mass_kg: 1.5,
            head_participation: 1.0,
            block_participation: 0.1,
            radiation_pa_per_m_s: 1.0,
        };
        Self {
            schema: 1,
            hardware: ProvenanceText::new("synthetic block/head").unwrap(),
            source: ProvenanceText::new(
                "BESS illustrative engineering modes; no measured response or source scale",
            )
            .unwrap(),
            measured: false,
            sensor_position_m: [0.0; 3],
            listener_position_m: [1.0, 0.0, 0.0],
            excitation_scale_measured: false,
            head_force_n_per_unit: 1000.0,
            block_force_n_per_unit: 1000.0,
            pa_to_sample: 0.01,
            count: 2,
            modes,
        }
    }
    /// Human-readable calibrated-SI interchange; never called from audio stepping.
    pub fn to_import_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(&serde_json::json!({"schema":self.schema,"hardware":self.hardware.text(),"source":self.source.text(),"measured":self.measured,"sensor_position_m":self.sensor_position_m,"listener_position_m":self.listener_position_m,"excitation_scale_measured":self.excitation_scale_measured,"head_force_n_per_unit":self.head_force_n_per_unit,"block_force_n_per_unit":self.block_force_n_per_unit,"pa_to_sample":self.pa_to_sample,"modes":&self.modes[..self.count]})).map_err(|e|e.to_string())
    }
}
#[derive(Deserialize)]
struct ResponseImport {
    schema: u32,
    hardware: String,
    source: String,
    measured: bool,
    sensor_position_m: [f64; 3],
    listener_position_m: [f64; 3],
    excitation_scale_measured: bool,
    head_force_n_per_unit: f64,
    block_force_n_per_unit: f64,
    pa_to_sample: f64,
    modes: Vec<StructuralMode>,
}
#[derive(Clone)]
pub struct StructuralObserver {
    response: StructuralResponse,
    position: [f64; 32],
    velocity: [f64; 32],
    dt: f64,
    energy_j: f64,
    work_j: f64,
    damping_j: f64,
}
impl StructuralObserver {
    pub fn new(response: StructuralResponse, rate: u32) -> Result<Self, String> {
        response.validate()?;
        if !(8000..=384000).contains(&rate)
            || response.modes[..response.count]
                .iter()
                .any(|m| m.frequency_hz > 0.4 * f64::from(rate))
        {
            return Err(
                "Structural mode exceeds resolved output-rate domain (0.4 sample rate)".into(),
            );
        }
        Ok(Self {
            response,
            position: [0.0; 32],
            velocity: [0.0; 32],
            dt: 1.0 / f64::from(rate),
            energy_j: 0.0,
            work_j: 0.0,
            damping_j: 0.0,
        })
    }
    pub fn next(&mut self, head: f32, block: f32) -> f32 {
        let mut out = 0.0;
        let mut energy = 0.0;
        let mut work = 0.0;
        let mut loss = 0.0;
        for i in 0..self.response.count {
            let m = self.response.modes[i];
            let omega = std::f64::consts::TAU * m.frequency_hz;
            let k = m.mass_kg * omega * omega;
            let c = 2.0 * m.damping_ratio * m.mass_kg * omega;
            let force = f64::from(head)
                * self.response.head_force_n_per_unit
                * m.head_participation
                + f64::from(block) * self.response.block_force_n_per_unit * m.block_participation;
            let x = self.position[i];
            let v = self.velocity[i];
            let dt = self.dt;
            let vm = (v + 0.5 * dt * (force - k * x) / m.mass_kg)
                / (1.0 + 0.5 * dt * c / m.mass_kg + 0.25 * dt * dt * k / m.mass_kg);
            let next_x = x + dt * vm;
            let next_v = 2.0 * vm - v;
            self.position[i] = next_x;
            self.velocity[i] = next_v;
            energy += 0.5 * m.mass_kg * next_v * next_v + 0.5 * k * next_x * next_x;
            work += force * dt * vm;
            loss += c * dt * vm * vm;
            out += vm * m.radiation_pa_per_m_s;
        }
        self.energy_j = energy;
        self.work_j = work;
        self.damping_j = loss;
        (out * self.response.pa_to_sample) as f32
    }
    pub fn energy_j(&self) -> f64 {
        self.energy_j
    }
    pub fn last_work_j(&self) -> f64 {
        self.work_j
    }
    pub fn last_damping_loss_j(&self) -> f64 {
        self.damping_j
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radiation_scale_cannot_change_modal_work_or_mechanics_and_rates_converge() {
        let run = |rate: u32| {
            let response = StructuralResponse::synthetic_example();
            let mut changed = response;
            changed.pa_to_sample *= 2.0;
            let mut a = StructuralObserver::new(response, rate).unwrap();
            let mut b = StructuralObserver::new(changed, rate).unwrap();
            let mut energy = 0.0;
            for i in 0..rate / 5 {
                let input =
                    (std::f64::consts::TAU * 200.0 * f64::from(i) / f64::from(rate)).sin() as f32;
                let x = a.next(input, 0.0);
                let y = b.next(input, 0.0);
                assert_eq!(a.energy_j().to_bits(), b.energy_j().to_bits());
                assert_eq!(a.last_work_j().to_bits(), b.last_work_j().to_bits());
                assert_eq!(
                    a.last_damping_loss_j().to_bits(),
                    b.last_damping_loss_j().to_bits()
                );
                assert!((y - 2.0 * x).abs() < 1e-10);
                if i >= rate / 10 {
                    energy += f64::from(x).powi(2);
                }
            }
            (energy / f64::from(rate / 10)).sqrt()
        };
        let coarse = run(48000);
        let fine = run(96000);
        let finer = run(192000);
        assert!((coarse / fine - 1.0).abs() < 0.02);
        assert!((fine / finer - 1.0).abs() < 0.005);
    }
    #[test]
    fn driven_modal_energy_closes_and_tail_dissipates() {
        let mut observer =
            StructuralObserver::new(StructuralResponse::synthetic_example(), 48000).unwrap();
        let mut work = 0.0;
        let mut loss = 0.0;
        for i in 0..4800 {
            let before = observer.energy_j();
            let sample = observer.next(if i < 100 { 1.0 } else { 0.0 }, 0.0);
            assert!(sample.is_finite());
            work += observer.last_work_j();
            loss += observer.last_damping_loss_j();
            assert!(
                (observer.energy_j() - before - observer.last_work_j()
                    + observer.last_damping_loss_j())
                .abs()
                    < 1e-10
            );
        }
        assert!((observer.energy_j() - work + loss).abs() < 1e-8);
        assert!(observer.energy_j() < 1e-10);
        assert!(loss > 0.0);
    }
    #[test]
    fn response_roundtrip_and_nyquist_rejection() {
        let response = StructuralResponse::synthetic_example();
        assert_eq!(
            StructuralResponse::from_json(&serde_json::to_string(&response).unwrap()).unwrap(),
            response
        );
        assert_eq!(
            StructuralResponse::from_json(&response.to_import_json().unwrap()).unwrap(),
            response
        );
        let mut bad = response;
        bad.modes[0].frequency_hz = 22000.0;
        assert!(StructuralObserver::new(bad, 48000).is_err());
        bad = response;
        bad.modes[0].mass_kg = 0.0;
        assert!(bad.validate().is_err());
    }
}
