//! Local BeamNG folder, vehicle library and explicit export actions.
use super::*;
use bess::{
    beamng_library::VehicleArchive,
    beamng_paths,
    export_job::{ExportJob, ExportProgress, ExportStage},
};

fn elapsed_label(seconds: f64) -> String {
    let seconds = seconds.max(0.).round() as u64;
    if seconds >= 3600 {
        format!(
            "{}h {:02}m {:02}s",
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60
        )
    } else if seconds >= 60 {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn export_progress_controls(
    ui: &mut egui::Ui,
    progress: &ExportProgress,
    cancelling: bool,
) -> bool {
    let terminal = matches!(
        progress.stage,
        ExportStage::Complete | ExportStage::Cancelled | ExportStage::Failed
    );
    let stage = if cancelling && !terminal {
        "Cancelling export…"
    } else {
        match progress.stage {
            ExportStage::Preparing => "Preparing vehicle export",
            ExportStage::Rendering => "Generating engine sounds",
            ExportStage::Packaging => "Writing vehicle ZIP",
            ExportStage::Verifying => "Verifying vehicle ZIP",
            ExportStage::Complete => "Vehicle ZIP verified",
            ExportStage::Cancelled => "Export cancelled",
            ExportStage::Failed => "Export failed",
        }
    };
    let mut cancel = false;
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.strong(stage);
        let unit = match progress.stage {
            ExportStage::Preparing => Some("preparation steps"),
            ExportStage::Rendering => Some("sounds generated"),
            ExportStage::Packaging => Some("files written"),
            ExportStage::Verifying => Some("checks completed"),
            ExportStage::Complete | ExportStage::Cancelled | ExportStage::Failed => None,
        };
        if progress.total > 0
            && let Some(unit) = unit
        {
            ui.label(format!(
                "{} / {} {unit}",
                progress.completed, progress.total
            ));
        }
        cancel = ui
            .add_enabled(!cancelling && !terminal, egui::Button::new("Cancel export"))
            .clicked();
    });
    // Audio generation is only part of the job. A full bar means the ZIP passed verification.
    let fraction = match progress.stage {
        ExportStage::Preparing => 0.,
        ExportStage::Rendering => (progress.fraction.unwrap_or_else(|| {
            if progress.total == 0 {
                0.
            } else {
                progress.completed as f32 / progress.total as f32
            }
        }) * 0.94)
            .clamp(0., 0.94),
        ExportStage::Packaging => 0.95,
        ExportStage::Verifying => 0.98,
        ExportStage::Complete => 1.,
        ExportStage::Cancelled | ExportStage::Failed => {
            progress.fraction.unwrap_or(0.).clamp(0., 0.99)
        }
    };
    ui.add(egui::ProgressBar::new(fraction).show_percentage());
    ui.horizontal_wrapped(|ui| {
        ui.label(format!(
            "Elapsed: {}",
            elapsed_label(progress.elapsed_seconds)
        ));
        if let Some(rpm) = progress.current_rpm {
            let load = progress
                .current_load
                .map(|load| format!(" · {:.0}% load", load * 100.))
                .unwrap_or_default();
            ui.label(format!("Latest sound: {rpm:.0} rpm{load}"));
        }
        if progress.workers > 0 {
            ui.label(format!("{} parallel tasks", progress.workers));
        }
        if progress.stage == ExportStage::Rendering && !cancelling {
            ui.label(match progress.estimated_remaining_seconds {
                Some(seconds) => format!(
                    "Sound generation left (estimate): about {}",
                    elapsed_label(seconds)
                ),
                None => "Estimating time left…".into(),
            });
        }
    });
    if cancelling && !terminal {
        ui.small("Stopping work and removing the incomplete export. The original vehicle stays unchanged.");
    } else if !progress.detail.is_empty() {
        ui.small(&progress.detail);
    }
    cancel
}

fn folder_label(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(network) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{network}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
}

