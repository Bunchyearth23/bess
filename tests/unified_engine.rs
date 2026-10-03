#[path = "../src/test_support.rs"]
mod test_support;

use bess::{
    automation_model::AutomationModel,
    bank::Bank,
    bench::Bench,
    drive::{Controls, Mode},
    engine_definition::EngineDefinition,
    hybrid::{Hybrid, Settings},
    physical::engine::Sample,
    project::Parameters,
    room::Room,
    scratch::{ScratchModel, ScratchVoice},
};
use std::sync::{Arc, OnceLock};

thread_local! {
    static TRACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static ALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static DEALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct CountingAllocator;
// SAFETY: forwards unchanged allocations to System; counters never allocate.
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if TRACK.try_with(|v| v.get()).unwrap_or(false) {
            let _ = ALLOCATIONS.try_with(|v| v.set(v.get() + 1));
        }
        // SAFETY: unchanged valid GlobalAlloc layout.
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        if TRACK.try_with(|v| v.get()).unwrap_or(false) {
            let _ = DEALLOCATIONS.try_with(|v| v.set(v.get() + 1));
        }
        // SAFETY: pointer and layout belong to the sole System backend.
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn fixture() -> Arc<Bank> {
    static BANK: OnceLock<Arc<Bank>> = OnceLock::new();
    BANK.get_or_init(|| {
        let path = test_support::automation_fixture();
        let bank = Bank::load(&path, None).unwrap();
        std::fs::remove_file(path).unwrap();
        Arc::new(bank)
    })
    .clone()
}

fn definition() -> EngineDefinition {
    EngineDefinition::from_scratch(&AutomationModel::from_bank(&fixture()).unwrap().scratch)
}

fn assert_state(a: Sample, b: Sample) {
    for (a, b) in [
        (a.rpm, b.rpm),
        (a.map_pa, b.map_pa),
        (a.torque_nm, b.torque_nm),
        (a.heat_j, b.heat_j),
        (a.fuel_injected_kg, b.fuel_injected_kg),
        (a.turbo_rpm, b.turbo_rpm),
    ] {
        assert_eq!(a.to_bits(), b.to_bits());
    }
    assert_eq!(a.fuel_cut, b.fuel_cut);
}

#[test]
fn both_origins_run_the_same_physical_commands_and_shaft_in_every_mode() {
    let definition = definition();
    for (mode, saved) in [Mode::Direct, Mode::Cycle, Mode::Simulated]
        .into_iter()
        .flat_map(|mode| [(mode, false), (mode, true)])
    {
        let mut h = Settings {
            engine: saved.then_some(definition),
            level_match: false,
            ..Default::default()
        };
        let mut p = Parameters {
            rpm: 2200.,
            load: 0.6,
            ..Default::default()
        };
        let mut c = Controls {
            mode,
            gear: 0,
            throttle: 0.4,
            automatic: false,
            ..Default::default()
        };
        let mut imported = Bench::new(48000, p, h, c, Some(fixture()));
        let mut designed = Bench::from_scratch(
            96000,
            p,
            h,
            c,
            ScratchModel::build(&definition.to_scratch(), 96000).unwrap(),
        );
        imported.set_cycle_seconds(0.5);
        designed.set_cycle_seconds(0.5);
        for frame in 0..24000 {
            if frame % 4000 == 0 {
                p.load = if frame % 8000 == 0 { 0.75 } else { 0. };
                c.throttle = p.load;
                h.fuel_cut = if frame < 12000 { 0. } else { 1. };
                h.accessory_ac = frame >= 8000;
                h.accessory_steering = frame >= 16000;
                h.starter = frame == 20000;
                let restart = u64::from(frame >= 16000);
                imported.set(p, h, c, restart);
                designed.set(p, h, c, restart);
            }
            assert!(imported.next(true).is_finite());
            designed.next(true);
            assert!(designed.next(true).is_finite());
            assert_state(
                imported.physical_state().unwrap(),
                designed.physical_state().unwrap(),
            );
        }
        assert!(!imported.failed() && !designed.failed());
    }
}

#[test]
fn imported_prepared_edits_and_controls_preserve_state_without_render_allocations() {
    let definition = definition();
    let p = Parameters {
        rpm: 2200.,
        load: 0.6,
        ..Default::default()
    };
    let mut h = Settings {
        engine: Some(definition),
        engine_baseline: Some(definition),
        level_match: false,
        ..Default::default()
    };
    let c = Controls {
        mode: Mode::Simulated,
        gear: 0,
        throttle: 0.4,
        automatic: false,
        ..Default::default()
    };
    let mut bench = Bench::new(48000, p, h, c, Some(fixture()));
    bench.enable_room();
    for _ in 0..4800 {
        bench.next(true);
    }
    let mut definitions = [definition; 4];
    let mut models: Vec<_> = definitions
        .iter_mut()
        .enumerate()
        .map(|(index, definition)| {
            definition.sound.bass_db = index as f32 + 2.;
            if index >= 2 {
                definition.inertia += 0.02 * index as f32;
            }
            let mut scratch = definition.to_scratch();
            // Old projects may retain dormant renderer calibration after the
            // initial canonical build. This must not restart physical state.
            scratch.standalone.calibration.pulse_ms += 0.2 * (index + 1) as f32;
            Some(ScratchModel::build(&scratch, 96000).unwrap())
        })
        .collect();
    let mut trash = Vec::with_capacity(8);
    ALLOCATIONS.with(|v| v.set(0));
    DEALLOCATIONS.with(|v| v.set(0));
    TRACK.with(|v| v.set(true));
    for (index, model) in models.iter_mut().enumerate() {
        let before = bench.physical_state().unwrap();
        h.engine = Some(definitions[index]);
        h.accessory_ac = index % 2 == 0;
        bench.set(p, h, c, 0);
        bench.set_room(
            if index % 2 == 0 {
                Room::Garage
            } else {
                Room::Hall
            },
            0.5,
        );
        let retired = bench.swap_scratch(model.take().unwrap());
        if index < 2 {
            assert_state(bench.physical_state().unwrap(), before);
            assert!(matches!(retired, Some(ScratchVoice::Prepared { .. })));
        }
        if let Some(retired) = retired {
            trash.push(retired);
        }
        for _ in 0..2400 {
            assert!(bench.next(true).is_finite());
        }
        while let Some(retired) = bench.take_retired() {
            trash.push(retired);
        }
    }
    TRACK.with(|v| v.set(false));
    assert_eq!(ALLOCATIONS.with(|v| v.get()), 0);
    assert_eq!(DEALLOCATIONS.with(|v| v.get()), 0);
    assert!(!bench.failed());
    assert_eq!(trash.len(), 4);
}

#[test]
fn a_recording_is_unchanged_by_complete_engine_overrides() {
    let original = definition();
    let mut changed = original;
    changed.inertia *= 1.5;
    changed.sound.presence_db = 8.;
    changed.experimental.afterfire = 0.7;
    let p = Parameters {
        rpm: 2200.,
        load: 0.6,
        ..Default::default()
    };
    let make = |definition| {
        Hybrid::new(
            48000,
            p,
            Settings {
                engine: Some(definition),
                level_match: false,
                ..Default::default()
            },
            Some(fixture()),
        )
    };
    let (mut a, mut b) = (make(original), make(changed));
    let mut difference = 0.;
    for _ in 0..12000 {
        let (a, b) = (a.next_stems(true), b.next_stems(true));
        assert_eq!(a.source_reference.to_bits(), b.source_reference.to_bits());
        difference += (a.mixed - b.mixed).abs();
    }
    assert!(difference > 0.001);
    assert!(!a.failed() && !b.failed());
}

#[test]
fn imported_room_off_is_exact_and_active_room_is_only_an_audition_effect() {
    let h = Settings {
        engine: Some(definition()),
        level_match: false,
        ..Default::default()
    };
    let make = || {
        Bench::new(
            48000,
            Parameters::default(),
            h,
            Controls::default(),
            Some(fixture()),
        )
    };
    let (mut dry, mut room) = (make(), make());
    room.enable_room();
    room.set_room(Room::Off, 0.7);
    for _ in 0..6000 {
        assert_eq!(dry.next(true).to_bits(), room.next(true).to_bits());
    }
    room.set_room(Room::Garage, 0.7);
    let mut differs = false;
    for _ in 0..12000 {
        differs |= dry.next(true).to_bits() != room.next(true).to_bits();
        assert_state(
            dry.physical_state().unwrap(),
            room.physical_state().unwrap(),
        );
    }
    assert!(differs);
}

#[test]
fn real_genesis_import_survives_the_complete_audition_cycle() {
    let path = std::path::Path::new("cars/bunchyearth23_genesis_phantom.zip");
    if !path.exists() {
        return;
    }
    let bank = Arc::new(Bank::load(path, None).unwrap());
    let baseline = AutomationModel::from_bank(&bank).unwrap().baseline;
    for edited in [false, true] {
        let mut definition = baseline;
        if edited {
            definition.sound.exhaust_body_db = 3.;
            definition.sound.presence_db = -2.;
            definition.tuning.intake.plenum_ratio = Some(1.8);
            definition.experimental.afterfire = 0.25;
        }
        let p = Parameters {
            cylinders: definition.design.cylinders,
            rpm: definition.idle_rpm,
            load: 0.6,
            volume: 0.65,
            ..Default::default()
        };
        let h = Settings {
            engine: Some(definition),
            engine_baseline: Some(baseline),
            level_match: false,
            ..Default::default()
        };
        let c = Controls {
            mode: Mode::Cycle,
            ..Default::default()
        };
        let mut bench = Bench::new(48000, p, h, c, Some(bank.clone()));
        assert!(bench.initialization_error().is_none());
        bench.set_cycle_seconds(8.);
        for frame in 0..384000 {
            let before = bench.physical_state().unwrap();
            let sample = bench.next(true);
            assert!(
                !bench.failed(),
                "edited={edited} frame={frame} time={} previous={before:?}",
                frame as f64 / 48000.
            );
            assert!(sample.is_finite());
        }
    }
}
