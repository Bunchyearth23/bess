//! Bounded discovery of Automation source archives; this is not an import validator.
//!
//! The mods directory, its direct `repo` child and manifest-listed originals in
//! `.babm_backup/<group>` are visited. Archive payloads stay compressed, except for
//! at most 64 KiB of display metadata. Ordinary single-disk ZIPs are supported;
//! ZIP64/multipart archives are deliberately skipped.
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleArchive {
    pub path: PathBuf,
    pub name: String,
    /// Recognized grouped archives remain visible, but the existing single-
    /// engine import must not be offered for them.
    pub unavailable_reason: Option<String>,
}

const MAX_DIRECTORY_ENTRIES: usize = 4096;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_CENTRAL_DIRECTORY: usize = 8 * 1024 * 1024;
const MAX_INFO_BYTES: u64 = 64 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

struct ArchivePath {
    path: PathBuf,
    hidden_backup: bool,
}

/// Find likely Automation originals without loading their audio. Invalid,
/// unsupported, unrelated and recognized BESS add-on archives are ignored.
/// A directory access error or exceeded directory bound is reported rather than
/// returning an apparently complete, arbitrarily truncated library.
pub fn scan_automation_archives(mods_dir: &Path) -> Result<Vec<VehicleArchive>, String> {
    let metadata = fs::symlink_metadata(mods_dir)
        .map_err(|error| format!("Cannot read mods directory {}: {error}", mods_dir.display()))?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err("The mods path must be a regular directory, not a link".into());
    }
    let mut paths = Vec::new();
    scan_directory(mods_dir, &mut paths)?;
    let repo = mods_dir.join("repo");
    match fs::symlink_metadata(&repo) {
        Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => {
            scan_directory(&repo, &mut paths)?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Cannot read {}: {error}", repo.display())),
    }
    scan_babm_backups(mods_dir, &mut paths)?;
    // Prefer an active original, then its disabled sibling, then the hidden
    // BABM preservation copy. Content identity ignores ZIP compression/order.
    paths.sort_by_cached_key(|candidate| {
        (
            candidate.hidden_backup,
            is_grouping_backup(&candidate.path),
            candidate.path.clone(),
        )
    });
    let mut fingerprints = BTreeSet::new();
    let mut vehicles: Vec<_> = paths
        .into_iter()
        .filter_map(|candidate| inspect_archive(candidate.path, candidate.hidden_backup))
        .filter_map(|(vehicle, fingerprint)| fingerprints.insert(fingerprint).then_some(vehicle))
        .collect();
    vehicles.sort_by_cached_key(|vehicle| {
        (
            vehicle.unavailable_reason.is_some(),
            vehicle.name.to_lowercase(),
            vehicle.name.clone(),
            vehicle.path.clone(),
        )
    });
    Ok(vehicles)
}

fn is_link(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    // Windows directory junctions are reparse points too, but need not be
    // reported as symbolic links. Neither kind should extend this scan.
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn scan_directory(directory: &Path, paths: &mut Vec<ArchivePath>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Cannot list {}: {error}", directory.display()))?;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(format!(
                "Too many entries in {} (limit {MAX_DIRECTORY_ENTRIES})",
                directory.display()
            ));
        }
        let entry = entry.map_err(|error| format!("Cannot read directory entry: {error}"))?;
        let path = entry.path();
        let is_zip = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
        if !is_zip && !is_grouping_backup(&path) {
            continue;
        }
        // A disappearing/unreadable file is an individual archive failure.
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() && !is_link(&metadata) {
            paths.push(ArchivePath {
                path,
                hidden_backup: false,
            });
        }
    }
    Ok(())
}

fn plain_filename(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
        && !name.ends_with(['.', ' '])
}