pub(super) enum FolderRequest {
    Discover,
    Select(PathBuf),
    Automatic,
    Session(CompanionWorkspace),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CompanionWorkspace {
    mods_dir: PathBuf,
    exports_dir: Option<PathBuf>,
}

/// These explicit companion arguments affect only this window, never saved preferences.
pub(super) fn companion_workspace(args: &[String]) -> Result<Option<CompanionWorkspace>, String> {
    let mut mods_dir = None;
    let mut exports_dir = None;
    let mut arguments = args.iter();
    while let Some(argument) = arguments.next() {
        let destination = match argument.as_str() {
            "--beamng-mods" => &mut mods_dir,
            "--bess-exports" => &mut exports_dir,
            _ => return Err(format!("Unknown BESS open argument: {argument}")),
        };
        if destination.is_some() {
            return Err(format!("Duplicate argument: {argument}"));
        }
        let value = arguments
            .next()
            .filter(|value| !value.starts_with("--"))
            .ok_or_else(|| format!("Folder required after {argument}"))?;
        *destination = Some(PathBuf::from(value));
    }
    match (mods_dir, exports_dir) {
        (Some(mods_dir), exports_dir) => Ok(Some(CompanionWorkspace {
            mods_dir,
            exports_dir,
        })),
        (None, None) => Ok(None),
        (None, Some(_)) => Err("--bess-exports requires --beamng-mods".into()),
    }
}

pub(super) fn initial_and_companion(
    args: &[String],
) -> Result<(Option<PathBuf>, Option<CompanionWorkspace>), String> {
    let initial = args
        .first()
        .filter(|arg| !arg.starts_with("--"))
        .map(PathBuf::from);
    let options = if initial.is_some() { &args[1..] } else { args };
    Ok((initial, companion_workspace(options)?))
}

pub(super) struct WorkspaceScan {
    folder: Option<beamng_paths::DetectedFolder>,
    vehicles: Vec<VehicleArchive>,
    export_dir: Option<PathBuf>,
    scan_error: Option<String>,
}

#[derive(Default)]
pub(super) struct BeamngWorkspace {
    folder: Option<beamng_paths::DetectedFolder>,
    vehicles: Vec<VehicleArchive>,
    export_dir: Option<PathBuf>,
    session: Option<CompanionWorkspace>,
    pending_export_dir: Option<PathBuf>,
    last_export_dir: Option<PathBuf>,
    export_job: Option<ExportJob>,
    filter: String,
    error: Option<String>,
    pub(super) worker: Option<mpsc::Receiver<Result<WorkspaceScan, String>>>,
}

impl App {
    pub(super) fn refresh_beamng_workspace(&mut self, request: FolderRequest) {
        if self.beamng_workspace.worker.is_some() {
            return;
        }
        let request = match request {
            FolderRequest::Session(session) => {
                self.beamng_workspace.session = Some(session);
                FolderRequest::Discover
            }
            FolderRequest::Select(path) => {
                self.beamng_workspace.session = None;
                FolderRequest::Select(path)
            }
            FolderRequest::Automatic => {
                self.beamng_workspace.session = None;
                FolderRequest::Automatic
            }
            FolderRequest::Discover => FolderRequest::Discover,
        };
        let session = self.beamng_workspace.session.clone();
        self.beamng_workspace.folder = None;
        self.beamng_workspace.export_dir = None;
        self.beamng_workspace.vehicles.clear();
        self.beamng_workspace.error = None;
        let (tx, rx) = mpsc::channel();
        self.beamng_workspace.worker = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                let folder = match request {
                    FolderRequest::Discover => {
                        if let Some(session) = &session {
                            Some(beamng_paths::DetectedFolder {
                                mods_dir: beamng_paths::normalize_selected_folder(
                                    &session.mods_dir,
                                )?,
                                description: "Opened from BABM (this session)".into(),
                            })
                        } else {
                            beamng_paths::detect()?
                        }
                    }
                    FolderRequest::Automatic => {
                        beamng_paths::save_override(None)?;
                        beamng_paths::detect()?
                    }
                    FolderRequest::Select(path) => {
                        let mods_dir = beamng_paths::normalize_selected_folder(&path)?;
                        beamng_paths::save_override(Some(&mods_dir))?;
                        Some(beamng_paths::DetectedFolder {
                            mods_dir,
                            description: "Saved folder choice".into(),
                        })
                    }
                    FolderRequest::Session(_) => {
                        unreachable!("session request resolved before scanning")
                    }
                };
                let mut scan_error = None;
                let vehicles = if let Some(folder) = &folder {
                    bess::beamng_library::scan_automation_archives(&folder.mods_dir).unwrap_or_else(
                        |error| {
                            scan_error = Some(error);
                            Vec::new()
                        },
                    )
                } else {
                    Vec::new()
                };
                let export_result = if let Some(path) = session
                    .as_ref()
                    .and_then(|session| session.exports_dir.as_ref())
                {
                    beamng_paths::validate_export_destination(
                        path,
                        folder.as_ref().map(|folder| folder.mods_dir.as_path()),
                    )
                    .map(Some)
                } else if let Some(folder) = &folder {
                    beamng_paths::export_directory(&folder.mods_dir).map(Some)
                } else {
                    beamng_paths::saved_export_directory()
                };
                let export_dir = match export_result {
                    Ok(path) => path,
                    Err(error) => {
                        scan_error = Some(match scan_error {
                            Some(previous) => format!("{previous}\n{error}"),
                            None => error,
                        });
                        None
                    }
                };
                Ok(WorkspaceScan {
                    folder,
                    vehicles,
                    export_dir,
                    scan_error,
                })
            })();
            let _ = tx.send(result);
        });
    }

    pub(super) fn poll_beamng_workspace(&mut self) {
        let Some(receiver) = &self.beamng_workspace.worker else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("BeamNG folder search stopped unexpectedly. Try Detect again.".into())
            }
        };
        self.beamng_workspace.worker = None;
        match result {
            Ok(scan) => {
                self.beamng_workspace.folder = scan.folder;
                self.beamng_workspace.vehicles = scan.vehicles;
                self.beamng_workspace.export_dir = scan.export_dir;
                self.beamng_workspace.error = scan.scan_error;
            }
            Err(error) => {
                self.beamng_workspace.folder = None;
                self.beamng_workspace.export_dir = None;
                self.beamng_workspace.vehicles.clear();
                self.beamng_workspace.error = Some(error);
            }
        }
    }

    pub(super) fn beamng_file_dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        if let Some(folder) = &self.beamng_workspace.folder {
            dialog.set_directory(&folder.mods_dir)
        } else {
            dialog
        }
    }

    pub(super) fn beamng_source_controls(&mut self, ui: &mut egui::Ui) {
        if let Some(folder) = &self.beamng_workspace.folder {
            ui.small(format!("BeamNG: {}", folder_label(&folder.mods_dir)))
                .on_hover_text(&folder.description);
        } else if self.beamng_workspace.worker.is_none() {
            ui.small("BeamNG folder not found. Choose its user folder or mods folder.");
        }
        let idle = self.beamng_workspace.worker.is_none();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(idle, egui::Button::new("Detect BeamNG"))
                .clicked()
            {
                self.refresh_beamng_workspace(FolderRequest::Automatic);
            }
            if ui
                .add_enabled(idle, egui::Button::new("Choose folder…"))
                .clicked()
                && let Some(path) = self
                    .beamng_file_dialog()
                    .set_title("Choose the BeamNG user folder or mods folder")
                    .pick_folder()
            {
                self.refresh_beamng_workspace(FolderRequest::Select(path));
            }
            if ui
                .add_enabled(idle, egui::Button::new("Refresh vehicles"))
                .clicked()
            {
                self.refresh_beamng_workspace(FolderRequest::Discover);
            }
        });
        if self.beamng_workspace.worker.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.small("Finding BeamNG and Automation vehicles…");
            });
        }
        if let Some(error) = &self.beamng_workspace.error {
            ui.colored_label(Color32::YELLOW, error);
        }
        if !self.beamng_workspace.vehicles.is_empty() {
            ui.add(
                egui::TextEdit::singleline(&mut self.beamng_workspace.filter)
                    .hint_text("Find an Automation vehicle"),
            );
            let filter = self.beamng_workspace.filter.to_lowercase();
            let mut import = None;
            egui::ScrollArea::vertical()
                .id_salt("beamng-vehicles")
                .max_height(100.)
                .show(ui, |ui| {
                    for vehicle in &self.beamng_workspace.vehicles {
                        if !vehicle.name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    self.importer.is_none() && vehicle.unavailable_reason.is_none(),
                                    egui::Button::new("Import"),
                                )
                                .clicked()
                            {
                                import = Some(vehicle.path.clone());
                            }
                            ui.label(&vehicle.name)
                                .on_hover_text(vehicle.path.display().to_string());
                        });
                        if let Some(reason) = &vehicle.unavailable_reason {
                            ui.small(reason);
                        }
                    }
                });
            if let Some(path) = import {
                self.import(path, None);
            }
        } else if self.beamng_workspace.folder.is_some() && idle {
            ui.small("No Automation vehicle ZIP found here. You can import a ZIP manually below.");
        }
    }

    pub(super) fn can_export_beamng(&self) -> bool {
        self.bank.is_some()
            && self.engine_draft_valid()
            && project::validate_profile_name(&self.profile_name).is_ok()
            && self.worker.is_none()
            && self.level_worker.is_none()
            && self.importer.is_none()
            && self.beamng_workspace.worker.is_none()
    }

    pub(super) fn beamng_export_destination_controls(&mut self, ui: &mut egui::Ui) {
        if let Some(destination) = self.beamng_workspace.export_dir.clone() {
            ui.small(format!("Export folder: {}", folder_label(&destination)));
            if ui
                .add_enabled(
                    self.can_export_beamng() && self.beamng_workspace.worker.is_none(),
                    egui::Button::new("Export vehicle + BESS variant ZIP"),
                )
                .clicked()
            {
                self.start_beamng_export(destination);
            }
        } else {
            ui.small("Detect BeamNG under Source vehicle, or choose an export folder here.");
        }
        if ui
            .add_enabled(
                self.worker.is_none() && self.beamng_workspace.worker.is_none(),
                egui::Button::new("Choose export folder…"),
            )
            .clicked()
            && let Some(path) = self
                .beamng_file_dialog()
                .set_title("Choose where BESS writes the original vehicle and BESS variant ZIP")
                .pick_folder()
        {
            let mods = self
                .beamng_workspace
                .folder
                .as_ref()
                .map(|folder| folder.mods_dir.as_path());
            let result = beamng_paths::validate_export_directory(&path, mods).and_then(|path| {
                beamng_paths::save_export_directory(&path)?;
                Ok(path)
            });
            match result {
                Ok(path) => {
                    if let Some(session) = &mut self.beamng_workspace.session {
                        session.exports_dir = Some(path.clone());
                    }
                    self.beamng_workspace.export_dir = Some(path);
                    self.status = "Export folder saved.".into();
                }
                Err(error) => self.status = format!("Export folder: {error}"),
            }
        }
        ui.small("Creates one complete vehicle ZIP with the original configuration and a separate BESS configuration. BABM can merge it with your other variants.");
        if let Some(directory) = self.babm_export_directory() {
            ui.small(format!("BABM export folder: {}", folder_label(directory)));
        }
        if ui
            .add_enabled(
                self.worker.is_none() && self.beamng_workspace.worker.is_none(),
                egui::Button::new("Open BABM…"),
            )
            .clicked()
        {
            self.open_babm();
        }
    }

    fn open_babm(&mut self) {
        let companion = std::env::current_exe()
            .ok()
            .and_then(|executable| {
                executable
                    .parent()?
                    .ancestors()
                    .take(4)
                    .map(|folder| folder.join("BABM.exe"))
                    .find(|path| path.is_file())
            })
            .or_else(|| {
                rfd::FileDialog::new()
                    .add_filter("BABM application", &["exe"])
                    .set_title("Choose BABM.exe")
                    .pick_file()
            });
        let Some(companion) = companion else {
            return;
        };
        let mut command = self.babm_command(&companion);
        self.status = match command.spawn() {
            Ok(_) => {
                "BABM opened. Under BESS sounds, choose Import BESS variant for the matching vehicle or grouped pack."
                    .into()
            }
            Err(error) => format!("Could not open BABM: {error}"),
        };
    }

    fn babm_command(&self, companion: &Path) -> std::process::Command {
        let mut command = std::process::Command::new(companion);
        command.arg("gui");
        if let Some(folder) = &self.beamng_workspace.folder {
            command.arg("--path").arg(&folder.mods_dir);
        }
        if let Some(folder) = self.babm_export_directory() {
            command.arg("--exports").arg(folder);
        }
        command
    }

    pub(super) fn start_beamng_export(&mut self, parent: PathBuf) {
        if !self.can_export_beamng() {
            return;
        }
        let mods = self
            .beamng_workspace
            .folder
            .as_ref()
            .map(|folder| folder.mods_dir.as_path());
        let parent = match beamng_paths::validate_export_destination(&parent, mods) {
            Ok(path) => path,
            Err(error) => {
                self.status = format!("Export folder: {error}");
                return;
            }
        };
        self.persist_engine_edit();
        let bank = self.bank.clone().unwrap();
        let p = self.params;
        let h = self.settings.for_beamng_export();
        let profile = self.profile_name.trim().to_owned();
        let (tx, rx) = mpsc::channel();
        self.beamng_workspace.pending_export_dir = Some(parent.clone());
        let job = ExportJob::default();
        self.beamng_workspace.export_job = Some(job.clone());
        self.worker = Some(rx);
        self.status = format!("Creating the complete vehicle ZIP with BESS variant ‘{profile}’…");
        std::thread::spawn(move || {
            let result = (|| {
                std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
                let folder = parent.join(format!(
                    "BESS-BeamNG-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_nanos()
                ));
                bess::variant::package_complete_with_job(&folder, p, h, bank, &profile, &job)
            })();
            let _ = tx.send(result);
        });
    }

    fn babm_export_directory(&self) -> Option<&Path> {
        self.beamng_workspace
            .last_export_dir
            .as_deref()
            .or(self.beamng_workspace.export_dir.as_deref())
    }

    pub(super) fn finish_beamng_export(&mut self, result: &Result<String, String>) -> bool {
        if let Some(parent) = self.beamng_workspace.pending_export_dir.take()
            && result.is_ok()
        {
            self.beamng_workspace.last_export_dir = Some(parent);
        }
        let Some(job) = self.beamng_workspace.export_job.take() else {
            return false;
        };
        let progress = job.snapshot();
        let elapsed = elapsed_label(progress.elapsed_seconds);
        self.status = match result {
            Ok(message) => format!("Export completed and verified in {elapsed}. {message}"),
            Err(_) if progress.stage == ExportStage::Cancelled => {
                format!("Export cancelled after {elapsed}. The original vehicle is unchanged.")
            }
            Err(error) => format!("Export failed after {elapsed}: {error}"),
        };
        true
    }

    pub(super) fn poll_output_worker(&mut self) {
        let Some(receiver) = self.worker.as_ref() else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Output worker stopped unexpectedly".into())
            }
        };
        self.worker = None;
        if !self.finish_beamng_export(&result) {
            self.status = result.unwrap_or_else(|error| format!("Error: {error}"));
        }
    }

    pub(super) fn beamng_export_progress(&mut self, ui: &mut egui::Ui) {
        if let Some(job) = &self.beamng_workspace.export_job {
            let progress = job.snapshot();
            if export_progress_controls(ui, &progress, job.is_cancelled()) {
                job.cancel();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(stage: ExportStage) -> ExportProgress {
        ExportProgress {
            stage,
            completed: 12,
            total: 12,
            current_rpm: Some(4200.),
            current_load: Some(0.7),
            workers: 4,
            elapsed_seconds: 65.,
            estimated_remaining_seconds: Some(20.),
            fraction: Some(1.),
            detail: "Current vehicle export".into(),
        }
    }

    #[test]
    fn export_progress_exposes_cancellation_and_never_finishes_before_verification() {
        for (stage, label, cancelling, disabled, complete) in [
            (
                ExportStage::Preparing,
                "Preparing vehicle export",
                false,
                false,
                false,
            ),
            (
                ExportStage::Rendering,
                "Generating engine sounds",
                false,
                false,
                false,
            ),
            (
                ExportStage::Packaging,
                "Writing vehicle ZIP",
                false,
                false,
                false,
            ),
            (
                ExportStage::Verifying,
                "Verifying vehicle ZIP",
                false,
                false,
                false,
            ),
            (
                ExportStage::Rendering,
                "Cancelling export…",
                true,
                true,
                false,
            ),
            (
                ExportStage::Complete,
                "Vehicle ZIP verified",
                false,
                true,
                true,
            ),
        ] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let draw = || {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(480., 800.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::TopBottomPanel::top("export-progress-header").show(ctx, |ui| {
                            export_progress_controls(ui, &progress(stage), cancelling);
                        });
                    },
                )
            };
            // TopBottomPanel initially clips to one row, then keeps its measured
            // height. Inspect the settled frame, as the live repaint does.
            let _ = draw();
            let output = draw();
            let accessibility = output.platform_output.accesskit_update.unwrap();
            let cancel = accessibility
                .nodes
                .iter()
                .map(|(_, node)| node)
                .find(|node| {
                    node.role() == egui::accesskit::Role::Button
                        && node.label() == Some("Cancel export")
                })
                .expect("Cancel must stay visible throughout the export");
            assert_eq!(cancel.is_disabled(), disabled);
            assert!(
                accessibility
                    .nodes
                    .iter()
                    .any(|(_, node)| node.value() == Some(label)),
                "missing stage {label}"
            );
            fn texts(shape: &egui::Shape, output: &mut String) {
                match shape {
                    egui::Shape::Text(text) => {
                        output.push_str(&text.galley.job.text);
                        output.push('\n');
                    }
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            texts(shape, output);
                        }
                    }
                    _ => {}
                }
            }
            let mut visible = String::new();
            for shape in &output.shapes {
                texts(&shape.shape, &mut visible);
            }
            assert_eq!(visible.contains("100%"), complete, "{stage:?}: {visible}");
            let expected_count = match stage {
                ExportStage::Preparing => Some("12 / 12 preparation steps"),
                ExportStage::Rendering => Some("12 / 12 sounds generated"),
                ExportStage::Packaging => Some("12 / 12 files written"),
                ExportStage::Verifying => Some("12 / 12 checks completed"),
                ExportStage::Complete | ExportStage::Cancelled | ExportStage::Failed => None,
            };
            if let Some(count) = expected_count {
                assert!(visible.contains(count), "{stage:?}: {visible}");
            } else {
                assert!(!visible.contains("12 / 12"));
            }
            assert_eq!(
                visible.contains("sounds generated"),
                stage == ExportStage::Rendering
            );
            assert!(
                visible.contains("4200 rpm") && visible.contains("70% load"),
                "{stage:?}: {visible}"
            );
            assert!(visible.contains("Elapsed: 1m 05s"));
            if stage == ExportStage::Rendering && !cancelling {
                assert!(visible.contains("Sound generation left (estimate): about 20s"));
            }
        }
    }

    #[test]
    fn output_completion_clears_progress_and_only_success_changes_companion_folder() {
        let mut app = App::with_ctx(&egui::Context::default(), None);
        let previous = PathBuf::from("previous successful exports");
        app.beamng_workspace.last_export_dir = Some(previous.clone());
        for success in [false, true] {
            app.beamng_workspace.export_job = Some(ExportJob::default());
            app.beamng_workspace.pending_export_dir = Some(PathBuf::from("new exports"));
            let (tx, rx) = mpsc::channel();
            app.worker = Some(rx);
            app.poll_output_worker();
            assert!(app.worker.is_some() && app.beamng_workspace.export_job.is_some());
            tx.send(if success {
                Ok("ZIP: vehicle.zip".into())
            } else {
                Err("Disk full".into())
            })
            .unwrap();
            app.poll_output_worker();
            assert!(app.worker.is_none() && app.beamng_workspace.export_job.is_none());
            assert!(app.beamng_workspace.pending_export_dir.is_none());
            if success {
                assert_eq!(app.babm_export_directory(), Some(Path::new("new exports")));
                assert!(
                    app.status.contains("Export completed and verified in")
                        && app.status.contains("vehicle.zip")
                );
            } else {
                assert_eq!(app.babm_export_directory(), Some(previous.as_path()));
                assert!(
                    app.status.contains("Export failed after") && app.status.contains("Disk full")
                );
            }
        }
        // Other output jobs keep their ordinary status and never acquire export controls.
        let (tx, rx) = mpsc::channel();
        app.worker = Some(rx);
        tx.send(Ok("WAV: comparison.wav".into())).unwrap();
        app.poll_output_worker();
        assert_eq!(app.status, "WAV: comparison.wav");
        assert!(app.beamng_workspace.export_job.is_none());
    }

    #[test]
    fn cancelled_export_releases_progress_and_does_not_change_the_source_or_companion_folder() {
        let source = crate::test_support::automation_fixture();
        let original = std::fs::read(&source).unwrap();
        let destination = source.with_extension("cancelled-export");
        let bank = Arc::new(Bank::load(&source, None).unwrap());
        let job = ExportJob::default();
        job.cancel();
        let result = bess::variant::package_complete_with_job(
            &destination,
            Parameters::default(),
            Settings::default(),
            bank,
            "Cancelled test variant",
            &job,
        );
        assert!(result.is_err());
        assert_eq!(job.snapshot().stage, ExportStage::Cancelled);
        let mut app = App::with_ctx(&egui::Context::default(), None);
        app.beamng_workspace.export_job = Some(job);
        app.beamng_workspace.pending_export_dir = Some(destination.clone());
        app.beamng_workspace.last_export_dir = Some(PathBuf::from("previous export"));
        let (tx, rx) = mpsc::channel();
        app.worker = Some(rx);
        tx.send(result).unwrap();
        app.poll_output_worker();
        assert!(app.worker.is_none() && app.beamng_workspace.export_job.is_none());
        assert!(app.beamng_workspace.pending_export_dir.is_none());
        assert_eq!(
            app.babm_export_directory(),
            Some(Path::new("previous export"))
        );
        assert!(app.status.starts_with("Export cancelled after"));
        assert!(!app.status.contains("Error:"));
        assert!(!destination.exists());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        std::fs::remove_file(source).unwrap();
    }

    #[test]
    fn stopped_export_worker_releases_the_busy_state_and_preserves_last_export() {
        let mut app = App::with_ctx(&egui::Context::default(), None);
        app.beamng_workspace.last_export_dir = Some(PathBuf::from("verified exports"));
        app.beamng_workspace.pending_export_dir = Some(PathBuf::from("incomplete exports"));
        app.beamng_workspace.export_job = Some(ExportJob::default());
        let (tx, rx) = mpsc::channel();
        app.worker = Some(rx);
        drop(tx);
        app.poll_output_worker();
        assert!(app.worker.is_none() && app.beamng_workspace.export_job.is_none());
        assert!(app.beamng_workspace.pending_export_dir.is_none());
        assert_eq!(
            app.babm_export_directory(),
            Some(Path::new("verified exports"))
        );
        assert!(app.status.contains("worker stopped unexpectedly"));
    }

    #[test]
    fn companion_uses_last_successful_export_without_changing_default() {
        let mut app = App::with_ctx(&egui::Context::default(), None);
        app.beamng_workspace.folder = Some(beamng_paths::DetectedFolder {
            mods_dir: PathBuf::from("custom mods"),
            description: "Test".into(),
        });
        let default = PathBuf::from("saved exports");
        let elsewhere = PathBuf::from("one-off exports");
        app.beamng_workspace.export_dir = Some(default.clone());
        app.beamng_workspace.pending_export_dir = Some(elsewhere.clone());
        assert_eq!(app.babm_export_directory(), Some(default.as_path()));
        app.finish_beamng_export(&Ok("Complete vehicle ZIP written".into()));
        assert_eq!(app.babm_export_directory(), Some(elsewhere.as_path()));
        assert_eq!(app.beamng_workspace.export_dir, Some(default));
        let command = app.babm_command(Path::new("BABM.exe"));
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "gui",
                "--path",
                "custom mods",
                "--exports",
                "one-off exports"
            ]
        );
        app.beamng_workspace.pending_export_dir = Some(PathBuf::from("failed exports"));
        app.finish_beamng_export(&Err("Disk full".into()));
        assert_eq!(app.babm_export_directory(), Some(elsewhere.as_path()));
        assert!(app.beamng_workspace.pending_export_dir.is_none());
        app.finish_beamng_export(&Ok("Unrelated WAV export".into()));
        assert_eq!(app.babm_export_directory(), Some(elsewhere.as_path()));
    }

    #[test]
    fn companion_arguments_are_explicit_and_validate_missing_values() {
        let parse = |args: &[&str]| {
            companion_workspace(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
        };
        assert_eq!(parse(&[]).unwrap(), None);
        assert_eq!(
            parse(&[
                "--beamng-mods",
                "custom mods",
                "--bess-exports",
                "custom exports"
            ])
            .unwrap(),
            Some(CompanionWorkspace {
                mods_dir: PathBuf::from("custom mods"),
                exports_dir: Some(PathBuf::from("custom exports")),
            })
        );
        assert!(parse(&["--beamng-mods"]).is_err());
        assert!(parse(&["--bess-exports", "custom exports"]).is_err());
        assert!(parse(&["--beamng-mods", "mods", "--beamng-mods", "other"]).is_err());
        assert!(parse(&["--beamng-mods", "--bess-exports", "exports"]).is_err());
        assert!(parse(&["--unknown", "folder"]).is_err());
        let args = [
            "muted project.bess.json",
            "--beamng-mods",
            "custom mods",
            "--bess-exports",
            "custom exports",
        ]
        .map(str::to_string);
        let (initial, companion) = initial_and_companion(&args).unwrap();
        assert_eq!(initial, Some(PathBuf::from("muted project.bess.json")));
        assert_eq!(
            companion,
            parse(&[
                "--beamng-mods",
                "custom mods",
                "--bess-exports",
                "custom exports"
            ])
            .unwrap()
        );
    }

    #[test]
    fn companion_workspace_and_refresh_keep_custom_folders_without_saved_preferences() {
        let temp = std::env::temp_dir().join(format!(
            "bess-companion-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mods = temp.join("custom BeamNG/mods");
        let exports = temp.join("custom exports");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&exports).unwrap();
        let session = CompanionWorkspace {
            mods_dir: mods.clone(),
            exports_dir: Some(exports.clone()),
        };
        let mut app = App::with_ctx(&egui::Context::default(), None);
        for request in [
            FolderRequest::Session(session.clone()),
            FolderRequest::Discover,
        ] {
            app.refresh_beamng_workspace(request);
            let result = app
                .beamng_workspace
                .worker
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            let (tx, rx) = mpsc::channel();
            tx.send(result).unwrap();
            app.beamng_workspace.worker = Some(rx);
            app.poll_beamng_workspace();
            assert!(
                app.beamng_workspace.error.is_none(),
                "{:?}",
                app.beamng_workspace.error
            );
            assert_eq!(
                app.beamng_workspace.folder.as_ref().unwrap().mods_dir,
                std::fs::canonicalize(&mods).unwrap()
            );
            assert_eq!(
                app.beamng_workspace.export_dir,
                Some(std::fs::canonicalize(&exports).unwrap())
            );
            assert_eq!(app.beamng_workspace.session, Some(session.clone()));
        }
        app.refresh_beamng_workspace(FolderRequest::Session(CompanionWorkspace {
            mods_dir: temp.join("missing mods"),
            exports_dir: Some(exports),
        }));
        let result = app
            .beamng_workspace
            .worker
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(result.is_err());
        assert!(app.beamng_workspace.folder.is_none());
        assert!(app.beamng_workspace.export_dir.is_none());
        std::fs::remove_dir_all(&temp).unwrap();
    }

    #[test]
    fn discovered_sources_show_importable_originals_and_explain_merged_archives() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = App::with_ctx(&ctx, None);
        let (tx, rx) = mpsc::channel();
        app.beamng_workspace.worker = Some(rx);
        tx.send(Ok(WorkspaceScan {
            folder: Some(beamng_paths::DetectedFolder {
                mods_dir: PathBuf::from("detected/mods"),
                description: "Launcher configuration".into(),
            }),
            export_dir: Some(PathBuf::from("detected/BESS-exports")),
            vehicles: vec![
                VehicleArchive {
                    path: PathBuf::from("detected/mods/source.zip.merged_backup"),
                    name: "Original (original before grouping)".into(),
                    unavailable_reason: None,
                },
                VehicleArchive {
                    path: PathBuf::from("detected/mods/merged.zip"),
                    name: "Grouped vehicles".into(),
                    unavailable_reason: Some("Choose an original vehicle archive.".into()),
                },
            ],
            scan_error: None,
        }))
        .unwrap();
        app.poll_beamng_workspace();
        assert!(app.beamng_workspace.worker.is_none());
        assert_eq!(
            app.beamng_workspace.export_dir,
            Some(PathBuf::from("detected/BESS-exports"))
        );
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.beamng_source_controls(ui));
        });
        let update = output.platform_output.accesskit_update.unwrap();
        let imports: Vec<_> = update
            .nodes
            .iter()
            .map(|(_, node)| node)
            .filter(|node| {
                node.role() == egui::accesskit::Role::Button && node.label() == Some("Import")
            })
            .collect();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports.iter().filter(|node| node.is_disabled()).count(), 1);
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| node.value() == Some("Choose an original vehicle archive."))
        );
    }

    #[test]
    fn failed_folder_search_clears_any_previous_destination() {
        let ctx = egui::Context::default();
        let mut app = App::with_ctx(&ctx, None);
        app.beamng_workspace.folder = Some(beamng_paths::DetectedFolder {
            mods_dir: PathBuf::from("old/mods"),
            description: "Previous result".into(),
        });
        app.beamng_workspace.export_dir = Some(PathBuf::from("old/BESS-exports"));
        let (tx, rx) = mpsc::channel();
        app.beamng_workspace.worker = Some(rx);
        tx.send(Err("Saved folder is missing".into())).unwrap();
        app.poll_beamng_workspace();
        assert!(app.beamng_workspace.folder.is_none());
        assert!(app.beamng_workspace.export_dir.is_none());
        assert_eq!(
            app.beamng_workspace.error.as_deref(),
            Some("Saved folder is missing")
        );
    }
}
