//! Bounded, read-only BeamNG user-folder discovery. Only BESS preferences are written.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const PREFERENCE_FILE: &str = "beamng-folder.json";
const MAX_CONFIG_BYTES: u64 = 262_144;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);
static PREFERENCE_WRITE: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedFolder {
    pub mods_dir: PathBuf,
    pub description: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mods_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    export_dir: Option<PathBuf>,
}

impl Default for Preference {
    fn default() -> Self {
        Self {
            version: 1,
            mods_dir: None,
            export_dir: None,
        }
    }
}

#[derive(Default, Deserialize)]
struct LegacyRegistry {
    user_folder: Option<String>,
    version: Option<String>,
    steam: Option<String>,
    documents: Option<String>,
}

pub fn app_data_dir() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .ok_or("Windows application-data folder is unavailable; choose a BeamNG folder manually")?;
    let root = PathBuf::from(root);
    if !root.is_absolute() {
        return Err("Windows application-data folder must be an absolute path".into());
    }
    Ok(root.join("BESS"))
}

fn text_file(path: &Path) -> Result<Option<String>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read {}: {error}", path.display())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return Err(format!("Configuration is too large: {}", path.display()));
    }
    String::from_utf8(bytes)
        .map(|text| Some(text.trim_start_matches('\u{feff}').to_owned()))
        .map_err(|_| format!("Configuration is not UTF-8: {}", path.display()))
}

fn ini_value(text: &str, key: &str) -> Result<Option<String>, String> {
    let mut found = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with([';', '#', '[']) {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case(key) {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value)
            .to_owned();
        if found.as_ref().is_some_and(|old| old != &value) {
            return Err(format!("Conflicting {key} entries in BeamNG configuration"));
        }
        found = Some(value);
    }
    Ok(found)
}

fn configured_path(file: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        file.parent().unwrap_or(Path::new(".")).join(path)
    }
}

fn version_parts(name: &str) -> Option<Vec<u32>> {
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    let parts: Option<Vec<_>> = name
        .split('.')
        .map(|part| part.parse::<u32>().ok())
        .collect();
    parts.filter(|parts| (2..=4).contains(&parts.len()))
}