fn scan_babm_backups(mods_dir: &Path, paths: &mut Vec<ArchivePath>) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        original_files: Vec<Backup>,
    }
    #[derive(serde::Deserialize)]
    struct Backup {
        backup_filename: String,
    }

    let directory = mods_dir.join(".babm_backup");
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Cannot read {}: {error}", directory.display())),
    };
    if !metadata.is_dir() || is_link(&metadata) {
        return Ok(());
    }
    let entries = fs::read_dir(&directory)
        .map_err(|error| format!("Cannot list {}: {error}", directory.display()))?;
    let mut original_count = 0usize;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_DIRECTORY_ENTRIES {
            return Err(format!(
                "Too many BABM backup groups (limit {MAX_DIRECTORY_ENTRIES})"
            ));
        }
        let entry = entry.map_err(|error| format!("Cannot read BABM backup group: {error}"))?;
        let group = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&group) else {
            continue;
        };
        if !metadata.is_dir() || is_link(&metadata) {
            continue;
        }
        let manifest_path = group.join("manifest.json");
        let Ok(metadata) = fs::symlink_metadata(&manifest_path) else {
            continue;
        };
        if !metadata.is_file() || is_link(&metadata) || metadata.len() > MAX_MANIFEST_BYTES {
            continue;
        }
        let Ok(file) = File::open(manifest_path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_MANIFEST_BYTES
        {
            continue;
        }
        let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
            continue;
        };
        original_count += manifest.original_files.len();
        if original_count > MAX_DIRECTORY_ENTRIES {
            return Err(format!(
                "Too many BABM backup originals (limit {MAX_DIRECTORY_ENTRIES})"
            ));
        }
        for backup in manifest.original_files {
            // Never use original_path from a manifest: it may refer outside the
            // selected mods tree. Only direct files inside this group qualify.
            if !plain_filename(&backup.backup_filename) {
                continue;
            }
            let path = group.join(backup.backup_filename);
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
                && !is_grouping_backup(&path)
            {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_file() && !is_link(&metadata) {
                paths.push(ArchivePath {
                    path,
                    hidden_backup: true,
                });
            }
        }
    }
    Ok(())
}

struct Member {
    name: String,
    crc32: u32,
    local_offset: u64,
    compressed_size: u64,
    size: u64,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// Parse only a bounded central directory. In particular, do not let an
/// untrusted ZIP64 entry count cause an allocation in ZipArchive::new before
/// checking our limits. All slices below are preceded by fixed-header checks.
fn central_directory(file: &mut File) -> Option<Vec<Member>> {
    let length = file.metadata().ok()?.len();
    let tail_size = length.min(22 + u16::MAX as u64) as usize;
    if tail_size < 22 {
        return None;
    }
    file.seek(SeekFrom::End(-(tail_size as i64))).ok()?;
    let mut tail = vec![0; tail_size];
    file.read_exact(&mut tail).ok()?;
    let end = (0..=tail.len() - 22).rev().find(|&index| {
        tail[index..].starts_with(b"PK\x05\x06")
            && index + 22 + u16_at(&tail, index + 20) as usize == tail.len()
    })?;
    let eocd = &tail[end..];
    let count = u16_at(eocd, 10) as usize;
    let directory_size = u32_at(eocd, 12) as usize;
    let directory_offset = u32_at(eocd, 16) as u64;
    let end_offset = length - tail_size as u64 + end as u64;
    if u16_at(eocd, 4) != 0
        || u16_at(eocd, 6) != 0
        || u16_at(eocd, 8) as usize != count
        || count == 0
        || count > MAX_ARCHIVE_ENTRIES
        || directory_size > MAX_CENTRAL_DIRECTORY
        || directory_offset + directory_size as u64 != end_offset
    {
        return None;
    }
    file.seek(SeekFrom::Start(directory_offset)).ok()?;
    let mut directory = vec![0; directory_size];
    file.read_exact(&mut directory).ok()?;
    let mut members = Vec::with_capacity(count);
    let mut index = 0_usize;
    for _ in 0..count {
        let header = directory.get(index..index.checked_add(46)?)?;
        if !header.starts_with(b"PK\x01\x02") || u16_at(header, 34) != 0 {
            return None;
        }
        let name_len = u16_at(header, 28) as usize;
        let extra_len = u16_at(header, 30) as usize;
        let comment_len = u16_at(header, 32) as usize;
        let record_end = index + 46 + name_len + extra_len + comment_len;
        directory.get(index..record_end)?;
        let name = std::str::from_utf8(directory.get(index + 46..index + 46 + name_len)?)
            .ok()?
            .to_owned();
        let compressed_size = u32_at(header, 20) as u64;
        let size = u32_at(header, 24) as u64;
        let local_offset = u32_at(header, 42) as u64;
        if compressed_size == u32::MAX as u64
            || size == u32::MAX as u64
            || local_offset >= directory_offset
        {
            return None;
        }
        members.push(Member {
            name,
            crc32: u32_at(header, 16),
            local_offset,
            compressed_size,
            size,
        });
        index = record_end;
    }
    (index == directory.len()).then_some(members)
}

fn normalized_member(name: &str) -> Option<String> {
    if name.contains('\\')
        || name.contains(':')
        || name.chars().any(char::is_control)
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return None;
    }
    Some(name.to_ascii_lowercase())
}

