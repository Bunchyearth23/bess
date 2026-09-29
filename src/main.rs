#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod audio;
#[cfg(test)]
mod control_count;
mod spectrum;
mod ui_kit;
use bess::{
    bank::Bank,
    beamng, beamng_level,
    bench::{AuditionMix, BeamNgCamera},
    drive::{Controls, Mode},
    engine_build::{EngineBuild, EngineTuning},
    hybrid::Settings,
    project::{self, Parameters, Project},
    render,
    room::Room,
    scratch::{EngineDesign, Layout, Scratch, ScratchModel, SoundTuning},
};
use eframe::egui::{self, Color32, RichText};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering, mpsc},
    time::Duration,
};

struct Loaded {
    attached_engine_reference: bool,
    bank: Arc<Bank>,
    vehicle: Option<beamng::Vehicle>,
    automation_model: Option<Result<bess::automation_model::AutomationModel, String>>,
    params: Parameters,
    settings: Settings,
    driving: Controls,
    profile_name: String,
}
#[derive(Clone, PartialEq)]
struct LevelKey {
    source_fingerprint: String,
    engine_fingerprint: Option<String>,
    params: Parameters,
    settings: Settings,
}
impl LevelKey {
    fn new(bank: &Bank, mut params: Parameters, mut settings: Settings) -> Self {
        // The exporter replaces these controls. Selecting another RPM/load or
        // changing listening volume should not invalidate a completed scan.
        params.rpm = bank.min_rpm;
        params.load = 0.;
        params.volume = 0.8;
        settings = settings.for_beamng_export();
        settings.enhanced = true;
        settings.level_match = false;
        settings.overrun = 0.;
        settings.turbo = 0.;
        settings.attack = 0.;
        settings.roughness = 0.;
        settings.fuel_cut = 0.;
        Self {
            source_fingerprint: bank.source.fingerprint.clone(),
            engine_fingerprint: bank.source.engine_fingerprint.clone(),
            params,
            settings,
        }
    }
}
struct App {
    capture: Option<PathBuf>,
    capture_level: bool,
    capture_requested: bool,
    frames: u32,
    params: Parameters,
    settings: Settings,
    bank: Option<Arc<Bank>>,
    vehicle: Option<beamng::Vehicle>,
    automation_model: Option<Result<bess::automation_model::AutomationModel, String>>,
    audio: Option<audio::Audio>,
    playing: bool,
    audition_mix: AuditionMix,
    camera: BeamNgCamera,
    room: Room,
    room_mix: f32,
    driving: Controls,
    restart: u64,
    show_driving: bool,
    sent: Option<audio::Command>,
    status: String,
    seconds: f32,
    worker: Option<mpsc::Receiver<Result<String, String>>>,
    importer: Option<mpsc::Receiver<Result<Loaded, String>>>,
    level_worker: Option<mpsc::Receiver<Result<beamng_level::Report, String>>>,
    level_pending: Option<LevelKey>,
    level_report: Option<(LevelKey, beamng_level::Report)>,
    level_error: Option<String>,
    level_rpm: f32,
    profile_name: String,
    /// Engine designed from scratch; mutually exclusive with `bank`.
    scratch: Option<Scratch>,
    /// Scratch settings the live voice was last built from.
    scratch_built: Option<Scratch>,
    scratch_builder: Option<mpsc::Receiver<Result<ScratchModel, String>>>,
    /// Firing-order text being edited, e.g. "1-3-4-2".
    firing_text: String,
    readout: Readout,
    dyno: Dyno,
}

