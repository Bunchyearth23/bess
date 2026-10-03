//! Complete vehicle replacement plus a versioned, source-bound BABM audio handoff.
use crate::{
    bank::{self, Bank},
    export_job::{ExportJob, ExportStage},
    hybrid::{Hybrid, Settings},
    project::Parameters,
};
use bdsp::svf::{StateVariableFilter, SvfMode};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

const MAX_SOURCE_WAV_BYTES: u64 = 16_000_000;

fn source_wav(zip: &mut zip::ZipArchive<File>, name: &str) -> Result<(u32, Vec<f32>), String> {
    let entry = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    if entry.size() > MAX_SOURCE_WAV_BYTES {
        return Err(format!("Source WAV is too large: {name}"));
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_SOURCE_WAV_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SOURCE_WAV_BYTES {
        return Err(format!("Source WAV is too large: {name}"));
    }
    bank::decode_wav(&bytes)
}

fn signal_stats(samples: &[f32]) -> Result<(f32, f32), String> {
    if samples.is_empty() {
        return Err("Silent source or rendered exhaust WAV".into());
    }
    // Automation WAVs can contain a large DC offset. Calibrate audible AC
    // energy, while retaining the absolute sample peak for PCM headroom.
    let mut mean = 0f64;
    let mut centered_power = 0f64;
    let mut peak = 0f32;
    for (index, &sample) in samples.iter().enumerate() {
        if !sample.is_finite() {
            return Err("Non-finite source or rendered exhaust WAV".into());
        }
        let delta = sample as f64 - mean;
        mean += delta / (index + 1) as f64;
        centered_power += delta * (sample as f64 - mean);
        peak = peak.max(sample.abs());
    }
    let rms = (centered_power / samples.len() as f64).sqrt() as f32;
    if !rms.is_finite() || rms < 1e-7 || peak < 1e-7 {
        return Err("Silent source or rendered exhaust WAV".into());
    }
    Ok((rms, peak))
}

/// A conservative estimate of level after the active Automation exhaust
/// configuration's 80 Hz low cut. The exact BeamNG filter is not exposed, so
/// this is used only to calibrate the experimental generated exhaust bank.
pub(crate) fn low_cut_rms(samples: &[f32], rate: u32) -> Result<f32, String> {
    if samples.is_empty() || rate < 8000 {
        return Err("Invalid WAV for low-cut calibration".into());
    }
    let mut filter = StateVariableFilter::new(rate as f32, 80., 0.707, SvfMode::Highpass);
    let mut mean = 0f64;
    let mut centered_power = 0f64;
    for (index, &sample) in samples.iter().enumerate() {
        if !sample.is_finite() {
            return Err("Non-finite WAV for low-cut calibration".into());
        }
        let filtered = filter.next_sample(sample) as f64;
        let delta = filtered - mean;
        mean += delta / (index + 1) as f64;
        centered_power += delta * (filtered - mean);
    }
    let rms = (centered_power / samples.len() as f64).sqrt() as f32;
    if !rms.is_finite() || rms < 1e-7 {
        return Err("Silent WAV after low-cut calibration".into());
    }
    Ok(rms)
}

fn vehicle_info_path(name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 3 && parts[0] == "vehicles" && !parts[1].is_empty() && parts[2] == "info.json"
}

fn quoted_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut escaped = false;
    for (i, &byte) in bytes.iter().enumerate().skip(start + 1) {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some(i + 1);
        }
    }
    None
}

/// Change only the top-level display name, retaining the rest of Automation's
/// metadata byte for byte (including any duplicate paint keys).
pub fn label_vehicle_info(bytes: &[u8]) -> Result<(Vec<u8>, String), String> {
    if bytes.len() > 1_000_000 {
        return Err("Vehicle info.json exceeds 1 MB".into());
    }
    let info: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let original = info
        .get("Name")
        .and_then(serde_json::Value::as_str)
        .ok_or("Vehicle info.json has no string Name")?;
    let labelled = if original.ends_with(" (BESS)") {
        original.to_owned()
    } else {
        format!("{original} (BESS)")
    };
    let mut depth = 0usize;
    let mut position = 0;
    let mut range = None;
    while position < bytes.len() {
        match bytes[position] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'"' => {
                let end = quoted_end(bytes, position).ok_or("Unterminated JSON string")?;
                if depth == 1 {
                    let after_key = bytes[end..]
                        .iter()
                        .position(|b| !b.is_ascii_whitespace())
                        .map(|offset| end + offset);
                    if after_key.is_some_and(|i| bytes[i] == b':') {
                        let key: String = serde_json::from_slice(&bytes[position..end])
                            .map_err(|e| e.to_string())?;
                        if key == "Name" {
                            let value = after_key.unwrap() + 1;
                            let value = bytes[value..]
                                .iter()
                                .position(|b| !b.is_ascii_whitespace())
                                .map(|offset| value + offset)
                                .ok_or("Missing vehicle name value")?;
                            let value_end = quoted_end(bytes, value)
                                .ok_or("Vehicle Name is not a JSON string")?;
                            if range.replace(value..value_end).is_some() {
                                return Err("Vehicle info.json has multiple top-level Names".into());
                            }
                        }
                    }
                }
                position = end;
                continue;
            }
            _ => {}
        }
        position += 1;
    }
    let range = range.ok_or("Vehicle info.json has no top-level Name")?;
    let mut output = Vec::with_capacity(bytes.len() + 8);
    output.extend_from_slice(&bytes[..range.start]);
    output.extend(serde_json::to_vec(&labelled).map_err(|e| e.to_string())?);
    output.extend_from_slice(&bytes[range.end..]);
    let checked: serde_json::Value = serde_json::from_slice(&output).map_err(|e| e.to_string())?;
    if checked["Name"] != labelled {
        return Err("Vehicle display name validation failed".into());
    }
    Ok((output, labelled))
}

pub fn package_name(bank: &Bank) -> String {
    let stem = Path::new(&bank.source.archive)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let slug: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("bess-{}-{}.zip", slug, &bank.source.fingerprint[..8])
}

