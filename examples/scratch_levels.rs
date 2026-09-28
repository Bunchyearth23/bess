//! Reproducible scratch listening levels, using the same transport as the GUI.
use bess::{
    bench::BeamNgCamera,
    drive::{Controls, Mode},
    engine_build::{Catalyst, Muffler},
    hybrid::Settings,
    project::Parameters,
    realtime::RenderEngine,
    scratch::{PRESETS, Scratch},
};

fn main() {
    println!("voice,build,condition,camera,volume,peak_dbfs,rms_dbfs");
    let stock_only = std::env::args().any(|arg| arg == "--stock-only");
    for name in ["stock-i4", "open-v12"] {
        if stock_only && name != "stock-i4" {
            continue;
        }
        let mut scratch = Scratch::default();
        if name == "open-v12" {
            scratch.design = PRESETS[13].1;
            scratch.build.muffler = Muffler::None;
            scratch.build.catalyst = Catalyst::None;
        }
        let mut settings = Settings::default();
        let mut p = Parameters {
            volume: 0.8,
            ..Parameters::default()
        };
        let mut controls = Controls {
            mode: Mode::Direct,
            ..Controls::default()
        };
        scratch.derive_from_build(&mut settings, &mut p, &mut controls);
        for (condition, rpm, load) in [("idle", scratch.idle_rpm, 0.05), ("loaded", 4000., 0.9)] {
            p.rpm = rpm;
            p.load = load;
            for volume in [0., 0.8, 1.] {
                p.volume = volume;
                for camera in [BeamNgCamera::Orbit, BeamNgCamera::Tailpipe] {
                    let mut engine =
                        RenderEngine::scratch(48_000, p, settings, controls, &scratch).unwrap();
                    engine.bench.set(p, settings, controls, 0);
                    engine.bench.set_beamng_camera(camera);
                    let mut peak = 0f32;
                    let mut power = 0f64;
                    for i in 0..144_000 {
                        let x = engine.next_sample(true);
                        assert!(x.is_finite());
                        if i >= 48_000 {
                            peak = peak.max(x.abs());
                            power += (x as f64).powi(2);
                        }
                    }
                    assert!(
                        !engine.bench.failed(),
                        "Physical solver failed for {name}/{condition}"
                    );
                    if volume == 0. {
                        assert_eq!(peak, 0.);
                        assert_eq!(power, 0.);
                    }
                    println!(
                        "Physical,{name},{condition},{camera:?},{},{:.2},{:.2}",
                        p.volume,
                        20. * peak.log10(),
                        10. * (power / 96_000.).log10()
                    );
                }
            }
        }
    }
}
