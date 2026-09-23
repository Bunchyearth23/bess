use bess::{
    bank::Bank,
    hybrid::{Hybrid, Settings},
    project::{self, Parameters, Project},
    render,
};
use std::{
    io::{Cursor, Write},
    sync::{Arc, OnceLock},
};

thread_local! {
    static TRACK:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};
    static ALLOCATIONS:std::cell::Cell<usize>=const {std::cell::Cell::new(0)};
}
struct CountingAllocator;
// SAFETY: all memory operations are forwarded unchanged to System. Tracking
// accesses only thread-local integer cells and never touches the allocation.
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if TRACK.try_with(|v| v.get()).unwrap_or(false) {
            let _ = ALLOCATIONS.try_with(|v| v.set(v.get() + 1));
        }
        // SAFETY: layout is passed through from the GlobalAlloc caller unchanged.
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: pointer and layout belong to System, the sole allocating backend.
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn fixture() -> Arc<Bank> {
    static BANK: OnceLock<Arc<Bank>> = OnceLock::new();
    BANK.get_or_init(|| build_fixture(false)).clone()
}
fn textured_fixture() -> Arc<Bank> {
    static BANK: OnceLock<Arc<Bank>> = OnceLock::new();
    BANK.get_or_init(|| build_fixture(true)).clone()
}
fn build_fixture(textured: bool) -> Arc<Bank> {
    let path = std::env::temp_dir().join(format!(
        "bess-fixture-{textured}-{}.zip",
        std::process::id()
    ));
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut layers = [Vec::new(), Vec::new()];
    for (layer, list) in layers.iter_mut().enumerate() {
        for rpm in [800, 1600, 4000] {
            let name = format!("art/sound/{layer}-{rpm}.wav");
            let mut cursor = Cursor::new(Vec::new());
            {
                let mut wav = hound::WavWriter::new(
                    &mut cursor,
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: 32000,
                        bits_per_sample: 32,
                        sample_format: hound::SampleFormat::Float,
                    },
                )
                .unwrap();
                let mut seed = 0xB355_7200_u32;
                for i in 0..64000 {
                    let phase = std::f32::consts::TAU * i as f32 / 32000. * rpm as f32 / 120.;
                    let mut sample = 0.08 * (phase * 4. + 0.2 * layer as f32).sin()
                        + 0.03 * (phase * 12.).sin()
                        + 0.012 * (phase * 33.).sin();
                    if textured {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        sample += (seed as f32 / u32::MAX as f32 * 2. - 1.) * 0.012;
                    }
                    wav.write_sample(sample * (0.6 + 0.4 * layer as f32))
                        .unwrap();
                }
                wav.finalize().unwrap();
            }
            zip.start_file(&name, options).unwrap();
            zip.write_all(&cursor.into_inner()).unwrap();
            list.push(serde_json::json!([name, rpm]));
        }
    }
    zip.start_file("art/sound/blends/test.sfxBlend2D.json", options)
        .unwrap();
    zip.write_all(
        serde_json::to_string(&serde_json::json!({"samples":layers}))
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    zip.start_file("vehicles/test/info.json", options).unwrap();
    zip.write_all(b"{\"Name\":\"Test Vehicle\",\"paints\":{\"Blue\":1,\"Blue\":2}}")
        .unwrap();
    zip.finish().unwrap();
    let bank = Arc::new(Bank::load(&path, None).unwrap());
    // Keep the source for the replacement-mod roundtrip test.
    bank
}
fn params() -> Parameters {
    Parameters {
        rpm: 800.,
        brightness: 10000.,
        exhaust: 1.,
        intake: 0.6,
        mechanical: 0.3,
        ..Default::default()
    }
}
fn rms_diff(a: &[f32], b: &[f32]) -> f64 {
    (a.iter()
        .zip(b)
        .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
        .sum::<f64>()
        / a.len() as f64)
        .sqrt()
}

