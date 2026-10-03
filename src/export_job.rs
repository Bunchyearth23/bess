//! Thread-safe, read-only progress snapshots and cooperative export cancellation.
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

pub(crate) const CANCELLED: &str = "Export cancelled";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStage {
    #[default]
    Preparing,
    Rendering,
    Packaging,
    Verifying,
    Complete,
    Cancelled,
    Failed,
}

impl ExportStage {
    fn terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Cancelled | Self::Failed)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ExportProgress {
    pub stage: ExportStage,
    pub completed: usize,
    pub total: usize,
    pub current_rpm: Option<f32>,
    pub current_load: Option<f32>,
    pub workers: usize,
    pub elapsed_seconds: f64,
    /// Estimate for the current phase, based on completed render work.
    pub estimated_remaining_seconds: Option<f64>,
    pub fraction: Option<f32>,
    pub detail: String,
}

struct State {
    progress: ExportProgress,
    started: Instant,
    phase_started: Instant,
    ended: Option<Instant>,
    render_fraction: Vec<f32>,
}

struct Inner {
    state: Mutex<State>,
    cancelled: AtomicBool,
    worker_limit: Option<usize>,
}

#[derive(Clone)]
pub struct ExportJob(Arc<Inner>);

impl Default for ExportJob {
    fn default() -> Self {
        Self::new(None)
    }
}

impl ExportJob {
    fn new(worker_limit: Option<usize>) -> Self {
        let now = Instant::now();
        Self(Arc::new(Inner {
            state: Mutex::new(State {
                progress: ExportProgress {
                    detail: "Preparing vehicle export".into(),
                    ..Default::default()
                },
                started: now,
                phase_started: now,
                ended: None,
                render_fraction: Vec::new(),
            }),
            cancelled: AtomicBool::new(false),
            worker_limit,
        }))
    }

    /// Determinism tests and performance probes only; the application uses the
    /// automatic limit, which leaves a logical CPU available for its interface.
    #[doc(hidden)]
    pub fn with_worker_limit(workers: usize) -> Self {
        Self::new(Some(workers.clamp(1, 8)))
    }

    pub fn snapshot(&self) -> ExportProgress {
        let state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut progress = state.progress.clone();
        progress.elapsed_seconds = state
            .ended
            .unwrap_or_else(Instant::now)
            .duration_since(state.started)
            .as_secs_f64();
        if progress.stage == ExportStage::Rendering
            && let Some(fraction) = progress.fraction
            && fraction > 0.
            && fraction < 1.
        {
            let elapsed = state.phase_started.elapsed().as_secs_f64();
            if elapsed >= 0.5 {
                progress.estimated_remaining_seconds =
                    Some(elapsed * (1. - f64::from(fraction)) / f64::from(fraction));
            }
        }
        progress
    }

