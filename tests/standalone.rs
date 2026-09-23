use bess::standalone::{CombustionState, Commands, Config, Synth};

fn render(config: Config, frames: usize, block: usize, command: Commands) -> (Vec<f32>, u64) {
    let mut synth = Synth::new(48_000, config, command).unwrap();
    let mut samples = vec![0.; frames];
    for chunk in samples.chunks_mut(block) {
        synth.render_block(chunk);
    }
    (samples, synth.counts().combustion)
}

#[test]
fn four_stroke_event_rate_and_block_boundaries() {
    for (name, expected) in [("single", 25), ("four-even", 100)] {
        let config = Config::preset(name).unwrap();
        let command = Commands::default();
        let (a, count_a) = render(config.clone(), 48_000, 1, command);
        let (b, count_b) = render(config.clone(), 48_000, 257, command);
        let (c, count_c) = render(config, 48_000, 48_000, command);
        assert_eq!((count_a, count_b, count_c), (expected, expected, expected));
        assert_eq!(a, b);
        assert_eq!(b, c);
        let (_, four_seconds) = render(Config::preset(name).unwrap(), 192_000, 256, command);
        assert_eq!(four_seconds, expected * 4);
    }
}

#[test]
fn deterministic_ramp_and_cycle_preserved() {
    let c = Config::preset("four-split").unwrap();
    let mut a = Synth::new(48_000, c.clone(), Commands::default()).unwrap();
    let mut b = Synth::new(48_000, c, Commands::default()).unwrap();
    for sample in 0..48_000 {
        if sample % 731 == 0 {
            let command = Commands {
                rpm: 900. + sample as f32 / 48_000. * 5_000.,
                ..Commands::default()
            };
            a.set_commands(command).unwrap();
            b.set_commands(command).unwrap();
        }
        assert_eq!(a.next_sample(), b.next_sample());
    }
    assert!(a.cycle_position() > 15.);
    assert_eq!(a.counts(), b.counts());
}

#[test]
fn zero_rpm_freezes_events_while_paths_decay() {
    let mut synth = Synth::new(
        48_000,
        Config::preset("single").unwrap(),
        Commands::default(),
    )
    .unwrap();
    for _ in 0..48_000 {
        synth.next_sample();
    }
    let before = synth.counts();
    let phase = synth.cycle_position();
    synth
        .set_commands(Commands {
            rpm: 0.,
            ..Commands::default()
        })
        .unwrap();
    let mut early = 0f64;
    let mut late = 0f64;
    for n in 0..48_000 {
        let sample = synth.next_sample();
        if n < 2_400 {
            early += (sample as f64).powi(2);
        }
        if n >= 45_600 {
            late += (sample as f64).powi(2);
        }
    }
    assert_eq!(synth.counts(), before);
    assert_eq!(synth.cycle_position(), phase);
    assert!(early > 1e-12);
    assert!(late < early * 0.001);
}

#[test]
fn fuel_cut_is_explicit_and_not_a_zero_load_alias() {
    let config = Config::preset("single").unwrap();
    let mut firing = Synth::new(
        48_000,
        config.clone(),
        Commands {
            load: 0.,
            ..Commands::default()
        },
    )
    .unwrap();
    let mut cut = Synth::new(
        48_000,
        config,
        Commands {
            combustion: CombustionState::FuelCut,
            ..Commands::default()
        },
    )
    .unwrap();
    for _ in 0..48_000 {
        firing.next_sample();
        cut.next_sample();
    }
    assert_eq!(firing.counts().combustion, 25);
    assert_eq!(cut.counts().combustion, 0);
    assert_eq!(cut.counts().intake, 25);
    assert_eq!(cut.counts().exhaust, 25);
    let config = Config::preset("single").unwrap();
    let (motoring, _) = render(
        config.clone(),
        24_000,
        256,
        Commands {
            combustion: CombustionState::Motoring,
            ..Commands::default()
        },
    );
    let (fuel_cut, _) = render(
        config,
        24_000,
        256,
        Commands {
            combustion: CombustionState::FuelCut,
            ..Commands::default()
        },
    );
    assert_ne!(motoring, fuel_cut);
}

#[test]
fn load_changes_shape_not_just_final_gain() {
    let config = Config::preset("four-even").unwrap();
    let (low, _) = render(
        config.clone(),
        24_000,
        256,
        Commands {
            load: 0.1,
            ..Commands::default()
        },
    );
    let (high, _) = render(
        config,
        24_000,
        256,
        Commands {
            load: 0.9,
            ..Commands::default()
        },
    );
    let dot = low
        .iter()
        .zip(&high)
        .map(|(a, b)| *a as f64 * *b as f64)
        .sum::<f64>();
    let energy = low.iter().map(|a| (*a as f64).powi(2)).sum::<f64>();
    let scale = dot / energy;
    let residual = low
        .iter()
        .zip(&high)
        .map(|(a, b)| (*b as f64 - scale * *a as f64).powi(2))
        .sum::<f64>();
    let high_energy = high.iter().map(|b| (*b as f64).powi(2)).sum::<f64>();
    assert!(residual / high_energy > 0.001);
}

#[test]
fn names_are_invariant_but_phasing_and_routing_are_audible() {
    let base = Config::preset("four-even").unwrap();
    let mut renamed = base.clone();
    for cylinder in &mut renamed.cylinders {
        cylinder.name.push_str(" renamed");
    }
    let (reference, _) = render(base.clone(), 24_000, 256, Commands::default());
    let (labels, _) = render(renamed, 24_000, 256, Commands::default());
    assert_eq!(reference, labels);
    let mut phase = base.clone();
    phase.cylinders[1].firing_deg += 20.;
    let (different_phase, _) = render(phase, 24_000, 256, Commands::default());
    assert_ne!(reference, different_phase);
    let mut route = base;
    route
        .banks
        .push(Config::preset("four-split").unwrap().banks[1].clone());
    route.cylinders[1].bank = 1;
    let (different_route, _) = render(route, 24_000, 256, Commands::default());
    assert_ne!(reference, different_route);
}

#[test]
fn json_validation_and_range_stability() {
    let config = Config::preset("four-split").unwrap();
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
    let mut invalid = config.clone();
    invalid.cylinders[0].bank = 99;
    assert!(invalid.validate().is_err());
    invalid = config.clone();
    invalid.banks[0].sound_speed_m_s = f32::NAN;
    assert!(invalid.validate().is_err());
    invalid = config;
    invalid.version = 2;
    assert!(invalid.validate().is_err());
    for name in ["single", "four-even", "four-split"] {
        for rate in [8_000, 48_000, 192_000] {
            for rpm in [0., 200., 3_000., 12_000.] {
                let commands = Commands {
                    rpm,
                    ..Commands::default()
                };
                let mut synth = Synth::new(rate, Config::preset(name).unwrap(), commands).unwrap();
                for _ in 0..rate / 2 {
                    let sample = synth.next_sample();
                    assert!(sample.is_finite() && sample.abs() <= 0.98);
                }
            }
        }
    }
    let mut twelve = Synth::new(
        48_000,
        Config::even(12),
        Commands {
            rpm: 12_000.,
            ..Commands::default()
        },
    )
    .unwrap();
    for _ in 0..48_000 {
        let sample = twelve.next_sample();
        assert!(sample.is_finite() && sample.abs() <= 0.98);
    }
}
