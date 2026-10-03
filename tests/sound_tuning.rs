use bess::{
    drive::Controls,
    engine_build::Muffler,
    hybrid::Settings,
    physical::engine::{Commands, Engine},
    project::Parameters,
    scratch::{Scratch, SoundTuning},
};

type Field = (&'static str, fn(&mut SoundTuning) -> &mut f32, f32);

// One non-default, valid value per public control. Checked against the serialized
// key set so adding a new control cannot silently escape integration coverage.
fn fields() -> [Field; 29] {
    [
        ("exhaust_bass_db", |s| &mut s.exhaust_bass_db, 8.0),
        ("exhaust_body_db", |s| &mut s.exhaust_body_db, 8.0),
        ("exhaust_body_hz", |s| &mut s.exhaust_body_hz, 700.0),
        ("exhaust_body_q", |s| &mut s.exhaust_body_q, 5.0),
        ("exhaust_rasp_db", |s| &mut s.exhaust_rasp_db, -8.0),
        ("exhaust_low_cut_hz", |s| &mut s.exhaust_low_cut_hz, 180.0),
        ("exhaust_high_cut_hz", |s| &mut s.exhaust_high_cut_hz, 900.0),
        ("exhaust_drive", |s| &mut s.exhaust_drive, 0.8),
        ("bass_db", |s| &mut s.bass_db, 8.),
        ("presence_db", |s| &mut s.presence_db, 8.),
        ("treble_db", |s| &mut s.treble_db, -8.),
        ("brightness_hz", |s| &mut s.brightness_hz, 900.),
        ("drive", |s| &mut s.drive, 0.8),
        ("flow_texture", |s| &mut s.flow_texture, 0.8),
        ("rpm_brightness_db", |s| &mut s.rpm_brightness_db, 10.),
        ("load_brightness_db", |s| &mut s.load_brightness_db, 10.),
        ("intake_length_m", |s| &mut s.intake_length_m, 0.9),
        ("intake_resonance", |s| &mut s.intake_resonance, 2.),
        ("intake_air_noise", |s| &mut s.intake_air_noise, 0.8),
        ("mechanical_pitch_hz", |s| &mut s.mechanical_pitch_hz, 1000.),
        ("mechanical_resonance", |s| &mut s.mechanical_resonance, 6.),
        ("cycle_variation", |s| &mut s.cycle_variation, 0.),
        ("combustion_duration", |s| &mut s.combustion_duration, 1.4),
        ("ignition_retard_deg", |s| &mut s.ignition_retard_deg, 12.),
        ("primary_length_scale", |s| &mut s.primary_length_scale, 1.6),
        ("tail_length_m", |s| &mut s.tail_length_m, 2.8),
        ("exhaust_decay_ms", |s| &mut s.exhaust_decay_ms, 35.),
        ("muffler_volume_scale", |s| &mut s.muffler_volume_scale, 2.),
        ("muffler_absorption", |s| &mut s.muffler_absorption, 0.85),
    ]
}

fn configured() -> Scratch {
    let mut scratch = Scratch::default();
    scratch.build.cam = 0.55;
    scratch.build.muffler = Muffler::Baffled;
    scratch.derive_from_build(
        &mut Settings::default(),
        &mut Parameters::default(),
        &mut Controls::default(),
    );
    scratch
}

#[test]
fn every_sound_control_rejects_invalid_values_at_project_and_engine_boundaries() {
    let serialized = serde_json::to_value(SoundTuning::default()).unwrap();
    let keys = serialized.as_object().unwrap();
    assert_eq!(keys.len(), fields().len());
    for (name, field, valid) in fields() {
        assert!(keys.contains_key(name), "{name}");
        let mut scratch = configured();
        *field(&mut scratch.sound) = valid;
        assert!(scratch.validate().is_ok(), "{name}: valid value rejected");
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1e6, 1e6] {
            *field(&mut scratch.sound) = invalid;
            assert!(scratch.sound.validate().is_err(), "{name}: {invalid}");
            assert!(
                scratch.validate().is_err(),
                "{name}: project accepted {invalid}"
            );
            assert!(
                Engine::new(&scratch, 48000).is_err(),
                "{name}: engine accepted {invalid}"
            );
        }
    }
}

