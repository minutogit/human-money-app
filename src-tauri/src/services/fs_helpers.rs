use crate::models::FrontendError;
use chrono::Utc;
use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use tauri::Manager;

const PROFILE_METADATA_FILE: &str = "profile_metadata.json";
const ACTIVE_RECOVERY_SESSION_MAX_AGE_SECS: u64 = 7 * 86_400;
const MAX_CUSTOM_STANDARD_TOML_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct ProfileMetadata {
    pub(crate) last_used: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) user_id: Option<String>,
}

pub(crate) fn get_wallet_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("wallet_data"))
        .map_err(|e| format!("Could not determine app data directory: {}", e))
}

pub(crate) fn load_profile_metadata_in_dir(
    wallet_data_dir: &Path,
) -> Result<HashMap<String, ProfileMetadata>, String> {
    let metadata_path = wallet_data_dir.join(PROFILE_METADATA_FILE);
    let content = match fs::read_to_string(&metadata_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(format!("Failed to read profile metadata: {}", e)),
    };
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse profile metadata: {}", e))
}

pub(crate) fn save_profile_metadata_in_dir(
    wallet_data_dir: &Path,
    metadata: &HashMap<String, ProfileMetadata>,
) -> Result<(), String> {
    fs::create_dir_all(wallet_data_dir)
        .map_err(|e| format!("Failed to create metadata directory: {}", e))?;
    let metadata_path = wallet_data_dir.join(PROFILE_METADATA_FILE);
    let content = serde_json::to_string_pretty(metadata)
        .map_err(|e| format!("Failed to serialize profile metadata: {}", e))?;
    write_file_atomic(&metadata_path, content.as_bytes()).map_err(|e| e.message)
}

pub(crate) fn load_profile_metadata(
    app: &tauri::AppHandle,
) -> Result<HashMap<String, ProfileMetadata>, String> {
    let wallet_data_dir = get_wallet_data_dir(app)?;
    load_profile_metadata_in_dir(&wallet_data_dir)
}

pub(crate) fn load_profile_metadata_map_from_dir(
    wallet_data_dir: &Path,
) -> HashMap<String, (String, Option<String>)> {
    match load_profile_metadata_in_dir(wallet_data_dir) {
        Ok(map) => map
            .into_iter()
            .map(|(k, v)| (k, (v.last_used, v.user_id)))
            .collect(),
        Err(e) => {
            warn!("Ignoring unreadable profile metadata for display: {}", e);
            HashMap::new()
        }
    }
}

pub(crate) fn record_profile_metadata_in_dir(
    wallet_data_dir: &Path,
    folder_name: &str,
    user_id: Option<String>,
) -> Result<(), String> {
    let mut metadata = load_profile_metadata_in_dir(wallet_data_dir)?;
    let existing_uid = metadata.get(folder_name).and_then(|m| m.user_id.clone());
    metadata.insert(
        folder_name.to_string(),
        ProfileMetadata {
            last_used: Utc::now().to_rfc3339(),
            user_id: user_id.or(existing_uid),
        },
    );
    save_profile_metadata_in_dir(wallet_data_dir, &metadata)
}

pub(crate) fn record_profile_metadata(
    app: &tauri::AppHandle,
    folder_name: &str,
    user_id: Option<String>,
) -> Result<(), String> {
    let wallet_data_dir = get_wallet_data_dir(app)?;
    record_profile_metadata_in_dir(&wallet_data_dir, folder_name, user_id)
}

pub(crate) fn remove_profile_metadata_entry_in_dir(
    wallet_data_dir: &Path,
    folder_name: &str,
) -> Result<(), String> {
    let mut metadata = load_profile_metadata_in_dir(wallet_data_dir)?;
    if metadata.remove(folder_name).is_some() {
        save_profile_metadata_in_dir(wallet_data_dir, &metadata)?;
    }
    Ok(())
}

/// Resolves the OS machine ID (on Desktop) or a persisted fallback UUID in `app_config_dir`.
pub(crate) fn resolve_or_create_local_instance_id(
    app: &tauri::AppHandle,
) -> Result<String, String> {
    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd"
    ))]
    {
        match machine_uid::get() {
            Ok(id) if !id.trim().is_empty() => {
                info!("Device binding: Using OS Machine ID.");
                return Ok(id.trim().to_string());
            }
            Ok(_) => {
                warn!("OS Machine ID returned empty. Falling back to file-based ID.");
            }
            Err(e) => {
                warn!(
                    "Failed to read OS Machine ID: {}. Falling back to file-based ID.",
                    e
                );
            }
        }
    }

    let app_config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    resolve_or_create_fallback_instance_id_in_dir(&app_config_dir)
}