/// Smoothed display values: instantaneous meters are unreadable.
struct Readout {
    spectrum: spectrum::Spectrum,
    block: Box<[f32; spectrum::WINDOW]>,
    peak_db: f32,
    rpm: f32,
}
impl Readout {
    fn new() -> Self {
        Self {
            spectrum: spectrum::Spectrum::new(48_000.),
            block: Box::new([0.; spectrum::WINDOW]),
            peak_db: -100.,
            rpm: 0.,
        }
    }
    fn update(&mut self, audio: &audio::Audio, dt: f32) {
        let meter = &audio.meter;
        let end = meter.scope_end.load(Ordering::Acquire);
        for (i, sample) in self.block.iter_mut().enumerate() {
            let index = (end + i) % spectrum::WINDOW;
            *sample = f32::from_bits(meter.scope[index].load(Ordering::Relaxed));
        }
        self.spectrum.update(&self.block);
        // Peak meter ballistics: instant rise, 20 dB/s fall.
        let peak = 20.
            * f32::from_bits(meter.peak.load(Ordering::Relaxed))
                .max(1e-5)
                .log10();
        self.peak_db = peak.max(self.peak_db - 20. * dt);
        let rpm = f32::from_bits(meter.rpm.load(Ordering::Relaxed));
        self.rpm += (rpm - self.rpm) * (dt / 0.15).min(1.);
    }
}
/// Dyno sweep points: ~250 rpm steps over a typical idle–redline span.
const DYNO_POINTS: usize = 25;
/// Full-load curve of the scratch engine, swept off the UI thread.
#[derive(Default)]
struct Dyno {
    curve: Option<bess::dyno::Curve>,
    /// The curve before the latest change, drawn dashed as a reference.
    previous: Option<bess::dyno::Curve>,
    /// A curve kept as the reference until cleared.
    pinned: Option<bess::dyno::Curve>,
    error: Option<String>,
    /// Torque-relevant scratch settings of the latest request.
    key: Option<Scratch>,
    /// Debounce deadline of the next sweep.
    due: Option<std::time::Instant>,
    /// Bumped per change: an older sweep stops early and its result is dropped.
    generation: Arc<std::sync::atomic::AtomicU64>,
    progress: Arc<std::sync::atomic::AtomicUsize>,
    worker: Option<mpsc::Receiver<(u64, Result<bess::dyno::Curve, String>)>>,
}
impl Dyno {
    fn busy(&self) -> bool {
        self.due.is_some() || self.worker.is_some()
    }
    /// Debounce changes, run sweeps and collect results. The clutch sizing
    /// torque follows the computed peak unless it was set by hand.
    fn poll(&mut self, scratch: &Scratch, clutch: &mut f32) {
        let now = std::time::Instant::now();
        let key = bess::dyno::key(scratch);
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.generation.fetch_add(1, Ordering::Relaxed);
            self.due = Some(now + Duration::from_millis(300));
        }
        if let Some(key) = self.key.clone()
            && self.due.is_some_and(|due| now >= due)
        {
            self.due = None;
            let (current, progress) = (self.generation.clone(), self.progress.clone());
            let generation = current.load(Ordering::Relaxed);
            progress.store(0, Ordering::Relaxed);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let result = bess::dyno::sweep_with(&key, DYNO_POINTS, |done| {
                    progress.store(done, Ordering::Relaxed);
                    current.load(Ordering::Relaxed) == generation
                });
                let _ = tx.send((generation, result));
            });
            self.worker = Some(rx);
        }
        let Some(rx) = &self.worker else { return };
        let (generation, result) = match rx.try_recv() {
            Ok(message) => message,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => (
                self.generation.load(Ordering::Relaxed),
                Err("The dyno sweep stopped unexpectedly.".into()),
            ),
        };
        self.worker = None;
        if generation != self.generation.load(Ordering::Relaxed) {
            return;
        }
        match result {
            Ok(curve) => {
                let sized = |nm: f64| (nm as f32).clamp(30., 2000.);
                let estimate = scratch
                    .build
                    .performance(scratch.design.cylinders)
                    .peak_torque_nm
                    .clamp(30., 2000.);
                if *clutch == estimate
                    || self
                        .curve
                        .as_ref()
                        .is_some_and(|c| *clutch == sized(c.peak_torque.0))
                {
                    *clutch = sized(curve.peak_torque.0);
                }
                // A re-sweep that changed nothing keeps the older reference.
                if self.curve.as_ref() != Some(&curve) {
                    self.previous = self.curve.take();
                }
                self.curve = Some(curve);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        Self::with_ctx(&cc.egui_ctx, initial)
    }
    fn with_ctx(ctx: &egui::Context, initial: Option<PathBuf>) -> Self {
        ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(12., 10.);
        style.visuals.panel_fill = Color32::from_rgb(17, 23, 30);
        style.visuals.selection.bg_fill = Color32::from_rgb(0, 115, 110);
        ctx.set_style(style);
        let mut app = Self {
            capture: None,
            capture_level: false,
            capture_requested: false,
            frames: 0,
            params: Parameters::default(),
            settings: Settings::default(),
            bank: None,
            vehicle: None,
            automation_model: None,
            audio: None,
            playing: false,
            audition_mix: AuditionMix::Live,
            camera: BeamNgCamera::Cockpit,
            room: Room::Off,
            room_mix: 0.35,
            driving: Controls {
                mode: Mode::Simulated,
                ..Default::default()
            },
            restart: 0,
            show_driving: true,
            sent: None,
            status: "Import a vehicle to use its original Automation sounds.".into(),
            seconds: 16.,
            worker: None,
            importer: None,
            level_worker: None,
            level_pending: None,
            level_report: None,
            level_error: None,
            level_rpm: 900.,
            profile_name: "Natural".into(),
            scratch: None,
            scratch_built: None,
            scratch_builder: None,
            firing_text: String::new(),
            readout: Readout::new(),
            dyno: Dyno::default(),
        };
        if let Some(path) = initial {
            if path.extension().is_some_and(|e| e == "json") {
                app.open_project(&path);
            } else {
                app.import(path, None);
            }
        }
        app
    }
    fn reconnect(&mut self) {
        self.audio = None;
        self.playing = false;
        self.sent = None;
        let audio = match &self.scratch {
            Some(scratch) => {
                self.params.cylinders = scratch.cylinders();
                self.scratch_built = Some(scratch.clone());
                self.scratch_builder = None;
                audio::Audio::with_scratch(self.params, self.settings, scratch, self.driving)
            }
            None => {
                audio::Audio::with_bank(self.params, self.settings, self.bank.clone(), self.driving)
            }
        };
        match audio {
            Ok(a) => {
                self.readout.spectrum.set_rate(a.rate as f32);
                self.audio = Some(a);
            }
            Err(e) => self.status = format!("Audio unavailable: {e}"),
        }
    }
    fn import(&mut self, path: PathBuf, project: Option<Project>) {
        let (tx, rx) = mpsc::channel();
        self.importer = Some(rx);
        self.status = "Loading WAV files and aligning engine cycles…".into();
        let volume = self.params.volume;
        std::thread::spawn(move || {
            let result = (|| {
                let expected = project.as_ref().and_then(|p| p.source.as_ref());
                let bank = Bank::load(&path, expected.map(|s| s.blend.as_str()))?;
                if expected.is_some_and(|s| s.fingerprint != bank.source.fingerprint) {
                    return Err(
                        "The ZIP changed since the project was saved. Import it again explicitly."
                            .into(),
                    );
                }
                if expected
                    .and_then(|source| source.engine_fingerprint.as_ref())
                    .is_some_and(|saved| Some(saved) != bank.source.engine_fingerprint.as_ref())
                {
                    return Err("The Automation engine data changed since this project was saved. Import the ZIP again explicitly.".into());
                }
                let attached_engine_reference =
                    expected.is_some_and(|source| source.engine_fingerprint.is_none());
                let automation_model =
                    Some(bess::automation_model::AutomationModel::from_bank(&bank));
                let vehicle = beamng::inspect(&path).ok();
                let driving = project.as_ref().map(|p| p.driving).unwrap_or(Controls {
                    mode: Mode::Simulated,
                    ..Default::default()
                });
                let profile_name = project
                    .as_ref()
                    .map(|p| p.profile_name.clone())
                    .unwrap_or_else(project::default_profile_name);
                let (mut params, mut settings) = if let Some(p) = project {
                    (p.parameters, p.hybrid)
                } else {
                    let rpm = vehicle
                        .as_ref()
                        .and_then(|v| v.idle_rpm)
                        .unwrap_or(bank.min_rpm)
                        .clamp(bank.min_rpm, bank.max_rpm);
                    (
                        Parameters {
                            cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
                            rpm,
                            load: 0.12,
                            brightness: 10000.,
                            exhaust: 1.,
                            intake: 0.25,
                            mechanical: 0.12,
                            volume,
                            ..Parameters::default()
                        },
                        Settings::calibrated(&bank),
                    )
                };
                if automation_model
                    .as_ref()
                    .is_some_and(|model| model.is_err())
                {
                    settings.enhanced = false;
                }
                params.rpm = params.rpm.clamp(bank.min_rpm, bank.max_rpm);
                Ok(Loaded {
                    attached_engine_reference,
                    bank: Arc::new(bank),
                    vehicle,
                    automation_model,
                    params,
                    settings,
                    driving,
                    profile_name,
                })
            })();
            let _ = tx.send(result);
        });
    }
    fn project(&self) -> Project {
        Project {
            version: 3,
            parameters: self.params,
            hybrid: self.settings,
            source: self.bank.as_ref().map(|b| b.source.clone()),
            driving: self.driving,
            profile_name: self.profile_name.clone(),
            scratch: self.scratch.clone(),
        }
    }
    fn level_key(&self) -> Option<LevelKey> {
        self.bank
            .as_deref()
            .map(|bank| LevelKey::new(bank, self.params, self.settings))
    }
    fn start_level_analysis(&mut self) {
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let key = LevelKey::new(&bank, self.params, self.settings);
        let (tx, rx) = mpsc::channel();
        self.level_pending = Some(key);
        self.level_worker = Some(rx);
        self.level_error = None;
        let params = self.params;
        let settings = self.settings.for_beamng_export();
        std::thread::spawn(move || {
            let _ = tx.send(beamng_level::analyze(bank, params, settings));
        });
    }
    fn open_project(&mut self, path: &Path) {
        match project::load_project(path) {
            Ok(p) => {
                if let Some(scratch) = p.scratch {
                    self.params = p.parameters;
                    self.settings = p.hybrid;
                    self.driving = p.driving;
                    self.profile_name = p.profile_name;
                    self.start_scratch(scratch, false);
                } else if let Some(source) = &p.source {
                    let source_path = PathBuf::from(&source.archive);
                    let resolved = if source_path.is_absolute() {
                        source_path
                    } else {
                        path.parent().unwrap_or(Path::new(".")).join(source_path)
                    };
                    self.import(resolved, Some(p));
                } else {
                    self.params = p.parameters;
                    self.settings = p.hybrid;
                    self.driving = p.driving;
                    self.profile_name = p.profile_name;
                    self.bank = None;
                    self.scratch = None;
                    self.scratch_built = None;
                    self.scratch_builder = None;
                    self.automation_model = None;
                    self.vehicle = None;
                    self.audio = None;
                    self.playing = false;
                    self.status =
                        "Legacy project loaded. Import a ZIP to enable hybrid synthesis.".into();
                }
            }
            Err(e) => self.status = e,
        }
    }
}
impl App {
    /// Leave any imported bank and play an engine designed from scratch.
    /// `fresh` resets sound settings; a loaded project keeps its own.
    fn start_scratch(&mut self, scratch: Scratch, fresh: bool) {
        self.bank = None;
        self.automation_model = None;
        if fresh {
            // Fresh start: generic acoustic defaults, an open outlet and B only.
            self.settings = Settings::default();
            self.params = Parameters {
                brightness: 10000.,
                exhaust: 1.,
                intake: 0.25,
                mechanical: 0.12,
                volume: self.params.volume,
                ..Parameters::default()
            };
        }
        let mut scratch = scratch;
        if fresh {
            scratch.derive_from_build(&mut self.settings, &mut self.params, &mut self.driving);
        }
        self.vehicle = None;
        self.level_report = None;
        self.audition_mix = AuditionMix::Live;
        self.camera = BeamNgCamera::Orbit;
        self.settings.enhanced = true;
        self.params.rpm = self.params.rpm.clamp(scratch.idle_rpm, scratch.redline_rpm);
        self.status =
            "Scratch engine: shape it with the controls on the left. No Automation ZIP is used."
                .into();
        self.firing_text = firing_text(&scratch.design);
        self.scratch = Some(scratch);
        self.reconnect();
    }
    /// Rebuild the scratch voice off the UI thread when its design changes,
    /// then hand it to the callback, which crossfades to it.
    fn sync_scratch(&mut self) {
        let Some(audio) = &self.audio else { return };
        for old in audio.trash.try_iter() {
            drop(old);
        }
        if let Some(rx) = &self.scratch_builder {
            match rx.try_recv() {
                Ok(Ok(model)) => {
                    // A full slot means the callback has not taken the last one yet.
                    if let Err(error) = audio.swap.try_send(model) {
                        drop(error.into_inner());
                        self.scratch_built = None;
                    }
                    self.scratch_builder = None;
                }
                Ok(Err(error)) => {
                    self.status = format!("Scratch engine: {error}");
                    self.scratch_builder = None;
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => self.scratch_builder = None,
            }
        }
        let Some(scratch) = &self.scratch else { return };
        if self.scratch_built.as_ref() == Some(scratch) || scratch.validate().is_err() {
            return;
        }
        if self.scratch_built.is_none() {
            // No prepared model yet: establish the initial chain.
            let playing = self.playing;
            self.reconnect();
            self.playing = playing && self.audio.is_some();
            return;
        }
        self.params.cylinders = scratch.cylinders();
        let (scratch, rate) = (scratch.clone(), audio.synth_rate);
        self.scratch_built = Some(scratch.clone());
        let (tx, rx) = mpsc::channel();
        self.scratch_builder = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(ScratchModel::build(&scratch, rate));
        });
    }
    fn scratch_panel(&mut self, ui: &mut egui::Ui) {
        let Some(scratch) = &mut self.scratch else {
            return;
        };
        self.dyno.poll(scratch, &mut self.driving.peak_torque_nm);
        let perf = scratch.build.performance(scratch.design.cylinders);
        let mut pin = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(format!(
                "{:.2} L {} · {:.1} × {:.1} mm",
                perf.displacement_l,
                layout_name(&scratch.design),
                scratch.build.bore_mm,
                scratch.build.stroke_mm
            ));
            if let Some(curve) = &self.dyno.curve {
                let ((torque, torque_rpm), (power, power_rpm)) =
                    (curve.peak_torque, curve.peak_power);
                ui.label(format!(
                    "{torque:.0} Nm @ {torque_rpm:.0} · {power:.0} kW ({:.0} hp) @ {power_rpm:.0} rpm",
                    power * 1.341
                ))
                .on_hover_text("Wide-open throttle, speed held: cylinder gas torque minus friction. Its peak also sizes the clutch.");
            } else {
                ui.label(format!(
                    "≈ {:.0} Nm · {:.0} kW ({:.0} hp) · idle {:.0} / redline {:.0} rpm",
                    perf.peak_torque_nm,
                    perf.peak_power_kw,
                    perf.peak_power_kw * 1.341,
                    perf.idle_rpm,
                    perf.redline_rpm
                ));
                ui.small("Builder estimate until the dyno sweep of the physical engine is ready.");
            }
            if self.dyno.busy() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.small(format!(
                        "Dyno sweep {}/{DYNO_POINTS}",
                        self.dyno.progress.load(Ordering::Relaxed)
                    ));
                });
            }
            if let Some(error) = &self.dyno.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            if let Some(curve) = &self.dyno.curve {
                let reference = self.dyno.pinned.as_ref().or(self.dyno.previous.as_ref());
                dyno_plot(ui, curve, reference, self.dyno.busy());
                ui.horizontal_wrapped(|ui| {
                    if let Some(reference) = reference {
                        ui.label(format!(
                            "Dashed: {} · Δ {:+.0} Nm · {:+.1} kW",
                            if self.dyno.pinned.is_some() {
                                "pinned"
                            } else {
                                "before the last change"
                            },
                            curve.peak_torque.0 - reference.peak_torque.0,
                            curve.peak_power.0 - reference.peak_power.0
                        ));
                    }
                    let label = if self.dyno.pinned.is_some() {
                        "Clear reference"
                    } else {
                        "Pin as reference"
                    };
                    pin = ui
                        .small_button(label)
                        .on_hover_text("Keep this curve dashed while you tune, instead of the previous one.")
                        .clicked();
                });
            }
        });
        if pin {
            self.dyno.pinned = match self.dyno.pinned {
                Some(_) => None,
                None => self.dyno.curve.clone(),
            };
        }
        let before = (scratch.design, scratch.build);
        section(ui, "Block", |ui| {
            engine_design(ui, &mut scratch.design, &mut self.firing_text);
            engine_block(ui, &mut scratch.build);
        });
        let multibank = scratch.design.banks[..scratch.design.cylinders as usize].contains(&1);
        let cylinders = scratch.design.cylinders;
        engine_parts(
            ui,
            &mut scratch.build,
            &mut scratch.tuning,
            &mut scratch.sound,
            cylinders,
            multibank,
        );
        if (scratch.design, scratch.build) != before
            && scratch.design.validate().is_ok()
            && scratch.build.validate().is_ok()
        {
            let mix = (
                self.params.exhaust,
                self.params.intake,
                self.params.mechanical,
                self.settings.fuel_cut,
            );
            scratch.derive_from_build(&mut self.settings, &mut self.params, &mut self.driving);
            (
                self.params.exhaust,
                self.params.intake,
                self.params.mechanical,
                self.settings.fuel_cut,
            ) = mix;
        }
        section(ui, "Combustion and engine physics", |ui| {
            combustion_controls(ui, &mut scratch.sound);
            percent_slider(
                ui,
                "Afterfire (lift-off pops)",
                &mut scratch.experimental.afterfire,
            )
            .on_hover_text("Off by default: pop-and-bang style overrun, independent of the parts.");
            percent_slider(
                ui,
                "Knock (end-gas detonation)",
                &mut scratch.experimental.knock,
            )
            .on_hover_text(
                "Off by default: fuel from RON 95 (0 %) to RON 85 (100 %); rings at high load or advanced spark, and the ECU retards the knocking cylinder.",
            );
            slider(
                ui,
                "Idle target (rpm)",
                &mut scratch.idle_rpm,
                300.0..=2000.0,
            );
            let floor = scratch.idle_rpm + 1000.;
            scratch.redline_rpm = scratch.redline_rpm.max(floor);
            slider(
                ui,
                "Rev limiter (rpm)",
                &mut scratch.redline_rpm,
                floor..=12_000.0,
            );
            slider(
                ui,
                "Rotating inertia (kg·m²)",
                &mut scratch.inertia,
                0.05..=2.0,
            );
            percent_slider(ui, "Deceleration fuel cut", &mut self.settings.fuel_cut);
            ui.checkbox(
                &mut self.settings.accessory_ac,
                "A/C compressor load · 16 Nm",
            )
            .on_hover_text("Accessory loads act on the physical crank in driving mode. Direct RPM tests hold shaft speed.");
            ui.checkbox(
                &mut self.settings.accessory_steering,
                "Power steering load · 22 Nm",
            );
        });
        section(ui, "Layer mix", |ui| {
            layer_mix_controls(ui, &mut self.params, &mut self.settings);
        });
        sound_tuning_controls(ui, &mut scratch.sound);
        if let Err(e) = scratch.validate() {
            ui.colored_label(Color32::LIGHT_RED, e);
        }
    }
    fn listen_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sound comparison bench");
        let rpm = self
            .audio
            .as_ref()
            .filter(|_| self.playing)
            .map(|_| self.readout.rpm)
            .unwrap_or(self.params.rpm);
        ui.label(
            RichText::new(format!("{:05.0} rpm", rpm))
                .size(44.)
                .color(Color32::from_rgb(75, 222, 195)),
        );
        if self.driving.mode == Mode::Simulated
            && let Some(audio) = &self.audio
        {
            let meter = &audio.meter;
            let gear = meter.gear.load(Ordering::Relaxed);
            let speed = f32::from_bits(meter.speed.load(Ordering::Relaxed));
            let load = f32::from_bits(meter.load.load(Ordering::Relaxed));
            ui.label(
                RichText::new(format!(
                    "{speed:.1} km/h  ·  Gear {}",
                    if gear == 0 {
                        "N".to_owned()
                    } else {
                        gear.to_string()
                    }
                ))
                .size(18.),
            );
            ui.label(format!("Engine load: {:.0} %", load * 100.));
            ui.small(format!(
                "Wheel torque: {:.0} Nm  ·  Total resistance: {:.0} Nm",
                f32::from_bits(meter.torque.load(Ordering::Relaxed)),
                f32::from_bits(meter.resistance.load(Ordering::Relaxed))
            ));
            let flags = meter.drive_flags.load(Ordering::Relaxed);
            if flags & 1 != 0 {
                ui.label("Shifting…");
            }
            if flags & 2 != 0 {
                ui.label(if self.scratch.is_some() {
                    "Physical rev limiter active"
                } else {
                    "Sound bank limit reached"
                });
            }
            if flags & 4 != 0 {
                ui.colored_label(
                    Color32::YELLOW,
                    if self.scratch.is_some() {
                        "Downshift delayed: would exceed the engine redline."
                    } else {
                        "Downshift delayed: RPM is outside the sound bank range."
                    },
                );
            }
        }
        if self.scratch.is_some() {
            camera_row(ui, &mut self.camera, true);
            ui.horizontal_wrapped(|ui| {
                ui.label("Room:")
                    .on_hover_text("Room is for listening only; WAV and BeamNG exports stay dry.");
                for room in Room::ALL {
                    ui.selectable_value(&mut self.room, room, room.label());
                }
                ui.add_enabled(
                    self.room != Room::Off,
                    egui::Slider::new(&mut self.room_mix, 0.0..=1.0).text("wet"),
                );
            });
        } else {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.settings.enhanced, false, "A · Source Automation")
                    .on_hover_text("Plays the imported WAV files with prepared transitions. Not a game recording.");
                let ready = self
                    .automation_model
                    .as_ref()
                    .is_some_and(|model| model.is_ok());
                ui.add_enabled_ui(ready, |ui| {
                    ui.selectable_value(
                        &mut self.settings.enhanced,
                        true,
                        "B · BESS physical engine",
                    )
                    .on_hover_text("The same physical engine as scratch engines, configured from the Automation vehicle data.");
                });
            });
        }
        if self.scratch.is_none() {
            ui.add_enabled(
                self.audition_mix == AuditionMix::Live,
                egui::Checkbox::new(
                    &mut self.settings.level_match,
                    "Match levels to compare tone",
                ),
            );
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let beamng = self.audition_mix == AuditionMix::BeamNgTwoEmitter;
                if camera_row(ui, &mut self.camera, beamng) {
                    self.audition_mix = AuditionMix::BeamNgTwoEmitter;
                }
                ui.horizontal(|ui| {
                    ui.label("Listening mix:");
                    ui.selectable_value(
                        &mut self.audition_mix,
                        AuditionMix::BeamNgTwoEmitter,
                        "BeamNG two-emitter preview",
                    );
                    ui.selectable_value(&mut self.audition_mix, AuditionMix::Live, "BESS live mix");
                });
                if self.audition_mix == AuditionMix::BeamNgTwoEmitter {
                    ui.small(self.camera.description());
                    if let Some(audio) = &self.audio {
                        let peak = f32::from_bits(audio.meter.peak.load(Ordering::Relaxed));
                        let peak_db = if peak > 1e-5 {
                            20. * peak.log10()
                        } else {
                            -100.
                        };
                        ui.label(format!(
                            "Live peak ({}) : {peak_db:.1} dBFS",
                            self.camera.label()
                        ));
                    } else if self.bank.is_none() {
                        ui.small("💡 Import a vehicle (Import Automation ZIP…) to preview audio.");
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.audio.is_some() && self.level_worker.is_none(),
                    egui::Button::new(if self.playing {
                        "■ Stop"
                    } else {
                        "▶ Listen"
                    })
                    .min_size(egui::vec2(150., 40.)),
                )
                .clicked()
            {
                self.playing = !self.playing;
            }
            if ui
                .add_enabled(
                    self.bank.is_some() || self.scratch.is_some(),
                    egui::Button::new("Reconnect audio"),
                )
                .clicked()
            {
                self.reconnect();
            }
        });
        slider(
            ui,
            "Listening / comparison WAV volume",
            &mut self.params.volume,
            0.0..=1.0,
        );
        if let Some(audio) = &self.audio {
            let latency = f32::from_bits(audio.meter.latency_ms.load(Ordering::Relaxed));
            ui.small(format!(
                "Output latency {} (device {})",
                audio::latency_text(latency, audio.bluetooth),
                audio.device_name
            ));
            if audio.bluetooth {
                ui.colored_label(
                    Color32::YELLOW,
                    "Bluetooth adds ~200–300 ms; use a wired output to judge timing.",
                );
            }
        }
    }
    fn driving_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Driving — test bench");
        ui.separator();
        self.drive_controls(ui);
        if self.driving.mode == Mode::Direct {
            let range = self
                .bank
                .as_ref()
                .map(|bank| (bank.min_rpm, bank.max_rpm))
                .or(self.scratch.as_ref().map(|s| (s.idle_rpm, s.redline_rpm)));
            if let Some((min, max)) = range {
                self.params.rpm = self.params.rpm.clamp(min, max);
                slider_help(
                    ui,
                    "Requested RPM",
                    &mut self.params.rpm,
                    min..=max,
                    &format!("Range: {min:.0}–{max:.0} rpm"),
                );
            } else {
                ui.small("Import a vehicle to adjust RPM.");
            }
            slider(ui, "Requested load", &mut self.params.load, 0.0..=1.0);
            slider(
                ui,
                "Inertia / response",
                &mut self.settings.response,
                0.0..=1.0,
            );
        }
    }
    fn drive_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.driving.mode, Mode::Simulated, "Simulated driving");
            ui.selectable_value(&mut self.driving.mode, Mode::Direct, "Direct RPM / load");
            ui.selectable_value(&mut self.driving.mode, Mode::Cycle, "Comparison cycle");
        });
        if self.driving.mode == Mode::Simulated {
            if self.scratch.is_some() {
                self.settings.starter = ui
                    .button("Hold to start")
                    .on_hover_text("Hold the starter to turn a stalled engine. Use neutral to reduce the load.")
                    .is_pointer_button_down_on();
            }
            percent_slider(ui, "Throttle", &mut self.driving.throttle);
            percent_slider(ui, "Brake", &mut self.driving.brake);
            if ui
                .checkbox(
                    &mut self.driving.automatic,
                    "Automatic transmission (6 gears)",
                )
                .changed()
                && !self.driving.automatic
                && let Some(audio) = &self.audio
            {
                self.driving.gear = audio.meter.gear.load(Ordering::Relaxed).min(6) as u8;
            }
            ui.add_enabled_ui(!self.driving.automatic, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Requested gear");
                    for gear in 0..=6 {
                        ui.selectable_value(
                            &mut self.driving.gear,
                            gear,
                            if gear == 0 {
                                "N".to_owned()
                            } else {
                                gear.to_string()
                            },
                        );
                    }
                });
            });
            slider(
                ui,
                "Wheel load (Nm)",
                &mut self.driving.resistance_nm,
                0.0..=4000.0,
            );
            slider(
                ui,
                "Uphill grade (%)",
                &mut self.driving.grade_percent,
                0.0..=25.0,
            );
            ui.small("The added load acts after the gearbox and final drive.");
            if ui.button("Restart from standstill").clicked() {
                self.restart = self.restart.wrapping_add(1);
            }
            section(ui, "Test bench vehicle and gearing", |ui| {
                ui.small("Adjustable simulation values; these are not identified in the ZIP.");
                slider(ui, "Mass (kg)", &mut self.driving.mass_kg, 300.0..=6000.0);
                slider(
                    ui,
                    if self.scratch.is_some() {
                        "Clutch sizing torque (Nm)"
                    } else {
                        "Peak engine torque (Nm)"
                    },
                    &mut self.driving.peak_torque_nm,
                    30.0..=2000.0,
                );
                if self.scratch.is_none() {
                    slider(
                        ui,
                        "Engine inertia (kg·m²)",
                        &mut self.driving.inertia,
                        0.1..=2.0,
                    );
                } else {
                    ui.small("Rotating engine inertia is set in the physical engine controls.");
                }
                slider(
                    ui,
                    "Wheel radius (m)",
                    &mut self.driving.wheel_radius,
                    0.2..=0.6,
                );
                slider(
                    ui,
                    "Final drive ratio",
                    &mut self.driving.final_drive,
                    1.5..=6.0,
                );
                for i in 0..6 {
                    let min = if i == 5 {
                        0.4
                    } else {
                        self.driving.ratios[i + 1] + 0.01
                    };
                    let max = if i == 0 {
                        5.
                    } else {
                        self.driving.ratios[i - 1] - 0.01
                    };
                    if min <= max {
                        slider(
                            ui,
                            &format!("Gear {}", i + 1),
                            &mut self.driving.ratios[i],
                            min..=max,
                        );
                    } else {
                        ui.small(format!("Gear {}: separate adjacent ratios first", i + 1));
                    }
                }
            });
            if let Some(bank) = &self.bank {
                ui.small(format!(
                    "Sound range: {:.0}–{:.0} rpm",
                    bank.min_rpm, bank.max_rpm
                ));
            }
        } else if self.driving.mode == Mode::Cycle {
            ui.small("16-second sequence: idle, acceleration, release, and recovery.");
            if ui.button("Restart cycle").clicked() {
                self.restart = self.restart.wrapping_add(1);
            }
        }
    }
}
fn wheel_adjust(
    ui: &mut egui::Ui,
    response: &mut egui::Response,
    value: &mut f32,
    min: f32,
    max: f32,
) {
    let remainder_id = response.id.with("wheel-remainder");
    if !response.hovered() {
        ui.data_mut(|data| data.remove::<f32>(remainder_id));
        return;
    }
    let (delta, fine) = ui.input_mut(|i| {
        // Raw events count wheel notches once. The smoothed scroll continues
        // across frames and must never drive a numeric control.
        let delta = i
            .events
            .iter()
            .filter_map(|event| {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                } = event
                {
                    let axis = if modifiers.shift && delta.y == 0. {
                        delta.x
                    } else {
                        delta.y
                    };
                    Some(match unit {
                        egui::MouseWheelUnit::Point => axis / 50.,
                        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => axis,
                    })
                } else {
                    None
                }
            })
            .sum::<f32>();
        i.raw_scroll_delta = egui::Vec2::ZERO;
        i.smooth_scroll_delta = egui::Vec2::ZERO;
        (delta, i.modifiers.shift)
    });
    if delta != 0. {
        let ticks = ui.data_mut(|data| {
            let mut remainder = data.get_temp::<f32>(remainder_id).unwrap_or(0.);
            if remainder.signum() != delta.signum() {
                remainder = 0.;
            }
            let total = remainder + delta;
            data.insert_temp(remainder_id, total.fract());
            total.trunc()
        });
        let span = max - min;
        let step = if span >= 10. {
            1.
        } else if span > 1. {
            0.1
        } else {
            0.01
        };
        let step = step * if fine { 0.1 } else { 1. };
        if ticks != 0. {
            let next = (((*value / step).round() + ticks) * step).clamp(min, max);
            if next == *value {
                return;
            }
            *value = next;
            response.mark_changed();
        }
    }
}
/// Full-load torque (left axis) and power (right axis) from idle to redline,
/// with peak marks and a hover readout. `reference` is drawn dashed under it
/// (the previous or pinned curve). `dim` while a newer sweep runs.
fn dyno_plot(
    ui: &mut egui::Ui,
    curve: &bess::dyno::Curve,
    reference: Option<&bess::dyno::Curve>,
    dim: bool,
) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 180.), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8., Color32::from_rgb(9, 15, 21));
    let plot = egui::Rect::from_min_max(
        rect.min + egui::vec2(36., 16.),
        rect.max - egui::vec2(36., 16.),
    );
    let (low, high) = (curve.rpm[0], curve.rpm[curve.rpm.len() - 1]);
    // Axis tops are whole multiples of four round quarter steps.
    let top =
        |peak: f64, quarter: f64| (peak * 1.15 / (4. * quarter)).ceil().max(1.) * 4. * quarter;
    let (torque_top, power_top) = (
        top(
            curve
                .peak_torque
                .0
                .max(reference.map_or(0., |r| r.peak_torque.0)),
            25.,
        ),
        top(
            curve
                .peak_power
                .0
                .max(reference.map_or(0., |r| r.peak_power.0)),
            10.,
        ),
    );
    let x = |rpm: f64| plot.left() + ((rpm - low) / (high - low)) as f32 * plot.width();
    let y =
        |value: f64, top: f64| plot.bottom() - (value / top).clamp(0., 1.) as f32 * plot.height();
    let grid = Color32::from_rgb(28, 38, 48);
    let font = egui::FontId::proportional(10.);
    for k in 0..=4 {
        let f = f64::from(k) / 4.;
        let row = y(f * torque_top, torque_top);
        painter.hline(plot.x_range(), row, egui::Stroke::new(1_f32, grid));
        painter.text(
            egui::pos2(plot.left() - 4., row),
            egui::Align2::RIGHT_CENTER,
            format!("{:.0}", f * torque_top),
            font.clone(),
            Color32::GRAY,
        );
        painter.text(
            egui::pos2(plot.right() + 4., row),
            egui::Align2::LEFT_CENTER,
            format!("{:.0}", f * power_top),
            font.clone(),
            Color32::GRAY,
        );
    }
    painter.text(
        egui::pos2(plot.left() - 4., rect.top() + 2.),
        egui::Align2::RIGHT_TOP,
        "Nm",
        font.clone(),
        Color32::GRAY,
    );
    painter.text(
        egui::pos2(plot.right() + 4., rect.top() + 2.),
        egui::Align2::LEFT_TOP,
        "kW",
        font.clone(),
        Color32::GRAY,
    );
    for thousand in (low / 1000.).ceil() as u32..=(high / 1000.) as u32 {
        let column = x(f64::from(thousand) * 1000.);
        painter.vline(column, plot.y_range(), egui::Stroke::new(1_f32, grid));
        painter.text(
            egui::pos2(column, plot.bottom() + 2.),
            egui::Align2::CENTER_TOP,
            format!("{thousand}k"),
            font.clone(),
            Color32::GRAY,
        );
    }
    for (rpm, label, align) in [
        (low, "idle", egui::Align2::LEFT_BOTTOM),
        (high, "redline", egui::Align2::RIGHT_BOTTOM),
    ] {
        painter.vline(
            x(rpm),
            plot.y_range(),
            egui::Stroke::new(1_f32, Color32::from_rgb(90, 60, 60)),
        );
        painter.text(
            egui::pos2(x(rpm), plot.top() - 1.),
            align,
            label,
            font.clone(),
            Color32::GRAY,
        );
    }
    let fade = if dim { 0.35 } else { 1. };
    let torque_color = Color32::from_rgb(75, 222, 195).gamma_multiply(fade);
    let power_color = Color32::from_rgb(255, 170, 70).gamma_multiply(fade);
    let ((torque, torque_rpm), (power, power_rpm)) = (curve.peak_torque, curve.peak_power);
    if let Some(reference) = reference {
        for (values, top, color) in [
            (&reference.torque_nm, torque_top, torque_color),
            (&reference.power_kw, power_top, power_color),
        ] {
            let points: Vec<_> = reference
                .rpm
                .iter()
                .zip(values.iter())
                .map(|(&r, &v)| egui::pos2(x(r), y(v, top)))
                .collect();
            painter.extend(egui::Shape::dashed_line(
                &points,
                egui::Stroke::new(1.4_f32, color.gamma_multiply(0.55)),
                5.,
                4.,
            ));
        }
    }
    // Torque peaks are labelled below their mark, power peaks above.
    for (values, top, color, (peak, peak_rpm), label, vertical) in [
        (
            &curve.torque_nm,
            torque_top,
            torque_color,
            curve.peak_torque,
            format!("{torque:.0} Nm @ {torque_rpm:.0}"),
            egui::Align::Min,
        ),
        (
            &curve.power_kw,
            power_top,
            power_color,
            curve.peak_power,
            format!("{power:.0} kW ({:.0} hp) @ {power_rpm:.0}", power * 1.341),
            egui::Align::Max,
        ),
    ] {
        let points = curve
            .rpm
            .iter()
            .zip(values.iter())
            .map(|(&r, &v)| egui::pos2(x(r), y(v, top)))
            .collect();
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.8_f32, color)));
        let mark = egui::pos2(x(peak_rpm), y(peak, top));
        painter.circle_filled(mark, 3.5, color);
        let horizontal = if mark.x > plot.center().x {
            egui::Align::Max
        } else {
            egui::Align::Min
        };
        let offset = if vertical == egui::Align::Max {
            -5.
        } else {
            5.
        };
        painter.text(
            mark + egui::vec2(0., offset),
            egui::Align2([horizontal, vertical]),
            label,
            font.clone(),
            color,
        );
    }
    if let Some(pointer) = response
        .hover_pos()
        .filter(|p| plot.x_range().contains(p.x))
    {
        let rpm = low + f64::from((pointer.x - plot.left()) / plot.width()) * (high - low);
        let (torque, power) = curve.at(rpm);
        painter.vline(
            pointer.x,
            plot.y_range(),
            egui::Stroke::new(1_f32, Color32::GRAY),
        );
        painter.circle_filled(
            egui::pos2(pointer.x, y(torque, torque_top)),
            3.,
            torque_color,
        );
        painter.circle_filled(egui::pos2(pointer.x, y(power, power_top)), 3., power_color);
        painter.text(
            egui::pos2(plot.center().x, plot.bottom() - 4.),
            egui::Align2::CENTER_BOTTOM,
            format!(
                "{rpm:.0} rpm · {torque:.0} Nm · {power:.0} kW ({:.0} hp)",
                power * 1.341
            ),
            egui::FontId::proportional(12.),
            Color32::WHITE,
        );
    }
}
/// Log-frequency analyser: 20 Hz–20 kHz, −100 to 0 dBFS, with a light grid.
fn spectrum_plot(ui: &mut egui::Ui, spectrum: &spectrum::Spectrum) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 140.), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8., Color32::from_rgb(9, 15, 21));
    let span = (spectrum::HIGH_HZ / spectrum::LOW_HZ).ln();
    let x = |hz: f32| rect.left() + (hz / spectrum::LOW_HZ).ln() / span * rect.width();
    let y = |db: f32| rect.top() + (db / spectrum::FLOOR_DB).clamp(0., 1.) * rect.height();
    let grid = Color32::from_rgb(28, 38, 48);
    for (hz, label) in [
        (50., "50"),
        (100., "100"),
        (200., "200"),
        (500., "500"),
        (1000., "1k"),
        (2000., "2k"),
        (5000., "5k"),
        (10_000., "10k"),
    ] {
        painter.vline(x(hz), rect.y_range(), egui::Stroke::new(1_f32, grid));
        painter.text(
            egui::pos2(x(hz) + 3., rect.bottom() - 3.),
            egui::Align2::LEFT_BOTTOM,
            label,
            egui::FontId::proportional(10.),
            Color32::GRAY,
        );
    }
    for db in [-20., -40., -60., -80.] {
        painter.hline(rect.x_range(), y(db), egui::Stroke::new(1_f32, grid));
        painter.text(
            egui::pos2(rect.left() + 3., y(db) - 1.),
            egui::Align2::LEFT_BOTTOM,
            format!("{db:.0} dB"),
            egui::FontId::proportional(10.),
            Color32::GRAY,
        );
    }
    let points: Vec<_> = (0..spectrum::BINS)
        .map(|bin| egui::pos2(x(spectrum::bin_hz(bin)), y(spectrum.db[bin])))
        .collect();
    let accent = Color32::from_rgb(75, 222, 195);
    for pair in points.windows(2) {
        // Fill each column to the floor; the curve is not convex as a whole.
        painter.add(egui::Shape::convex_polygon(
            vec![
                pair[0],
                pair[1],
                egui::pos2(pair[1].x, rect.bottom()),
                egui::pos2(pair[0].x, rect.bottom()),
            ],
            accent.gamma_multiply(0.18),
            egui::Stroke::NONE,
        ));
    }
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5_f32, accent),
    ));
}
/// A titled block, always shown (no collapsing menus).
fn section<R>(
    ui: &mut egui::Ui,
    title: impl Into<String>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.add_space(6.);
    ui.separator();
    ui.strong(title.into());
    add(ui)
}
fn layout_name(design: &EngineDesign) -> String {
    let n = design.cylinders;
    match design.layout {
        Layout::Inline => format!("I{n}"),
        Layout::V => format!("V{n} {:.0}°", design.bank_angle),
        Layout::Flat => format!("Flat-{n}"),
    }
}
fn choice<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> egui::Response {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        for (option, name) in options {
            ui.selectable_value(value, *option, *name);
        }
    })
    .response
}
fn engine_block(ui: &mut egui::Ui, b: &mut EngineBuild) {
    use bess::engine_build::BlockMaterial;
    // Free sizes: drag or type any positive value.
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut b.bore_mm)
                .range(0.1..=f32::MAX)
                .speed(0.5)
                .suffix(" mm"),
        );
        ui.label("Bore");
        ui.add(
            egui::DragValue::new(&mut b.stroke_mm)
                .range(0.1..=f32::MAX)
                .speed(0.5)
                .suffix(" mm"),
        );
        ui.label("Stroke");
    });
    choice(
        ui,
        "Block",
        &mut b.block,
        &[
            (BlockMaterial::CastIron, "Cast iron"),
            (BlockMaterial::Aluminium, "Aluminium"),
        ],
    )
    .on_hover_text(
        "Block material only shades mechanical noise: iron is heavier and better damped.",
    );
}
/// Automation-style part sections below the block. Each part section holds
/// its choices first, then the overrides of what those parts derive (W-007.3)
/// and, for the exhaust, the geometry that only shapes the sound (X-021).
fn engine_parts(
    ui: &mut egui::Ui,
    b: &mut EngineBuild,
    tuning: &mut EngineTuning,
    sound: &mut SoundTuning,
    cylinders: u32,
    multibank: bool,
) {
    use bess::engine_build::{
        Aspiration, BlowOff, Catalyst, Crankshaft, Crossover, Fuel, Head, Headers, Muffler,
        Throttle,
    };
    use ui_kit::{Derived, override_slider as over, range, reset_button};
    let d = Derived::of(b, cylinders);
    section(ui, "Head and valvetrain", |ui| {
        choice(
            ui,
            "Head",
            &mut b.head,
            &[
                (Head::Pushrod, "Pushrod (OHV)"),
                (Head::Sohc, "SOHC"),
                (Head::Dohc, "DOHC"),
            ],
        );
        let mut valves = b.valves as u32;
        ui.add(egui::Slider::new(&mut valves, 2..=5).text("Valves per cylinder"));
        b.valves = valves as u8;
        percent_slider(ui, "Cam profile (mild → race)", &mut b.cam)
            .on_hover_text("More overlap: stronger top end, lumpier and less stable idle.");
        ui.checkbox(&mut b.vvt, "Variable valve timing");
        ui.horizontal_wrapped(|ui| {
            ui.strong("Cam timing and lift");
            reset_button(
                ui,
                "Reset cams",
                &mut tuning.cam,
                "Back to the values the cam profile derives",
            );
        });
        let c = &mut tuning.cam;
        over(
            ui,
            "Duration at 0.050″",
            &mut c.duration_deg,
            d.duration_deg,
            range::DURATION_DEG,
            "°",
            "Longer: peak torque moves up the rev range, idle gets lumpier.",
        );
        over(
            ui,
            "Lift",
            &mut c.lift_mm,
            d.lift_mm,
            range::LIFT_MM,
            " mm",
            "Flow capacity at high revs. Both cams share one profile for now.",
        );
        over(
            ui,
            "Lobe separation",
            &mut c.lsa_deg,
            d.lsa_deg,
            range::LSA_DEG,
            "°",
            "Angle between the intake and exhaust lobe centres. Tighter: more overlap.",
        );
        over(
            ui,
            "Intake cam advance",
            &mut c.intake_advance_deg,
            d.advance_deg,
            range::ADVANCE_DEG,
            "°",
            "Moves the intake centreline earlier: torque slides down the rev range.",
        );
        ui.label(format!(
            "Valve overlap ≈ {:.0}° at 0.050″",
            ui_kit::overlap_deg(
                c.duration_deg.unwrap_or(d.duration_deg),
                c.lsa_deg.unwrap_or(d.lsa_deg),
                c.intake_advance_deg.unwrap_or(d.advance_deg),
            )
        ))
        .on_hover_text("Duration minus the centre spacing. Negative: the valves never overlap.");
        ui.horizontal_wrapped(|ui| {
            ui.strong("Valve sizes (fraction of bore)");
            reset_button(
                ui,
                "Reset valves",
                &mut tuning.valves,
                "Back to the sizes the head derives",
            );
        });
        let v = &mut tuning.valves;
        over(
            ui,
            "Intake valve / bore",
            &mut v.intake_to_bore,
            d.intake_to_bore,
            range::INTAKE_TO_BORE,
            "",
            "Head diameter of the intake valves; follows the bore.",
        );
        over(
            ui,
            "Exhaust valve / bore",
            &mut v.exhaust_to_bore,
            d.exhaust_to_bore,
            range::EXHAUST_TO_BORE,
            "",
            "Head diameter of the exhaust valves; follows the bore. Whether they fit is not checked.",
        );
    });
    section(ui, "Bottom end", |ui| {
        choice(
            ui,
            "Crankshaft",
            &mut b.crank,
            &[
                (Crankshaft::Cast, "Cast"),
                (Crankshaft::Forged, "Forged"),
                (Crankshaft::Billet, "Billet"),
            ],
        );
        slider(ui, "Compression ratio", &mut b.compression, 7.0..=14.0);
        over(
            ui,
            "Rod / stroke",
            &mut tuning.bottom.rod_to_stroke,
            d.rod_to_stroke,
            range::ROD_TO_STROKE,
            "",
            "Longer rods: less side load and a slower piston at top dead centre.",
        );
    });
    section(ui, "Aspiration", |ui| {
        choice(
            ui,
            "Induction",
            &mut b.aspiration,
            &[
                (Aspiration::Natural, "Natural"),
                (Aspiration::Turbo, "Turbo"),
                (Aspiration::TwinTurbo, "Twin turbo"),
            ],
        );
        if b.aspiration != Aspiration::Natural {
            slider(ui, "Boost (bar)", &mut b.boost_bar, 0.2..=2.5);
            choice(
                ui,
                "Blow-off",
                &mut b.blow_off,
                &[
                    (BlowOff::Recirculating, "Recirculating"),
                    (BlowOff::Atmospheric, "Atmospheric"),
                    (BlowOff::None, "None (flutter)"),
                ],
            );
            over(
                ui,
                "Turbo size",
                &mut tuning.turbo.size,
                d.turbo_size,
                range::TURBO_SIZE,
                " ×",
                "Small: spools early and runs out of breath. Large: lag, then more top end. Scales the compressor and the rotor inertia.",
            );
        }
    });
    section(ui, "Fuel and intake", |ui| {
        choice(
            ui,
            "Fuel system",
            &mut b.fuel,
            &[
                (Fuel::Carburettor, "Carburettor"),
                (Fuel::PortInjection, "Port injection"),
                (Fuel::DirectInjection, "Direct injection"),
            ],
        );
        choice(
            ui,
            "Throttle",
            &mut b.throttle,
            &[
                (Throttle::Single, "Single body"),
                (Throttle::Individual, "Individual throttle bodies"),
            ],
        );
        ui.horizontal_wrapped(|ui| {
            ui.strong("Intake overrides");
            reset_button(
                ui,
                "Reset intake",
                &mut tuning.intake,
                "Back to the sizes the throttle type derives",
            );
        });
        over(
            ui,
            "Throttle diameter",
            &mut tuning.intake.throttle_mm,
            d.throttle_mm,
            range::THROTTLE_MM,
            " mm",
            "Equivalent single throttle: restricts the top end when small.",
        );
        over(
            ui,
            "Plenum / displacement",
            &mut tuning.intake.plenum_ratio,
            d.plenum_ratio,
            range::PLENUM_RATIO,
            " ×",
            "Plenum volume as a multiple of the engine's displacement.",
        );
    });
    section(ui, "Exhaust", |ui| {
        choice(
            ui,
            "Headers",
            &mut b.headers,
            &[
                (Headers::CastManifold, "Cast manifold"),
                (Headers::Tubular, "Tubular"),
                (Headers::EqualLength, "Equal-length"),
            ],
        );
        slider(ui, "Pipe diameter (mm)", &mut b.exhaust_mm, 35.0..=100.0);
        if multibank {
            choice(
                ui,
                "Bank connection",
                &mut b.crossover,
                &[
                    (Crossover::None, "Separate"),
                    (Crossover::H, "H-pipe"),
                    (Crossover::X, "X-pipe"),
                ],
            );
        }
        choice(
            ui,
            "Catalyst",
            &mut b.catalyst,
            &[
                (Catalyst::None, "None"),
                (Catalyst::Standard, "Standard"),
                (Catalyst::HighFlow, "High-flow"),
            ],
        );
        choice(
            ui,
            "Muffler",
            &mut b.muffler,
            &[
                (Muffler::None, "None"),
                (Muffler::StraightThrough, "Straight-through"),
                (Muffler::Baffled, "Baffled"),
                (Muffler::ReverseFlow, "Reverse-flow"),
            ],
        );
        exhaust_geometry_controls(ui, sound, b.muffler != Muffler::None);
    });
}
fn firing_text(design: &EngineDesign) -> String {
    design
        .order()
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join("-")
}
/// Layout, cylinders, crankpins, banks and a free firing order, shared by
/// the scratch event voice. Any combination is allowed, working engine or not.
fn engine_design(ui: &mut egui::Ui, design: &mut EngineDesign, text: &mut String) {
    design.resolve_cam_revolutions();
    ui.strong("Engine design");
    ui.horizontal_wrapped(|ui| {
        for (name, preset) in bess::scratch::PRESETS {
            if ui.selectable_label(*design == preset, name).clicked() {
                *design = preset;
                *text = firing_text(design);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Layout");
        let before = design.layout;
        ui.selectable_value(&mut design.layout, Layout::Inline, "Inline");
        ui.selectable_value(&mut design.layout, Layout::V, "V");
        ui.selectable_value(&mut design.layout, Layout::Flat, "Flat / boxer");
        if design.layout != before {
            // The layout only proposes banks and an angle; everything stays editable.
            design.bank_angle = match design.layout {
                Layout::Inline => 0.,
                Layout::V => 90.,
                Layout::Flat => 180.,
            };
            for (i, bank) in design.banks.iter_mut().enumerate() {
                *bank = u8::from(design.layout != Layout::Inline && i % 2 == 1);
            }
        }
    });
    let mut cylinders = design.cylinders;
    ui.add(egui::Slider::new(&mut cylinders, 1..=12).text("Cylinders"));
    if cylinders != design.cylinders {
        design.set_cylinders(cylinders);
        *text = firing_text(design);
    }
    slider(ui, "Bank angle (°)", &mut design.bank_angle, 0.0..=180.0);
    ui.horizontal(|ui| {
        ui.label("Firing order");
        if ui
            .text_edit_singleline(text)
            .on_hover_text(
                "Spark wiring is separate from cam timing. Missing or mistimed sparks can misfire.",
            )
            .changed()
        {
            let numbers: Vec<u8> = text
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|n| n.parse().ok())
                .take(12)
                .collect();
            let mut candidate = *design;
            candidate.order_len = numbers.len() as u8;
            candidate.firing_order[..numbers.len()].copy_from_slice(&numbers);
            if candidate.validate().is_ok() {
                *design = candidate;
            }
        }
    });
    if firing_text(design) != *text {
        ui.colored_label(
            Color32::YELLOW,
            format!(
                "Use cylinder numbers 1–{} (repeats allowed); playing {}",
                design.cylinders,
                firing_text(design)
            ),
        );
    }
    section(ui, "Crankpins and banks", |ui| {
        egui::Grid::new("cylinders").num_columns(4).show(ui, |ui| {
            for i in 0..design.cylinders as usize {
                ui.label(format!("Cylinder {}", i + 1));
                ui.add(
                    egui::DragValue::new(&mut design.pins[i])
                        .range(0.0..=359.9)
                        .speed(1.)
                        .suffix("°"),
                );
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut design.banks[i], 0, "Bank 1");
                    ui.selectable_value(&mut design.banks[i], 1, "Bank 2");
                });
                if let Some(revolutions) = &mut design.cam_revolutions {
                    ui.checkbox(&mut revolutions[i], "Compression on second turn");
                }
                ui.end_row();
            }
        });
    });
    if design.banks[..design.cylinders as usize].contains(&1) {
        slider(
            ui,
            "Bank 2 level (dB)",
            &mut design.bank_gain_db,
            -12.0..=12.0,
        );
        slider(
            ui,
            "Bank 2 header delay (ms)",
            &mut design.bank_delay_ms,
            0.0..=5.0,
        );
    }
    let firing = design.firing();
    let mut events: Vec<(f32, u8)> = (0..firing.events)
        .map(|k| (firing.angles[k], firing.cylinder[k]))
        .collect();
    events.sort_by(|a, b| a.0.total_cmp(&b.0));
    let gaps: Vec<String> = events
        .iter()
        .enumerate()
        .map(|(k, e)| {
            let next = events.get(k + 1).map_or(events[0].0 + 720., |n| n.0);
            format!("{:.0}", next - e.0)
        })
        .collect();
    ui.small(format!(
        "Fires at: {} · intervals {}°",
        events
            .iter()
            .map(|(a, c)| format!("{}@{a:.0}°", c + 1))
            .collect::<Vec<_>>()
            .join("  "),
        gaps.join(" / ")
    ));
}
fn percent_slider(ui: &mut egui::Ui, label: &str, value: &mut f32) -> egui::Response {
    let mut percent = *value * 100.;
    let mut response = ui.add(
        egui::Slider::new(&mut percent, 0.0..=100.0)
            .suffix(" %")
            .text(label),
    );
    wheel_adjust(ui, &mut response, &mut percent, 0., 100.);
    *value = percent / 100.;
    response
}
const WHEEL_HELP: &str = "Mouse wheel: fixed steps (1, 0.1 or 0.01) · Shift + wheel: 10× finer";
fn slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    slider_help(ui, label, v, range, "");
}
/// A slider whose unit, written in the label as "Bass (dB)", shows as a suffix.
/// `help` (what it does) leads its tooltip, followed by the wheel hint.
fn slider_help(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    help: &str,
) {
    let (min, max) = (*range.start(), *range.end());
    let (text, suffix) = ui_kit::split_unit(label);
    let mut response = ui.add(egui::Slider::new(v, range).text(text).suffix(suffix));
    wheel_adjust(ui, &mut response, v, min, max);
    response.on_hover_text(if help.is_empty() {
        WHEEL_HELP.to_owned()
    } else {
        format!("{help}\n{WHEEL_HELP}")
    });
}
/// A slider for a geometry control that shapes the sound but not the torque.
fn sound_only_slider(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    help: &str,
) {
    ui.horizontal_wrapped(|ui| {
        slider_help(ui, label, v, range, help);
        ui_kit::sound_only_tag(ui);
    });
}
/// One listening-position selector for every source. `active` is false while
/// the choice is not the one in force (imported vehicles, live mix); true when
/// a click returned `true` means the caller should make it so.
fn camera_row(ui: &mut egui::Ui, camera: &mut BeamNgCamera, active: bool) -> bool {
    let mut clicked = false;
    ui.horizontal_wrapped(|ui| {
        ui.label("Listening position:");
        for (option, label) in [
            (BeamNgCamera::Cockpit, "Cabin"),
            (BeamNgCamera::Hood, "Engine bay"),
            (BeamNgCamera::Tailpipe, "Tailpipe"),
            (BeamNgCamera::Orbit, "Outside"),
        ] {
            if ui
                .selectable_label(active && *camera == option, label)
                .on_hover_text(option.description())
                .clicked()
            {
                *camera = option;
                clicked = true;
            }
        }
    });
    clicked
}
/// Exhaust, intake and mechanical layer levels and the two layer gains.
fn layer_mix_controls(ui: &mut egui::Ui, params: &mut Parameters, settings: &mut Settings) {
    slider(ui, "Exhaust level", &mut params.exhaust, 0.0..=1.0);
    slider(ui, "Intake level", &mut params.intake, 0.0..=1.0);
    slider(ui, "Mechanical level", &mut params.mechanical, 0.0..=1.0);
    slider_help(
        ui,
        "Engine layer gain",
        &mut settings.engine_gain,
        0.0..=2.0,
        "Level of the engine layer against the exhaust in the mix.",
    );
    slider_help(
        ui,
        "Idle gain",
        &mut settings.idle_gain,
        0.0..=2.0,
        "Level of the idle sound. Also sets the idle level of the BeamNG export.",
    );
}
fn adjacent_level_points(
    report: &beamng_level::Report,
    load: f32,
    rpm: f32,
) -> Option<(&beamng_level::Point, &beamng_level::Point)> {
    let row = || {
        report
            .points
            .iter()
            .filter(|point| (point.load - load).abs() < 0.01)
    };
    let lower = row()
        .filter(|point| point.rpm <= rpm)
        .max_by(|a, b| a.rpm.total_cmp(&b.rpm))
        .or_else(|| row().min_by(|a, b| a.rpm.total_cmp(&b.rpm)))?;
    let upper = row()
        .filter(|point| point.rpm >= rpm)
        .min_by(|a, b| a.rpm.total_cmp(&b.rpm))
        .or_else(|| row().max_by(|a, b| a.rpm.total_cmp(&b.rpm)))?;
    Some((lower, upper))
}
fn level_span(
    pair: (&beamng_level::Point, &beamng_level::Point),
    value: impl Fn(&beamng_level::Point) -> f32,
    unit: &str,
) -> String {
    let a = value(pair.0);
    let b = value(pair.1);
    if a == f32::NEG_INFINITY && b == f32::NEG_INFINITY {
        return "silent".to_owned();
    }
    if a == f32::NEG_INFINITY || b == f32::NEG_INFINITY {
        let audible = if a.is_finite() { a } else { b };
        return format!("silent to {audible:+.1} {unit}");
    }
    if (a - b).abs() < 0.05 {
        format!("{a:+.1} {unit}")
    } else {
        format!("{:+.1} to {:+.1} {unit}", a.min(b), a.max(b))
    }
}
fn show_level_report(ui: &mut egui::Ui, report: &beamng_level::Report, rpm: f32) {
    let (Some(off), Some(on)) = (
        adjacent_level_points(report, 0., rpm),
        adjacent_level_points(report, 1., rpm),
    ) else {
        ui.small("The sound bank has no usable off-load and on-load sample rows.");
        return;
    };
    ui.small(format!(
        "At {rpm:.0} rpm · off-load points: {:.0}–{:.0} rpm · full-load points: {:.0}–{:.0} rpm",
        off.0.rpm, off.1.rpm, on.0.rpm, on.1.rpm
    ));
    let exhaust_difference_label = if report.post_low_cut_estimate {
        "BESS exhaust vs original (80 Hz estimate)"
    } else {
        "BESS exhaust vs original · file AC RMS"
    };
    egui::Grid::new("beamng-export-levels")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("WAV level");
            ui.strong("Off load");
            ui.strong("Full load");
            ui.end_row();
            for (label, value) in [
                (
                    exhaust_difference_label,
                    (|point: &beamng_level::Point| point.exhaust_vs_source_db)
                        as fn(&beamng_level::Point) -> f32,
                ),
                (
                    "Added engine vs BESS exhaust · file AC RMS",
                    (|point: &beamng_level::Point| point.engine_vs_exhaust_db)
                        as fn(&beamng_level::Point) -> f32,
                ),
            ] {
                ui.label(label);
                ui.label(level_span(off, value, "dB"));
                ui.label(level_span(on, value, "dB"));
                ui.end_row();
            }
        });
    if report.post_low_cut_estimate {
        ui.small("The exhaust comparison applies a modeled 80 Hz high-pass to the original WAV and to the final exported PCM24. The 80 Hz setting is verified for the current Automation fleet; the exact BeamNG filter and perceived loudness remain unknown. Detailed AC RMS and peaks below remain unfiltered file measurements.");
    } else {
        ui.small("Positive exhaust values have more AC energy than the original WAV at the same RPM and load. DC offset is excluded from RMS; engine and exhaust play from different locations in the vehicle.");
    }
    ui.add_space(4.0);
    ui.strong("BeamNG camera volume preview");
    ui.small("Estimated in-game levels by camera position and cabin insulation:");
    egui::Grid::new("beamng-camera-levels")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Camera / Perspective");
            ui.strong("Off load (idle/decel)");
            ui.strong("Full load");
            ui.strong("Engine / Exhaust balance");
            ui.end_row();
            for camera in [
                BeamNgCamera::Cockpit,
                BeamNgCamera::Hood,
                BeamNgCamera::Tailpipe,
                BeamNgCamera::Orbit,
            ] {
                let off_rms = level_span(off, |p| p.camera_rms_dbfs(camera), "dBFS");
                let on_rms = level_span(on, |p| p.camera_rms_dbfs(camera), "dBFS");
                let eng_share = on.1.camera_engine_share(camera);
                let ex_share = 100. - eng_share;
                ui.label(camera.label());
                ui.label(off_rms);
                ui.label(on_rms);
                ui.label(format!("{eng_share:.0}% eng. / {ex_share:.0}% exh."));
                ui.end_row();
            }
        });
    section(ui, "WAV level details", |ui| {
        egui::Grid::new("beamng-export-level-details")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("AC RMS / peak (dBFS)");
                ui.strong("Off load");
                ui.strong("Full load");
                ui.end_row();
                for (label, value) in [
                    (
                        "Original exhaust AC RMS",
                        (|point: &beamng_level::Point| point.source_rms_dbfs)
                            as fn(&beamng_level::Point) -> f32,
                    ),
                    (
                        "BESS exhaust AC RMS",
                        (|point: &beamng_level::Point| point.exhaust_rms_dbfs)
                            as fn(&beamng_level::Point) -> f32,
                    ),
                    (
                        "BESS engine AC RMS",
                        (|point: &beamng_level::Point| point.engine_rms_dbfs)
                            as fn(&beamng_level::Point) -> f32,
                    ),
                    (
                        "BESS exhaust peak",
                        (|point: &beamng_level::Point| point.exhaust_peak_dbfs)
                            as fn(&beamng_level::Point) -> f32,
                    ),
                    (
                        "BESS engine peak",
                        (|point: &beamng_level::Point| point.engine_peak_dbfs)
                            as fn(&beamng_level::Point) -> f32,
                    ),
                ] {
                    ui.label(label);
                    ui.label(level_span(off, value, "dBFS"));
                    ui.label(level_span(on, value, "dBFS"));
                    ui.end_row();
                }
            });
        ui.small("BESS export files are mono 48 kHz / 24-bit PCM; the imported Automation WAV may use another format.");
    });
    ui.small("The added engine emitter uses a -2 dB base gain; the exhaust keeps the original vehicle's gain. BeamNG also applies cabin filtering, exhaust parts, camera distance, and its own mixer, so file levels are not guaranteed in-game loudness.");
}
#[cfg(test)]
mod level_ui_tests {
    use super::*;

