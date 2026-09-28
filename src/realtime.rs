//! Deterministic scratch rendering and a bounded producer-to-device audio pipe.
//! Synthesis and voice destruction belong to the producer; the device only pops
//! samples. The same render engine is used without a worker for WAV export.
use crate::{
    bench::Bench,
    drive::Controls,
    hybrid::Settings,
    project::Parameters,
    scratch::{Scratch, ScratchModel},
};
use bdsp::filters::{PolyphaseDecimator, generate_lowpass_taps};
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Single UI-thread writer. Interior mutability preserves the existing
/// `audio.tx.try_send(command)` API without a lock or an MPMC command channel.
/// This is deliberately not Sync: a second producer is not supported.
pub struct CommandSender<T>(RefCell<Producer<T>>);

impl<T> CommandSender<T> {
    pub fn try_send(&self, command: T) -> Result<(), PushError<T>> {
        self.0.borrow_mut().push(command)
    }
}

pub fn command_ring<T>() -> (CommandSender<T>, Consumer<T>) {
    let (producer, consumer) = RingBuffer::new(1);
    (CommandSender(RefCell::new(producer)), consumer)
}

/// Scoped x86-64 FTZ/DAZ. Restore the host thread's floating-point environment
/// after a callback or offline render; never change the UI's environment.
/// Other architectures retain their platform default.
pub struct DenormalGuard {
    #[cfg(target_arch = "x86_64")]
    previous: u32,
    _thread_bound: std::marker::PhantomData<*mut ()>,
}

impl DenormalGuard {
    #[allow(deprecated)]
    pub fn enter() -> Self {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: SSE2 and MXCSR are part of the x86-64 baseline. Only the
        // documented flush-to-zero and denormals-are-zero bits are changed.
        let previous = unsafe {
            let previous = std::arch::x86_64::_mm_getcsr();
            std::arch::x86_64::_mm_setcsr(previous | (1 << 15) | (1 << 6));
            previous
        };
        Self {
            #[cfg(target_arch = "x86_64")]
            previous,
            _thread_bound: std::marker::PhantomData,
        }
    }
}

impl Drop for DenormalGuard {
    #[allow(deprecated)]
    fn drop(&mut self) {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: restores the MXCSR captured on this same thread.
        unsafe {
            std::arch::x86_64::_mm_setcsr(self.previous);
        }
    }
}

/// All synthesis remains outside CPAL, including scratch's 2:1 anti-alias FIR.
pub struct RenderEngine {
    pub bench: Bench,
    decimator: Option<PolyphaseDecimator>,
    pub synth_rate: u32,
}

impl RenderEngine {
    pub fn native(bench: Bench, rate: u32) -> Self {
        Self {
            bench,
            decimator: None,
            synth_rate: rate,
        }
    }

    pub fn scratch(
        output_rate: u32,
        params: Parameters,
        settings: Settings,
        driving: Controls,
        scratch: &Scratch,
    ) -> Result<Self, String> {
        if !(8_000..=192_000).contains(&output_rate) {
            return Err("Scratch audio needs an output rate between 8 and 192 kHz".into());
        }
        params.validate()?;
        settings.validate()?;
        driving.validate()?;
        let synth_rate = output_rate * 2;
        let model = ScratchModel::build(scratch, synth_rate)?;
        Ok(Self {
            bench: Bench::from_scratch(synth_rate, params, settings, driving, model),
            // 95-tap Blackman-Harris low-pass: unity DC, transition before the
            // output Nyquist frequency. Construction allocates only here.
            decimator: Some(PolyphaseDecimator::new(
                generate_lowpass_taps(synth_rate as f32, output_rate as f32 * 0.45, 95),
                2,
            )),
            synth_rate,
        })
    }