fn aligned_warmup_frames(rpm: f32) -> usize {
    let warmup_cycles = (rpm as f64 / 120.).ceil();
    (warmup_cycles * 120. * 48000. / rpm as f64).round() as usize
}

pub(crate) fn exhaust_safety_gain(peak: f32) -> f32 {
    (0.95 / peak).min(1.)
}

/// Per-knot level calibration for the selectable two-emitter export only.
/// RMS inputs are AC levels measured after subtracting each signal's mean.
/// Inputs must first pass `signal_stats`. Absolute peak safety takes precedence over
/// the 0.5 gain floor when both limits cannot be satisfied together.
#[cfg(test)]
fn exhaust_level_gain(source_rms: f32, exhaust_rms: f32, exhaust_peak: f32) -> f32 {
    exhaust_level_gain_with_floor(source_rms, exhaust_rms, exhaust_peak, 0.5)
}

pub(crate) fn exhaust_level_gain_with_floor(
    source_rms: f32,
    exhaust_rms: f32,
    exhaust_peak: f32,
    floor: f32,
) -> f32 {
    const MINUS_ONE_DB: f32 = 0.891_250_9;
    (MINUS_ONE_DB * source_rms / exhaust_rms)
        .clamp(floor, 2.5)
        .min(0.94 / exhaust_peak)
}

/// Uses an 80 Hz low-cut proxy matching the current Automation fleet JBeam
/// setting. Peak headroom still uses the unfiltered rendered waveform.
pub(crate) fn physical_exhaust_level_gain(
    source: &[f32],
    source_rate: u32,
    rendered: &[f32],
    rendered_peak: f32,
) -> Result<f32, String> {
    Ok(exhaust_level_gain_with_floor(
        low_cut_rms(source, source_rate)?,
        low_cut_rms(rendered, 48_000)?,
        rendered_peak,
        0.25,
    ))
}

pub(crate) type LoopStems = (Vec<f32>, Vec<f32>, Vec<f32>);

pub(crate) fn physical_settings(h: Settings) -> Settings {
    let mut h = h.for_beamng_export();
    h.physical = true;
    h.procedural = false;
    // The source blend's off-load row is a steady running engine. DFCO and
    // starter engagement are driving events, not repeatable loop states.
    h.fuel_cut = 0.;
    h.starter = false;
    h
}

/// Freeze the complete resolved engine for both emitted stems and reports.
/// Legacy projects acquire their import defaults here; edited projects retain
/// every builder, tuning and acoustic value instead of reimporting a baseline.
pub(crate) fn resolved_settings(
    bank: &Bank,
    h: Settings,
) -> Result<(Settings, crate::automation_model::AutomationModel), String> {
    let mut h = physical_settings(h);
    h.validate()?;
    let model = crate::automation_model::AutomationModel::from_settings(bank, &h)?;
    h.engine = Some(crate::engine_definition::EngineDefinition::from_scratch(
        &model.scratch,
    ));
    h.engine_baseline = Some(model.baseline);
    h.physical_sound = model.scratch.sound;
    Ok((h, model))
}

pub(crate) fn loop_policy() -> serde_json::Value {
    json!({
        "mode": "stationary_rpm_load_loops",
        "rpm": "imposed at every original Automation blend knot",
        "off_load": "steady running combustion, fuel cut disabled",
        "starter": "disengaged",
        "accessories": "saved accessory loads are included",
        "supported": "settled combustion, admission, exhaust, mechanics and boost at fixed RPM/load",
        "runtime_events": "BESS starter sequences, shutdown, DFCO, triggered afterfire and turbo spool transients are not encoded by these loops; original BeamNG event references remain in place",
        "vehicle_physics": "original Automation vehicle physics retained; BESS mechanical edits affect generated sound only"
    })
}

/// Old projects without a motor hash migrate once; recorded motor identities
/// must match even when the blend/WAV bytes have not changed.
pub(crate) fn verify_source_identity(
    saved: &bank::SourceRef,
    fresh: &bank::SourceRef,
) -> Result<(), String> {
    if saved.fingerprint != fresh.fingerprint || saved.blend != fresh.blend {
        return Err("Source audio archive changed since import".into());
    }
    if saved.engine_fingerprint.is_some() && saved.engine_fingerprint != fresh.engine_fingerprint {
        return Err(
            "Source engine metadata changed since import; reimport the Automation vehicle".into(),
        );
    }
    Ok(())
}

pub(crate) fn loop_stems(
    bank: Arc<Bank>,
    p: Parameters,
    h: Settings,
    rpm: f32,
    load: f32,
) -> Result<LoopStems, String> {
    render_stems(bank, p, h, rpm, load, 4.)
}

fn render_stems(
    bank: Arc<Bank>,
    p: Parameters,
    h: Settings,
    rpm: f32,
    load: f32,
    engine_seconds: f32,
) -> Result<LoopStems, String> {
    render_stems_controlled(bank, p, h, rpm, load, engine_seconds, None)
}

#[derive(Clone, Copy)]
struct RenderControl<'a> {
    job: &'a ExportJob,
    abort: &'a AtomicBool,
    index: usize,
}

impl RenderControl<'_> {
    fn step(
        self,
        done: usize,
        total: usize,
        rpm: f32,
        load: f32,
        detail: &str,
    ) -> Result<(), String> {
        self.job.check()?;
        if self.abort.load(Ordering::Acquire) {
            return Err("Another export worker failed".into());
        }
        self.job
            .render_step(self.index, done, total, rpm, load, detail);
        Ok(())
    }
}

