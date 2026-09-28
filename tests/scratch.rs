// Historical event-voice A/reference regressions. Never selected by the GUI.
mod legacy_reference {
    use bess::{
        bench::Bench,
        drive::{Controls, Mode},
        engine_build::{Headers, Muffler},
        hybrid::Settings,
        project::{self, Parameters, Project},
        scratch::{EngineDesign, Layout, PRESETS, Scratch, ScratchEngine, ScratchModel},
    };

    #[test]
    fn any_size_renders_and_open_exhausts_stay_under_full_scale() {
        use bess::engine_build::Catalyst;
        for (bore, stroke) in [(400., 300.), (5., 3.), (86., 86.)] {
            let (scratch, settings, mut params) = derived(&|s| {
                s.design = PRESETS[13].1;
                s.build.bore_mm = bore;
                s.build.stroke_mm = stroke;
                s.build.muffler = Muffler::None;
                s.build.catalyst = Catalyst::None;
            });
            scratch.validate().unwrap();
            params.volume = 1.;
            params.rpm = scratch.idle_rpm;
            let cycle = Controls {
                mode: Mode::Cycle,
                ..Default::default()
            };
            let model = ScratchModel::build_reference(&scratch, 48_000).unwrap();
            let mut bench = Bench::from_scratch(48_000, params, settings, cycle, model);
            bench.set(params, settings, cycle, 0);
            bench.set_beamng_camera(bess::bench::BeamNgCamera::Tailpipe);
            let x = render(&mut bench, 48_000 * 16);
            let peak = x.iter().fold(0f32, |a, v| a.max(v.abs()));
            assert!(x.iter().all(|v| v.is_finite()), "{bore}×{stroke}");
            assert!(
                peak <= 0.892,
                "{bore}×{stroke} mm peaks at {:.2} dBFS",
                20. * peak.log10()
            );
            assert!(peak > 0.1, "{bore}×{stroke} mm is nearly silent");
        }
    }

    fn bench(scratch: &Scratch, params: Parameters) -> Bench {
        let model = ScratchModel::build_reference(scratch, 48_000).unwrap();
        let direct = Controls {
            mode: Mode::Direct,
            ..Default::default()
        };
        let mut bench = Bench::from_scratch(48_000, params, Settings::default(), direct, model);
        bench.set(params, Settings::default(), direct, 0);
        bench
    }

