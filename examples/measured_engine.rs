//! Offline P1 reference comparison and compression-ratio identification.
//! measured_engine <output-dir> [reference.json]
//! Without input the generated reference is explicitly synthetic.
//! `--gx160` uses public Honda ratings for comparison only, preserving known geometry.
use bess::{
    engine_build::{Catalyst, Fuel, Head},
    engine_definition::EngineDefinition,
    measured_reference::{
        self as reference, Acceptance, Channels, Origin, Prediction, Provenance, ReferenceSet,
        Split, StationaryPoint,
    },
    physical::engine::{Commands, Engine},
    realtime::DenormalGuard,
    scratch::{PRESETS, Scratch},
};
use std::{fmt::Write as _, path::Path};

fn predict(
    definition: EngineDefinition,
    points: &[StationaryPoint],
) -> Result<Vec<Prediction>, String> {
    let mut result = Vec::new();
    for point in points {
        let scratch = definition.to_scratch();
        let mut engine = Engine::new(&scratch, 96_000)?;
        let settle = (point.settle_s * 96_000.).round() as usize;
        let frames = (point.average_s * 96_000.).round() as usize;
        let (mut torque, mut map) = (0., 0.);
        for i in 0..settle + frames {
            let sample = engine.next(Commands {
                imposed_rpm: Some(point.rpm),
                throttle: point.throttle,
                ..Commands::default()
            });
            if engine.failed() {
                return Err(format!("Physical engine failed at {}", point.id));
            }
            if i >= settle {
                torque += sample.torque_nm - engine.friction_nm();
                map += sample.map_pa;
            }
        }
        result.push(Prediction {
            id: point.id.clone(),
            channels: Channels {
                torque_nm: Some(torque / frames as f64),
                map_pa: Some(map / frames as f64),
                lambda: None,
            },
        });
    }
    Ok(result)
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), String> {
    std::fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn gx160() -> ReferenceSet {
    let mut scratch = Scratch {
        design: PRESETS[0].1,
        ..Scratch::default()
    };
    scratch.build.bore_mm = 68.;
    scratch.build.stroke_mm = 45.;
    scratch.build.compression = 9.;
    scratch.build.head = Head::Pushrod;
    scratch.build.valves = 2;
    scratch.build.fuel = Fuel::Carburettor;
    scratch.build.vvt = false;
    scratch.build.cam = 0.15; // Estimated, not a Honda cam measurement.
    scratch.build.exhaust_mm = 35.; // Smallest supported diameter, not measured.
    scratch.build.catalyst = Catalyst::None;
    scratch.idle_rpm = 1400.; // Bench limits only, not a published Honda idle/redline.
    scratch.redline_rpm = 4000.;
    scratch.apply_design();
    ReferenceSet {
        version:1,
        engine:EngineDefinition::from_scratch(&scratch),
        provenance:Provenance {
            origin:Origin::ManufacturerPublished,
            engine_id:"Honda GX160, current EU net ratings".into(),
            source_uri:"https://www.honda-engines-eu.com/en/products/power-units/gx160".into(),
            instrument_and_conditions: "Consulted 2026-10-11: production-engine net ratings, SAE J1349. No raw dynamometer uncertainty, throttle, temperatures, MAP, lambda or audio supplied. WOT=1 is a bench assumption. 3600-rpm torque derived as 3600 W / (3600 rpm * 2pi/60). Bore/stroke/compression/OHV/carburated single fixed. Cam=.15, exhaust=35 mm (model minimum), generic manifolds/valves/walls/friction, muffler, inertia, idle=1400/redline=4000 are estimates. No compression fitting to published ratings.".into(),
        },
        acceptance:Acceptance {
            declared_before_fitting:"Before first GX160 run: exploratory 10% of the 10.3 Nm peak = 1.03 Nm absolute at both points. Engineering target, not Honda uncertainty. Comparison-only, no identification performed.".into(),
            absolute:Channels {torque_nm:Some(1.03), ..Channels::default()},
        },
        points:[(2500.,10.3,Split::Identification),(3600.,3600./(3600.*std::f64::consts::TAU/60.),Split::Validation)]
            .into_iter().map(|(rpm,torque,split)|StationaryPoint {
                id:format!("gx160-net-{rpm:.0}"), split,rpm,throttle:1.,settle_s:2.,average_s:2.,
                observed:Channels {torque_nm:Some(torque), ..Channels::default()},
            }).collect(),
    }
}

fn run(dir: &Path, input: Option<&Path>) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let set = if input == Some(Path::new("--gx160")) {
        gx160()
    } else if let Some(path) = input {
        ReferenceSet::load(path)?
    } else {
        let mut engine = EngineDefinition::default();
        engine.build.compression = 10.;
        let mut points: Vec<_> = [
            (1500., 0.3, Split::Identification),
            (3000., 0.7, Split::Identification),
            (4500., 0.9, Split::Validation),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (rpm, throttle, split))| StationaryPoint {
            id: format!("point-{i}"),
            split,
            rpm,
            throttle,
            settle_s: 0.2,
            average_s: 0.15,
            observed: Channels::default(),
        })
        .collect();
        let predictions = predict(engine, &points)?;
        for (point, prediction) in points.iter_mut().zip(predictions) {
            point.observed = prediction.channels;
        }
        // Start from a different physical compression ratio; reserved points are not used by the objective.
        engine.build.compression = 9.;
        ReferenceSet {version:1,engine,provenance:Provenance {origin:Origin::Synthetic,
            engine_id:"BESS synthetic stock I4".into(),source_uri:"synthetic:BESS-physical-engine".into(),
            instrument_and_conditions:"96 kHz imposed-RPM simulation; not measured hardware; short windows for tool demonstration".into()},
            acceptance:Acceptance {declared_before_fitting:"Tool fixture contract declared before search".into(),
                absolute:Channels {torque_nm:Some(1.),map_pa:Some(500.),lambda:None}},points}
    };
    set.validate()?;
    write_json(&dir.join("reference.json"), &set)?;
    let baseline = predict(set.engine, &set.points)?;
    write_json(
        &dir.join("before.json"),
        &reference::compare(&set, &baseline)?,
    )?;
    let mut fitted = set.engine;
    // Public ratings are compared without modifying known measured geometry.
    // The synthetic fixture demonstrates bounded identification independently.
    if set.provenance.origin != Origin::ManufacturerPublished {
        let result = reference::identify(
            &set,
            &[(7., 13.)],
            &[f64::from(set.engine.build.compression)],
            24,
            |parameters, points| {
                let mut definition = set.engine;
                definition.build.compression = parameters[0] as f32;
                predict(definition, points)
            },
        )?;
        write_json(&dir.join("identification.json"), &result)?;
        fitted.build.compression = result.parameters[0] as f32;
        write_json(&dir.join("identified-engine.json"), &fitted)?;
        println!(
            "Identification: compression {:.5}; loss {:.6}; reserved pass {}; measured qualification {}",
            result.parameters[0],
            result.identification_loss,
            result.reserved.validation_passed,
            result.reserved.measured_qualification
        );
    } else {
        write_json(&dir.join("unfitted-engine.json"), &fitted)?;
    }
    let mut sensitivity = String::from("cam,point_id,brake_torque_nm,map_pa\n");
    for offset in [-0.1, 0., 0.1] {
        let mut definition = fitted;
        definition.build.cam = (definition.build.cam + offset).clamp(0., 1.);
        for prediction in predict(definition, &set.points)? {
            writeln!(
                sensitivity,
                "{},{},{},{}",
                definition.build.cam,
                prediction.id,
                prediction.channels.torque_nm.unwrap(),
                prediction.channels.map_pa.unwrap()
            )
            .map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(dir.join("sensitivity.csv"), sensitivity).map_err(|e| e.to_string())?;
    // Constant-RPM exhaust orders; no guess about cylinder number is needed by the analyzer.
    let rpm = 3000.;
    let mut engine = Engine::new(&fitted.to_scratch(), 48_000)?;
    let mut samples = Vec::with_capacity(48_000);
    for i in 0..60_000 {
        let sample = engine.next(Commands {
            imposed_rpm: Some(rpm),
            throttle: 0.7,
            ..Default::default()
        });
        if engine.failed() {
            return Err("Order-analysis engine failed".into());
        }
        if i >= 12_000 {
            samples.push(sample.exhaust);
        }
    }
    let orders = [0.5, 1., 1.5, 2., 4., 6., 8.];
    let amplitudes = reference::order_amplitudes(&samples, 48_000, rpm, &orders)?;
    write_json(
        &dir.join("exhaust-orders.json"),
        &serde_json::json!({"rpm":rpm,"orders":orders,
        "amplitude":amplitudes,"origin":"synthetic","presentation_gain":1}),
    )?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav =
        hound::WavWriter::create(dir.join("exhaust.wav"), spec).map_err(|e| e.to_string())?;
    for sample in samples {
        wav.write_sample(sample).map_err(|e| e.to_string())?;
    }
    wav.finalize().map_err(|e| e.to_string())?;
    println!(
        "Reference {:?}; output {}",
        set.provenance.origin,
        dir.display()
    );
    Ok(())
}

fn main() -> Result<(), String> {
    let _guard = DenormalGuard::enter();
    let args: Vec<_> = std::env::args().collect();
    let dir = Path::new(
        args.get(1)
            .map(String::as_str)
            .unwrap_or("output/simulation-parallel-20261011/p1"),
    );
    run(dir, args.get(2).map(Path::new))
}