#[test]
fn older_and_partial_projects_load_defaults_without_losing_new_controls() {
    let scratch = configured();
    let mut legacy = serde_json::to_value(&scratch).unwrap();
    legacy.as_object_mut().unwrap().remove("sound");
    let loaded: Scratch = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(loaded.sound, SoundTuning::default());
    assert_eq!(loaded, scratch);
    legacy["sound"] = serde_json::json!({"bass_db": 3.5, "combustion_duration": 1.2});
    let partial: Scratch = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        partial.sound,
        SoundTuning {
            bass_db: 3.5,
            combustion_duration: 1.2,
            ..Default::default()
        }
    );

    let mut tuned = partial;
    for (_, field, value) in fields() {
        *field(&mut tuned.sound) = value;
    }
    let reloaded: Scratch = serde_json::from_str(&serde_json::to_string(&tuned).unwrap()).unwrap();
    assert_eq!(reloaded, tuned);
    assert!(reloaded.validate().is_ok());

    // Older projects must render exactly like explicit default controls.
    let mut a = Engine::new(&loaded, 48000).unwrap();
    let mut b = Engine::new(&scratch, 48000).unwrap();
    for _ in 0..12000 {
        let x = a.next(command());
        let y = b.next(command());
        assert_eq!(
            [
                x.exhaust.to_bits(),
                x.intake.to_bits(),
                x.mechanical.to_bits()
            ],
            [
                y.exhaust.to_bits(),
                y.intake.to_bits(),
                y.mechanical.to_bits()
            ]
        );
        assert_eq!(x.heat_j.to_bits(), y.heat_j.to_bits());
    }
    assert!(!a.failed() && !b.failed());
}

#[test]
fn exhaust_decay_keeps_old_defaults_and_round_trips_at_both_boundaries() {
    use bess::engine_definition::EngineDefinition;

    // A pre-decay project already carries other sound adjustments; adding this
    // field must neither erase them nor silently shorten the historical tail.
    let mut original = configured();
    original.sound.exhaust_body_db = 5.;
    original.sound.mechanical_pitch_hz = 1800.;
    let mut old = serde_json::to_value(&original).unwrap();
    old["sound"]
        .as_object_mut()
        .unwrap()
        .remove("exhaust_decay_ms");
    let migrated: Scratch = serde_json::from_value(old).unwrap();
    assert_eq!(migrated.sound.exhaust_decay_ms, 120.);
    assert_eq!(migrated, original);
    assert_eq!(render(&migrated), render(&original));

    for valid in [10., 35., 120., 250.] {
        let mut tuned = migrated.clone();
        tuned.sound.exhaust_decay_ms = valid;
        let definition = EngineDefinition::from_scratch(&tuned);
        let settings = Settings {
            engine: Some(definition),
            physical_sound: tuned.sound,
            ..Default::default()
        };
        assert!(settings.validate().is_ok());
        let reloaded: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(reloaded.engine, Some(definition));
        assert_eq!(reloaded.physical_sound.exhaust_decay_ms, valid);
        assert!(Engine::new(&reloaded.engine.unwrap().to_scratch(), 48000).is_ok());
    }
    for invalid in [0., 9.99, 250.01] {
        let mut bad = migrated.clone();
        bad.sound.exhaust_decay_ms = invalid;
        assert!(bad.sound.validate().is_err());
        assert!(EngineDefinition::from_scratch(&bad).validate().is_err());
        assert!(Engine::new(&bad, 48000).is_err());
    }
}

