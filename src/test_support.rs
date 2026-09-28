//! Small verified Automation ZIP fixture, shared by unit/integration tests.
use std::{
    io::{Cursor, Write},
    path::PathBuf,
};

pub fn automation_fixture() -> PathBuf {
    const UID: &str = "694E80154252F6189DE80988120C7F13";
    let path = std::env::temp_dir().join(format!(
        "bess-physical-fixture-{}-{}.zip",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    let mut layers = [Vec::new(), Vec::new()];
    for (load, layer) in layers.iter_mut().enumerate() {
        for rpm in [800, 4000] {
            let name = format!("art/sound/{load}-{rpm}.wav");
            let mut bytes = Cursor::new(Vec::new());
            let mut writer = hound::WavWriter::new(
                &mut bytes,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 48000,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                },
            )
            .unwrap();
            for frame in 0..48000 {
                let phase = std::f32::consts::TAU * frame as f32 * rpm as f32 / (120. * 48000.);
                writer
                    .write_sample(
                        (0.06 * (phase * 4.).sin() + 0.02 * (phase * 12.).sin())
                            * (0.5 + load as f32 * 0.5),
                    )
                    .unwrap();
            }
            writer.finalize().unwrap();
            zip.start_file(&name, options).unwrap();
            zip.write_all(&bytes.into_inner()).unwrap();
            layer.push(serde_json::json!([name, rpm]));
        }
    }
    zip.start_file(format!("art/sound/blends/{UID}.sfxBlend2D.json"), options)
        .unwrap();
    zip.write_all(
        serde_json::to_string(&serde_json::json!({"samples":layers}))
            .unwrap()
            .as_bytes(),
    )
    .unwrap();
    zip.start_file("vehicles/test/test.car", options).unwrap();
    zip.write_all(format!("do local _={{\n\tFamily={{\n\t\tUID=\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\n\t\tBlockType=\"EngBlock_Inl_Name\",\n\t\tBlockConfig=\"EngBlock_Inl4_Name\"\n\t}},\n\tVariant={{\n\t\tUID=\"{UID}\",\n\t\tFUID=\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\n\t\tCapacity=2,\n\t\tCompression=9,\n\t\tRPMLimit=6000,\n\t\tAspirationType=\"Aspiration_Natural_Name\"\n\t}}\n}}").as_bytes()).unwrap();
    zip.start_file("vehicles/test/test.pc", options).unwrap();
    zip.write_all(br#"{"parts":{"Camso_Engine":"Camso_Engine_694e8"},"vars":{"$idleRPM":800}}"#)
        .unwrap();
    zip.start_file("vehicles/test/eng_694e8/camso_engine_694e8.jbeam", options)
        .unwrap();
    zip.write_all(br#"{"Camso_Engine_694e8":{}}"#).unwrap();
    zip.start_file("vehicles/test/info.json", options).unwrap();
    zip.write_all(br#"{"Name":"Test Vehicle","paints":{"Blue":1,"Blue":2}}"#)
        .unwrap();
    zip.finish().unwrap();
    path
}
