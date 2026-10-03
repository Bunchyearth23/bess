//! Full-vehicle export retains the original archive and the edited engine.
#[path = "../src/test_support.rs"]
mod test_support;

use bess::{
    automation_model::AutomationModel,
    bank::Bank,
    engine_definition::EngineDefinition,
    export,
    hybrid::Settings,
    project::{self, Parameters},
};
use std::{collections::BTreeSet, fs, io::Read, sync::Arc};

#[test]
fn complete_vehicle_export_preserves_source_and_the_current_engine() {
    // The fixture atomically reserves a unique name. Keep its associated
    // output beside it so a concurrent fixture cannot share our destination.
    let source = test_support::automation_fixture();
    let output = source.with_extension("full-export");
    assert!(!output.exists());
    let original = fs::read(&source).unwrap();
    let bank = Arc::new(Bank::load(&source, None).unwrap());
    let baseline = AutomationModel::from_settings(&bank, &Settings::default())
        .unwrap()
        .baseline;
    let mut current = baseline;
    current.build.compression = 10.2;
    current.build.cam = 0.41;
    current.idle_rpm = 900.;
    current.redline_rpm = 6400.;
    current.inertia = 0.337;
    current.sound.exhaust_decay_ms = 40.;
    current.sound.intake_air_noise = 0.017;
    current.sound.presence_db = 2.;
    current.validate().unwrap();
    let settings = Settings {
        engine: Some(current),
        engine_baseline: Some(baseline),
        // Deliberately stale compatibility fields must not override the
        // complete active engine when the export resolves its model.
        physical_sound: baseline.sound,
        engine_gain: 0.77,
        idle_gain: 0.85,
        ..Settings::default()
    };
    let parameters = Parameters {
        intake: 0.21,
        mechanical: 0.09,
        exhaust: 0.73,
        volume: 0.42,
        ..Parameters::default()
    };
    export::package(&output, parameters, settings, bank.clone()).unwrap();

    assert_eq!(fs::read(&source).unwrap(), original);
    let mut before = zip::ZipArchive::new(std::io::Cursor::new(&original)).unwrap();
    let archive_path = output.join(export::package_name(&bank));
    let mut after = zip::ZipArchive::new(fs::File::open(&archive_path).unwrap()).unwrap();
    let before_names: BTreeSet<_> = before.file_names().map(str::to_owned).collect();
    let after_names: BTreeSet<_> = after.file_names().map(str::to_owned).collect();
    let mut expected_names = before_names.clone();
    expected_names.insert(bess::babm_exchange::MARKER_PATH.into());
    assert_eq!(after_names, expected_names);
    assert_eq!(after.len(), before.len() + 1);
    let marker: bess::babm_exchange::ExportMarker =
        serde_json::from_reader(after.by_name(bess::babm_exchange::MARKER_PATH).unwrap()).unwrap();
    assert_eq!(marker.version, 1);
    assert_eq!(marker.kind, "bess-full-vehicle");
    assert_eq!(
        marker.source_archive_sha256,
        bess::babm_exchange::sha256(&original)
    );
    assert_eq!(marker.vehicle_root, "vehicles/test/");
    assert_eq!(marker.blend_path, bank.source.blend);
    assert_eq!(marker.sounds.len(), 4);
    let mut changed_wavs = 0;
    let mut labelled_info = 0;
    for name in &before_names {
        let mut old_bytes = Vec::new();
        before
            .by_name(name)
            .unwrap()
            .read_to_end(&mut old_bytes)
            .unwrap();
        let mut new_bytes = Vec::new();
        after
            .by_name(name)
            .unwrap()
            .read_to_end(&mut new_bytes)
            .unwrap();
        if name.ends_with(".wav") {
            let sound = marker
                .sounds
                .iter()
                .find(|sound| &sound.path == name)
                .unwrap();
            assert_eq!(
                sound.original_sha256,
                bess::babm_exchange::sha256(&old_bytes)
            );
            assert_eq!(
                sound.rendered_sha256,
                bess::babm_exchange::sha256(&new_bytes)
            );
            assert_ne!(new_bytes, old_bytes, "Unreplaced engine WAV: {name}");
            let (rate, samples) = bess::bank::decode_wav(&new_bytes).unwrap();
            assert_eq!(rate, 48_000);
            assert!(samples.len() >= 96_000);
            assert!(samples.iter().all(|v| v.is_finite() && v.abs() <= 0.951));
            assert!(samples.iter().any(|v| v.abs() > 1e-4));
            changed_wavs += 1;
        } else if name == "vehicles/test/info.json" {
            assert_eq!(new_bytes, export::label_vehicle_info(&old_bytes).unwrap().0);
            labelled_info += 1;
        } else {
            assert_eq!(new_bytes, old_bytes, "Changed vehicle member: {name}");
        }
    }
    assert_eq!(changed_wavs, 4, "Two RPM points in each load row");
    assert_eq!(labelled_info, 1);

    let saved = project::load_project(&output.join("settings.bess.json")).unwrap();
    assert_eq!(saved.source.as_ref(), Some(&bank.source));
    assert_eq!(saved.hybrid.engine, Some(current));
    assert_eq!(saved.hybrid.engine_baseline, Some(baseline));
    assert_eq!(saved.hybrid.physical_sound, current.sound);
    assert_eq!(saved.hybrid.engine_gain, settings.engine_gain);
    assert_eq!(saved.hybrid.idle_gain, settings.idle_gain);
    assert_eq!(
        serde_json::to_value(saved.parameters).unwrap(),
        serde_json::to_value(parameters).unwrap()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["render_channel"], "mixed");
    assert_eq!(manifest["render_model"], "physical_automation");
    assert_eq!(manifest["loops"].as_array().unwrap().len(), 4);
    assert_eq!(
        manifest["source"],
        serde_json::to_value(&bank.source).unwrap()
    );
    assert_eq!(
        serde_json::from_value::<EngineDefinition>(manifest["engine_definition"].clone()).unwrap(),
        current
    );
    assert_eq!(
        serde_json::from_value::<EngineDefinition>(manifest["engine_baseline"].clone()).unwrap(),
        baseline
    );
    let reimported = Bank::load(&archive_path, None).unwrap();
    assert_eq!(reimported.min_rpm, bank.min_rpm);
    assert_eq!(reimported.max_rpm, bank.max_rpm);
    let again_dir = output.join("re-export");
    let reimported = Arc::new(reimported);
    let mut next_settings = settings;
    next_settings.engine.as_mut().unwrap().sound.presence_db = -2.;
    export::package(&again_dir, parameters, next_settings, reimported.clone()).unwrap();
    let mut again = zip::ZipArchive::new(
        fs::File::open(again_dir.join(export::package_name(&reimported))).unwrap(),
    )
    .unwrap();
    let next: bess::babm_exchange::ExportMarker =
        serde_json::from_reader(again.by_name(bess::babm_exchange::MARKER_PATH).unwrap()).unwrap();
    assert_eq!(next.source_archive_sha256, marker.source_archive_sha256);
    assert!(next.exported_at_unix_ms > marker.exported_at_unix_ms);
    assert_eq!(
        next.sounds
            .iter()
            .map(|s| (&s.path, &s.original_sha256))
            .collect::<Vec<_>>(),
        marker
            .sounds
            .iter()
            .map(|s| (&s.path, &s.original_sha256))
            .collect::<Vec<_>>()
    );
    assert!(
        next.sounds
            .iter()
            .zip(&marker.sounds)
            .any(|(a, b)| a.rendered_sha256 != b.rendered_sha256)
    );
    // Importing the result is also read-only with respect to the A reference.
    assert_eq!(fs::read(&source).unwrap(), original);
    // Optional durable inputs for the cross-repository CLI acceptance check.
    // Creation must be exclusive so a rerun never overwrites existing artifacts.
    if let Some(root) = std::env::var_os("BESS_BABM_TEST_ARTIFACTS") {
        let root = std::path::PathBuf::from(root);
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("exports")).unwrap();
        fs::copy(&source, root.join("original.zip")).unwrap();
        fs::copy(&archive_path, root.join("exports/first.zip")).unwrap();
        fs::copy(
            again_dir.join(export::package_name(&reimported)),
            root.join("exports/revised.zip"),
        )
        .unwrap();
        fs::write(
            root.join("receipt.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "source_sha256":marker.source_archive_sha256,
                "first":marker,
                "revised":next,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