fn render_stems_controlled(
    bank: Arc<Bank>,
    mut p: Parameters,
    mut h: Settings,
    rpm: f32,
    load: f32,
    engine_seconds: f32,
    control: Option<RenderControl<'_>>,
) -> Result<LoopStems, String> {
    if !rpm.is_finite() || !(200.0..=12_000.0).contains(&rpm) {
        return Err(format!(
            "Unsupported export RPM {rpm}: physical synthesis supports 200–12000 rpm"
        ));
    }
    p.rpm = rpm;
    p.load = load;
    p.volume = 0.8;
    p.validate()?;
    (h, _) = resolved_settings(&bank, h)?;
    h.enhanced = true;
    h.level_match = false;
    if let Some(control) = control {
        control.step(0, 1, rpm, load, "Preparing engine voice")?;
    }
    let mut engine = Hybrid::new(48000, p, h, Some(bank.clone()));
    if let Some(error) = engine.initialization_error() {
        return Err(format!(
            "Cannot construct export voice at {rpm} rpm: {error}"
        ));
    }
    // Start every RPM knot at the same 720-degree phase. A fixed one-second
    // preroll ends at a different crank angle for every RPM and makes adjacent
    // BeamNG samples cancel as they crossfade.
    // The physical gas/thermal state gets at least two seconds to settle.
    // Integer-cycle multiplication retains the same phase alignment.
    const BLOCK: usize = 4096;
    let warmup = aligned_warmup_frames(rpm) * 2;
    let cycle = 48000. * 120. / rpm;
    let frames = (cycle * (2. * 48000. / cycle).ceil()) as usize;
    let engine_frames = (cycle * (engine_seconds * 48000. / cycle).ceil()) as usize;
    let overlap = cycle.round().max(64.) as usize;
    let work = warmup + engine_frames + overlap + 1;
    for start in (0..warmup).step_by(BLOCK) {
        if let Some(control) = control {
            control.step(
                start,
                work,
                rpm,
                load,
                "Settling engine pressure and temperature",
            )?;
        }
        for _ in start..(start + BLOCK).min(warmup) {
            engine.next(true);
        }
    }
    if engine.failed() {
        return Err(format!(
            "Export voice failed during warmup at {rpm} rpm / load {load}"
        ));
    }
    // The engine-side mechanical variation needs longer before it repeats.
    // Its emitter has independent WAVs; keep the exhaust bank compact.
    let count = engine_frames + overlap;
    let mut stems = Vec::with_capacity(count);
    for start in (0..count).step_by(BLOCK) {
        if let Some(control) = control {
            control.step(warmup + start, work, rpm, load, "Generating engine sound")?;
        }
        for _ in start..(start + BLOCK).min(count) {
            stems.push(engine.next_stems(true));
        }
    }
    if let Some(control) = control {
        control.step(
            work - 1,
            work,
            rpm,
            load,
            "Joining the seamless engine loop",
        )?;
    }
    if engine.failed()
        || stems
            .iter()
            .any(|s| !s.exhaust.is_finite() || !s.engine.is_finite() || !s.mixed.is_finite())
    {
        return Err(format!(
            "Invalid export voice output at {rpm} rpm / load {load}"
        ));
    }
    let join = |frames: usize, sample: fn(&crate::hybrid::HybridStems) -> f32| {
        let raw: Vec<f32> = stems
            .iter()
            .map(|stem| sample(stem) / (0.8 * bank.gain))
            .collect();
        let mut out = raw[overlap..overlap + frames].to_vec();
        for i in 0..overlap {
            let t = i as f32 / (overlap - 1) as f32;
            let w = 0.5 - 0.5 * (std::f32::consts::PI * t).cos();
            out[frames - overlap + i] = raw[frames + i] * (1. - w) + raw[i] * w;
        }
        out
    };
    Ok((
        join(frames, |s| s.exhaust),
        join(engine_frames, |s| s.engine),
        // Exports use the physical stem mix, never the audition limiter or A/B transition.
        join(frames, |s| s.exhaust + s.engine * 0.25),
    ))
}

/// Keep the previous full-replacement sound as a single mixed exhaust bank.
pub fn package(dir: &Path, p: Parameters, h: Settings, bank: Arc<Bank>) -> Result<String, String> {
    package_with_job(dir, p, h, bank, &ExportJob::default())
}

/// Full-vehicle export with cooperative cancellation and observable progress.
pub fn package_with_job(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    job: &ExportJob,
) -> Result<String, String> {
    run_package(dir, p, h.for_beamng_export(), bank, false, job)
}

/// Produce the exhaust stem used by selectable variants with a second emitter.
pub fn package_exhaust_stem(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
) -> Result<String, String> {
    run_package(dir, p, h, bank, true, &ExportJob::default())
}

fn run_package(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    exhaust_only: bool,
    job: &ExportJob,
) -> Result<String, String> {
    let result = package_inner(dir, p, h, bank, exhaust_only, job);
    job.finish(&result);
    result
}

struct RenderPlan {
    name: String,
    rpm: f32,
    load: f32,
}
struct RenderedLoop {
    index: usize,
    samples: Vec<f32>,
    exhaust_level_gain: Option<f32>,
}

struct RenderContext<'a> {
    bank: &'a Arc<Bank>,
    parameters: Parameters,
    settings: Settings,
    exhaust_only: bool,
    job: &'a ExportJob,
    abort: &'a AtomicBool,
}

impl RenderContext<'_> {
    fn render_plan(
        &self,
        plan: &RenderPlan,
        index: usize,
        source_zip: &mut Option<zip::ZipArchive<File>>,
    ) -> Result<RenderedLoop, String> {
        let job = self.job;
        let stems = render_stems_controlled(
            self.bank.clone(),
            self.parameters,
            self.settings,
            plan.rpm,
            plan.load,
            2.,
            Some(RenderControl {
                job,
                abort: self.abort,
                index,
            }),
        )?;
        let mut rendered = if self.exhaust_only { stems.0 } else { stems.2 };
        job.check()?;
        let level_gain = if self.exhaust_only {
            let (source_rate, source) = source_wav(
                source_zip.as_mut().ok_or("Missing calibration source")?,
                &plan.name,
            )?;
            signal_stats(&source)?;
            let (_, rendered_peak) = signal_stats(&rendered)?;
            let level_gain =
                physical_exhaust_level_gain(&source, source_rate, &rendered, rendered_peak)?;
            if !level_gain.is_finite() || level_gain <= 0. {
                return Err(format!("Invalid exhaust level gain: {}", plan.name));
            }
            for sample in &mut rendered {
                *sample *= level_gain;
            }
            Some(level_gain)
        } else {
            None
        };
        job.check()?;
        job.rendered(index, plan.rpm, plan.load);
        Ok(RenderedLoop {
            index,
            samples: rendered,
            exhaust_level_gain: level_gain,
        })
    }
}

