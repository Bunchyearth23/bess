use bess::{
    bank::Bank,
    bench::{AuditionMix, BeamNgCamera, Bench},
    drive::Controls,
    hybrid::Settings,
    project::Parameters,
    realtime::{
        AudioReader, AudioWorker, CommandSender, DenormalGuard, PipeStats, RenderEngine,
        command_ring,
    },
    scratch::{Scratch, ScratchModel, ScratchVoice},
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
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

fn preferred_buffer(rate: u32, supported: &cpal::SupportedBufferSize) -> u32 {
    let preferred = (rate / 100).max(1);
    match *supported {
        cpal::SupportedBufferSize::Range { min, max } => preferred.clamp(min, max),
        cpal::SupportedBufferSize::Unknown => preferred,
    }
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
    pub physical_failed: AtomicBool,
    /// 1%-of-deadline upper-bound buckets (1..=100), then >100% overflow.
    callback_budget_histogram: [AtomicU64; 101],
    /// Contiguous recent output (f32 bits) for the spectrum; `scope_end` is
    /// the index one past the newest sample.
    pub scope: Box<[AtomicU32; crate::spectrum::WINDOW]>,
    pub scope_end: AtomicUsize,
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
            physical_failed: AtomicBool::new(false),
            callback_budget_histogram: std::array::from_fn(|_| AtomicU64::new(0)),
            scope: Box::new(std::array::from_fn(|_| AtomicU32::new(0))),
            scope_end: AtomicUsize::new(0),
        }
    }
}
impl Meter {
    fn record_callback(&self, elapsed_ns: u64, budget_ns: u64) {
        self.max_ns.fetch_max(elapsed_ns, Ordering::Relaxed);
        self.blocks.fetch_add(1, Ordering::Relaxed);
        if elapsed_ns > budget_ns {
            self.overruns.fetch_add(1, Ordering::Relaxed);
        }
        let upper_percent = elapsed_ns.saturating_mul(100).div_ceil(budget_ns.max(1));
        let bucket = upper_percent.saturating_sub(1).min(100) as usize;
        self.callback_budget_histogram[bucket].fetch_add(1, Ordering::Relaxed);
    }