    fn render(bench: &mut Bench, frames: usize) -> Vec<f32> {
        (0..frames).map(|_| bench.next(true)).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn stock_scratch_idle_is_audible_from_every_listening_position() {
        use bess::bench::BeamNgCamera;
        let voice = ScratchEngine::Standalone;
        let (scratch, settings, mut p) = derived(&|s| s.engine = voice);
        p.rpm = scratch.idle_rpm;
        p.load = 0.05;
        p.volume = 0.8;
        for camera in [
            BeamNgCamera::Orbit,
            BeamNgCamera::Tailpipe,
            BeamNgCamera::Cockpit,
            BeamNgCamera::Hood,
        ] {
            let mut bench = bench_with(&scratch, settings, p);
            bench.set_beamng_camera(camera);
            let x = render(&mut bench, 144_000);
            let db = 20. * rms(&x[48_000..]).log10();
            assert!(
                db > -39.,
                "{voice:?} / {camera:?}: idle too quiet at {db:.1} dBFS RMS"
            );
            assert!(x.iter().all(|s| s.is_finite() && s.abs() <= 0.892));
        }
    }

    #[test]
    fn scratch_listening_volume_reaches_unity_and_zero() {
        let (scratch, settings, mut p) = derived(&|_| {});
        p.rpm = scratch.idle_rpm;
        p.load = 0.05;
        let mut levels = Vec::new();
        for volume in [0., 0.5, 0.8, 1.] {
            p.volume = volume;
            p.validate().unwrap();
            let x = render(&mut bench_with(&scratch, settings, p), 96_000);
            levels.push(rms(&x[48_000..]));
        }
        assert_eq!(levels[0], 0.);
        assert!((levels[3] / levels[2] - 1.25).abs() < 0.01);
        assert!((levels[3] / levels[1] - 2.).abs() < 0.01);
    }

    fn params(rpm: f32, load: f32) -> Parameters {
        Parameters {
            rpm,
            load,
            volume: 0.6,
            ..Parameters::default()
        }
    }

    #[test]
    fn scratch_events_are_audible_bounded_and_follow_load() {
        let scratch = Scratch::default();
        let off = render(&mut bench(&scratch, params(3000., 0.05)), 96_000);
        let full = render(&mut bench(&scratch, params(3000., 1.)), 96_000);
        for x in off.iter().chain(&full) {
            assert!(x.is_finite() && x.abs() <= 0.999);
        }
        let (off, full) = (rms(&off[48_000..]), rms(&full[48_000..]));
        assert!(off > 1e-3, "off-load is silent: {off}");
        assert!(full > off * 1.2, "load rise not audible: {off} → {full}");
    }

    #[test]
    fn shorter_event_pulses_move_energy_to_upper_bands() {
        let high_share = |pulse_ms: f32| {
            // An open exhaust: a baffled muffler rightly masks the top end.
            let mut scratch = Scratch::default();
            scratch.build.muffler = Muffler::None;
            scratch.build.catalyst = bess::engine_build::Catalyst::None;
            scratch.standalone.calibration.pulse_ms = pulse_ms;
            let x = render(
                &mut bench(
                    &scratch,
                    Parameters {
                        brightness: 10_000.,
                        ..params(2500., 0.6)
                    },
                ),
                96_000,
            );
            let x = &x[48_000..];
            // Upper-mid (1–3 kHz) against low (100–300 Hz) band energy, in dB.
            let band = |lo: f32, hi: f32| {
                (0..16)
                    .map(|i| tone(x, lo * (hi / lo).powf(i as f32 / 15.)).powi(2))
                    .sum::<f32>()
            };
            10. * (band(1000., 3000.) / band(100., 300.)).log10()
        };
        let (dark, bright) = (high_share(5.), high_share(0.5));
        assert!(bright > dark + 6., "{dark:.1} → {bright:.1} dB");
    }

    #[test]
    fn standalone_scratch_plays_through_the_bench() {
        let scratch = Scratch {
            engine: ScratchEngine::Standalone,
            ..Scratch::default()
        };
        let x = render(&mut bench(&scratch, params(3000., 0.7)), 48_000);
        assert!(x.iter().all(|s| s.is_finite() && s.abs() <= 0.98));
        assert!(rms(&x[24_000..]) > 1e-3);
    }

    #[test]
    fn live_rebuilds_swap_without_a_click() {
        let engine = ScratchEngine::Standalone;
        let mut scratch = Scratch {
            engine,
            ..Scratch::default()
        };
        let mut bench = bench(&scratch, params(2000., 0.5));
        let before = render(&mut bench, 48_000);
        let max_step = |x: &[f32]| x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0., f32::max);
        scratch.experimental.brightness = 0.8;
        scratch.standalone.banks[0].exhaust_length_m = 2.4;
        let displaced =
            bench.swap_scratch(ScratchModel::build_reference(&scratch, 48_000).unwrap());
        let after = render(&mut bench, 4_800);
        assert!(
            max_step(&after) < max_step(&before) * 2.,
            "{engine:?} swap stepped {} vs {}",
            max_step(&after),
            max_step(&before)
        );
        // The texture grid returns at once; the event synth after its fade.
        let retired = displaced.is_some() || bench.take_retired().is_some();
        assert!(retired, "{engine:?} kept the old voice on the audio thread");
    }

