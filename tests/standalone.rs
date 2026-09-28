//! Compatibility data only. The retired standalone renderer no longer exists.
use bess::standalone::Config;

#[test]
fn saved_legacy_configuration_remains_readable_without_a_renderer() {
    for json in [
        include_str!("../presets/standalone/single.json"),
        include_str!("../presets/standalone/four-even.json"),
        include_str!("../presets/standalone/four-split.json"),
    ] {
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<Config>(&serde_json::to_string(&config).unwrap()).unwrap(),
            config
        );
    }
}

#[test]
fn malformed_legacy_configuration_is_not_accepted_during_migration() {
    let mut config = Config::even(4);
    config.cylinders[0].bank = 99;
    assert!(config.validate().is_err());
    config = Config::even(4);
    config.banks[0].sound_speed_m_s = f32::NAN;
    assert!(config.validate().is_err());
    config = Config::even(4);
    config.version = 2;
    assert!(config.validate().is_err());
}