#[test]
fn reconstructed_engine_stem_is_distinct_and_preserves_audition_mix() {
    let bank = fixture();
    let p = params();
    let h = Settings::default();
    let mut normal = Hybrid::new(48000, p, h, Some(bank.clone()));
    let mut split = Hybrid::new(48000, p, h, Some(bank.clone()));
    let mut engine_energy = 0.;
    let mut exhaust_energy = 0.;
    for i in 0..48000 {
        let sample = normal.next(true);
        let stems = split.next_stems(true);
        assert_eq!(sample, stems.mixed);
        assert!(stems.exhaust.is_finite() && stems.engine.is_finite());
        if i >= 24000 {
            engine_energy += stems.engine * stems.engine;
            exhaust_energy += stems.exhaust * stems.exhaust;
        }
    }
    // A strictly periodic recording has no independent intake or mechanical
    // detail to reconstruct. BESS should not invent a broadband layer for it.
    assert!(
        engine_energy < 1e-6,
        "Periodic exhaust copied into the engine stem: {engine_energy}"
    );
    assert!(
        engine_energy < exhaust_energy,
        "The companion stem is an exhaust copy"
    );

    let mut source_only = p;
    source_only.intake = 0.;
    source_only.mechanical = 0.;
    let mut split = Hybrid::new(48000, source_only, h, Some(bank));
    for _ in 0..48000 {
        assert_eq!(split.next_stems(true).engine, 0.);
    }
}

#[test]
fn source_reference_retains_original_phase_across_listening_volumes() {
    let bank = fixture();
    let source_settings = Settings {
        enhanced: false,
        ..Settings::default()
    };
    let mut source = Hybrid::new(
        48_000,
        Parameters {
            volume: 0.25,
            ..params()
        },
        source_settings,
        Some(bank.clone()),
    );
    let mut enhanced = Hybrid::new(
        48_000,
        Parameters {
            volume: 0.8,
            ..params()
        },
        Settings::default(),
        Some(bank),
    );
    for _ in 0..48_000 {
        source.next_stems(true);
        enhanced.next_stems(true);
    }
    let mut reference_energy = 0.;
    let mut source_ratio: Option<f32> = None;
    for _ in 0..12_000 {
        let a = source.next_stems(true);
        let b = enhanced.next_stems(true);
        assert_eq!(a.mixed, a.exhaust);
        assert!(
            (a.source_reference / 0.25 - b.source_reference / 0.8).abs() < 2e-5,
            "The source reference followed listening volume or B sound controls"
        );
        reference_energy += a.source_reference * a.source_reference;
        if a.exhaust.abs() > 1e-4 {
            let ratio = a.source_reference / a.exhaust;
            if let Some(expected) = source_ratio {
                assert!((ratio - expected).abs() < 1e-4, "A reference changed phase");
            } else {
                source_ratio = Some(ratio);
            }
        }
    }
    assert!(reference_energy > 1e-5);
}

#[test]
fn recorded_irregular_texture_drives_engine_without_copying_exhaust_orders() {
    let bank = textured_fixture();
    let p = Parameters {
        rpm: 1600.,
        load: 1.,
        intake: 0.6,
        mechanical: 0.,
        exhaust: 1.,
        ..params()
    };
    let mut synth = Hybrid::new(48000, p, Settings::default(), Some(bank));
    for _ in 0..48000 {
        synth.next_stems(true);
    }
    let period = 48000 * 120 / 1600;
    let cycles = 12;
    let mut exhaust = Vec::with_capacity(period * cycles);
    let mut engine = Vec::with_capacity(period * cycles);
    for _ in 0..period * cycles {
        let stems = synth.next_stems(true);
        exhaust.push(stems.exhaust);
        engine.push(stems.engine);
    }
    let power = |audio: &[f32]| -> f64 {
        audio.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / audio.len() as f64
    };
    let coherent_share = |audio: &[f32]| -> f64 {
        let template: Vec<_> = (0..period)
            .map(|phase| {
                (0..cycles)
                    .map(|cycle| audio[cycle * period + phase] as f64)
                    .sum::<f64>()
                    / cycles as f64
            })
            .collect();
        template.iter().map(|x| x * x).sum::<f64>() / period as f64 / power(audio)
    };
    assert!(
        power(&engine) > 1e-8,
        "Source texture produced no engine stem"
    );
    assert!(power(&engine) < power(&exhaust));
    let engine_orders = coherent_share(&engine);
    let exhaust_orders = coherent_share(&exhaust);
    assert!(
        engine_orders < exhaust_orders * 0.5,
        "Engine copied exhaust orders: {engine_orders:.3} vs {exhaust_orders:.3}"
    );
}

