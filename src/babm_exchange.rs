//! Versioned full-vehicle handoff to BABM. The source identity survives re-export.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub const MARKER_PATH: &str = "bess-export.json";
const MAX_MARKER_BYTES: u64 = 1_000_000;
const MAX_SOUND_BYTES: u64 = 16_000_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundReplacement {
    pub path: String,
    pub original_sha256: String,
    pub rendered_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExportMarker {
    pub version: u32,
    pub kind: String,
    pub source_archive_sha256: String,
    pub source_archive_name: String,
    pub vehicle_root: String,
    pub blend_path: String,
    pub exported_at_unix_ms: u64,
    pub sounds: Vec<SoundReplacement>,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn archive_hash(path: &Path) -> Result<String, String> {
    let mut source = File::open(path).map_err(|e| e.to_string())?;
    if source.metadata().map_err(|e| e.to_string())?.len() > 2_000_000_000 {
        return Err("Source archive exceeds the 2 GB handoff limit".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn archive_bytes(
    zip: &mut zip::ZipArchive<File>,
    path: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let entry = zip.by_name(path).map_err(|e| e.to_string())?;
    if entry.size() > limit {
        return Err(format!("Handoff member exceeds its size limit: {path}"));
    }
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("Handoff member exceeds its size limit: {path}"));
    }
    Ok(bytes)
}

/// Validate an inherited marker before keeping its original identity. Every
/// current WAV must still match the previous export; metadata alone is not proof.
pub(crate) fn prepare(
    source: &Path,
    zip: &mut zip::ZipArchive<File>,
    vehicle_root: &str,
    blend_path: &str,
    paths: impl Iterator<Item = String>,
) -> Result<ExportMarker, String> {
    let expected: BTreeSet<_> = paths.collect();
    if expected.is_empty() || expected.len() > 512 {
        return Err("Unsupported number of sound replacements for BABM".into());
    }
    let previous: Option<ExportMarker> = if zip.file_names().any(|name| name == MARKER_PATH) {
        Some(
            serde_json::from_slice(&archive_bytes(zip, MARKER_PATH, MAX_MARKER_BYTES)?)
                .map_err(|e| format!("Invalid previous BESS handoff: {e}"))?,
        )
    } else {
        None
    };
    let previous_sounds = if let Some(previous) = &previous {
        if previous.version != 1
            || previous.kind != "bess-full-vehicle"
            || previous.vehicle_root != vehicle_root
            || previous.blend_path != blend_path
            || !digest_valid(&previous.source_archive_sha256)
            || previous.sounds.len() != expected.len()
        {
            return Err(
                "Previous BESS handoff does not describe this vehicle and sound bank".into(),
            );
        }
        let sounds: BTreeMap<_, _> = previous
            .sounds
            .iter()
            .map(|sound| (sound.path.clone(), sound))
            .collect();
        if sounds.keys().cloned().collect::<BTreeSet<_>>() != expected {
            return Err("Previous BESS handoff has missing or duplicate sounds".into());
        }
        sounds
    } else {
        BTreeMap::new()
    };
    let mut sounds = Vec::new();
    for path in expected {
        let current_hash = sha256(&archive_bytes(zip, &path, MAX_SOUND_BYTES)?);
        let original_sha256 = if let Some(old) = previous_sounds.get(&path) {
            if old.rendered_sha256 != current_hash || !digest_valid(&old.original_sha256) {
                return Err(format!(
                    "Previously exported sound changed outside BESS: {path}"
                ));
            }
            old.original_sha256.clone()
        } else {
            current_hash
        };
        sounds.push(SoundReplacement {
            path,
            original_sha256,
            rendered_sha256: String::new(),
        });
    }
    let now: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "Clock exceeds the handoff timestamp range")?;
    let (source_archive_sha256, source_archive_name, exported_at_unix_ms) =
        if let Some(previous) = previous {
            (
                previous.source_archive_sha256,
                previous.source_archive_name,
                now.max(
                    previous
                        .exported_at_unix_ms
                        .checked_add(1)
                        .ok_or("Previous handoff timestamp is invalid")?,
                ),
            )
        } else {
            (
                archive_hash(source)?,
                source
                    .file_name()
                    .ok_or("Source archive has no filename")?
                    .to_string_lossy()
                    .into_owned(),
                now,
            )
        };
    Ok(ExportMarker {
        version: 1,
        kind: "bess-full-vehicle".into(),
        source_archive_sha256,
        source_archive_name,
        vehicle_root: vehicle_root.into(),
        blend_path: blend_path.into(),
        exported_at_unix_ms,
        sounds,
    })
}

impl ExportMarker {
    pub(crate) fn record_rendered(&mut self, path: &str, bytes: &[u8]) -> Result<(), String> {
        let sound = self
            .sounds
            .iter_mut()
            .find(|sound| sound.path == path)
            .ok_or_else(|| format!("Unexpected sound in handoff: {path}"))?;
        sound.rendered_sha256 = sha256(bytes);
        Ok(())
    }

    pub(crate) fn bytes(&self) -> Result<Vec<u8>, String> {
        if self
            .sounds
            .iter()
            .any(|sound| !digest_valid(&sound.rendered_sha256))
        {
            return Err("Incomplete BABM sound handoff".into());
        }
        serde_json::to_vec_pretty(self).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new(marker: &ExportMarker, payload: &[u8]) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let (path, file) = loop {
                let path = std::env::temp_dir().join(format!(
                    "bess-handoff-{}-{}.zip",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match File::create_new(&path) {
                    Ok(file) => break (path, file),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("{error}"),
                }
            };
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("art/sound/test.wav", options).unwrap();
            writer.write_all(payload).unwrap();
            writer.start_file(MARKER_PATH, options).unwrap();
            writer
                .write_all(&serde_json::to_vec(marker).unwrap())
                .unwrap();
            writer.finish().unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn tampered_or_ambiguous_previous_handoff_is_rejected_without_changing_source() {
        let marker = ExportMarker {
            version: 1,
            kind: "bess-full-vehicle".into(),
            source_archive_sha256: sha256(b"original zip"),
            source_archive_name: "original.zip".into(),
            vehicle_root: "vehicles/test/".into(),
            blend_path: "art/sound/blends/test.sfxBlend2D.json".into(),
            exported_at_unix_ms: 1,
            sounds: vec![SoundReplacement {
                path: "art/sound/test.wav".into(),
                original_sha256: sha256(b"original"),
                rendered_sha256: sha256(b"rendered"),
            }],
        };
        for scenario in 0..5 {
            let mut invalid = marker.clone();
            match scenario {
                0 => invalid.sounds[0].rendered_sha256 = sha256(b"different"),
                1 => invalid.sounds.push(invalid.sounds[0].clone()),
                2 => invalid.version = 2,
                3 => invalid.vehicle_root = "vehicles/other/".into(),
                _ => invalid.source_archive_sha256 = "invalid".into(),
            }
            let fixture = Fixture::new(&invalid, b"rendered");
            let before = archive_hash(&fixture.0).unwrap();
            let mut zip = zip::ZipArchive::new(File::open(&fixture.0).unwrap()).unwrap();
            assert!(
                prepare(
                    &fixture.0,
                    &mut zip,
                    &marker.vehicle_root,
                    &marker.blend_path,
                    std::iter::once("art/sound/test.wav".into())
                )
                .is_err()
            );
            assert_eq!(archive_hash(&fixture.0).unwrap(), before);
        }
    }
}