    #[test]
    fn invalid_scratch_is_rejected_and_project_round_trips() {
        let mut bad = Scratch::default();
        bad.redline_rpm = bad.idle_rpm + 10.;
        assert!(ScratchModel::build_reference(&bad, 48_000).is_err());
        bad = Scratch::default();
        bad.experimental.tone_db[1][3] = 40.;
        assert!(bad.validate().is_err());

        let mut scratch = Scratch::default();
        scratch.design.set_cylinders(6);
        scratch.apply_design();
        assert_eq!(scratch.standalone.cylinders.len(), 6);
        scratch.validate().unwrap();
        scratch.experimental.noise_db[0][5] = -6.;
        let path = std::env::temp_dir().join(format!("bess-scratch-{}.json", std::process::id()));
        let p = Project {
            version: 3,
            parameters: Parameters::default(),
            hybrid: Settings::default(),
            source: None,
            driving: Controls::default(),
            profile_name: project::default_profile_name(),
            scratch: Some(scratch.clone()),
        };
        project::save_project(&path, &p).unwrap();
        assert_eq!(project::load_project(&path).unwrap().scratch, Some(scratch));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_descriptor_projects_migrate_to_events_and_save_the_new_voice() {
        let mut scratch = Scratch::default();
        assert_eq!(scratch.engine, ScratchEngine::Standalone);
        scratch.design = preset("V6 90° odd-fire");
        scratch.build.bore_mm = 91.;
        scratch.apply_design();
        scratch.standalone.calibration.pulse_ms = 2.3;
        let expected = Project {
            version: 3,
            parameters: Parameters {
                cylinders: 6,
                volume: 1.,
                ..params(1800., 0.4)
            },
            hybrid: Settings::default(),
            source: None,
            driving: Controls::default(),
            profile_name: project::default_profile_name(),
            scratch: Some(scratch.clone()),
        };
        let mut legacy = serde_json::to_value(&expected).unwrap();
        legacy["scratch"]["engine"] = "experimental".into();
        let path =
            std::env::temp_dir().join(format!("bess-legacy-voice-{}.json", std::process::id()));
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let migrated = project::load_project(&path).unwrap();
        assert_eq!(migrated.parameters, expected.parameters);
        assert_eq!(migrated.scratch.as_ref(), Some(&scratch));
        let actual = render(
            &mut bench(migrated.scratch.as_ref().unwrap(), migrated.parameters),
            48_000,
        );
        let reference = render(&mut bench(&scratch, expected.parameters), 48_000);
        assert_eq!(actual, reference);
        project::save_project(&path, &migrated).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["scratch"]["engine"], "standalone");
        legacy["scratch"]["engine"] = "unknown_voice".into();
        assert!(serde_json::from_value::<Project>(legacy).is_err());
        std::fs::remove_file(path).unwrap();
    }

    fn intervals(design: &EngineDesign) -> Vec<f32> {
        let firing = design.firing();
        let mut angles = firing.angles[..firing.events].to_vec();
        angles.sort_by(f32::total_cmp);
        let mut gaps: Vec<f32> = angles.windows(2).map(|w| w[1] - w[0]).collect();
        gaps.push(angles[0] + 720. - angles[angles.len() - 1]);
        gaps.iter().map(|g| g.round()).collect()
    }

    fn preset(name: &str) -> EngineDesign {
        PRESETS.iter().find(|(n, _)| *n == name).unwrap().1
    }

    #[test]
    fn engine_designs_resolve_to_known_firing_intervals() {
        for (name, design) in PRESETS {
            design.validate().unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(intervals(&design).iter().sum::<f32>(), 720., "{name}");
        }
        assert_eq!(intervals(&preset("Inline-4")), [180.; 4]);
        assert_eq!(intervals(&preset("V-twin 45°")), [405., 315.]);
        assert_eq!(
            intervals(&preset("V6 90° odd-fire")),
            [90., 150., 90., 150., 90., 150.]
        );
        // Cross-plane: even timing, but each bank fires unevenly (the burble).
        let cross = preset("V8 cross-plane").firing();
        let bank_sequence: Vec<u8> = {
            let mut k: Vec<usize> = (0..cross.events).collect();
            k.sort_by(|&a, &b| cross.angles[a].total_cmp(&cross.angles[b]));
            k.iter()
                .map(|&e| cross.banks[cross.cylinder[e] as usize])
                .collect()
        };
        assert_ne!(bank_sequence, [0, 1, 0, 1, 0, 1, 0, 1]);
        let flat = preset("V8 flat-plane").firing();
        assert!((0..8).all(|i| flat.banks[i] == (i % 2) as u8));
        // Same even timing; only the per-bank header offset tells them apart.
        let voice = |name: &str| {
            let design = preset(name);
            let mut scratch = Scratch {
                design,
                ..Scratch::default()
            };
            scratch.apply_design();
            let x = render(
                &mut bench(
                    &scratch,
                    Parameters {
                        cylinders: 8,
                        ..params(1500., 0.6)
                    },
                ),
                48_000,
            );
            x[24_000..].to_vec()
        };
        let (cross, flat) = (voice("V8 cross-plane"), voice("V8 flat-plane"));
        let diff: Vec<f32> = cross.iter().zip(&flat).map(|(a, b)| a - b).collect();
        assert!(
            rms(&diff) > rms(&cross) * 0.3,
            "cross- and flat-plane V8 sound the same"
        );

        let mut bad = preset("Inline-4");
        bad.firing_order[..4].copy_from_slice(&[1, 3, 9, 2]);
        assert!(bad.validate().is_err(), "cylinder 9 does not exist");
        bad = preset("Inline-4");
        bad.layout = Layout::V;
        bad.set_cylinders(1);
        assert_eq!(bad.layout, Layout::Inline);
    }

