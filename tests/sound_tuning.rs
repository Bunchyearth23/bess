use bess::{
    drive::Controls,
    engine_build::Muffler,
    hybrid::Settings,
    physical::engine::{Commands, Engine},
    project::Parameters,
    scratch::{Scratch, SoundTuning},
};

type Field = (&'static str, fn(&mut SoundTuning) -> &mut f32, f32);

// One non-neutral, valid value per public control. Checked against the serialized
// key set so adding a new control cannot silently escape integration coverage.
fn fields() -> [Field; 27] {
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
        ("mechanical_pitch_hz", |s| &mut s.mechanical_pitch_hz, 1000.),
        ("mechanical_resonance", |s| &mut s.mechanical_resonance, 6.),
        ("cycle_variation", |s| &mut s.cycle_variation, 0.),
        ("combustion_duration", |s| &mut s.combustion_duration, 1.4),
        ("ignition_retard_deg", |s| &mut s.ignition_retard_deg, 12.),
        ("primary_length_scale", |s| &mut s.primary_length_scale, 1.6),
        ("tail_length_m", |s| &mut s.tail_length_m, 2.8),
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
fn older_and_partial_projects_load_neutral_defaults_without_losing_new_controls() {
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

    // Older projects must render exactly like explicit neutral controls.
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
    let reference = render(&base);
    for (name, field, value) in fields() {
        let mut tuned = base.clone();
        // Intake length has no audible function until its resonance is enabled.
        let active_reference = if name == "intake_length_m" {
            tuned.sound.intake_resonance = 1.;
            Some(render(&tuned))
        } else if name == "exhaust_body_hz" || name == "exhaust_body_q" {
            tuned.sound.exhaust_body_db = 8.;
            Some(render(&tuned))
        } else {
            None
        };
        let reference = active_reference.as_ref().unwrap_or(&reference);
        *field(&mut tuned.sound) = value;
        let changed = render(&tuned);
        let stem = match name {
            "intake_length_m" | "intake_resonance" => 1,
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
        let mut expected = if name.starts_with("exhaust_") {
            defaults
        } else {
            before
        };
        assert_eq!(*field(&mut s), *field(&mut expected), "{name}");
    }
}