    pub fn cancel(&self) {
        let state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if !state.progress.stage.terminal() {
            self.0.cancelled.store(true, Ordering::Release);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(CANCELLED.into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn worker_count(&self, tasks: usize) -> usize {
        let available = std::thread::available_parallelism().map_or(1, usize::from);
        self.0
            .worker_limit
            .unwrap_or_else(|| available.saturating_sub(1).clamp(1, 8))
            .min(tasks.max(1))
    }

    pub(crate) fn begin(
        &self,
        stage: ExportStage,
        total: usize,
        workers: usize,
        detail: impl Into<String>,
    ) {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.progress.stage.terminal() {
            return;
        }
        state.phase_started = Instant::now();
        state.progress.stage = stage;
        state.progress.completed = 0;
        state.progress.total = total;
        state.progress.workers = workers;
        state.progress.current_rpm = None;
        state.progress.current_load = None;
        state.progress.fraction = (total > 0).then_some(0.);
        state.progress.estimated_remaining_seconds = None;
        state.progress.detail = detail.into();
        state.render_fraction = if stage == ExportStage::Rendering {
            vec![0.; total]
        } else {
            Vec::new()
        };
    }

    pub(crate) fn advance(&self, completed: usize, detail: impl Into<String>) {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.progress.stage.terminal() {
            return;
        }
        state.progress.completed = state
            .progress
            .completed
            .max(completed.min(state.progress.total));
        state.progress.fraction = (state.progress.total > 0)
            .then(|| state.progress.completed as f32 / state.progress.total as f32);
        state.progress.detail = detail.into();
    }

    pub(crate) fn render_step(
        &self,
        index: usize,
        done: usize,
        total: usize,
        rpm: f32,
        load: f32,
        detail: &str,
    ) {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.progress.stage != ExportStage::Rendering {
            return;
        }
        if let Some(fraction) = state.render_fraction.get_mut(index) {
            *fraction = fraction.max((done as f32 / total.max(1) as f32).min(1.));
        }
        let fraction =
            state.render_fraction.iter().sum::<f32>() / state.progress.total.max(1) as f32;
        state.progress.fraction = Some(state.progress.fraction.unwrap_or(0.).max(fraction.min(1.)));
        state.progress.current_rpm = Some(rpm);
        state.progress.current_load = Some(load);
        state.progress.detail = detail.to_owned();
    }

    pub(crate) fn rendered(&self, index: usize, rpm: f32, load: f32) {
        self.render_step(index, 1, 1, rpm, load, "Engine loop rendered");
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.progress.stage != ExportStage::Rendering {
            return;
        }
        state.progress.completed = (state.progress.completed + 1).min(state.progress.total);
    }

    pub(crate) fn finish(&self, result: &Result<String, String>) {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.progress.stage.terminal() {
            return;
        }
        state.progress.stage = match result {
            Ok(_) => ExportStage::Complete,
            Err(error) if error == CANCELLED => ExportStage::Cancelled,
            Err(_) => ExportStage::Failed,
        };
        state.progress.detail = match result {
            Ok(detail) | Err(detail) => detail.clone(),
        };
        state.ended = Some(Instant::now());
        state.progress.estimated_remaining_seconds = None;
        if result.is_ok() {
            state.progress.completed = state.progress.total;
            state.progress.fraction = Some(1.);
        }
    }

    /// Serialize the final, verified archive publication against Cancel. Once
    /// publication succeeds, a later click cannot turn a saved export into a
    /// cancellation message or trigger cleanup of its completed files.
    pub(crate) fn publish(
        &self,
        publish: impl FnOnce() -> Result<String, String>,
    ) -> Result<String, String> {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        self.check()?;
        let result = publish();
        if let Ok(detail) = &result {
            state.progress.stage = ExportStage::Complete;
            state.progress.completed = state.progress.total;
            state.progress.fraction = Some(1.);
            state.progress.estimated_remaining_seconds = None;
            state.progress.detail = detail.clone();
            state.ended = Some(Instant::now());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_progress_is_monotonic_and_phase_scoped() {
        let job = ExportJob::with_worker_limit(4);
        job.begin(ExportStage::Rendering, 4, 4, "Rendering");
        std::thread::scope(|scope| {
            for index in 0..4 {
                let job = &job;
                scope.spawn(move || {
                    for step in 0..=40 {
                        job.render_step(index, step, 40, 1000. + index as f32, 0., "Rendering");
                    }
                    job.rendered(index, 1000., 0.);
                });
            }
        });
        let progress = job.snapshot();
        assert_eq!(progress.completed, 4);
        assert_eq!(progress.fraction, Some(1.));
        assert!(progress.elapsed_seconds >= 0.);
        job.begin(ExportStage::Packaging, 5, 4, "Packaging");
        job.advance(3, "Three entries");
        job.advance(2, "An older update");
        assert_eq!(job.snapshot().completed, 3);
        assert_eq!(job.snapshot().fraction, Some(0.6));
        job.cancel();
        job.finish(&Err(CANCELLED.into()));
        assert_eq!(job.snapshot().stage, ExportStage::Cancelled);
        assert_ne!(job.snapshot().fraction, Some(1.));
    }

    #[test]
    fn publication_wins_over_a_late_cancel_but_errors_are_not_hidden() {
        let complete = ExportJob::default();
        complete
            .publish(|| Ok("Saved and verified".into()))
            .unwrap();
        complete.cancel();
        assert_eq!(complete.snapshot().stage, ExportStage::Complete);
        assert!(!complete.is_cancelled());
        let failed = ExportJob::default();
        failed.cancel();
        failed.finish(&Err("Target is not writable".into()));
        assert_eq!(failed.snapshot().stage, ExportStage::Failed);
        assert_eq!(failed.snapshot().detail, "Target is not writable");
    }
}