    #[test]
    fn inspection_rpm_uses_neighboring_export_points_without_interpolating_a_claimed_level() {
        let point = |rpm, load, delta| beamng_level::Point {
            rpm,
            load,
            source_rms_dbfs: -30.,
            exhaust_rms_dbfs: -30. + delta,
            engine_rms_dbfs: -40.,
            exhaust_peak_dbfs: -5.,
            engine_peak_dbfs: -8.,
            exhaust_vs_source_db: delta,
            engine_vs_exhaust_db: -10.,
        };
        let report = beamng_level::Report {
            safety_gain: 1.,
            post_low_cut_estimate: false,
            points: vec![
                point(4989., 0., -1.0),
                point(5338., 0., 0.5),
                point(4989., 1., -2.0),
                point(5338., 1., -0.5),
            ],
        };
        let off = adjacent_level_points(&report, 0., 5200.).unwrap();
        let on = adjacent_level_points(&report, 1., 5200.).unwrap();
        assert_eq!((off.0.rpm, off.1.rpm), (4989., 5338.));
        assert_eq!((on.0.rpm, on.1.rpm), (4989., 5338.));
        assert_eq!(
            level_span(off, |point| point.exhaust_vs_source_db, "dB"),
            "-1.0 to +0.5 dB"
        );
        assert_eq!(
            adjacent_level_points(&report, 0., 700.).unwrap().0.rpm,
            4989.
        );
        assert_eq!(
            adjacent_level_points(&report, 0., 9000.).unwrap().1.rpm,
            5338.
        );
    }