#[test]
fn imported_bank_and_project_retain_provenance() {
    let bank = fixture();
    assert_eq!(bank.layers[0].len(), 3);
    assert_eq!(bank.layers[1].len(), 3);
    assert_eq!(bank.min_rpm, 800.);
    assert_eq!(bank.max_rpm, 4000.);
    let path =
        std::env::temp_dir().join(format!("bess-hybrid-project-{}.json", std::process::id()));
    let p = Project {
        version: 3,
        parameters: params(),
        hybrid: Settings {
            turbo: 0.4,
            ..Default::default()
        },
        source: Some(bank.source.clone()),
        driving: bess::drive::Controls {
            mode: bess::drive::Mode::Simulated,
            throttle: 0.65,
            gear: 3,
            automatic: false,
            resistance_nm: 450.,
            ..Default::default()
        },
    };
    project::save_project(&path, &p).unwrap();
    let read = project::load_project(&path).unwrap();
    assert_eq!(read.hybrid, p.hybrid);
    assert_eq!(read.source, p.source);
    assert_eq!(read.driving, p.driving);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn bank_playback_is_deterministic_and_ab_crossfade_is_bounded() {
    let h = Settings::default();
    let bank = fixture();
    let a = render::hybrid_samples(params(), h, bank.clone(), 2., true).unwrap();
    let b = render::hybrid_samples(params(), h, bank.clone(), 2., true).unwrap();
    assert_eq!(a, b);
    assert!(a.iter().all(|s| s.is_finite() && s.abs() < 1.));
    let mut engine = Hybrid::new(48000, params(), h, Some(bank));
    let mut previous = 0f32;
    let mut jump = 0f32;
    for i in 0..96000 {
        if i == 48000 {
            engine.set(
                params(),
                Settings {
                    enhanced: false,
                    ..h
                },
            );
        }
        let s = engine.next(true);
        jump = jump.max((s - previous).abs());
        previous = s;
    }
    assert!(jump < 0.08, "Discontinuity {jump}");
    for _ in 0..48000 {
        engine.next(false);
    }
    assert!(engine.next(false).abs() < 1e-6);
}
#[test]
fn every_exposed_hybrid_control_affects_the_audio() {
    let bank = fixture();
    let h = Settings {
        level_match: false,
        ..Default::default()
    };
    let baseline = render::hybrid_samples(params(), h, bank.clone(), 3., true).unwrap();
    for (name, variant) in [
        ("response", Settings { response: 0.9, ..h }),
        ("attack", Settings { attack: 1., ..h }),
        ("body", Settings { body: 1., ..h }),
        ("rasp", Settings { rasp: 1., ..h }),
        ("texture", Settings { texture: 1., ..h }),
        (
            "pulse_gain",
            Settings {
                pulse_gain: 0.,
                ..h
            },
        ),
        (
            "residual_gain",
            Settings {
                residual_gain: 0.,
                ..h
            },
        ),
        ("pipe", Settings { pipe: 1., ..h }),
        ("overrun", Settings { overrun: 1., ..h }),
        ("turbo", Settings { turbo: 1., ..h }),
        ("roughness", Settings { roughness: 1., ..h }),
        (
            "cycle_life",
            Settings {
                cycle_life: 1.,
                ..h
            },
        ),
        (
            "pulse_texture",
            Settings {
                pulse_texture: 1.,
                ..h
            },
        ),
        (
            "pressure_shape",
            Settings {
                pressure_shape: 1.,
                ..h
            },
        ),
        (
            "header_length",
            Settings {
                header_length: 1.4,
                ..h
            },
        ),
        (
            "diameter",
            Settings {
                diameter: 125.,
                ..h
            },
        ),
        ("chamber", Settings { chamber: 17., ..h }),
        (
            "absorption",
            Settings {
                absorption: 0.95,
                ..h
            },
        ),
        (
            "temperature",
            Settings {
                temperature: 160.,
                ..h
            },
        ),
        (
            "intake_length",
            Settings {
                intake_length: 1.15,
                ..h
            },
        ),
        ("airbox", Settings { airbox: 1., ..h }),
        ("fuel_cut", Settings { fuel_cut: 1., ..h }),
    ] {
        let audio = render::hybrid_samples(params(), variant, bank.clone(), 3., true).unwrap();
        let diff = rms_diff(&baseline, &audio);
        println!("{name}: delta RMS {diff:.8}");
        assert!(diff > 1e-7, "Ineffective control: {name}");
    }
    for (name, p) in [
        (
            "intake",
            Parameters {
                intake: 0.,
                ..params()
            },
        ),
        (
            "mechanical",
            Parameters {
                mechanical: 1.,
                ..params()
            },
        ),
        (
            "exhaust",
            Parameters {
                exhaust: 0.2,
                ..params()
            },
        ),
        (
            "pipe_length",
            Parameters {
                pipe_length: 4.5,
                ..params()
            },
        ),
        (
            "resonance",
            Parameters {
                resonance: 3.8,
                ..params()
            },
        ),
        (
            "brightness",
            Parameters {
                brightness: 700.,
                ..params()
            },
        ),
    ] {
        let audio = render::hybrid_samples(p, h, bank.clone(), 3., true).unwrap();
        let diff = rms_diff(&baseline, &audio);
        println!("{name}: delta RMS {diff:.8}");
        assert!(diff > 1e-7, "Ineffective control: {name}");
    }
}
#[test]
fn calibration_is_bounded_and_color_zero_removes_added_layers() {
    let bank = fixture();
    let h = Settings::calibrated(&bank);
    h.validate().unwrap();
    assert_eq!((h.overrun, h.turbo, h.roughness), (0., 0., 0.));
    assert!((0.6..=0.7).contains(&h.coloration));
    let base = Settings {
        coloration: 0.,
        level_match: false,
        ..h
    };
    let exaggerated = Settings {
        pipe: 1.,
        airbox: 1.,
        overrun: 1.,
        turbo: 1.,
        rasp: 1.,
        body: 1.,
        ..base
    };
    let a = render::hybrid_samples(params(), base, bank.clone(), 3., true).unwrap();
    let b = render::hybrid_samples(params(), exaggerated, bank, 3., true).unwrap();
    assert_eq!(a, b, "added layers leaked through color=0");
}
#[test]
fn simplified_controls_generate_distinct_bounded_timbre_curves() {
    for rpm in [-1., 0., 1.] {
        for load in [-1., 0., 1.] {
            let mut h = Settings {
                rpm_character: rpm,
                load_character: load,
                ..Default::default()
            };
            h.rebuild_character_maps();
            h.validate().unwrap();
            assert_eq!(h.maps.pulse.at(0., 0.), 1.);
            if rpm != 0. || load != 0. {
                assert_ne!(h.maps.pulse, h.maps.texture);
            }
            let decoded: Settings =
                serde_json::from_str(&serde_json::to_string(&h).unwrap()).unwrap();
            assert_eq!(h, decoded);
        }
    }
}
#[test]
fn timbre_maps_change_selected_regions_and_roundtrip() {
    let bank = fixture();
    let mut h = Settings {
        level_match: false,
        ..Default::default()
    };
    let low = Parameters {
        rpm: bank.min_rpm,
        load: 0.,
        ..params()
    };
    let high = Parameters {
        rpm: bank.max_rpm,
        load: 1.,
        ..params()
    };
    let baseline_low = render::hybrid_samples(low, h, bank.clone(), 1., false).unwrap();
    let baseline_high = render::hybrid_samples(high, h, bank.clone(), 1., false).unwrap();
    h.maps.exhaust.0[2][2] = 0.1;
    let modified_low = render::hybrid_samples(low, h, bank.clone(), 1., false).unwrap();
    let modified_high = render::hybrid_samples(high, h, bank.clone(), 1., false).unwrap();
    assert_eq!(baseline_low, modified_low, "unrelated region changed");
    assert!(rms_diff(&baseline_high, &modified_high) > 0.001);
    let path = std::env::temp_dir().join(format!("bess-maps-{}.json", std::process::id()));
    let project = Project {
        version: 3,
        parameters: high,
        hybrid: h,
        source: Some(bank.source.clone()),
        driving: Default::default(),
    };
    project::save_project(&path, &project).unwrap();
    assert_eq!(project::load_project(&path).unwrap().hybrid.maps, h.maps);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn invalid_waveforms_and_settings_are_rejected() {
    assert!(bess::bank::decode_wav(b"not a wav").is_err());
    assert!(
        Settings {
            response: f32::NAN,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn simulated_wav_uses_the_same_transport_as_listening() {
    use bess::{
        bench::Bench,
        drive::{Controls, Mode},
    };
    let c = Controls {
        mode: Mode::Simulated,
        throttle: 0.85,
        ..Default::default()
    };
    let h = Settings::default();
    let export = render::bench_samples(params(), h, fixture(), 3., c).unwrap();
    let mut live = Bench::new(48000, params(), h, c, Some(fixture()));
    let total = export.len();
    for (i, exported) in export.iter().enumerate() {
        let fade = ((total - i) as f32 / 2400.).min(1.);
        assert_eq!(*exported, live.next(true) * fade);
    }
    let heavy = render::bench_samples(
        params(),
        h,
        fixture(),
        3.,
        Controls {
            resistance_nm: 1800.,
            ..c
        },
    )
    .unwrap();
    assert!(rms_diff(&export, &heavy) > 0.005);
}

#[test]
fn simulated_transport_pauses_and_accepts_live_controls_without_allocating() {
    use bess::{
        bench::Bench,
        drive::{Controls, Mode},
    };
    let mut c = Controls {
        mode: Mode::Simulated,
        throttle: 0.7,
        automatic: false,
        ..Default::default()
    };
    let mut bench = Bench::new(48000, params(), Settings::default(), c, Some(fixture()));
    ALLOCATIONS.with(|v| v.set(0));
    TRACK.with(|v| v.set(true));
    for i in 0..96000 {
        if i % 256 == 0 {
            c.throttle = i as f32 / 96000.;
            c.resistance_nm = i as f32 / 96.;
            if i > 48000 {
                c.gear = 2;
            }
            bench.set(params(), Settings::default(), c, 0);
        }
        std::hint::black_box(bench.next(true));
    }
    TRACK.with(|v| v.set(false));
    assert_eq!(ALLOCATIONS.with(|v| v.get()), 0);
    let before = bench.state();
    for _ in 0..48000 {
        bench.next(false);
    }
    assert_eq!(bench.state().speed_kmh, before.speed_kmh);
    assert_eq!(bench.state().gear, before.gear);
    bench.set(params(), Settings::default(), c, 1);
    assert_eq!(bench.state().speed_kmh, 0.);
}

#[test]
fn playback_and_parameter_changes_do_not_allocate() {
    let mut engine = Hybrid::new(48000, params(), Settings::default(), Some(fixture()));
    ALLOCATIONS.with(|v| v.set(0));
    TRACK.with(|v| v.set(true));
    for i in 0..48000 {
        if i % 128 == 0 {
            engine.set(
                Parameters {
                    rpm: 800. + i as f32 * 0.06,
                    ..params()
                },
                Settings {
                    diameter: 30. + 100. * i as f32 / 48000.,
                    header_length: 0.15 + 1.35 * i as f32 / 48000.,
                    ..Settings::default()
                },
            );
        }
        std::hint::black_box(engine.next(true));
    }
    TRACK.with(|v| v.set(false));
    let allocations = ALLOCATIONS.with(|v| v.get());
    assert_eq!(allocations, 0, "Audio path allocated");
}

#[test]
fn source_ab_branch_is_unaffected_by_acoustic_and_event_controls() {
    let h = Settings {
        enhanced: false,
        ..Settings::default()
    };
    let a = render::hybrid_samples(params(), h, fixture(), 2., true).unwrap();
    let b = render::hybrid_samples(
        params(),
        Settings {
            enhanced: false,
            roughness: 1.,
            turbo: 1.,
            overrun: 1.,
            header_length: 1.5,
            diameter: 30.,
            chamber: 18.,
            temperature: 950.,
            airbox: 1.,
            intake_length: 1.2,
            combustion: bess::combustion::Combustion::even(12),
            ..Settings::character(2)
        },
        fixture(),
        2.,
        true,
    )
    .unwrap();
    assert_eq!(
        a, b,
        "The source reference must remain independent of enrichment"
    );
}

#[test]
fn afterfire_requires_a_release_and_returns_to_silence() {
    let p = Parameters {
        rpm: 3600.,
        load: 0.8,
        ..params()
    };
    let off = Settings {
        level_match: false,
        overrun: 0.,
        ..Settings::default()
    };
    let on = Settings { overrun: 1., ..off };
    let mut a = Hybrid::new(48000, p, off, Some(fixture()));
    let mut b = Hybrid::new(48000, p, on, Some(fixture()));
    let mut release_energy = 0.;
    let mut late_energy = 0.;
    for i in 0..192000 {
        if i == 48000 {
            let p = Parameters { load: 0.02, ..p };
            a.set(p, off);
            b.set(p, on);
        }
        let diff = a.next(true) - b.next(true);
        if i < 48000 {
            assert_eq!(diff, 0., "Afterfire on steady load");
        } else if i < 96000 {
            release_energy += diff * diff;
        } else if i > 144000 {
            late_energy += diff * diff;
        }
    }
    assert!(release_energy > 1e-5, "No lift-off event");
    assert!(
        late_energy < release_energy * 1e-5,
        "Persistent noise after release"
    );
}

#[test]
fn legacy_settings_load_and_geometry_boundaries_are_validated() {
    let old: Settings = serde_json::from_str(r#"{"body":0.7,"enhanced":false}"#).unwrap();
    assert_eq!(old.body, 0.7);
    assert!(!old.enhanced);
    old.validate().unwrap();
    for h in [
        Settings {
            diameter: 0.,
            ..old
        },
        Settings {
            chamber: f32::NAN,
            ..old
        },
        Settings {
            temperature: 10000.,
            ..old
        },
        Settings {
            header_length: 0.,
            ..old
        },
        Settings {
            intake_length: -1.,
            ..old
        },
    ] {
        assert!(h.validate().is_err());
    }
    for i in 0..3 {
        Settings::character(i).validate().unwrap();
    }
}

#[test]
fn rapid_geometry_and_load_changes_remain_bounded_at_device_rates() {
    for rate in [44100, 192000] {
        let mut p = Parameters {
            volume: 0.8,
            ..params()
        };
        let mut engine = Hybrid::new(rate, p, Settings::default(), Some(fixture()));
        let mut last = 0f32;
        let mut jump = 0f32;
        for i in 0..rate * 2 {
            if i % (rate / 8) == 0 {
                let high = (i / (rate / 8)).is_multiple_of(2);
                p.rpm = if high { 4000. } else { 800. };
                p.load = if high { 1. } else { 0. };
                p.pipe_length = if high { 5. } else { 0.2 };
                p.resonance = 4.;
                engine.set(
                    p,
                    Settings {
                        pipe: 1.,
                        chamber: if high { 18. } else { 0.3 },
                        diameter: if high { 30. } else { 130. },
                        temperature: if high { 150. } else { 950. },
                        header_length: if high { 0.15 } else { 1.5 },
                        absorption: 0.,
                        ..Settings::default()
                    },
                );
            }
            let s = engine.next(true);
            assert!(s.is_finite() && s.abs() < 1.);
            jump = jump.max((s - last).abs());
            last = s;
        }
        assert!(jump < 0.15, "Geometry discontinuity at {rate}: {jump}");
    }
}

#[test]
fn configured_combustion_is_audible_bounded_and_allocation_free() {
    let bank = fixture();
    let h = Settings {
        level_match: false,
        ..Settings::calibrated(&bank)
    };
    let configured = Settings {
        combustion: bess::combustion::Combustion::even(6),
        ..h
    };
    let a = render::hybrid_samples(params(), h, bank.clone(), 2., false).unwrap();
    let b = render::hybrid_samples(params(), configured, bank.clone(), 2., false).unwrap();
    assert!(rms_diff(&a, &b) > 0.0001);
    let mut engine = Hybrid::new(48000, params(), configured, Some(bank));
    ALLOCATIONS.with(|v| v.set(0));
    TRACK.with(|v| v.set(true));
    for _ in 0..48000 {
        let v = engine.next(true);
        assert!(v.is_finite() && v.abs() < 1.);
    }
    TRACK.with(|v| v.set(false));
    assert_eq!(ALLOCATIONS.with(|v| v.get()), 0);
}

#[test]
fn sound_presets_keep_explicit_engine_timing() {
    let bank = fixture();
    let mut custom = bess::combustion::Combustion::even(6);
    custom.angles[1] = 130.;
    custom.exhaust_delay = 190.;
    let starting = Settings {
        combustion: custom,
        enhanced: false,
        level_match: false,
        response: 0.7,
        ..Settings::calibrated(&bank)
    };
    for preset in 0..3 {
        let next = starting.character_preserving_engine(preset, Some(&bank));
        assert_eq!(next.combustion, custom);
        assert!(!next.enhanced && !next.level_match);
        assert_eq!(next.response, 0.7);
        assert_ne!(next.pipe, 0.28);
    }
}

#[test]
fn beamng_copy_labels_vehicle_and_preserves_all_other_non_audio_entries() {
    use std::io::Read;
    let bank = fixture();
    let original = std::fs::read(&bank.source.archive).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "bess-export-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    bess::export::package(&dir, params(), Settings::calibrated(&bank), bank.clone()).unwrap();
    assert_eq!(original, std::fs::read(&bank.source.archive).unwrap());
    let mut old = zip::ZipArchive::new(Cursor::new(&original)).unwrap();
    let package_name = bess::export::package_name(&bank);
    assert!(
        std::fs::read_to_string(dir.join("INSTALLATION.txt"))
            .unwrap()
            .contains(&package_name)
    );
    let mut new =
        zip::ZipArchive::new(std::fs::File::open(dir.join(&package_name)).unwrap()).unwrap();
    assert_eq!(old.len(), new.len());
    for i in 0..old.len() {
        let mut entry = old.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut a = Vec::new();
        entry.read_to_end(&mut a).unwrap();
        let mut b = Vec::new();
        new.by_name(&name).unwrap().read_to_end(&mut b).unwrap();
        if name.ends_with(".wav") {
            assert_ne!(a, b);
            let (rate, pcm) = bess::bank::decode_wav(&b).unwrap();
            assert_eq!(rate, 48000);
            assert!(pcm.len() >= 96000);
            assert!(pcm.iter().all(|v| v.is_finite() && v.abs() <= 0.951));
            let rms = (pcm.iter().map(|v| v * v).sum::<f32>() / pcm.len() as f32).sqrt();
            assert!(rms > 0.0001);
            assert!((pcm[0] - pcm[pcm.len() - 1]).abs() < rms * 0.25);
        } else if name == "vehicles/test/info.json" {
            let (expected, display_name) = bess::export::label_vehicle_info(&a).unwrap();
            assert_eq!(display_name, "Test Vehicle (BESS)");
            assert_eq!(b, expected);
            assert_eq!(
                b,
                b"{\"Name\":\"Test Vehicle (BESS)\",\"paints\":{\"Blue\":1,\"Blue\":2}}"
            );
        } else {
            assert_eq!(a, b, "Changed {name}");
        }
    }
    let restored = Bank::load(&dir.join(&package_name), None).unwrap();
    assert_eq!(restored.min_rpm, bank.min_rpm);
    assert_eq!(restored.max_rpm, bank.max_rpm);
    let project = project::load_project(&dir.join("settings.bess.json")).unwrap();
    assert_eq!(project.source.as_ref().unwrap(), &bank.source);
}

#[test]
fn vehicle_label_preserves_nested_names_escapes_and_duplicate_paints() {
    let source =
        br#"{"paints":{"Name":"Blue","Blue":1,"Blue":2},"Name":"Cerberus \"A\"","notes":"Name"}"#;
    let (labelled, name) = bess::export::label_vehicle_info(source).unwrap();
    assert_eq!(name, "Cerberus \"A\" (BESS)");
    assert_eq!(
        labelled,
        br#"{"paints":{"Name":"Blue","Blue":1,"Blue":2},"Name":"Cerberus \"A\" (BESS)","notes":"Name"}"#
    );
    let (again, _) = bess::export::label_vehicle_info(&labelled).unwrap();
    assert_eq!(again, labelled);
    assert!(bess::export::label_vehicle_info(br#"{"Name":"A","Name":"B"}"#).is_err());
}