pub(crate) fn resolve_or_create_fallback_instance_id_in_dir(
    app_config_dir: &Path,
) -> Result<String, String> {
    let fallback_path = app_config_dir.join("instance_id_fallback");

    if let Ok(id) = fs::read_to_string(&fallback_path) {
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() {
            info!("Device binding: Using fallback file-based ID.");
            return Ok(trimmed);
        }
    }

    fs::create_dir_all(app_config_dir).map_err(|e| e.to_string())?;
    let new_id = uuid::Uuid::new_v4().to_string();

    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    match opts.open(&fallback_path) {
        Ok(mut file) => {
            if let Err(e) = file
                .write_all(new_id.as_bytes())
                .and_then(|()| file.sync_all())
            {
                let _ = fs::remove_file(&fallback_path);
                return Err(e.to_string());
            }
            warn!("Device binding: Generated new fallback instance ID for sandboxed environment.");
            Ok(new_id)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read_to_string(&fallback_path).map_err(|err| err.to_string())?;
            let trimmed = existing.trim().to_string();
            if !trimmed.is_empty() {
                info!("Device binding: Using fallback file-based ID.");
                return Ok(trimmed);
            }
            write_file_atomic(&fallback_path, new_id.as_bytes()).map_err(|err| err.message)?;
            warn!("Device binding: Replaced empty fallback instance ID.");
            Ok(new_id)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Validates that `s` is a single safe path component (no separators, traversal, colons,
/// control characters, or leading/trailing whitespace).
pub(crate) fn is_safe_filename_component(s: &str) -> bool {
    if s.is_empty()
        || s == "."
        || s == ".."
        || s != s.trim()
        || s.contains(['/', '\\', ':'])
        || s.bytes().any(|b| b < 0x20 || b == 0x7f)
    {
        return false;
    }
    let mut comps = Path::new(s).components();
    matches!(
        (comps.next(), comps.next()),
        (Some(Component::Normal(_)), None)
    )
}

/// Returns `true` iff `path` is NOT a symlink and its canonical path is strictly inside `base`.
pub(crate) fn is_canonical_child_of(path: &Path, base: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return false;
    }
    let (Ok(canon_path), Ok(canon_base)) = (fs::canonicalize(path), fs::canonicalize(base)) else {
        return false;
    };
    canon_path.starts_with(&canon_base) && canon_path != canon_base
}

/// Reads a regular file (rejecting symlinks, directories, FIFOs, and device nodes) up to `max_bytes`.
pub(crate) fn read_regular_file_bounded(
    path: &Path,
    max_bytes: usize,
) -> Result<Vec<u8>, FrontendError> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        FrontendError::from(format!("Failed to inspect '{}': {}", path.display(), e))
    })?;
    if !meta.file_type().is_file() {
        return Err(FrontendError::from(format!(
            "Path '{}' is not a regular file",
            path.display()
        )));
    }
    let file = fs::File::open(path)
        .map_err(|e| FrontendError::from(format!("Failed to open '{}': {}", path.display(), e)))?;
    let limit = (max_bytes as u64).saturating_add(1);
    let mut buf = Vec::new();
    file.take(limit)
        .read_to_end(&mut buf)
        .map_err(|e| FrontendError::from(format!("Failed to read '{}': {}", path.display(), e)))?;
    if buf.len() > max_bytes {
        return Err(FrontendError::from(format!(
            "File '{}' exceeds maximum allowed size ({} bytes)",
            path.display(),
            max_bytes
        )));
    }
    Ok(buf)
}

