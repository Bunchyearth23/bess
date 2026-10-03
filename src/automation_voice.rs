//! Physical Automation B, prepared off the audio thread. The source recording
//! never excites this engine; it remains available separately as reference A.
use crate::{
    automation_model::AutomationModel,
    bank::Bank,
    hybrid::Settings,
    physical::engine::{Commands, Engine, Sample},
    scratch::{Scratch, ScratchModel, ScratchVoice, SoundTuning},
};
use bdsp::filters::{PolyphaseDecimator, generate_lowpass_taps};

pub struct AutomationVoice {
    engine: Box<Engine>,
    outgoing: Option<Box<Engine>>,
    retired: Option<ScratchVoice>,
    fade: f32,
    rate: f32,
    rpm_range: (f32, f32),
    decimators: Option<[PolyphaseDecimator; 3]>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stems {
    pub exhaust: f32,
    pub intake: f32,
    pub mechanical: f32,
}

impl AutomationVoice {
    pub fn from_bank(rate: u32, bank: &Bank, sound: &SoundTuning) -> Result<Self, String> {
        Self::from_settings(
            rate,
            bank,
            Settings {
                physical_sound: *sound,
                ..Default::default()
            },
        )
    }

    pub fn from_settings(rate: u32, bank: &Bank, settings: Settings) -> Result<Self, String> {
        if !bank.min_rpm.is_finite()
            || !bank.max_rpm.is_finite()
            || bank.min_rpm < 0.
            || bank.max_rpm > 12000.
        {
            return Err(
                "Physical Automation resynthesis supports source RPM points up to 12000 RPM".into(),
            );
        }
        let model = AutomationModel::from_settings(bank, &settings)?;
        Self::new(rate, &model.scratch)
    }

    pub fn new(rate: u32, scratch: &Scratch) -> Result<Self, String> {
        if !(8000..=192000).contains(&rate) {
            return Err(
                "Physical Automation audio needs an output rate between 8 and 192 kHz".into(),
            );
        }
        let synth_rate = scratch.synthesis_rate(rate);
        let engine = Box::new(Engine::new(scratch, synth_rate)?);
        let decimators = (synth_rate != rate).then(|| {
            std::array::from_fn(|_| {
                PolyphaseDecimator::new(
                    generate_lowpass_taps(synth_rate as f32, rate as f32 * 0.45, 95),
                    2,
                )
            })
        });
        Ok(Self {
            engine,
            decimators,
            outgoing: None,
            retired: None,
            fade: 1.,
            rate: synth_rate as f32,
            rpm_range: (scratch.idle_rpm, scratch.redline_rpm),
        })
    }

    pub fn set_sound_tuning(&mut self, sound: &SoundTuning) -> bool {
        self.engine.set_sound_tuning(sound)
    }

    pub fn failed(&self) -> bool {
        self.engine.failed()
    }

    pub fn state(&self) -> Sample {
        self.engine.state()
    }

    pub fn inertia(&self) -> f64 {
        self.engine.inertia()
    }

    pub fn rpm_range(&self) -> (f32, f32) {
        self.rpm_range
    }

    pub fn reset(&mut self) {
        self.engine.reset();
        if let Some(outgoing) = &mut self.outgoing {
            outgoing.reset();
        }
    }

    /// Install off-thread prepared storage, keeping gas/shaft state on sound edits.
    pub fn swap_model(&mut self, mut model: ScratchModel) -> Option<ScratchVoice> {
        let Some(prepared) = &mut model.physical else {
            return None;
        };
        self.rpm_range = (model.idle_rpm, model.redline_rpm);
        if self.engine.apply_sound_tuning(prepared) {
            return Some(ScratchVoice::Prepared {
                physical: model.physical,
            });
        }
        let retired = self.outgoing.take().map(ScratchVoice::Physical);
        self.outgoing = Some(std::mem::replace(
            &mut self.engine,
            model.physical.take().expect("prepared engine"),
        ));
        self.fade = 0.;
        retired
    }

    pub fn take_retired(&mut self) -> Option<ScratchVoice> {
        self.retired.take()
    }

    fn physical_sample(&mut self, commands: Commands) -> Sample {
        let mut sample = self.engine.next(commands);
        if let Some(outgoing) = &mut self.outgoing {
            let old = outgoing.next(commands);
            self.fade = (self.fade + 1. / (self.rate * 0.030)).min(1.);
            sample.exhaust = old.exhaust + (sample.exhaust - old.exhaust) * self.fade;
            sample.intake = old.intake + (sample.intake - old.intake) * self.fade;
            sample.mechanical = old.mechanical + (sample.mechanical - old.mechanical) * self.fade;
            if self.fade >= 1. && self.retired.is_none() {
                self.retired = self.outgoing.take().map(ScratchVoice::Physical);
            }
        }
        sample
    }

    /// Two physical steps and three independent unity-DC FIR decimators.
    /// No clipping, gain matching or normalization is applied to raw stems.
    pub fn next(&mut self, rpm: f32, load: f32) -> Stems {
        self.next_commands(Commands {
            imposed_rpm: Some(f64::from(rpm)),
            throttle: f64::from(load),
            ..Default::default()
        })
    }

    pub fn next_commands(&mut self, commands: Commands) -> Stems {
        let a = self.physical_sample(commands);
        if self.decimators.is_none() {
            return Stems {
                exhaust: a.exhaust,
                intake: a.intake,
                mechanical: a.mechanical,
            };
        }
        let b = self.physical_sample(commands);
        let inputs = [
            [a.exhaust, b.exhaust],
            [a.intake, b.intake],
            [a.mechanical, b.mechanical],
        ];
        let mut values = [0.; 3];
        for ((decimator, input), value) in self
            .decimators
            .as_mut()
            .expect("2x path")
            .iter_mut()
            .zip(inputs)
            .zip(&mut values)
        {
            let mut output = [0.];
            decimator.process_block(&input, &mut output);
            *value = output[0];
        }
        Stems {
            exhaust: values[0],
            intake: values[1],
            mechanical: values[2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_rates_include_192khz_and_reject_unsupported_rates() {
        let scratch = Scratch::default();
        assert!(AutomationVoice::new(7999, &scratch).is_err());
        assert!(AutomationVoice::new(192001, &scratch).is_err());
        for rate in [8000, 44100, 48000, 192000] {
            let mut voice = AutomationVoice::new(rate, &scratch).unwrap();
            for _ in 0..rate / 50 {
                let stems = voice.next(1200., 0.35);
                assert!(
                    [stems.exhaust, stems.intake, stems.mechanical]
                        .iter()
                        .all(|v| v.is_finite())
                );
            }
            assert!(!voice.failed(), "{rate} Hz");
        }
    }
}
