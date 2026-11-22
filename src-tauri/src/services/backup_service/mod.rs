use crate::models::{
    BackupProtectionMode, FrontendError, MnemonicLanguage, PortableBackupContainer,
};
use crate::services::fs_helpers::{is_safe_filename_component, read_regular_file_bounded};
use human_money_core::models::backup::{BackupProtection, BackupRestoreCredentials};
use human_money_core::models::storage_integrity::StorageIntegrityRecord;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tauri::Manager;

mod local_io;
mod probe;
mod restore;
mod sync_upload;

#[cfg(test)]
mod tests;

#[cfg(test)]
use local_io::{export_backup_file_impl, preview_mnemonic_identity_impl};
pub(crate) use local_io::{
    export_wallet_backup_file_impl, find_local_profile_collision_impl,
    inspect_backup_with_collision_impl, inspect_wallet_backup_impl,
    preview_mnemonic_identity_for_app_impl, stage_dropped_backup_bytes_impl,
    sync_backup_folder_impl, sync_wallet_backup_folder_impl, user_ids_from_metadata_dir,
};
pub(crate) use probe::{probe_sync_backup_for_app_impl, probe_sync_backup_impl};
pub(crate) use restore::{
    dismiss_wallet_sync_alert_impl, finalize_restored_wallet_and_reconcile_l2,
    probe_wallet_backup_impl, reconcile_restored_wallet_l2_impl,
    reset_local_password_via_backup_for_app_impl, restore_wallet_backup_impl,
};
#[cfg(test)]
use restore::{reset_local_password_via_backup_impl, restore_backup_impl};
pub use sync_upload::{compute_backup_status_dto, sync_backup_impl};
pub(crate) use sync_upload::{
    fetch_and_merge_backup_slot, get_backup_status_impl, run_auto_backup_sync_impl,
};
#[cfg(test)]
use sync_upload::{
    is_older_than_14_days, merge_backup_delta_into_base, sync_backup_with_standards_impl,
};

const BACKUP_BASE_TTL_SECS: u64 = 7 * 24 * 3600; // 7 days
const MAX_DROPPED_BACKUP_BYTES: usize = 64 * 1024 * 1024;
const BACKUP_CONTAINER_FORMAT_V1: &str = "humoco-app-backup-v1";
const BACKUP_DELTA_FORMAT_V1: &str = "humoco-app-backup-delta-v1";
const MAX_PROBE_SYNC_INPUTS: usize = 32;

fn parse_portable_backup_container(bytes: &[u8]) -> Result<PortableBackupContainer, FrontendError> {
    let container: PortableBackupContainer = serde_json::from_slice(bytes)
        .map_err(|e| FrontendError::from(format!("Invalid backup container: {}", e)))?;
    if container.container_format != BACKUP_CONTAINER_FORMAT_V1 {
        return Err(FrontendError::from(format!(
            "Unsupported backup container format: {}",
            container.container_format
        )));
    }
    Ok(container)
}

fn resolve_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, FrontendError> {
    app.path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))
}

fn encode_blobs_hex(blobs: HashMap<String, Vec<u8>>) -> HashMap<String, String> {
    let mut blobs_hex = HashMap::with_capacity(blobs.len());
    for (name, data) in blobs {
        blobs_hex.insert(name, human_money_core::hex::encode(&data));
    }
    blobs_hex
}

fn parse_credentials<'a>(
    credential_mode: &str,
    backup_password: Option<&'a str>,
    mnemonic: Option<&'a str>,
    passphrase: Option<&'a str>,
    user_prefix: Option<&'a str>,
    language: Option<MnemonicLanguage>,
) -> Result<BackupRestoreCredentials<'a>, FrontendError> {
    match credential_mode {
        "password" => {
            let pwd = backup_password
                .ok_or_else(|| FrontendError::from("Missing backup password".to_string()))?;
            if pwd.is_empty() {
                return Err(FrontendError::from(
                    "Backup password cannot be empty".to_string(),
                ));
            }
            Ok(BackupRestoreCredentials::BackupPassword(pwd))
        }
        "mnemonic" => {
            let phrase = mnemonic
                .ok_or_else(|| FrontendError::from("Missing mnemonic phrase".to_string()))?;
            if phrase.trim().is_empty() {
                return Err(FrontendError::from(
                    "Mnemonic phrase cannot be empty".to_string(),
                ));
            }
            let lang: human_money_core::MnemonicLanguage =
                language.unwrap_or(MnemonicLanguage::English).into();
            Ok(BackupRestoreCredentials::Mnemonic {
                phrase,
                passphrase,
                prefix: user_prefix,
                language: lang,
            })
        }
        _ => Err(FrontendError::from(format!(
            "Invalid credential mode: {}",
            credential_mode
        ))),
    }
}

