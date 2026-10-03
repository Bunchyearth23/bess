//! Local BeamNG folder, vehicle library and explicit export actions.
use super::*;
use bess::{beamng_library::VehicleArchive, beamng_paths};

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
                    egui::Button::new("Export vehicle ZIP"),
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
                .set_title("Choose where BESS writes complete vehicle ZIPs")
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
        ui.small("Creates a complete vehicle ZIP. BABM finds this export under BESS sounds and can apply it to the matching vehicle or grouped pack.");
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
                "BABM opened. Review the matching vehicle under BESS sounds, then apply the export."
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
        let (tx, rx) = mpsc::channel();
        self.beamng_workspace.pending_export_dir = Some(parent.clone());
        self.worker = Some(rx);
        self.status = "Creating and verifying the complete vehicle ZIP…".into();
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
                bess::export::package(&folder, p, h, bank)
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

    pub(super) fn finish_beamng_export(&mut self, result: &Result<String, String>) {
        if let Some(parent) = self.beamng_workspace.pending_export_dir.take()
            && result.is_ok()
        {
            self.beamng_workspace.last_export_dir = Some(parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