#[test]
fn intake_air_noise_migrates_old_projects_without_changing_saved_layer_levels() {
    use bess::{bank::SourceRef, engine_definition::EngineDefinition, project};

    let path = std::env::temp_dir().join(format!(
        "bess-intake-migration-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for version in [3, 4] {
        for imported in [false, true] {
            for intake in [0., 0.3, 0.73, 1.] {
                let mut scratch = configured();
                scratch.sound.intake_resonance = 1.3;
                scratch.sound.exhaust_body_db = 3.;
                let definition = EngineDefinition::from_scratch(&scratch);
                let original = project::Project {
                    version,
                    parameters: Parameters {
                        intake,
                        exhaust: 0.63,
                        mechanical: 0.31,
                        volume: 0.42,
                        ..Default::default()
                    },
                    hybrid: Settings {
                        engine: (version == 4).then_some(definition),
                        physical_sound: scratch.sound,
                        ..Default::default()
                    },
                    source: imported.then(|| SourceRef {
                        archive: "original.zip".into(),
                        blend: "original.sfxBlend2D.json".into(),
                        fingerprint: "original-audio".into(),
                        engine_fingerprint: Some("original-engine".into()),
                    }),
                    driving: Default::default(),
                    profile_name: "Saved levels".into(),
                    scratch: (!imported).then_some(scratch.clone()),
                };
                let mut old = serde_json::to_value(&original).unwrap();
                for pointer in [
                    "/scratch/sound",
                    "/hybrid/engine/sound",
                    "/hybrid/physical_sound",
                ] {
                    if let Some(sound) = old.pointer_mut(pointer).and_then(|v| v.as_object_mut()) {
                        sound.remove("intake_air_noise");
                    }
                }
                std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
                let loaded = project::load_project(&path).unwrap();
                assert_eq!(loaded.parameters, original.parameters);
                assert_eq!(loaded.hybrid.physical_sound, scratch.sound);
                assert_eq!(loaded.hybrid.physical_sound.intake_air_noise, 0.008);
                assert_eq!(
                    serde_json::to_value(&loaded.source).unwrap(),
                    serde_json::to_value(&original.source).unwrap()
                );
                if let Some(engine) = loaded.hybrid.engine {
                    assert_eq!(engine, definition);
                }
                project::save_project(&path, &loaded).unwrap();
                let roundtrip = project::load_project(&path).unwrap();
                assert_eq!(roundtrip.parameters, original.parameters);
                assert_eq!(roundtrip.hybrid.physical_sound, scratch.sound);
            }
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn intake_air_noise_round_trips_explicit_values_and_rejects_outside_bounds() {
    use bess::engine_definition::EngineDefinition;

    let mut scratch = configured();
    for valid in [0., 0.2, 0.73, 1.] {
        scratch.sound.intake_air_noise = valid;
        let definition = EngineDefinition::from_scratch(&scratch);
        let settings = Settings {
            engine: Some(definition),
            physical_sound: scratch.sound,
            ..Default::default()
        };
        assert!(settings.validate().is_ok());
        let serialized = serde_json::to_string(&settings).unwrap();
        let loaded: Settings = serde_json::from_str(&serialized).unwrap();
        assert_eq!(loaded.engine, Some(definition));
        assert_eq!(loaded.physical_sound.intake_air_noise, valid);
        assert!(Engine::new(&loaded.engine.unwrap().to_scratch(), 48000).is_ok());
    }
    for invalid in [-0.01, 1.01] {
        scratch.sound.intake_air_noise = invalid;
        assert!(scratch.sound.validate().is_err());
        assert!(EngineDefinition::from_scratch(&scratch).validate().is_err());
        assert!(Engine::new(&scratch, 48000).is_err());
    }
}

#[test]
fn changing_engine_parts_preserves_every_user_sound_adjustment() {
    let mut scratch = configured();
    for (_, field, value) in fields() {
        *field(&mut scratch.sound) = value;
    }
    let expected = scratch.sound;
    scratch.build.cam = 0.2;
    scratch.build.muffler = Muffler::ReverseFlow;
    scratch.build.exhaust_mm = 70.;
    scratch.derive_from_build(
        &mut Settings::default(),
        &mut Parameters::default(),
        &mut Controls::default(),
    );
    assert_eq!(scratch.sound, expected);
    assert!(scratch.validate().is_ok());
}

fn command() -> Commands {
    Commands {
        imposed_rpm: Some(3000.),
        throttle: 0.6,
        ..Default::default()
    }
}

fn render(scratch: &Scratch) -> (Vec<[f32; 3]>, Vec<f64>) {
    let mut engine = Engine::new(scratch, 48000).unwrap();
    let mut samples = Vec::with_capacity(12000);
    let mut heat = Vec::with_capacity(12000);
    for frame in 0..36000 {
        let sample = engine.next(command());
        assert!(!engine.failed());
        let stems = [sample.exhaust, sample.intake, sample.mechanical];
        assert!(stems.iter().all(|x| x.is_finite()));
        assert!(sample.heat_j.is_finite() && sample.correction_j.is_finite());
        if frame >= 24000 {
            samples.push(stems);
            heat.push(sample.heat_j);
        }
    }
    assert!(
        heat.iter().sum::<f64>() > 100.,
        "test requires firing under load"
    );
    (samples, heat)
}

#[test]
fn every_control_changes_a_real_running_engine_stem_in_its_active_conditions() {
    let base = configured();
    assert!(!base.experimental.wave_coupling);
    let reference = render(&base);
    for (name, field, value) in fields() {
        let mut tuned = base.clone();
        // Intake length sets the physical runners; no resonance EQ needed.
        let active_reference = if name == "exhaust_body_hz" || name == "exhaust_body_q" {
            tuned.sound.exhaust_body_db = 8.;
            Some(render(&tuned))
        } else {
            None
        };
        let reference = active_reference.as_ref().unwrap_or(&reference);
        *field(&mut tuned.sound) = value;
        let changed = render(&tuned);
        let stem = match name {
            "intake_length_m" | "intake_resonance" | "intake_air_noise" => 1,
            "mechanical_pitch_hz" | "mechanical_resonance" => 2,
            _ => 0,
        };
        let energy: f64 = reference.0.iter().map(|s| f64::from(s[stem]).powi(2)).sum();
        let difference: f64 = reference
            .0
            .iter()
            .zip(&changed.0)
            .map(|(a, b)| f64::from(a[stem] - b[stem]).powi(2))
            .sum();
        assert!(energy > 1e-12, "{name}: inactive reference stem");
        assert!(
            difference > energy * 1e-8,
            "{name}: disconnected control, relative difference {}",
            difference / energy
        );
        if name.starts_with("exhaust_") {
            assert_eq!(reference.1, changed.1);
            for (a, b) in reference.0.iter().zip(&changed.0) {
                assert_eq!(a[1..], b[1..], "{name}: changed another stem");
            }
        }
        if name == "intake_air_noise" {
            for (a, b) in reference.0.iter().zip(&changed.0) {
                assert_eq!(a[0].to_bits(), b[0].to_bits(), "changed exhaust");
                assert_eq!(a[2].to_bits(), b[2].to_bits(), "changed mechanical");
            }
        }
        if matches!(
            name,
            "bass_db"
                | "presence_db"
                | "treble_db"
                | "brightness_hz"
                | "drive"
                | "flow_texture"
                | "rpm_brightness_db"
                | "load_brightness_db"
                | "intake_length_m"
                | "intake_resonance"
                | "intake_air_noise"
                | "mechanical_pitch_hz"
                | "mechanical_resonance"
        ) {
            assert_eq!(
                reference.1, changed.1,
                "{name}: observation control changed combustion"
            );
        }
    }
}

#[test]
fn exhaust_reset_preserves_other_tone_and_geometry_and_settings_roundtrip() {
    let mut s = SoundTuning::default();
    for (_, field, value) in fields() {
        *field(&mut s) = value;
    }
    let settings = Settings {
        physical_sound: s,
        ..Default::default()
    };
    let loaded: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(loaded.physical_sound, s);
    let before = s;
    s.reset_exhaust_tone();
    let defaults = SoundTuning::default();
    for (name, field, _) in fields() {
        // Decay belongs to propagation/geometry, not the observation EQ reset.
        let mut expected = if name.starts_with("exhaust_") && name != "exhaust_decay_ms" {
            defaults
        } else {
            before
        };
        assert_eq!(*field(&mut s), *field(&mut expected), "{name}");
    }
}
