use bess::{
    bench::Bench,
    drive::{Controls, Mode},
    engine_build::{Aspiration, Crossover},
    hybrid::Settings,
    physical::engine::{Commands, Engine},
    project::Parameters,
    scratch::{PRESETS, Scratch, ScratchModel, ScratchVoice},
};

thread_local! {
    static TRACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static ALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static DEALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
struct CountingAllocator;
// SAFETY: memory operations forward their unchanged arguments to System;
// tracking uses thread-local integer cells and performs no allocation.
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
        // SAFETY: pointer and layout belong to the sole backend System.
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

#[test]
fn prepared_sound_retunes_preserve_running_bench_and_allocate_or_drop_no_buffers() {
    let scratch = Scratch::default();
    let params = Parameters {
        volume: 0.2,
        ..Default::default()
    };
    let controls = Controls {
        mode: Mode::Simulated,
        gear: 0,
        automatic: false,
        throttle: 0.35,
        ..Default::default()
    };
    let mut bench = Bench::from_scratch(
        96_000,
        params,
        Settings::default(),
        controls,
        ScratchModel::build(&scratch, 96_000).unwrap(),
    );
    for _ in 0..9600 {
        bench.next(true);
    }
    let mut tuning = scratch.clone();
    tuning.sound = bess::scratch::SoundTuning {
        bass_db: 6.,
        presence_db: -4.,
        treble_db: 3.,
        brightness_hz: 6000.,
        drive: 0.35,
        flow_texture: 0.3,
        rpm_brightness_db: 3.,
        load_brightness_db: -2.,
        intake_length_m: 0.55,
        intake_resonance: 1.2,
        intake_air_noise: 0.7,
        mechanical_pitch_hz: 1600.,
        mechanical_resonance: 3.,
        cycle_variation: 1.3,
        combustion_duration: 1.2,
        ignition_retard_deg: 8.,
        primary_length_scale: 1.2,
        tail_length_m: 2.2,
        exhaust_decay_ms: 40.,
        muffler_volume_scale: 1.8,
        muffler_absorption: 0.7,
        exhaust_body_db: 6.,
        exhaust_body_hz: 400.,
        exhaust_low_cut_hz: 80.,
        exhaust_high_cut_hz: 5000.,
        exhaust_drive: 0.3,
        ..Default::default()
    };
    let mut prepared: Vec<_> = (0..4)
        .map(|i| {
            tuning.sound.bass_db = 6. + i as f32;
            // Include both damping bounds and a return to the historical value;
            // rapid prepared swaps must not allocate, reset, or destroy buffers.
            tuning.sound.exhaust_decay_ms = [10., 40., 120., 250.][i];
            Some(ScratchModel::build(&tuning, 96_000).unwrap())
        })
        .collect();
    // All retirement storage is reserved before entering the rendering region.
    let mut trash = Vec::with_capacity(8);
    ALLOCATIONS.with(|v| v.set(0));
    DEALLOCATIONS.with(|v| v.set(0));
    TRACK.with(|v| v.set(true));
    for model in &mut prepared {
        let before = bench.state();
        let retired = bench.swap_scratch(model.take().unwrap()).unwrap();
        assert!(matches!(retired, ScratchVoice::Prepared { .. }));
        assert_eq!(bench.state().rpm.to_bits(), before.rpm.to_bits());
        assert_eq!(
            bench.state().speed_kmh.to_bits(),
            before.speed_kmh.to_bits()
        );
        trash.push(retired);
        // Include rapid repeated retunes while the previous DSP fade is active.
        for _ in 0..960 {
            assert!(bench.next(true).is_finite());
        }
        while let Some(retired) = bench.take_retired() {
            trash.push(retired);
        }
    }
    for _ in 0..4800 {
        assert!(bench.next(true).is_finite());
    }
    TRACK.with(|v| v.set(false));
    assert!(!bench.failed());
    assert_eq!(ALLOCATIONS.with(|v| v.get()), 0);
    assert_eq!(DEALLOCATIONS.with(|v| v.get()), 0);
    assert_eq!(
        trash.len(),
        4,
        "each complete prepared model must return to the owner"
    );
}
#[test]
fn live_room_switching_allocates_or_drops_nothing() {
    use bess::room::Room;
    let mut bench = Bench::from_scratch(
        96_000,
        Parameters {
            volume: 0.2,
            ..Default::default()
        },
        Settings::default(),
        Controls::default(),
        ScratchModel::build(&Scratch::default(), 96_000).unwrap(),
    );
    bench.enable_room();
    for _ in 0..9600 {
        bench.next(true);
    }
    ALLOCATIONS.with(|v| v.set(0));
    DEALLOCATIONS.with(|v| v.set(0));
    TRACK.with(|v| v.set(true));
    // Includes a switch requested while the previous crossfade is running.
    for (room, mix, frames) in [
        (Room::Garage, 0.5, 9600),
        (Room::Hall, 0.5, 400),
        (Room::Outdoor, 1., 9600),
        (Room::Hall, 0.2, 9600),
        (Room::Off, 0.2, 9600),
        (Room::Garage, 0.8, 9600),
    ] {
        bench.set_room(room, mix);
        for _ in 0..frames {
            assert!(bench.next(true).is_finite());
        }
    }
    TRACK.with(|v| v.set(false));
    assert!(!bench.failed());
    assert_eq!(ALLOCATIONS.with(|v| v.get()), 0);
    assert_eq!(DEALLOCATIONS.with(|v| v.get()), 0);
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn turbo_v12_control_changes_and_restart_allocate_nothing() {
    let mut scratch = Scratch {
        design: PRESETS.iter().find(|p| p.0 == "V12 60°").unwrap().1,
        ..Default::default()
    };
    scratch.build.aspiration = Aspiration::TwinTurbo;
    scratch.build.crossover = Crossover::X;
    scratch.apply_design();
    // The X-017 wave/valve junction must also stay allocation-free.
    for (afterfire, coupled) in [(0., false), (1., false), (0., true)] {
        scratch.experimental.afterfire = afterfire;
        scratch.experimental.wave_coupling = coupled;
        let mut engine = Engine::new(&scratch, 96000).unwrap();
        ALLOCATIONS.with(|v| v.set(0));
        TRACK.with(|v| v.set(true));
        for i in 0..48000 {
            if i == 24000 {
                engine.reset();
            }
            std::hint::black_box(engine.next(Commands {
                imposed_rpm: Some(1200. + f64::from(i % 24000) * 0.1),
                throttle: if i % 12000 < 6000 { 0.8 } else { 0. },
                ac: i % 8000 < 4000,
                ..Default::default()
            }));
        }
        TRACK.with(|v| v.set(false));
        assert!(!engine.failed());
        assert_eq!(ALLOCATIONS.with(|v| v.get()), 0);
    }
}
