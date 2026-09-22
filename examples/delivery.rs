//! Build all replacement mods and audit preservation of non-audio entries.
use bess::{bank::Bank, hybrid::Settings, project::Parameters};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, sync::Arc};
fn digest(mut r: impl Read) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = r.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize().to_vec())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("delivery CARS NEW_OUTPUT".into());
    }
    let out = Path::new(&args[2]);
    fs::create_dir(out)?;
    let mut archives: Vec<_> = fs::read_dir(&args[1])?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "zip"))
        .collect();
    archives.sort();
    let mut reports = Vec::new();
    for path in archives {
        let before = digest(fs::File::open(&path)?)?;
        let bank = Arc::new(Bank::load(&path, None)?);
        let h = Settings::calibrated(&bank);
        let p = Parameters {
            rpm: bank.min_rpm,
            load: 0.12,
            brightness: 10000.,
            exhaust: 1.,
            intake: 0.25,
            ..Default::default()
        };
        let dir = out.join(path.file_stem().ok_or("Name")?);
        println!("Export {}", path.display());
        bess::export::package(&dir, p, h, bank.clone())?;
        assert_eq!(before, digest(fs::File::open(&path)?)?, "Source modified");
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("manifest.json"))?)?;
        let loops = manifest["loops"].as_array().ok_or("Loops")?;
        let mut original = zip::ZipArchive::new(fs::File::open(&path)?)?;
        let zip_name = manifest["zip_file"].as_str().ok_or("ZIP name")?;
        let mut exported = zip::ZipArchive::new(fs::File::open(dir.join(zip_name))?)?;
        assert_eq!(original.len(), exported.len());
        let mut preserved = 0;
        let mut labelled = 0;
        let mut seam_ratio = 0f32;
        for i in 0..original.len() {
            let entry = original.by_index(i)?;
            let mut target = exported.by_name(entry.name())?;
            if loops.iter().any(|l| l["path"] == entry.name()) {
                let mut bytes = Vec::new();
                target.read_to_end(&mut bytes)?;
                let (rate, pcm) = bess::bank::decode_wav(&bytes)?;
                assert_eq!(rate, 48000);
                let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
                let delta_rms = (pcm.windows(2).map(|x| (x[1] - x[0]).powi(2)).sum::<f32>()
                    / (pcm.len() - 1) as f32)
                    .sqrt();
                let seam = (pcm[0] - pcm[pcm.len() - 1]).abs();
                assert!(rms > 1e-6 && pcm.iter().all(|x| x.is_finite() && x.abs() <= 0.951));
                assert!(
                    seam < 8. * delta_rms.max(1e-6),
                    "Abnormal seam {}",
                    entry.name()
                );
                seam_ratio = seam_ratio.max(seam / delta_rms.max(1e-6));
            } else if entry.name()
                == manifest["display_name_path"]
                    .as_str()
                    .ok_or("Display name path")?
            {
                let mut source = Vec::new();
                let mut actual = Vec::new();
                entry.take(1_000_001).read_to_end(&mut source)?;
                target.take(1_000_001).read_to_end(&mut actual)?;
                let (expected, display_name) = bess::export::label_vehicle_info(&source)?;
                assert_eq!(actual, expected, "Unexpected vehicle metadata change");
                assert_eq!(manifest["display_name"], display_name);
                assert!(display_name.ends_with(" (BESS)"));
                labelled += 1;
            } else {
                assert_eq!(digest(entry)?, digest(target)?);
                preserved += 1;
            }
        }
        assert_eq!(labelled, 1, "Expected one labelled vehicle metadata file");
        reports.push(serde_json::json!({"archive":path,"display_name":manifest["display_name"],"loops":loops.len(),"labelled_metadata_files":labelled,"unchanged_entries":preserved,"source_unchanged":true,"max_seam_over_delta_rms":seam_ratio}));
        fs::write(
            out.join("verification.json"),
            serde_json::to_vec_pretty(&reports)?,
        )?;
        println!("OK: {} loops; {} entries preserved", loops.len(), preserved);
    }
    Ok(())
}