    #[test]
    fn firing_design_changes_the_event_voice() {
        let engine = ScratchEngine::Standalone;
        let render_design = |design: EngineDesign| {
            let mut scratch = Scratch {
                engine,
                design,
                ..Scratch::default()
            };
            scratch.apply_design();
            let x = render(
                &mut bench(
                    &scratch,
                    Parameters {
                        cylinders: design.cylinders,
                        ..params(1500., 0.5)
                    },
                ),
                48_000,
            );
            assert!(x.iter().all(|s| s.is_finite()));
            x[24_000..].to_vec()
        };
        let even = render_design(preset("V6 60°"));
        let odd = render_design(preset("V6 90° odd-fire"));
        let diff: Vec<f32> = even.iter().zip(&odd).map(|(a, b)| a - b).collect();
        assert!(
            rms(&diff) > rms(&even) * 0.3,
            "{engine:?}: odd-fire sounds the same"
        );
        assert!(rms(&odd) > 1e-3);
    }

    #[test]
    fn engine_parts_drive_the_sound_design() {
        let derive = |edit: &dyn Fn(&mut Scratch)| {
            let mut scratch = Scratch::default();
            edit(&mut scratch);
            let (mut settings, mut params, mut controls) = (
                Settings::default(),
                Parameters::default(),
                Controls::default(),
            );
            scratch.derive_from_build(&mut settings, &mut params, &mut controls);
            scratch.validate().unwrap();
            (scratch, settings, controls)
        };
        let (open, open_settings, _) = derive(&|s| s.build.muffler = Muffler::None);
        let (quiet, quiet_settings, _) = derive(&|s| s.build.muffler = Muffler::ReverseFlow);
        assert!(open_settings.absorption < quiet_settings.absorption);
        // The exhaust network, not a level knob, makes an open system louder.
        let loudness = |scratch: &Scratch, settings: Settings| {
            let p = Parameters {
                rpm: 4000.,
                load: 0.9,
                volume: 0.6,
                brightness: 10_000.,
                ..Parameters::default()
            };
            let x = render(&mut bench_with(scratch, settings, p), 48_000 * 2);
            20. * rms(&x[48_000..]).log10()
        };
        let (open_db, quiet_db) = (
            loudness(&open, open_settings),
            loudness(&quiet, quiet_settings),
        );
        assert!(
            open_db > quiet_db + 6.,
            "open {open_db:.1} dB vs reverse-flow {quiet_db:.1} dB"
        );
        let (race, race_settings, race_controls) = derive(&|s| s.build.cam = 1.);
        let (mild, mild_settings, mild_controls) = derive(&|s| s.build.cam = 0.);
        assert!(race.experimental.variation > mild.experimental.variation);
        assert!(race_settings.roughness > mild_settings.roughness && race.idle_rpm > mild.idle_rpm);
        assert!(race_controls.peak_torque_nm > mild_controls.peak_torque_nm);
        let (equal, _, _) = derive(&|s| {
            s.design = PRESETS[4].1;
            s.build.headers = Headers::EqualLength;
        });
        assert_eq!(equal.design.bank_delay_ms, 0.);
        assert_eq!(equal.standalone.cylinders.len(), 4);
    }

    /// Magnitude of `freq` in `x` (Goertzel), for order analysis.
    fn tone(x: &[f32], freq: f32) -> f32 {
        let w = 2. * std::f32::consts::PI * freq / 48_000.;
        let (mut s1, mut s2) = (0f32, 0f32);
        for &v in x {
            let s0 = v + 2. * w.cos() * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - 2. * w.cos() * s1 * s2).sqrt() / x.len() as f32
    }

