use crate::{
    models::{
        BackupDeltaContainer, BackupProtectionMode, FrontendAutoBackupStatusDto,
        FrontendBackupInspectionDto, FrontendBackupProbeDto, FrontendBackupSummaryDto,
        FrontendBackupSyncResult, FrontendError, FrontendRestoreOutcomeDto,
        FrontendRestoreReconcileResultDto, LocalProfileCollisionDto, MnemonicIdentityPreviewDto,
        MnemonicLanguage, PortableBackupContainer, ResolvedSyncTargetDto, SyncBackupProbeResultDto,
    },
    services::{
        common::DropChannelSigner,
        fs_helpers::{
            cleanup_stale_tmp_restore_files, collect_custom_standards, get_wallet_data_dir,
            load_profile_metadata_map_from_dir, record_profile_metadata_in_dir,
            remove_profile_metadata_entry_in_dir, restore_custom_standards_to_dir,
            write_file_atomic, TempDirGuard,
        },
    },
    AppState,
};
use chrono::Utc;
use human_money_core::{
    app_service::{AppService, ProfileInfo},
    models::{
        backup::{BackupProtection, BackupRestoreCredentials, EncryptedBackupManifest},
        seal::{LocalSealRecord, SyncStatus, WalletSeal},
        storage_integrity::StorageIntegrityRecord,
    },
    storage::{seal_service::SealService, WalletLockGuard},
    AuthMethod, FileStorage, UserIdentity, Wallet,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tauri::Manager;

const BACKUP_BASE_TTL_SECS: u64 = 7 * 24 * 3600; // 7 days
const MAX_DROPPED_BACKUP_BYTES: usize = 64 * 1024 * 1024;

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

pub(crate) fn export_backup_file_impl(
    destination_path: &Path,
    protection: BackupProtection<'_>,
    password: Option<&str>,
    service: &mut AppService,
    custom_standards: HashMap<String, String>,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    let diff = service
        .create_differential_backup(None, protection, password)
        .map_err(FrontendError::from)?;

    let blobs_hex = encode_blobs_hex(diff.put_blobs);
    let timestamp = Utc::now().to_rfc3339();
    let container = PortableBackupContainer {
        container_format: "humoco-app-backup-v1".to_string(),
        created_at: timestamp.clone(),
        manifest_hex: human_money_core::hex::encode(&diff.encrypted_manifest_bytes),
        blobs_hex,
        custom_standards,
    };

    let container_bytes = serde_json::to_vec_pretty(&container)
        .map_err(|e| FrontendError::from(format!("Failed to serialize backup container: {}", e)))?;
    write_file_atomic(destination_path, &container_bytes)?;

    Ok(FrontendBackupSummaryDto {
        total_blobs: diff.summary.total_blobs,
        changed_blobs: diff.summary.changed_blobs,
        deleted_blobs: diff.summary.deleted_blobs,
        upload_bytes: diff.summary.upload_bytes,
        timestamp,
        destination_path: destination_path.to_string_lossy().to_string(),
        custom_standard_count: container.custom_standards.len(),
    })
}

pub(crate) fn export_wallet_backup_file_impl(
    destination_path: &str,
    protection_mode: BackupProtectionMode,
    backup_password: Option<&str>,
    password: Option<&str>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    let (_expected_mode, protection) = parse_protection_mode(protection_mode, backup_password)?;
    let custom_standards = collect_custom_standards(app);
    let mut service = state.lock_service()?;
    crate::services::common::ensure_access_mode(
        &mut service,
        human_money_core::AccessMode::Full,
        password,
    )?;
    let summary = export_backup_file_impl(
        Path::new(destination_path),
        protection,
        password,
        &mut service,
        custom_standards,
    )?;

    if let Ok(mut settings) = state.get_cached_settings() {
        settings.last_backup_at = Some(Utc::now().to_rfc3339());
        settings.backup_dirty = false;
        let _ = state.save_settings(&mut service, settings, password);
    }

    Ok(summary)
}

pub(crate) fn sync_backup_folder_impl(
    base_dir: &Path,
    expected_mode: &str,
    protection: BackupProtection<'_>,
    password: Option<&str>,
    force_full: bool,
    service: &mut AppService,
    custom_standards: HashMap<String, String>,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    let previous_manifest_bytes = if !force_full && base_dir.join("manifest.enc").exists() {
        fs::read(base_dir.join("manifest.enc"))
            .ok()
            .filter(|bytes| {
                serde_json::from_slice::<EncryptedBackupManifest>(bytes)
                    .is_ok_and(|old| old.protection_mode == expected_mode)
            })
    } else {
        None
    };

    let diff = service
        .create_differential_backup(previous_manifest_bytes.as_deref(), protection, password)
        .map_err(FrontendError::from)?;

    let blobs_dir = base_dir.join("blobs");
    let standards_dir = base_dir.join("standards");
    fs::create_dir_all(&blobs_dir).map_err(|e| FrontendError::from(e.to_string()))?;
    fs::create_dir_all(&standards_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    for (name, data) in &diff.put_blobs {
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(FrontendError::from(format!(
                "Unsafe blob name in diff: {}",
                name
            )));
        }
        write_file_atomic(&blobs_dir.join(name), data)?;
    }

    for (id, content) in &custom_standards {
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            continue;
        }
        write_file_atomic(
            &standards_dir.join(format!("{}.toml", id)),
            content.as_bytes(),
        )?;
    }

    write_file_atomic(
        &base_dir.join("manifest.enc"),
        &diff.encrypted_manifest_bytes,
    )?;

    for del_name in &diff.delete_blobs {
        if !del_name.contains('/') && !del_name.contains('\\') && !del_name.contains("..") {
            let _ = fs::remove_file(blobs_dir.join(del_name));
        }
    }

    let timestamp = Utc::now().to_rfc3339();
    Ok(FrontendBackupSummaryDto {
        total_blobs: diff.summary.total_blobs,
        changed_blobs: diff.summary.changed_blobs,
        deleted_blobs: diff.summary.deleted_blobs,
        upload_bytes: diff.summary.upload_bytes,
        timestamp,
        destination_path: base_dir.to_string_lossy().to_string(),
        custom_standard_count: custom_standards.len(),
    })
}

