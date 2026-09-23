use bess::{
    bank::Bank,
    combustion::Combustion,
    hybrid::Settings,
    project::{Parameters, Project},
    render,
};
use std::{fs, path::Path, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("combustion_demo ZIP NEW_OUTPUT".into());
    }
    let out = Path::new(&args[2]);
    fs::create_dir(out)?;
    let bank = Arc::new(Bank::load(Path::new(&args[1]), None)?);
    let p = Parameters {
        rpm: bank.min_rpm,
        brightness: 10000.,
        exhaust: 1.,
        intake: 0.25,
        ..Default::default()
    };
    let base = Settings {
        level_match: false,
        ..Settings::calibrated(&bank)
    };
    let even = Settings {
        combustion: Combustion::even(4),
        ..base
    };
    let mut uneven = even;
    uneven.combustion.angles[1] = 140.;
    uneven.combustion.angles[3] = 500.;
    for (name, h) in [
        ("01-source-bank", base),
        ("02-four-even", even),
        ("03-four-uneven", uneven),
    ] {
        render::hybrid_wav(
            &out.join(format!("{name}.wav")),
            p,
            h,
            bank.clone(),
            16.,
            true,
        )?;
        bess::project::save_project(
            &out.join(format!("{name}.bess.json")),
            &Project {
                version: 3,
                parameters: p,
                hybrid: h,
                source: Some(bank.source.clone()),
                driving: bess::drive::Controls {
                    mode: bess::drive::Mode::Cycle,
                    ..Default::default()
                },
                profile_name: bess::project::default_profile_name(),
            },
        )?;
    }
    fs::write(
        out.join("README.txt"),
        "A control demonstration, not vehicle identification. The same sound bank and cycle are used with no events, then an explicit even four-cylinder configuration, then an uneven configuration. These configurations are examples entered by the user, not inferred properties of the imported engine. No separate normalization is applied.\n",
    )?;
    Ok(())
}