fn parse_protection_mode<'a>(
    protection_mode: BackupProtectionMode,
    backup_password: Option<&'a str>,
) -> Result<(&'static str, BackupProtection<'a>), FrontendError> {
    match protection_mode {
        BackupProtectionMode::MnemonicOnly => Ok(("mnemonic", BackupProtection::MnemonicOnly)),
        BackupProtectionMode::CustomPassword => {
            let pwd = backup_password.unwrap_or("");
            if pwd.is_empty() {
                return Err(FrontendError::from(
                    "Backup password cannot be empty".to_string(),
                ));
            }
            Ok((
                "backup_password",
                BackupProtection::CustomBackupPassword(pwd),
            ))
        }
    }
}

fn is_safe_folder_name(folder: &str) -> bool {
    !folder.is_empty()
        && folder
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn verify_integrity_record_signature(
    record: &StorageIntegrityRecord,
    target_user_id: &str,
) -> bool {
    let Ok(pubkey) = human_money_core::crypto::get_pubkey_from_user_id(target_user_id) else {
        return false;
    };
    let Ok(payload_canonical) = human_money_core::to_canonical_json(&record.payload) else {
        return false;
    };
    let payload_hash = human_money_core::crypto::get_hash(payload_canonical.as_bytes());
    let Ok(sig_vec) = bs58::decode(&record.signature).into_vec() else {
        return false;
    };
    let Ok(sig_bytes) = <[u8; 64]>::try_from(sig_vec.as_slice()) else {
        return false;
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    human_money_core::crypto::verify_ed25519(&pubkey, payload_hash.as_bytes(), &signature)
}

struct BackupParts {
    manifest_bytes: Vec<u8>,
    blobs: HashMap<String, Vec<u8>>,
    custom_standards: HashMap<String, String>,
}

fn read_backup_parts(source_path: &Path) -> Result<BackupParts, FrontendError> {
    let source_meta = fs::symlink_metadata(source_path).map_err(|_| {
        FrontendError::from(format!("Backup path not found: {}", source_path.display()))
    })?;
    if source_meta.file_type().is_file() {
        let bytes = read_regular_file_bounded(source_path, MAX_DROPPED_BACKUP_BYTES)?;
        let container = parse_portable_backup_container(&bytes)?;
        let m_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        let mut blob_map = HashMap::with_capacity(container.blobs_hex.len());
        for (name, hex_data) in container.blobs_hex {
            let data = human_money_core::hex::decode(&hex_data).map_err(|e| {
                FrontendError::from(format!("Invalid hex encoding for blob {}: {}", name, e))
            })?;
            blob_map.insert(name, data);
        }
        Ok(BackupParts {
            manifest_bytes: m_bytes,
            blobs: blob_map,
            custom_standards: container.custom_standards,
        })
    } else if source_meta.file_type().is_dir() {
        let manifest_path = source_path.join("manifest.enc");
        let m_bytes = read_regular_file_bounded(&manifest_path, MAX_DROPPED_BACKUP_BYTES)?;
        let blobs_dir = source_path.join("blobs");
        let mut blob_map = HashMap::new();
        if blobs_dir.exists() {
            for entry in fs::read_dir(&blobs_dir).map_err(|e| FrontendError::from(e.to_string()))? {
                let entry = entry.map_err(|e| FrontendError::from(e.to_string()))?;
                let p = entry.path();
                if let Ok(meta) = fs::symlink_metadata(&p) {
                    if meta.file_type().is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if !is_safe_filename_component(&name) {
                            continue;
                        }
                        let data = read_regular_file_bounded(&p, MAX_DROPPED_BACKUP_BYTES)?;
                        blob_map.insert(name, data);
                    }
                }
            }
        }
        let standards_dir = source_path.join("standards");
        let mut std_map = HashMap::new();
        if standards_dir.exists() {
            for entry in
                fs::read_dir(&standards_dir).map_err(|e| FrontendError::from(e.to_string()))?
            {
                let entry = entry.map_err(|e| FrontendError::from(e.to_string()))?;
                let p = entry.path();
                if let Ok(meta) = fs::symlink_metadata(&p) {
                    if meta.file_type().is_file() && p.extension().is_some_and(|ext| ext == "toml")
                    {
                        let stem = p
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        if !is_safe_filename_component(&stem) {
                            continue;
                        }
                        if let Ok(bytes) = read_regular_file_bounded(&p, 1024 * 1024) {
                            if let Ok(content) = String::from_utf8(bytes) {
                                std_map.insert(stem, content);
                            }
                        }
                    }
                }
            }
        }
        Ok(BackupParts {
            manifest_bytes: m_bytes,
            blobs: blob_map,
            custom_standards: std_map,
        })
    } else {
        Err(FrontendError::from(format!(
            "Backup path is not a regular file or directory: {}",
            source_path.display()
        )))
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    human_money_core::hex::encode(hasher.finalize())
}
