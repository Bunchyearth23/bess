#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod audio;
use bess::{
    bank::Bank,
    beamng, beamng_level,
    bench::AuditionMix,
    drive::{Controls, Mode},
    hybrid::Settings,
    project::{self, Parameters, Project},
    render,
};
use eframe::egui::{self, Color32, RichText};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering, mpsc},
    time::Duration,
};

struct Loaded {
    bank: Arc<Bank>,
    vehicle: Option<beamng::Vehicle>,
    params: Parameters,
    settings: Settings,
    driving: Controls,
}
#[derive(Clone, PartialEq)]
struct LevelKey {
    source_fingerprint: String,
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
        settings.enhanced = true;
        settings.level_match = false;
        settings.overrun = 0.;
        settings.turbo = 0.;
        settings.attack = 0.;
        settings.roughness = 0.;
        settings.fuel_cut = 0.;
        Self {
            source_fingerprint: bank.source.fingerprint.clone(),
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
    audio: Option<audio::Audio>,
    playing: bool,
    audition_mix: AuditionMix,
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
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(12., 10.);
        style.visuals.panel_fill = Color32::from_rgb(17, 23, 30);
        style.visuals.selection.bg_fill = Color32::from_rgb(0, 115, 110);
        cc.egui_ctx.set_style(style);
        let mut app = Self {
            capture: None,
            capture_level: false,
            capture_requested: false,
            frames: 0,
            params: Parameters::default(),
            settings: Settings::default(),
            bank: None,
            vehicle: None,
            audio: None,
            playing: false,
            audition_mix: AuditionMix::Live,
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
        };
        if let Some(path) = initial {
            app.import(path, None);
        }
        app
    }
    fn reconnect(&mut self) {
        self.audio = None;
        self.playing = false;
        self.sent = None;
        match audio::Audio::with_bank(self.params, self.settings, self.bank.clone(), self.driving) {
            Ok(a) => self.audio = Some(a),
            Err(e) => self.status = format!("Audio unavailable: {e}. Export is still available."),
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
                bank.prepare_procedural();
                let vehicle = beamng::inspect(&path).ok();
                let driving = project.as_ref().map(|p| p.driving).unwrap_or(Controls {
                    mode: Mode::Simulated,
                    ..Default::default()
                });
                let (mut params, settings) = if let Some(p) = project {
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
                params.rpm = params.rpm.clamp(bank.min_rpm, bank.max_rpm);
                Ok(Loaded {
                    bank: Arc::new(bank),
                    vehicle,
                    params,
                    settings,
                    driving,
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
        let settings = self.settings;
        std::thread::spawn(move || {
            let _ = tx.send(beamng_level::analyze(bank, params, settings));
        });
    }
    fn open_project(&mut self, path: &Path) {
        match project::load_project(path) {
            Ok(p) => {
                if let Some(source) = &p.source {
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
                    self.bank = None;
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
    fn listen_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Sound comparison bench");
        ui.horizontal(|ui| {
                ui.label("Character:");
                for (i,name) in ["Balanced", "Muted", "Open"].iter().enumerate() {
                    if ui.button(*name).on_hover_text("Apply a starting point for the sound settings while keeping the vehicle and driving setup.").clicked() {
                        self.settings=self.settings.character_preserving_engine(i,self.bank.as_deref());
                    }
                }
            });
        ui.small(
            "Three interpretations of the same imported engine, adjustable in the sound settings.",
        );
        let rpm = self
            .audio
            .as_ref()
            .filter(|_| self.playing)
            .map(|a| f32::from_bits(a.meter.rpm.load(Ordering::Relaxed)))
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
                ui.label("Sound bank limit reached");
            }
            if flags & 4 != 0 {
                ui.colored_label(
                    Color32::YELLOW,
                    "Downshift delayed: RPM is outside the sound bank range.",
                );
            }
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.settings.enhanced, false, "A · Source Automation");
            ui.selectable_value(&mut self.settings.enhanced, true, "B · BESS resynthesis");
        });
        if self.bank.is_some()
            && ui
                .checkbox(
                    &mut self.settings.procedural,
                    "Experimental: generate B from measured engine characteristics",
                )
                .changed()
        {
            self.reconnect();
        }
        ui.add_enabled(
            self.audition_mix == AuditionMix::Live,
            egui::Checkbox::new(
                &mut self.settings.level_match,
                "Match levels to compare tone",
            ),
        );
        ui.small(
            "A plays the imported WAV files with prepared transitions. It is not a game recording.",
        );
        if self.settings.procedural {
            ui.small("Experimental B uses the ZIP to measure level and broad tonal character, then generates new pulses and flow. The engine-side sound remains an estimate from exhaust-only audio.");
        } else {
            ui.small("B uses the Automation recording as a seed. Its additional engine-side layer is estimated from exhaust-only audio.");
        }
        if self.bank.is_some() {
            ui.horizontal(|ui| {
                ui.label("Listening mix:");
                ui.selectable_value(&mut self.audition_mix, AuditionMix::Live, "BESS live mix");
                ui.selectable_value(
                    &mut self.audition_mix,
                    AuditionMix::BeamNgTwoEmitter,
                    "BeamNG two-emitter preview",
                );
            });
            if self.audition_mix == AuditionMix::BeamNgTwoEmitter {
                ui.small("Approximate mono balance of the separate exhaust and engine emitters. The preview follows export level calibration with smoothed live estimates and assumes -8 dB engine gain relative to exhaust. The live A/B level match is ignored. BeamNG camera, cabin and spatial filtering can change what you hear. A remains the source reference.");
            }
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
                .add_enabled(self.bank.is_some(), egui::Button::new("Reconnect audio"))
                .clicked()
            {
                self.reconnect();
            }
        });
        slider(
            ui,
            "Listening / comparison WAV volume",
            &mut self.params.volume,
            0.0..=0.8,
        );
    }
    fn driving_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Driving — test bench");
        ui.small("Control listening tests and WAV renders.");
        if ui
            .add_enabled(
                self.audio.is_some() && self.level_worker.is_none(),
                egui::Button::new(if self.playing {
                    "Pause listening"
                } else {
                    "Listen"
                }),
            )
            .clicked()
        {
            self.playing = !self.playing;
        }
        if let Some(audio) = &self.audio {
            let rpm = f32::from_bits(audio.meter.rpm.load(Ordering::Relaxed));
            let load = f32::from_bits(audio.meter.load.load(Ordering::Relaxed));
            ui.label(format!("{rpm:.0} rpm · engine load {:.0} %", load * 100.));
            if self.driving.mode == Mode::Simulated {
                let speed = f32::from_bits(audio.meter.speed.load(Ordering::Relaxed));
                let gear = audio.meter.gear.load(Ordering::Relaxed);
                ui.label(format!(
                    "{speed:.1} km/h · gear {}",
                    if gear == 0 {
                        "N".to_owned()
                    } else {
                        gear.to_string()
                    }
                ));
            }
        }
        ui.separator();
        self.drive_controls(ui);
        if self.driving.mode == Mode::Direct {
            if let Some(bank) = &self.bank {
                self.params.rpm = self.params.rpm.clamp(bank.min_rpm, bank.max_rpm);
                slider(
                    ui,
                    "Requested RPM",
                    &mut self.params.rpm,
                    bank.min_rpm..=bank.max_rpm,
                );
                ui.small(format!(
                    "Exported range: {:.0}–{:.0} rpm",
                    bank.min_rpm, bank.max_rpm
                ));
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
        egui::ComboBox::from_id_salt("drive-mode")
            .selected_text(match self.driving.mode {
                Mode::Direct => "Direct RPM / load",
                Mode::Simulated => "Simulated driving",
                Mode::Cycle => "Comparison cycle",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.driving.mode, Mode::Simulated, "Simulated driving");
                ui.selectable_value(&mut self.driving.mode, Mode::Direct, "Direct RPM / load");
                ui.selectable_value(&mut self.driving.mode, Mode::Cycle, "Comparison cycle");
            });
        if self.driving.mode == Mode::Simulated {
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
            egui::CollapsingHeader::new("Test bench vehicle and gearing").show(ui, |ui| {
                ui.small("Adjustable simulation values; these are not identified in the ZIP.");
                slider(ui, "Mass (kg)", &mut self.driving.mass_kg, 300.0..=6000.0);
                slider(
                    ui,
                    "Peak engine torque (Nm)",
                    &mut self.driving.peak_torque_nm,
                    30.0..=2000.0,
                );
                slider(
                    ui,
                    "Engine inertia (kg·m²)",
                    &mut self.driving.inertia,
                    0.1..=2.0,
                );
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
    if response.hovered() {
        let (delta, fine) = ui.input_mut(|i| {
            let y = if i.modifiers.shift && i.smooth_scroll_delta.y == 0. {
                let x = i.smooth_scroll_delta.x;
                i.smooth_scroll_delta.x = 0.;
                x
            } else {
                i.smooth_scroll_delta.y
            };
            i.smooth_scroll_delta.y = 0.;
            (y, i.modifiers.shift)
        });
        if delta != 0. {
            let step = (max - min) * 0.01 * if fine { 0.1 } else { 1. };
            *value = (*value + delta / 50. * step).clamp(min, max);
            response.mark_changed();
        }
    }
}
fn percent_slider(ui: &mut egui::Ui, label: &str, value: &mut f32) {
    let mut percent = *value * 100.;
    let mut response = ui.add(
        egui::Slider::new(&mut percent, 0.0..=100.0)
            .suffix(" %")
            .text(label),
    );
    wheel_adjust(ui, &mut response, &mut percent, 0., 100.);
    *value = percent / 100.;
}
fn slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    let (min, max) = (*range.start(), *range.end());
    let mut response = ui.add(egui::Slider::new(v, range).text(label));
    wheel_adjust(ui, &mut response, v, min, max);
    response.on_hover_text("Mouse wheel: adjust · Shift + wheel: fine adjustment");
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
    value: fn(&beamng_level::Point) -> f32,
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
    ui.collapsing("WAV level details", |ui| {
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
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(33));
        self.frames += 1;
        if let Some(path) = &self.capture {
            let level_ready = self
                .level_report
                .as_ref()
                .is_some_and(|(key, _)| self.level_key().as_ref() == Some(key));
            if self.bank.is_some()
                && self.frames > 20
                && !self.capture_requested
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
        if let Some(rx) = &self.importer
            && let Ok(result) = rx.try_recv()
        {
            match result {
                Ok(v) => {
                    self.params = v.params;
                    self.level_rpm = v.params.rpm;
                    self.settings = v.settings;
                    self.driving = v.driving;
                    self.bank = Some(v.bank);
                    self.vehicle = v.vehicle;
                    self.status =
                        "Sound bank ready. Compare the Automation source and BESS resynthesis."
                            .into();
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
                    ui.heading("Sound settings");
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
                    ui.separator();
                    ui.heading("02 / Character and dynamics");
                    if self.settings.procedural {
                        if let Some(cylinders) = self.bank.as_ref().and_then(|bank| bank.engine_meta.as_ref().map(|meta| meta.cylinders)) {
                            ui.small(format!("Generated pulse timing uses {cylinders} cylinders verified in the vehicle ZIP. Firing order is not identified."));
                        } else {
                            ui.small("Cylinder metadata is unavailable. The generated pulse timing uses this configured approximation:");
                            ui.add(egui::Slider::new(&mut self.params.cylinders, 1..=12).text("Cylinders"));
                        }
                        ui.small("The ZIP sets broad tone and level. These three controls balance the generated sources; the original waveform does not enter B.");
                        slider(ui, "Generated exhaust", &mut self.params.exhaust, 0.0..=1.0);
                        slider(ui, "Generated intake", &mut self.params.intake, 0.0..=1.0);
                        slider(ui, "Generated mechanics", &mut self.params.mechanical, 0.0..=1.0);
                        ui.small("This is an experimental exhaust-guided model. Its intake and mechanical sound are estimates, not separate recordings.");
                    } else {
                    ui.collapsing("Engine and combustion (optional)", |ui| {
                        ui.small("Automation engine data may provide the layout, but not the firing order. Combustion events are optional: the WAV files already contain pulses.");
                        let mut enabled=self.settings.combustion.cylinders>0;
                        if ui.checkbox(&mut enabled,"Enable combustion events").changed() {
                            let cylinders=self.bank.as_ref().and_then(|b|b.engine_meta.as_ref()).map_or(4,|meta|meta.cylinders);
                            self.settings.combustion=if enabled {bess::combustion::Combustion::even(cylinders)}else{Default::default()};
                        }
                        if enabled {
                            let before=self.settings.combustion.cylinders;
                            let response=ui.add(egui::Slider::new(&mut self.settings.combustion.cylinders,1..=12).text("Cylinders"));
                            if response.hovered() {
                                let delta=ui.input_mut(|input| {
                                    let delta=input.events.iter().filter_map(|e|if let egui::Event::MouseWheel{delta,..}=e {Some(delta.y)}else{None}).sum::<f32>();
                                    input.smooth_scroll_delta=egui::Vec2::ZERO;
                                    delta
                                });
                                if delta!=0. {self.settings.combustion.cylinders=(self.settings.combustion.cylinders as i32+delta.signum() as i32).clamp(1,12) as u32;}
                            }
                            if before!=self.settings.combustion.cylinders {self.settings.combustion=bess::combustion::Combustion::even(self.settings.combustion.cylinders);}
                            slider(ui,"Event strength",&mut self.settings.combustion.amount,0.0..=1.0);
                            slider(ui,"Pressure duration (ms)",&mut self.settings.combustion.width_ms,0.5..=8.0);
                            slider(ui,"Exhaust opening (° after ignition)",&mut self.settings.combustion.exhaust_delay,60.0..=240.0);
                            ui.small("Even spacing is suggested, not a manufacturer firing order. Resynthesis amount also scales these events.");
                            ui.collapsing("Ignition angles over 720°", |ui| {
                                for i in 0..self.settings.combustion.cylinders as usize {slider(ui,&format!("Cylinder {} (°)",i+1),&mut self.settings.combustion.angles[i],0.0..=719.9);}
                            });
                        }
                    });
                    ui.small("Character curve: −1 = softer, 0 = neutral, +1 = stronger.");
                    let before=(self.settings.rpm_character,self.settings.load_character);
                    slider(ui,"At high RPM",&mut self.settings.rpm_character,-1.0..=1.0);
                    slider(ui,"At full load",&mut self.settings.load_character,-1.0..=1.0);
                    if before!=(self.settings.rpm_character,self.settings.load_character) {self.settings.rebuild_character_maps();}
                    if self.settings.maps != { let mut h=self.settings;h.rebuild_character_maps();h.maps } {
                        ui.small("The project's detailed curves are preserved. Changing either setting above replaces them.");
                    }
                    slider(ui,"Resynthesis amount",&mut self.settings.coloration,0.0..=1.0);
                    if ui.add_enabled(self.bank.is_some(),egui::Button::new("Fit vehicle / natural background")).clicked()
                        && let Some(bank)=&self.bank {
                            let enhanced=self.settings.enhanced;let level_match=self.settings.level_match;
                            let procedural=self.settings.procedural;
                            let combustion=self.settings.combustion;
                            self.settings=Settings{enhanced,procedural,level_match,combustion,..Settings::calibrated(bank)};
                    }
                    if self.settings.procedural {
                        ui.small("The source-pulse and source-texture controls below apply to the earlier source-guided mode only.");
                    } else {
                        ui.small("Pulses and texture separated from the imported WAV files.");
                    }
                    slider(
                        ui,
                        "Source pulses",
                        &mut self.settings.pulse_gain,
                        0.0..=2.0,
                    );
                    slider(
                        ui,
                        "Natural source texture",
                        &mut self.settings.residual_gain,
                        0.0..=2.0,
                    );
                    slider(
                        ui,
                        "Cycle variation",
                        &mut self.settings.cycle_life,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Pressure front",
                        &mut self.settings.pressure_shape,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Pulse-linked texture",
                        &mut self.settings.pulse_texture,
                        0.0..=1.0,
                    );
                    ui.small("The pressure front replaces some recorded pulses. Slow cycle variation; texture follows measured pulses.");
                    slider(
                        ui,
                        "Acceleration attack",
                        &mut self.settings.attack,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Pulse body",
                        &mut self.settings.body,
                        0.0..=1.0,
                    );
                    slider(ui, "Load rasp", &mut self.settings.rasp, 0.0..=1.0);
                    slider(
                        ui,
                        "Intake texture",
                        &mut self.settings.texture,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Idle micro-variation",
                        &mut self.settings.roughness,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Lift-off crackle",
                        &mut self.settings.overrun,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Deceleration fuel cut",
                        &mut self.settings.fuel_cut,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Added turbo (optional)",
                        &mut self.settings.turbo,
                        0.0..=1.0,
                    );
                    ui.separator();
                    ui.heading("03 / Exhaust");
                    ui.small("Header / chamber / outlet. Dimensions of the acoustic model.");
                    slider(
                        ui,
                        "Header length (m)",
                        &mut self.settings.header_length,
                        0.15..=1.5,
                    );
                    slider(
                        ui,
                        "Pipe after chamber (m)",
                        &mut self.params.pipe_length,
                        0.2..=5.0,
                    );
                    slider(
                        ui,
                        "Geometry influence",
                        &mut self.settings.pipe,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Diameter (mm)",
                        &mut self.settings.diameter,
                        30.0..=130.0,
                    );
                    slider(
                        ui,
                        "Chamber volume (L)",
                        &mut self.settings.chamber,
                        0.3..=18.0,
                    );
                    slider(
                        ui,
                        "Muffler absorption",
                        &mut self.settings.absorption,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Acoustic temperature (°C)",
                        &mut self.settings.temperature,
                        150.0..=950.0,
                    );
                    slider(ui, "Resonance", &mut self.params.resonance, 0.5..=4.0);
                    slider(
                        ui,
                        "Brightness / cutoff (Hz)",
                        &mut self.params.brightness,
                        200.0..=10000.0,
                    );
                    slider(
                        ui,
                        "Source exhaust",
                        &mut self.params.exhaust,
                        0.0..=1.0,
                    );
                    ui.separator();
                    ui.heading("04 / Intake and mechanical");
                    slider(
                        ui,
                        "Intake runner (m)",
                        &mut self.settings.intake_length,
                        0.12..=1.2,
                    );
                    slider(
                        ui,
                        "Intake resonance",
                        &mut self.settings.airbox,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Inferred intake detail",
                        &mut self.params.intake,
                        0.0..=1.0,
                    );
                    slider(
                        ui,
                        "Reconstructed mechanical sound",
                        &mut self.params.mechanical,
                        0.0..=1.0,
                    );
                    ui.small(
                        "Reconstructed layers are not isolated recordings from the vehicle.",
                    );
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
                let peak=f32::from_bits(audio.meter.peak.load(Ordering::Relaxed));
                ui.add(egui::ProgressBar::new(peak).text(format!("Peak {:.1} dBFS",20.*peak.max(0.00001).log10())));
                let (rect,_)=ui.allocate_exact_size(egui::vec2(ui.available_width(),120.),egui::Sense::hover());
                ui.painter().rect_filled(rect,8.,Color32::from_rgb(9,15,21));
                let points:Vec<_>=audio.meter.samples.iter().enumerate().map(|(i,s)|egui::pos2(rect.left()+i as f32/255.*rect.width(),rect.center().y-f32::from_bits(s.load(Ordering::Relaxed))*rect.height()*0.8)).collect();
                ui.painter().add(egui::Shape::line(points,egui::Stroke::new(1.5_f32,Color32::from_rgb(75,222,195))));
            }
            ui.separator();ui.heading("Export BeamNG");
            ui.small("Adds a BESS configuration to the original Automation vehicle. Keep the original mod enabled.");
            ui.small("Game afterfire, turbo, and startup sounds are preserved; BESS transient effects are not transferred.");
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.strong("BeamNG volume estimate");
                ui.small("Compare the WAV levels BESS will export with the original Automation samples. Listening volume does not change these files.");
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
            if ui.add_enabled(self.bank.is_some()&&self.worker.is_none()&&self.level_worker.is_none(),egui::Button::new("Create BeamNG configuration…")).clicked()
                &&let Some(dir)=rfd::FileDialog::new().pick_folder(){
                let bank=self.bank.clone().unwrap();let p=self.params;let h=self.settings;
                let (tx,rx)=mpsc::channel();self.worker=Some(rx);self.status="Creating and verifying BeamNG configuration…".into();
                std::thread::spawn(move||{let folder=dir.join(format!("BESS-BeamNG-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));
                    let _=tx.send(bess::variant::package(&folder,p,h,bank));});
            }
            ui.separator();ui.heading("Listening exports");
            slider(ui,"WAV duration (seconds)",&mut self.seconds,1.0..=60.0);
            ui.small(match self.driving.mode {
                Mode::Simulated=>"WAV: starts from standstill with current throttle, brake, and load held; transmission follows the selected mode.",
                Mode::Direct=>"WAV: holds the currently requested RPM and load.",
                Mode::Cycle=>"WAV: complete comparison cycle fitted to the selected duration."
            });
            ui.small("Mono 48 kHz / 24-bit. Rendering does not record earlier control changes.");
            if ui.add_enabled(self.bank.is_some()&&self.worker.is_none(),egui::Button::new("Export selected mode…")).clicked()
                &&let Some(path)=rfd::FileDialog::new().add_filter("Audio",&["wav"]).set_file_name("hybrid-engine.wav").save_file(){
                let bank=self.bank.clone().unwrap();let p=self.params;let h=self.settings;let seconds=self.seconds;let driving=self.driving;
                let (tx,rx)=mpsc::channel();self.worker=Some(rx);self.status="Rendering audio…".into();
                std::thread::spawn(move||{let _=tx.send(render::bench_wav(&path,p,h,bank,seconds,driving).map(|()|format!("WAV : {}",path.display())));});
            }
            if ui.add_enabled(self.bank.is_some()&&self.worker.is_none(),egui::Button::new("Export A/B comparison (16 s)…")).clicked()
                &&let Some(dir)=rfd::FileDialog::new().pick_folder(){
                let bank=self.bank.clone().unwrap();let p=self.params;let h=self.settings;
                let (tx,rx)=mpsc::channel();self.worker=Some(rx);self.status="Rendering level-matched comparison…".into();
                std::thread::spawn(move||{let folder=dir.join(format!("BESS-AB-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()));
                    let _=tx.send(render::comparison(&folder,p,h,bank).map(|_|format!("Comparison: {}",folder.display())));});
            }
            if self.worker.is_some()||self.importer.is_some(){ui.spinner();}
            ui.small("A/B and character exports always use the comparison cycle.");
            if ui.add_enabled(self.bank.is_some()&&self.worker.is_none(),egui::Button::new("Export three characters + source…")).clicked()
                &&let Some(dir)=rfd::FileDialog::new().pick_folder(){
                let bank=self.bank.clone().unwrap();let p=self.params;
                let (tx,rx)=mpsc::channel();self.worker=Some(rx);self.status="Rendering level-matched characters…".into();
                std::thread::spawn(move||{let folder=dir.join(format!("BESS-Characters-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()));
                    let _=tx.send(render::characters(&folder,p,bank).map(|_|format!("Listening files and projects: {}",folder.display())));});
            }
            ui.add_space(12.);ui.small("Acoustic dimensions can be edited; they are not inferred from the vehicle parts.");
        });});

        let command = audio::Command {
            params: self.params,
            settings: self.settings,
            playing: self.playing,
            driving: self.driving,
            restart: self.restart,
            audition_mix: self.audition_mix,
        };
        if self.sent != Some(command)
            && let Some(audio) = &self.audio
            && audio.tx.try_send(command).is_ok()
        {
            self.sent = Some(command);
        }
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
    if args.get(1).map(String::as_str) == Some("--steady-procedural") {
        let result = (|| {
            let archive = args.get(2).ok_or("ZIP file required")?;
            let dir = args.get(3).ok_or("Output folder required")?;
            let bank = Arc::new(Bank::load(Path::new(archive), None)?);
            let rpm = args
                .get(4)
                .ok_or("RPM required")?
                .parse::<f32>()
                .map_err(|_| "Invalid RPM")?
                .clamp(bank.min_rpm, bank.max_rpm);
            let load = args
                .get(5)
                .ok_or("Load required")?
                .parse::<f32>()
                .map_err(|_| "Invalid load")?;
            let params = Parameters {
                cylinders: bank.engine_meta.as_ref().map_or(4, |meta| meta.cylinders),
                rpm,
                load,
                brightness: 10_000.,
                exhaust: 1.,
                intake: 0.25,
                mechanical: 0.12,
                ..Parameters::default()
            };
            render::steady_procedural_comparison(Path::new(dir), params, bank, 6.)
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if matches!(
        args.get(1).map(String::as_str),
        Some(
            "--compare"
                | "--compare-procedural"
                | "--characters"
                | "--drive-demo"
                | "--beamng"
                | "--beamng-procedural"
                | "--beamng-replacement"
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
            let mut settings = Settings::calibrated(&bank);
            settings.procedural = matches!(
                args[1].as_str(),
                "--compare-procedural" | "--beamng-procedural"
            );
            if matches!(args[1].as_str(), "--beamng" | "--beamng-procedural") {
                bess::variant::package(Path::new(dir), params, settings, bank)
            } else if args[1] == "--beamng-replacement" {
                bess::export::package(Path::new(dir), params, settings, bank)
            } else if args[1] == "--drive-demo" {
                render::drive_demo(Path::new(dir), params, bank)
            } else if args[1] == "--characters" {
                render::characters(Path::new(dir), params, bank)
            } else {
                render::comparison(Path::new(dir), params, settings, bank)
            }
        })();
        if let Err(e) = result {
            if let Some(dir) = args.get(3) {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(Path::new(dir).join("error.txt"), e);
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
            if !(1..=60).contains(&seconds) {
                return Err("Audio check duration must be 1–60 seconds".into());
            }
            let params = Parameters {
                volume: 0.,
                ..Parameters::default()
            };
            let a = if let Some(path) = args.get(3) {
                let bank = Arc::new(Bank::load(Path::new(path), None)?);
                let check_settings = Settings {
                    procedural: args.get(5).is_some_and(|s| s == "procedural"),
                    combustion: if args.get(5).is_some_and(|s| s == "events") {
                        bess::combustion::Combustion::even(12)
                    } else {
                        Default::default()
                    },
                    ..Settings::default()
                };
                let a = audio::Audio::with_bank(
                    params,
                    check_settings,
                    Some(bank),
                    Controls {
                        mode: Mode::Simulated,
                        throttle: 0.8,
                        ..Default::default()
                    },
                )?;
                a.tx.try_send(audio::Command {
                    params,
                    settings: check_settings,
                    playing: true,
                    driving: Controls {
                        mode: Mode::Simulated,
                        throttle: 0.8,
                        ..Default::default()
                    },
                    restart: 0,
                    audition_mix: AuditionMix::Live,
                })
                .map_err(|e| e.to_string())?;
                a
            } else {
                audio::Audio::start(params)?
            };
            std::thread::sleep(Duration::from_secs(seconds));
            let n = a.meter.blocks.load(Ordering::Relaxed);
            if n == 0 || a.meter.failed.load(Ordering::Relaxed) {
                return Err("No audio stream detected".into());
            }
            Ok(format!(
                "{}\nDuration: {seconds} s\nCallbacks: {n}\nMaximum callback CPU time: {:.3} ms\nBudget overruns: {}\nSilent test: synthesis calculated, output volume set to zero.\n",
                a.description,
                a.meter.max_ns.load(Ordering::Relaxed) as f64 / 1e6,
                a.meter.overruns.load(Ordering::Relaxed)
            ))
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
        "BESS — Hybrid Synthesis",
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
        let ctx = egui::Context::default();
        let mut value = initial;
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = ui.add(egui::Slider::new(&mut value, 0.0..=1.0)).rect;
            });
        });
        let modifiers = egui::Modifiers {
            shift,
            ..Default::default()
        };
        let input = egui::RawInput {
            modifiers,
            events: vec![
                egui::Event::PointerMoved(if hover {
                    rect.center()
                } else {
                    egui::pos2(700., 500.)
                }),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., 50.),
                    modifiers,
                },
            ],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut response = ui.add(egui::Slider::new(&mut value, 0.0..=1.0));
                wheel_adjust(ui, &mut response, &mut value, 0., 1.);
            });
        });
        (value, ctx.input(|i| i.smooth_scroll_delta.y))
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
}