fn is_grouping_backup(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".zip.merged_backup"))
}

fn vehicle_root(name: &str) -> Option<String> {
    let rest = name.strip_prefix("vehicles/")?;
    let (vehicle, _) = rest.split_once('/')?;
    Some(format!("vehicles/{vehicle}/"))
}

fn inspect_archive(path: PathBuf, hidden_backup: bool) -> Option<(VehicleArchive, [u8; 32])> {
    let mut file = File::open(&path).ok()?;
    let members = central_directory(&mut file)?;
    let names: Vec<_> = members
        .iter()
        .filter_map(|member| normalized_member(&member.name))
        .collect();
    // Both replacement exports and original-plus-BESS-variant vehicle ZIPs
    // must not displace an untouched original in the reference library.
    if names.iter().any(|name| name == "bess-export.json") {
        return None;
    }
    let blend_count = names
        .iter()
        .filter(|name| name.starts_with("art/sound/blends/") && name.ends_with(".sfxblend2d.json"))
        .count();
    // Recognize the additive layout written by variant.rs, never the ZIP's
    // filename: a legitimate source can itself be named "bess".
    let has_bess_engine = names.iter().any(|name| {
        name.starts_with("vehicles/")
            && name
                .rsplit('/')
                .next()
                .is_some_and(|base| base.starts_with("bess_engine_") && base.ends_with(".jbeam"))
    });
    let has_bess_config = names.iter().any(|name| {
        name.starts_with("vehicles/")
            && name
                .rsplit('/')
                .next()
                .is_some_and(|base| base.starts_with("bess_") && base.ends_with(".pc"))
    });
    let has_bess_blend = names.iter().any(|name| {
        name.starts_with("art/sound/blends/")
            && name.contains("_bess_")
            && name.ends_with(".sfxblend2d.json")
    });
    if blend_count == 0 || (has_bess_engine && has_bess_config && has_bess_blend) {
        return None;
    }
    let engine_roots: BTreeSet<_> = names
        .iter()
        .filter_map(|name| {
            let base = name.rsplit('/').next()?;
            if (base.starts_with("camso_engine_") || base.starts_with("enginecamso"))
                && base.ends_with(".jbeam")
            {
                vehicle_root(name)
            } else {
                None
            }
        })
        .collect();
    let roots: BTreeSet<_> = names
        .iter()
        .filter_map(|name| {
            let rest = name.strip_prefix("vehicles/")?;
            let (vehicle, leaf) = rest.split_once('/')?;
            if leaf.contains('/') || !leaf.ends_with(".car") {
                return None;
            }
            let root = format!("vehicles/{vehicle}/");
            engine_roots.contains(&root).then_some(root)
        })
        .collect();
    if roots.is_empty() {
        return None;
    }
    let backup = is_grouping_backup(&path);
    let fallback = if backup {
        let filename = path.file_name()?.to_str()?;
        filename[..filename.len() - ".zip.merged_backup".len()].to_owned()
    } else {
        path.file_stem()?.to_string_lossy().into_owned()
    };
    let mut name = if roots.len() == 1 {
        let info_path = format!("{}info.json", roots.first()?);
        let mut info = members
            .iter()
            .filter(|member| member.name.eq_ignore_ascii_case(&info_path));
        let first = info.next();
        // Duplicated case-insensitive metadata paths are not a reliable label.
        if info.next().is_none() {
            first
                .and_then(|member| read_name(&mut file, member))
                .unwrap_or(fallback)
        } else {
            fallback
        }
    } else {
        format!("{fallback} ({} véhicules — archive fusionnée)", roots.len())
    };
    if backup || hidden_backup {
        name.push_str(" (original before grouping)");
    }
    let unavailable_reason = (roots.len() > 1 || blend_count > 1).then(|| {
        format!(
            "Grouped archive: {} vehicle(s), {blend_count} audio blends. Import an individual original instead.",
            roots.len()
        )
    });
    let mut signature_members: Vec<_> = members.iter().collect();
    signature_members.sort_by(|left, right| left.name.cmp(&right.name));
    let mut signature = Sha256::new();
    for member in signature_members {
        signature.update((member.name.len() as u64).to_le_bytes());
        signature.update(member.name.as_bytes());
        signature.update(member.crc32.to_le_bytes());
        signature.update(member.size.to_le_bytes());
    }
    Some((
        VehicleArchive {
            path,
            name,
            unavailable_reason,
        },
        signature.finalize().into(),
    ))
}

