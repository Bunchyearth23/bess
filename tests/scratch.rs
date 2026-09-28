mod physical {
    use bess::{
        bench::Bench,
        drive::{Controls, Mode},
        hybrid::Settings,
        project::Parameters,
        scratch::{Scratch, ScratchModel, ScratchVoice},
    };

    fn make(scratch: &Scratch, params: Parameters, settings: Settings) -> Bench {
        let model = ScratchModel::build(scratch, 48_000).unwrap();
        assert!(model.physical.is_some(), "default scratch must be physical");
        Bench::from_scratch(48_000, params, settings, Controls::default(), model)
    }

    fn render(bench: &mut Bench, count: usize) -> Vec<f32> {
        let x: Vec<_> = (0..count).map(|_| bench.next(true)).collect();
        assert!(!bench.failed(), "physical engine failed while rendering");
        assert!(x.iter().all(|x| x.is_finite() && x.abs() <= 0.892));
        x
    }

    #[test]
    fn default_scratch_plays_physical_flow_and_reports_imposed_rpm() {
        let mut bench = make(
            &Scratch::default(),
            Parameters {
                rpm: 2200.,
                load: 0.4,
                volume: 0.3,
                ..Default::default()
            },
            Settings::default(),
        );
        let x = render(&mut bench, 24_000);
        assert!(x.iter().any(|x| x.abs() > 1e-4));
        assert!((bench.state().rpm - 2200.).abs() < 0.01);
    }

    #[test]
    fn legacy_pulse_controls_cannot_change_the_physical_engine() {
        let original = Scratch::default();
        let mut old_pulse_edit = original.clone();
        old_pulse_edit.standalone.calibration.pulse_ms = 4.8;
        old_pulse_edit.standalone.calibration.exhaust_level = 0.01;
        let params = Parameters {
            rpm: 2000.,
            load: 0.5,
            ..Default::default()
        };
        let a = render(&mut make(&original, params, Settings::default()), 12_000);
        let b = render(
            &mut make(&old_pulse_edit, params, Settings::default()),
            12_000,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn retired_voice_projects_migrate_to_the_only_physical_renderer() {
        let mut scratch = Scratch::default();
        scratch.design = bess::scratch::PRESETS[10].1;
        scratch.sound.bass_db = 3.;
        scratch.apply_design();
        let mut old = serde_json::to_value(&scratch).unwrap();
        old["engine"] = "experimental".into();
        let migrated: Scratch = serde_json::from_value(old).unwrap();
        assert_eq!(migrated, scratch);
        let model = ScratchModel::build(&migrated, 48_000).unwrap();
        assert!(model.physical.is_some());
        assert_eq!(
            serde_json::to_value(&migrated).unwrap()["engine"],
            "standalone"
        );
        let params = Parameters {
            rpm: 2000.,
            load: 0.4,
            volume: 0.1,
            ..Default::default()
        };
        assert_eq!(
            render(&mut make(&migrated, params, Settings::default()), 12000),
            render(&mut make(&scratch, params, Settings::default()), 12000)
        );
    }

    #[test]
    fn physical_volume_and_engine_layer_gain_can_really_mute_their_outputs() {
        let scratch = Scratch::default();
        let muted = render(
            &mut make(
                &scratch,
                Parameters {
                    volume: 0.,
                    ..Default::default()
                },
                Settings::default(),
            ),
            12_000,
        );
        assert!(muted.iter().all(|&x| x == 0.));
        let params = Parameters {
            rpm: 2500.,
            load: 0.5,
            exhaust: 0.,
            intake: 1.,
            mechanical: 1.,
            volume: 0.3,
            ..Default::default()
        };
        let layer = render(&mut make(&scratch, params, Settings::default()), 12_000);
        let no_layer = render(
            &mut make(
                &scratch,
                params,
                Settings {
                    engine_gain: 0.,
                    ..Default::default()
                },
            ),
            12_000,
        );
        assert!(layer.iter().any(|x| x.abs() > 1e-5));
        assert!(no_layer.iter().all(|&x| x == 0.));
    }

    #[test]
    fn physical_rebuild_returns_the_outgoing_engine_after_its_fade() {
        let mut scratch = Scratch::default();
        let params = Parameters {
            rpm: 2000.,
            load: 0.4,
            volume: 0.1,
            ..Default::default()
        };
        let mut bench = make(&scratch, params, Settings::default());
        render(&mut bench, 12_000);
        scratch.build.compression += 0.5;
        assert!(
            bench
                .swap_scratch(ScratchModel::build(&scratch, 48_000).unwrap())
                .is_none()
        );
        assert!(bench.take_retired().is_none());
        render(&mut bench, 2000);
        assert!(matches!(
            bench.take_retired(),
            Some(ScratchVoice::Physical(_))
        ));
    }

    #[test]
    fn physical_simulated_mode_reports_the_unclamped_shaft() {
        let scratch = Scratch::default();
        let model = ScratchModel::build(&scratch, 48_000).unwrap();
        let controls = Controls {
            mode: Mode::Simulated,
            gear: 0,
            automatic: false,
            throttle: 0.,
            ..Default::default()
        };
        let mut bench = Bench::from_scratch(
            48_000,
            Parameters::default(),
            Settings::default(),
            controls,
            model,
        );
        render(&mut bench, 24_000);
        let rpm = bench.state().rpm;
        assert!(rpm.is_finite() && rpm >= 0.);
        assert_ne!(
            rpm, scratch.idle_rpm,
            "simulated RPM must come from shaft integration"
        );
        assert_eq!(bench.state().speed_kmh, 0.);
    }

    #[test]
    fn accessory_load_is_a_live_crank_torque_step_not_an_rpm_reset() {
        let scratch = Scratch::default();
        let controls = Controls {
            mode: Mode::Simulated,
            gear: 0,
            automatic: false,
            ..Default::default()
        };
        let params = Parameters {
            volume: 0.,
            ..Default::default()
        };
        let make = || {
            Bench::from_scratch(
                48_000,
                params,
                Settings::default(),
                controls,
                ScratchModel::build(&scratch, 48_000).unwrap(),
            )
        };
        let mut baseline = make();
        render(&mut baseline, 4800);
        let before = baseline.state().rpm;
        baseline.next(true);
        let unloaded = baseline.state().rpm;
        for (ac, steering, torque) in [(true, false, 16.), (false, true, 22.), (true, true, 38.)] {
            let mut loaded = make();
            render(&mut loaded, 4800);
            assert_eq!(loaded.state().rpm, before);
            loaded.set(
                params,
                Settings {
                    accessory_ac: ac,
                    accessory_steering: steering,
                    ..Default::default()
                },
                controls,
                0,
            );
            assert_eq!(
                loaded.state().rpm,
                before,
                "changing loads must not reset the engine"
            );
            loaded.next(true);
            let rpm_drop = unloaded - loaded.state().rpm;
            let expected = torque / scratch.inertia / 48_000. * 60. / std::f32::consts::TAU;
            assert!(
                (rpm_drop - expected).abs() < 0.001,
                "load {torque} Nm: {rpm_drop} rpm vs {expected}"
            );
            assert!(!loaded.failed());
        }
    }

    #[test]
    fn accessory_settings_persist_and_old_projects_default_to_no_load() {
        let defaults: Settings = serde_json::from_str("{}").unwrap();
        assert!(!defaults.accessory_ac && !defaults.accessory_steering);
        let selected = Settings {
            accessory_ac: true,
            accessory_steering: true,
            ..Default::default()
        };
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&selected).unwrap()).unwrap();
        assert_eq!(selected, restored);
    }

    #[test]
    fn physical_starter_is_momentary_and_never_restored_from_projects() {
        let settings = Settings {
            starter: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("starter"));
        let restored: Settings = serde_json::from_str(&json).unwrap();
        assert!(!restored.starter);
        let untrusted_saved_state: Settings = serde_json::from_str(r#"{"starter":true}"#).unwrap();
        assert!(!untrusted_saved_state.starter);
    }

    #[test]
    fn simulated_starter_turns_a_stopped_shaft_without_resetting_to_idle() {
        let scratch = Scratch::default();
        let controls = Controls {
            mode: Mode::Simulated,
            gear: 0,
            automatic: false,
            throttle: 0.,
            ..Default::default()
        };
        let params = Parameters::default();
        let make_stopped = || {
            let mut model = ScratchModel::build(&scratch, 48_000).unwrap();
            model
                .physical
                .as_mut()
                .unwrap()
                .next(bess::physical::engine::Commands {
                    imposed_rpm: Some(0.),
                    throttle: 0.,
                    ..Default::default()
                });
            Bench::from_scratch(48_000, params, Settings::default(), controls, model)
        };
        let mut baseline = make_stopped();
        let mut cranking = make_stopped();
        render(&mut baseline, 48);
        render(&mut cranking, 48);
        let before = cranking.state().rpm;
        cranking.set(
            params,
            Settings {
                starter: true,
                ..Default::default()
            },
            controls,
            0,
        );
        assert_eq!(
            cranking.state().rpm,
            before,
            "starter is a torque command, not a reset"
        );
        render(&mut baseline, 96);
        render(&mut cranking, 96);
        assert!(cranking.state().rpm > baseline.state().rpm + 0.1);
        assert!(
            cranking.state().rpm < 100.,
            "starter must not jump to idle RPM"
        );
    }
}
