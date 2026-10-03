#[path = "../src/test_support.rs"]
mod test_support;
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
    static DEALLOCATIONS:std::cell::Cell<usize>=const {std::cell::Cell::new(0)};
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
        if TRACK.try_with(|v| v.get()).unwrap_or(false) {
            let _ = DEALLOCATIONS.try_with(|v| v.set(v.get() + 1));
        }
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
fn build_fixture(textured: bool) -> Arc<Bank> {
    let path = fixture_zip(textured, false);
    let bank = Arc::new(Bank::load(&path, None).unwrap());
    // Keep the source for the replacement-mod roundtrip test.
    bank
}
fn fixture_zip(textured: bool, silent_knot: bool) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "bess-fixture-{textured}-{silent_knot}-{}.zip",
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
                    let silent = silent_knot && layer == 0 && rpm == 1600;
                    wav.write_sample(if silent {
                        0.
                    } else {
                        sample * (0.6 + 0.4 * layer as f32)
                    })
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
    path
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
            engine_gain: 0.4,
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
        profile_name: "Test profile".into(),
        scratch: None,
    };
    project::save_project(&path, &p).unwrap();
    let read = project::load_project(&path).unwrap();
    assert_eq!(read.hybrid, p.hybrid);
    assert_eq!(read.source, p.source);
    assert_eq!(read.driving, p.driving);
    assert_eq!(read.profile_name, p.profile_name);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn invalid_waveforms_and_settings_are_rejected() {
    assert!(bess::bank::decode_wav(b"not a wav").is_err());
    let silent = Bank::load(&fixture_zip(false, true), None)
        .map(|_| ())
        .unwrap_err();
    assert!(silent.contains("Silent WAV at 1600 rpm"), "{silent}");
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

#[test]
fn beamng_copy_labels_vehicle_and_preserves_all_other_non_audio_entries() {
    use std::io::Read;
    let bank = physical_fixture();
    let original = std::fs::read(&bank.source.archive).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "bess-export-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    bess::export::package(
        &dir,
        params(),
        Settings {
            procedural: true,
            ..Settings::calibrated(&bank)
        },
        bank.clone(),
    )
    .unwrap();
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
    assert_eq!(old.len() + 1, new.len());
    assert!(new.by_name(bess::babm_exchange::MARKER_PATH).is_ok());
    for i in 0..old.len() {
        let mut entry = old.by_index(i).unwrap();
        let name = entry.name().to_owned();
        let mut a = Vec::new();
        entry.read_to_end(&mut a).unwrap();
        let mut b = Vec::new();
        new.by_name(&name).unwrap().read_to_end(&mut b).unwrap();
        if name.ends_with(".wav") && a != b {
            assert_ne!(a, b);
            let (rate, pcm) = bess::bank::decode_wav(&b).unwrap();
            assert_eq!(rate, 48000);
            assert!(pcm.len() >= 96000);
            assert!(pcm.iter().all(|v| v.is_finite() && v.abs() <= 0.951));
            let rms = (pcm.iter().map(|v| v * v).sum::<f32>() / pcm.len() as f32).sqrt();
            assert!(rms > 0.0001);
            assert!((pcm[0] - pcm[pcm.len() - 1]).abs() < rms * 0.25);
        } else if name.ends_with("/info.json") && a != b {
            let (expected, _) = bess::export::label_vehicle_info(&a).unwrap();
            assert_eq!(b, expected);
        } else {
            assert_eq!(a, b, "Changed {name}");
        }
    }
    let restored = Bank::load(&dir.join(&package_name), None).unwrap();
    assert_eq!(restored.min_rpm, bank.min_rpm);
    assert_eq!(restored.max_rpm, bank.max_rpm);
    let project = project::load_project(&dir.join("settings.bess.json")).unwrap();
    assert_eq!(project.source.as_ref().unwrap(), &bank.source);
    assert!(!project.hybrid.procedural);
}

fn physical_fixture() -> Arc<Bank> {
    static BANK: OnceLock<Arc<Bank>> = OnceLock::new();
    BANK.get_or_init(|| Arc::new(Bank::load(&test_support::automation_fixture(), None).unwrap()))
        .clone()
}

#[test]
fn original_a_is_exact_source_playback_with_no_physical_metadata() {
    use bdsp::resample::{SincQuality, SincTable};
    let bank = fixture();
    let p = params();
    let h = Settings {
        enhanced: false,
        ..Default::default()
    };
    let mut hybrid = Hybrid::new(48000, p, h, Some(bank.clone()));
    assert!(hybrid.initialization_error().is_some());
    assert!(!hybrid.failed());
    let sinc = SincTable::for_quality(SincQuality::Realtime);
    let mut cycle = 0.;
    let mut gain = 0.;
    let mut energy = 0.;
    for _ in 0..12000 {
        cycle += p.rpm as f64 / (120. * 48000.);
        gain += (p.volume - gain) * (1. / (48000. * 0.025));
        let raw = bank.read_original(cycle, p.rpm, p.load, 48000., &sinc) * gain;
        let expected = if raw.abs() > 0.95 {
            raw.signum() * (0.95 + 0.049 * ((raw.abs() - 0.95) / 0.049).tanh())
        } else {
            raw
        };
        assert_eq!(hybrid.next(true).to_bits(), expected.to_bits());
        energy += expected * expected;
    }
    assert!(energy > 0.01);
    hybrid.set(
        p,
        Settings {
            enhanced: true,
            ..h
        },
    );
    assert!(hybrid.failed()); // Explicit error; never another synthesizer.
}