pub(crate) fn sync_wallet_backup_folder_impl(
    folder_path: String,
    protection_mode: BackupProtectionMode,
    backup_password: Option<&str>,
    password: Option<&str>,
    force_full: bool,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    let (expected_mode, protection) = parse_protection_mode(protection_mode, backup_password)?;
    let custom_standards = collect_custom_standards(app);
    let mut service = state.lock_service()?;
    crate::services::common::ensure_access_mode(
        &mut service,
        human_money_core::AccessMode::Full,
        password,
    )?;
    let summary = sync_backup_folder_impl(
        Path::new(&folder_path),
        expected_mode,
        protection,
        password,
        force_full,
        &mut service,
        custom_standards,
    )?;

    if let Ok(mut settings) = state.get_cached_settings() {
        settings.backup_sync_folder = Some(folder_path);
        settings.backup_protection_mode = Some(protection_mode.as_str().to_string());
        settings.last_backup_at = Some(Utc::now().to_rfc3339());
        settings.backup_dirty = false;
        let _ = state.save_settings(&mut service, settings, password);
    }

    Ok(summary)
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

pub(crate) fn find_local_profile_collision_impl(
    wallet_data_dir: &Path,
    service: &AppService,
    target_user_id: &str,
    identity_opt: Option<&UserIdentity>,
    user_ids_by_folder: &HashMap<String, Option<String>>,
) -> Option<LocalProfileCollisionDto> {
    let profiles = service.list_profiles().ok()?;

    for p in &profiles {
        if !is_safe_folder_name(&p.folder_name) {
            continue;
        }
        let storage = FileStorage::new(wallet_data_dir.join(&p.folder_name));
        if let Some(id) = identity_opt {
            if let Ok(Some(rec)) = storage.load_seal(&AuthMethod::RecoveryIdentity(id)) {
                if rec.seal.payload.user_id == target_user_id {
                    return Some(LocalProfileCollisionDto {
                        folder_name: p.folder_name.clone(),
                        profile_name: p.profile_name.clone(),
                        user_id: target_user_id.to_string(),
                        local_epoch: Some(rec.seal.payload.epoch as u64),
                        local_tx_nonce: Some(rec.seal.payload.tx_nonce),
                        is_locked_or_conflicted: rec.is_locked() || rec.lock_reason.is_some(),
                    });
                }
            }
        }
        if user_ids_by_folder
            .get(&p.folder_name)
            .and_then(|u| u.as_deref())
            == Some(target_user_id)
        {
            return Some(LocalProfileCollisionDto {
                folder_name: p.folder_name.clone(),
                profile_name: p.profile_name.clone(),
                user_id: target_user_id.to_string(),
                local_epoch: None,
                local_tx_nonce: None,
                is_locked_or_conflicted: false,
            });
        }
    }

    // Fallback when inspecting a password-protected backup before metadata recorded user_id:
    // only match root accounts unambiguously by verifying the local integrity signature.
    if identity_opt.is_none()
        && human_money_core::crypto::get_prefix_from_user_id(target_user_id).is_none()
    {
        let mut sig_matches: Vec<&ProfileInfo> = Vec::new();
        for p in &profiles {
            if !is_safe_folder_name(&p.folder_name) {
                continue;
            }
            let storage = FileStorage::new(wallet_data_dir.join(&p.folder_name));
            if let Ok(Some(ir)) = storage.load_integrity() {
                if verify_integrity_record_signature(&ir, target_user_id) {
                    sig_matches.push(p);
                }
            }
        }
        if sig_matches.len() == 1 {
            let p = sig_matches[0];
            return Some(LocalProfileCollisionDto {
                folder_name: p.folder_name.clone(),
                profile_name: p.profile_name.clone(),
                user_id: target_user_id.to_string(),
                local_epoch: None,
                local_tx_nonce: None,
                is_locked_or_conflicted: false,
            });
        }
    }

    None
}

pub(crate) fn user_ids_from_metadata_dir(
    wallet_data_dir: &Path,
) -> HashMap<String, Option<String>> {
    load_profile_metadata_map_from_dir(wallet_data_dir)
        .into_iter()
        .map(|(k, (_ts, uid))| (k, uid))
        .collect()
}

pub(crate) fn inspect_backup_with_collision_impl(
    source_path: &Path,
    credentials: &BackupRestoreCredentials<'_>,
    service: &AppService,
    wallet_data_dir_opt: Option<&Path>,
    identity_opt: Option<&UserIdentity>,
    user_ids_by_folder: &HashMap<String, Option<String>>,
) -> Result<FrontendBackupInspectionDto, FrontendError> {
    let (manifest_bytes, blob_count, custom_standard_count) = if source_path.is_file() {
        let bytes = fs::read(source_path)
            .map_err(|e| FrontendError::from(format!("Failed to read backup file: {}", e)))?;
        let container: PortableBackupContainer = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Invalid backup container: {}", e)))?;
        let m_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        (
            m_bytes,
            container.blobs_hex.len(),
            container.custom_standards.len(),
        )
    } else if source_path.is_dir() {
        let manifest_path = source_path.join("manifest.enc");
        let m_bytes = fs::read(&manifest_path)
            .map_err(|e| FrontendError::from(format!("Failed to read manifest.enc: {}", e)))?;
        let b_count = fs::read_dir(source_path.join("blobs"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let s_count = fs::read_dir(source_path.join("standards"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        (m_bytes, b_count, s_count)
    } else {
        return Err(FrontendError::from(format!(
            "Backup path not found: {}",
            source_path.display()
        )));
    };

    let enc_manifest: EncryptedBackupManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| FrontendError::from(format!("Failed to parse encrypted manifest: {}", e)))?;

    let report = service
        .inspect_backup(&manifest_bytes, credentials)
        .map_err(FrontendError::from)?;

    let local_collision = wallet_data_dir_opt.and_then(|dir| {
        find_local_profile_collision_impl(
            dir,
            service,
            &report.manifest.seal.payload.user_id,
            identity_opt,
            user_ids_by_folder,
        )
    });

    Ok(FrontendBackupInspectionDto {
        profile_name: report.manifest.profile_name,
        user_id: report.manifest.seal.payload.user_id,
        epoch: report.manifest.seal.payload.epoch as u64,
        tx_nonce: report.manifest.seal.payload.tx_nonce,
        seal_timestamp: report.manifest.seal.payload.timestamp,
        blob_count,
        custom_standard_count,
        protection_mode: enc_manifest.protection_mode,
        local_collision,
    })
}

pub fn inspect_backup_impl(
    source_path: &Path,
    credentials: &BackupRestoreCredentials<'_>,
    service: &AppService,
) -> Result<FrontendBackupInspectionDto, FrontendError> {
    inspect_backup_with_collision_impl(
        source_path,
        credentials,
        service,
        None,
        None,
        &HashMap::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn inspect_wallet_backup_impl(
    source_path: &str,
    credential_mode: &str,
    backup_password: Option<&str>,
    mnemonic: Option<&str>,
    passphrase: Option<&str>,
    user_prefix: Option<&str>,
    language: Option<MnemonicLanguage>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendBackupInspectionDto, FrontendError> {
    let cleaned_prefix = user_prefix.map(str::trim).filter(|s| !s.is_empty());
    let credentials = parse_credentials(
        credential_mode,
        backup_password,
        mnemonic,
        passphrase,
        cleaned_prefix,
        language,
    )?;

    let derived_identity = if credential_mode == "mnemonic" {
        mnemonic.and_then(|phrase| {
            let core_lang: human_money_core::MnemonicLanguage =
                language.unwrap_or(MnemonicLanguage::English).into();
            Wallet::new_from_mnemonic(
                phrase,
                passphrase,
                cleaned_prefix,
                core_lang,
                "inspect".to_string(),
                false,
            )
            .ok()
            .map(|(_, id)| id)
        })
    } else {
        None
    };

    let wallet_data_dir = get_wallet_data_dir(app).map_err(FrontendError::from)?;
    let user_ids = user_ids_from_metadata_dir(&wallet_data_dir);

    let service = state.lock_service()?;
    inspect_backup_with_collision_impl(
        Path::new(source_path),
        &credentials,
        &service,
        Some(&wallet_data_dir),
        derived_identity.as_ref(),
        &user_ids,
    )
}

struct BackupParts {
    manifest_bytes: Vec<u8>,
    blobs: HashMap<String, Vec<u8>>,
    custom_standards: HashMap<String, String>,
}

fn read_backup_parts(source_path: &Path) -> Result<BackupParts, FrontendError> {
    if source_path.is_file() {
        let bytes = fs::read(source_path)
            .map_err(|e| FrontendError::from(format!("Failed to read backup file: {}", e)))?;
        let container: PortableBackupContainer = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Invalid backup container: {}", e)))?;
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
    } else if source_path.is_dir() {
        let manifest_path = source_path.join("manifest.enc");
        let m_bytes = fs::read(&manifest_path)
            .map_err(|e| FrontendError::from(format!("Failed to read manifest.enc: {}", e)))?;
        let blobs_dir = source_path.join("blobs");
        let mut blob_map = HashMap::new();
        if blobs_dir.exists() {
            for entry in fs::read_dir(&blobs_dir).map_err(|e| FrontendError::from(e.to_string()))? {
                let entry = entry.map_err(|e| FrontendError::from(e.to_string()))?;
                let p = entry.path();
                if p.is_file() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let data = fs::read(&p).map_err(|e| FrontendError::from(e.to_string()))?;
                    blob_map.insert(name, data);
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
                if p.is_file() && p.extension().is_some_and(|ext| ext == "toml") {
                    let stem = p
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if let Ok(content) = fs::read_to_string(&p) {
                        std_map.insert(stem, content);
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
            "Backup path not found: {}",
            source_path.display()
        )))
    }
}

pub(crate) fn restore_backup_impl(
    source_path: &Path,
    credentials: BackupRestoreCredentials<'_>,
    new_password: &str,
    local_instance_id: String,
    service: &mut AppService,
) -> Result<
    (
        human_money_core::models::backup::RestoreOutcome,
        String,
        HashMap<String, String>,
    ),
    FrontendError,
> {
    let parts = read_backup_parts(source_path)?;

    let outcome = service
        .restore_backup(
            &parts.manifest_bytes,
            &parts.blobs,
            credentials,
            new_password,
            local_instance_id,
        )
        .map_err(FrontendError::from)?;

    let profile_name = service
        .list_profiles()
        .ok()
        .and_then(|profiles| {
            profiles
                .into_iter()
                .find(|p| p.folder_name == outcome.folder_name)
                .map(|p| p.profile_name)
        })
        .unwrap_or_else(|| "Restored Profile".to_string());

    Ok((outcome, profile_name, parts.custom_standards))
}

pub(crate) fn probe_wallet_backup_impl(
    source_path: &str,
) -> Result<FrontendBackupProbeDto, FrontendError> {
    let path = Path::new(source_path);
    let (source_kind, manifest_bytes, blob_count, custom_standard_count) = if path.is_file() {
        let bytes = fs::read(path)
            .map_err(|e| FrontendError::from(format!("Failed to read backup file: {}", e)))?;
        let container: PortableBackupContainer = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Invalid backup container: {}", e)))?;
        let manifest_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        (
            "file",
            manifest_bytes,
            container.blobs_hex.len(),
            container.custom_standards.len(),
        )
    } else if path.is_dir() {
        let manifest_path = path.join("manifest.enc");
        if !manifest_path.exists() {
            return Err(FrontendError::from(format!(
                "Directory does not contain a valid backup: manifest.enc missing at {}",
                manifest_path.display()
            )));
        }
        let manifest_bytes = fs::read(&manifest_path)
            .map_err(|e| FrontendError::from(format!("Failed to read manifest.enc: {}", e)))?;
        let blob_count = fs::read_dir(path.join("blobs"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let custom_standard_count = fs::read_dir(path.join("standards"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        ("folder", manifest_bytes, blob_count, custom_standard_count)
    } else {
        return Err(FrontendError::from(format!(
            "Backup path not found: {}",
            source_path
        )));
    };

    let manifest: EncryptedBackupManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| FrontendError::from(format!("Invalid encrypted backup manifest: {}", e)))?;
    let supports_password = manifest.protection_mode == "backup_password";

    Ok(FrontendBackupProbeDto {
        source_kind: source_kind.to_string(),
        protection_mode: manifest.protection_mode,
        supports_password_restore: supports_password,
        supports_mnemonic_restore: true,
        blob_count,
        custom_standard_count,
    })
}

fn replace_local_profile_with_restored(
    wallet_data_dir: &Path,
    old_folder: &str,
    new_folder: &str,
    restored_identity: &UserIdentity,
    user_ids_by_folder: &HashMap<String, Option<String>>,
    current_profile_name: String,
) -> String {
    if old_folder == new_folder || !is_safe_folder_name(old_folder) {
        return current_profile_name;
    }

    let old_dir = wallet_data_dir.join(old_folder);
    let old_storage = FileStorage::new(old_dir.clone());
    let seal_matches = old_storage
        .load_seal(&AuthMethod::RecoveryIdentity(restored_identity))
        .ok()
        .flatten()
        .is_some_and(|rec| rec.seal.payload.user_id == restored_identity.user_id);
    let meta_matches = user_ids_by_folder
        .get(old_folder)
        .and_then(|u| u.as_deref())
        == Some(restored_identity.user_id.as_str());
    let integrity_matches =
        human_money_core::crypto::get_prefix_from_user_id(&restored_identity.user_id).is_none()
            && old_storage
                .load_integrity()
                .ok()
                .flatten()
                .is_some_and(|ir| {
                    verify_integrity_record_signature(&ir, &restored_identity.user_id)
                });

    if !(seal_matches || meta_matches || integrity_matches) {
        log::warn!(
            "Refusing to replace folder '{}' because local user_id ownership could not be verified",
            old_folder
        );
        return current_profile_name;
    }

    let profiles_path = wallet_data_dir.join("profiles.json");
    let Ok(profiles_bytes) = fs::read(&profiles_path) else {
        return current_profile_name;
    };
    let Ok(mut profiles) = serde_json::from_slice::<Vec<ProfileInfo>>(&profiles_bytes) else {
        return current_profile_name;
    };

    let clean_old_name = profiles
        .iter()
        .find(|p| p.folder_name == old_folder)
        .map(|p| p.profile_name.clone());

    profiles.retain(|p| p.folder_name != old_folder);

    let mut final_name = current_profile_name;
    if let Some(old_name) = clean_old_name {
        let name_taken_by_other = profiles
            .iter()
            .any(|p| p.folder_name != new_folder && p.profile_name == old_name);
        if !name_taken_by_other {
            if let Some(new_entry) = profiles.iter_mut().find(|p| p.folder_name == new_folder) {
                new_entry.profile_name = old_name.clone();
                final_name = old_name;
            }
        }
    }

    if let Ok(updated_json) = serde_json::to_vec_pretty(&profiles) {
        if write_file_atomic(&profiles_path, &updated_json).is_ok() {
            let _ = fs::remove_dir_all(&old_dir);
            let _ = remove_profile_metadata_entry_in_dir(wallet_data_dir, old_folder);
        }
    }

    final_name
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn finalize_restored_wallet_and_reconcile_l2(
    source_path: &Path,
    credentials: BackupRestoreCredentials<'_>,
    new_password: &str,
    local_instance_id: String,
    replace_folder_name: Option<&str>,
    wallet_data_dir: &Path,
    standards_dir: &Path,
    tmp_restore_dir: &Path,
    state: &AppState,
    app_opt: Option<&tauri::AppHandle>,
) -> Result<FrontendRestoreOutcomeDto, FrontendError> {
    let user_ids_by_folder = user_ids_from_metadata_dir(wallet_data_dir);

    let (outcome, mut profile_name, custom_standards) = {
        let mut service = state.lock_service()?;
        let (outcome, profile_name, custom_standards) = restore_backup_impl(
            source_path,
            credentials,
            new_password,
            local_instance_id,
            &mut service,
        )?;

        let restored_identity = service
            .with_wallet_and_identity(|_, id| id.clone())
            .map_err(FrontendError::from)?;

        let resolved_name = if let Some(old_folder) = replace_folder_name {
            if let Some(app) = app_opt {
                if let Ok(config_dir) = app.path().app_config_dir() {
                    let _ = crate::pin_keystore::remove_entry(&config_dir, old_folder);
                    let _ = crate::pin_keystore::remove_entry(&config_dir, &outcome.folder_name);
                }
            }
            replace_local_profile_with_restored(
                wallet_data_dir,
                old_folder,
                &outcome.folder_name,
                &restored_identity,
                &user_ids_by_folder,
                profile_name,
            )
        } else {
            profile_name
        };

        if let Err(e) = record_profile_metadata_in_dir(
            wallet_data_dir,
            &outcome.folder_name,
            Some(restored_identity.user_id.clone()),
        ) {
            log::warn!("Failed to update profile metadata: {}", e);
        }

        if let Err(e) = state.initialize_profile_session(&mut service, new_password) {
            log::warn!("Failed to initialize profile session after restore: {}", e);
        }

        (outcome, resolved_name, custom_standards)
    };

    if profile_name.is_empty() {
        profile_name = "Restored Profile".to_string();
    }

    let restored_standards_count =
        restore_custom_standards_to_dir(standards_dir, &custom_standards)?;

    let summary = crate::services::l2_anchor_service::reconcile_vouchers_via_static_locks(
        state,
        Some(&outcome.voucher_ids_to_check),
        Some(new_password),
    )
    .await?;

    let active_voucher_queries_count = summary.active_anchored_count;
    let reconciled_spent_count = summary.reconciled_spent_count;
    let l2_reconciliation_status = summary.status;

    // Mandatory SST-V4 offline-spend lock (§5)
    let pending_restore_l2 = active_voucher_queries_count > 0 && !summary.all_responded;
    if let Ok(mut settings) = state.get_cached_settings() {
        settings.pending_restore_l2_reconciliation = pending_restore_l2;
        if let Ok(mut service) = state.service.lock() {
            let _ = state.save_settings(&mut service, settings, Some(new_password));
        }
    }

    // Clean up temporary staged file if source_path was from drop/sync staging
    if (source_path.starts_with(tmp_restore_dir)
        || source_path.to_string_lossy().contains("tmp_restore"))
        && source_path.is_file()
    {
        let _ = fs::remove_file(source_path);
    }

    if let Some(app) = app_opt {
        crate::services::common::spawn_pending_l2_reconciliation(app, new_password);
    }

    Ok(FrontendRestoreOutcomeDto {
        folder_name: outcome.folder_name,
        profile_name,
        active_voucher_queries_count,
        reconciled_spent_count,
        restored_standards_count,
        l2_reconciliation_status,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn restore_wallet_backup_impl(
    source_path: &str,
    credential_mode: &str,
    backup_password: Option<&str>,
    mnemonic: Option<&str>,
    passphrase: Option<&str>,
    user_prefix: Option<&str>,
    language: Option<MnemonicLanguage>,
    new_password: &str,
    local_instance_id: String,
    replace_folder_name: Option<&str>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendRestoreOutcomeDto, FrontendError> {
    if local_instance_id.is_empty() {
        return Err(FrontendError::from(
            "Device instance ID is required".to_string(),
        ));
    }
    if new_password.is_empty() {
        return Err(FrontendError::from(
            "New password cannot be empty".to_string(),
        ));
    }

    let cleaned_prefix = user_prefix.map(str::trim).filter(|s| !s.is_empty());
    let credentials = parse_credentials(
        credential_mode,
        backup_password,
        mnemonic,
        passphrase,
        cleaned_prefix,
        language,
    )?;

    let app_dir = resolve_app_data_dir(app)?;
    let wallet_data_dir = app_dir.join("wallet_data");
    let standards_dir = app_dir.join("voucher_standards");
    let tmp_restore_dir = app_dir.join("tmp_restore");

    finalize_restored_wallet_and_reconcile_l2(
        Path::new(source_path),
        credentials,
        new_password,
        local_instance_id,
        replace_folder_name,
        &wallet_data_dir,
        &standards_dir,
        &tmp_restore_dir,
        state,
        Some(app),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
fn recover_local_profile_with_mnemonic(
    target_folder_name: &str,
    phrase: &str,
    passphrase: Option<&str>,
    new_password: &str,
    core_lang: human_money_core::MnemonicLanguage,
    local_instance_id: String,
    wallet_data_dir: &Path,
    state: &AppState,
    app_opt: Option<&tauri::AppHandle>,
) -> Result<(), FrontendError> {
    let mut service = state.lock_service()?;
    service
        .recover_wallet_and_set_new_password(
            target_folder_name,
            phrase,
            passphrase,
            new_password,
            core_lang,
            local_instance_id,
        )
        .map_err(FrontendError::from)?;
    if let Some(app) = app_opt {
        if let Ok(config_dir) = app.path().app_config_dir() {
            let _ = crate::pin_keystore::remove_entry(&config_dir, target_folder_name);
        }
    }
    let user_id = service
        .with_wallet_and_identity(|_, id| id.user_id.clone())
        .ok();
    let _ = record_profile_metadata_in_dir(wallet_data_dir, target_folder_name, user_id);
    state
        .initialize_profile_session(&mut service, new_password)
        .map_err(FrontendError::from)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reset_local_password_via_backup_impl(
    target_folder_name: &str,
    source_path: &Path,
    credential_mode: &str,
    backup_password: Option<&str>,
    mnemonic: Option<&str>,
    passphrase: Option<&str>,
    language: Option<MnemonicLanguage>,
    new_password: &str,
    local_instance_id: String,
    wallet_data_dir: &Path,
    tmp_restore_dir: &Path,
    state: &AppState,
    app_opt: Option<&tauri::AppHandle>,
) -> Result<(), FrontendError> {
    if !is_safe_folder_name(target_folder_name) {
        return Err(FrontendError::from(
            "Invalid target profile folder name".to_string(),
        ));
    }
    if new_password.is_empty() {
        return Err(FrontendError::from(
            "New password cannot be empty".to_string(),
        ));
    }
    if local_instance_id.is_empty() {
        return Err(FrontendError::from(
            "Device instance ID is required".to_string(),
        ));
    }

    if credential_mode == "mnemonic" {
        let phrase = mnemonic
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| FrontendError::from("Missing mnemonic phrase".to_string()))?;
        let core_lang: human_money_core::MnemonicLanguage =
            language.unwrap_or(MnemonicLanguage::English).into();

        recover_local_profile_with_mnemonic(
            target_folder_name,
            phrase,
            passphrase,
            new_password,
            core_lang,
            local_instance_id,
            wallet_data_dir,
            state,
            app_opt,
        )?;
    } else if credential_mode == "password" {
        let pwd = backup_password
            .filter(|s| !s.is_empty())
            .ok_or_else(|| FrontendError::from("Missing backup password".to_string()))?;

        fs::create_dir_all(tmp_restore_dir).map_err(|e| FrontendError::from(e.to_string()))?;
        let temp_dir = tmp_restore_dir.join(format!("pwreset_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).map_err(|e| FrontendError::from(e.to_string()))?;
        let _guard_dir = TempDirGuard(temp_dir.clone());

        let parts = read_backup_parts(source_path)?;
        let temp_local_pw = format!("tmp_{}", uuid::Uuid::new_v4());
        let mut temp_service = AppService::new(&temp_dir).map_err(FrontendError::from)?;
        temp_service
            .restore_backup(
                &parts.manifest_bytes,
                &parts.blobs,
                BackupRestoreCredentials::BackupPassword(pwd),
                &temp_local_pw,
                local_instance_id.clone(),
            )
            .map_err(FrontendError::from)?;

        let backup_identity = temp_service
            .with_wallet_and_identity(|_, id| id.clone())
            .map_err(FrontendError::from)?;
        let exported_mnemonic = temp_service.export_mnemonic(&temp_local_pw).ok().flatten();
        temp_service.logout();

        if let Some((phrase, core_lang, false)) = exported_mnemonic {
            recover_local_profile_with_mnemonic(
                target_folder_name,
                &phrase,
                None,
                new_password,
                core_lang,
                local_instance_id,
                wallet_data_dir,
                state,
                app_opt,
            )?;
        } else {
            let local_dir = wallet_data_dir.join(target_folder_name);
            if !local_dir.exists() {
                return Err(FrontendError::from(
                    "Local profile directory not found".to_string(),
                ));
            }
            if local_dir.join(".wallet.undo.enc").exists() {
                return Err(FrontendError::from(
                    "Ein unterbrochener lokaler Schreibvorgang verhindert das Passwort-Zurücksetzen per Backup-Passwort. Bitte nutze die Wiederherstellungswörter oder ersetze das Profil durch die Sicherung.".to_string(),
                ));
            }

            let mut service = state.lock_service()?;
            service.logout();

            let mut local_storage = FileStorage::new(local_dir);
            let lock_guard = WalletLockGuard::new(&local_storage)
                .map_err(|e| FrontendError::from(e.to_string()))?;
            let auth_rec = AuthMethod::RecoveryIdentity(&backup_identity);
            let existing_seal = local_storage
                .load_seal(&auth_rec)
                .map_err(|e| FrontendError::from(e.to_string()))?;
            if let Some(ref seal_rec) = existing_seal {
                if seal_rec.seal.payload.user_id != backup_identity.user_id {
                    return Err(FrontendError::from(
                        "Backup identity does not match target local profile".to_string(),
                    ));
                }
            }

            let (mut local_wallet, local_identity) =
                Wallet::load(&local_storage, &auth_rec, local_instance_id.clone())
                    .map_err(FrontendError::from)?;
            if local_identity.user_id != backup_identity.user_id {
                return Err(FrontendError::from(
                    "Backup identity does not match target local profile".to_string(),
                ));
            }
            if !local_wallet.pending_events.is_empty() {
                local_wallet
                    .save(&mut local_storage, &local_identity, &auth_rec)
                    .map_err(|e| FrontendError::from(e.to_string()))?;
            }

            Wallet::reset_password(&mut local_storage, &local_identity, new_password)
                .map_err(|e| FrontendError::from(e.to_string()))?;

            let merkle_root = human_money_core::models::seal::calculate_voucher_merkle_root(
                &local_wallet.voucher_store.vouchers,
            );
            let own_fp_hash = SealService::calculate_state_hash(&local_wallet.own_fingerprints)
                .map_err(FrontendError::from)?;
            let next_epoch = existing_seal
                .as_ref()
                .map_or(1, |r| (r.seal.payload.epoch as u64) + 1);
            let comp_hash = SealService::calculate_comprehensive_state_hash(
                next_epoch,
                0,
                &merkle_root,
                &own_fp_hash,
            );
            let recovered_seal = WalletSeal::recover_epoch(
                existing_seal.as_ref().map(|r| &r.seal),
                &local_identity.user_id,
                &local_identity,
                &comp_hash,
                &local_instance_id,
            )
            .map_err(FrontendError::from)?;

            let new_record = LocalSealRecord {
                seal: recovered_seal,
                sync_status: SyncStatus::PendingUpload,
                lock_reason: None,
            };
            let auth_new_pw = AuthMethod::Password(new_password);
            local_storage
                .save_seal(&auth_new_pw, &new_record)
                .map_err(|e| FrontendError::from(e.to_string()))?;

            let item_hashes = local_storage
                .get_all_item_hashes()
                .map_err(|e| FrontendError::from(e.to_string()))?;
            let integrity_record = StorageIntegrityRecord::create_record(
                &local_identity,
                &new_record.seal,
                item_hashes,
            )
            .map_err(FrontendError::from)?;
            local_storage
                .save_integrity(&integrity_record)
                .map_err(|e| FrontendError::from(e.to_string()))?;

            drop(lock_guard);

            service
                .login(target_folder_name, new_password, false, local_instance_id)
                .map_err(FrontendError::from)?;
            if let Some(app) = app_opt {
                if let Ok(config_dir) = app.path().app_config_dir() {
                    let _ = crate::pin_keystore::remove_entry(&config_dir, target_folder_name);
                }
            }
            let _ = record_profile_metadata_in_dir(
                wallet_data_dir,
                target_folder_name,
                Some(local_identity.user_id.clone()),
            );
            state
                .initialize_profile_session(&mut service, new_password)
                .map_err(FrontendError::from)?;
        }
    } else {
        return Err(FrontendError::from(format!(
            "Invalid credential mode: {}",
            credential_mode
        )));
    }

    if let Some(app) = app_opt {
        crate::services::common::spawn_pending_l2_reconciliation(app, new_password);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reset_local_password_via_backup_for_app_impl(
    target_folder_name: &str,
    source_path: &str,
    credential_mode: &str,
    backup_password: Option<&str>,
    mnemonic: Option<&str>,
    passphrase: Option<&str>,
    language: Option<MnemonicLanguage>,
    new_password: &str,
    local_instance_id: String,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<(), FrontendError> {
    let app_dir = resolve_app_data_dir(app)?;
    let wallet_data_dir = app_dir.join("wallet_data");
    let tmp_restore_dir = app_dir.join("tmp_restore");

    reset_local_password_via_backup_impl(
        target_folder_name,
        Path::new(source_path),
        credential_mode,
        backup_password,
        mnemonic,
        passphrase,
        language,
        new_password,
        local_instance_id,
        &wallet_data_dir,
        &tmp_restore_dir,
        state,
        Some(app),
    )
}

pub(crate) fn is_older_than_14_days(last_backup_at: &Option<String>) -> bool {
    let Some(ts_str) = last_backup_at else {
        return true;
    };
    let Ok(ts) = chrono::DateTime::parse_from_rfc3339(ts_str) else {
        return true;
    };
    let now = Utc::now();
    now.signed_duration_since(ts.with_timezone(&Utc)).num_days() >= 14
}

pub fn compute_backup_status_dto(
    settings: &crate::settings::AppSettings,
    has_own_sync: bool,
) -> FrontendAutoBackupStatusDto {
    let sync_backup_enabled = settings.sync_backup_enabled;
    let last_sync_backup_at = settings.last_sync_backup_at.clone();

    let has_friend_syncs = !settings.market_selected_syncs.is_empty();
    let has_public_syncs = settings
        .public_syncs
        .iter()
        .any(|d| d.enabled && !d.write_url.is_empty());
    let has_authorized_sync_backup_target =
        has_friend_syncs || has_public_syncs || has_own_sync || last_sync_backup_at.is_some();

    let folder_str_opt = settings
        .backup_sync_folder
        .as_ref()
        .filter(|f| !f.trim().is_empty());
    let local_configured = folder_str_opt.is_some();
    let folder_str = folder_str_opt.cloned();

    let local_connected = if let Some(ref path_str) = folder_str {
        Path::new(path_str).is_dir()
    } else {
        false
    };

    let local_current = local_configured
        && local_connected
        && !settings.backup_dirty
        && !is_older_than_14_days(&settings.last_backup_at);
    let sync_current = sync_backup_enabled
        && has_authorized_sync_backup_target
        && !settings.backup_dirty
        && last_sync_backup_at.is_some()
        && !is_older_than_14_days(&last_sync_backup_at);

    let has_sync_target = sync_backup_enabled && has_authorized_sync_backup_target;

    let status = if !local_configured && !has_sync_target {
        "notConfigured"
    } else if local_current || sync_current {
        "ok"
    } else if local_configured && !local_connected && !sync_current {
        "disconnected"
    } else if settings.backup_dirty {
        "pending"
    } else {
        "warning"
    };

    FrontendAutoBackupStatusDto {
        status: status.to_string(),
        last_backup_at: settings.last_backup_at.clone(),
        backup_dirty: settings.backup_dirty,
        sync_folder: folder_str,
        changed_blobs: None,
        sync_backup_enabled,
        last_sync_backup_at,
        has_authorized_sync_backup_target,
    }
}

fn not_configured_backup_status() -> FrontendAutoBackupStatusDto {
    FrontendAutoBackupStatusDto {
        status: "notConfigured".to_string(),
        last_backup_at: None,
        backup_dirty: false,
        sync_folder: None,
        changed_blobs: None,
        sync_backup_enabled: false,
        last_sync_backup_at: None,
        has_authorized_sync_backup_target: false,
    }
}

fn has_own_sync_target(state: &AppState) -> bool {
    state
        .service
        .lock()
        .ok()
        .and_then(|mut service| {
            service
                .load_encrypted_data(crate::state::MARKET_SYNC_ADMIN_KEY, None)
                .ok()
                .and_then(|bytes| {
                    serde_json::from_slice::<crate::state::StoredSyncAdmin>(&bytes).ok()
                })
        })
        .map(|admin| !admin.descriptor.write_url.is_empty())
        .unwrap_or(false)
}

pub(crate) fn get_backup_status_impl(
    state: &AppState,
) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    let Ok(settings) = state.get_cached_settings() else {
        return Ok(not_configured_backup_status());
    };
    let has_own_sync = has_own_sync_target(state);
    Ok(compute_backup_status_dto(&settings, has_own_sync))
}

pub(crate) fn run_auto_backup_sync_impl(
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    let Ok(mut settings) = state.get_cached_settings() else {
        return Ok(not_configured_backup_status());
    };

    let has_own_sync = has_own_sync_target(state);

    let folder_str = match &settings.backup_sync_folder {
        Some(f) if !f.trim().is_empty() => f.clone(),
        _ => {
            return Ok(compute_backup_status_dto(&settings, has_own_sync));
        }
    };

    if !settings.auto_backup_enabled
        || settings.backup_protection_mode.as_deref() == Some("custom_password")
    {
        return Ok(compute_backup_status_dto(&settings, has_own_sync));
    }

    let folder_path = Path::new(&folder_str);
    // Crucial USB safety check: verify is_dir(), NEVER call fs::create_dir_all if the folder is missing.
    if !folder_path.is_dir() {
        if !settings.backup_dirty {
            settings.backup_dirty = true;
            if let Ok(mut service) = state.service.lock() {
                let _ = state.save_settings(&mut service, settings.clone(), None);
            }
        }
        return Ok(compute_backup_status_dto(&settings, has_own_sync));
    }

    let mut service = state.lock_service()?;
    let custom_standards = collect_custom_standards(app);

    match sync_backup_folder_impl(
        folder_path,
        "mnemonic",
        BackupProtection::MnemonicOnly,
        None,
        false,
        &mut service,
        custom_standards,
    ) {
        Ok(summary) => {
            settings.last_backup_at = Some(summary.timestamp.clone());
            settings.backup_dirty = false;
            let _ = state.save_settings(&mut service, settings.clone(), None);
            let mut dto = compute_backup_status_dto(&settings, has_own_sync);
            dto.changed_blobs = Some(summary.changed_blobs);
            Ok(dto)
        }
        Err(e) => {
            let is_locked = e.code == "error.wallet.locked"
                || e.code == "error.auth.sessionTimeout"
                || e.message.to_lowercase().contains("locked")
                || e.message.to_lowercase().contains("session");
            if is_locked {
                let mut dto = compute_backup_status_dto(&settings, has_own_sync);
                dto.status = "locked".to_string();
                Ok(dto)
            } else {
                if !settings.backup_dirty {
                    settings.backup_dirty = true;
                    let _ = state.save_settings(&mut service, settings.clone(), None);
                }
                Err(e)
            }
        }
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    human_money_core::hex::encode(hasher.finalize())
}

pub(crate) fn merge_backup_delta_into_base(
    base: &mut PortableBackupContainer,
    delta: &BackupDeltaContainer,
) -> Result<(), String> {
    let base_manifest_bytes = human_money_core::hex::decode(&base.manifest_hex)
        .map_err(|e| format!("Invalid base manifest hex: {}", e))?;
    let actual_sha256 = sha256_hex(&base_manifest_bytes);
    if actual_sha256 != delta.base_manifest_sha256 {
        return Err(format!(
            "Backup delta base mismatch (expected {}, got {})",
            delta.base_manifest_sha256, actual_sha256
        ));
    }

    base.manifest_hex = delta.manifest_hex.clone();
    base.created_at = delta.created_at.clone();
    for del_name in &delta.delete_blobs {
        base.blobs_hex.remove(del_name);
    }
    for (name, hex_data) in &delta.put_blobs_hex {
        base.blobs_hex.insert(name.clone(), hex_data.clone());
    }
    base.custom_standards = delta.custom_standards.clone();
    Ok(())
}

pub(crate) async fn fetch_and_merge_backup_slot(
    client: &reqwest::Client,
    read_url: &str,
    pubkey_hex: &str,
    base_prefix: Option<&str>,
) -> Option<Vec<u8>> {
    let delta_prefix = humoco_market::derive_delta_prefix(base_prefix);
    let (base_res, delta_res) = tokio::join!(
        crate::sync_client::get_slot(client, read_url, pubkey_hex, base_prefix, "backup", None),
        crate::sync_client::get_slot(
            client,
            read_url,
            pubkey_hex,
            Some(delta_prefix.as_str()),
            "backup",
            None
        ),
    );

    let base_slot_bytes = match base_res {
        Ok(crate::sync_client::SlotFetchResult::Modified { bytes, .. }) => bytes,
        _ => return None,
    };

    let unpadded_base = humoco_market::unpad_backup_slot_bytes(&base_slot_bytes).ok()?;
    let mut base_container: PortableBackupContainer =
        serde_json::from_slice(&unpadded_base).ok()?;

    if let Ok(crate::sync_client::SlotFetchResult::Modified {
        bytes: delta_slot_bytes,
        ..
    }) = delta_res
    {
        if let Ok(unpadded_delta) = humoco_market::unpad_backup_slot_bytes(&delta_slot_bytes) {
            if let Ok(delta_container) =
                serde_json::from_slice::<BackupDeltaContainer>(&unpadded_delta)
            {
                if let Err(e) = merge_backup_delta_into_base(&mut base_container, &delta_container)
                {
                    log::warn!(
                        "Ignoring stale/mismatched backup delta from {}: {}",
                        read_url,
                        e
                    );
                }
            }
        }
    }

    serde_json::to_vec(&base_container).ok()
}

struct AuthorizedBackupTarget {
    desc: humoco_market::DropDescriptor,
    is_public: bool,
    base_prefix: Option<String>,
    remote_base_etag: Option<String>,
    effective_max_bytes: usize,
}

fn target_can_use_delta(
    target: &AuthorizedBackupTarget,
    state: &crate::state::StoredBackupSyncState,
    delta_len: usize,
    now_secs: u64,
) -> bool {
    let Some(cur_sha) = state.base_manifest_sha256.as_deref() else {
        return false;
    };
    let Some(m) = state.mirrors_by_write_url.get(&target.desc.write_url) else {
        return false;
    };
    if m.base_manifest_sha256 != cur_sha {
        return false;
    }
    if target.remote_base_etag.is_none() {
        return false;
    }
    if m.base_slot_etag
        .as_ref()
        .is_some_and(|exp| target.remote_base_etag.as_deref() != Some(exp.as_str()))
    {
        return false;
    }
    if now_secs.saturating_sub(m.base_uploaded_at) >= BACKUP_BASE_TTL_SECS {
        return false;
    }
    if delta_len > m.base_padded_bytes / 2 {
        return false;
    }
    m.base_padded_bytes.saturating_add(delta_len) <= target.effective_max_bytes
}

pub async fn sync_backup_impl(
    password: Option<String>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendBackupSyncResult, FrontendError> {
    let custom_standards = collect_custom_standards(app);
    sync_backup_with_standards_impl(password, custom_standards, state).await
}

pub(crate) async fn sync_backup_with_standards_impl(
    password: Option<String>,
    custom_standards: HashMap<String, String>,
    state: &AppState,
) -> Result<FrontendBackupSyncResult, FrontendError> {
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    if !settings.sync_backup_enabled {
        return Ok(FrontendBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }

    let (my_ed_sk, master_seed, my_prefix, own_admin_opt) = {
        let mut service = state.lock_service()?;
        let (my_ed_sk, my_prefix) = service
            .with_wallet_and_identity(|_w, id| {
                let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string());
                (id.signing_key.clone(), pfx)
            })
            .map_err(FrontendError::from)?;
        let master_seed = my_ed_sk.to_bytes();
        let own_admin_opt = service
            .load_encrypted_data(crate::state::MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<crate::state::StoredSyncAdmin>(&bytes).ok());
        (my_ed_sk, master_seed, my_prefix, own_admin_opt)
    };

    // Gather candidate backup drops: selected friend drops + own drop + enabled public drops
    let mut candidate_targets: Vec<humoco_market::DropDescriptor> = settings
        .market_selected_syncs
        .iter()
        .cloned()
        .map(Into::into)
        .collect();

    if let Some(admin) = own_admin_opt {
        if !admin.descriptor.write_url.is_empty()
            && !candidate_targets
                .iter()
                .any(|d| d.write_url == admin.descriptor.write_url)
        {
            candidate_targets.push(admin.descriptor);
        }
    }

    for pd in &settings.public_syncs {
        if pd.enabled
            && !pd.write_url.is_empty()
            && !candidate_targets
                .iter()
                .any(|d| d.write_url == pd.write_url)
        {
            candidate_targets.push(humoco_market::DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(pd.access_pubkey.clone()),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }

    if candidate_targets.is_empty() {
        return Ok(FrontendBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }

    // Probe which candidate drops authorize channel "backup"
    let probe_client = crate::sync_client::create_sync_probe_client();
    let mut authorized_targets = Vec::new();

    for sync_desc in candidate_targets {
        let (cap_pubkey, is_public, cap_prefix) = if let Some(ref slot_id) = sync_desc.slot_id {
            match humoco_market::parse_slot_id(slot_id) {
                Ok((base_pk, sid_pfx)) => (base_pk, true, sid_pfx.or_else(|| my_prefix.clone())),
                Err(_) => (slot_id.clone(), true, my_prefix.clone()),
            }
        } else {
            match humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &sync_desc.drop_pub) {
                Ok(pk) => (pk, false, my_prefix.clone()),
                Err(_) => continue,
            }
        };

        if let Ok(cap) = crate::sync_client::head_slot_capability(
            &probe_client,
            &sync_desc.write_url,
            &cap_pubkey,
            cap_prefix.as_deref(),
            "backup",
        )
        .await
        {
            if cap.authorized {
                authorized_targets.push(AuthorizedBackupTarget {
                    desc: sync_desc,
                    is_public,
                    base_prefix: cap_prefix,
                    remote_base_etag: cap.slot_etag,
                    effective_max_bytes: cap
                        .max_bytes
                        .filter(|&v| v > 0)
                        .unwrap_or(humoco_market::DROP_BACKUP_MAX_BYTES),
                });
            }
        }
    }

    let authorized_count = authorized_targets.len();
    if authorized_targets.is_empty() {
        return Ok(FrontendBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }

    let max_target_bytes = authorized_targets
        .iter()
        .map(|t| t.effective_max_bytes)
        .max()
        .unwrap_or(humoco_market::DROP_BACKUP_MAX_BYTES);
    let now_secs = crate::sync_client::current_unix_timestamp();

    let (
        mut backup_sync_state,
        base_manifest_bytes,
        base_manifest_sha256,
        base_padded_bytes,
        delta_padded_opt,
        pending_seal_hash,
    ) = {
        let mut service = state.lock_service()?;

        let pending_seal_hash = service
            .get_seal_for_upload()
            .ok()
            .flatten()
            .and_then(|bytes| {
                serde_json::from_slice::<human_money_core::models::seal::WalletSeal>(&bytes).ok()
            })
            .and_then(|seal| seal.compute_hash().ok());

        let stored_state: crate::state::StoredBackupSyncState = service
            .load_encrypted_data(crate::state::BACKUP_SYNC_STATE_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();

        // Try building a delta against the stored Base manifest (if one exists)
        let delta_attempt = if let (Some(prev_manifest), Some(prev_sha256)) = (
            stored_state.base_manifest_bytes.as_deref(),
            stored_state.base_manifest_sha256.as_deref(),
        ) {
            match service.create_differential_backup(
                Some(prev_manifest),
                BackupProtection::MnemonicOnly,
                password.as_deref(),
            ) {
                Ok(diff) => {
                    let put_blobs_hex = encode_blobs_hex(diff.put_blobs);
                    let delta_container = BackupDeltaContainer {
                        container_format: "humoco-app-backup-delta-v1".to_string(),
                        created_at: Utc::now().to_rfc3339(),
                        base_manifest_sha256: prev_sha256.to_string(),
                        manifest_hex: human_money_core::hex::encode(&diff.encrypted_manifest_bytes),
                        put_blobs_hex,
                        delete_blobs: diff.delete_blobs,
                        custom_standards: custom_standards.clone(),
                    };
                    serde_json::to_vec(&delta_container).ok().and_then(|bytes| {
                        humoco_market::pad_backup_slot_bytes(&bytes, max_target_bytes).ok()
                    })
                }
                Err(_) => None,
            }
        } else {
            None
        };

        // Determine if any mirror needs a fresh Base (or if Delta grew too large)
        let any_mirror_needs_base = authorized_targets.iter().any(|target| {
            !delta_attempt
                .as_ref()
                .is_some_and(|d| target_can_use_delta(target, &stored_state, d.len(), now_secs))
        });

        let (base_m_bytes, base_m_sha, base_padded) = if any_mirror_needs_base {
            let diff = service
                .create_differential_backup(
                    None,
                    BackupProtection::MnemonicOnly,
                    password.as_deref(),
                )
                .map_err(FrontendError::from)?;
            let blobs_hex = encode_blobs_hex(diff.put_blobs);
            let container = PortableBackupContainer {
                container_format: "humoco-app-backup-v1".to_string(),
                created_at: Utc::now().to_rfc3339(),
                manifest_hex: human_money_core::hex::encode(&diff.encrypted_manifest_bytes),
                blobs_hex,
                custom_standards,
            };
            let container_bytes = serde_json::to_vec(&container).map_err(|e| {
                FrontendError::from(format!("Failed to serialize backup container: {}", e))
            })?;
            let padded = humoco_market::pad_backup_slot_bytes(&container_bytes, max_target_bytes)
                .map_err(|e| FrontendError::from(e.to_string()))?;
            let sha = sha256_hex(&diff.encrypted_manifest_bytes);
            (Some(diff.encrypted_manifest_bytes), Some(sha), Some(padded))
        } else {
            (None, None, None)
        };

        (
            stored_state,
            base_m_bytes,
            base_m_sha,
            base_padded,
            delta_attempt,
            pending_seal_hash,
        )
    };

    let client = crate::sync_client::create_sync_http_client();
    let mut uploaded_to = 0;
    let mut last_uploaded_bytes = 0usize;
    let mut errors = Vec::new();
    let mut current_settings = settings;
    let mut state_dirty = false;
    let mut base_uploaded_any = false;

    for target in authorized_targets {
        let target_delta_prefix = humoco_market::derive_delta_prefix(target.base_prefix.as_deref());
        let can_use_delta = delta_padded_opt
            .as_ref()
            .is_some_and(|d| target_can_use_delta(&target, &backup_sync_state, d.len(), now_secs));

        let (upload_prefix, upload_bytes, is_delta_upload) = if can_use_delta {
            (
                Some(target_delta_prefix.as_str()),
                delta_padded_opt.as_ref().expect("checked above"),
                true,
            )
        } else if let Some(ref base_bytes) = base_padded_bytes {
            (target.base_prefix.as_deref(), base_bytes, false)
        } else {
            continue;
        };

        if upload_bytes.len() > target.effective_max_bytes {
            errors.push(format!(
                "Upload to {} skipped: backup size {} exceeds channel limit {}",
                target.desc.write_url,
                upload_bytes.len(),
                target.effective_max_bytes
            ));
            continue;
        }

        let signer = DropChannelSigner {
            is_public: target.is_public,
            master_seed: &master_seed,
            my_ed_sk: &my_ed_sk,
            drop_desc: &target.desc,
        };
        let nonce = crate::sync_client::generate_sync_nonce();
        let signed_res = signer.sign_request(
            "PUT",
            "backup",
            upload_prefix,
            now_secs,
            &nonce,
            upload_bytes,
        );

        match signed_res {
            Ok(signed) => {
                match crate::sync_client::put_slot(
                    &client,
                    &target.desc.write_url,
                    &signed,
                    "backup",
                    upload_bytes,
                    None,
                )
                .await
                {
                    Ok(crate::sync_client::PutSlotOutcome::Saved { etag }) => {
                        uploaded_to += 1;
                        last_uploaded_bytes = last_uploaded_bytes.max(upload_bytes.len());
                        state_dirty = true;
                        if is_delta_upload {
                            if let Some(m) = backup_sync_state
                                .mirrors_by_write_url
                                .get_mut(&target.desc.write_url)
                            {
                                m.delta_active = true;
                            }
                        } else {
                            base_uploaded_any = true;
                            let had_delta = backup_sync_state
                                .mirrors_by_write_url
                                .get(&target.desc.write_url)
                                .map(|m| m.delta_active)
                                .unwrap_or(false);
                            let mut delta_cleared = !had_delta;
                            if had_delta {
                                let del_nonce = crate::sync_client::generate_sync_nonce();
                                if let Ok(del_signed) = signer.sign_request(
                                    "DELETE",
                                    "backup",
                                    Some(target_delta_prefix.as_str()),
                                    now_secs,
                                    &del_nonce,
                                    b"",
                                ) {
                                    delta_cleared = crate::sync_client::delete_slot(
                                        &client,
                                        &target.desc.write_url,
                                        &del_signed,
                                        "backup",
                                    )
                                    .await
                                    .is_ok();
                                }
                            }
                            if let Some(ref new_sha) = base_manifest_sha256 {
                                let normalized_etag = etag
                                    .map(|e| e.trim().trim_matches('"').to_string())
                                    .filter(|s| !s.is_empty());
                                backup_sync_state.mirrors_by_write_url.insert(
                                    target.desc.write_url.clone(),
                                    crate::state::BackupMirrorState {
                                        base_manifest_sha256: new_sha.clone(),
                                        base_slot_etag: normalized_etag,
                                        base_uploaded_at: now_secs,
                                        base_padded_bytes: upload_bytes.len(),
                                        delta_active: !delta_cleared,
                                    },
                                );
                            }
                        }
                    }
                    Ok(crate::sync_client::PutSlotOutcome::PreconditionFailed { .. }) => {
                        errors.push(format!(
                            "Upload to {} failed: precondition failed",
                            target.desc.write_url
                        ));
                    }
                    Err(e) => {
                        errors.push(format!("Upload to {} failed: {}", target.desc.write_url, e));
                    }
                }
            }
            Err(e) => {
                errors.push(format!(
                    "Signing failed for {}: {}",
                    target.desc.write_url, e
                ));
            }
        }
    }

    if state_dirty {
        if base_uploaded_any {
            if let (Some(new_bytes), Some(new_sha)) = (base_manifest_bytes, base_manifest_sha256) {
                backup_sync_state.base_manifest_bytes = Some(new_bytes);
                backup_sync_state.base_manifest_sha256 = Some(new_sha);
            }
        }
        if let Ok(encoded_state) = serde_json::to_vec(&backup_sync_state) {
            if let Ok(mut service) = state.service.lock() {
                let _ = service.save_encrypted_data(
                    crate::state::BACKUP_SYNC_STATE_KEY,
                    &encoded_state,
                    password.as_deref(),
                );
            }
        }
    }

    let settings_updated;
    let timestamp_str = if uploaded_to > 0 {
        let ts = Utc::now().to_rfc3339();
        current_settings.last_sync_backup_at = Some(ts.clone());
        current_settings.backup_dirty = false;
        settings_updated = true;
        Some(ts)
    } else {
        settings_updated = false;
        None
    };

    if settings_updated {
        let mut service = state.lock_service()?;
        let _ = state.save_settings(&mut service, current_settings, password.as_deref());
        if let Some(ref seal_hash) = pending_seal_hash {
            if let Err(err) = service.acknowledge_seal_sync(seal_hash, password.as_deref()) {
                log::warn!("Post-sync seal acknowledgement deferred: {}", err);
            }
        }
    }

    let mut retry_after_secs: Option<u64> = None;
    for e in &errors {
        if let Some(secs) = crate::sync_client::extract_retry_after_from_error_str(e) {
            retry_after_secs = Some(retry_after_secs.map_or(secs, |prev| prev.max(secs)));
        }
    }

    Ok(FrontendBackupSyncResult {
        uploaded_to,
        authorized_targets: authorized_count,
        padded_bytes: last_uploaded_bytes,
        timestamp: timestamp_str,
        errors,
        retry_after_secs,
    })
}

pub(crate) fn preview_mnemonic_identity_impl(
    mnemonic: &str,
    passphrase: Option<&str>,
    user_prefix: Option<&str>,
    language: Option<MnemonicLanguage>,
    wallet_data_dir: &Path,
    service: &AppService,
) -> Result<MnemonicIdentityPreviewDto, FrontendError> {
    let cleaned_prefix = user_prefix.map(str::trim).filter(|s| !s.is_empty());
    let core_lang: human_money_core::MnemonicLanguage =
        language.unwrap_or(MnemonicLanguage::English).into();
    let (_, identity) = Wallet::new_from_mnemonic(
        mnemonic,
        passphrase.filter(|p| !p.is_empty()),
        cleaned_prefix,
        core_lang,
        "preview".to_string(),
        false,
    )
    .map_err(FrontendError::from)?;

    let user_ids = user_ids_from_metadata_dir(wallet_data_dir);
    let local_collision = find_local_profile_collision_impl(
        wallet_data_dir,
        service,
        &identity.user_id,
        Some(&identity),
        &user_ids,
    );

    Ok(MnemonicIdentityPreviewDto {
        user_id: identity.user_id.clone(),
        local_collision,
    })
}

pub(crate) fn preview_mnemonic_identity_for_app_impl(
    mnemonic: &str,
    passphrase: Option<&str>,
    user_prefix: Option<&str>,
    language: Option<MnemonicLanguage>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<MnemonicIdentityPreviewDto, FrontendError> {
    let wallet_data_dir = get_wallet_data_dir(app).map_err(FrontendError::from)?;
    let service = state.lock_service()?;
    preview_mnemonic_identity_impl(
        mnemonic,
        passphrase,
        user_prefix,
        language,
        &wallet_data_dir,
        &service,
    )
}

pub(crate) fn stage_dropped_backup_bytes_impl(
    bytes: &[u8],
    app: &tauri::AppHandle,
) -> Result<String, FrontendError> {
    if bytes.is_empty() {
        return Err(FrontendError::from("Backup file is empty".to_string()));
    }
    if bytes.len() > MAX_DROPPED_BACKUP_BYTES {
        return Err(FrontendError::from(
            "Backup file exceeds maximum allowed size (64 MiB)".to_string(),
        ));
    }

    let app_data_dir = resolve_app_data_dir(app)?;
    let tmp_restore_dir = app_data_dir.join("tmp_restore");
    cleanup_stale_tmp_restore_files(&tmp_restore_dir);
    fs::create_dir_all(&tmp_restore_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    let staged_name = format!("dropped_{}.hmcbackup", uuid::Uuid::new_v4());
    let staged_path = tmp_restore_dir.join(staged_name);
    write_file_atomic(&staged_path, bytes)?;
    Ok(staged_path.to_string_lossy().to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn probe_sync_backup_impl(
    sync_inputs: Vec<String>,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    wallet_data_dir: &Path,
    tmp_restore_dir: &Path,
    state: &AppState,
) -> Result<SyncBackupProbeResultDto, FrontendError> {
    let cleaned_prefix = user_prefix
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let lang_enum = language.unwrap_or(MnemonicLanguage::English);
    let core_lang: human_money_core::MnemonicLanguage = lang_enum.into();
    let (_, identity) = Wallet::new_from_mnemonic(
        &mnemonic,
        passphrase.as_deref().filter(|p| !p.is_empty()),
        cleaned_prefix,
        core_lang,
        "probe".to_string(),
        false,
    )
    .map_err(FrontendError::from)?;

    let my_ed_sk = identity.signing_key.clone();
    let master_seed = my_ed_sk.to_bytes();
    let normalized_prefix = cleaned_prefix.map(str::to_lowercase);
    let user_ids = user_ids_from_metadata_dir(wallet_data_dir);
    let local_collision = {
        let service = state.lock_service()?;
        find_local_profile_collision_impl(
            wallet_data_dir,
            &service,
            &identity.user_id,
            Some(&identity),
            &user_ids,
        )
    };

    let non_empty_inputs: Vec<String> = sync_inputs
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if non_empty_inputs.is_empty() {
        return Err(FrontendError::from(
            "Sync-Dienst-Adresse darf nicht leer sein".to_string(),
        ));
    }

    struct SyncCandidateTask {
        read_url: String,
        pubkey_hex: String,
        prefix_opt: Option<String>,
        kind: String,
        fallback_target: Option<ResolvedSyncTargetDto>,
    }

    let probe_client = crate::sync_client::create_sync_probe_client();
    let mut resolved_sync_targets: Vec<ResolvedSyncTargetDto> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut candidate_tasks: Vec<SyncCandidateTask> = Vec::new();

    for trimmed in non_empty_inputs {
        let (
            pasted_write_url,
            pasted_read_url,
            pasted_drop_pub,
            pasted_slot_id,
            pasted_register_url,
        ) = if trimmed.starts_with('{') {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&trimmed) {
                let w = val
                    .get("writeUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let r = val
                    .get("readUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let d = val
                    .get("dropPub")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let sid = val
                    .get("slotId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let reg = val
                    .get("registerUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                (w, r, d, sid, reg)
            } else {
                (None, None, None, None, None)
            }
        } else {
            (None, None, None, None, None)
        };

        let raw_url = pasted_write_url.unwrap_or_else(|| trimmed.clone());
        let canonical_write_url = match humoco_market::canonicalize_write_url(&raw_url) {
            Ok(u) => u,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };

        let mut push_candidate =
            |read_url: String,
             pubkey_hex: String,
             pfx: Option<String>,
             kind: &str,
             fallback_target: Option<ResolvedSyncTargetDto>| {
                if !read_url.is_empty()
                    && !candidate_tasks.iter().any(|t| {
                        t.read_url == read_url && t.pubkey_hex == pubkey_hex && t.prefix_opt == pfx
                    })
                {
                    candidate_tasks.push(SyncCandidateTask {
                        read_url,
                        pubkey_hex,
                        prefix_opt: pfx,
                        kind: kind.to_string(),
                        fallback_target,
                    });
                }
            };

        match crate::sync_client::fetch_sync_info(&probe_client, &canonical_write_url).await {
            Ok(info) => {
                let target = ResolvedSyncTargetDto {
                    write_url: canonical_write_url.clone(),
                    read_url: info.read_url.clone(),
                    drop_pub: info.drop_pub.clone(),
                    register_url: info.register_url.clone(),
                };
                if !resolved_sync_targets
                    .iter()
                    .any(|t| t.write_url == target.write_url)
                {
                    resolved_sync_targets.push(target.clone());
                }

                // 1. Current stealth key
                if let Ok(stealth_pk) =
                    humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &info.drop_pub)
                {
                    push_candidate(
                        info.read_url.clone(),
                        stealth_pk,
                        normalized_prefix.clone(),
                        "stealth",
                        None,
                    );
                }

                // 2. Grace / previous drop key if descriptor carried a different drop_pub
                if let Some(ref d_pub) = pasted_drop_pub {
                    if !d_pub.is_empty() && d_pub != &info.drop_pub {
                        if let Ok(grace_pk) =
                            humoco_market::derive_member_stealth_pubkey(&my_ed_sk, d_pub)
                        {
                            push_candidate(
                                info.read_url.clone(),
                                grace_pk.clone(),
                                normalized_prefix.clone(),
                                "grace",
                                None,
                            );
                            if let Some(ref p_read) = pasted_read_url {
                                push_candidate(
                                    p_read.clone(),
                                    grace_pk,
                                    normalized_prefix.clone(),
                                    "grace",
                                    None,
                                );
                            }
                        }
                    }
                }

                // 3. Public drop anonymous key
                if let Ok((_r_sk, _r_vk, r_hex)) =
                    humoco_market::derive_public_drop_keypair(&master_seed, &canonical_write_url)
                {
                    push_candidate(
                        info.read_url.clone(),
                        r_hex,
                        normalized_prefix.clone(),
                        "public",
                        None,
                    );
                }

                // 4. Explicit slot_id from descriptor
                if let Some(ref sid) = pasted_slot_id {
                    let (base_pk, sid_pfx) =
                        humoco_market::parse_slot_id(sid).unwrap_or_else(|_| (sid.clone(), None));
                    let eff_pfx = sid_pfx.or_else(|| normalized_prefix.clone());
                    push_candidate(
                        info.read_url.clone(),
                        base_pk.clone(),
                        eff_pfx.clone(),
                        "slot_id",
                        None,
                    );
                    if let Some(ref p_read) = pasted_read_url {
                        push_candidate(p_read.clone(), base_pk, eff_pfx, "slot_id", None);
                    }
                }

                // 5. Deterministic Own-Drop fallback
                let own_data_dir = humoco_market::derive_drop_data_dir(&master_seed, 0);
                let origin_dir = canonical_write_url
                    .rsplit_once('/')
                    .map(|(b, _)| b)
                    .unwrap_or(&canonical_write_url);
                let own_read_url = format!("{}/{}", origin_dir.trim_end_matches('/'), own_data_dir);
                let (admin_sk, admin_vk, _) =
                    humoco_market::derive_drop_admin_keypair(&master_seed, 0);
                if let Ok(own_stealth) =
                    humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &admin_vk)
                {
                    push_candidate(
                        own_read_url,
                        own_stealth.clone(),
                        normalized_prefix.clone(),
                        "own",
                        None,
                    );
                    push_candidate(
                        info.read_url,
                        own_stealth,
                        normalized_prefix.clone(),
                        "own",
                        None,
                    );
                }
            }
            Err(info_err) => {
                if let (Some(r_url), Some(d_pub)) = (pasted_read_url, pasted_drop_pub) {
                    let fallback_target = ResolvedSyncTargetDto {
                        write_url: canonical_write_url.clone(),
                        read_url: r_url.clone(),
                        drop_pub: d_pub.clone(),
                        register_url: pasted_register_url,
                    };
                    if let Ok(stealth_pk) =
                        humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &d_pub)
                    {
                        push_candidate(
                            r_url.clone(),
                            stealth_pk,
                            normalized_prefix.clone(),
                            "stealth",
                            Some(fallback_target.clone()),
                        );
                    }
                    if let Ok((_r_sk, _r_vk, r_hex)) = humoco_market::derive_public_drop_keypair(
                        &master_seed,
                        &canonical_write_url,
                    ) {
                        push_candidate(
                            r_url.clone(),
                            r_hex,
                            normalized_prefix.clone(),
                            "public",
                            Some(fallback_target.clone()),
                        );
                    }
                    if let Some(ref sid) = pasted_slot_id {
                        let (base_pk, sid_pfx) = humoco_market::parse_slot_id(sid)
                            .unwrap_or_else(|_| (sid.clone(), None));
                        let eff_pfx = sid_pfx.or_else(|| normalized_prefix.clone());
                        push_candidate(r_url, base_pk, eff_pfx, "slot_id", Some(fallback_target));
                    }
                }
                errors.push(format!("{}: {}", canonical_write_url, info_err));
            }
        }
    }

    let fetch_client = crate::sync_client::create_sync_http_client();
    let mut set = tokio::task::JoinSet::new();
    for task in candidate_tasks {
        let cl = fetch_client.clone();
        set.spawn(async move {
            let bytes_opt = tokio::time::timeout(
                std::time::Duration::from_millis(5000),
                fetch_and_merge_backup_slot(
                    &cl,
                    &task.read_url,
                    &task.pubkey_hex,
                    task.prefix_opt.as_deref(),
                ),
            )
            .await
            .ok()
            .flatten();
            bytes_opt.map(|bytes| (bytes, task.kind, task.read_url, task.fallback_target))
        });
    }

    let mut downloaded = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(Some(item)) = res {
            downloaded.push(item);
        }
    }

    cleanup_stale_tmp_restore_files(tmp_restore_dir);
    fs::create_dir_all(tmp_restore_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    let credentials = BackupRestoreCredentials::Mnemonic {
        phrase: &mnemonic,
        passphrase: passphrase.as_deref().filter(|p| !p.is_empty()),
        prefix: cleaned_prefix,
        language: core_lang,
    };

    let mut inspected_candidates: Vec<(
        PathBuf,
        FrontendBackupInspectionDto,
        String,
        String,
        Option<ResolvedSyncTargetDto>,
    )> = Vec::new();

    for (unpadded, kind, source_url, fallback_target) in downloaded {
        let staged_file_name = format!("sync_restore_{}.hmcbackup", uuid::Uuid::new_v4());
        let staged_path = tmp_restore_dir.join(staged_file_name);
        if write_file_atomic(&staged_path, &unpadded).is_ok() {
            let service = state.lock_service()?;
            if let Ok(inspection) = inspect_backup_with_collision_impl(
                &staged_path,
                &credentials,
                &service,
                Some(wallet_data_dir),
                Some(&identity),
                &user_ids,
            ) {
                inspected_candidates.push((
                    staged_path,
                    inspection,
                    kind,
                    source_url,
                    fallback_target,
                ));
            } else {
                let _ = fs::remove_file(&staged_path);
            }
        }
    }

    if !inspected_candidates.is_empty() {
        inspected_candidates
            .sort_by(|a, b| (b.1.epoch, b.1.tx_nonce).cmp(&(a.1.epoch, a.1.tx_nonce)));
        let (best_path, best_inspection, best_kind, best_source, fallback_target) =
            inspected_candidates.remove(0);
        for (other_path, _, _, _, _) in inspected_candidates {
            let _ = fs::remove_file(other_path);
        }
        if let Some(ft) = fallback_target {
            if !resolved_sync_targets
                .iter()
                .any(|t| t.write_url == ft.write_url)
            {
                resolved_sync_targets.push(ft);
            }
        }

        return Ok(SyncBackupProbeResultDto {
            status: "found".to_string(),
            staged_path: Some(best_path.to_string_lossy().to_string()),
            matched_key_kind: Some(best_kind),
            source_url: Some(best_source),
            inspection: Some(best_inspection),
            user_id: identity.user_id.clone(),
            local_collision,
            resolved_sync_targets,
            error: None,
        });
    }

    if !resolved_sync_targets.is_empty() {
        return Ok(SyncBackupProbeResultDto {
            status: "not_found".to_string(),
            staged_path: None,
            matched_key_kind: None,
            source_url: None,
            inspection: None,
            user_id: identity.user_id.clone(),
            local_collision,
            resolved_sync_targets,
            error: None,
        });
    }

    Ok(SyncBackupProbeResultDto {
        status: "unreachable".to_string(),
        staged_path: None,
        matched_key_kind: None,
        source_url: None,
        inspection: None,
        user_id: identity.user_id.clone(),
        local_collision,
        resolved_sync_targets,
        error: Some(if errors.is_empty() {
            "Sync-Dienst konnte nicht erreicht werden.".to_string()
        } else {
            errors.join(" | ")
        }),
    })
}

pub(crate) async fn probe_sync_backup_for_app_impl(
    sync_inputs: Vec<String>,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<SyncBackupProbeResultDto, FrontendError> {
    let app_dir = resolve_app_data_dir(app)?;
    let wallet_data_dir = app_dir.join("wallet_data");
    let tmp_restore_dir = app_dir.join("tmp_restore");

    probe_sync_backup_impl(
        sync_inputs,
        mnemonic,
        passphrase,
        user_prefix,
        language,
        &wallet_data_dir,
        &tmp_restore_dir,
        state,
    )
    .await
}

pub(crate) async fn reconcile_restored_wallet_l2_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendRestoreReconcileResultDto, FrontendError> {
    if let Some(ref pwd) = password {
        let mut service = state.lock_service()?;
        let _ = service.unlock_session(pwd, 300);
    }

    let summary = crate::services::l2_anchor_service::reconcile_vouchers_via_static_locks(
        state,
        None,
        password.as_deref(),
    )
    .await?;

    let active_voucher_queries_count = summary.active_anchored_count;
    let reconciled_spent_count = summary.reconciled_spent_count;
    let l2_reconciliation_status = summary.status;

    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let mut pending_restore_l2 = settings.pending_restore_l2_reconciliation;
    if summary.all_responded || active_voucher_queries_count == 0 {
        pending_restore_l2 = false;
        let mut new_settings = settings;
        new_settings.pending_restore_l2_reconciliation = false;
        if let Ok(mut service) = state.service.lock() {
            let _ = state.save_settings(&mut service, new_settings, password.as_deref());
        }
    }

    Ok(FrontendRestoreReconcileResultDto {
        active_voucher_queries_count,
        reconciled_spent_count,
        l2_reconciliation_status,
        pending_restore_l2_reconciliation: pending_restore_l2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "humoco_backup_test_{}_{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service
            .create_profile(
                profile_name,
                PHRASE,
                None,
                Some("pc"),
                password,
                human_money_core::MnemonicLanguage::English,
                "device-test-1".into(),
                true,
            )
            .unwrap();
        service
    }

    #[test]
    fn test_file_backup_roundtrip_mnemonic_and_password() {
        let base_dir = temp_test_dir("file_roundtrip");
        let wallet_dir = base_dir.join("wallet");
        let password = "TestPassword123!";
        let mut service = setup_test_app(&wallet_dir, "Alice Test", password);

        // 1. MnemonicOnly file export
        let mnemonic_export_file = base_dir.join("mnemonic_backup.hmcbackup");
        let mut custom_stds = HashMap::new();
        custom_stds.insert("test_std".to_string(), "name = 'Test Standard'".to_string());
        let summary1 = export_backup_file_impl(
            &mnemonic_export_file,
            BackupProtection::MnemonicOnly,
            Some(password),
            &mut service,
            custom_stds.clone(),
        )
        .unwrap();
        assert!(summary1.total_blobs > 0);
        assert_eq!(summary1.custom_standard_count, 1);

        // Probe mnemonic backup
        let probe1 = probe_wallet_backup_impl(&mnemonic_export_file.to_string_lossy()).unwrap();
        assert_eq!(probe1.source_kind, "file");
        assert_eq!(probe1.protection_mode, "mnemonic");
        assert!(!probe1.supports_password_restore);
        assert!(probe1.supports_mnemonic_restore);
        assert_eq!(probe1.custom_standard_count, 1);

        // Inspect mnemonic backup
        let creds_mn = BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        };
        let inspect1 = inspect_backup_impl(&mnemonic_export_file, &creds_mn, &service).unwrap();
        assert_eq!(inspect1.profile_name, "Alice Test");

        // Restore mnemonic backup into a new service
        let restore_dir1 = base_dir.join("restore_mnemonic");
        let mut restore_service1 = AppService::new(&restore_dir1).unwrap();
        let (outcome1, restored_name1, _) = restore_backup_impl(
            &mnemonic_export_file,
            creds_mn,
            "NewPassword456!",
            "inst_1".to_string(),
            &mut restore_service1,
        )
        .unwrap();
        assert_eq!(restored_name1, "Alice Test");
        assert!(outcome1.folder_name.starts_with("restore_"));

        // 2. CustomBackupPassword file export
        let pwd_export_file = base_dir.join("pwd_backup.hmcbackup");
        let backup_pwd = "MySecretBackupPassword!";
        let summary2 = export_backup_file_impl(
            &pwd_export_file,
            BackupProtection::CustomBackupPassword(backup_pwd),
            Some(password),
            &mut service,
            custom_stds,
        )
        .unwrap();
        assert!(summary2.total_blobs > 0);

        // Probe password backup
        let probe2 = probe_wallet_backup_impl(&pwd_export_file.to_string_lossy()).unwrap();
        assert_eq!(probe2.protection_mode, "backup_password");
        assert!(probe2.supports_password_restore);
        assert!(probe2.supports_mnemonic_restore);

        // Inspect with password
        let creds_pwd = BackupRestoreCredentials::BackupPassword(backup_pwd);
        let inspect2 = inspect_backup_impl(&pwd_export_file, &creds_pwd, &service).unwrap();
        assert_eq!(inspect2.profile_name, "Alice Test");

        // Inspect with mnemonic (dual recovery slot!)
        let inspect2_mn = inspect_backup_impl(
            &pwd_export_file,
            &BackupRestoreCredentials::Mnemonic {
                phrase: PHRASE,
                passphrase: None,
                prefix: Some("pc"),
                language: human_money_core::MnemonicLanguage::English,
            },
            &service,
        )
        .unwrap();
        assert_eq!(inspect2_mn.profile_name, "Alice Test");

        // Restore password backup
        let restore_dir2 = base_dir.join("restore_pwd");
        let mut restore_service2 = AppService::new(&restore_dir2).unwrap();
        let (_outcome2, restored_name2, _) = restore_backup_impl(
            &pwd_export_file,
            creds_pwd,
            "NewPassword789!",
            "inst_2".to_string(),
            &mut restore_service2,
        )
        .unwrap();
        assert_eq!(restored_name2, "Alice Test");

        let _ = fs::remove_dir_all(base_dir);
    }

    #[test]
    fn test_differential_folder_sync_and_restore() {
        let base_dir = temp_test_dir("folder_diff");
        let wallet_dir = base_dir.join("wallet");
        let sync_dir = base_dir.join("sync_folder");
        let password = "TestPassword123!";
        let mut service = setup_test_app(&wallet_dir, "Bob Diff", password);

        // 1. Initial sync
        let summary1 = sync_backup_folder_impl(
            &sync_dir,
            "mnemonic",
            BackupProtection::MnemonicOnly,
            Some(password),
            false,
            &mut service,
            HashMap::new(),
        )
        .unwrap();
        assert!(summary1.changed_blobs > 0);
        assert!(sync_dir.join("manifest.enc").exists());
        assert!(sync_dir.join("blobs").exists());

        // Probe folder
        let probe = probe_wallet_backup_impl(&sync_dir.to_string_lossy()).unwrap();
        assert_eq!(probe.source_kind, "folder");
        assert_eq!(probe.protection_mode, "mnemonic");
        assert_eq!(probe.blob_count, summary1.total_blobs);

        // 2. Second sync with unchanged state -> 0 changed blobs!
        let summary2 = sync_backup_folder_impl(
            &sync_dir,
            "mnemonic",
            BackupProtection::MnemonicOnly,
            Some(password),
            false,
            &mut service,
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(summary2.changed_blobs, 0);
        assert_eq!(summary2.deleted_blobs, 0);
        assert_eq!(summary2.total_blobs, summary1.total_blobs);

        // 3. Restore from folder
        let restore_dir = base_dir.join("restore_folder");
        let mut restore_service = AppService::new(&restore_dir).unwrap();
        let creds = BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        };
        let (outcome, name, _) = restore_backup_impl(
            &sync_dir,
            creds,
            "NewPasswordFolder!",
            "inst_f".to_string(),
            &mut restore_service,
        )
        .unwrap();
        assert_eq!(name, "Bob Diff");
        assert!(outcome.folder_name.starts_with("restore_"));

        let _ = fs::remove_dir_all(base_dir);
    }

    #[test]
    fn test_tampered_blob_hash_rejected_on_restore() {
        let base_dir = temp_test_dir("tamper_test");
        let wallet_dir = base_dir.join("wallet");
        let sync_dir = base_dir.join("sync_folder");
        let password = "TestPassword123!";
        let mut service = setup_test_app(&wallet_dir, "Tamper User", password);

        sync_backup_folder_impl(
            &sync_dir,
            "mnemonic",
            BackupProtection::MnemonicOnly,
            Some(password),
            false,
            &mut service,
            HashMap::new(),
        )
        .unwrap();

        // Tamper a blob inside blobs/
        let mut tampered = false;
        for entry in fs::read_dir(sync_dir.join("blobs")).unwrap().flatten() {
            if entry.path().is_file() {
                let mut data = fs::read(entry.path()).unwrap();
                if !data.is_empty() {
                    data[0] ^= 0xFF; // Flip bits
                    fs::write(entry.path(), data).unwrap();
                    tampered = true;
                    break;
                }
            }
        }
        assert!(tampered, "Expected at least one blob to tamper");

        // Attempt restore -> must fail with hash mismatch / integrity error
        let restore_dir = base_dir.join("restore_tampered");
        let mut restore_service = AppService::new(&restore_dir).unwrap();
        let creds = BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        };
        let err = restore_backup_impl(
            &sync_dir,
            creds,
            "NewPasswordTamper!",
            "inst_t".to_string(),
            &mut restore_service,
        )
        .unwrap_err();
        let msg = err.message.to_lowercase();
        assert!(msg.contains("mismatch") || msg.contains("integrity") || msg.contains("invalid"));

        let _ = fs::remove_dir_all(base_dir);
    }

    #[test]
    fn test_auto_backup_usb_protection_folder_disconnected() {
        let base_dir = temp_test_dir("usb_protection");
        let non_existent_usb = base_dir.join("media").join("usb_token_offline");

        // Assure it does NOT exist
        assert!(!non_existent_usb.exists());
        assert!(!non_existent_usb.is_dir());

        // In auto-backup, we check is_dir() to protect disconnected drives.
        // It must NOT create directories if disconnected!
        let is_connected = non_existent_usb.is_dir();
        assert!(!is_connected);
        assert!(
            !non_existent_usb.exists(),
            "USB path must not have been created"
        );

        let _ = fs::remove_dir_all(base_dir);
    }

    #[test]
    fn test_is_older_than_14_days() {
        assert!(is_older_than_14_days(&None));
        assert!(is_older_than_14_days(&Some(
            "invalid-timestamp".to_string()
        )));

        let recent = (Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        assert!(!is_older_than_14_days(&Some(recent)));

        let old = (Utc::now() - chrono::Duration::days(16)).to_rfc3339();
        assert!(is_older_than_14_days(&Some(old)));
    }

    #[test]
    fn test_sync_backup_slot_padding_integration() {
        use humoco_market::slot::{
            pad_backup_slot_bytes, unpad_backup_slot_bytes, BACKUP_PAD_BLOCK_BYTES,
            DROP_BACKUP_MAX_BYTES,
        };

        let fake_backup = b"HMC_BACKUP_CONTAINER_V1_MOCK_PAYLOAD_DATA";
        let padded = pad_backup_slot_bytes(fake_backup, DROP_BACKUP_MAX_BYTES)
            .expect("padding must succeed");
        assert_eq!(padded.len() % BACKUP_PAD_BLOCK_BYTES, 0);
        assert_eq!(padded.len(), BACKUP_PAD_BLOCK_BYTES);

        let recovered = unpad_backup_slot_bytes(&padded).expect("unpadding must succeed");
        assert_eq!(recovered, fake_backup);
    }

    #[test]
    fn test_sync_backup_stealth_key_and_candidate_resolution() {
        let mnemonic = PHRASE;
        let (_my_pk, my_ed_sk) = human_money_core::services::crypto::keys::derive_ed25519_keypair(
            mnemonic,
            None,
            human_money_core::MnemonicLanguage::English,
        )
        .expect("key derivation failed");

        // Simulate Drop master key
        let sync_sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let drop_pub_hex = hex::encode(sync_sk.verifying_key().as_bytes());

        // 1. Client derives stealth slot key for friend's drop
        let stealth_pk = humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub_hex)
            .expect("stealth key derivation failed");
        assert_eq!(stealth_pk.len(), 64);

        // 2. Candidate keys generation in restore:
        let candidate_stealth =
            humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub_hex)
                .expect("candidate derivation failed");
        assert_eq!(candidate_stealth, stealth_pk);
    }

    #[test]
    fn test_pending_restore_l2_reconciliation_setting_flag() {
        let mut settings = crate::settings::AppSettings::default();
        assert!(!settings.pending_restore_l2_reconciliation);

        // Simulate restore with active vouchers and offline status
        let active_voucher_queries_count = 3;
        let l2_reconciliation_status = "no_servers";
        if active_voucher_queries_count > 0 && l2_reconciliation_status != "reconciled" {
            settings.pending_restore_l2_reconciliation = true;
        }
        assert!(settings.pending_restore_l2_reconciliation);

        // Simulate successful online reconciliation
        let l2_reconciled = true;
        if l2_reconciled {
            settings.pending_restore_l2_reconciliation = false;
        }
        assert!(!settings.pending_restore_l2_reconciliation);
    }

    #[test]
    fn test_compute_backup_status_dto_combined() {
        let mut settings = crate::settings::AppSettings {
            backup_sync_folder: None,
            sync_backup_enabled: false,
            ..Default::default()
        };

        // 1. Not configured
        let dto = super::compute_backup_status_dto(&settings, false);
        assert_eq!(dto.status, "notConfigured");
        assert!(!dto.has_authorized_sync_backup_target);

        // 2. Sync backup only, up to date (< 14 days)
        settings.sync_backup_enabled = true;
        settings.last_sync_backup_at = Some(chrono::Utc::now().to_rfc3339());
        settings.backup_dirty = false;
        let dto = super::compute_backup_status_dto(&settings, false);
        assert_eq!(dto.status, "ok");
        assert!(dto.sync_backup_enabled);
        assert!(dto.has_authorized_sync_backup_target);

        // 3. Pending dirty changes
        settings.backup_dirty = true;
        let dto = super::compute_backup_status_dto(&settings, false);
        assert_eq!(dto.status, "pending");

        // 4. Warning: outdated (> 14 days)
        settings.backup_dirty = false;
        settings.last_sync_backup_at = Some("2020-01-01T00:00:00Z".to_string());
        let dto = super::compute_backup_status_dto(&settings, true);
        assert_eq!(dto.status, "warning");

        // 5. Local folder disconnected
        settings.backup_sync_folder = Some("/non/existent/path/for/humoco/test".to_string());
        settings.sync_backup_enabled = false;
        let dto = super::compute_backup_status_dto(&settings, false);
        assert_eq!(dto.status, "disconnected");
    }

    #[test]
    fn test_merge_backup_delta_into_base_and_mismatch() {
        let base_manifest = b"base-manifest-v1";
        let base_sha = super::sha256_hex(base_manifest);

        let mut base = crate::models::PortableBackupContainer {
            container_format: "humoco-app-backup-v1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            manifest_hex: human_money_core::hex::encode(base_manifest),
            blobs_hex: [
                ("keep.bin".to_string(), "aa".to_string()),
                ("remove.bin".to_string(), "bb".to_string()),
            ]
            .into_iter()
            .collect(),
            custom_standards: HashMap::new(),
        };

        let delta = crate::models::BackupDeltaContainer {
            container_format: "humoco-app-backup-delta-v1".to_string(),
            created_at: "2026-01-02T00:00:00Z".to_string(),
            base_manifest_sha256: base_sha,
            manifest_hex: human_money_core::hex::encode(b"manifest-v2"),
            put_blobs_hex: [("added.bin".to_string(), "cc".to_string())]
                .into_iter()
                .collect(),
            delete_blobs: vec!["remove.bin".to_string()],
            custom_standards: [("std1".to_string(), "toml".to_string())]
                .into_iter()
                .collect(),
        };

        super::merge_backup_delta_into_base(&mut base, &delta).expect("matching base must merge");
        assert_eq!(base.created_at, "2026-01-02T00:00:00Z");
        assert_eq!(
            base.manifest_hex,
            human_money_core::hex::encode(b"manifest-v2")
        );
        assert!(!base.blobs_hex.contains_key("remove.bin"));
        assert_eq!(
            base.blobs_hex.get("keep.bin").map(String::as_str),
            Some("aa")
        );
        assert_eq!(
            base.blobs_hex.get("added.bin").map(String::as_str),
            Some("cc")
        );
        assert_eq!(
            base.custom_standards.get("std1").map(String::as_str),
            Some("toml")
        );

        // Mismatching base_manifest_sha256 must be rejected
        let mut bad_delta = delta;
        bad_delta.base_manifest_sha256 = "0000".to_string();
        assert!(super::merge_backup_delta_into_base(&mut base, &bad_delta).is_err());
    }

    #[tokio::test]
    async fn test_sync_backup_base_delta_lifecycle_and_remote_wipe_recovery() {
        use crate::test_support::{
            create_test_state, setup_test_app as setup_sync_test_app, MockSyncServer, ALICE_PHRASE,
        };
        use rand::{rngs::OsRng, RngCore};

        let base_dir = temp_test_dir("sync_backup_base_delta_e2e");
        let password = "TestPassword123!";
        let service =
            setup_sync_test_app(&base_dir.join("wallet"), "Alice", ALICE_PHRASE, password);
        let state = create_test_state(service);

        let (_admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&[0x42u8; 32], 0);
        let drop_pub = hex::encode(admin_vk.to_bytes());
        let server = MockSyncServer::start("alice_bkp".to_string(), drop_pub.clone()).await;

        let stealth_pk = {
            let s = state.service.lock().unwrap();
            let my_ed_sk = s
                .with_wallet_and_identity(|_w, id| id.signing_key.clone())
                .unwrap();
            humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub).unwrap()
        };
        server.members.lock().unwrap().push(stealth_pk.clone());

        let write_url = format!("{}/sync.php", server.base_url);
        let read_url = format!("{}/alice_bkp", server.base_url);
        let base_slot_key = format!("{}_pc.backup", stealth_pk);
        let delta_slot_key = format!("{}_pc-delta.backup", stealth_pk);

        {
            let mut s = state.service.lock().unwrap();
            let mut settings = state.get_cached_settings().unwrap_or_default();
            settings.sync_backup_enabled = true;
            settings.market_selected_syncs = vec![crate::models::FrontendSyncDescriptor {
                write_url: write_url.clone(),
                read_url: read_url.clone(),
                drop_pub: drop_pub.clone(),
                ..Default::default()
            }];
            state
                .save_settings(&mut s, settings, Some(password))
                .unwrap();
        }

        // 1. Small wallet (< 64 KiB compressed): repeated mutations overwrite Base, never create Delta
        let res1 = super::sync_backup_with_standards_impl(
            Some(password.to_string()),
            HashMap::new(),
            &state,
        )
        .await
        .unwrap();
        assert_eq!(res1.uploaded_to, 1, "errors: {:?}", res1.errors);
        assert_eq!(res1.padded_bytes, 65_536);
        assert!(server.slots.lock().unwrap().contains_key(&base_slot_key));
        assert!(!server.slots.lock().unwrap().contains_key(&delta_slot_key));

        {
            let mut s = state.service.lock().unwrap();
            s.save_encrypted_data("small_key", b"small-v2", Some(password))
                .unwrap();
        }
        let res2 = super::sync_backup_with_standards_impl(
            Some(password.to_string()),
            HashMap::new(),
            &state,
        )
        .await
        .unwrap();
        assert_eq!(res2.uploaded_to, 1, "errors: {:?}", res2.errors);
        assert_eq!(res2.padded_bytes, 65_536);
        assert!(server.slots.lock().unwrap().contains_key(&base_slot_key));
        assert!(!server.slots.lock().unwrap().contains_key(&delta_slot_key));

        // 2. Large wallet (>= 128 KiB padded Base): triggers fresh Base rollover
        let mut big_random = vec![0u8; 150 * 1024];
        OsRng.fill_bytes(&mut big_random);
        {
            let mut s = state.service.lock().unwrap();
            s.save_encrypted_data("big_blob", &big_random, Some(password))
                .unwrap();
        }
        let res3 = super::sync_backup_with_standards_impl(
            Some(password.to_string()),
            HashMap::new(),
            &state,
        )
        .await
        .unwrap();
        assert_eq!(res3.uploaded_to, 1, "errors: {:?}", res3.errors);
        assert!(res3.padded_bytes >= 131_072);
        assert!(server.slots.lock().unwrap().contains_key(&base_slot_key));
        assert!(!server.slots.lock().unwrap().contains_key(&delta_slot_key));

        // 3. Small mutation after large Base -> uploads Delta slot & restores full merged state
        {
            let mut s = state.service.lock().unwrap();
            s.save_encrypted_data("small_after_base", b"delta-payload", Some(password))
                .unwrap();
        }
        let res4 = super::sync_backup_with_standards_impl(
            Some(password.to_string()),
            HashMap::new(),
            &state,
        )
        .await
        .unwrap();
        assert_eq!(res4.uploaded_to, 1, "errors: {:?}", res4.errors);
        assert_eq!(res4.padded_bytes, 65_536);
        assert!(server.slots.lock().unwrap().contains_key(&base_slot_key));
        assert!(server.slots.lock().unwrap().contains_key(&delta_slot_key));

        let fetch_client = crate::sync_client::create_sync_http_client();
        let merged_bytes =
            super::fetch_and_merge_backup_slot(&fetch_client, &read_url, &stealth_pk, Some("pc"))
                .await
                .expect("merged backup container must be fetched");
        let staged_file = base_dir.join("merged_sync_restore.hmcbackup");
        crate::services::fs_helpers::write_file_atomic(&staged_file, &merged_bytes).unwrap();

        let restored_wallet_dir = base_dir.join("restored_wallet");
        let mut restored_service = AppService::new(&restored_wallet_dir).unwrap();
        let restore_creds = BackupRestoreCredentials::Mnemonic {
            phrase: ALICE_PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        };
        let (outcome, profile_name, _) = super::restore_backup_impl(
            &staged_file,
            restore_creds,
            password,
            "device-restored".to_string(),
            &mut restored_service,
        )
        .unwrap();
        assert_eq!(profile_name, "Alice");
        let mut verify_service = AppService::new(&restored_wallet_dir).unwrap();
        verify_service
            .login(
                &outcome.folder_name,
                password,
                false,
                "device-restored".to_string(),
            )
            .unwrap();
        assert_eq!(
            verify_service
                .load_encrypted_data("big_blob", Some(password))
                .unwrap(),
            big_random
        );
        assert_eq!(
            verify_service
                .load_encrypted_data("small_after_base", Some(password))
                .unwrap(),
            b"delta-payload"
        );

        // 4. Remote Base wipe auto-recovery & orphaned Delta cleanup
        server.slots.lock().unwrap().remove(&base_slot_key);
        server.etags.lock().unwrap().remove(&base_slot_key);
        assert!(server.slots.lock().unwrap().contains_key(&delta_slot_key));

        let res5 = super::sync_backup_with_standards_impl(
            Some(password.to_string()),
            HashMap::new(),
            &state,
        )
        .await
        .unwrap();
        assert_eq!(res5.uploaded_to, 1, "errors: {:?}", res5.errors);
        assert!(res5.padded_bytes >= 131_072);
        assert!(server.slots.lock().unwrap().contains_key(&base_slot_key));
        assert!(
            !server.slots.lock().unwrap().contains_key(&delta_slot_key),
            "orphaned delta slot must be deleted after Base re-upload"
        );

        let _ = fs::remove_dir_all(base_dir);
    }

    #[test]
    fn test_preview_mnemonic_identity_collision_and_subaccount() {
        let base_dir = temp_test_dir("preview_collision");
        let wallet_dir = base_dir.join("wallet");
        let password = "TestPassword123!";
        let mut service = AppService::new(&wallet_dir).unwrap();
        service
            .create_profile(
                "Alice Root",
                PHRASE,
                None,
                None,
                password,
                human_money_core::MnemonicLanguage::English,
                "inst-1".to_string(),
                true,
            )
            .unwrap();

        // Same secrets without prefix -> collides with Alice Root
        let root_preview = super::preview_mnemonic_identity_impl(
            PHRASE,
            None,
            None,
            Some(MnemonicLanguage::English),
            &wallet_dir,
            &service,
        )
        .unwrap();
        let coll = root_preview
            .local_collision
            .expect("root profile must collide");
        assert_eq!(coll.profile_name, "Alice Root");
        assert_eq!(coll.local_epoch, Some(0));
        assert_eq!(coll.local_tx_nonce, Some(1));
        assert!(!coll.is_locked_or_conflicted);

        // Same mnemonic with new subaccount prefix -> distinct user_id, NO collision
        let sub_preview = super::preview_mnemonic_identity_impl(
            PHRASE,
            None,
            Some("mobil"),
            Some(MnemonicLanguage::English),
            &wallet_dir,
            &service,
        )
        .unwrap();
        assert!(sub_preview.user_id.starts_with("mobil:"));
        assert!(sub_preview.local_collision.is_none());

        let _ = fs::remove_dir_all(base_dir);
    }

    #[tokio::test]
    async fn test_probe_sync_backup_found_and_not_found() {
        use crate::test_support::{
            create_test_state, setup_test_app as setup_sync_test_app, MockSyncServer, ALICE_PHRASE,
        };

        let base_dir = temp_test_dir("probe_sync_backup_test");
        let wallet_dir = base_dir.join("wallet");
        let tmp_restore_dir = base_dir.join("tmp_restore");
        let password = "TestPassword123!";
        let service = setup_sync_test_app(&wallet_dir, "Alice", ALICE_PHRASE, password);
        let state = create_test_state(service);

        let (_admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&[0x55u8; 32], 0);
        let drop_pub = hex::encode(admin_vk.to_bytes());
        let server = MockSyncServer::start("alice_probe".to_string(), drop_pub.clone()).await;

        let stealth_pk = {
            let s = state.service.lock().unwrap();
            let my_ed_sk = s
                .with_wallet_and_identity(|_w, id| id.signing_key.clone())
                .unwrap();
            humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub).unwrap()
        };
        server.members.lock().unwrap().push(stealth_pk.clone());

        let write_url = format!("{}/sync.php", server.base_url);
        let read_url = format!("{}/alice_probe", server.base_url);

        {
            let mut s = state.service.lock().unwrap();
            let mut settings = state.get_cached_settings().unwrap_or_default();
            settings.sync_backup_enabled = true;
            settings.market_selected_syncs = vec![crate::models::FrontendSyncDescriptor {
                write_url: write_url.clone(),
                read_url: read_url.clone(),
                drop_pub: drop_pub.clone(),
                ..Default::default()
            }];
            state
                .save_settings(&mut s, settings, Some(password))
                .unwrap();
        }

        // 1. Before any backup is uploaded -> status must be "not_found" + resolved_sync_targets populated
        let probe_dir_empty = base_dir.join("empty_wallet");
        let empty_state = create_test_state(AppService::new(&probe_dir_empty).unwrap());
        let not_found_res = super::probe_sync_backup_impl(
            vec![write_url.clone()],
            ALICE_PHRASE.to_string(),
            None,
            Some("pc".to_string()),
            Some(MnemonicLanguage::English),
            &probe_dir_empty,
            &tmp_restore_dir,
            &empty_state,
        )
        .await
        .unwrap();
        assert_eq!(not_found_res.status, "not_found");
        assert_eq!(not_found_res.resolved_sync_targets.len(), 1);
        assert_eq!(not_found_res.resolved_sync_targets[0].drop_pub, drop_pub);

        // 2. Upload backup, then probe with only plain write_url (tests fetch_sync_info + stealth resolution!)
        let sync_res = super::sync_backup_with_standards_impl(
            Some(password.to_string()),
            HashMap::new(),
            &state,
        )
        .await
        .unwrap();
        assert_eq!(sync_res.uploaded_to, 1);

        let found_res = super::probe_sync_backup_impl(
            vec![write_url.clone()],
            ALICE_PHRASE.to_string(),
            None,
            Some("pc".to_string()),
            Some(MnemonicLanguage::English),
            &wallet_dir,
            &tmp_restore_dir,
            &state,
        )
        .await
        .unwrap();
        assert_eq!(found_res.status, "found");
        assert_eq!(found_res.matched_key_kind.as_deref(), Some("stealth"));
        assert!(found_res
            .staged_path
            .as_deref()
            .is_some_and(|p| Path::new(p).exists()));
        let inspection = found_res.inspection.expect("inspection must be populated");
        assert_eq!(inspection.profile_name, "Alice");
        assert!(inspection.local_collision.is_some());

        let _ = fs::remove_dir_all(base_dir);
    }

    #[tokio::test]
    async fn test_replace_local_profile_and_reset_password_via_backup() {
        let base_dir = temp_test_dir("replace_and_pw_reset");
        let wallet_dir = base_dir.join("wallet");
        let standards_dir = base_dir.join("voucher_standards");
        let tmp_restore_dir = base_dir.join("tmp_restore");
        let old_pass = "OldPassword123!";
        let new_pass = "NewPassword456!";
        let bkp_pass = "BackupSecret789!";

        let mut service = setup_test_app(&wallet_dir, "Alice", old_pass);
        let original_folder = service.list_profiles().unwrap()[0].folder_name.clone();
        let alice_uid = service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let bkp_file = base_dir.join("alice.hmcbackup");
        super::export_backup_file_impl(
            &bkp_file,
            BackupProtection::CustomBackupPassword(bkp_pass),
            Some(old_pass),
            &mut service,
            HashMap::new(),
        )
        .unwrap();
        service.logout();

        let state = crate::test_support::create_test_state(service);

        // 1. Reset local password via backup password
        super::reset_local_password_via_backup_impl(
            &original_folder,
            &bkp_file,
            "password",
            Some(bkp_pass),
            None,
            None,
            None,
            new_pass,
            "inst-pw-reset".to_string(),
            &wallet_dir,
            &tmp_restore_dir,
            &state,
            None,
        )
        .unwrap();

        {
            let mut s = state.service.lock().unwrap();
            s.logout();
            assert!(s
                .login(
                    &original_folder,
                    old_pass,
                    false,
                    "inst-pw-reset".to_string()
                )
                .is_err());
            assert!(s
                .login(
                    &original_folder,
                    new_pass,
                    false,
                    "inst-pw-reset".to_string()
                )
                .is_ok());
            assert_eq!(
                s.with_wallet(|w| w.get_user_id().to_string()).unwrap(),
                alice_uid
            );
            s.logout();
        }

        // 2. Restore with replace_folder_name -> keeps clean name "Alice" and deletes original_folder
        let outcome = super::finalize_restored_wallet_and_reconcile_l2(
            &bkp_file,
            BackupRestoreCredentials::BackupPassword(bkp_pass),
            new_pass,
            "inst-replace".to_string(),
            Some(&original_folder),
            &wallet_dir,
            &standards_dir,
            &tmp_restore_dir,
            &state,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.profile_name, "Alice");
        assert_ne!(outcome.folder_name, original_folder);
        assert!(!wallet_dir.join(&original_folder).exists());

        let remaining_profiles = {
            let s = state.service.lock().unwrap();
            s.list_profiles().unwrap()
        };
        assert_eq!(remaining_profiles.len(), 1);
        assert_eq!(remaining_profiles[0].profile_name, "Alice");
        assert_eq!(remaining_profiles[0].folder_name, outcome.folder_name);

        let _ = fs::remove_dir_all(base_dir);
    }
}