fn render_parallel(
    plans: &[RenderPlan],
    bank: Arc<Bank>,
    p: Parameters,
    h: Settings,
    exhaust_only: bool,
    job: &ExportJob,
) -> Result<Vec<RenderedLoop>, String> {
    let workers = job.worker_count(plans.len());
    job.begin(
        ExportStage::Rendering,
        plans.len(),
        workers,
        "Generating replacement engine loops",
    );
    let next = AtomicUsize::new(0);
    let abort = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let mut threads = Vec::new();
        let mut error = None;
        for worker in 0..workers {
            let bank = bank.clone();
            let next = &next;
            let abort = &abort;
            let thread = std::thread::Builder::new()
                .name(format!("bess-export-{worker}"))
                .spawn_scoped(scope, move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut source_zip = if exhaust_only {
                            Some(
                                zip::ZipArchive::new(
                                    File::open(&bank.source.archive).map_err(|e| e.to_string())?,
                                )
                                .map_err(|e| e.to_string())?,
                            )
                        } else {
                            None
                        };
                        let mut rendered = Vec::new();
                        let context = RenderContext {
                            bank: &bank,
                            parameters: p,
                            settings: h,
                            exhaust_only,
                            job,
                            abort,
                        };
                        loop {
                            job.check()?;
                            if abort.load(Ordering::Acquire) {
                                break;
                            }
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some(plan) = plans.get(index) else {
                                break;
                            };
                            rendered.push(context.render_plan(plan, index, &mut source_zip)?);
                        }
                        Ok::<_, String>(rendered)
                    }))
                    .unwrap_or_else(|_| Err("An engine export worker stopped unexpectedly".into()));
                    if result.is_err() {
                        abort.store(true, Ordering::Release);
                    }
                    result
                });
            match thread {
                Ok(thread) => threads.push(thread),
                Err(e) => {
                    abort.store(true, Ordering::Release);
                    error = Some(format!("Cannot start export worker: {e}"));
                    break;
                }
            }
        }
        let mut result = Vec::with_capacity(plans.len());
        for thread in threads {
            match thread
                .join()
                .unwrap_or_else(|_| Err("An export worker could not be joined".into()))
            {
                Ok(rendered) => result.extend(rendered),
                Err(e) => {
                    let priority = |message: &str| match message {
                        "Another export worker failed" => 0,
                        crate::export_job::CANCELLED => 1,
                        _ => 2,
                    };
                    if error
                        .as_ref()
                        .is_none_or(|previous| priority(&e) > priority(previous))
                    {
                        error = Some(e)
                    }
                }
            }
        }
        if let Some(error) = error {
            return Err(error);
        }
        job.check()?;
        if result.len() != plans.len() {
            return Err("Incomplete parallel engine rendering".into());
        }
        result.sort_by_key(|item| item.index);
        Ok(result)
    })
}

/// Only paths reserved by this export are removed after failure/cancellation.
/// A caller-owned folder or a file added by another process is never traversed.
struct OutputFiles {
    dir: PathBuf,
    paths: Vec<PathBuf>,
    complete: bool,
}
impl OutputFiles {
    fn create(dir: &Path) -> Result<Self, String> {
        fs::create_dir(dir).map_err(|e| format!("Choose a new output folder: {e}"))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            paths: Vec::new(),
            complete: false,
        })
    }
    fn reserve(&mut self, name: &str) -> Result<File, String> {
        let path = self.dir.join(name);
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        self.paths.push(path);
        Ok(file)
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.reserve(name)?
            .write_all(bytes)
            .map_err(|e| e.to_string())
    }
}
impl Drop for OutputFiles {
    fn drop(&mut self) {
        if !self.complete {
            for path in self.paths.iter().rev() {
                let _ = fs::remove_file(path);
            }
            let _ = fs::remove_dir(&self.dir);
        }
    }
}

