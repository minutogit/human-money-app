use crate::models::FrontendError;
use chrono::Utc;
use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::Manager;

const PROFILE_METADATA_FILE: &str = "profile_metadata.json";

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
    let fallback_path = app_config_dir.join("instance_id_fallback");

    if fallback_path.exists() {
        let id = fs::read_to_string(&fallback_path).map_err(|e| e.to_string())?;
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() {
            info!("Device binding: Using fallback file-based ID.");
            return Ok(trimmed);
        }
    }

    if !app_config_dir.exists() {
        fs::create_dir_all(&app_config_dir).map_err(|e| e.to_string())?;
    }
    let new_id = uuid::Uuid::new_v4().to_string();
    fs::write(&fallback_path, &new_id).map_err(|e| e.to_string())?;
    warn!("Device binding: Generated new fallback instance ID for sandboxed environment.");
    Ok(new_id)
}

/// Atomically writes `bytes` to `path` via a unique temporary file in the same parent directory.
pub(crate) fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), FrontendError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| FrontendError::from(e.to_string()))?;
    let tmp_name = format!(
        ".tmp_{}_{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    );
    let tmp_path = parent.join(tmp_name);
    {
        let mut file =
            fs::File::create(&tmp_path).map_err(|e| FrontendError::from(e.to_string()))?;
        file.write_all(bytes)
            .map_err(|e| FrontendError::from(e.to_string()))?;
        file.sync_all()
            .map_err(|e| FrontendError::from(e.to_string()))?;
    }
    fs::rename(&tmp_path, path).map_err(|e| FrontendError::from(e.to_string()))?;
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
    let mut count = 0;
    for (id, toml_content) in standards {
        if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
            log::warn!("Skipping standard with suspicious path characters: {}", id);
            continue;
        }
        if VoucherStandardDefinition::from_toml(toml_content).is_err() {
            log::warn!("Skipping standard with invalid TOML definition: {}", id);
            continue;
        }
        let target_dir = standards_base.join(id);
        let target_file = target_dir.join("standard.toml");
        if !target_file.exists() {
            fs::create_dir_all(&target_dir).map_err(|e| FrontendError::from(e.to_string()))?;
            fs::write(&target_file, toml_content)
                .map_err(|e| FrontendError::from(e.to_string()))?;
            count += 1;
        }
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

/// Removes stale temporary restore files older than 1 hour, preserving `active_recovery_session.json`.
pub(crate) fn cleanup_stale_tmp_restore_files(tmp_dir: &Path) {
    if let Ok(entries) = fs::read_dir(tmp_dir) {
        let now = std::time::SystemTime::now();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if path.file_name().and_then(|n| n.to_str()) == Some("active_recovery_session.json")
                {
                    continue;
                }
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = now.duration_since(modified) {
                            if elapsed.as_secs() > 3600 {
                                let _ = fs::remove_file(path);
                            }
                        }
                    }
                }
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
}