    pub fn next_sample(&mut self, playing: bool) -> f32 {
        if let Some(decimator) = &mut self.decimator {
            let input = [self.bench.next(playing), self.bench.next(playing)];
            let mut output = [0.];
            decimator.process_block(&input, &mut output);
            // FIR ringing can overshoot a pre-decimation limiter by a few
            // samples. Keep the existing scratch -1 dBFS output guarantee.
            output[0].clamp(-0.891, 0.891)
        } else {
            self.bench.next(playing)
        }
    }
}

#[derive(Default)]
pub struct PipeStats {
    pub underruns: AtomicU64,
    pub missing_frames: AtomicU64,
    pub produced_blocks: AtomicU64,
    pub max_render_ns: AtomicU64,
    pub failed: AtomicBool,
}

/// The consumer has no engine, commands or heap-owning sample payloads.
pub struct AudioReader {
    consumer: Consumer<f32>,
    stats: Arc<PipeStats>,
    last: f32,
    fade_origin: f32,
    fade_frames: usize,
    missing: usize,
    recovering: usize,
}

impl AudioReader {
    fn new(consumer: Consumer<f32>, stats: Arc<PipeStats>, fade_frames: usize) -> Self {
        Self {
            consumer,
            stats,
            last: 0.,
            fade_origin: 0.,
            fade_frames: fade_frames.max(1),
            missing: 0,
            recovering: 0,
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        match self.consumer.pop() {
            Ok(sample) => {
                let sample = if sample.is_finite() { sample } else { 0. };
                if self.missing != 0 {
                    self.missing = 0;
                    self.fade_origin = self.last;
                    self.recovering = self.fade_frames;
                }
                self.last = if self.recovering > 0 {
                    self.recovering -= 1;
                    let mix = 1. - self.recovering as f32 / self.fade_frames as f32;
                    self.fade_origin + (sample - self.fade_origin) * mix
                } else {
                    sample
                };
            }
            Err(_) => {
                if self.missing == 0 {
                    self.stats.underruns.fetch_add(1, Ordering::Relaxed);
                    self.fade_origin = self.last;
                    self.recovering = 0;
                }
                self.missing = self.missing.saturating_add(1);
                self.stats.missing_frames.fetch_add(1, Ordering::Relaxed);
                self.last = self.fade_origin
                    * (1. - self.missing.min(self.fade_frames) as f32 / self.fade_frames as f32);
            }
        }
        self.last
    }

    pub fn failed(&self) -> bool {
        self.stats.failed.load(Ordering::Relaxed)
    }
}

/// Own on the control/UI thread. Dropping it requests shutdown and joins, so the
/// engine and all pending replacement voices are destroyed by the worker.
pub struct AudioWorker {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    pub stats: Arc<PipeStats>,
    pub block_frames: usize,
    pub capacity_frames: usize,
}

impl AudioWorker {
    pub fn start(
        rate: u32,
        render: impl FnMut(&mut [f32]) + Send + 'static,
    ) -> Result<(Self, AudioReader), String> {
        Self::for_callback(rate, 0, render)
    }