/// Atomically writes `bytes` to `path` via a unique `0600` temporary file in the same parent directory.
pub(crate) fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), FrontendError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| FrontendError::from(e.to_string()))?;
    let tmp_name = format!(
        ".tmp_{}_{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    );
    let tmp_path = parent.join(tmp_name);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let write_res: Result<(), std::io::Error> = (|| {
        let mut file = opts.open(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(e) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(FrontendError::from(e.to_string()));
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(FrontendError::from(e.to_string()));
    }
    Ok(())
}

/// Reads custom voucher standard TOMLs from `<app_data_dir>/voucher_standards`.
pub(crate) fn collect_custom_standards(app: &tauri::AppHandle) -> HashMap<String, String> {
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        return HashMap::new();
    };
    let standards_dir = app_data_dir.join("voucher_standards");
    collect_custom_standards_from_dir(&standards_dir)
}

/// Reads custom voucher standard TOMLs from a specific `voucher_standards` directory.
pub(crate) fn collect_custom_standards_from_dir(standards_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(base_meta) = fs::symlink_metadata(standards_dir) else {
        return map;
    };
    if base_meta.file_type().is_symlink() || !base_meta.file_type().is_dir() {
        return map;
    }
    let Ok(entries) = fs::read_dir(standards_dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(entry_meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if entry_meta.file_type().is_symlink() || !entry_meta.file_type().is_dir() {
            continue;
        }
        let standard_id = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !is_safe_filename_component(&standard_id) {
            continue;
        }
        let toml_path = path.join("standard.toml");
        if let Ok(bytes) = read_regular_file_bounded(&toml_path, MAX_CUSTOM_STANDARD_TOML_BYTES) {
            if let Ok(content) = String::from_utf8(bytes) {
                map.insert(standard_id, content);
            }
        }
    }
    map
}

/// Restores validated custom voucher standard TOMLs into `standards_base` without overwriting existing ones.
pub(crate) fn restore_custom_standards_to_dir(
    standards_base: &Path,
    standards: &HashMap<String, String>,
) -> Result<usize, FrontendError> {
    if let Ok(meta) = fs::symlink_metadata(standards_base) {
        if meta.file_type().is_symlink() {
            return Err(FrontendError::from(
                "Refusing to restore custom standards into a symlinked directory".to_string(),
            ));
        }
    }
    let mut count = 0;
    for (id, toml_content) in standards {
        if !is_safe_filename_component(id) {
            log::warn!("Skipping standard with suspicious path characters: {}", id);
            continue;
        }
        if VoucherStandardDefinition::from_toml(toml_content).is_err() {
            log::warn!("Skipping standard with invalid TOML definition: {}", id);
            continue;
        }
        let target_dir = standards_base.join(id);
        if let Ok(meta) = fs::symlink_metadata(&target_dir) {
            if meta.file_type().is_symlink() {
                return Err(FrontendError::from(format!(
                    "Refusing to restore custom standard '{}' through a symlink directory",
                    id
                )));
            }
        }
        let target_file = target_dir.join("standard.toml");
        match fs::symlink_metadata(&target_file) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(FrontendError::from(format!(
                        "Refusing to restore custom standard '{}' onto a symlink file",
                        id
                    )));
                }
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(FrontendError::from(e.to_string())),
        }
        fs::create_dir_all(&target_dir).map_err(|e| FrontendError::from(e.to_string()))?;
        write_file_atomic(&target_file, toml_content.as_bytes())?;
        count += 1;
    }
    Ok(count)
}