    #[test]
    fn camera_level_preview_orders_cockpit_quieter_than_tailpipe() {
        let pt = beamng_level::Point {
            rpm: 3000.,
            load: 1.,
            source_rms_dbfs: -15.,
            exhaust_rms_dbfs: -15.,
            engine_rms_dbfs: -23.,
            exhaust_peak_dbfs: -6.,
            engine_peak_dbfs: -12.,
            exhaust_vs_source_db: 0.,
            engine_vs_exhaust_db: -8.,
        };
        let cockpit = pt.camera_rms_dbfs(BeamNgCamera::Cockpit);
        let tailpipe = pt.camera_rms_dbfs(BeamNgCamera::Tailpipe);
        let hood = pt.camera_rms_dbfs(BeamNgCamera::Hood);
        let orbit = pt.camera_rms_dbfs(BeamNgCamera::Orbit);
        assert!(
            cockpit < tailpipe,
            "Cockpit should be quieter than tailpipe due to cabin insulation"
        );
        assert!(
            cockpit.is_finite() && tailpipe.is_finite() && hood.is_finite() && orbit.is_finite()
        );
        assert!(
            pt.camera_engine_share(BeamNgCamera::Hood)
                > pt.camera_engine_share(BeamNgCamera::Tailpipe)
        );
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        // Only a currently held, visible starter button can request cranking.
        self.settings.starter = false;
        ctx.request_repaint_after(Duration::from_millis(33));
        self.frames += 1;
        if let Some(path) = &self.capture {
            let level_ready = self
                .level_report
                .as_ref()
                .is_some_and(|(key, _)| self.level_key().as_ref() == Some(key));
            if (self.bank.is_some() || self.scratch.is_some())
                && self.frames > 20
                && !self.capture_requested
                && !self.dyno.busy()
                && (!self.capture_level || level_ready)
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.capture_requested = true;
            }
            let screenshot = ctx.input(|i| {
                i.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(screenshot) = screenshot {
                let bytes: Vec<u8> = screenshot
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_array())
                    .collect();
                self.status = match image::save_buffer(
                    path,
                    &bytes,
                    screenshot.width() as u32,
                    screenshot.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => format!("Validation screenshot: {}", path.display()),
                    Err(e) => e.to_string(),
                };
                self.capture = None;
            }
        }
        if self.audio.is_some()
            && self.level_worker.is_none()
            && !ctx.wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
        {
            self.playing = !self.playing;
        }
        self.sync_scratch();
        if let Some(audio) = &self.audio {
            let dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.2);
            self.readout.update(audio, dt);
        }
        if let Some(rx) = &self.importer
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(v) => {
                    self.params = v.params;
                    self.level_rpm = v.params.rpm;
                    self.settings = v.settings;
                    self.driving = v.driving;
                    self.profile_name = v.profile_name;
                    self.bank = Some(v.bank);
                    self.vehicle = v.vehicle;
                    self.automation_model = v.automation_model;
                    self.scratch = None;
                    self.status = if v.attached_engine_reference {
                        "Engine data attached to this older project. Save to retain its verified physical reference.".into()
                    } else {
                        "Ready. Compare the original Automation source and the BESS physical engine.".into()
                    };
                    self.reconnect();
                    if self.capture_level {
                        self.start_level_analysis();
                    }
                }
                Err(e) => self.status = format!("Import failed: {e}"),
            }
            self.importer = None;
        }
        if let Some(rx) = &self.worker
            && let Ok(result) = rx.try_recv()
        {
            self.status = result.unwrap_or_else(|e| format!("Error: {e}"));
            self.worker = None;
        }
        if let Some(rx) = &self.level_worker {
            match rx.try_recv() {
                Ok(result) => {
                    let requested = self.level_pending.take();
                    let still_current = requested.as_ref() == self.level_key().as_ref();
                    match (requested, result) {
                        (Some(key), Ok(report)) => {
                            self.level_report = Some((key, report));
                            self.level_error = None;
                            self.status = if still_current {
                                "BeamNG file level analysis ready.".into()
                            } else {
                                "BeamNG level analysis finished for earlier settings. Analyze again."
                                    .into()
                            };
                        }
                        (_, Err(error)) => {
                            self.level_error = Some(error);
                        }
                        _ => {}
                    }
                    self.level_worker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.level_error = Some("The level analysis stopped unexpectedly.".into());
                    self.level_pending = None;
                    self.level_worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(10.);
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("BESS")
                        .color(Color32::from_rgb(75, 222, 195))
                        .size(30.),
                );
                ui.label("ENGINE SOUND WORKSHOP");
                ui.separator();
                ui.label(concat!(
                    env!("CARGO_PKG_VERSION"),
                    " · Dynamic acoustics + BDSP"
                ));
                ui.separator();
                if ui.selectable_label(self.show_driving, "Driving").clicked() {
                    self.show_driving = !self.show_driving;
                }
            });
            ui.add_space(8.);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });
        egui::SidePanel::left("controls")
            .min_width(380.)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Engine and sound");
                    ui.separator();
                    ui.heading("01 / Source vehicle");
                    if ui
                        .add_enabled(
                            self.importer.is_none(),
                            egui::Button::new("Import Automation ZIP…"),
                        )
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Vehicle", &["zip"])
                            .pick_file()
                    {
                        self.import(path, None);
                    }
                    if ui
                        .add_enabled(self.importer.is_none(), egui::Button::new("New engine from scratch"))
                        .on_hover_text("Design an engine sound without an Automation ZIP.")
                        .clicked()
                    {
                        self.start_scratch(Scratch::default(), true);
                    }
                    if self.scratch.is_some() {
                        self.scratch_panel(ui);
                    }
                    if let Some(bank) = &self.bank {
                        ui.label(
                            self.vehicle
                                .as_ref()
                                .map(|v| v.name.as_str())
                                .unwrap_or("Automation sound bank"),
                        );
                        ui.small(format!(
                            "{} + {} sounds · {:.0} to {:.0} rpm",
                            bank.layers[0].len(),
                            bank.layers[1].len(),
                            bank.min_rpm,
                            bank.max_rpm
                        ));
                        ui.small("Off-load + on-load · source files preserved");
                        if let Some(meta) = &bank.engine_meta {
                            let layout = match meta.layout {
                                bess::engine_meta::Layout::Inline => format!("L{}", meta.cylinders),
                                bess::engine_meta::Layout::V { bank_angle_degrees } => {
                                     format!("V{} at {}°", meta.cylinders, bank_angle_degrees)
                                }
                            };
                            ui.small(format!(
                                "Automation engine data: {}{}",
                                layout,
                                meta.displacement_l
                                    .map(|capacity| format!(" · {:.2} L", capacity))
                                    .unwrap_or_default()
                            ));
                        }
                    }
                    if self.scratch.is_none() && let Some(model) = &self.automation_model {
                        ui.separator();
                        ui.heading("02 / Physical engine sound");
                        match model {
                            Ok(model) => {
                                ui.small("Automation engine data configures our physical engine. Original recordings remain available as A.");
                                ui.label("Model assumptions:");
                                for assumption in &model.assumptions { ui.small(assumption); }
                                layer_mix_controls(ui, &mut self.params, &mut self.settings);
                                section(ui, "Combustion and exhaust geometry", |ui| {
                                    combustion_controls(ui, &mut self.settings.physical_sound);
                                    exhaust_geometry_controls(ui, &mut self.settings.physical_sound, model.scratch.build.muffler != bess::engine_build::Muffler::None);
                                });
                                sound_tuning_controls(ui, &mut self.settings.physical_sound);
                            }
                            Err(error) => { ui.colored_label(Color32::YELLOW, format!("Source A is available. Physical resynthesis unavailable: {error}")); }
                        }
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Save project").clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("BESS project", &["json"])
                                .set_file_name("engine.bess.json")
                                .save_file()
                        {
                            self.status = match project::save_project(&path, &self.project()) {
                                Ok(()) => "Project and sound bank reference saved.".into(),
                                Err(e) => e,
                            };
                        }
                        if ui
                            .add_enabled(self.importer.is_none(), egui::Button::new("Open"))
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("BESS project", &["json"])
                                .pick_file()
                        {
                            self.open_project(&path);
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ctx,|ui|{egui::ScrollArea::vertical().show(ui,|ui|{
            let top_height=(ui.available_height()*0.57).clamp(330.,460.);
            if self.show_driving && ui.available_width()>=740. {
                ui.columns(2,|columns| {
                    egui::ScrollArea::vertical().id_salt("listen-dock").max_height(top_height).auto_shrink([false,false]).show(&mut columns[0],|ui|self.listen_controls(ui));
                    egui::Frame::group(columns[1].style()).show(&mut columns[1],|ui| {
                        egui::ScrollArea::vertical().id_salt("drive-dock").max_height(top_height).auto_shrink([false,false]).show(ui,|ui|self.driving_panel(ui));
                    });
                });
            } else {
                self.listen_controls(ui);
                if self.show_driving {
                    ui.separator();
                    egui::Frame::group(ui.style()).show(ui,|ui|self.driving_panel(ui));
                }
            }
            ui.add_space(10.);
            if let Some(audio)=&self.audio {
                ui.small(&audio.description);if audio.meter.failed.load(Ordering::Relaxed){ui.colored_label(Color32::LIGHT_RED,"Audio stream interrupted. Reconnect audio.");}
                let dropouts = audio.pipe_stats.underruns.load(Ordering::Relaxed);
                let producer_ms = audio.pipe_stats.max_render_ns.load(Ordering::Relaxed) as f64 / 1_000_000.;
                ui.small(format!("Audio dropouts: {dropouts}"))
                    .on_hover_text(format!("Since connecting audio. Longest synthesis block: {producer_ms:.2} ms for {:.1} ms of sound. Output buffer: {:.1} ms.", audio.block_ms, audio.buffer_ms));
                if audio.meter.physical_failed.load(Ordering::Relaxed) { ui.colored_label(Color32::LIGHT_RED,"Physical engine unavailable or stopped. Check the imported engine data and reconnect audio."); }
                let peak_db=self.readout.peak_db;
                ui.add(egui::ProgressBar::new(((peak_db+60.)/60.).clamp(0.,1.)).text(format!("Peak {peak_db:.0} dBFS")));
                spectrum_plot(ui,&self.readout.spectrum);
            }
            // Scratch engines export WAV only: the BeamNG block needs an imported vehicle.
            if self.scratch.is_none() {
            ui.separator();ui.heading("Export BeamNG");
            ui.small("Adds a BESS configuration to the original Automation vehicle (keep the original mod enabled). It receives the same physical engine as B listening; the vehicle's own afterfire, turbo and startup sounds are preserved.");
            ui.horizontal(|ui| {
                ui.label("Sound profile name");
                ui.text_edit_singleline(&mut self.profile_name);
            });
            ui.small("Named profiles appear as separate BESS configurations for the same vehicle.");
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong("BeamNG volume estimate");
                ui.small("Compare the WAV levels BESS will export with the original Automation samples. Listening volume does not change these files. Idle gain is set with the layer mix.");
                if ui
                    .add_enabled(
                        self.bank.is_some()
                            && !self.playing
                            && self.level_worker.is_none()
                            && self.worker.is_none()
                            && self.importer.is_none(),
                        egui::Button::new("Calculate BeamNG level"),
                    )
                    .clicked()
                {
                    self.start_level_analysis();
                }
                if self.level_worker.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Analyzing every RPM and load sample…");
                    });
                }
                if self.playing && self.level_worker.is_none() {
                    ui.small("Stop listening to calculate; an existing analysis can still follow live RPM.");
                }
                if let Some(error) = &self.level_error {
                    ui.colored_label(Color32::LIGHT_RED, format!("Level analysis: {error}"));
                }
                if let Some((measured_key, report)) = &self.level_report {
                    if self.level_key().as_ref() == Some(measured_key) {
                        if let Some(bank) = &self.bank {
                            self.level_rpm = self.level_rpm.clamp(bank.min_rpm, bank.max_rpm);
                            slider(
                                ui,
                                "Inspect at RPM",
                                &mut self.level_rpm,
                                bank.min_rpm..=bank.max_rpm,
                            );
                            show_level_report(ui, report, self.level_rpm);
                        }
                    } else if self.level_worker.is_none() {
                        ui.small("Sound settings changed. Calculate again for the current export.");
                    }
                } else if self.bank.is_some() && self.level_worker.is_none() {
                    ui.small("Calculate once to inspect both load rows across the imported RPM range.");
                }
            });
            if ui.add_enabled(self.bank.is_some()&&self.worker.is_none()&&self.level_worker.is_none()&&project::validate_profile_name(&self.profile_name).is_ok(),egui::Button::new("Create BeamNG configuration…")).clicked()
                &&let Some(dir)=rfd::FileDialog::new().pick_folder(){
                let bank=self.bank.clone().unwrap();let p=self.params;let h=self.settings.for_beamng_export();let profile=self.profile_name.trim().to_owned();
                let (tx,rx)=mpsc::channel();self.worker=Some(rx);self.status="Creating and verifying BeamNG configuration…".into();
                std::thread::spawn(move||{let folder=dir.join(format!("BESS-BeamNG-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
                    let _=tx.send(bess::variant::package_named(&folder,p,h,bank,&profile));});
            }
            }
            ui.separator();ui.heading("Listening exports");
            slider(ui,"WAV duration (seconds)",&mut self.seconds,1.0..=60.0);
            ui.small(match self.driving.mode {
                Mode::Simulated=>"WAV: starts from standstill with current throttle, brake, and load held; transmission follows the selected mode.",
                Mode::Direct=>"WAV: holds the currently requested RPM and load.",
                Mode::Cycle=>"WAV: complete comparison cycle fitted to the selected duration."
            });
            ui.small("Mono 48 kHz / 24-bit. Rendering does not record earlier control changes.");
            if ui.add_enabled((self.bank.is_some()||self.scratch.is_some())&&self.worker.is_none(),egui::Button::new("Export selected mode…")).clicked()
                &&let Some(path)=rfd::FileDialog::new().add_filter("Audio",&["wav"]).set_file_name(if self.scratch.is_some(){"scratch-engine.wav"}else{"physical-engine.wav"}).save_file(){
                let bank=self.bank.clone();let scratch=self.scratch.clone();let p=self.params;let h=self.settings;let seconds=self.seconds;let driving=self.driving;
                let (tx,rx)=mpsc::channel();self.worker=Some(rx);self.status="Rendering audio…".into();
                std::thread::spawn(move||{
                    let result=match (scratch,bank) {
                        (Some(scratch),_)=>render::scratch_wav(&path,p,h,&scratch,seconds,driving),
                        (None,Some(bank))=>render::bench_wav(&path,p,h,bank,seconds,driving),
                        (None,None)=>Err("Import a vehicle or start a scratch engine".into()),
                    };
                    let _=tx.send(result.map(|()|format!("WAV : {}",path.display())));});
            }
            if ui.add_enabled(self.bank.is_some()&&self.worker.is_none(),egui::Button::new("Export A/B comparison (16 s)…")).clicked()
                &&let Some(dir)=rfd::FileDialog::new().pick_folder(){
                let bank=self.bank.clone().unwrap();let p=self.params;let h=self.settings;
                let (tx,rx)=mpsc::channel();self.worker=Some(rx);self.status="Rendering level-matched comparison…".into();
                std::thread::spawn(move||{let folder=dir.join(format!("BESS-AB-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()));
                    let _=tx.send(render::comparison(&folder,p,h,bank).map(|_|format!("Comparison: {}",folder.display())));});
            }
            if self.worker.is_some()||self.importer.is_some(){ui.spinner();}
            ui.small("A/B exports use the comparison cycle.");
            ui.add_space(12.);ui.small("Imported engine data and model assumptions are listed in the sound settings.");
        });});

        let command = audio::Command {
            params: self.params,
            settings: self.settings,
            playing: self.playing,
            driving: self.driving,
            restart: self.restart,
            audition_mix: self.audition_mix,
            camera: self.camera,
            room: self.room,
            room_mix: self.room_mix,
        };
        if self.sent != Some(command)
            && let Some(audio) = &self.audio
            && audio.tx.try_send(command).is_ok()
        {
            self.sent = Some(command);
        }
    }
}
/// Fixed test-stand cycle: no drivetrain can run away beyond requested RPM.
fn audio_check_operating_point(seconds: f64) -> (f32, f32) {
    match ((seconds.max(0.) / 3.).floor() as u64) % 5 {
        0 => (1200., 0.1),
        1 => (3000., 0.8),
        2 => (6000., 0.8),
        3 => (6000., 0.),
        _ => (3000., 0.8),
    }
}

