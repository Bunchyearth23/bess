//! Cylinder mass-flow boundaries and separate primaries joined to two exhausts.
//! Linear acoustic perturbations ride on the 0D mean state. This is not 1D CFD.
use crate::{
    acoustics::{ExhaustLayout, ExhaustNetwork, Geometry},
    engine_build::{Catalyst, Crossover, EngineBuild, Headers, Muffler},
    scratch::{EngineDesign, SoundTuning},
};
use bdsp::delay::DelayLine;

struct Primary {
    forward: DelayLine,
    backward: DelayLine,
    length_m: f64,
    target_length_m: f64,
    base_length_m: f64,
    bank_extension_m: f64,
    area_m2: f64,
    bank: usize,
    previous_flow: f64,
    ac_flow: f64,
    forward_state: f64,
    backward_state: f64,
    mean_flow: f64,
    loss: f64,
}

pub struct Acoustic {
    primaries: Vec<Primary>,
    tails: [ExhaustNetwork; 2],
    rate: f64,
    tail_area: f64,
    feedback: [f64; 12],
    temperatures: [f64; 2],
    geometry: Geometry,
    muffler_volume_scale: f32,
    tick: u32,
    filter: f64,
    pole: f64,
    mean_slew: f64,
    length_slew: f64,
    crossover: Option<([DelayLine; 2], f64, f64)>,
}

impl Acoustic {
    pub fn new(
        rate: u32,
        design: &EngineDesign,
        build: &EngineBuild,
        tuning: &SoundTuning,
    ) -> Self {
        let area = std::f64::consts::PI * (f64::from(build.exhaust_mm) * 0.0005).powi(2);
        let primary_area = std::f64::consts::PI * (f64::from(build.bore_mm) * 0.00021).powi(2);
        let primaries = (0..design.cylinders as usize)
            .map(|i| {
                let bank = design.banks[i] as usize;
                let primary_length = match build.headers {
                    Headers::CastManifold => 0.23 + 0.06 * (i % 4) as f64,
                    Headers::Tubular => 0.55 + 0.04 * (i % 3) as f64,
                    Headers::EqualLength => 0.7,
                };
                // Scale the primary itself; the separate bank timing control
                // retains its requested extra propagation time.
                let bank_extension = if bank == 1 {
                    f64::from(design.bank_delay_ms) * 0.001 * 550.
                } else {
                    0.
                };
                let length =
                    primary_length * f64::from(tuning.primary_length_scale) + bank_extension;
                Primary {
                    forward: DelayLine::new(rate as f32, 0.03),
                    backward: DelayLine::new(rate as f32, 0.03),
                    length_m: length,
                    target_length_m: length,
                    base_length_m: primary_length,
                    bank_extension_m: bank_extension,
                    area_m2: primary_area,
                    bank,
                    previous_flow: 0.,
                    ac_flow: 0.,
                    forward_state: 0.,
                    backward_state: 0.,
                    mean_flow: 0.,
                    loss: 0.96,
                }
            })
            .collect();
        let geometry = Geometry {
            header: 0.05,
            tail: tuning.tail_length_m,
            diameter_mm: build.exhaust_mm,
            chamber_litres: match build.muffler {
                Muffler::None => 0.3,
                Muffler::StraightThrough => 2.5,
                Muffler::Baffled => 6.,
                Muffler::ReverseFlow => 9.,
            },
            absorption: tuning.muffler_absorption,
            resonance: 1.,
            temperature_c: 400.,
        };
        let layout = ExhaustLayout {
            catalyst: build.catalyst != Catalyst::None,
            muffler: match build.muffler {
                Muffler::None => 0,
                Muffler::StraightThrough => 1,
                Muffler::Baffled => 2,
                Muffler::ReverseFlow => 3,
            },
        };
        Self {
            primaries,
            tails: std::array::from_fn(|_| {
                let mut tail = ExhaustNetwork::new(rate as f32, geometry, layout);
                tail.set_chamber_length_scale(tuning.muffler_volume_scale);
                tail.tune(geometry, true);
                tail
            }),
            rate: f64::from(rate),
            tail_area: area,
            feedback: [0.; 12],
            temperatures: [673.; 2],
            geometry,
            muffler_volume_scale: tuning.muffler_volume_scale,
            tick: 0,
            filter: 1. - (-std::f64::consts::TAU * 7000. / f64::from(rate)).exp(),
            pole: (-std::f64::consts::TAU * 12. / f64::from(rate)).exp(),
            mean_slew: 1. - (-1. / (0.12 * f64::from(rate))).exp(),
            length_slew: 1. - (-1. / (0.05 * f64::from(rate))).exp(),
            crossover: match build.crossover {
                Crossover::None => None,
                kind if design.banks[..design.cylinders as usize].contains(&1) => {
                    // H is a narrow balancing tube; X joins through a short,
                    // full-bore path. Both are passive bidirectional ports.
                    let (length, fraction) = if kind == Crossover::H {
                        (0.55, 0.25)
                    } else {
                        (0.12, 1.)
                    };
                    Some((
                        std::array::from_fn(|_| DelayLine::new(rate as f32, 0.01)),
                        length,
                        area * fraction,
                    ))
                }
                _ => None,
            },
        }
    }

