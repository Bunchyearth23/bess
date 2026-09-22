use bess::{bank::Bank, hybrid::Settings, project::Parameters, render};
use std::{path::Path, sync::Arc};

fn level_variation(samples: &[f32]) -> f64 {
    let levels: Vec<f64> = samples[48_000..]
        .as_chunks::<2_400>()
        .0
        .iter()
        .map(|block| {
            (block.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / block.len() as f64).sqrt()
        })
        .collect();
    let mean = levels.iter().sum::<f64>() / levels.len() as f64;
    (levels.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / levels.len() as f64).sqrt() / mean
}

#[test]
fn cerberus_5200_has_no_excess_level_modulation_at_steady_load() {
    let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("cars/bunchyearth23_cerberus_a.zip");
    let bank = Arc::new(Bank::load(&archive, None).unwrap());
    let settings = Settings::calibrated(&bank);
    for load in [0.12, 1.0] {
        let params = Parameters {
            rpm: 5200.,
            load,
            brightness: 10_000.,
            exhaust: 1.,
            intake: 0.25,
            mechanical: 0.12,
            ..Parameters::default()
        };
        let source = render::hybrid_samples(
            params,
            Settings {
                enhanced: false,
                ..settings
            },
            bank.clone(),
            6.,
            false,
        )
        .unwrap();
        let bess = render::hybrid_samples(params, settings, bank.clone(), 6., false).unwrap();
        let source_cv = level_variation(&source);
        let bess_cv = level_variation(&bess);
        assert!(
            bess_cv < source_cv * 1.55,
            "5200 rpm load {load}: source CV {source_cv:.3}, BESS CV {bess_cv:.3}"
        );
    }
}