    /// Approximate callback p99 as a bucket's upper deadline percentage.
    /// 0 means no callbacks; 101 explicitly means the >100% overflow bin.
    /// A concurrently running callback can add one sample during this snapshot.
    pub fn callback_p99_budget_percent(&self) -> u32 {
        let buckets: [u64; 101] =
            std::array::from_fn(|i| self.callback_budget_histogram[i].load(Ordering::Relaxed));
        let count: u64 = buckets.iter().sum();
        if count == 0 {
            return 0;
        }
        let target = count.saturating_mul(99).div_ceil(100);
        let mut cumulative = 0;
        for (i, value) in buckets.into_iter().enumerate() {
            cumulative += value;
            if cumulative >= target {
                return i as u32 + 1;
            }
        }
        101
    }
}
pub struct Audio {
    _stream: cpal::Stream,
    _worker: AudioWorker,
    pub tx: CommandSender<Command>,
    pub meter: Arc<Meter>,
    pub description: String,
    pub rate: u32,
    /// Rate used to prepare replacement scratch voices (twice device rate).
    pub synth_rate: u32,
    pub pipe_stats: Arc<PipeStats>,
    pub buffer_ms: f32,
    pub block_ms: f32,
    /// Rebuilt scratch voices, prepared off the callback at `synth_rate`.
    pub swap: Sender<ScratchModel>,
    /// Voices displaced by a swap; drain and drop them on the UI thread.
    pub trash: Receiver<ScratchVoice>,
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
        Self::open(|rate| {
            Ok(RenderEngine::native(
                Bench::new(rate, params, settings, driving, bank),
                rate,
            ))
        })
    }
    pub fn with_scratch(
        params: Parameters,
        settings: Settings,
        scratch: &Scratch,
        driving: Controls,
    ) -> Result<Self, String> {
        Self::open(|rate| RenderEngine::scratch(rate, params, settings, driving, scratch))
    }
    fn open(make: impl FnOnce(u32) -> Result<RenderEngine, String>) -> Result<Self, String> {
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
        let mut description = format!(
            "Default output · {} Hz · {} channels · {}",
            rate,
            supported.channels(),
            format
        );
        let mut config = supported.config();
        let requested_frames = preferred_buffer(rate, supported.buffer_size());
        config.buffer_size = cpal::BufferSize::Fixed(requested_frames);
        let channels = config.channels as usize;
        let meter = Arc::new(Meter::default());
        let error_meter = meter.clone();
        let (tx, mut rx) = command_ring();
        let mut playing = false;
        // Build typed streams so the default device need not accept f32.
        // Control mailboxes are consumed only on the producer, never by CPAL.
        let mut engine = make(rate)?;
        let synth_rate = engine.synth_rate;
        let (swap, swap_rx) = bounded::<ScratchModel>(1);
        let (trash_tx, trash) = bounded::<ScratchVoice>(8);
        let producer_meter = meter.clone();
        let render = move |block: &mut [f32]| {
            if let Ok(command) = rx.pop() {
                let command: Command = command;
                engine.bench.set_audition_mix(command.audition_mix);
                engine.bench.set_beamng_camera(command.camera);
                engine.bench.set(
                    command.params,
                    command.settings,
                    command.driving,
                    command.restart,
                );
                playing = command.playing;
            }
            if let Ok(model) = swap_rx.try_recv()
                && let Some(old) = engine.bench.swap_scratch(model)
            {
                // A full UI trash slot drops on this producer, never CPAL.
                let _ = trash_tx.try_send(old);
            }
            for sample in block {
                *sample = engine.next_sample(playing);
            }
            if engine.bench.failed() {
                producer_meter.failed.store(true, Ordering::Relaxed);
                producer_meter
                    .physical_failed
                    .store(true, Ordering::Relaxed);
            }
            while let Some(old) = engine.bench.take_retired() {
                let _ = trash_tx.try_send(old);
            }
            let state = engine.bench.state();
            producer_meter
                .speed
                .store(state.speed_kmh.to_bits(), Ordering::Relaxed);
            producer_meter
                .gear
                .store(state.gear as u32, Ordering::Relaxed);
            producer_meter
                .torque
                .store(state.wheel_torque.to_bits(), Ordering::Relaxed);
            producer_meter
                .resistance
                .store(state.resisting_torque.to_bits(), Ordering::Relaxed);
            producer_meter.drive_flags.store(
                u32::from(state.shifting)
                    | (u32::from(state.limited) << 1)
                    | (u32::from(state.shift_rejected) << 2),
                Ordering::Relaxed,
            );
            producer_meter
                .rpm
                .store(state.rpm.to_bits(), Ordering::Relaxed);
            producer_meter
                .load
                .store(state.load.to_bits(), Ordering::Relaxed);
        };
        // Negotiate the stream before sizing the pipe. The setup slot is filled
        // before play(); CPAL only moves the already allocated consumer once.
        let (mut setup_tx, mut setup_rx) = rtrb::RingBuffer::<AudioReader>::new(1);
        let mut reader: Option<AudioReader> = None;
        let callback_meter = meter.clone();
        let mut scope_index = 0usize;
        macro_rules! stream {
            ($ty:ty, $convert:expr) => {{
                device.build_output_stream(
                    config,
                    move |data: &mut [$ty], _: &cpal::OutputCallbackInfo| {
                        let _denormals = DenormalGuard::enter();
                        let started = std::time::Instant::now();
                        if reader.is_none() {
                            reader = setup_rx.pop().ok();
                        }
                        if reader.as_ref().is_some_and(AudioReader::failed) {
                            callback_meter.failed.store(true, Ordering::Relaxed);
                        }
                        let mut peak = 0.0f32;
                        for frame in data.chunks_mut(channels) {
                            let sample = reader.as_mut().map_or(0., AudioReader::next_sample);
                            // Never hand NaN/inf to the device; silence until Reconnect audio.
                            let sample = if sample.is_finite() { sample } else { 0. };
                            peak = peak.max(sample.abs());
                            callback_meter.scope[scope_index % crate::spectrum::WINDOW]
                                .store(sample.to_bits(), Ordering::Relaxed);
                            scope_index = (scope_index + 1) % crate::spectrum::WINDOW;
                            for slot in frame {
                                *slot = ($convert)(sample);
                            }
                        }
                        callback_meter
                            .scope_end
                            .store(scope_index, Ordering::Release);
                        callback_meter.peak.store(peak.to_bits(), Ordering::Relaxed);
                        let elapsed = started.elapsed().as_nanos() as u64;
                        callback_meter.record_callback(
                            elapsed,
                            1_000_000_000 * (data.len() / channels) as u64 / rate as u64,
                        );
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
        .map_err(|e| format!("Could not open {requested_frames}-frame audio stream: {e}"))?;
        let callback_frames = stream.buffer_size().unwrap_or(requested_frames).max(1);
        let (worker, reader) = AudioWorker::for_callback(rate, callback_frames as usize, render)?;
        let pipe_stats = worker.stats.clone();
        let buffer_ms = worker.capacity_frames as f32 * 1000. / rate as f32;
        let block_ms = worker.block_frames as f32 * 1000. / rate as f32;
        description.push_str(&format!(" · {:.1} ms queue", buffer_ms));
        setup_tx
            .push(reader)
            .map_err(|_| "Audio reader setup slot full")?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            _worker: worker,
            tx,
            meter,
            description,
            rate,
            synth_rate,
            pipe_stats,
            buffer_ms,
            block_ms,
            swap,
            trash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_percentile_is_upper_bound_and_exposes_overflow() {
        let meter = Meter::default();
        assert_eq!(meter.callback_p99_budget_percent(), 0);
        for _ in 0..99 {
            meter.record_callback(410_001, 1_000_000);
        }
        meter.record_callback(2_000_000, 1_000_000);
        assert_eq!(meter.callback_p99_budget_percent(), 42);
        assert_eq!(meter.overruns.load(Ordering::Relaxed), 1);
        for _ in 0..2 {
            meter.record_callback(2_000_000, 1_000_000);
        }
        assert_eq!(meter.callback_p99_budget_percent(), 101);
    }

    #[test]
    fn callback_histogram_uses_each_blocks_actual_deadline() {
        let meter = Meter::default();
        meter.record_callback(100, 1000);
        meter.record_callback(200, 2000);
        assert_eq!(meter.callback_p99_budget_percent(), 10);
        assert_eq!(meter.blocks.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn buffer_request_respects_known_limits_and_bounds_unknown_devices() {
        use cpal::SupportedBufferSize;
        assert_eq!(preferred_buffer(48_000, &SupportedBufferSize::Unknown), 480);
        assert_eq!(
            preferred_buffer(
                48_000,
                &SupportedBufferSize::Range {
                    min: 1024,
                    max: 2048
                }
            ),
            1024
        );
        assert_eq!(
            preferred_buffer(48_000, &SupportedBufferSize::Range { min: 64, max: 256 }),
            256
        );
    }

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