    fn derived(edit: &dyn Fn(&mut Scratch)) -> (Scratch, Settings, Parameters) {
        let mut scratch = Scratch::default();
        edit(&mut scratch);
        let (mut settings, mut params, mut controls) = (
            Settings::default(),
            Parameters::default(),
            Controls::default(),
        );
        scratch.derive_from_build(&mut settings, &mut params, &mut controls);
        params.volume = 0.6;
        (scratch, settings, params)
    }

    fn bench_with(scratch: &Scratch, settings: Settings, params: Parameters) -> Bench {
        let direct = Controls {
            mode: Mode::Direct,
            ..Default::default()
        };
        let mut bench = Bench::from_scratch(
            48_000,
            params,
            settings,
            direct,
            ScratchModel::build_reference(scratch, 48_000).unwrap(),
        );
        bench.set(params, settings, direct, 0);
        bench
    }

    #[test]
    fn fuel_cut_changes_source_and_drops_level() {
        let (scratch, settings, mut params) = derived(&|_| {});
        params.rpm = 4000.;
        params.load = 0.8;
        let mut bench = bench_with(&scratch, settings, params);
        let firing = render(&mut bench, 48_000 * 2);
        let direct = Controls {
            mode: Mode::Direct,
            ..Default::default()
        };
        bench.set(Parameters { load: 0., ..params }, settings, direct, 0);
        let coasting = render(&mut bench, 48_000 * 4);
        let (fire, coast) = (rms(&firing[48_000..]), rms(&coasting[48_000 * 3..]));
        let drop = 20. * (coast / fire).log10();
        eprintln!("overrun drop {drop:.1} dB");
        assert!(drop < -8., "overrun only {drop:.1} dB below firing");
        assert!(coast > 1e-4, "overrun went silent");
    }

    #[test]
    fn direct_injection_ticks_at_idle() {
        let high = |di: bool| {
            let (scratch, settings, mut params) = derived(&|s| {
                s.build.fuel = if di {
                    bess::engine_build::Fuel::DirectInjection
                } else {
                    bess::engine_build::Fuel::PortInjection
                };
            });
            params.rpm = scratch.idle_rpm;
            params.load = 0.05;
            let x = render(&mut bench_with(&scratch, settings, params), 48_000 * 2);
            let x = &x[48_000..];
            // Injector clicks sit above 5 kHz.
            (0..16)
                .map(|i| tone(x, 6000. + 250. * i as f32).powi(2))
                .sum::<f32>()
                / rms(x).powi(2)
        };
        let (di, port) = (high(true), high(false));
        assert!(di > port * 2., "{di:e} vs {port:e}");
    }

    /// 20 ms RMS envelope (dB) of a lift-off from 4000 rpm, and the largest
    /// 1 ms peak over the local 20 ms RMS after it.
    fn lift_off(afterfire: f32) -> (Vec<f32>, f32) {
        let (mut scratch, settings, mut params) = derived(&|_| {});
        scratch.experimental.afterfire = afterfire;
        params.rpm = 4000.;
        params.load = 0.8;
        let mut bench = bench_with(&scratch, settings, params);
        let mut x = render(&mut bench, 48_000);
        let direct = Controls {
            mode: Mode::Direct,
            ..Default::default()
        };
        bench.set(Parameters { load: 0., ..params }, settings, direct, 0);
        x.extend(render(&mut bench, 48_000 * 3));
        let envelope = x
            .chunks(960)
            .map(|w| 20. * rms(w).max(1e-9).log10())
            .collect();
        let salience = (1000..4000)
            .map(|m| {
                let local = rms(&x[(m - 10) * 48..(m + 10).min(3999) * 48]);
                x[m * 48..(m + 1) * 48]
                    .iter()
                    .fold(0f32, |a, v| a.max(v.abs()))
                    / local
            })
            .fold(0f32, f32::max);
        (envelope, salience)
    }

