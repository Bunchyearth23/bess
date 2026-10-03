//! End-to-end editor state boundaries without opening an audio device.
use super::*;
#[path = "test_support.rs"]
mod test_support;

fn imported_app(ctx: &egui::Context) -> App {
    let bank = Arc::new(Bank::load(&test_support::automation_fixture(), None).unwrap());
    let model = bess::automation_model::AutomationModel::from_bank(&bank).unwrap();
    let mut app = App::with_ctx(ctx, None);
    app.firing_text = firing_text(&model.scratch.design);
    app.scratch = Some(model.scratch.clone());
    app.settings.engine_baseline = Some(model.baseline);
    app.automation_model = Some(Ok(model));
    app.bank = Some(bank);
    app.persist_engine_edit();
    app
}

#[test]
fn new_engine_mixes_mute_mechanics_and_saved_projects_keep_their_level() {
    use bess::engine_build::{Fuel, Head};

    assert_eq!(Parameters::default().mechanical, 0.);
    for preset in 0..4 {
        assert_eq!(Parameters::preset(preset).mechanical, 0.);
    }
    let mut scratch = Scratch::default();
    let mut settings = Settings::default();
    let mut controls = Controls::default();
    for head in [Head::Pushrod, Head::Sohc, Head::Dohc] {
        scratch.build.head = head;
        scratch.build.fuel = Fuel::DirectInjection;
        let mut params = Parameters::default();
        scratch.derive_from_build(&mut settings, &mut params, &mut controls);
        assert_eq!(params.mechanical, 0., "fresh scratch {head:?}");
        params.mechanical = 0.37;
        scratch.derive_from_build(&mut settings, &mut params, &mut controls);
        assert_eq!(params.mechanical, 0.37, "explicit scratch {head:?}");
    }

    // Receive the actual import worker's result without opening audio hardware.
    let ctx = egui::Context::default();
    let mut app = App::with_ctx(&ctx, None);
    let archive = test_support::automation_fixture();
    app.import(archive.clone(), None);
    let loaded = app
        .importer
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(loaded.params.mechanical, 0.);
    let saved = Project {
        version: 4,
        parameters: Parameters {
            mechanical: 0.37,
            ..loaded.params
        },
        hybrid: loaded.settings,
        source: Some(loaded.bank.source.clone()),
        driving: loaded.driving,
        profile_name: loaded.profile_name,
        scratch: None,
    };
    let path = std::env::temp_dir().join(format!(
        "bess-mechanical-default-{}.json",
        std::process::id()
    ));
    project::save_project(&path, &saved).unwrap();
    let restored = project::load_project(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(restored.parameters.mechanical, 0.37);
    app.import(archive, Some(restored));
    let loaded = app
        .importer
        .take()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    assert_eq!(loaded.params.mechanical, 0.37);
}

#[test]
fn imported_edits_survive_project_roundtrip_and_invalidate_export_levels() {
    let ctx = egui::Context::default();
    let mut app = imported_app(&ctx);
    let original_key = app.level_key().unwrap();
    let baseline = app
        .automation_model
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .baseline;
    let scratch = app.scratch.as_mut().unwrap();
    scratch.build.compression = 11.7;
    scratch.tuning.intake.throttle_mm = Some(48.);
    scratch.sound.exhaust_bass_db = 0.7;
    scratch.sound.exhaust_decay_ms = 40.;
    app.persist_engine_edit();
    assert!(app.level_key().unwrap() != original_key);
    let snapshot = app.project();
    assert!(snapshot.source.is_some());
    assert!(
        snapshot.scratch.is_none(),
        "imported engine must keep its Automation origin"
    );
    assert_eq!(snapshot.hybrid.engine.unwrap().build.compression, 11.7);
    let path = std::env::temp_dir().join(format!("bess-workshop-ui-{}.json", std::process::id()));
    project::save_project(&path, &snapshot).unwrap();
    let restored = project::load_project(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(restored.hybrid.engine, snapshot.hybrid.engine);
    assert_eq!(restored.hybrid.engine.unwrap().sound.exhaust_decay_ms, 40.);
    assert_eq!(restored.hybrid.engine_baseline, Some(baseline));
    assert_eq!(
        restored.source.unwrap().fingerprint,
        snapshot.source.unwrap().fingerprint
    );
    let model = app.automation_model.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(
        model.baseline, baseline,
        "editing B cannot rewrite its source reference"
    );
    assert!(
        model
            .provenance
            .iter()
            .any(|p| p.field == "build.compression"
                && p.origin == bess::automation_model::ValueOrigin::Modified)
    );
}

#[test]
fn changing_engine_parts_preserves_vehicle_and_listening_settings() {
    let mut scratch = Scratch::default();
    let mut settings = Settings::default();
    let mut params = Parameters::default();
    let mut driving = Controls::default();
    scratch.derive_from_build(&mut settings, &mut params, &mut driving);
    driving.peak_torque_nm = 278.;
    driving.mass_kg = 1372.;
    driving.set_gear_count(7).unwrap();
    driving.set_ratio(6, 0.60);
    let vehicle = driving;
    params.exhaust = 0.43;
    params.intake = 0.27;
    params.mechanical = 0.19;
    settings.fuel_cut = 0.36;
    scratch.idle_rpm = 812.;
    scratch.redline_rpm = 7123.;
    scratch.inertia = 0.27;
    scratch.build.compression = 11.7;
    refresh_engine_build(&mut scratch, true, &mut settings, &mut params, &mut driving);
    assert_eq!(driving, vehicle);
    assert_eq!(
        (scratch.idle_rpm, scratch.redline_rpm, scratch.inertia),
        (812., 7123., 0.27)
    );
    assert_eq!(
        (
            params.exhaust,
            params.intake,
            params.mechanical,
            settings.fuel_cut
        ),
        (0.43, 0.27, 0.19, 0.36)
    );
    refresh_engine_build(
        &mut scratch,
        false,
        &mut settings,
        &mut params,
        &mut driving,
    );
    assert_eq!(driving, vehicle);
}

#[test]
fn restoring_one_imported_section_keeps_other_edits() {
    let ctx = egui::Context::default();
    let mut app = imported_app(&ctx);
    let baseline = app.settings.engine.unwrap();
    let mut edited = baseline;
    edited.build.compression = 11.7;
    edited.sound.exhaust_bass_db = 0.7;
    edited.reset_section(&baseline, EngineSection::Parts);
    app.scratch = Some(edited.to_scratch());
    app.persist_engine_edit();
    let saved = app.project().hybrid.engine.unwrap();
    assert_eq!(saved.build, baseline.build);
    assert_eq!(saved.sound.exhaust_bass_db, 0.7);
}

#[test]
fn invalid_visible_draft_cannot_silently_save_the_previous_valid_engine() {
    let ctx = egui::Context::default();
    let mut app = imported_app(&ctx);
    app.scratch.as_mut().unwrap().inertia = f32::NAN;
    app.persist_engine_edit();
    assert!(!app.engine_draft_valid());
    assert!(app.automation_model.as_ref().unwrap().is_ok());
    let path =
        std::env::temp_dir().join(format!("bess-invalid-workshop-{}.json", std::process::id()));
    assert!(project::save_project(&path, &app.project()).is_err());
    assert!(!path.exists());
}

#[test]
fn imported_workshop_retains_source_ab_controls_and_all_engine_controls() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = imported_app(&ctx);
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.listen_controls(ui));
    });
    let update = output.platform_output.accesskit_update.unwrap();
    let labels: Vec<_> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| node.label())
        .collect();
    assert!(
        labels
            .iter()
            .any(|label| label.contains("A · Source Automation")),
        "{labels:?}"
    );
    assert!(
        labels
            .iter()
            .any(|label| label.contains("B · BESS physical engine")),
        "{labels:?}"
    );
    // Both origins render the same full editor; import adds only restoration
    // buttons and provenance, never replaces the engine controls with a subset.
    fn slider_count(ctx: &egui::Context, app: &mut App) -> usize {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.scratch_panel(ui));
        });
        let update = output.platform_output.accesskit_update.unwrap();
        // egui stores plain Label text in value(), while controls use label().
        // Both receive their widget bounds from fill_accesskit_node_common.
        let top = |label| {
            update
                .nodes
                .iter()
                .filter(|(_, node)| node.label().or(node.value()) == Some(label))
                .find_map(|(_, node)| node.bounds())
                .unwrap_or_else(|| panic!("missing bounds for {label}"))
                .y0
        };
        assert!(top("Exhaust sound") < top("Block"));
        assert!(top("Exhaust decay") < top("Block"));
        // Units sit beside the value, outside the slider's accessible label.
        for (label, role) in [
            ("Exhaust decay", egui::accesskit::Role::Slider),
            (
                "Experimental back pressure",
                egui::accesskit::Role::CheckBox,
            ),
        ] {
            assert_eq!(
                update
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.label() == Some(label) && node.role() == role)
                    .count(),
                1,
                "both origins must expose one {label} control"
            );
        }
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::accesskit::Role::Slider)
            .count()
    }
    let imported = slider_count(&ctx, &mut app);
    let scratch = app.scratch.as_mut().unwrap();
    scratch.sound.exhaust_decay_ms = 40.;
    scratch.experimental.wave_coupling = true;
    scratch.experimental.coupled_level_db = -4.5;
    app.persist_engine_edit();
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.beamng_export_controls(ui, false));
    });
    let update = output.platform_output.accesskit_update.unwrap();
    let export = update
        .nodes
        .iter()
        .map(|(_, node)| node)
        .find(|node| {
            node.role() == egui::accesskit::Role::Button
                && node.label() == Some("Create BeamNG configuration…")
        })
        .expect("imported engines must expose BeamNG export");
    assert!(!export.is_disabled());
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, node)| node.label().or(node.value())
                == Some(
                    "Exported exhaust: decay 40 ms · back pressure On · coupled level -4.5 dB"
                ))
    );
    app.bank = None;
    app.automation_model = None;
    let free = slider_count(&ctx, &mut app);
    assert!(imported > 30);
    assert_eq!(imported, free);
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.beamng_export_controls(ui, false));
    });
    let update = output.platform_output.accesskit_update.unwrap();
    assert!(
        update
            .nodes
            .iter()
            .any(|(_, node)| node
                .label()
                .or(node.value())
                .is_some_and(|label| label.contains("Import an Automation vehicle ZIP")
                    && label.contains("WAV")))
    );
    assert!(
        update
            .nodes
            .iter()
            .all(|(_, node)| node.label() != Some("Create BeamNG configuration…"))
    );
}
