//! Physical Automation B, prepared off the audio thread. The source recording
//! never excites this engine; it remains available separately as reference A.
use crate::{
    automation_model::AutomationModel,
    bank::Bank,
    physical::engine::{Commands, Engine},
    scratch::{Scratch, SoundTuning},
};
use bdsp::filters::{PolyphaseDecimator, generate_lowpass_taps};

pub struct AutomationVoice {
    engine: Engine,
    decimators: [PolyphaseDecimator; 3],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stems {
    pub exhaust: f32,
    pub intake: f32,
    pub mechanical: f32,
}

impl AutomationVoice {
    pub fn from_bank(rate: u32, bank: &Bank, sound: &SoundTuning) -> Result<Self, String> {
        if !bank.min_rpm.is_finite()
            || !bank.max_rpm.is_finite()
            || bank.min_rpm < 0.
            || bank.max_rpm > 12000.
        {
            return Err(
                "Physical Automation resynthesis supports source RPM points up to 12000 RPM".into(),
            );
        }
        let mut model = AutomationModel::from_bank(bank)?;
        model.scratch.sound = *sound;
        Self::new(rate, &model.scratch)
    }

    pub fn new(rate: u32, scratch: &Scratch) -> Result<Self, String> {
        if !(8000..=192000).contains(&rate) {
            return Err(
                "Physical Automation audio needs an output rate between 8 and 192 kHz".into(),
            );
        }
        let synth_rate = rate * 2;
        let engine = Engine::new(scratch, synth_rate)?;
        let decimators = std::array::from_fn(|_| {
            PolyphaseDecimator::new(
                generate_lowpass_taps(synth_rate as f32, rate as f32 * 0.45, 95),
                2,
            )
        });
        Ok(Self { engine, decimators })
    }

    pub fn set_sound_tuning(&mut self, sound: &SoundTuning) -> bool {
        self.engine.set_sound_tuning(sound)
    }

    pub fn failed(&self) -> bool {
        self.engine.failed()
    }

    /// Two physical steps and three independent unity-DC FIR decimators.
    /// No clipping, gain matching or normalization is applied to raw stems.
    pub fn next(&mut self, rpm: f32, load: f32) -> Stems {
        let commands = Commands {
            imposed_rpm: Some(f64::from(rpm)),
            throttle: f64::from(load),
            ..Default::default()
        };
        let a = self.engine.next(commands);
        let b = self.engine.next(commands);
        let inputs = [
            [a.exhaust, b.exhaust],
            [a.intake, b.intake],
            [a.mechanical, b.mechanical],
        ];
        let mut values = [0.; 3];
        for ((decimator, input), value) in self.decimators.iter_mut().zip(inputs).zip(&mut values) {
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