fn package_inner(
    dir: &Path,
    p: Parameters,
    h: Settings,
    bank: Arc<Bank>,
    exhaust_only: bool,
    job: &ExportJob,
) -> Result<String, String> {
    job.check()?;
    if dir.exists() {
        return Err("Choose a new output folder: the destination already exists".into());
    }
    job.begin(
        ExportStage::Preparing,
        4,
        0,
        "Checking the original vehicle and engine settings",
    );
    p.validate()?;
    let (h, physical_model) = resolved_settings(&bank, h)?;
    // Verify the file has not been swapped since import before copying the vehicle.
    let fresh = Bank::load(Path::new(&bank.source.archive), Some(&bank.source.blend))?;
    verify_source_identity(&bank.source, &fresh.source)?;
    drop(fresh);
    job.check()?;
    job.advance(1, "Original vehicle identity verified");
    let mut zip =
        zip::ZipArchive::new(File::open(&bank.source.archive).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut names = std::collections::HashSet::new();
    let mut info_path = None;
    let mut total = 0u64;
    for i in 0..zip.len() {
        job.check()?;
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        total = total.saturating_add(entry.size());
        if entry.enclosed_name().is_none()
            || entry.name().contains('\\')
            || !names.insert(entry.name().to_owned())
            || total > 2_000_000_000
        {
            return Err("Ambiguous archive, unsafe paths, or uncompressed size above 2 GB".into());
        }
        if vehicle_info_path(entry.name()) && info_path.replace(entry.name().to_owned()).is_some() {
            return Err("Archive has multiple vehicle info.json files".into());
        }
    }
    let info_path = info_path.ok_or("Archive has no vehicle info.json")?;
    let mut source_info = Vec::new();
    zip.by_name(&info_path)
        .map_err(|e| e.to_string())?
        .take(1_000_001)
        .read_to_end(&mut source_info)
        .map_err(|e| e.to_string())?;
    let (labelled_info, display_name) = label_vehicle_info(&source_info)?;
    let mut bytes = Vec::new();
    zip.by_name(&bank.source.blend)
        .map_err(|e| e.to_string())?
        .take(1_000_001)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let blend: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    job.check()?;
    job.advance(2, "Vehicle archive structure checked");
    let mut plans = Vec::new();
    let mut wav_names = std::collections::HashSet::new();
    for (layer, rows) in blend["samples"]
        .as_array()
        .ok_or("Missing blend")?
        .iter()
        .enumerate()
    {
        for row in rows.as_array().ok_or("Missing load layer")? {
            let name = row[0].as_str().ok_or("Missing WAV name")?;
            let rpm = row[1].as_f64().ok_or("Missing RPM")? as f32;
            if !wav_names.insert(name.to_owned()) {
                return Err(
                    "A WAV shared by multiple RPM points cannot be replaced unambiguously".into(),
                );
            }
            plans.push(RenderPlan {
                name: name.to_owned(),
                rpm,
                load: layer as f32,
            });
        }
    }
    if plans.is_empty() {
        return Err("The source blend has no engine loops".into());
    }
    job.advance(3, "RPM and load points prepared");
    let mut handoff = if exhaust_only {
        None
    } else {
        Some(crate::babm_exchange::prepare(
            Path::new(&bank.source.archive),
            &mut zip,
            info_path
                .strip_suffix("info.json")
                .ok_or("Invalid vehicle root")?,
            &bank.source.blend,
            plans.iter().map(|plan| plan.name.clone()),
        )?)
    };
    job.check()?;
    job.advance(4, "Source metadata and BABM handoff prepared");
    // Each voice keeps its original sequential DSP evolution. Only independent
    // RPM/load loops run concurrently; ZIP order and the common gain stay fixed.
    let rendered = render_parallel(&plans, bank.clone(), p, h, exhaust_only, job)?;
    let workers = job.worker_count(plans.len());
    let mut replacements = BTreeMap::new();
    let mut exhaust_level_gains = BTreeMap::new();
    for rendered in rendered {
        let name = &plans[rendered.index].name;
        if let Some(gain) = rendered.exhaust_level_gain {
            exhaust_level_gains.insert(name.clone(), gain);
        }
        replacements.insert(name.clone(), rendered.samples);
    }
    let peak = replacements
        .values()
        .flatten()
        .fold(0f32, |a, &b| a.max(b.abs()));
    if !peak.is_finite() || peak < 1e-8 || replacements.values().flatten().any(|v| !v.is_finite()) {
        return Err("Silent or non-finite rendering".into());
    }
    let gain = exhaust_safety_gain(peak);
    let zip_name = package_name(&bank);
    job.check()?;
    job.begin(
        ExportStage::Packaging,
        zip.len() + usize::from(handoff.is_some()),
        workers,
        "Writing the complete vehicle ZIP",
    );
    let mut files = OutputFiles::create(dir)?;
    (|| -> Result<String, String> {
        let partial = dir.join(format!("{zip_name}.partial"));
        let mut output = zip::ZipWriter::new(files.reserve(&format!("{zip_name}.partial"))?);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut measurements = Vec::new();
        for i in 0..zip.len() {
            job.check()?;
            let entry = zip.by_index(i).map_err(|e| e.to_string())?;
            if handoff.is_some() && entry.name() == crate::babm_exchange::MARKER_PATH {
                job.advance(i + 1, "Replacing the previous BABM handoff");
                continue;
            }
            if let Some(samples) = replacements.get(entry.name()) {
                let mut wav = Cursor::new(Vec::new());
                {
                    let mut writer = hound::WavWriter::new(
                        &mut wav,
                        hound::WavSpec {
                            channels: 1,
                            sample_rate: 48000,
                            bits_per_sample: 24,
                            sample_format: hound::SampleFormat::Int,
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    for (index, s) in samples.iter().enumerate() {
                        if index % 4096 == 0 {
                            job.check()?;
                        }
                        writer
                            .write_sample((s * gain * 8388607.) as i32)
                            .map_err(|e| e.to_string())?;
                    }
                    writer.finalize().map_err(|e| e.to_string())?;
                }
                output
                    .start_file(entry.name(), options)
                    .map_err(|e| e.to_string())?;
                output.write_all(wav.get_ref()).map_err(|e| e.to_string())?;
                if let Some(handoff) = &mut handoff {
                    handoff.record_rendered(entry.name(), wav.get_ref())?;
                }
                let mut measurement = json!({"path":entry.name(),"frames":samples.len(),"seam":(samples[0]-samples[samples.len()-1]).abs()*gain});
                if let Some(level_gain) = exhaust_level_gains.get(entry.name()) {
                    measurement["exhaust_level_gain"] = json!(level_gain);
                }
                measurements.push(measurement);
            } else if entry.name() == info_path {
                output
                    .start_file(entry.name(), options)
                    .map_err(|e| e.to_string())?;
                output
                    .write_all(&labelled_info)
                    .map_err(|e| e.to_string())?;
            } else {
                output.raw_copy_file(entry).map_err(|e| e.to_string())?;
            }
            job.advance(i + 1, "Writing vehicle files and replacement sounds");
        }
        job.check()?;
        if let Some(handoff) = &handoff {
            output
                .start_file(crate::babm_exchange::MARKER_PATH, options)
                .map_err(|e| e.to_string())?;
            output
                .write_all(&handoff.bytes()?)
                .map_err(|e| e.to_string())?;
            job.advance(zip.len() + 1, "BABM handoff written");
        }
        output.finish().map_err(|e| e.to_string())?;
        job.check()?;
        job.begin(
            ExportStage::Verifying,
            3,
            workers,
            "Reopening the generated vehicle ZIP",
        );
        // Reimport the real produced archive, not only an in-memory rendering.
        let check = Bank::load(&partial, Some(&bank.source.blend))?;
        job.check()?;
        if check.layers.iter().map(Vec::len).sum::<usize>() != replacements.len() {
            return Err("Incomplete loop coverage".into());
        }
        job.advance(1, "Generated sound archive reimported");
        let mut check_zip = zip::ZipArchive::new(File::open(&partial).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut check_info = Vec::new();
        check_zip
            .by_name(&info_path)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut check_info)
            .map_err(|e| e.to_string())?;
        if check_info != labelled_info {
            return Err("Exported vehicle display metadata differs from the prepared label".into());
        }
        job.check()?;
        job.advance(2, "Vehicle metadata and loop coverage verified");
        drop(files.reserve("settings.bess.json")?);
        crate::project::save_project(
            &dir.join("settings.bess.json"),
            &crate::project::Project {
                version: 3,
                parameters: p,
                hybrid: h,
                source: Some(bank.source.clone()),
                driving: Default::default(),
                profile_name: crate::project::default_profile_name(),
                scratch: None,
            },
        )?;
        let report = json!({
            "version":env!("CARGO_PKG_VERSION"),
            "zip_file":zip_name,
            "display_name":display_name,
            "display_name_path":info_path,
            "source":bank.source,
            "settings":h,
            "parameters":p,
            "gain":gain,
            "loops":measurements,
            "render_channel":if exhaust_only {"exhaust"} else {"mixed"},
            "babm_export":handoff,
            "render_model":"physical_automation",
            "engine_definition":h.engine,
            "engine_baseline":physical_model.baseline,
            "engine_provenance":physical_model.provenance,
            "loop_policy":loop_policy(),
            "physical_assumptions":physical_model.assumptions,
            "physical_sound":h.physical_sound,
            "physical_warmup":"at least 2 seconds, whole 720-degree cycles",
            "exhaust_level_reference":if exhaust_only {"estimated post-80-Hz low-cut RMS; absolute PCM peak still bounds gain"} else {"unfiltered AC RMS"},
            "runtime_events":"The original vehicle references for afterfire, turbo, startup, and shutdown are retained. BESS driving transients are not exported.",
            "validation":"BESS reimported the generated archive; testing in BeamNG is still required"
        });
        job.check()?;
        files.write(
            "manifest.json",
            &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
        )?;
        let instructions = if exhaust_only {
            "BESS intermediate exhaust-stem render\n\nDo not install this ZIP directly. It contains only the exhaust half of the selectable BESS sound and is an input to variant conversion. Install the final bess-variant-*.zip beside the original Automation vehicle instead.\n".to_owned()
        } else {
            format!(
                "BESS — full vehicle with modified engine loops\n\nThe vehicle selector shows: {display_name}\n\nWith BABM: refresh BESS sounds, then apply this export to its original vehicle or existing grouped pack. BABM checks the source identity and replaces only the associated sound files, retaining the previous pack. The embedded bess-export.json travels with {zip_name}.\n\nFor a direct BeamNG test without BABM:\n1. Keep a backup of the original Automation ZIP.\n2. Disable the original vehicle in BeamNG's mod manager.\n3. Install {zip_name} in the mods folder under your BeamNG user folder.\n4. Enable only this copy. Do not enable both versions at once.\n5. Reload the vehicle and compare idle, acceleration, lift-off, and camera views.\n6. To restore the original, disable the BESS copy and re-enable the original.\n\nEach BESS copy has a distinct ZIP name to avoid collisions between vehicles.\nThe off-load and on-load loops cover every RPM point in the original blend.\nEvents and physics remain those of the original vehicle. BESS transients are not exported as a BeamNG driving script.\nThe listening volume is not applied to the mod; one common safety gain preserves the relative dynamics.\nIn-game validation is still required.\n"
            )
        };
        files.write("INSTALLATION.txt", instructions.as_bytes())?;
        job.check()?;
        job.advance(3, "Verification complete; saving the finished archive");
        job.publish(|| {
            let final_path = dir.join(&zip_name);
            if final_path.exists() {
                return Err("The final ZIP path already exists".into());
            }
            fs::rename(&partial, &final_path).map_err(|e| e.to_string())?;
            files.paths.push(final_path);
            files.complete = true;
            Ok(format!("{} loops — {}", replacements.len(), dir.display()))
        })
    })()
}

#[cfg(test)]
mod parallel_job_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn fixture() -> (PathBuf, Arc<Bank>) {
        let path = crate::test_support::automation_fixture();
        let bank = Arc::new(Bank::load(&path, None).unwrap());
        (path, bank)
    }
    fn read_entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        let mut entries = BTreeMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            entries.insert(entry.name().to_string(), data);
        }
        entries
    }
    fn compressed_entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
        let bytes = fs::read(path).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        (0..archive.len())
            .map(|i| {
                let entry = archive.by_index(i).unwrap();
                let start = entry.data_start() as usize;
                let end = start + entry.compressed_size() as usize;
                (entry.name().to_string(), bytes[start..end].to_vec())
            })
            .collect()
    }

    #[test]
    fn export_workers_preserve_exact_pcm_and_raw_vehicle_members_with_back_pressure() {
        let (source, bank) = fixture();
        let original = fs::read(&source).unwrap();
        let original_entries = read_entries(&source);
        let original_compressed = compressed_entries(&source);
        for coupled in [false, true] {
            let mut settings = resolved_settings(&bank, Settings::default()).unwrap().0;
            settings.engine.as_mut().unwrap().experimental.wave_coupling = coupled;
            let serial = source.with_extension(format!("serial-{coupled}"));
            let parallel = source.with_extension(format!("parallel-{coupled}"));
            let one = ExportJob::with_worker_limit(1);
            let many = ExportJob::with_worker_limit(3);
            package_with_job(&serial, Parameters::default(), settings, bank.clone(), &one).unwrap();
            package_with_job(
                &parallel,
                Parameters::default(),
                settings,
                bank.clone(),
                &many,
            )
            .unwrap();
            let a_path = serial.join(package_name(&bank));
            let b_path = parallel.join(package_name(&bank));
            let a = read_entries(&a_path);
            let b = read_entries(&b_path);
            let compressed = compressed_entries(&b_path);
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (name, bytes) in &a {
                if name != crate::babm_exchange::MARKER_PATH {
                    assert_eq!(bytes, &b[name], "workers changed {name}, coupled={coupled}");
                }
            }
            for (name, bytes) in &original_entries {
                if !name.ends_with(".wav") && !vehicle_info_path(name) {
                    assert_eq!(&b[name], bytes);
                    assert_eq!(
                        compressed[name], original_compressed[name],
                        "recompressed preserved member {name}"
                    );
                }
            }
            let a_marker: crate::babm_exchange::ExportMarker =
                serde_json::from_slice(&a[crate::babm_exchange::MARKER_PATH]).unwrap();
            let b_marker: crate::babm_exchange::ExportMarker =
                serde_json::from_slice(&b[crate::babm_exchange::MARKER_PATH]).unwrap();
            assert_eq!(
                serde_json::to_value(a_marker.sounds).unwrap(),
                serde_json::to_value(b_marker.sounds).unwrap()
            );
            assert_eq!(one.snapshot().stage, ExportStage::Complete);
            assert_eq!(many.snapshot().stage, ExportStage::Complete);
            assert_eq!(many.snapshot().workers, 3);
            one.cancel();
            assert_eq!(one.snapshot().stage, ExportStage::Complete);
            assert!(!one.is_cancelled());
            fs::remove_dir_all(serial).unwrap();
            fs::remove_dir_all(parallel).unwrap();
        }
        assert_eq!(fs::read(&source).unwrap(), original);
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn cancellation_during_generation_stops_workers_without_publishing() {
        let (source, bank) = fixture();
        let output = source.with_extension("cancel-render");
        let original = fs::read(&source).unwrap();
        let job = ExportJob::with_worker_limit(2);
        let observer = job.clone();
        let cancel = std::thread::spawn(move || {
            let start = Instant::now();
            let mut previous = 0.;
            loop {
                let progress = observer.snapshot();
                if progress.stage == ExportStage::Rendering {
                    let fraction = progress.fraction.unwrap_or(0.);
                    assert!(fraction >= previous);
                    previous = fraction;
                    if fraction > 0. {
                        observer.cancel();
                        return;
                    }
                }
                assert!(!matches!(
                    progress.stage,
                    ExportStage::Complete | ExportStage::Failed
                ));
                assert!(start.elapsed() < Duration::from_secs(30));
                std::thread::yield_now();
            }
        });
        let result = package_with_job(
            &output,
            Parameters::default(),
            Settings::default(),
            bank,
            &job,
        );
        cancel.join().unwrap();
        assert!(result.is_err());
        assert_eq!(job.snapshot().stage, ExportStage::Cancelled);
        assert!(!output.exists());
        assert_eq!(fs::read(&source).unwrap(), original);
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn cancellation_while_packaging_removes_only_owned_partial_files() {
        let (source, bank) = fixture();
        let output = source.with_extension("cancel-package");
        let partial = output.join(format!("{}.partial", package_name(&bank)));
        let job = ExportJob::with_worker_limit(2);
        let observer = job.clone();
        let watched = output.clone();
        let cancel = std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                let progress = observer.snapshot();
                if progress.stage == ExportStage::Packaging && partial.is_file() {
                    fs::write(watched.join("user-note.txt"), b"keep this file").unwrap();
                    observer.cancel();
                    return;
                }
                assert!(!matches!(
                    progress.stage,
                    ExportStage::Complete | ExportStage::Failed
                ));
                assert!(start.elapsed() < Duration::from_secs(30));
                std::thread::yield_now();
            }
        });
        let result = package_with_job(
            &output,
            Parameters::default(),
            Settings::default(),
            bank,
            &job,
        );
        cancel.join().unwrap();
        assert!(result.is_err());
        assert_eq!(job.snapshot().stage, ExportStage::Cancelled);
        let files: Vec<_> = fs::read_dir(&output)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(files, vec![std::ffi::OsString::from("user-note.txt")]);
        assert_eq!(
            fs::read(output.join("user-note.txt")).unwrap(),
            b"keep this file"
        );
        fs::remove_file(output.join("user-note.txt")).unwrap();
        fs::remove_dir(output).unwrap();
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn cancellation_and_existing_destination_fail_before_rendering() {
        let (source, bank) = fixture();
        let output = source.with_extension("existing-output");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep.txt"), b"untouched").unwrap();
        let job = ExportJob::default();
        assert!(
            package_with_job(
                &output,
                Parameters::default(),
                Settings::default(),
                bank.clone(),
                &job
            )
            .is_err()
        );
        assert_eq!(job.snapshot().stage, ExportStage::Failed);
        assert_eq!(job.snapshot().workers, 0);
        assert_eq!(fs::read(output.join("keep.txt")).unwrap(), b"untouched");
        let cancelled = ExportJob::default();
        cancelled.cancel();
        let fresh = source.with_extension("never-created");
        assert!(
            package_with_job(
                &fresh,
                Parameters::default(),
                Settings::default(),
                bank,
                &cancelled
            )
            .is_err()
        );
        assert_eq!(cancelled.snapshot().stage, ExportStage::Cancelled);
        assert!(!fresh.exists());
        fs::remove_dir_all(output).unwrap();
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn worker_failure_aborts_peers_and_keeps_the_actual_failure() {
        let (source, bank) = fixture();
        let job = ExportJob::with_worker_limit(2);
        let plans = [
            RenderPlan {
                name: "first.wav".into(),
                rpm: 800.,
                load: 0.,
            },
            RenderPlan {
                name: "bad.wav".into(),
                rpm: 13000.,
                load: 1.,
            },
        ];
        let result = render_parallel(
            &plans,
            bank,
            Parameters::default(),
            Settings::default(),
            false,
            &job,
        )
        .map(|_| "Unexpected successful export".to_owned());
        assert!(
            result
                .as_ref()
                .unwrap_err()
                .contains("Unsupported export RPM 13000")
        );
        job.finish(&result);
        assert_eq!(job.snapshot().stage, ExportStage::Failed);
        fs::remove_file(source).unwrap();
    }
}