    /// Retarget acoustic dimensions without clearing delayed waves, filters,
    /// mean flow or sample clocks. Primary lengths approach targets over 50 ms;
    /// downstream tubes retain their existing internal delay smoothing.
    pub fn retune(&mut self, tuning: &SoundTuning) {
        for primary in &mut self.primaries {
            primary.target_length_m = primary.base_length_m
                * f64::from(tuning.primary_length_scale)
                + primary.bank_extension_m;
        }
        if self.geometry.tail == tuning.tail_length_m
            && self.geometry.absorption == tuning.muffler_absorption
            && self.muffler_volume_scale == tuning.muffler_volume_scale
        {
            return;
        }
        self.geometry.tail = tuning.tail_length_m;
        self.geometry.absorption = tuning.muffler_absorption;
        self.muffler_volume_scale = tuning.muffler_volume_scale;
        for (bank, tail) in self.tails.iter_mut().enumerate() {
            tail.set_chamber_length_scale(tuning.muffler_volume_scale);
            tail.tune(
                Geometry {
                    temperature_c: (self.temperatures[bank] - 273.15) as f32,
                    ..self.geometry
                },
                false,
            );
        }
    }

    /// Reflected wave at a prescribed-flow port, available for diagnostics.
    /// It is not an explicit pressure input to the nonlinear 0D valve solver.
    pub fn valve_pressure(&self, cylinder: usize) -> f64 {
        self.feedback[cylinder]
    }