/// RAII guard that removes a temporary restore directory when dropped.
pub(crate) struct TempDirGuard(pub(crate) PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Removes stale temporary restore files and `pwreset_*` directories older than 1 hour,
/// and expires `active_recovery_session.json` after 7 days.
pub(crate) fn cleanup_stale_tmp_restore_files(tmp_dir: &Path) {
    cleanup_stale_tmp_restore_files_at(tmp_dir, std::time::SystemTime::now());
}

pub(crate) fn cleanup_stale_tmp_restore_files_at(tmp_dir: &Path, now: std::time::SystemTime) {
    if let Ok(entries) = fs::read_dir(tmp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                continue;
            }
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let Ok(elapsed) = now.duration_since(modified) else {
                continue;
            };
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_type.is_file() {
                let max_age = if file_name == "active_recovery_session.json" {
                    ACTIVE_RECOVERY_SESSION_MAX_AGE_SECS
                } else {
                    3600
                };
                if elapsed.as_secs() > max_age {
                    let _ = fs::remove_file(&path);
                }
            } else if file_type.is_dir()
                && file_name.starts_with("pwreset_")
                && elapsed.as_secs() > 3600
            {
                let _ = fs::remove_dir_all(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f4_corrupt_profile_metadata_fails_closed_without_overwrite() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("f4_corrupt_profile_meta"));
        let dir = &dir_guard.0;
        let meta_path = dir.join(PROFILE_METADATA_FILE);
        let corrupt = br#"{"broken": "#;
        std::fs::write(&meta_path, corrupt).unwrap();

        let err_load = load_profile_metadata_in_dir(dir)
            .expect_err("corrupt profile_metadata.json must fail load");
        assert!(
            err_load.contains("Failed to parse profile metadata"),
            "unexpected load error: {}",
            err_load
        );

        let err_rec = record_profile_metadata_in_dir(dir, "p1", Some("did:key:z1".into()))
            .expect_err("corrupt profile_metadata.json must fail record");
        assert!(
            err_rec.contains("Failed to parse profile metadata"),
            "unexpected record error: {}",
            err_rec
        );

        let err_rem = remove_profile_metadata_entry_in_dir(dir, "p1")
            .expect_err("corrupt profile_metadata.json must fail remove");
        assert!(
            err_rem.contains("Failed to parse profile metadata"),
            "unexpected remove error: {}",
            err_rem
        );

        assert!(
            load_profile_metadata_map_from_dir(dir).is_empty(),
            "display helper must fall back to empty map without mutating disk"
        );
        assert_eq!(
            std::fs::read(&meta_path).unwrap(),
            corrupt,
            "F4: corrupt profile_metadata.json must NEVER be overwritten on read/parse error"
        );
    }

    #[test]
    fn test_profile_metadata_roundtrip_preserves_user_id_and_removes_entry() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("profile_meta_roundtrip"));
        let dir = &dir_guard.0;

        assert!(
            load_profile_metadata_in_dir(dir).unwrap().is_empty(),
            "missing profile_metadata.json must load as empty map"
        );

        record_profile_metadata_in_dir(dir, "wallet_1", Some("did:key:zAlice".to_string()))
            .unwrap();
        record_profile_metadata_in_dir(dir, "wallet_1", None).unwrap();

        let loaded = load_profile_metadata_in_dir(dir).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded.get("wallet_1").and_then(|m| m.user_id.as_deref()),
            Some("did:key:zAlice"),
            "updating last_used with None user_id must preserve existing user_id"
        );

        let display_map = load_profile_metadata_map_from_dir(dir);
        assert_eq!(
            display_map.get("wallet_1").and_then(|(_, uid)| uid.as_deref()),
            Some("did:key:zAlice")
        );

        remove_profile_metadata_entry_in_dir(dir, "nonexistent").unwrap();
        remove_profile_metadata_entry_in_dir(dir, "wallet_1").unwrap();
        assert!(load_profile_metadata_in_dir(dir).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn s07_f16_write_file_atomic_sets_0600_and_restore_rejects_symlink_and_colon_id() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("s07_f16_fs"));
        let dir = &dir_guard.0;

        let target = dir.join("secret.json");
        write_file_atomic(&target, b"secret-data").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "S07-F16: write_file_atomic must create 0600 temp files on Unix"
        );

        let standards_base = dir.join("standards");
        let outside_dir = dir.join("outside");
        std::fs::create_dir_all(&standards_base).unwrap();
        std::fs::create_dir_all(&outside_dir).unwrap();
        symlink(&outside_dir, standards_base.join("sym_std")).unwrap();

        let valid_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );

        let mut bad_map = HashMap::new();
        bad_map.insert("bad:colon".to_string(), valid_toml.clone());
        bad_map.insert("invalid_toml".to_string(), "not = [valid toml".to_string());
        let skipped = restore_custom_standards_to_dir(&standards_base, &bad_map).unwrap();
        assert_eq!(skipped, 0, "unsafe id and invalid TOML must both be skipped");
        assert!(!standards_base.join("invalid_toml").exists());

        // Positive baseline: valid standard restores once and never overwrites existing file
        let mut good_map = HashMap::new();
        good_map.insert("good_std".to_string(), valid_toml.clone());
        let restored = restore_custom_standards_to_dir(&standards_base, &good_map).unwrap();
        assert_eq!(restored, 1);
        let existing_file = standards_base.join("good_std").join("standard.toml");
        std::fs::write(&existing_file, b"sentinel").unwrap();
        assert_eq!(
            restore_custom_standards_to_dir(&standards_base, &good_map).unwrap(),
            0,
            "existing custom standard must not be overwritten"
        );
        assert_eq!(std::fs::read(&existing_file).unwrap(), b"sentinel");

        // Negative: symlinked target_dir
        let mut sym_map = HashMap::new();
        sym_map.insert("sym_std".to_string(), valid_toml.clone());
        let sym_dir_err = restore_custom_standards_to_dir(&standards_base, &sym_map)
            .expect_err("symlinked target_dir must return error");
        assert!(
            sym_dir_err.message.contains("symlink directory"),
            "unexpected error for symlink dir: {}",
            sym_dir_err.message
        );
        assert!(
            !outside_dir.join("standard.toml").exists(),
            "outside symlink target must remain untouched"
        );

        // Negative: symlinked standards_base directory
        let sym_base = dir.join("sym_base");
        symlink(&standards_base, &sym_base).unwrap();
        let sym_base_err = restore_custom_standards_to_dir(&sym_base, &good_map)
            .expect_err("symlinked standards_base must return error");
        assert!(
            sym_base_err.message.contains("symlinked directory"),
            "unexpected error for symlink base: {}",
            sym_base_err.message
        );

        // Negative: symlinked target standard.toml file
        std::fs::create_dir_all(standards_base.join("sym_file_std")).unwrap();
        std::fs::write(outside_dir.join("outside.toml"), b"outside").unwrap();
        symlink(
            outside_dir.join("outside.toml"),
            standards_base.join("sym_file_std").join("standard.toml"),
        )
        .unwrap();
        let mut sym_file_map = HashMap::new();
        sym_file_map.insert("sym_file_std".to_string(), valid_toml);
        let sym_file_err = restore_custom_standards_to_dir(&standards_base, &sym_file_map)
            .expect_err("symlinked standard.toml target must return error");
        assert!(
            sym_file_err.message.contains("symlink file"),
            "unexpected error for symlink file: {}",
            sym_file_err.message
        );
        assert_eq!(
            std::fs::read(outside_dir.join("outside.toml")).unwrap(),
            b"outside"
        );
    }

    #[test]
    fn test_collect_custom_standards_rejects_symlinks_unsafe_ids_and_oversized_files() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("collect_custom_standards_sec"));
        let dir = &dir_guard.0;
        let standards_base = dir.join("standards");
        let outside_dir = dir.join("outside");
        std::fs::create_dir_all(standards_base.join("valid_std")).unwrap();
        std::fs::create_dir_all(standards_base.join("exact_max")).unwrap();
        std::fs::create_dir_all(standards_base.join("huge_std")).unwrap();
        std::fs::create_dir_all(standards_base.join("non_utf8")).unwrap();
        std::fs::create_dir_all(standards_base.join("bad:colon")).unwrap();
        std::fs::create_dir_all(&outside_dir).unwrap();

        std::fs::write(
            standards_base.join("valid_std").join("standard.toml"),
            "name = \"ok\"",
        )
        .unwrap();
        let exact_max_bytes = vec![b'#'; MAX_CUSTOM_STANDARD_TOML_BYTES];
        std::fs::write(
            standards_base.join("exact_max").join("standard.toml"),
            &exact_max_bytes,
        )
        .unwrap();
        std::fs::write(
            standards_base.join("huge_std").join("standard.toml"),
            vec![b'a'; MAX_CUSTOM_STANDARD_TOML_BYTES + 1],
        )
        .unwrap();
        std::fs::write(
            standards_base.join("non_utf8").join("standard.toml"),
            [0xffu8, 0xfe],
        )
        .unwrap();
        std::fs::write(
            standards_base.join("bad:colon").join("standard.toml"),
            "name = \"bad\"",
        )
        .unwrap();
        std::fs::write(outside_dir.join("standard.toml"), "secret = true").unwrap();

        // Missing or file (non-dir) standards_base returns empty map
        assert!(collect_custom_standards_from_dir(&dir.join("missing")).is_empty());
        let not_dir = dir.join("not_a_dir");
        std::fs::write(&not_dir, b"x").unwrap();
        assert!(collect_custom_standards_from_dir(&not_dir).is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&outside_dir, standards_base.join("sym_dir")).unwrap();
            std::fs::create_dir_all(standards_base.join("sym_file_std")).unwrap();
            symlink(
                outside_dir.join("standard.toml"),
                standards_base.join("sym_file_std").join("standard.toml"),
            )
            .unwrap();
            let sym_base = dir.join("sym_base");
            symlink(&standards_base, &sym_base).unwrap();
            assert!(
                collect_custom_standards_from_dir(&sym_base).is_empty(),
                "symlinked standards_dir base must yield empty map"
            );
        }

        let collected = collect_custom_standards_from_dir(&standards_base);
        assert_eq!(
            collected.get("valid_std").map(String::as_str),
            Some("name = \"ok\"")
        );
        assert_eq!(
            collected.get("exact_max").map(|s| s.len()),
            Some(MAX_CUSTOM_STANDARD_TOML_BYTES),
            "exact 1 MiB limit standard.toml must still be accepted"
        );
        assert!(
            !collected.contains_key("huge_std"),
            "oversized standard.toml (> 1 MiB) must not be collected"
        );
        assert!(
            !collected.contains_key("non_utf8"),
            "non-UTF-8 standard.toml must not be collected"
        );
        assert!(
            !collected.contains_key("bad:colon"),
            "unsafe directory name with ':' must not be collected"
        );
        #[cfg(unix)]
        {
            assert!(
                !collected.contains_key("sym_dir"),
                "symlink child directory must not be collected"
            );
            assert!(
                !collected.contains_key("sym_file_std"),
                "symlink standard.toml file must not be collected"
            );
        }
    }

    #[test]
    fn test_is_safe_filename_component_and_canonical_child_of() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("fs_safe_name_child"));
        let dir = &dir_guard.0;

        assert!(is_safe_filename_component("minuto_v1"));
        assert!(is_safe_filename_component("blob-01.enc"));
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a:b",
            " leading",
            "trailing ",
            "ctrl\x00byte",
            "del\x7fbyte",
        ] {
            assert!(
                !is_safe_filename_component(bad),
                "expected '{}' to be rejected",
                bad
            );
        }

        let child = dir.join("child.bin");
        write_file_atomic(&child, b"12345678").unwrap();
        assert!(is_canonical_child_of(&child, dir));
        assert!(!is_canonical_child_of(dir, dir));
        assert!(!is_canonical_child_of(&dir.join("missing.bin"), dir));

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let sym = dir.join("sym_child.bin");
            symlink(&child, &sym).unwrap();
            assert!(!is_canonical_child_of(&sym, dir));
        }
    }

    #[test]
    fn test_read_regular_file_bounded_enforces_limit_and_rejects_non_regular() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("fs_read_bounded"));
        let dir = &dir_guard.0;

        let child = dir.join("child.bin");
        write_file_atomic(&child, b"12345678").unwrap();
        assert_eq!(read_regular_file_bounded(&child, 8).unwrap(), b"12345678");

        let err_limit = read_regular_file_bounded(&child, 7)
            .expect_err("file exceeding max_bytes must be rejected");
        assert!(
            err_limit
                .message
                .contains("exceeds maximum allowed size (7 bytes)"),
            "unexpected size-limit error: {}",
            err_limit.message
        );

        let err_dir =
            read_regular_file_bounded(dir, 1024).expect_err("directory must not pass as file");
        assert!(
            err_dir.message.contains("is not a regular file"),
            "unexpected directory error: {}",
            err_dir.message
        );

        let err_missing = read_regular_file_bounded(&dir.join("missing.bin"), 1024)
            .expect_err("missing path must fail inspect");
        assert!(
            err_missing.message.contains("Failed to inspect"),
            "unexpected missing error: {}",
            err_missing.message
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let sym = dir.join("sym_child.bin");
            symlink(&child, &sym).unwrap();
            let err_sym =
                read_regular_file_bounded(&sym, 1024).expect_err("symlink must be rejected");
            assert!(
                err_sym.message.contains("is not a regular file"),
                "unexpected symlink error: {}",
                err_sym.message
            );
        }
    }

    #[test]
    fn test_resolve_or_create_fallback_instance_id_stable_and_replaces_empty() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("fs_fallback_id"));
        let dir = &dir_guard.0;
        let cfg_dir = dir.join("cfg");

        let id1 = resolve_or_create_fallback_instance_id_in_dir(&cfg_dir).unwrap();
        let id2 = resolve_or_create_fallback_instance_id_in_dir(&cfg_dir).unwrap();
        assert_eq!(id1, id2);
        assert!(uuid::Uuid::parse_str(&id1).is_ok());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(cfg_dir.join("instance_id_fallback"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        // Whitespace-only fallback file is replaced with a fresh valid UUID and persisted
        std::fs::write(cfg_dir.join("instance_id_fallback"), b"   \n\t ").unwrap();
        let replacement = resolve_or_create_fallback_instance_id_in_dir(&cfg_dir).unwrap();
        assert!(uuid::Uuid::parse_str(&replacement).is_ok());
        assert_ne!(replacement, id1);
        assert_eq!(
            std::fs::read_to_string(cfg_dir.join("instance_id_fallback")).unwrap(),
            replacement
        );
    }

    #[test]
    fn test_cleanup_stale_tmp_restore_files_at_respects_ttls_prefixes_and_symlinks() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("fs_cleanup_ttl"));
        let dir = &dir_guard.0;
        let tmp_dir = dir.join("tmp_restore");
        let outside = dir.join("outside");
        std::fs::create_dir_all(tmp_dir.join("pwreset_old")).unwrap();
        std::fs::create_dir_all(tmp_dir.join("other_dir")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(tmp_dir.join("pwreset_old").join("dummy"), b"x").unwrap();
        std::fs::write(tmp_dir.join("other_dir").join("keep"), b"k").unwrap();
        std::fs::write(tmp_dir.join("dropped_old.hmcbackup"), b"y").unwrap();
        std::fs::write(tmp_dir.join("active_recovery_session.json"), b"{}").unwrap();
        std::fs::write(outside.join("keep.txt"), b"outside").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&outside, tmp_dir.join("pwreset_sym")).unwrap();
        }

        let mtimes = [
            fs::symlink_metadata(tmp_dir.join("pwreset_old"))
                .unwrap()
                .modified()
                .unwrap(),
            fs::symlink_metadata(tmp_dir.join("dropped_old.hmcbackup"))
                .unwrap()
                .modified()
                .unwrap(),
            fs::symlink_metadata(tmp_dir.join("active_recovery_session.json"))
                .unwrap()
                .modified()
                .unwrap(),
        ];
        let earliest_mtime = *mtimes.iter().min().unwrap();
        let latest_mtime = *mtimes.iter().max().unwrap();
        let session_mtime = mtimes[2];

        // Exact 3600s boundary: elapsed.as_secs() <= 3600 -> everything survives
        cleanup_stale_tmp_restore_files_at(
            &tmp_dir,
            earliest_mtime + std::time::Duration::from_secs(3600),
        );
        assert!(tmp_dir.join("pwreset_old").exists());
        assert!(tmp_dir.join("dropped_old.hmcbackup").exists());
        assert!(tmp_dir.join("active_recovery_session.json").exists());

        // 3601s after latest creation: pwreset_* and dropped file expire; other_dir and session survive
        cleanup_stale_tmp_restore_files_at(
            &tmp_dir,
            latest_mtime + std::time::Duration::from_secs(3601),
        );
        assert!(!tmp_dir.join("pwreset_old").exists());
        assert!(!tmp_dir.join("dropped_old.hmcbackup").exists());
        assert!(tmp_dir.join("other_dir").join("keep").exists());
        assert!(tmp_dir.join("active_recovery_session.json").exists());

        // Exact 7-day boundary: active_recovery_session.json still survives
        cleanup_stale_tmp_restore_files_at(
            &tmp_dir,
            session_mtime + std::time::Duration::from_secs(ACTIVE_RECOVERY_SESSION_MAX_AGE_SECS),
        );
        assert!(tmp_dir.join("active_recovery_session.json").exists());

        // 7d + 1s: active_recovery_session.json expires; non-pwreset dir and symlink/outside survive
        cleanup_stale_tmp_restore_files_at(
            &tmp_dir,
            session_mtime + std::time::Duration::from_secs(ACTIVE_RECOVERY_SESSION_MAX_AGE_SECS + 1),
        );
        assert!(!tmp_dir.join("active_recovery_session.json").exists());
        assert!(tmp_dir.join("other_dir").join("keep").exists());
        #[cfg(unix)]
        {
            assert!(fs::symlink_metadata(tmp_dir.join("pwreset_sym")).is_ok());
            assert_eq!(std::fs::read(outside.join("keep.txt")).unwrap(), b"outside");
        }
    }
}
