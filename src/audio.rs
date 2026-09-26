use bess::{
    bank::Bank,
    bench::{AuditionMix, BeamNgCamera, Bench},
    drive::Controls,
    hybrid::Settings,
    project::Parameters,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Sender, bounded};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};

/// Prefer a 48 kHz stream over a higher-rate device default.
/// The DSP runs in f32; exported WAV precision is selected independently.
fn preferred_output(
    default: cpal::SupportedStreamConfig,
    ranges: &[cpal::SupportedStreamConfigRange],
) -> cpal::SupportedStreamConfig {
    for channels in [default.channels(), 2, 1] {
        for format in [
            cpal::SampleFormat::F32,
            cpal::SampleFormat::I16,
            cpal::SampleFormat::U16,
        ] {
            if let Some(config) = ranges
                .iter()
                .filter(|range| range.channels() == channels && range.sample_format() == format)
                .find_map(|range| range.try_with_sample_rate(48_000))
            {
                return config;
            }
        }
    }
    default
}

pub struct Meter {
    pub max_ns: AtomicU64,
    pub overruns: AtomicU32,
    pub rpm: AtomicU32,
    pub load: AtomicU32,
    pub speed: AtomicU32,
    pub gear: AtomicU32,
    pub torque: AtomicU32,
    pub resistance: AtomicU32,
    pub drive_flags: AtomicU32,
    pub blocks: AtomicU32,
    pub peak: AtomicU32,
    pub failed: AtomicBool,
    pub samples: [AtomicU32; 256],
}
impl Default for Meter {
    fn default() -> Self {
        Self {
            max_ns: AtomicU64::new(0),
            overruns: AtomicU32::new(0),
            rpm: AtomicU32::new(0),
            load: AtomicU32::new(0),
            speed: AtomicU32::new(0),
            gear: AtomicU32::new(0),
            torque: AtomicU32::new(0),
            resistance: AtomicU32::new(0),
            drive_flags: AtomicU32::new(0),
            blocks: AtomicU32::new(0),
            peak: AtomicU32::new(0),
            failed: AtomicBool::new(false),
            samples: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}
pub struct Audio {
    _stream: cpal::Stream,
    pub tx: Sender<Command>,
    pub meter: Arc<Meter>,
    pub description: String,
}
impl Audio {
    pub fn start(params: Parameters) -> Result<Self, String> {
        Self::with_bank(params, Settings::default(), None, Controls::default())
    }
    pub fn with_bank(
        params: Parameters,
        settings: Settings,
        bank: Option<Arc<Bank>>,
        driving: Controls,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device available")?;
        let default = device.default_output_config().map_err(|e| e.to_string())?;
        let ranges: Vec<_> = device
            .supported_output_configs()
            .map(|configs| configs.collect())
            .unwrap_or_default();
        let supported = preferred_output(default, &ranges);
        let rate = supported.sample_rate();
        let format = match supported.sample_format() {
            cpal::SampleFormat::I16 => "16-bit signed",
            cpal::SampleFormat::U16 => "16-bit unsigned",
            cpal::SampleFormat::F32 => "32-bit float",
            _ => "unknown format",
        };
        // Endpoint names come from Windows and can follow the system language.
        // The selected stream format is the relevant diagnostic here.
        let description = format!(
            "Default output · {} Hz · {} channels · {}",
            rate,
            supported.channels(),
            format
        );
        let config = supported.config();
        let channels = config.channels as usize;
        let meter = Arc::new(Meter::default());
        let error_meter = meter.clone();
        let (tx, rx) = bounded(1);
        let mut playing = false;
        // Build typed streams so the default device need not accept f32.
        // Command handling belongs at the block boundary; engine is owned by callback.
        let mut engine = Bench::new(rate, params, settings, driving, bank);
        let callback_meter = meter.clone();
        let mut scope_index = 0usize;
        macro_rules! stream {
            ($ty:ty, $convert:expr) => {{
                device.build_output_stream(
                    config,
                    move |data: &mut [$ty], _: &cpal::OutputCallbackInfo| {
                        let started = std::time::Instant::now();
                        if let Ok(next) = rx.try_recv() {
                            let command: Command = next;
                            engine.set_audition_mix(command.audition_mix);
                            engine.set_beamng_camera(command.camera);
                            engine.set(
                                command.params,
                                command.settings,
                                command.driving,
                                command.restart,
                            );
                            playing = command.playing;
                        }
                        let mut peak = 0.0f32;
                        for frame in data.chunks_mut(channels) {
                            let sample = engine.next(playing);
                            peak = peak.max(sample.abs());
                            if scope_index.is_multiple_of(8) {
                                callback_meter.samples[(scope_index / 8) % 256]
                                    .store(sample.to_bits(), Ordering::Relaxed);
                            }
                            scope_index = scope_index.wrapping_add(1);
                            for slot in frame {
                                *slot = ($convert)(sample);
                            }
                        }
                        callback_meter.peak.store(peak.to_bits(), Ordering::Relaxed);
                        callback_meter.blocks.fetch_add(1, Ordering::Relaxed);
                        let state = engine.state();
                        callback_meter
                            .speed
                            .store(state.speed_kmh.to_bits(), Ordering::Relaxed);
                        callback_meter
                            .gear
                            .store(state.gear as u32, Ordering::Relaxed);
                        callback_meter
                            .torque
                            .store(state.wheel_torque.to_bits(), Ordering::Relaxed);
                        callback_meter
                            .resistance
                            .store(state.resisting_torque.to_bits(), Ordering::Relaxed);
                        callback_meter.drive_flags.store(
                            u32::from(state.shifting)
                                | (u32::from(state.limited) << 1)
                                | (u32::from(state.shift_rejected) << 2),
                            Ordering::Relaxed,
                        );
                        callback_meter
                            .rpm
                            .store(state.rpm.to_bits(), Ordering::Relaxed);
                        callback_meter
                            .load
                            .store(state.load.to_bits(), Ordering::Relaxed);
                        let elapsed = started.elapsed().as_nanos() as u64;
                        callback_meter.max_ns.fetch_max(elapsed, Ordering::Relaxed);
                        if elapsed > 1_000_000_000 * (data.len() / channels) as u64 / rate as u64 {
                            callback_meter.overruns.fetch_add(1, Ordering::Relaxed);
                        }
                    },
                    move |_| {
                        error_meter.failed.store(true, Ordering::Relaxed);
                    },
                    None,
                )
            }};
        }
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => stream!(f32, |s: f32| s),
            cpal::SampleFormat::I16 => stream!(i16, |s: f32| (s * i16::MAX as f32) as i16),
            cpal::SampleFormat::U16 => {
                stream!(u16, |s: f32| ((s * 0.5 + 0.5) * u16::MAX as f32) as u16)
            }
            other => return Err(format!("Unsupported audio format: {other:?}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            tx,
            meter,
            description,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooses_48k_float_and_falls_back_to_device_default() {
        use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};
        let default = cpal::SupportedStreamConfig::new(
            2,
            192_000,
            SupportedBufferSize::Unknown,
            SampleFormat::F32,
        );
        let ranges = [
            SupportedStreamConfigRange::new(
                2,
                44_100,
                192_000,
                SupportedBufferSize::Unknown,
                SampleFormat::F32,
            ),
            SupportedStreamConfigRange::new(
                2,
                44_100,
                192_000,
                SupportedBufferSize::Unknown,
                SampleFormat::I16,
            ),
        ];
        let chosen = preferred_output(default, &ranges);
        assert_eq!(chosen.sample_rate(), 48_000);
        assert_eq!(chosen.sample_format(), SampleFormat::F32);
        assert_eq!(chosen.channels(), 2);
        assert_eq!(preferred_output(default, &ranges[..0]), default);
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct Command {
    pub params: Parameters,
    pub settings: Settings,
    pub playing: bool,
    pub driving: Controls,
    pub restart: u64,
    pub audition_mix: AuditionMix,
    pub camera: BeamNgCamera,
}
