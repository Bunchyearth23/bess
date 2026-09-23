//! Same RPM and controls within each pair: only the map changes.
use bess::{
    bank::Bank,
    drive::{Controls, Mode},
    hybrid::Settings,
    maps::Map,
    project::{self, Parameters, Project},
    render,
};
use std::{path::Path, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("Usage: maps_demo ZIP NEW_OUTPUT".into());
    }
    let out = Path::new(&args[2]);
    std::fs::create_dir(out)?;
    let bank = Arc::new(Bank::load(Path::new(&args[1]), None)?);
    let neutral = Settings {
        level_match: false,
        ..Default::default()
    };
    let mut mapped = neutral;
    mapped.maps.pulse = Map([[1., 1., 1.], [1., 0.8, 0.9], [1., 0.5, 0.7]]);
    mapped.maps.texture = Map([[1., 1., 1.], [1., 1.3, 1.2], [1., 1.8, 1.5]]);
    mapped.maps.intake = Map([[1., 1., 1.], [1., 1.3, 1.2], [1., 1.7, 1.5]]);
    mapped.maps.exhaust = Map([[1., 1., 1.], [1., 0.85, 0.9], [1., 0.55, 0.7]]);
    for (label, load) in [("low-load", 0.), ("full-load", 1.)] {
        let p = Parameters {
            rpm: (bank.min_rpm + bank.max_rpm) * 0.5,
            load,
            brightness: 10000.,
            exhaust: 1.,
            intake: 0.25,
            ..Default::default()
        };
        let mut pair = Vec::new();
        for (variant, h) in [("neutral", neutral), ("mapped", mapped)] {
            let audio = render::hybrid_samples(p, h, bank.clone(), 6., false)?;
            render::write_pcm(&out.join(format!("{label}-{variant}.wav")), &audio)?;
            project::save_project(
                &out.join(format!("{label}-{variant}.bess.json")),
                &Project {
                    version: 3,
                    parameters: p,
                    hybrid: h,
                    source: Some(bank.source.clone()),
                    driving: Controls {
                        mode: Mode::Direct,
                        ..Default::default()
                    },
                    profile_name: bess::project::default_profile_name(),
                },
            )?;
            pair.push(audio);
        }
        let difference = (pair[0]
            .iter()
            .zip(&pair[1])
            .map(|(a, b)| (*a as f64 - *b as f64).powi(2))
            .sum::<f64>()
            / pair[0].len() as f64)
            .sqrt();
        if load == 0. && difference != 0. {
            return Err("Unrelated low-load region changed".into());
        }
        if load == 1. && difference < 0.0001 {
            return Err("Map has no measurable effect".into());
        }
        println!("{label}: RMS difference {difference:.8}");
    }
    std::fs::write(
        out.join("README.txt"),
        "The same mid-range RPM is used at low load and then full load. Only the maps change within each pair. At low load the outputs should match exactly. At full load, pulse and exhaust levels are reduced while texture and intake are increased. No level normalization or A/B compensation is applied, preserving the effect of the gains. Open the projects in BESS to reproduce and edit the maps.",
    )?;
    Ok(())
}