#[cfg(test)]
mod audio_check_tests {
    use super::audio_check_operating_point;
    #[test]
    fn cycle_contains_idle_load_high_rpm_fuel_cut_and_recovery() {
        assert_eq!(audio_check_operating_point(0.), (1200., 0.1));
        assert_eq!(audio_check_operating_point(2.99), (1200., 0.1));
        assert_eq!(audio_check_operating_point(3.), (3000., 0.8));
        assert_eq!(audio_check_operating_point(6.), (6000., 0.8));
        assert_eq!(audio_check_operating_point(9.), (6000., 0.));
        assert_eq!(audio_check_operating_point(12.), (3000., 0.8));
        assert_eq!(audio_check_operating_point(15.), (1200., 0.1));
        assert_eq!(audio_check_operating_point(599.), (3000., 0.8));
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--inspect") {
        let result = args
            .get(2)
            .ok_or("ZIP file required".to_owned())
            .and_then(|p| beamng::inspect(Path::new(p)))
            .map(|v| format!("{v:#?}"));
        let ok = result.is_ok();
        if let Some(path) = args.get(3) {
            let _ = std::fs::write(path, result.unwrap_or_else(|e| e));
        }
        if !ok {
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--render") {
        let path = args.get(2).map(String::as_str).unwrap_or("bess-demo.wav");
        if render::wav(Path::new(path), Parameters::default(), 10., true).is_err() {
            std::process::exit(1);
        }
        return Ok(());
    }
    if matches!(
        args.get(1).map(String::as_str),
        Some(
            "--compare" | "--drive-demo" | "--beamng" | "--beamng-profile" | "--beamng-replacement"
        )
    ) {
        let result = (|| {
            let path = args.get(2).ok_or("ZIP file required")?;
            let dir = args.get(3).ok_or("Output folder required")?;
            let bank = Arc::new(Bank::load(Path::new(path), None)?);
            let idle = beamng::inspect(Path::new(path))
                .ok()
                .and_then(|v| v.idle_rpm)
                .unwrap_or(bank.min_rpm)
                .clamp(bank.min_rpm, bank.max_rpm);
            let params = Parameters {
                cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
                rpm: idle,
                load: 0.12,
                brightness: 10000.,
                exhaust: 1.,
                intake: 0.25,
                ..Default::default()
            };
            let settings = Settings::calibrated(&bank);
            if args[1] == "--beamng-profile" {
                bess::variant::package_named(
                    Path::new(dir),
                    params,
                    settings,
                    bank,
                    args.get(4).ok_or("Profile name required")?,
                )
            } else if args[1] == "--beamng" {
                bess::variant::package(Path::new(dir), params, settings, bank)
            } else if args[1] == "--beamng-replacement" {
                bess::export::package(Path::new(dir), params, settings, bank)
            } else if args[1] == "--drive-demo" {
                render::drive_demo(Path::new(dir), params, bank)
            } else {
                render::comparison(Path::new(dir), params, settings, bank)
            }
        })();
        if let Err(e) = result {
            if let Some(dir) = args.get(3) {
                eprintln!("{e}");
                // Windows GUI builds have no console, so keep error.txt, but never overwrite a user's file.
                let _ = std::fs::create_dir_all(dir);
                if let Ok(mut file) = std::fs::File::create_new(Path::new(dir).join("error.txt")) {
                    let _ = std::io::Write::write_all(&mut file, e.as_bytes());
                }
            }
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--audio-check") {
        let result = (|| -> Result<String, String> {
            let seconds = args
                .get(4)
                .map(|s| s.parse::<u64>())
                .transpose()
                .map_err(|_| "Invalid audio check duration")?
                .unwrap_or(5);
            if !(1..=600).contains(&seconds) {
                return Err("Audio check duration must be 1–600 seconds".into());
            }
            let mut params = Parameters {
                volume: 0.,
                ..Parameters::default()
            };
            let mut command = None;
            let mut cyclic = false;
            let engine_label;
            let scenario;
            let a = if args.get(3).is_some_and(|source| source == "scratch") {
                let preset_name = args.get(5).map(String::as_str).unwrap_or("Inline-4");
                let design = bess::scratch::PRESETS
                    .iter()
                    .find(|(name, _)| *name == preset_name)
                    .map(|(_, design)| *design)
                    .ok_or_else(|| format!("Unknown physical engine preset: {preset_name}"))?;
                let mut scratch = Scratch {
                    design,
                    ..Scratch::default()
                };
                let mut settings = Settings::default();
                let mut driving = Controls {
                    mode: Mode::Direct,
                    ..Default::default()
                };
                scratch.derive_from_build(&mut settings, &mut params, &mut driving);
                match args.get(6).map(String::as_str).unwrap_or("steady") {
                    "cycle" => cyclic = true,
                    "steady" => (),
                    other => {
                        return Err(format!(
                            "Unknown scratch audio check scenario: {other}; expected steady or cycle"
                        ));
                    }
                }
                (params.rpm, params.load) = audio_check_operating_point(0.);
                params.volume = 0.;
                engine_label = format!(
                    "Physical scratch / {preset_name} / {} cylinders",
                    scratch.design.cylinders
                );
                scenario = if cyclic {
                    "15 s cycle: 1200/.1, 3000/.8, 6000/.8, 6000/0 lift-off, 3000/.8 recovery; 3 s each"
                } else {
                    "Direct test stand: 1200 rpm, throttle .1"
                };
                let a = audio::Audio::with_scratch(params, settings, &scratch, driving)
                    .map_err(|e| format!("Engine: {engine_label}\nPhysical engine/audio initialization failed: {e}"))?;
                command = Some(audio::Command {
                    params,
                    settings,
                    playing: true,
                    driving,
                    restart: 0,
                    audition_mix: AuditionMix::Live,
                    camera: BeamNgCamera::Cockpit,
                    room: Room::Off,
                    room_mix: 0.,
                });
                a
            } else if let Some(path) = args.get(3) {
                let bank = Arc::new(Bank::load(Path::new(path), None)?);
                let settings = Settings::calibrated(&bank);
                let driving = Controls {
                    mode: Mode::Simulated,
                    throttle: 0.8,
                    ..Default::default()
                };
                engine_label = format!("Imported bank / {path}");
                scenario = "Imported-bank driving at throttle .8";
                let a = audio::Audio::with_bank(params, settings, Some(bank), driving)?;
                command = Some(audio::Command {
                    params,
                    settings,
                    playing: true,
                    driving,
                    restart: 0,
                    audition_mix: AuditionMix::Live,
                    camera: BeamNgCamera::Cockpit,
                    room: Room::Off,
                    room_mix: 0.,
                });
                a
            } else {
                engine_label = "Silent output check".into();
                scenario = "Output availability";
                audio::Audio::start(params)?
            };
            let start = std::time::Instant::now();
            let duration = Duration::from_secs(seconds);
            let mut command_updates = 0u64;
            // Command dispatch is bounded to 10 Hz; the synthesizer continues
            // at its normal fixed internal rate. Never launch a GUI for a check.
            while start.elapsed() < duration {
                if let Some(next) = &mut command {
                    if cyclic {
                        (next.params.rpm, next.params.load) =
                            audio_check_operating_point(start.elapsed().as_secs_f64());
                    }
                    if a.tx.try_send(*next).is_ok() {
                        command_updates += 1;
                    }
                }
                if a.meter.physical_failed.load(Ordering::Relaxed)
                    || a.pipe_stats.failed.load(Ordering::Relaxed)
                    || a.meter.failed.load(Ordering::Relaxed)
                {
                    break;
                }
                let remaining = duration.saturating_sub(start.elapsed());
                std::thread::sleep(remaining.min(Duration::from_millis(100)));
            }
            let n = a.meter.blocks.load(Ordering::Relaxed);
            let physical_failed = a.meter.physical_failed.load(Ordering::Relaxed);
            let producer_failed = a.pipe_stats.failed.load(Ordering::Relaxed);
            let failed = a.meter.failed.load(Ordering::Relaxed);
            let underruns = a.pipe_stats.underruns.load(Ordering::Relaxed);
            let p99 = a.meter.callback_p99_budget_percent();
            let status = if physical_failed {
                "FAIL: physical engine solver stopped"
            } else if producer_failed {
                "FAIL: audio producer stopped"
            } else if failed {
                "FAIL: output stream error"
            } else if n == 0 {
                "FAIL: output stream delivered no callbacks"
            } else if underruns > 0 {
                "FAIL: audio underrun observed"
            } else if p99 >= 50 {
                "FAIL: callback p99 is not below 50% of deadline"
            } else {
                "PASS: observed stream/producer/physical checks"
            };
            let report = format!(
                "{}\nOutput device: {} (Bluetooth: {})\nMeasured output latency: {:.1} ms (shown: {})\nEngine: {engine_label}\nScenario: {scenario}\nRequested duration: {seconds} s\nObserved duration: {:.3} s\nStatus: {status}\nPhysical engine failure: {physical_failed}\nProducer failure: {producer_failed}\nSynthesis rate: {} Hz\nSynthesis block: {:.3} ms\nCommand updates: {command_updates} (at most 10 Hz)\nCallbacks: {n}\nMaximum callback CPU time: {:.3} ms\nCallback p99 budget percent (bucket upper bound, 101 = overflow): {p99}\nBudget overruns: {}\nAudio underruns: {underruns}\nMissing frames: {}\nProduced blocks: {}\nMaximum synthesis block time: {:.3} ms\nSilent test: synthesis calculated, output volume set to zero.\n",
                a.description,
                a.device_name,
                a.bluetooth,
                f32::from_bits(a.meter.latency_ms.load(Ordering::Relaxed)),
                audio::latency_text(
                    f32::from_bits(a.meter.latency_ms.load(Ordering::Relaxed)),
                    a.bluetooth
                ),
                start.elapsed().as_secs_f64(),
                a.synth_rate,
                a.block_ms,
                a.meter.max_ns.load(Ordering::Relaxed) as f64 / 1e6,
                a.meter.overruns.load(Ordering::Relaxed),
                a.pipe_stats.missing_frames.load(Ordering::Relaxed),
                a.pipe_stats.produced_blocks.load(Ordering::Relaxed),
                a.pipe_stats.max_render_ns.load(Ordering::Relaxed) as f64 / 1e6,
            );
            if status.starts_with("FAIL") {
                Err(report)
            } else {
                Ok(report)
            }
        })();
        let ok = result.is_ok();
        let _ = std::fs::write(
            args.get(2).map(String::as_str).unwrap_or("audio-check.txt"),
            result.unwrap_or_else(|e| e),
        );
        if !ok {
            std::process::exit(1);
        }
        return Ok(());
    }
    let capture_level = args.get(1).is_some_and(|arg| arg == "--capture-level");
    let capture = if matches!(
        args.get(1).map(String::as_str),
        Some("--capture" | "--capture-driving" | "--capture-level")
    ) {
        args.get(2).map(PathBuf::from)
    } else {
        None
    };
    let initial = if capture.is_some() {
        args.get(3).map(PathBuf::from)
    } else if args.get(1).map(String::as_str) == Some("--open") {
        args.get(2).map(PathBuf::from)
    } else {
        None
    };
    eframe::run_native(
        "BESS — Physical Engine Sound",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(if capture_level {
                    [1180., 1400.]
                } else {
                    [1180., 860.]
                })
                .with_min_inner_size([960., 720.]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = App::new(cc, initial);
            app.capture = capture;
            app.capture_level = capture_level;
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod wheel_tests {
    use super::*;
    fn run(shift: bool, hover: bool, initial: f32) -> (f32, f32) {
        let (values, left) = run_events(
            shift,
            hover,
            initial,
            1.,
            egui::MouseWheelUnit::Point,
            &[50.],
        );
        (values[0], left)
    }

    fn run_events(
        shift: bool,
        hover: bool,
        initial: f32,
        max: f32,
        unit: egui::MouseWheelUnit,
        deltas: &[f32],
    ) -> (Vec<f32>, f32) {
        let ctx = egui::Context::default();
        let mut value = initial;
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = ui.add(egui::Slider::new(&mut value, 0.0..=max)).rect;
            });
        });
        let modifiers = egui::Modifiers {
            shift,
            ..Default::default()
        };
        let mut values = Vec::new();
        for &delta in deltas {
            let mut input = egui::RawInput {
                modifiers,
                events: vec![
                    egui::Event::PointerMoved(if hover {
                        rect.center()
                    } else {
                        egui::pos2(700., 500.)
                    }),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., delta),
                        modifiers,
                    },
                ],
                ..Default::default()
            };
            if let egui::Event::MouseWheel {
                unit: event_unit, ..
            } = &mut input.events[1]
            {
                *event_unit = unit;
            }
            if delta == 0. {
                input.events.pop();
            }
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut response = ui.add(egui::Slider::new(&mut value, 0.0..=max));
                    wheel_adjust(ui, &mut response, &mut value, 0., max);
                });
            });
            values.push(value);
        }
        (values, ctx.input(|i| i.smooth_scroll_delta.y))
    }
    #[test]
    fn wheel_adjusts_hovered_control_consumes_scroll_and_shift_is_finer() {
        let (normal, left) = run(false, true, 0.5);
        let (fine, _) = run(true, true, 0.5);
        assert!(normal > fine && fine > 0.5, "{normal} {fine}");
        assert_eq!(left, 0.);
        assert_eq!(run(false, false, 0.5).0, 0.5);
        assert!(run(false, true, 0.9999).0 <= 1.);
    }

    #[test]
    fn wheel_uses_integer_ticks_without_smoothing_tail() {
        let (values, left) = run_events(
            false,
            true,
            50.3,
            100.,
            egui::MouseWheelUnit::Line,
            &[1., 0., 0., -1., 4.],
        );
        assert_eq!(values, [51., 51., 51., 50., 54.]);
        assert_eq!(left, 0.);
        let (values, _) = run_events(
            false,
            true,
            99.8,
            100.,
            egui::MouseWheelUnit::Line,
            &[1., 1., -1.],
        );
        assert_eq!(values, [100., 100., 99.]);
    }

    #[test]
    fn fractional_scroll_accumulates_without_fractional_values() {
        let (values, _) = run_events(
            false,
            true,
            50.,
            100.,
            egui::MouseWheelUnit::Point,
            &[25., 0., 25., 0., -50.],
        );
        assert_eq!(values, [50., 50., 51., 51., 50.]);
        let (values, _) = run_events(
            false,
            true,
            50.,
            100.,
            egui::MouseWheelUnit::Point,
            &[25., -50.],
        );
        assert_eq!(values, [50., 49.]);
    }

    #[test]
    fn small_ranges_keep_clean_decimal_steps() {
        let (values, _) = run_events(
            false,
            true,
            0.5,
            1.,
            egui::MouseWheelUnit::Line,
            &[1., 0., -1.],
        );
        for (value, expected) in values.iter().zip([0.51, 0.51, 0.50]) {
            assert!((value - expected).abs() < 1e-6);
        }
        let (values, _) = run_events(false, true, 0.5, 2., egui::MouseWheelUnit::Line, &[1., -1.]);
        for (value, expected) in values.iter().zip([0.6, 0.5]) {
            assert!((value - expected).abs() < 1e-6);
        }
    }
}