    pub fn next(
        &mut self,
        flow_kg_s: &[f64; 12],
        temperatures_k: [f64; 2],
        pressures_pa: [f64; 2],
        reaction_w: [f64; 2],
    ) -> [f32; 2] {
        self.tick += 1;
        for primary in &mut self.primaries {
            primary.length_m += self.length_slew * (primary.target_length_m - primary.length_m);
        }
        for (old, target) in self.temperatures.iter_mut().zip(temperatures_k) {
            *old += (target.clamp(250., 2500.) - *old) / (self.rate * 0.05);
        }
        if self.tick.is_multiple_of((self.rate as u32 / 200).max(1)) {
            for bank in 0..2 {
                self.tails[bank].tune(
                    Geometry {
                        temperature_c: (self.temperatures[bank] - 273.15) as f32,
                        ..self.geometry
                    },
                    false,
                );
            }
            for primary in &mut self.primaries {
                let speed = (1.33 * 287. * self.temperatures[primary.bank]).sqrt();
                primary.loss = 0.001_f64.powf((primary.length_m / speed) / 0.12);
            }
        }
        let speeds = self.temperatures.map(|t| (1.33 * 287. * t).sqrt());
        let densities = std::array::from_fn::<_, 2, _>(|b| {
            pressures_pa[b].max(10000.) / (287. * self.temperatures[b])
        });
        let mut arrivals = [0.; 12];
        let mut returns = [0.; 12];
        let mut weighted = [0.; 2];
        let mut admittance = [0.; 2];
        let mut tail_y = [0.; 2];
        // Read every port before scattering. Reading the tail's previous
        // sample here and advancing it again at launch adds a spurious delay.
        let tail_incoming = self.tails.each_mut().map(ExhaustNetwork::prepare_inlet);
        for b in 0..2 {
            let c = speeds[b];
            let rho = densities[b];
            tail_y[b] = self.tail_area / (rho * c);
            admittance[b] = tail_y[b];
            weighted[b] = tail_y[b] * f64::from(tail_incoming[b]);
            // Volume source from chemical heat release: Qdot*(gamma-1)/(rho*c²).
            // Junction admittances convert this volume velocity to pressure.
            weighted[b] += reaction_w[b] * 0.33 / (rho * c * c) * 0.5;
        }
        for (i, p) in self.primaries.iter_mut().enumerate() {
            let b = p.bank;
            let c = speeds[b];
            let rho = densities[b];
            // Convect acoustic perturbations on the mean flow. Moving the
            // entire delay read head with each valve pulse frequency-modulates
            // waves already in the pipe and counts that pulse twice.
            p.mean_flow += self.mean_slew * (flow_kg_s[i] - p.mean_flow);
            let velocity = (p.mean_flow / (rho * p.area_m2)).clamp(-0.3 * c, 0.3 * c);
            let f = f64::from(
                p.forward
                    .read_at((p.length_m * self.rate / (c + velocity)).max(1.) as f32),
            );
            let r = f64::from(
                p.backward
                    .read_at((p.length_m * self.rate / (c - velocity)).max(1.) as f32),
            );
            p.forward_state += self.filter * (f - p.forward_state);
            p.backward_state += self.filter * (r - p.backward_state);
            arrivals[i] = p.forward_state * p.loss;
            returns[i] = p.backward_state * p.loss;
            self.feedback[i] = returns[i];
            let y = p.area_m2 / (rho * c);
            weighted[b] += y * arrivals[i];
            admittance[b] += y;
        }
        let mut cross_arrivals = [0.; 2];
        if let Some((lines, length, area)) = &self.crossover {
            let speed = 0.5 * (speeds[0] + speeds[1]);
            let density = 0.5 * (densities[0] + densities[1]);
            let y = area / (density * speed);
            for b in 0..2 {
                cross_arrivals[b] =
                    f64::from(lines[1 - b].read_at((length * self.rate / speed).max(1.) as f32))
                        * 0.98;
                weighted[b] += y * cross_arrivals[b];
                admittance[b] += y;
            }
        }
        let junction =
            std::array::from_fn::<_, 2, _>(|b| 2. * weighted[b] / admittance[b].max(1e-12));
        if let Some((lines, _, _)) = &mut self.crossover {
            for b in 0..2 {
                lines[b].write((junction[b] - cross_arrivals[b]) as f32);
            }
        }
        for (i, p) in self.primaries.iter_mut().enumerate() {
            let c = speeds[p.bank];
            // Remove the slowly varying mean already represented by 0D mass.
            p.ac_flow = flow_kg_s[i] - p.previous_flow + self.pole * p.ac_flow;
            p.previous_flow = flow_kg_s[i];
            let excitation = c / p.area_m2 * p.ac_flow;
            // Prescribed volume flow Q = (p+ - p-)/Z: p+ = Z*Q + p-.
            // At zero perturbation flow this is a rigid, lossless termination;
            // multiplying the return by an arbitrary gain removes low modes.
            p.forward
                .write((excitation + returns[i]).clamp(-200000., 200000.) as f32);
            p.backward
                .write((junction[p.bank] - arrivals[i]).clamp(-200000., 200000.) as f32);
        }
        std::array::from_fn(|b| {
            let incoming = f64::from(tail_incoming[b]);
            self.tails[b].next_coupled((junction[b] - incoming) as f32)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::PRESETS;

    #[test]
    fn retune_preserves_running_state_and_slews_primary_lengths() {
        let design = PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1;
        let build = EngineBuild::default();
        let mut reference = Acoustic::new(48000, &design, &build, &SoundTuning::default());
        let mut tuned = Acoustic::new(48000, &design, &build, &SoundTuning::default());
        for sample in 0..4800 {
            let mut flow = [0.; 12];
            flow[0] = (sample as f64 * 0.11).sin() * 1e-5;
            tuned.retune(&SoundTuning::default());
            assert_eq!(
                reference.next(&flow, [673.; 2], [101325.; 2], [0.; 2]),
                tuned.next(&flow, [673.; 2], [101325.; 2], [0.; 2])
            );
        }
        let before_length = tuned.primaries[0].length_m;
        let before_clock = tuned.tick;
        let before_wave = tuned.primaries[0].forward.read_at(10.);
        let before_mean = tuned.primaries[0].mean_flow;
        let setting = SoundTuning {
            primary_length_scale: 2.,
            tail_length_m: 5.,
            muffler_volume_scale: 3.,
            muffler_absorption: 1.,
            ..Default::default()
        };
        tuned.retune(&setting);
        assert_eq!(tuned.primaries[0].length_m, before_length);
        assert_eq!(tuned.tick, before_clock);
        assert_eq!(tuned.primaries[0].forward.read_at(10.), before_wave);
        assert_eq!(tuned.primaries[0].mean_flow, before_mean);
        let target = tuned.primaries[0].target_length_m;
        let (mut early, mut late) = (0., 0.);
        for sample in 0..24000 {
            let out = tuned.next(&[0.; 12], [673.; 2], [101325.; 2], [0.; 2]);
            assert!(out.iter().all(|x| x.is_finite()));
            if sample == 2399 {
                let remaining = (target - tuned.primaries[0].length_m) / (target - before_length);
                assert!((remaining - (-1_f64).exp()).abs() < 1e-10);
            }
            let energy = out.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
            if sample < 6000 {
                early += energy;
            }
            if sample > 18000 {
                late += energy;
            }
        }
        assert!(early > 0. && late < early * 1e-5);
    }

    fn response(rate: u32, tuning: &SoundTuning) -> Vec<f64> {
        let design = PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1;
        let mut network = Acoustic::new(rate, &design, &EngineBuild::default(), tuning);
        let cylinder = design.banks[..8]
            .iter()
            .position(|bank| *bank == 0)
            .unwrap();
        (0..rate / 2)
            .map(|sample| {
                let mut flow = [0.; 12];
                if sample == 0 {
                    flow[cylinder] = 1e-5;
                }
                let output = network.next(&flow, [673.; 2], [101325.; 2], [0.; 2]);
                assert!(output.iter().all(|value| value.is_finite()));
                f64::from(output[0])
            })
            .collect()
    }

    #[test]
    fn tuning_defaults_preserve_existing_acoustic_dimensions() {
        let design = PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1;
        let build = EngineBuild::default();
        let network = Acoustic::new(48000, &design, &build, &SoundTuning::default());
        assert_eq!(network.geometry.tail.to_bits(), 1.4_f32.to_bits());
        assert_eq!(network.geometry.absorption.to_bits(), 0.4_f32.to_bits());
        let volume: f32 = match build.muffler {
            Muffler::None => 0.3,
            Muffler::StraightThrough => 2.5,
            Muffler::Baffled => 6.,
            Muffler::ReverseFlow => 9.,
        };
        assert_eq!(network.geometry.chamber_litres.to_bits(), volume.to_bits());
        for (i, primary) in network.primaries.iter().enumerate() {
            let length = match build.headers {
                Headers::CastManifold => 0.23 + 0.06 * (i % 4) as f64,
                Headers::Tubular => 0.55 + 0.04 * (i % 3) as f64,
                Headers::EqualLength => 0.7,
            } + if primary.bank == 1 {
                f64::from(design.bank_delay_ms) * 0.001 * 550.
            } else {
                0.
            };
            assert_eq!(primary.length_m.to_bits(), length.to_bits());
        }
    }

    #[test]
    fn every_tuning_control_changes_propagation_or_dissipation() {
        let base = response(48000, &SoundTuning::default());
        let base_energy: f64 = base.iter().map(|x| x * x).sum();
        let first = |samples: &[f64]| samples.iter().position(|x| x.abs() > 1e-9).unwrap();
        for (name, tuning) in [
            (
                "primary",
                SoundTuning {
                    primary_length_scale: 2.,
                    ..Default::default()
                },
            ),
            (
                "tail",
                SoundTuning {
                    tail_length_m: 5.,
                    ..Default::default()
                },
            ),
            (
                "volume",
                SoundTuning {
                    muffler_volume_scale: 3.,
                    ..Default::default()
                },
            ),
            (
                "absorption",
                SoundTuning {
                    muffler_absorption: 1.,
                    ..Default::default()
                },
            ),
        ] {
            let changed = response(48000, &tuning);
            let difference: f64 = base
                .iter()
                .zip(&changed)
                .map(|(a, b)| (a - b).powi(2))
                .sum();
            assert!(
                difference > base_energy * 1e-4,
                "{name}: negligible response change"
            );
            if matches!(name, "primary" | "tail") {
                assert!(
                    first(&changed) > first(&base),
                    "{name}: length did not delay arrival"
                );
            }
            println!(
                "{name}: relative squared response difference {}, first arrival {} samples (default {})",
                difference / base_energy,
                first(&changed),
                first(&base)
            );
        }
        let roughness = |samples: &[f64]| {
            samples
                .windows(2)
                .map(|x| (x[1] - x[0]).powi(2))
                .sum::<f64>()
        };
        let low = response(
            48000,
            &SoundTuning {
                muffler_absorption: 0.,
                ..Default::default()
            },
        );
        let high = response(
            48000,
            &SoundTuning {
                muffler_absorption: 1.,
                ..Default::default()
            },
        );
        assert!(
            roughness(&high) < roughness(&low),
            "packing must dissipate upper-frequency energy"
        );
        println!(
            "absorption high/low first-difference energy {}",
            roughness(&high) / roughness(&low)
        );
    }

    #[test]
    fn extreme_tunings_decay_without_self_excitation() {
        for rate in [48000, 96000] {
            for tuning in [
                SoundTuning {
                    primary_length_scale: 0.5,
                    tail_length_m: 0.2,
                    muffler_volume_scale: 0.25,
                    muffler_absorption: 0.,
                    ..Default::default()
                },
                SoundTuning {
                    primary_length_scale: 2.,
                    tail_length_m: 5.,
                    muffler_volume_scale: 3.,
                    muffler_absorption: 1.,
                    ..Default::default()
                },
            ] {
                let samples = response(rate, &tuning);
                let early: f64 = samples[..samples.len() / 2].iter().map(|x| x * x).sum();
                let late: f64 = samples[samples.len() * 3 / 4..].iter().map(|x| x * x).sum();
                assert!(
                    early > 0. && late < early * 1e-5,
                    "{rate}: non-decaying response {late}/{early}"
                );
            }
        }
    }

    #[test]
    fn crossover_transmits_between_banks_and_impulse_decays() {
        let design = PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1;
        for rate in [48000, 96000] {
            for kind in [Crossover::None, Crossover::H, Crossover::X] {
                let build = EngineBuild {
                    crossover: kind,
                    ..Default::default()
                };
                let mut network = Acoustic::new(rate, &design, &build, &SoundTuning::default());
                let cylinder = design.banks[..8].iter().position(|b| *b == 0).unwrap();
                let (mut other, mut early, mut late) = (0., 0., 0.);
                for i in 0..rate {
                    let mut flow = [0.; 12];
                    if i < rate / 1000 {
                        flow[cylinder] = 0.01;
                    }
                    let out = network.next(&flow, [673.; 2], [101325.; 2], [0.; 2]);
                    assert!(out.iter().all(|x| x.is_finite()));
                    let energy = out.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
                    other += f64::from(out[1]).powi(2);
                    if i < rate / 4 {
                        early += energy;
                    }
                    if i > rate * 3 / 4 {
                        late += energy;
                    }
                }
                assert!(early > 0.);
                assert!(
                    late < early * 1e-5,
                    "{kind:?}: growing/stuck tail {late}/{early}"
                );
                if kind == Crossover::None {
                    assert_eq!(other, 0.);
                } else {
                    assert!(other > early * 1e-6, "{kind:?}: no crossover transmission");
                }
            }
        }
    }
}
