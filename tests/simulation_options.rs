use bess::{
    dyno,
    engine_definition::EngineDefinition,
    physical::{
        combustion_model::PredictiveCombustionConfig, crank::TorsionConfig,
        fuel_transient::FuelTransientConfig, structural::StructuralResponse, turbo_map::TurboModel,
    },
    project::{self, Project},
    scratch::Scratch,
};

thread_local! {
    static TRACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static HEAP_OPERATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct CountingAllocator;
// SAFETY: unchanged pointer/layout forwarding to System; tracking is thread-local.
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if TRACK.try_with(|v| v.get()).unwrap_or(false) {
            let _ = HEAP_OPERATIONS.try_with(|v| v.set(v.get() + 1));
        }
        // SAFETY: unchanged valid GlobalAlloc layout.
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        if TRACK.try_with(|v| v.get()).unwrap_or(false) {
            let _ = HEAP_OPERATIONS.try_with(|v| v.set(v.get() + 1));
        }
        // SAFETY: pointer and layout belong to System.
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn enhanced() -> Scratch {
    let mut scratch = Scratch::default();
    scratch.experimental.intake_wave_coupling = true;
    scratch.experimental.fuel_transients = Some(FuelTransientConfig::default());
    scratch.experimental.predictive_combustion = Some(PredictiveCombustionConfig::default());
    scratch.experimental.turbo_model = Some(TurboModel::synthetic_example());
    scratch.experimental.torsion = Some(TorsionConfig::default());
    scratch.experimental.mechanical_transfer = Some(StructuralResponse::synthetic_example());
    scratch
}

#[test]
fn old_project_missing_new_options_keeps_legacy_defaults() {
    let scratch = Scratch::default();
    let mut value = serde_json::to_value(&scratch).unwrap();
    let fields = value["experimental"].as_object_mut().unwrap();
    for key in [
        "intake_wave_coupling",
        "fuel_transients",
        "predictive_combustion",
        "turbo_model",
        "torsion",
        "mechanical_transfer",
    ] {
        fields.remove(key);
    }
    let restored: Scratch = serde_json::from_value(value).unwrap();
    assert_eq!(restored, scratch);
}

#[test]
fn liquid_delivery_defaults_and_explicit_cylinder_roundtrip() {
    use bess::physical::fuel_transient::FuelDelivery;
    let mut value = serde_json::to_value(FuelTransientConfig::default()).unwrap();
    value.as_object_mut().unwrap().remove("delivery");
    let mut restored: FuelTransientConfig = serde_json::from_value(value).unwrap();
    assert_eq!(restored.delivery, FuelDelivery::Admission);
    restored.delivery = FuelDelivery::Cylinder;
    restored.injection_start_deg = 550.;
    restored.validate().unwrap();
    let parsed: FuelTransientConfig =
        serde_json::from_str(&serde_json::to_string(&restored).unwrap()).unwrap();
    assert_eq!(parsed, restored);
}

#[test]
fn enhanced_shared_definition_project_roundtrips_and_baseline_is_preserved() {
    let scratch = enhanced();
    let definition = EngineDefinition::from_scratch(&scratch);
    definition.validate().unwrap();
    let baseline = EngineDefinition::default();
    let settings = bess::hybrid::Settings {
        engine: Some(definition),
        engine_baseline: Some(baseline),
        ..Default::default()
    };
    let mut project = Project {
        version: 4,
        parameters: Default::default(),
        hybrid: settings,
        source: None,
        driving: Default::default(),
        profile_name: "Simulation engineering fixture".into(),
        scratch: Some(scratch.clone()),
    };
    let path = std::env::temp_dir().join(format!(
        "bess-simulation-options-{}.json",
        std::process::id()
    ));
    project::save_project(&path, &project).unwrap();
    let restored = project::load_project(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(restored.hybrid.engine, Some(definition));
    assert_eq!(restored.hybrid.engine_baseline, None); // A free engine has no imported baseline.
    assert_eq!(restored.scratch.unwrap().experimental, scratch.experimental);
    assert_eq!(restored.parameters.mechanical, 0.);
    project.scratch = None;
    project.source = Some(bess::bank::SourceRef {
        archive: "original.zip".into(),
        blend: "original.sfxBlend2D.json".into(),
        fingerprint: "original-audio".into(),
        engine_fingerprint: Some("original-engine".into()),
    });
    project::save_project(&path, &project).unwrap();
    let restored = project::load_project(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(restored.hybrid.engine, Some(definition));
    assert_eq!(restored.hybrid.engine_baseline, Some(baseline));
    assert_eq!(restored.source.unwrap().fingerprint, "original-audio");
}

#[test]
fn physical_dyno_knobs_are_keyed_but_observation_transfer_is_not() {
    let base = Scratch::default();
    let mut changed = base.clone();
    changed.experimental.mechanical_transfer = Some(StructuralResponse::synthetic_example());
    assert_eq!(dyno::key(&base), dyno::key(&changed));
    for choice in 0..5 {
        let mut changed = base.clone();
        match choice {
            0 => changed.experimental.intake_wave_coupling = true,
            1 => changed.experimental.fuel_transients = Some(FuelTransientConfig::default()),
            2 => {
                changed.experimental.predictive_combustion =
                    Some(PredictiveCombustionConfig::default())
            }
            3 => changed.experimental.turbo_model = Some(TurboModel::synthetic_example()),
            _ => changed.experimental.torsion = Some(TorsionConfig::default()),
        }
        assert_ne!(dyno::key(&base), dyno::key(&changed));
    }
    let mut intake = base.clone();
    intake.experimental.intake_wave_coupling = true;
    let mut longer = intake.clone();
    longer.sound.intake_length_m += 0.1;
    assert_ne!(dyno::key(&intake), dyno::key(&longer));
    let mut observed = intake.clone();
    observed.sound.intake_resonance += 0.1;
    assert_eq!(dyno::key(&intake), dyno::key(&observed));
}

#[test]
fn invalid_saved_model_options_are_rejected_before_rendering() {
    let mut scratch = Scratch::default();
    scratch.experimental.fuel_transients = Some(FuelTransientConfig {
        deposited_fraction: f64::NAN,
        ..Default::default()
    });
    assert!(scratch.validate().is_err());
    let mut scratch = Scratch::default();
    scratch.experimental.torsion = Some(TorsionConfig {
        damping_nm_s_rad: -1.,
        ..Default::default()
    });
    assert!(scratch.validate().is_err());
}

#[test]
fn imported_model_coefficients_roundtrip_without_decimal_drift() {
    let mut scratch = enhanced();
    let coefficient = 3600. / (3600. * std::f64::consts::TAU / 60.);
    scratch
        .experimental
        .torsion
        .as_mut()
        .unwrap()
        .damping_nm_s_rad = coefficient;
    let encoded = serde_json::to_string(&scratch).unwrap();
    let restored: Scratch = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        restored
            .experimental
            .torsion
            .unwrap()
            .damping_nm_s_rad
            .to_bits(),
        coefficient.to_bits()
    );
    assert_eq!(restored.experimental, scratch.experimental);
}

#[test]
fn combined_models_share_live_and_wav_samples_without_hot_path_heap_operations() {
    use bess::{
        drive::{Controls, Mode},
        hybrid::Settings,
        project::Parameters,
        realtime::{DenormalGuard, RenderEngine},
    };
    let _guard = DenormalGuard::enter();
    let mut scratch = enhanced();
    scratch.build.aspiration = bess::engine_build::Aspiration::Turbo;
    scratch.build.boost_bar = 0.3;
    let parameters = Parameters {
        rpm: 2600.,
        load: 0.6,
        volume: 0.2,
        mechanical: 0.4,
        ..Default::default()
    };
    let settings = Settings {
        engine: Some(EngineDefinition::from_scratch(&scratch)),
        level_match: false,
        ..Default::default()
    };
    let controls = Controls {
        mode: Mode::Direct,
        ..Default::default()
    };
    // The WAV path adds only the final file fade after the shared live engine.
    let expected =
        bess::render::scratch_samples(parameters, settings, &scratch, 1., controls).unwrap();
    let mut live = RenderEngine::scratch(48000, parameters, settings, controls, &scratch).unwrap();
    live.bench.set_cycle_seconds(1.);
    HEAP_OPERATIONS.with(|v| v.set(0));
    TRACK.with(|v| v.set(true));
    for (i, expected) in expected.iter().enumerate() {
        let actual = live.next_sample(true) * ((48000 - i) as f32 / 2400.).min(1.);
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
    TRACK.with(|v| v.set(false));
    assert!(!live.bench.failed());
    assert_eq!(HEAP_OPERATIONS.with(|v| v.get()), 0);
}