#[test]
fn physical_b_is_deterministic_and_sound_tuning_never_changes_reference_a() {
    let bank = physical_fixture();
    let p = params();
    let h = Settings {
        level_match: false,
        ..Default::default()
    };
    let mut a = Hybrid::new(48000, p, h, Some(bank.clone()));
    let mut b = Hybrid::new(48000, p, h, Some(bank.clone()));
    assert!(a.initialization_error().is_none());
    let mut difference = 0.;
    let mut energy = 0.;
    for i in 0..24000 {
        if i == 12000 {
            let mut tuned = h;
            tuned.physical_sound.presence_db = 9.;
            tuned.physical_sound.intake_resonance = 1.5;
            b.set(p, tuned);
        }
        let x = a.next_stems(true);
        let y = b.next_stems(true);
        assert_eq!(x.source_reference.to_bits(), y.source_reference.to_bits());
        assert!(
            [x.exhaust, x.engine, x.mixed, y.mixed]
                .iter()
                .all(|v| v.is_finite())
        );
        assert!(x.mixed.abs() < 0.95 && y.mixed.abs() < 0.95);
        if i < 12000 {
            assert_eq!(x.mixed.to_bits(), y.mixed.to_bits());
        } else {
            difference += (x.mixed - y.mixed).abs();
        }
        energy += x.exhaust * x.exhaust + x.engine * x.engine;
    }
    assert!(energy > 1e-6 && difference > 1e-5);
    assert!(!a.failed() && !b.failed());
}

#[test]
fn physical_render_and_live_bench_share_identical_signal() {
    use bess::{
        bench::Bench,
        drive::{Controls, Mode},
    };
    let bank = physical_fixture();
    let p = params();
    let h = Settings::default();
    let c = Controls {
        mode: Mode::Direct,
        ..Default::default()
    };
    let rendered = render::bench_samples(p, h, bank.clone(), 1., c).unwrap();
    let mut live = Bench::new(48000, p, h, c, Some(bank));
    for (i, output) in rendered.iter().enumerate() {
        let fade = ((rendered.len() - i) as f32 / 2400.).min(1.);
        assert_eq!(output.to_bits(), (live.next(true) * fade).to_bits());
    }
}

#[test]
fn physical_processing_and_live_retunes_allocate_nothing() {
    let p = params();
    let mut h = Settings::default();
    let mut hybrid = Hybrid::new(48000, p, h, Some(physical_fixture()));
    for _ in 0..2400 {
        hybrid.next(true);
    }
    ALLOCATIONS.with(|n| n.set(0));
    DEALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|v| v.set(true));
    for i in 0..9600 {
        if i % 2400 == 0 {
            h.physical_sound.bass_db = i as f32 / 2400.;
            h.physical_sound.tail_length_m = 1. + i as f32 / 9600.;
            h.physical_sound.exhaust_body_db = 8.;
            h.physical_sound.exhaust_body_hz = 150. + i as f32 / 10.;
            h.physical_sound.exhaust_low_cut_hz = 60.;
            h.physical_sound.exhaust_high_cut_hz = 4000.;
            h.physical_sound.exhaust_drive = 0.4;
            hybrid.set(p, h);
        }
        std::hint::black_box(hybrid.next(true));
    }
    TRACK.with(|v| v.set(false));
    assert_eq!(ALLOCATIONS.with(|n| n.get()), 0);
    assert_eq!(DEALLOCATIONS.with(|n| n.get()), 0);
    assert!(!hybrid.failed());
}

#[test]
fn migration_reads_old_controls_but_saves_only_physical_audio_controls() {
    let h: Settings =
        serde_json::from_str(r#"{"procedural":true,"source_timbre":0.8,"enhanced":false}"#)
            .unwrap();
    let saved = serde_json::to_value(h).unwrap();
    for old in [
        "procedural",
        "source_timbre",
        "generated_body",
        "combustion",
        "maps",
    ] {
        assert!(saved.get(old).is_none(), "{old}");
    }
    assert!(saved.get("physical_sound").is_some());
    assert!(!h.enhanced);
}

#[test]
fn optional_level_match_converges_to_source_without_lifting_muted_stems() {
    let bank = physical_fixture();
    let p = Parameters {
        rpm: 2000.,
        load: 0.4,
        volume: 0.8,
        ..params()
    };
    let h = Settings {
        level_match: true,
        ..Default::default()
    };
    let mut original = Hybrid::new(
        48000,
        p,
        Settings {
            enhanced: false,
            ..h
        },
        Some(bank.clone()),
    );
    let mut physical = Hybrid::new(48000, p, h, Some(bank));
    let mut a2 = 0_f64;
    let mut b2 = 0_f64;
    for frame in 0..48000 * 12 {
        let a = original.next(true);
        let b = physical.next(true);
        if frame >= 48000 * 9 {
            a2 += f64::from(a).powi(2);
            b2 += f64::from(b).powi(2);
        }
    }
    let difference_db = 10. * (b2 / a2).log10();
    assert!(
        difference_db.abs() < 1.,
        "Matched RMS difference {difference_db} dB"
    );
    physical.set(
        Parameters {
            intake: 0.,
            exhaust: 0.,
            mechanical: 0.,
            ..p
        },
        h,
    );
    let mut peak = 0_f32;
    for frame in 0..48000 * 2 {
        let sample = physical.next(true);
        if frame > 48000 {
            peak = peak.max(sample.abs());
        }
    }
    assert!(peak < 1e-7, "Muted layer was raised: {peak}");
}