#[cfg(test)]
mod phase_tests {
    use super::aligned_warmup_frames;

    #[test]
    fn export_preroll_lands_at_a_shared_crank_phase() {
        for rpm in [803., 2215., 4989., 5338., 5712., 10_000.] {
            let frames = aligned_warmup_frames(rpm);
            assert!(frames >= 48_000);
            let cycles = frames as f64 * rpm as f64 / (48_000. * 120.);
            let sample_tolerance = rpm as f64 / (2. * 48_000. * 120.);
            assert!((cycles - cycles.round()).abs() <= sample_tolerance + 1e-9);
        }
    }
}

#[cfg(test)]
mod source_identity_tests {
    use super::*;

    #[test]
    fn unchanged_audio_does_not_hide_changed_physical_metadata() {
        let saved = bank::SourceRef {
            archive: "original.zip".into(),
            blend: "motor.json".into(),
            fingerprint: "same-wav-hash".into(),
            engine_fingerprint: Some("engine-v1".into()),
        };
        let mut fresh = saved.clone();
        assert!(verify_source_identity(&saved, &fresh).is_ok());
        fresh.engine_fingerprint = Some("engine-v2".into());
        assert!(
            verify_source_identity(&saved, &fresh)
                .unwrap_err()
                .contains("engine metadata")
        );
        fresh.engine_fingerprint = None;
        assert!(verify_source_identity(&saved, &fresh).is_err());
        let mut legacy = saved.clone();
        legacy.engine_fingerprint = None;
        assert!(verify_source_identity(&legacy, &saved).is_ok());
        fresh.fingerprint = "different-wav-hash".into();
        assert!(verify_source_identity(&legacy, &fresh).is_err());
    }
}