    #[test]
    fn lift_off_fades_without_steps_and_pops_only_when_asked() {
        let (envelope, calm) = lift_off(0.);
        // Exhaust pulse trains alone reach crest factors of 4–5.
        assert!(
            calm < 6.,
            "default build has a {calm:.1}× transient on lift-off"
        );
        let worst = envelope[50..]
            .windows(2)
            .map(|w| w[0] - w[1])
            .fold(0f32, f32::max);
        // Lift-off is fast by design (manifold empties in ~3 revolutions), but
        // never a hard step.
        assert!(worst < 8., "level steps down {worst:.1} dB in 20 ms");
        let settled = envelope[envelope.len() - 10..].iter().sum::<f32>() / 10.;
        assert!(
            settled < envelope[40] - 6.,
            "overrun barely quieter: {settled:.1} vs {:.1}",
            envelope[40]
        );
        // Pops last several ms: look for 20 ms bursts above the calm lift-off.
        let (popping, _) = lift_off(1.);
        let loudest = |e: &[f32]| e[60..].iter().copied().fold(f32::MIN, f32::max);
        let gain = loudest(&popping) - loudest(&envelope);
        assert!(gain > 4., "afterfire 100 % adds only {gain:.1} dB bursts");
    }

    #[test]
    fn cabin_listening_is_darker_than_the_tailpipe() {
        let tilt = |camera: bess::bench::BeamNgCamera| {
            let (scratch, settings, mut params) = derived(&|_| {});
            params.rpm = 3000.;
            params.load = 0.7;
            let mut bench = bench_with(&scratch, settings, params);
            bench.set_beamng_camera(camera);
            let x = render(&mut bench, 48_000 * 2);
            let x = &x[48_000..];
            let band = |lo: f32, hi: f32| {
                (0..16)
                    .map(|i| tone(x, lo * (hi / lo).powf(i as f32 / 15.)).powi(2))
                    .sum::<f32>()
            };
            10. * (band(1000., 3000.) / band(100., 300.)).log10()
        };
        let (cabin, tailpipe) = (
            tilt(bess::bench::BeamNgCamera::Cockpit),
            tilt(bess::bench::BeamNgCamera::Tailpipe),
        );
        assert!(
            cabin < tailpipe - 10.,
            "cabin {cabin:.1} dB vs tailpipe {tailpipe:.1} dB"
        );
    }

    #[test]
    fn turbo_blow_off_vents_on_lift_off() {
        use bess::engine_build::{Aspiration, BlowOff};
        let vent = |blow_off: BlowOff| {
            let (scratch, settings, mut params) = derived(&|s| {
                s.build.aspiration = Aspiration::Turbo;
                s.build.blow_off = blow_off;
            });
            params.rpm = 4500.;
            params.load = 1.;
            let mut bench = bench_with(&scratch, settings, params);
            render(&mut bench, 48_000 * 2);
            let direct = Controls {
                mode: Mode::Direct,
                ..Default::default()
            };
            bench.set(Parameters { load: 0., ..params }, settings, direct, 0);
            let x = render(&mut bench, 24_000);
            (0..12)
                .map(|i| tone(&x, 2000. + 150. * i as f32).powi(2))
                .sum::<f32>()
        };
        let (open, quiet) = (vent(BlowOff::Atmospheric), vent(BlowOff::Recirculating));
        assert!(
            open > quiet * 2.,
            "atmospheric valve not audible: {open:e} vs {quiet:e}"
        );
    }

    #[test]
    fn nonsense_engines_are_allowed_and_still_render() {
        let engine = ScratchEngine::Standalone;
        let mut scratch = Scratch {
            engine,
            ..Scratch::default()
        };
        // Cylinder 1 fires twice, 3 and 4 never; all pins at 0°, two banks.
        scratch.design.pins = [0.; 12];
        scratch.design.banks[1] = 1;
        scratch.design.order_len = 3;
        scratch.design.firing_order[..3].copy_from_slice(&[1, 1, 2]);
        scratch.design.validate().unwrap();
        scratch.apply_design();
        scratch.validate().unwrap();
        assert_eq!(scratch.standalone.cylinders.len(), 3);
        let x = render(&mut bench(&scratch, params(2000., 0.5)), 48_000);
        assert!(
            x.iter().all(|v| v.is_finite()) && rms(&x[24_000..]) > 1e-4,
            "{engine:?}"
        );
    }
}

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
