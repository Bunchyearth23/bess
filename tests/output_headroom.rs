use bess::{
    bench::BeamNgCamera,
    drive::{Controls, Mode},
    hybrid::Settings,
    project::Parameters,
    realtime::RenderEngine,
    scratch::Scratch,
};

#[test]
fn stock_muffled_engine_at_eighty_percent_keeps_idle_audible_without_hard_clipping() {
    let mut scratch = Scratch::default();
    let mut params = Parameters {
        volume: 0.8,
        ..Default::default()
    };
    let mut settings = Settings::default();
    let mut controls = Controls {
        mode: Mode::Direct,
        ..Default::default()
    };
    scratch.derive_from_build(&mut settings, &mut params, &mut controls);
    params.rpm = 850.;
    params.load = 0.05;
    let mut engine = RenderEngine::scratch(48000, params, settings, controls, &scratch).unwrap();
    engine.bench.set_beamng_camera(BeamNgCamera::Orbit);
    let mut ceiling_samples = 0;
    let mut idle_energy = 0.;
    let mut idle_frames = 0;
    let mut peak = 0_f32;
    for frame in 0..384000 {
        let time = frame as f32 / 48000.;
        if frame % 48 == 0 {
            (params.rpm, params.load) = if time < 2. {
                (850., 0.05)
            } else if time < 4. {
                (850. + (3500. - 850.) * (time - 2.) / 2., 0.75)
            } else if time < 5. {
                (3500., 0.75)
            } else if time < 7. {
                (3500. + (850. - 3500.) * (time - 5.) / 2., 0.)
            } else {
                (850., 0.05)
            };
            engine.bench.set(params, settings, controls, 0);
        }
        let sample = engine.next_sample(true);
        assert!(sample.is_finite() && sample.abs() <= 0.891);
        assert!(!engine.bench.failed());
        peak = peak.max(sample.abs());
        ceiling_samples += usize::from(sample.abs() >= 0.891 - 1e-7);
        if (48000..96000).contains(&frame) {
            idle_energy += f64::from(sample).powi(2);
            idle_frames += 1;
        }
    }
    assert_eq!(
        ceiling_samples, 0,
        "normal acceleration must not use the hard safety clamp"
    );
    assert!(
        peak > 0.25,
        "do not solve clipping by making the whole engine inaudible"
    );
    let idle_db = 10. * (idle_energy / f64::from(idle_frames)).log10();
    assert!(
        idle_db > -35.,
        "stock idle became too quiet: {idle_db} dBFS"
    );
}

#[test]
fn scratch_render_rejects_solver_failure_instead_of_exporting_silence() {
    let result = bess::render::scratch_samples(
        bess::project::Parameters {
            rpm: 13000.,
            ..Default::default()
        },
        bess::hybrid::Settings::default(),
        &bess::scratch::Scratch::default(),
        1.,
        bess::drive::Controls {
            mode: bess::drive::Mode::Direct,
            ..Default::default()
        },
    );
    assert!(result.is_err());
}
