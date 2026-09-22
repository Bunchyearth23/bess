use bess::{
    project::{self, Parameters},
    render,
};
use std::time::Instant;

#[test]
fn projects_roundtrip_and_reject_invalid_data() {
    let path = std::env::temp_dir().join(format!("bess-project-{}.json", std::process::id()));
    let p = Parameters::preset(2);
    project::save(&path, p).unwrap();
    assert_eq!(project::load(&path).unwrap(), p);
    std::fs::write(&path, r#"{"version":99,"parameters":{}}"#).unwrap();
    assert!(project::load(&path).is_err());
    std::fs::write(&path, r#"{"version":1,"parameters":{"rpm":-1}}"#).unwrap();
    assert!(project::load(&path).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn rendered_wav_has_expected_format_signal_and_duration() {
    let path = std::env::temp_dir().join(format!("bess-audio-{}.wav", std::process::id()));
    let start = Instant::now();
    render::wav(&path, Parameters::default(), 1., true).unwrap();
    let elapsed = start.elapsed();
    let mut wav = hound::WavReader::open(&path).unwrap();
    assert_eq!(wav.spec().sample_rate, 48000);
    assert_eq!(wav.spec().bits_per_sample, 24);
    assert_eq!(wav.spec().channels, 1);
    assert_eq!(wav.duration(), 48000);
    let samples: Vec<i32> = wav.samples().map(Result::unwrap).collect();
    assert!(samples.iter().any(|s| s.abs() > 10000));
    assert!(samples.iter().all(|s| s.abs() < 8388607));
    assert!(samples.last().unwrap().abs() < 10000);
    println!("One-second render: {elapsed:?}");
    std::fs::remove_file(path).unwrap();
}
