use bess::{
    physical::{
        crank::TorsionConfig,
        engine::{Commands, Engine, Sample},
        structural::StructuralResponse,
    },
    scratch::Scratch,
};
#[test]
fn zero_turbine_map_capacity_keeps_turbo_topology_closed_under_positive_pressure() {
    use bess::{
        engine_build::{Aspiration, EngineBuild, EngineTuning},
        physical::{
            cylinder::Reservoir,
            induction::Induction,
            manifolds::{Flows, Manifolds},
            thermo::Mixture,
            turbo_map::TurboModel,
        },
    };
    let build = EngineBuild {
        aspiration: Aspiration::Turbo,
        ..Default::default()
    };
    let tuning = EngineTuning::default().resolve(&build, 4);
    let mut model = TurboModel::synthetic_example();
    model.turbine.values = [[0.0; 16]; 8];
    let mut induction = Induction::new(&build, &tuning, 0.002).unwrap();
    induction.set_model(Some(model)).unwrap();
    let area = induction
        .mapped_turbine_area_m2(Reservoir {
            pressure_pa: 220000.0,
            temperature_k: 1000.0,
        })
        .unwrap();
    assert_eq!(area, 0.0);
    let mut manifold = Manifolds::new(&build, &tuning, 4, 0.002).unwrap();
    manifold.set_active_bank_mask([true, false]).unwrap();
    manifold.set_turbine_area_m2(area).unwrap();
    let mut transported = 0.0;
    for _ in 0..1000 {
        let mut flows = Flows::default();
        flows.exhaust[0].mass_in_kg = 1e-8;
        flows.exhaust[0].enthalpy_in_j = 1e-8 * Mixture::PRODUCTS.enthalpy(1000.0);
        let s = manifold.step(1.0 / 96000.0, 0.0, 0.0, flows).unwrap();
        transported += s.exhaust_mass_flow_kg_s[0].abs() / 96000.0;
    }
    assert!(manifold.exhaust_bank(0).pressure_pa > 101325.0);
    assert_eq!(transported, 0.0);
}

fn assert_same_physics(a: Sample, b: Sample) {
    for (x, y) in [
        (a.rpm, b.rpm),
        (a.map_pa, b.map_pa),
        (a.torque_nm, b.torque_nm),
        (a.heat_j, b.heat_j),
        (a.correction_j, b.correction_j),
        (a.fuel_injected_kg, b.fuel_injected_kg),
        (a.turbo_rpm, b.turbo_rpm),
        (a.charge_pa, b.charge_pa),
    ] {
        assert_eq!(x.to_bits(), y.to_bits());
    }
    assert_eq!(a.exhaust.to_bits(), b.exhaust.to_bits());
    assert_eq!(a.intake.to_bits(), b.intake.to_bits());
    assert_eq!(a.fuel_cut, b.fuel_cut);
    assert_eq!(a.misfires, b.misfires);
}
#[test]
fn imposed_stand_with_torsion_remains_bit_identical_to_historical_rigid_stand() {
    for rate in [48000, 96000] {
        for rpm in [850.0, 3000.0] {
            let base = Scratch::default();
            let mut changed = base.clone();
            changed.experimental.torsion = Some(TorsionConfig::default());
            let mut a = Engine::new(&base, rate).unwrap();
            let mut b = Engine::new(&changed, rate).unwrap();
            for _ in 0..rate / 10 {
                let commands = Commands {
                    imposed_rpm: Some(rpm),
                    throttle: 0.35,
                    ..Default::default()
                };
                let x = a.next(commands);
                let y = b.next(commands);
                assert!(!a.failed() && !b.failed());
                assert_same_physics(x, y);
                assert_eq!(x.mechanical.to_bits(), y.mechanical.to_bits());
            }
        }
    }
}
#[test]
fn imported_radiation_changes_sound_but_keeps_free_crank_gas_and_port_audio_identical() {
    for rate in [48000, 96000] {
        let base = Scratch::default();
        let mut changed = base.clone();
        changed.experimental.mechanical_transfer = Some(StructuralResponse::synthetic_example());
        let mut a = Engine::new(&base, rate).unwrap();
        let mut b = Engine::new(&changed, rate).unwrap();
        let mut difference = 0.0;
        for i in 0..rate / 5 {
            let commands = Commands {
                imposed_rpm: if i < rate / 10 { Some(2000.0) } else { None },
                throttle: 0.35,
                load_nm: 10.0,
                ..Default::default()
            };
            let x = a.next(commands);
            let y = b.next(commands);
            assert!(!a.failed() && !b.failed());
            assert_same_physics(x, y);
            difference += (x.mechanical - y.mechanical).abs();
        }
        assert!(difference > 0.0);
    }
}
