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
    let Ok(entries) = fs::read_dir(standards_dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let standard_id = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let toml_path = path.join("standard.toml");
            if toml_path.exists() {
                if let Ok(content) = fs::read_to_string(&toml_path) {
                    map.insert(standard_id, content);
                }
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
        let dir = crate::test_support::temp_test_dir("f4_corrupt_profile_meta");
        std::fs::create_dir_all(&dir).unwrap();
        let meta_path = dir.join(PROFILE_METADATA_FILE);
        let corrupt = br#"{"broken": "#;
        std::fs::write(&meta_path, corrupt).unwrap();

        assert!(load_profile_metadata_in_dir(&dir).is_err());
        assert!(record_profile_metadata_in_dir(&dir, "p1", Some("did:key:z1".into())).is_err());
        assert!(remove_profile_metadata_entry_in_dir(&dir, "p1").is_err());
        assert_eq!(
            std::fs::read(&meta_path).unwrap(),
            corrupt,
            "F4: corrupt profile_metadata.json must NEVER be overwritten on read/parse error"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn s07_f16_write_file_atomic_sets_0600_and_restore_rejects_symlink_and_colon_id() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let dir = crate::test_support::temp_test_dir("s07_f16_fs");
        std::fs::create_dir_all(&dir).unwrap();

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
        let restored_colon = restore_custom_standards_to_dir(&standards_base, &bad_map).unwrap();
        assert_eq!(
            restored_colon, 0,
            "S07-F16: unsafe filename component with ':' must be rejected"
        );

        let mut sym_map = HashMap::new();
        sym_map.insert("sym_std".to_string(), valid_toml);
        let sym_res = restore_custom_standards_to_dir(&standards_base, &sym_map);
        assert!(
            sym_res.is_err() || !outside_dir.join("standard.toml").exists(),
            "S07-F16: restore_custom_standards_to_dir must NEVER write through symlink target_dir"
        );
        assert!(
            !outside_dir.join("standard.toml").exists(),
            "S07-F16: outside symlink target must remain untouched"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn s07_f4_f8_f14_f16_f20_fs_primitives_and_cleanup_invariants() {
        let dir = crate::test_support::temp_test_dir("s07_fs_primitives");
        std::fs::create_dir_all(&dir).unwrap();

        // 1. is_safe_filename_component
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

        // 2. is_canonical_child_of + read_regular_file_bounded
        let child = dir.join("child.bin");
        write_file_atomic(&child, b"12345678").unwrap();
        assert!(is_canonical_child_of(&child, &dir));
        assert!(!is_canonical_child_of(&dir, &dir));
        assert_eq!(read_regular_file_bounded(&child, 8).unwrap(), b"12345678");
        assert!(read_regular_file_bounded(&child, 7).is_err());
        assert!(read_regular_file_bounded(&dir, 1024).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let sym = dir.join("sym_child.bin");
            symlink(&child, &sym).unwrap();
            assert!(!is_canonical_child_of(&sym, &dir));
            assert!(read_regular_file_bounded(&sym, 1024).is_err());
        }

        // 3. resolve_or_create_fallback_instance_id_in_dir is stable across repeated calls
        let cfg_dir = dir.join("cfg");
        let id1 = resolve_or_create_fallback_instance_id_in_dir(&cfg_dir).unwrap();
        let id2 = resolve_or_create_fallback_instance_id_in_dir(&cfg_dir).unwrap();
        assert_eq!(id1, id2);
        assert!(!id1.is_empty());

        // 4. cleanup_stale_tmp_restore_files_at expires pwreset_* after 1h and active_recovery_session.json after 7d
        let tmp_dir = dir.join("tmp_restore");
        std::fs::create_dir_all(tmp_dir.join("pwreset_old")).unwrap();
        std::fs::write(tmp_dir.join("pwreset_old").join("dummy"), b"x").unwrap();
        std::fs::write(tmp_dir.join("dropped_old.hmcbackup"), b"y").unwrap();
        std::fs::write(tmp_dir.join("active_recovery_session.json"), b"{}").unwrap();

        let after_2h = std::time::SystemTime::now() + std::time::Duration::from_secs(7200);
        cleanup_stale_tmp_restore_files_at(&tmp_dir, after_2h);
        assert!(
            !tmp_dir.join("pwreset_old").exists(),
            "S07-F20: stale pwreset_* directory must be swept after 1 hour"
        );
        assert!(!tmp_dir.join("dropped_old.hmcbackup").exists());
        assert!(
            tmp_dir.join("active_recovery_session.json").exists(),
            "active_recovery_session.json must survive < 7 days"
        );

        let after_8d = std::time::SystemTime::now() + std::time::Duration::from_secs(8 * 86_400);
        cleanup_stale_tmp_restore_files_at(&tmp_dir, after_8d);
        assert!(
            !tmp_dir.join("active_recovery_session.json").exists(),
            "S07-F4: active_recovery_session.json must expire after 7 days"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