    /// Reserve at least two negotiated device callbacks. Devices with a large
    /// minimum buffer can exceed the preferred 40 ms lead; the caller must
    /// display the resulting latency rather than claiming the usual target.
    pub fn for_callback(
        rate: u32,
        callback_frames: usize,
        mut render: impl FnMut(&mut [f32]) + Send + 'static,
    ) -> Result<(Self, AudioReader), String> {
        if rate < 8_000 {
            return Err("Audio worker rate must be at least 8 kHz".into());
        }
        // Four 10 ms blocks tolerate scheduling jitter with the physical V12.
        // A 30 ms queue observed underruns during concurrent compilation
        // (26.6 ms maximum producer block). Keep latency bounded and report
        // starvation honestly rather than replaying old samples.
        let block_frames = (rate as usize / 100).max(1);
        let capacity_blocks = callback_frames
            .saturating_mul(2)
            .div_ceil(block_frames)
            .max(4);
        if capacity_blocks > 200 {
            return Err("Audio device requests more than two seconds of buffering".into());
        }
        let capacity_frames = block_frames * capacity_blocks;
        let (mut producer, consumer) = RingBuffer::new(capacity_frames);
        let stats = Arc::new(PipeStats::default());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_stats = stats.clone();
        let join = thread::Builder::new()
            .name("bess-audio-producer".into())
            .spawn(move || {
                let _denormals = DenormalGuard::enter();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut block = vec![0.; block_frames];
                    while !worker_stop.load(Ordering::Acquire) {
                        if producer.slots() < block_frames {
                            // Only the worker waits. No unbounded producer lead and
                            // no busy spinning when the device is paused/stopped.
                            thread::sleep(Duration::from_micros(500));
                            continue;
                        }
                        let started = Instant::now();
                        render(&mut block);
                        worker_stats
                            .max_render_ns
                            .fetch_max(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        for &sample in &block {
                            // Consumer can only increase available space. The
                            // checked block fits; no sample payload can allocate.
                            producer.push(sample).expect("reserved audio block fits");
                        }
                        worker_stats.produced_blocks.fetch_add(1, Ordering::Release);
                    }
                }));
                if result.is_err() {
                    worker_stats.failed.store(true, Ordering::Release);
                }
                // The rendering closure (and engine) is dropped here on the worker.
                drop(render);
            })
            .map_err(|e| format!("Could not start audio producer: {e}"))?;
        let worker = Self {
            stop,
            join: Some(join),
            stats: stats.clone(),
            block_frames,
            capacity_frames,
        };
        // Prime before starting CPAL. This wait is on the opening/UI thread,
        // bounded even when synthesis panics or cannot keep up.
        let deadline = Instant::now() + Duration::from_secs(2);
        while stats.produced_blocks.load(Ordering::Acquire) < capacity_blocks as u64 {
            if stats.failed.load(Ordering::Acquire) || Instant::now() >= deadline {
                return Err("Audio producer could not prime its output buffer".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        let reader = AudioReader::new(consumer, stats, (rate as usize / 1000).max(1));
        Ok((worker, reader))
    }
}

impl Drop for AudioWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starvation_fades_to_silence_and_recovers_without_replaying_buffer() {
        let (mut producer, consumer) = RingBuffer::new(16);
        let stats = Arc::new(PipeStats::default());
        let mut reader = AudioReader::new(consumer, stats.clone(), 4);
        producer.push(1.).unwrap();
        assert_eq!(reader.next_sample(), 1.);
        assert_eq!(
            (0..6).map(|_| reader.next_sample()).collect::<Vec<_>>(),
            vec![0.75, 0.5, 0.25, 0., 0., 0.]
        );
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 1);
        assert_eq!(stats.missing_frames.load(Ordering::Relaxed), 6);
        for _ in 0..4 {
            producer.push(-1.).unwrap();
        }
        assert_eq!(
            (0..4).map(|_| reader.next_sample()).collect::<Vec<_>>(),
            vec![-0.25, -0.5, -0.75, -1.]
        );
        producer.push(f32::NAN).unwrap();
        assert_eq!(reader.next_sample(), 0.);
    }