#[cfg(test)]
mod unified_engine_tests {
    use super::*;
    use crate::{automation_model::AutomationModel, engine_definition::EngineDefinition};

    #[test]
    fn stationary_loops_preserve_legacy_defaults_and_use_saved_mechanics() {
        let path = crate::test_support::automation_fixture();
        let bank = Arc::new(Bank::load(&path, None).unwrap());
        let settings = Settings::default();
        let imported = AutomationModel::from_settings(&bank, &settings).unwrap();
        let definition = EngineDefinition::from_scratch(&imported.scratch);
        let explicit = Settings {
            engine: Some(definition),
            // Momentary bench controls cannot contaminate repeatable loops.
            starter: true,
            fuel_cut: 1.,
            ..settings
        };
        let original =
            loop_stems(bank.clone(), Parameters::default(), settings, 4000., 0.).unwrap();
        let saved = loop_stems(bank.clone(), Parameters::default(), explicit, 4000., 0.).unwrap();
        assert_eq!(original, saved);
        let mut edited = definition;
        edited.build.compression = 12.;
        edited.tuning.cam.lift_mm = Some(12.);
        let changed = loop_stems(
            bank,
            Parameters::default(),
            Settings {
                engine: Some(edited),
                ..settings
            },
            4000.,
            0.,
        )
        .unwrap();
        assert!(
            original
                .0
                .iter()
                .zip(&changed.0)
                .any(|(a, b)| (a - b).abs() > 1e-5),
            "saved builder/tuning edits must reach the exported exhaust"
        );
        assert!(changed.0.iter().chain(&changed.1).all(|v| v.is_finite()));
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod level_tests {
    use super::{exhaust_level_gain, low_cut_rms, signal_stats};

    #[test]
    fn selectable_exhaust_gain_targets_one_db_below_source_with_bounds() {
        let target = exhaust_level_gain(0.1, 0.05, 0.1);
        assert!((target - 1.782_501_8).abs() < 1e-6);
        assert_eq!(exhaust_level_gain(0.1, 0.2, 0.5), 0.5);
        assert_eq!(exhaust_level_gain(0.1, 0.01, 0.1), 2.5);
        assert!((exhaust_level_gain(0.1, 0.05, 0.8) - 1.175).abs() < 1e-6);
        // If a render is already very hot, the peak ceiling wins over the
        // nominal minimum gain rather than allowing a clipped WAV.
        assert!((exhaust_level_gain(0.1, 0.05, 2.) - 0.47).abs() < 1e-6);
    }

    #[test]
    fn source_and_render_stats_reject_silent_or_non_finite_audio() {
        assert!(signal_stats(&[]).is_err());
        assert!(signal_stats(&[0.; 512]).is_err());
        assert!(signal_stats(&[0.15; 512]).is_err());
        assert!(signal_stats(&[f32::NAN, 0.1]).is_err());
        assert!(signal_stats(&[f32::INFINITY, 0.1]).is_err());
        assert!(signal_stats(&[0.2, -0.2]).is_ok());
    }

    #[test]
    fn calibration_uses_ac_rms_but_absolute_pcm_peak() {
        let (source_rms, source_peak) = signal_stats(&[0.15, 0.05]).unwrap();
        let (rendered_rms, rendered_peak) = signal_stats(&[0.06, -0.04]).unwrap();
        assert!((source_rms - 0.05).abs() < 1e-7);
        assert!((source_peak - 0.15).abs() < 1e-7);
        assert!((rendered_rms - 0.05).abs() < 1e-7);
        assert!((rendered_peak - 0.06).abs() < 1e-7);
        assert!(
            (exhaust_level_gain(source_rms, rendered_rms, rendered_peak) - 0.891_250_9).abs()
                < 1e-6
        );
    }

    #[test]
    fn physical_level_reference_reduces_sub_bass_without_losing_engine_midrange() {
        let rate = 48_000;
        let sine = |hz: f32| {
            (0..rate)
                .map(|i| (i as f32 * hz * std::f32::consts::TAU / rate as f32).sin() * 0.1)
                .collect::<Vec<_>>()
        };
        let low = sine(40.);
        let mid = sine(400.);
        let low_ratio = low_cut_rms(&low, rate).unwrap() / signal_stats(&low).unwrap().0;
        let mid_ratio = low_cut_rms(&mid, rate).unwrap() / signal_stats(&mid).unwrap().0;
        assert!(low_ratio < 0.5, "40 Hz ratio {low_ratio}");
        assert!(mid_ratio > 0.9, "400 Hz ratio {mid_ratio}");
    }
}