fn read_name(file: &mut File, member: &Member) -> Option<String> {
    if member.size > MAX_INFO_BYTES || member.compressed_size > MAX_INFO_BYTES {
        return None;
    }
    file.seek(SeekFrom::Start(member.local_offset)).ok()?;
    // Local header + maximum filename/extra fields + compressed metadata.
    // The bound also covers the streaming ZIP reader's drain-on-drop behavior.
    let mut limited = file.take(30 + 2 * u16::MAX as u64 + MAX_INFO_BYTES);
    let entry = zip::read::read_zipfile_from_stream(&mut limited).ok()??;
    if entry.name() != member.name
        || entry.size() != member.size
        || entry.compressed_size() != member.compressed_size
    {
        return None;
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_INFO_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 != member.size {
        return None;
    }
    // Deriving a one-field struct rejects duplicate Name fields, while unrelated
    // metadata (including duplicate paint names in actual exports) is ignored.
    #[derive(serde::Deserialize)]
    struct DisplayInfo {
        #[serde(rename = "Name")]
        name: String,
    }
    let info: DisplayInfo = serde_json::from_slice(&bytes).ok()?;
    let name = info.name.trim();
    if name.is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
        return None;
    }
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "bess-library-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot reserve test directory: {error}"),
                }
            }
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn archive(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn original(path: &Path, info: &[u8]) {
        archive(
            path,
            &[
                ("vehicles/test/test.car", b"Automation metadata"),
                ("vehicles/test/eng_694e8/camso_engine_694e8.jbeam", b"{}"),
                (
                    "art/sound/blends/694E80154252F6189DE80988120C7F13.sfxBlend2D.json",
                    b"{}",
                ),
                ("vehicles/test/info.json", info),
                // Deliberately not a WAV: discovery must never decode audio.
                ("art/sound/test.wav", b"not an audio stream"),
            ],
        );
    }

    fn backup_manifest(mods: &Path, group: &str, filenames: &[&str]) -> PathBuf {
        let directory = mods.join(".babm_backup").join(group);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "chassis_name": group,
                "original_files": filenames.iter().map(|filename| serde_json::json!({
                    "backup_filename": filename,
                    "original_filename": filename,
                    // Discovery must not follow this external path.
                    "original_path": "Z:/unrelated/original.zip"
                })).collect::<Vec<_>>()
            }))
            .unwrap(),
        )
        .unwrap();
        directory
    }

    #[test]
    fn library_recovers_manifest_listed_original_when_disabled_sibling_is_missing() {
        let directory = Directory::new();
        let group = backup_manifest(&directory.0, "roadster", &["source.zip"]);
        let source = group.join("source.zip");
        original(&source, br#"{"Name":"Preserved roadster"}"#);
        original(&group.join("not-listed.zip"), br#"{"Name":"Not listed"}"#);
        let bytes = fs::read(&source).unwrap();
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(
            found,
            vec![VehicleArchive {
                path: source.clone(),
                name: "Preserved roadster (original before grouping)".into(),
                unavailable_reason: None,
            }]
        );
        assert_eq!(fs::read(source).unwrap(), bytes);
    }

    #[test]
    fn library_deduplicates_backup_content_and_prefers_accessible_originals() {
        let directory = Directory::new();
        let group = backup_manifest(&directory.0, "roadster", &["source.zip"]);
        let hidden = group.join("source.zip");
        original(&hidden, br#"{"Name":"Roadster"}"#);
        let disabled = directory.0.join("a.zip.merged_backup");
        fs::copy(&hidden, &disabled).unwrap();
        assert_eq!(
            scan_automation_archives(&directory.0).unwrap()[0].path,
            disabled
        );

        // Recompress and reverse member order: different ZIP bytes still carry
        // the same original assets and should occupy just one library row.
        let active = directory.0.join("z.zip");
        let mut input = zip::ZipArchive::new(File::open(&hidden).unwrap()).unwrap();
        let mut output = zip::ZipWriter::new(File::create(&active).unwrap());
        for index in (0..input.len()).rev() {
            let mut entry = input.by_index(index).unwrap();
            output
                .start_file(
                    entry.name(),
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .unwrap();
            std::io::copy(&mut entry, &mut output).unwrap();
        }
        output.finish().unwrap();
        assert_ne!(fs::read(&active).unwrap(), fs::read(hidden).unwrap());
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, active);
        assert_eq!(found[0].name, "Roadster");
    }

    #[test]
    fn library_preserves_distinct_sources_and_excludes_rendered_full_vehicle() {
        let directory = Directory::new();
        original(&directory.0.join("original.zip"), br#"{"Name":"Roadster"}"#);
        let entries: Vec<(&str, &[u8])> = vec![
            ("vehicles/test/test.car", b"Automation metadata"),
            ("vehicles/test/eng_694e8/camso_engine_694e8.jbeam", b"{}"),
            (
                "art/sound/blends/694E80154252F6189DE80988120C7F13.sfxBlend2D.json",
                b"{}",
            ),
            ("vehicles/test/info.json", br#"{"Name":"Roadster"}"#),
            ("art/sound/test.wav", b"different recording"),
        ];
        archive(&directory.0.join("other-source.zip"), &entries);
        let mut rendered = entries;
        rendered.push(("bess-export.json", b"{}"));
        archive(&directory.0.join("rendered.zip"), &rendered);
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|vehicle| vehicle.path.file_name().unwrap() != "rendered.zip")
        );
    }

    #[test]
    fn library_preserves_original_reference_beside_a_complete_selectable_variant() {
        let directory = Directory::new();
        let source = directory.0.join("original.zip");
        original(&source, br#"{"Name":"Original reference"}"#);
        let entries: Vec<(&str, &[u8])> = vec![
            ("vehicles/test/test.car", b"Automation metadata"),
            ("vehicles/test/eng_694e8/camso_engine_694e8.jbeam", b"{}"),
            ("vehicles/test/test.pc", b"{}"),
            (
                "art/sound/blends/694E80154252F6189DE80988120C7F13.sfxBlend2D.json",
                b"{}",
            ),
            (
                "vehicles/test/info.json",
                br#"{"Name":"Original reference"}"#,
            ),
            ("vehicles/test/bess_engine_natural.jbeam", b"{}"),
            ("vehicles/test/bess_natural.pc", b"{}"),
            (
                "art/sound/blends/694E80154252F6189DE80988120C7F13_BESS_natural.sfxBlend2D.json",
                b"{}",
            ),
            (
                "art/sound/blends/694E80154252F6189DE80988120C7F13_BESS_natural_engine.sfxBlend2D.json",
                b"{}",
            ),
        ];
        archive(&directory.0.join("complete-without-marker.zip"), &entries);
        let mut marked = entries;
        marked.push((
            "bess-export.json",
            br#"{"version":1,"kind":"bess-variant-vehicle"}"#,
        ));
        archive(&directory.0.join("complete-with-marker.zip"), &marked);
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, source);
        assert_eq!(found[0].name, "Original reference");
        assert!(found[0].unavailable_reason.is_none());
    }

    #[test]
    fn library_ignores_unsafe_manifest_paths_and_unlisted_nested_files() {
        let directory = Directory::new();
        let group = backup_manifest(
            &directory.0,
            "roadster",
            &[
                "../outside.zip",
                "../../outside.zip",
                "nested/source.zip",
                "nested\\source.zip",
                "C:/outside.zip",
                "source.zip:stream",
                "source.zip.",
                "source.zip ",
            ],
        );
        original(
            &directory.0.join(".babm_backup/outside.zip"),
            br#"{"Name":"Outside"}"#,
        );
        fs::create_dir(group.join("nested")).unwrap();
        original(&group.join("nested/source.zip"), br#"{"Name":"Nested"}"#);
        original(&group.join("source.zip"), br#"{"Name":"Unlisted"}"#);
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
    }

    #[test]
    fn library_filters_sources_without_using_archive_name_or_decoding_audio() {
        let directory = Directory::new();
        original(
            &directory.0.join("legitimate-bess.ZIP"),
            br#"{"Name":"Roadster","paints":{"Blue":1,"Blue":2}}"#,
        );
        archive(
            &directory.0.join("unrelated.zip"),
            &[("vehicles/truck/info.json", br#"{"Name":"Truck"}"#)],
        );
        archive(
            &directory.0.join("addon.zip"),
            &[
                ("vehicles/test/bess_engine_abc.jbeam", b"{}"),
                ("vehicles/test/bess_main_abc.pc", b"{}"),
                ("art/sound/blends/uid_BESS_abc.sfxBlend2D.json", b"{}"),
            ],
        );
        fs::write(directory.0.join("broken.zip"), b"not a zip").unwrap();
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(
            found,
            vec![VehicleArchive {
                path: directory.0.join("legitimate-bess.ZIP"),
                name: "Roadster".into(),
                unavailable_reason: None
            }]
        );
    }

    #[test]
    fn library_only_visits_root_and_direct_repo_and_sorts_stably() {
        let directory = Directory::new();
        fs::create_dir_all(directory.0.join("repo/deeper")).unwrap();
        fs::create_dir(directory.0.join("other")).unwrap();
        original(&directory.0.join("z.zip"), br#"{"Name":"Zulu"}"#);
        original(&directory.0.join("repo/a.zip"), br#"{"Name":"Alpha"}"#);
        original(
            &directory.0.join("repo/deeper/hidden.zip"),
            br#"{"Name":"Hidden"}"#,
        );
        original(
            &directory.0.join("other/hidden.zip"),
            br#"{"Name":"Hidden"}"#,
        );
        original(
            &directory.0.join("old.zip.merged_backup"),
            br#"{"Name":"Backup"}"#,
        );
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(
            found.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["Alpha", "Backup (original before grouping)", "Zulu"]
        );
        assert!(
            found
                .iter()
                .all(|vehicle| vehicle.unavailable_reason.is_none())
        );
        assert_eq!(found, scan_automation_archives(&directory.0).unwrap());
    }

    #[test]
    fn library_metadata_failure_falls_back_without_hiding_source() {
        let directory = Directory::new();
        original(&directory.0.join("a.zip"), b"invalid json");
        original(
            &directory.0.join("b.zip"),
            &vec![b' '; MAX_INFO_BYTES as usize + 1],
        );
        original(&directory.0.join("c.zip"), br#"{"Name":"\u0001bad"}"#);
        original(
            &directory.0.join("d.zip"),
            br#"{"Name":"First","Name":"Second"}"#,
        );
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(
            found.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c", "d"]
        );
    }

    #[test]
    fn library_rejects_unbounded_central_directory_before_reading_it() {
        let directory = Directory::new();
        let path = directory.0.join("bad.zip");
        let mut end = [0_u8; 22];
        end[..4].copy_from_slice(b"PK\x05\x06");
        end[8..10].copy_from_slice(&u16::MAX.to_le_bytes());
        end[10..12].copy_from_slice(&u16::MAX.to_le_bytes());
        end[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&path, end).unwrap();
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
    }

    #[test]
    fn library_does_not_join_unrelated_vehicle_markers() {
        let directory = Directory::new();
        archive(
            &directory.0.join("mixed.zip"),
            &[
                ("vehicles/a/a.car", b"metadata"),
                ("vehicles/b/eng_abc/camso_engine_abc.jbeam", b"{}"),
                ("art/sound/blends/abc.sfxBlend2D.json", b"{}"),
            ],
        );
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
    }

    #[test]
    fn library_labels_grouped_archives_unavailable_for_single_engine_import() {
        let directory = Directory::new();
        archive(
            &directory.0.join("grouped.zip"),
            &[
                ("vehicles/a/a.car", b"metadata"),
                ("vehicles/a/eng_aaa/camso_engine_aaa.jbeam", b"{}"),
                ("vehicles/b/b.car", b"metadata"),
                ("vehicles/b/eng_bbb/camso_engine_bbb.jbeam", b"{}"),
                ("art/sound/blends/aaa.sfxBlend2D.json", b"{}"),
                ("art/sound/blends/bbb.sfxBlend2D.json", b"{}"),
            ],
        );
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].name.contains("2 véhicules"));
        assert!(
            found[0]
                .unavailable_reason
                .as_ref()
                .unwrap()
                .contains("2 audio blends")
        );
    }

    #[test]
    fn library_disables_multiple_blends_even_in_one_vehicle_root() {
        let directory = Directory::new();
        archive(
            &directory.0.join("multiple.zip.merged_backup"),
            &[
                ("vehicles/a/a.car", b"metadata"),
                ("vehicles/a/eng_aaa/camso_engine_aaa.jbeam", b"{}"),
                ("vehicles/a/info.json", br#"{"Name":"One car"}"#),
                ("art/sound/blends/aaa.sfxBlend2D.json", b"{}"),
                ("art/sound/blends/bbb.sfxBlend2D.json", b"{}"),
            ],
        );
        let found = scan_automation_archives(&directory.0).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "One car (original before grouping)");
        assert!(found[0].unavailable_reason.is_some());
    }

    #[test]
    fn library_recognizes_bess_layout_even_when_source_metadata_was_copied() {
        let directory = Directory::new();
        archive(
            &directory.0.join("processed.zip"),
            &[
                ("vehicles/test/test.car", b"metadata"),
                ("vehicles/test/eng_aaa/camso_engine_aaa.jbeam", b"{}"),
                ("vehicles/test/bess_engine_abc.jbeam", b"{}"),
                ("vehicles/test/bess_main_abc.pc", b"{}"),
                ("art/sound/blends/uid_BESS_abc.sfxBlend2D.json", b"{}"),
            ],
        );
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn library_never_follows_linked_archives_or_repo_directory() {
        use std::os::unix::fs::symlink;
        let directory = Directory::new();
        let outside = Directory::new();
        original(&outside.0.join("outside.zip"), br#"{"Name":"Outside"}"#);
        symlink(&outside.0, directory.0.join("repo")).unwrap();
        symlink(
            outside.0.join("outside.zip"),
            directory.0.join("linked.zip"),
        )
        .unwrap();
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
        assert!(scan_automation_archives(&directory.0.join("repo")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn library_never_follows_babm_backup_links_at_any_level() {
        use std::os::unix::fs::symlink;
        let directory = Directory::new();
        let outside = Directory::new();
        let outside_group = backup_manifest(&outside.0, "outside", &["source.zip"]);
        original(&outside_group.join("source.zip"), br#"{"Name":"Outside"}"#);
        symlink(
            outside.0.join(".babm_backup"),
            directory.0.join(".babm_backup"),
        )
        .unwrap();
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
        fs::remove_file(directory.0.join(".babm_backup")).unwrap();
        let group = backup_manifest(&directory.0, "local", &["source.zip"]);
        symlink(
            &outside_group,
            directory.0.join(".babm_backup/linked-group"),
        )
        .unwrap();
        symlink(outside_group.join("source.zip"), group.join("source.zip")).unwrap();
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
        fs::remove_file(group.join("manifest.json")).unwrap();
        symlink(
            outside_group.join("manifest.json"),
            group.join("manifest.json"),
        )
        .unwrap();
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn library_never_follows_babm_backup_junctions() {
        use std::os::windows::process::CommandExt;
        fn junction(link: &Path, target: &Path) {
            let result = std::process::Command::new("cmd")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(link)
                .arg(target)
                .creation_flags(0x0800_0000)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let directory = Directory::new();
        let outside = Directory::new();
        let outside_group = backup_manifest(&outside.0, "outside", &["source.zip"]);
        original(&outside_group.join("source.zip"), br#"{"Name":"Outside"}"#);
        let backup_root = directory.0.join(".babm_backup");
        junction(&backup_root, &outside.0.join(".babm_backup"));
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
        fs::remove_dir(&backup_root).unwrap();
        fs::create_dir(&backup_root).unwrap();
        let linked_group = backup_root.join("linked-group");
        junction(&linked_group, &outside_group);
        assert!(scan_automation_archives(&directory.0).unwrap().is_empty());
        fs::remove_dir(linked_group).unwrap();
        assert!(outside_group.join("source.zip").is_file());
    }
}