    #[test]
    fn producer_lead_is_bounded_and_full_pipe_shuts_down() {
        let (worker, _reader) = AudioWorker::start(48_000, |block| block.fill(0.25)).unwrap();
        assert_eq!(worker.block_frames, 480);
        assert_eq!(worker.capacity_frames, 1920);
        thread::sleep(Duration::from_millis(20));
        assert_eq!(worker.stats.produced_blocks.load(Ordering::Acquire), 4);
        let started = Instant::now();
        drop(worker);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn oversized_device_callback_gets_two_complete_callbacks_of_capacity() {
        for frames in [480, 1024, 2048] {
            let (worker, mut reader) =
                AudioWorker::for_callback(48_000, frames, |block| block.fill(0.25)).unwrap();
            assert!(worker.capacity_frames >= frames * 2);
            for _ in 0..frames * 2 {
                assert_eq!(reader.next_sample(), 0.25);
            }
            assert_eq!(worker.stats.underruns.load(Ordering::Relaxed), 0);
        }
    }

    #[test]
    fn commands_reach_output_within_one_bounded_queue() {
        let (tx, mut rx) = command_ring();
        let mut level = 0.;
        let (worker, mut reader) = AudioWorker::start(48_000, move |block| {
            if let Ok(next) = rx.pop() {
                level = next;
            }
            block.fill(level);
        })
        .unwrap();
        tx.try_send(0.5).unwrap();
        // Drain exactly the already queued 40 ms. The next rendered block
        // must observe the command, regardless of producer scheduling.
        for _ in 0..worker.capacity_frames {
            assert_eq!(reader.next_sample(), 0.);
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while reader.consumer.slots() == 0 {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert_eq!(reader.next_sample(), 0.5);
        assert_eq!(worker.stats.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn render_owner_is_destroyed_on_worker() {
        struct Owner(std::sync::mpsc::Sender<String>);
        impl Drop for Owner {
            fn drop(&mut self) {
                self.0
                    .send(thread::current().name().unwrap_or("").to_owned())
                    .unwrap();
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let owner = Owner(tx);
        let (worker, _reader) = AudioWorker::start(48_000, move |block| {
            let _keep_owner_alive = &owner;
            block.fill(0.);
        })
        .unwrap();
        drop(worker);
        assert_eq!(rx.recv().unwrap(), "bess-audio-producer");
    }

    #[test]
    fn scratch_worker_matches_wav_samples_bit_for_bit_before_file_fade() {
        let scratch = Scratch::default();
        let params = Parameters::default();
        let settings = Settings::default();
        let driving = Controls::default();
        let expected =
            crate::render::scratch_samples(params, settings, &scratch, 1., driving).unwrap();
        let mut engine =
            RenderEngine::scratch(48_000, params, settings, driving, &scratch).unwrap();
        engine.bench.set_cycle_seconds(1.);
        let (worker, mut reader) = AudioWorker::start(48_000, move |block| {
            for sample in block {
                *sample = engine.next_sample(true);
            }
        })
        .unwrap();
        for (block_index, block) in expected.chunks(worker.block_frames).enumerate() {
            let deadline = Instant::now() + Duration::from_secs(2);
            while reader.consumer.slots() < block.len() {
                assert!(Instant::now() < deadline, "producer stalled");
                thread::yield_now();
            }
            for (offset, &expected) in block.iter().enumerate() {
                let i = block_index * worker.block_frames + offset;
                let fade = ((48_000 - i) as f32 / 2400.).min(1.);
                assert_eq!(
                    (reader.next_sample() * fade).to_bits(),
                    expected.to_bits(),
                    "sample {i}"
                );
            }
        }
        assert_eq!(worker.stats.underruns.load(Ordering::Relaxed), 0);
        assert!(expected.iter().any(|x| x.abs() > 0.001));
    }

    #[test]
    fn scratch_supports_exact_double_rate_at_44k_and_192k() {
        for rate in [44_100, 192_000] {
            let mut engine = RenderEngine::scratch(
                rate,
                Parameters::default(),
                Settings::default(),
                Controls::default(),
                &Scratch::default(),
            )
            .unwrap();
            assert_eq!(engine.synth_rate, rate * 2);
            for _ in 0..2048 {
                let sample = engine.next_sample(true);
                assert!(sample.is_finite() && sample.abs() <= 0.891);
            }
        }
    }

    #[test]
    fn native_bank_path_preserves_every_sample() {
        let make = || {
            Bench::new(
                48_000,
                Parameters::default(),
                Settings::default(),
                Controls::default(),
                None,
            )
        };
        let mut baseline = make();
        let mut wrapped = RenderEngine::native(make(), 48_000);
        for i in 0..4096 {
            let playing = i < 2048;
            assert_eq!(
                baseline.next(playing).to_bits(),
                wrapped.next_sample(playing).to_bits()
            );
        }
    }
}