/// Cylinder-pressure controls: they change torque as well as sound.
fn combustion_controls(ui: &mut egui::Ui, sound: &mut SoundTuning) {
    slider_help(
        ui,
        "Cycle variation (×)",
        &mut sound.cycle_variation,
        0.0..=2.0,
        "Cycle-to-cycle irregularity of the combustion.",
    );
    slider_help(
        ui,
        "Combustion duration (×)",
        &mut sound.combustion_duration,
        0.5..=1.5,
        "Changes cylinder pressure and torque.",
    );
    slider_help(
        ui,
        "Ignition retard (degrees)",
        &mut sound.ignition_retard_deg,
        -20.0..=20.0,
        "Changes cylinder pressure and torque; negative retard advances ignition.",
    );
    let neutral = SoundTuning::default();
    let set = (
        sound.cycle_variation,
        sound.combustion_duration,
        sound.ignition_retard_deg,
    ) != (
        neutral.cycle_variation,
        neutral.combustion_duration,
        neutral.ignition_retard_deg,
    );
    if ui
        .add_enabled(set, egui::Button::new("Reset combustion").small())
        .clicked()
    {
        sound.cycle_variation = neutral.cycle_variation;
        sound.combustion_duration = neutral.combustion_duration;
        sound.ignition_retard_deg = neutral.ignition_retard_deg;
    }
}