fn existing_mods(root: &Path, version: Option<&str>) -> Result<Option<PathBuf>, String> {
    // A manually selected mods directory is explicit. Otherwise current wins
    // over a leftover pre-migration mods directory beside it.
    if root.is_dir()
        && root
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("mods"))
    {
        return fs::canonicalize(root).map(Some).map_err(|e| e.to_string());
    }
    for candidate in [root.join("current/mods"), root.join("mods")] {
        if candidate.is_dir() {
            return fs::canonicalize(candidate)
                .map(Some)
                .map_err(|e| e.to_string());
        }
    }
    // Legacy folders were versioned. Prefer the recorded version when it exists;
    // otherwise inspect only immediate numeric version directories, never backups.
    if let Some(parts) = version.and_then(version_parts) {
        let short = format!("{}.{}", parts[0], parts[1]);
        for suffix in [version.unwrap_or_default(), short.as_str()] {
            let candidate = root.join(suffix).join("mods");
            if candidate.is_dir() {
                return fs::canonicalize(candidate)
                    .map(Some)
                    .map_err(|e| e.to_string());
            }
        }
    }
    if !root.is_dir() {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for entry in
        fs::read_dir(root).map_err(|e| format!("Cannot inspect {}: {e}", root.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(parts) = entry.file_name().to_str().and_then(version_parts) else {
            continue;
        };
        let candidate = entry.path().join("mods");
        if candidate.is_dir() {
            candidates.push((parts, candidate));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates
        .into_iter()
        .next()
        .map(|(_, path)| fs::canonicalize(path).map_err(|e| e.to_string()))
        .transpose()
}

/// Accept a mods directory, active user folder, or version-container root.
/// No directories are created, and unrelated directories are rejected.
pub fn normalize_selected_folder(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute BeamNG user-folder or mods path".into());
    }
    existing_mods(path, None)?.ok_or_else(|| {
        format!(
            "No existing BeamNG mods folder under {}. Choose the active user folder or its mods directory.",
            path.display()
        )
    })
}

fn explicit_folder(
    path: &Path,
    version: Option<&str>,
    source: &str,
) -> Result<DetectedFolder, String> {
    if !path.is_absolute() {
        return Err(format!("{source} must specify an absolute BeamNG folder"));
    }
    let mods_dir = existing_mods(path, version)?.ok_or_else(|| {
        format!(
            "{source} points to {}, but its mods folder is missing. Correct this location; no other folder was selected.",
            path.display()
        )
    })?;
    Ok(DetectedFolder {
        description: format!("{source}: {}", mods_dir.display()),
        mods_dir,
    })
}

fn saved_override(preference: &Path) -> Result<Option<DetectedFolder>, String> {
    let saved = load_preference(preference)?;
    let Some(mods_dir) = saved.mods_dir else {
        return Ok(None);
    };
    if !mods_dir.is_absolute() {
        return Err(
            "Unsupported saved BeamNG folder preference; choose or reset the folder".into(),
        );
    }
    // An override stores the exact resolved mods path. If it disappeared, never
    // reinterpret a parent/version folder or silently export to another location.
    if !mods_dir.is_dir()
        || !mods_dir
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("mods"))
    {
        return Err(format!(
            "Saved BeamNG mods folder is unavailable: {}. Choose or reset it; automatic fallback was not used.",
            mods_dir.display()
        ));
    }
    explicit_folder(&mods_dir, None, "Saved BeamNG folder").map(Some)
}

fn load_preference(path: &Path) -> Result<Preference, String> {
    let Some(text) = text_file(path)? else {
        return Ok(Preference::default());
    };
    let saved: Preference = serde_json::from_str(&text).map_err(|e| {
        format!("Invalid saved BeamNG folder preference: {e}. Use Detect folder to back up and reset this BESS preference.")
    })?;
    if saved.version != 1 {
        return Err("Unsupported saved BeamNG preference version. Use Detect folder to back up and reset this BESS preference.".into());
    }
    Ok(saved)
}

fn detect_modern(local: &Path) -> Result<Option<DetectedFolder>, String> {
    let ini = local.join("BeamNG/BeamNG.drive.ini");
    let Some(text) = text_file(&ini)? else {
        return Ok(None);
    };
    let value = ini_value(&text, "userFolder")?.unwrap_or_default();
    let root = if value.is_empty() {
        local.join("BeamNG/BeamNG.drive")
    } else {
        configured_path(&ini, &value)
    };
    let version = ini_value(&text, "version")?;
    explicit_folder(&root, version.as_deref(), "BeamNG launcher configuration").map(Some)
}

fn steam_installs(steam: &Path) -> Result<Vec<PathBuf>, String> {
    let mut libraries = vec![steam.to_owned()];
    if let Some(text) = text_file(&steam.join("steamapps/libraryfolders.vdf"))? {
        for line in text.lines() {
            let mut values = serde_json::Deserializer::from_str(line.trim()).into_iter::<String>();
            if matches!(values.next(), Some(Ok(key)) if key == "path")
                && let Some(Ok(path)) = values.next()
            {
                let path = PathBuf::from(path);
                if path.is_absolute() && !libraries.contains(&path) {
                    libraries.push(path);
                }
            }
        }
    }
    Ok(libraries
        .into_iter()
        .map(|library| library.join("steamapps/common/BeamNG.drive"))
        .collect())
}

fn detect_legacy(
    local: &Path,
    registry: &LegacyRegistry,
) -> Result<Option<DetectedFolder>, String> {
    let mut configured = Vec::new();
    if let Some(path) = registry
        .user_folder
        .as_ref()
        .filter(|path| !path.trim().is_empty())
    {
        configured.push(explicit_folder(
            Path::new(path),
            registry.version.as_deref(),
            "Legacy BeamNG launcher setting",
        )?);
    }
    if let Some(steam) = &registry.steam {
        for install in steam_installs(Path::new(steam))? {
            if !install.join("BeamNG.drive.exe").is_file() {
                continue;
            }
            let ini = install.join("startup.ini");
            if let Some(text) = text_file(&ini)?
                && let Some(value) = ini_value(&text, "UserPath")?.filter(|value| !value.is_empty())
            {
                configured.push(explicit_folder(
                    &configured_path(&ini, &value),
                    registry.version.as_deref(),
                    "BeamNG startup.ini",
                )?);
            }
        }
    }
    if let Some(first) = configured.first() {
        if configured
            .iter()
            .any(|folder| folder.mods_dir != first.mods_dir)
        {
            return Err("BeamNG startup.ini and legacy launcher settings disagree. Choose the active mods folder explicitly.".into());
        }
        return Ok(Some(first.clone()));
    }
    let mut roots = vec![
        local.join("BeamNG/BeamNG.drive"),
        local.join("BeamNG.drive"),
    ];
    if let Some(documents) = &registry.documents {
        roots.push(Path::new(documents).join("BeamNG.drive"));
    }
    for root in roots {
        if let Some(mods_dir) = existing_mods(&root, registry.version.as_deref())? {
            return Ok(Some(DetectedFolder {
                description: format!("Detected BeamNG user folder: {}", mods_dir.display()),
                mods_dir,
            }));
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn legacy_registry() -> Result<LegacyRegistry, String> {
    use std::os::windows::process::CommandExt;
    // Fixed read-only script, not interpolated shell text. UTF-8 preserves paths
    // outside the console code page, and the helper never opens a visible window.
    let script = r#"[Console]::OutputEncoding=[Text.UTF8Encoding]::new();$b=Get-ItemProperty -LiteralPath 'HKCU:\SOFTWARE\BeamNG\BeamNG.drive' -ErrorAction SilentlyContinue;$s=Get-ItemProperty -LiteralPath 'HKCU:\SOFTWARE\Valve\Steam' -ErrorAction SilentlyContinue;@{user_folder=$b.userpath_override;version=$b.version;steam=$s.SteamPath;documents=[Environment]::GetFolderPath('MyDocuments')}|ConvertTo-Json -Compress"#;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| format!("Cannot read legacy BeamNG paths: {e}"))?;
    if !output.status.success() {
        return Err("Cannot read legacy BeamNG settings; choose the user folder manually".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Cannot decode legacy BeamNG paths: {e}"))
}

#[cfg(not(windows))]
fn legacy_registry() -> Result<LegacyRegistry, String> {
    Ok(LegacyRegistry::default())
}

/// Explicit BESS choice, then current launcher INI, then bounded legacy discovery.
pub fn detect() -> Result<Option<DetectedFolder>, String> {
    let app = app_data_dir()?;
    if let Some(folder) = saved_override(&app.join(PREFERENCE_FILE))? {
        return Ok(Some(folder));
    }
    let local = app.parent().ok_or("Invalid application-data path")?;
    if let Some(folder) = detect_modern(local)? {
        return Ok(Some(folder));
    }
    detect_legacy(local, &legacy_registry()?)
}

fn save_override_at(app: &Path, folder: Option<&Path>) -> Result<(), String> {
    let _guard = PREFERENCE_WRITE.lock().map_err(|e| e.to_string())?;
    let path = app.join(PREFERENCE_FILE);
    let mut preference = match load_preference(&path) {
        Ok(preference) => preference,
        // Only an explicit user reset may recover invalid preferences. Automatic
        // discovery and ordinary choices keep reporting the error unchanged.
        Err(error) if folder.is_none() => {
            let backup = app.join(format!(
                "beamng-folder.invalid-{}-{}-{}.json.bak",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos(),
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::rename(&path, &backup).map_err(|e| {
                format!(
                    "{error} Cannot back up {} to {}: {e}",
                    path.display(),
                    backup.display()
                )
            })?;
            Preference::default()
        }
        Err(error) => return Err(error),
    };
    preference.mods_dir = folder.map(normalize_selected_folder).transpose()?;
    write_preference(app, &preference)
}

fn write_preference(app: &Path, preference: &Preference) -> Result<(), String> {
    let destination = app.join(PREFERENCE_FILE);
    if preference.mods_dir.is_none() && preference.export_dir.is_none() {
        return match fs::remove_file(&destination) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Cannot reset BeamNG folder preference: {error}")),
        };
    }
    let bytes = serde_json::to_vec_pretty(preference).map_err(|e| e.to_string())?;
    fs::create_dir_all(app).map_err(|e| format!("Cannot create BESS preferences: {e}"))?;
    let temporary = app.join(format!(
        "beamng-folder-{}-{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, &destination)
            .map_err(|e| format!("Cannot save BeamNG folder preference: {e}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Persist only BESS's local choice. Never creates or changes BeamNG directories.
/// None is an explicit reset: valid export preferences survive; an unreadable or
/// invalid preference file is backed up before resetting both unknown values.
pub fn save_override(folder: Option<&Path>) -> Result<(), String> {
    save_override_at(&app_data_dir()?, folder)
}

fn existing_export_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(format!(
            "Saved export folder is unavailable: {}. Choose another existing directory; automatic fallback was not used.",
            path.display()
        ));
    }
    fs::canonicalize(path).map_err(|e| format!("Cannot access export folder: {e}"))
}

/// Validate a manually selected existing output directory. With a known BeamNG
/// folder, reject output inside its active mods directory after resolving paths.
pub fn validate_export_directory(path: &Path, mods_dir: Option<&Path>) -> Result<PathBuf, String> {
    let path = existing_export_directory(path)?;
    validate_export_destination(&path, mods_dir)
}

/// Validate an export destination without creating it. Its final directory may
/// be absent, but its parent must already exist and is resolved before checking
/// whether the destination would write into active BeamNG mods.
pub fn validate_export_destination(
    path: &Path,
    mods_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute export folder path".into());
    }
    let path = match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => existing_export_directory(path)?,
        Ok(_) => {
            return Err(format!(
                "Export destination is not a directory: {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let Some(std::path::Component::Normal(name)) = path.components().next_back() else {
                return Err("Export destination must end with a normal directory name".into());
            };
            let parent = path.parent().ok_or("Export destination has no parent")?;
            existing_export_directory(parent)?.join(name)
        }
        Err(error) => {
            return Err(format!(
                "Cannot access export destination {}: {error}",
                path.display()
            ));
        }
    };
    if let Some(mods_dir) = mods_dir {
        let mods_dir = normalize_selected_folder(mods_dir)?;
        if path.starts_with(&mods_dir) {
            return Err("Choose an export folder outside BeamNG mods; export does not install or replace active mods automatically".into());
        }
    }
    Ok(path)
}

fn saved_export_directory_at(app: &Path) -> Result<Option<PathBuf>, String> {
    load_preference(&app.join(PREFERENCE_FILE))?
        .export_dir
        .map(|path| validate_export_directory(&path, None))
        .transpose()
}

/// Restore a saved output directory independently of BeamNG installation or
/// detection. A missing saved directory remains an explicit error, not a fallback.
pub fn saved_export_directory() -> Result<Option<PathBuf>, String> {
    saved_export_directory_at(&app_data_dir()?)
}

fn export_directory_at(app: &Path, mods_dir: &Path) -> Result<PathBuf, String> {
    let mods_dir = normalize_selected_folder(mods_dir)?;
    if let Some(path) = saved_export_directory_at(app)? {
        return validate_export_directory(&path, Some(&mods_dir));
    }
    Ok(mods_dir
        .parent()
        .ok_or("BeamNG mods folder has no parent")?
        .join("BESS-exports"))
}

/// Output is a sibling of mods by default. Returning it does not create it.
pub fn export_directory(mods_dir: &Path) -> Result<PathBuf, String> {
    export_directory_at(&app_data_dir()?, mods_dir)
}

fn save_export_directory_at(app: &Path, path: &Path) -> Result<(), String> {
    let _guard = PREFERENCE_WRITE.lock().map_err(|e| e.to_string())?;
    let export_dir = validate_export_directory(path, None)?;
    let mut preference = load_preference(&app.join(PREFERENCE_FILE))?;
    if let Some(mods_dir) = &preference.mods_dir
        && let Ok(mods_dir) = fs::canonicalize(mods_dir)
        && export_dir.starts_with(&mods_dir)
    {
        return Err("Choose an export folder outside BeamNG mods".into());
    }
    preference.export_dir = Some(export_dir);
    write_preference(app, &preference)
}

/// Store an existing output directory without changing the selected BeamNG folder.
pub fn save_export_directory(path: &Path) -> Result<(), String> {
    save_export_directory_at(&app_data_dir()?, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("bess-beamng-paths-{}-{id}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn mods(&self, suffix: &str) -> PathBuf {
            let path = self.0.join(suffix).join("mods");
            fs::create_dir_all(&path).unwrap();
            fs::canonicalize(path).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn custom_base_resolves_current_without_choosing_older_backups() {
        let fixture = Fixture::new();
        let current = fixture.mods("custom/current");
        fixture.mods("custom/0.36");
        fixture.mods("custom/2026-backup-v0.99");
        fixture.mods("custom");
        assert_eq!(
            normalize_selected_folder(&fixture.0.join("custom")).unwrap(),
            current
        );
        assert_eq!(normalize_selected_folder(&current).unwrap(), current);
    }

    #[test]
    fn modern_relative_ini_and_empty_default_resolve_from_the_ini_directory() {
        let fixture = Fixture::new();
        let relative = fixture.mods("BeamNG/custom/current");
        let ini = fixture.0.join("BeamNG/BeamNG.drive.ini");
        fs::write(
            &ini,
            "\u{feff};comment\nversion=0.39.4.0\nuserFolder=custom\n",
        )
        .unwrap();
        assert_eq!(
            detect_modern(&fixture.0).unwrap().unwrap().mods_dir,
            relative
        );
        let default = fixture.mods("BeamNG/BeamNG.drive/current");
        fs::write(&ini, "userFolder =\n").unwrap();
        assert_eq!(
            detect_modern(&fixture.0).unwrap().unwrap().mods_dir,
            default
        );
    }

    #[test]
    fn authoritative_missing_folder_never_uses_an_existing_default() {
        let fixture = Fixture::new();
        fixture.mods("BeamNG/BeamNG.drive/current");
        fs::write(
            fixture.0.join("BeamNG/BeamNG.drive.ini"),
            "userFolder=missing\n",
        )
        .unwrap();
        assert!(
            detect_modern(&fixture.0)
                .unwrap_err()
                .contains("no other folder")
        );
    }

    #[test]
    fn legacy_version_order_is_numeric_and_ignores_migration_backups() {
        let fixture = Fixture::new();
        fixture.mods("BeamNG.drive/0.9");
        let latest = fixture.mods("BeamNG.drive/0.36");
        fixture.mods("BeamNG.drive/2026-08__v0.99");
        assert_eq!(
            detect_legacy(&fixture.0, &LegacyRegistry::default())
                .unwrap()
                .unwrap()
                .mods_dir,
            latest
        );
    }

    #[test]
    fn preference_roundtrip_update_reset_and_missing_override_are_local_only() {
        let fixture = Fixture::new();
        let first = fixture.mods("first");
        let second = fixture.mods("second");
        let app = fixture.0.join("BESS");
        save_override_at(&app, Some(&first)).unwrap();
        assert_eq!(
            saved_override(&app.join(PREFERENCE_FILE))
                .unwrap()
                .unwrap()
                .mods_dir,
            first
        );
        save_override_at(&app, Some(&second)).unwrap();
        assert_eq!(
            saved_override(&app.join(PREFERENCE_FILE))
                .unwrap()
                .unwrap()
                .mods_dir,
            second
        );
        fs::remove_dir(&second).unwrap();
        assert!(
            saved_override(&app.join(PREFERENCE_FILE))
                .unwrap_err()
                .contains("automatic fallback was not used")
        );
        save_override_at(&app, None).unwrap();
        save_override_at(&app, None).unwrap();
        assert!(
            saved_override(&app.join(PREFERENCE_FILE))
                .unwrap()
                .is_none()
        );
        assert!(first.is_dir());
    }

    #[test]
    fn malformed_preferences_and_conflicting_ini_are_not_silently_ignored() {
        let fixture = Fixture::new();
        let path = fixture.0.join(PREFERENCE_FILE);
        fs::write(&path, "{}").unwrap();
        assert!(saved_override(&path).is_err());
        assert!(ini_value("userFolder=a\nuserFolder=b", "userFolder").is_err());
        assert_eq!(
            ini_value(
                ";UserPath=wrong\n[filesystem]\nUserPath=\"right\"",
                "UserPath"
            )
            .unwrap()
            .as_deref(),
            Some("right")
        );
    }

    #[test]
    fn explicit_reset_backs_up_invalid_preferences_without_silent_recovery() {
        let fixture = Fixture::new();
        let app = fixture.0.join("BESS");
        fs::create_dir(&app).unwrap();
        let path = app.join(PREFERENCE_FILE);
        let damaged = b"{\"version\":1,\"mods_dir\":unfinished";
        fs::write(&path, damaged).unwrap();
        let mods = fixture.mods("user/current");
        assert!(saved_override(&path).is_err());
        assert!(save_override_at(&app, Some(&mods)).is_err());
        assert_eq!(fs::read(&path).unwrap(), damaged);
        save_override_at(&app, None).unwrap();
        assert!(saved_override(&path).unwrap().is_none());
        let backups: Vec<_> = fs::read_dir(&app)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), damaged);
        save_override_at(&app, Some(&mods)).unwrap();
        assert_eq!(saved_override(&path).unwrap().unwrap().mods_dir, mods);
    }

    #[test]
    fn selecting_unrelated_folder_does_not_create_mods() {
        let fixture = Fixture::new();
        assert!(normalize_selected_folder(&fixture.0).is_err());
        assert!(!fixture.0.join("mods").exists());
    }

    #[test]
    fn independent_preferences_preserve_each_other_and_default_export_stays_outside_mods() {
        let fixture = Fixture::new();
        let mods = fixture.mods("user/current");
        let other_mods = fixture.mods("second/current");
        let app = fixture.0.join("BESS");
        let expected = mods.parent().unwrap().join("BESS-exports");
        assert_eq!(export_directory_at(&app, &mods).unwrap(), expected);
        assert!(!expected.exists());
        let output = fixture.0.join("rendered");
        fs::create_dir(&output).unwrap();
        save_override_at(&app, Some(&mods)).unwrap();
        save_export_directory_at(&app, &output).unwrap();
        assert_eq!(
            saved_override(&app.join(PREFERENCE_FILE))
                .unwrap()
                .unwrap()
                .mods_dir,
            mods
        );
        save_override_at(&app, Some(&other_mods)).unwrap();
        assert_eq!(
            export_directory_at(&app, &other_mods).unwrap(),
            fs::canonicalize(&output).unwrap()
        );
        save_override_at(&app, None).unwrap();
        assert!(
            saved_override(&app.join(PREFERENCE_FILE))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            export_directory_at(&app, &mods).unwrap(),
            fs::canonicalize(&output).unwrap()
        );
        fs::remove_dir(&output).unwrap();
        assert!(
            export_directory_at(&app, &mods)
                .unwrap_err()
                .contains("automatic fallback was not used")
        );
    }

    #[test]
    fn explicit_export_never_points_inside_active_mods() {
        let fixture = Fixture::new();
        let mods = fixture.mods("user/current");
        let app = fixture.0.join("BESS");
        save_override_at(&app, Some(&mods)).unwrap();
        assert!(save_export_directory_at(&app, &mods).is_err());
    }

    #[test]
    fn export_only_preference_restores_without_any_beamng_folder() {
        let fixture = Fixture::new();
        let app = fixture.0.join("BESS");
        let output = fixture.0.join("rendered");
        fs::create_dir(&output).unwrap();
        assert!(saved_export_directory_at(&app).unwrap().is_none());
        save_export_directory_at(&app, &output).unwrap();
        assert!(
            saved_override(&app.join(PREFERENCE_FILE))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            saved_export_directory_at(&app).unwrap(),
            Some(fs::canonicalize(&output).unwrap())
        );
        // An unavailable separately saved BeamNG folder cannot block restoring
        // a still-valid output choice for an already loaded source archive.
        let mut preference = load_preference(&app.join(PREFERENCE_FILE)).unwrap();
        preference.mods_dir = Some(fixture.0.join("missing/mods"));
        write_preference(&app, &preference).unwrap();
        assert!(saved_override(&app.join(PREFERENCE_FILE)).is_err());
        assert_eq!(
            saved_export_directory_at(&app).unwrap(),
            Some(fs::canonicalize(&output).unwrap())
        );
        fs::remove_dir(&output).unwrap();
        assert!(
            saved_export_directory_at(&app)
                .unwrap_err()
                .contains("automatic fallback was not used")
        );
    }

    #[test]
    fn manual_export_validation_rejects_active_mods_and_descendants() {
        let fixture = Fixture::new();
        let user = fixture.0.join("user/current");
        let mods = fixture.mods("user/current");
        let nested = mods.join("nested");
        let sibling = user.join("BESS-exports");
        fs::create_dir(&nested).unwrap();
        fs::create_dir(&sibling).unwrap();
        assert!(validate_export_directory(&mods, Some(&user)).is_err());
        assert!(validate_export_directory(&nested, Some(&mods)).is_err());
        assert_eq!(
            validate_export_directory(&sibling, Some(&user)).unwrap(),
            fs::canonicalize(&sibling).unwrap()
        );
        assert!(validate_export_directory(&user.join("not-created"), None).is_err());
        assert!(!user.join("not-created").exists());
    }

    #[test]
    fn export_destination_allows_only_one_missing_directory_outside_mods() {
        let fixture = Fixture::new();
        let user = fixture.0.join("user/current");
        let mods = fixture.mods("user/current");
        let sibling = user.join("BESS-exports");
        assert_eq!(
            validate_export_destination(&sibling, Some(&mods)).unwrap(),
            fs::canonicalize(&user).unwrap().join("BESS-exports")
        );
        assert!(!sibling.exists());
        assert!(validate_export_destination(&mods.join("absent"), Some(&mods)).is_err());
        assert!(validate_export_destination(&user.join("missing/second"), Some(&mods)).is_err());
        assert!(validate_export_destination(Path::new("relative"), None).is_err());
        let file = user.join("file");
        fs::write(&file, b"unchanged").unwrap();
        assert!(validate_export_destination(&file, None).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"unchanged");
    }
}