/// Exhaust lengths and muffler: sound only, because pressure waves do not
/// feed back on the valves in this model (X-021).
fn exhaust_geometry_controls(ui: &mut egui::Ui, sound: &mut SoundTuning, has_muffler: bool) {
    sound_only_slider(
        ui,
        "Header length (× part)",
        &mut sound.primary_length_scale,
        0.5..=2.0,
        "Scales the primary length of the fitted headers. Shifts the exhaust resonances.",
    );
    sound_only_slider(
        ui,
        "Tailpipe length (m)",
        &mut sound.tail_length_m,
        0.2..=5.0,
        "Shifts the exhaust resonances.",
    );
    ui.add_enabled_ui(has_muffler, |ui| {
        sound_only_slider(
            ui,
            "Muffler volume (× part)",
            &mut sound.muffler_volume_scale,
            0.25..=3.0,
            "Needs a fitted muffler.",
        );
        ui.horizontal_wrapped(|ui| {
            percent_slider(ui, "Muffler absorption", &mut sound.muffler_absorption)
                .on_hover_text("Needs a fitted muffler.");
            ui_kit::sound_only_tag(ui);
        });
    });
}

fn sound_tuning_controls(ui: &mut egui::Ui, sound: &mut SoundTuning) {
    section(ui, "Sound shaping", |ui| {
        ui.small("Saved with this engine. Part changes keep these adjustments.")
            .on_hover_text(
                "Shapes exhaust and intake. Mechanical sound has its own controls below.",
            );
        slider(ui, "Bass (dB)", &mut sound.bass_db, -12.0..=12.0);
        slider(ui, "Presence (dB)", &mut sound.presence_db, -12.0..=12.0);
        slider(ui, "Treble (dB)", &mut sound.treble_db, -12.0..=12.0);
        slider_help(
            ui,
            "Brightness cutoff (Hz)",
            &mut sound.brightness_hz,
            500.0..=20_000.0,
            "20,000 Hz leaves the top end open.",
        );
        percent_slider(ui, "Saturation", &mut sound.drive).on_hover_text("Rounds the peaks.");
        percent_slider(ui, "Flow texture", &mut sound.flow_texture)
            .on_hover_text("Texture that follows the engine signal.");
        slider(
            ui,
            "Brightness at high RPM (dB)",
            &mut sound.rpm_brightness_db,
            -12.0..=12.0,
        );
        slider(
            ui,
            "Brightness under load (dB)",
            &mut sound.load_brightness_db,
            -12.0..=12.0,
        );
    });
    section(ui, "Intake and mechanical character", |ui| {
        slider(
            ui,
            "Intake resonance",
            &mut sound.intake_resonance,
            0.0..=3.0,
        );
        ui.add_enabled_ui(sound.intake_resonance > 0., |ui| {
            sound_only_slider(
                ui,
                "Intake resonator length (m)",
                &mut sound.intake_length_m,
                0.15..=1.5,
                "Longer resonators deepen the intake note. Needs resonance above zero.",
            );
        });
        slider(
            ui,
            "Mechanical pitch (Hz)",
            &mut sound.mechanical_pitch_hz,
            600.0..=6000.0,
        );
        slider(
            ui,
            "Mechanical resonance",
            &mut sound.mechanical_resonance,
            0.5..=8.0,
        );
    });
    section(ui, "Exhaust tone", |ui| {
        ui.small("Only the exhaust outlet.").on_hover_text(
            "Intake and mechanical sound keep their own character. The controls above shape exhaust and intake together.",
        );
        slider(
            ui,
            "Exhaust bass (dB)",
            &mut sound.exhaust_bass_db,
            -12.0..=12.0,
        );
        slider_help(
            ui,
            "Exhaust body (dB)",
            &mut sound.exhaust_body_db,
            -12.0..=12.0,
            "Positive adds a resonant note; negative reduces drone.",
        );
        ui.add_enabled_ui(sound.exhaust_body_db != 0., |ui| {
            slider(
                ui,
                "Body frequency (Hz)",
                &mut sound.exhaust_body_hz,
                40.0..=2000.0,
            );
            slider_help(
                ui,
                "Body focus (Q)",
                &mut sound.exhaust_body_q,
                0.5..=8.0,
                "Higher Q targets a narrower band.",
            );
        });
        slider(
            ui,
            "Exhaust rasp (dB)",
            &mut sound.exhaust_rasp_db,
            -12.0..=12.0,
        );
        slider_help(
            ui,
            "Exhaust low cut (Hz)",
            &mut sound.exhaust_low_cut_hz,
            20.0..=300.0,
            "20 Hz bypasses the filter. Removes rumble.",
        );
        slider_help(
            ui,
            "Exhaust high cut (Hz)",
            &mut sound.exhaust_high_cut_hz,
            500.0..=20000.0,
            "20,000 Hz bypasses the filter. Softens the edge.",
        );
        percent_slider(ui, "Exhaust saturation", &mut sound.exhaust_drive);
        if ui.button("Reset exhaust tone").clicked() {
            sound.reset_exhaust_tone();
        }
    });
    if ui
        .button("Reset sound shaping")
        .on_hover_text("Reset the sound adjustments above; keep engine parts, combustion, exhaust geometry and layer levels.")
        .clicked()
    {
        let kept = *sound;
        *sound = SoundTuning {
            cycle_variation: kept.cycle_variation,
            combustion_duration: kept.combustion_duration,
            ignition_retard_deg: kept.ignition_retard_deg,
            primary_length_scale: kept.primary_length_scale,
            tail_length_m: kept.tail_length_m,
            muffler_volume_scale: kept.muffler_volume_scale,
            muffler_absorption: kept.muffler_absorption,
            ..Default::default()
        };
    }
}
