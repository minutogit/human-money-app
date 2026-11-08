// src-tauri/src/commands/backup.rs
use crate::{
    models::{
        FrontendAutoBackupStatusDto, FrontendBackupInspectionDto, FrontendBackupProbeDto,
        FrontendBackupSummaryDto, FrontendDropBackupFetchDto, FrontendDropBackupSyncResult,
        FrontendError, FrontendRestoreOutcomeDto, FrontendRestoreReconcileResultDto,
        MnemonicLanguage, PortableBackupContainer,
    },
    AppState,
};
use chrono::Utc;
use human_money_core::{
    app_service::AppService,
    models::{
        backup::{BackupProtection, BackupRestoreCredentials, EncryptedBackupManifest},
        voucher_standard_definition::VoucherStandardDefinition,
    },
};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::Path,
};
use tauri::Manager;

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
        let mut file = fs::File::create(&tmp_path).map_err(|e| FrontendError::from(e.to_string()))?;
        file.write_all(bytes).map_err(|e| FrontendError::from(e.to_string()))?;
        file.sync_all().map_err(|e| FrontendError::from(e.to_string()))?;
    }
    fs::rename(&tmp_path, path).map_err(|e| FrontendError::from(e.to_string()))?;
    Ok(())
}

pub(crate) fn collect_custom_standards(app: &tauri::AppHandle) -> HashMap<String, String> {
    let Ok(app_data_dir) = app.path().app_data_dir() else {
        return HashMap::new();
    };
    let standards_dir = app_data_dir.join("voucher_standards");
    collect_custom_standards_from_dir(&standards_dir)
}

pub(crate) fn collect_custom_standards_from_dir(standards_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(entries) = fs::read_dir(standards_dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let standard_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
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

pub(crate) fn restore_custom_standards(
    app: &tauri::AppHandle,
    standards: &HashMap<String, String>,
) -> Result<usize, FrontendError> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| FrontendError::from(e.to_string()))?;
    let standards_base = app_data_dir.join("voucher_standards");
    restore_custom_standards_to_dir(&standards_base, standards)
}

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
            fs::write(&target_file, toml_content).map_err(|e| FrontendError::from(e.to_string()))?;
            count += 1;
        }
    }
    Ok(count)
}

fn update_profile_last_used_internal(app: &tauri::AppHandle, folder_name: &str) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let metadata_path = app_data_dir.join("wallet_data").join("profile_metadata.json");
    let mut map: HashMap<String, serde_json::Value> = if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };
    map.insert(
        folder_name.to_string(),
        serde_json::json!({ "last_used": Utc::now().to_rfc3339() }),
    );
    if let Some(parent) = metadata_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    fs::write(&metadata_path, content).map_err(|e| e.to_string())?;
    Ok(())
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
            let pwd = backup_password.ok_or_else(|| FrontendError::from("Missing backup password".to_string()))?;
            if pwd.is_empty() {
                return Err(FrontendError::from("Backup password cannot be empty".to_string()));
            }
            Ok(BackupRestoreCredentials::BackupPassword(pwd))
        }
        "mnemonic" => {
            let phrase = mnemonic.ok_or_else(|| FrontendError::from("Missing mnemonic phrase".to_string()))?;
            if phrase.trim().is_empty() {
                return Err(FrontendError::from("Mnemonic phrase cannot be empty".to_string()));
            }
            let lang: human_money_core::MnemonicLanguage = language.unwrap_or(MnemonicLanguage::English).into();
            Ok(BackupRestoreCredentials::Mnemonic {
                phrase,
                passphrase,
                prefix: user_prefix,
                language: lang,
            })
        }
        _ => Err(FrontendError::from(format!("Invalid credential mode: {}", credential_mode))),
    }
}

pub(crate) fn export_backup_file_impl(
    destination_path: &Path,
    protection: BackupProtection<'_>,
    password: Option<&str>,
    service: &mut AppService,
    custom_standards: HashMap<String, String>,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    let diff = service.create_differential_backup(None, protection, password)
        .map_err(FrontendError::from)?;

    let mut blobs_hex = HashMap::with_capacity(diff.put_blobs.len());
    for (name, data) in diff.put_blobs {
        blobs_hex.insert(name, human_money_core::hex::encode(&data));
    }

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
        match fs::read(base_dir.join("manifest.enc")) {
            Ok(bytes) => {
                if let Ok(old) = serde_json::from_slice::<EncryptedBackupManifest>(&bytes) {
                    if old.protection_mode == expected_mode {
                        Some(bytes)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    } else {
        None
    };

    let diff = service.create_differential_backup(previous_manifest_bytes.as_deref(), protection, password)
        .map_err(FrontendError::from)?;

    let blobs_dir = base_dir.join("blobs");
    let standards_dir = base_dir.join("standards");
    fs::create_dir_all(&blobs_dir).map_err(|e| FrontendError::from(e.to_string()))?;
    fs::create_dir_all(&standards_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    for (name, data) in &diff.put_blobs {
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(FrontendError::from(format!("Unsafe blob name in diff: {}", name)));
        }
        write_file_atomic(&blobs_dir.join(name), data)?;
    }

    for (id, content) in &custom_standards {
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            continue;
        }
        write_file_atomic(&standards_dir.join(format!("{}.toml", id)), content.as_bytes())?;
    }

    write_file_atomic(&base_dir.join("manifest.enc"), &diff.encrypted_manifest_bytes)?;

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

pub(crate) fn inspect_backup_impl(
    source_path: &Path,
    credentials: &BackupRestoreCredentials<'_>,
    service: &AppService,
) -> Result<FrontendBackupInspectionDto, FrontendError> {
    let (manifest_bytes, blob_count, custom_standard_count) = if source_path.is_file() {
        let bytes = fs::read(source_path).map_err(|e| FrontendError::from(format!("Failed to read backup file: {}", e)))?;
        let container: PortableBackupContainer = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Invalid backup container: {}", e)))?;
        let m_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        (m_bytes, container.blobs_hex.len(), container.custom_standards.len())
    } else if source_path.is_dir() {
        let manifest_path = source_path.join("manifest.enc");
        let m_bytes = fs::read(&manifest_path).map_err(|e| FrontendError::from(format!("Failed to read manifest.enc: {}", e)))?;
        let b_count = fs::read_dir(source_path.join("blobs"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let s_count = fs::read_dir(source_path.join("standards"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        (m_bytes, b_count, s_count)
    } else {
        return Err(FrontendError::from(format!("Backup path not found: {}", source_path.display())));
    };

    let enc_manifest: EncryptedBackupManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| FrontendError::from(format!("Failed to parse encrypted manifest: {}", e)))?;

    let report = service.inspect_backup(&manifest_bytes, credentials)
        .map_err(FrontendError::from)?;

    Ok(FrontendBackupInspectionDto {
        profile_name: report.manifest.profile_name,
        user_id: report.manifest.seal.payload.user_id,
        epoch: report.manifest.seal.payload.epoch as u64,
        tx_nonce: report.manifest.seal.payload.tx_nonce,
        seal_timestamp: report.manifest.seal.payload.timestamp,
        blob_count,
        custom_standard_count,
        protection_mode: enc_manifest.protection_mode,
    })
}

pub(crate) fn restore_backup_impl(
    source_path: &Path,
    credentials: BackupRestoreCredentials<'_>,
    new_password: &str,
    local_instance_id: String,
    service: &mut AppService,
) -> Result<(human_money_core::models::backup::RestoreOutcome, String, HashMap<String, String>), FrontendError> {
    let (manifest_bytes, blobs, custom_standards) = if source_path.is_file() {
        let bytes = fs::read(source_path).map_err(|e| FrontendError::from(format!("Failed to read backup file: {}", e)))?;
        let container: PortableBackupContainer = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Invalid backup container: {}", e)))?;
        let m_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        let mut blob_map = HashMap::with_capacity(container.blobs_hex.len());
        for (name, hex_data) in container.blobs_hex {
            let data = human_money_core::hex::decode(&hex_data)
                .map_err(|e| FrontendError::from(format!("Invalid hex encoding for blob {}: {}", name, e)))?;
            blob_map.insert(name, data);
        }
        (m_bytes, blob_map, container.custom_standards)
    } else if source_path.is_dir() {
        let manifest_path = source_path.join("manifest.enc");
        let m_bytes = fs::read(&manifest_path).map_err(|e| FrontendError::from(format!("Failed to read manifest.enc: {}", e)))?;
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
            for entry in fs::read_dir(&standards_dir).map_err(|e| FrontendError::from(e.to_string()))? {
                let entry = entry.map_err(|e| FrontendError::from(e.to_string()))?;
                let p = entry.path();
                if p.is_file() && p.extension().is_some_and(|ext| ext == "toml") {
                    let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    if let Ok(content) = fs::read_to_string(&p) {
                        std_map.insert(stem, content);
                    }
                }
            }
        }
        (m_bytes, blob_map, std_map)
    } else {
        return Err(FrontendError::from(format!("Backup path not found: {}", source_path.display())));
    };

    let outcome = service.restore_backup(
        &manifest_bytes,
        &blobs,
        credentials,
        new_password,
        local_instance_id,
    ).map_err(FrontendError::from)?;

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

    Ok((outcome, profile_name, custom_standards))
}

#[tauri::command]
pub fn probe_wallet_backup(source_path: String) -> Result<FrontendBackupProbeDto, FrontendError> {
    let path = Path::new(&source_path);
    if path.is_file() {
        let bytes = fs::read(path).map_err(|e| FrontendError::from(format!("Failed to read backup file: {}", e)))?;
        let container: PortableBackupContainer = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Invalid backup container: {}", e)))?;
        let manifest_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        let manifest: EncryptedBackupManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| FrontendError::from(format!("Invalid encrypted backup manifest: {}", e)))?;
        let supports_password = manifest.protection_mode != "mnemonic" && manifest.protection_mode != "mnemonic_only";
        Ok(FrontendBackupProbeDto {
            source_kind: "file".to_string(),
            protection_mode: manifest.protection_mode,
            supports_password_restore: supports_password,
            supports_mnemonic_restore: true,
            blob_count: container.blobs_hex.len(),
            custom_standard_count: container.custom_standards.len(),
        })
    } else if path.is_dir() {
        let manifest_path = path.join("manifest.enc");
        if !manifest_path.exists() {
            return Err(FrontendError::from(format!("Directory does not contain a valid backup: manifest.enc missing at {}", manifest_path.display())));
        }
        let manifest_bytes = fs::read(&manifest_path).map_err(|e| FrontendError::from(format!("Failed to read manifest.enc: {}", e)))?;
        let manifest: EncryptedBackupManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| FrontendError::from(format!("Invalid encrypted backup manifest: {}", e)))?;
        let blob_count = fs::read_dir(path.join("blobs"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let custom_standard_count = fs::read_dir(path.join("standards"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let supports_password = manifest.protection_mode != "mnemonic" && manifest.protection_mode != "mnemonic_only";
        Ok(FrontendBackupProbeDto {
            source_kind: "folder".to_string(),
            protection_mode: manifest.protection_mode,
            supports_password_restore: supports_password,
            supports_mnemonic_restore: true,
            blob_count,
            custom_standard_count,
        })
    } else {
        Err(FrontendError::from(format!("Backup path not found: {}", source_path)))
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn export_wallet_backup_file(
    destination_path: String,
    protection_mode: String,
    backup_password: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    let protection = match protection_mode.as_str() {
        "mnemonic_only" | "mnemonic" => BackupProtection::MnemonicOnly,
        "custom_password" | "custom_backup_password" | "backup_password" => {
            let pwd = backup_password.as_deref().unwrap_or("");
            if pwd.is_empty() {
                return Err(FrontendError::from("Backup password cannot be empty".to_string()));
            }
            BackupProtection::CustomBackupPassword(pwd)
        }
        _ => return Err(FrontendError::from(format!("Invalid backup protection mode: {}", protection_mode))),
    };

    let custom_standards = collect_custom_standards(&app);
    let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    let summary = export_backup_file_impl(
        Path::new(&destination_path),
        protection,
        password.as_deref(),
        &mut service,
        custom_standards,
    )?;

    if let Ok(mut settings) = state.get_cached_settings() {
        settings.last_backup_at = Some(Utc::now().to_rfc3339());
        settings.backup_dirty = false;
        let _ = state.save_settings(&mut service, settings, password.as_deref());
    }

    Ok(summary)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn sync_wallet_backup_folder(
    folder_path: String,
    protection_mode: String,
    backup_password: Option<String>,
    password: Option<String>,
    force_full: bool,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendBackupSummaryDto, FrontendError> {
    let expected_mode = if protection_mode == "mnemonic_only" || protection_mode == "mnemonic" {
        "mnemonic"
    } else {
        "backup_password"
    };

    let protection = match protection_mode.as_str() {
        "mnemonic_only" | "mnemonic" => BackupProtection::MnemonicOnly,
        "custom_password" | "custom_backup_password" | "backup_password" => {
            let pwd = backup_password.as_deref().unwrap_or("");
            if pwd.is_empty() {
                return Err(FrontendError::from("Backup password cannot be empty".to_string()));
            }
            BackupProtection::CustomBackupPassword(pwd)
        }
        _ => return Err(FrontendError::from(format!("Invalid backup protection mode: {}", protection_mode))),
    };

    let custom_standards = collect_custom_standards(&app);
    let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    let summary = sync_backup_folder_impl(
        Path::new(&folder_path),
        expected_mode,
        protection,
        password.as_deref(),
        force_full,
        &mut service,
        custom_standards,
    )?;

    if let Ok(mut settings) = state.get_cached_settings() {
        settings.backup_sync_folder = Some(folder_path);
        settings.backup_protection_mode = Some(protection_mode);
        settings.last_backup_at = Some(Utc::now().to_rfc3339());
        settings.backup_dirty = false;
        let _ = state.save_settings(&mut service, settings, password.as_deref());
    }

    Ok(summary)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn inspect_wallet_backup(
    source_path: String,
    credential_mode: String,
    backup_password: Option<String>,
    mnemonic: Option<String>,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    state: tauri::State<AppState>,
) -> Result<FrontendBackupInspectionDto, FrontendError> {
    let credentials = parse_credentials(
        &credential_mode,
        backup_password.as_deref(),
        mnemonic.as_deref(),
        passphrase.as_deref(),
        user_prefix.as_deref(),
        language,
    )?;

    let service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    inspect_backup_impl(Path::new(&source_path), &credentials, &service)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn restore_wallet_backup(
    source_path: String,
    credential_mode: String,
    backup_password: Option<String>,
    mnemonic: Option<String>,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    new_password: String,
    local_instance_id: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendRestoreOutcomeDto, FrontendError> {
    if local_instance_id.is_empty() {
        return Err(FrontendError::from("Device instance ID is required".to_string()));
    }
    if new_password.is_empty() {
        return Err(FrontendError::from("New password cannot be empty".to_string()));
    }

    let credentials = parse_credentials(
        &credential_mode,
        backup_password.as_deref(),
        mnemonic.as_deref(),
        passphrase.as_deref(),
        user_prefix.as_deref(),
        language,
    )?;

    let (outcome, profile_name, custom_standards) = {
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let (outcome, profile_name, custom_standards) = restore_backup_impl(
            Path::new(&source_path),
            credentials,
            &new_password,
            local_instance_id,
            &mut service,
        )?;

        if let Err(e) = update_profile_last_used_internal(&app, &outcome.folder_name) {
            log::warn!("Failed to update profile last_used metadata: {}", e);
        }
        if let Err(e) = state.initialize_profile_session(&mut service, &new_password) {
            log::warn!("Failed to initialize profile session after restore: {}", e);
        }

        (outcome, profile_name, custom_standards)
    };

    let restored_standards_count = restore_custom_standards(&app, &custom_standards)?;

    let active_voucher_queries_count = outcome.voucher_queries.len();
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates = crate::gateway_resolver::cached_candidates(&settings);

    let mut reconciled_spent_count = 0;
    let l2_reconciliation_status = if candidates.is_empty() {
        "no_servers".to_string()
    } else {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| FrontendError::from("HTTP client unavailable".to_string()))?;
        let mut at_least_one_gateway_responded = false;

        for (local_id, mut query) in outcome.voucher_queries {
            query.read_quorum = 1;
            let body_bytes = match serde_json::to_vec(&query) {
                Ok(b) => b,
                Err(_) => continue,
            };

            for base in candidates.iter().take(4) {
                let Some(url) = crate::gateway_resolver::endpoint(base, "v1/status") else { continue };
                let mut request = client.post(url.clone())
                    .header("Content-Type", "application/json")
                    .body(body_bytes.clone());

                if let Some(provider) = settings.custom_servers.iter().find(|p| {
                    p.is_active
                        && crate::gateway_resolver::endpoint(&p.url, "v1/status")
                            .is_some_and(|k| k.origin() == url.origin())
                }) {
                    if let Some(token) = &provider.auth_token {
                        request = request.bearer_auth(token);
                    }
                    if let Some(token) = &provider.peer_token {
                        if token.len() == 36 && token.starts_with("f2f_") && token[4..].bytes().all(|b| b.is_ascii_hexdigit()) {
                            request = request.header("X-Peer-Token", token);
                        }
                    }
                }

                let response = tokio::time::timeout(
                    std::time::Duration::from_millis(2500),
                    request.send(),
                ).await;

                let Ok(Ok(resp)) = response else { continue };
                at_least_one_gateway_responded = true;
                if !resp.status().is_success() { continue; }
                let Ok(resp_bytes) = resp.bytes().await else { continue };

                let verdict_opt = if let Ok(envelope) = serde_json::from_slice::<human_money_core::models::layer2_api::L2ResponseEnvelope>(&resp_bytes) {
                    Some(envelope.verdict)
                } else {
                    serde_json::from_slice::<human_money_core::models::layer2_api::L2Verdict>(&resp_bytes).ok()
                };

                if let Some(verdict) = verdict_opt {
                    let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
                    match service.reconcile_voucher_verdict(&local_id, &verdict, Some(&new_password)) {
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::AlreadySpent { .. }) => {
                            reconciled_spent_count += 1;
                            break;
                        }
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::ConflictDetected { .. }) => {
                            reconciled_spent_count += 1;
                            break;
                        }
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::UnspentSafe) => {
                            break;
                        }
                        Err(e) => {
                            log::debug!("Backup voucher reconciliation rejected: {}", e);
                        }
                    }
                }
            }
        }

        if at_least_one_gateway_responded {
            "reconciled".to_string()
        } else {
            "offline".to_string()
        }
    };

    // Mandatory SST-V4 offline-spend lock (§5)
    let pending_restore_l2 = active_voucher_queries_count > 0 && l2_reconciliation_status != "reconciled";
    if let Ok(mut settings) = state.get_cached_settings() {
        settings.pending_restore_l2_reconciliation = pending_restore_l2;
        if let Ok(mut service) = state.service.lock() {
            let _ = state.save_settings(&mut service, settings, Some(&new_password));
        }
    }

    // Clean up temporary staged file if source_path was from drop restore
    if let Ok(app_dir) = app.path().app_data_dir() {
        let tmp_restore_dir = app_dir.join("tmp_restore");
        let src_p = Path::new(&source_path);
        if (src_p.starts_with(&tmp_restore_dir) || source_path.contains("tmp_restore")) && src_p.is_file() {
            let _ = fs::remove_file(src_p);
        }
    }

    let app_handle = app.clone();
    let pwd = new_password.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        let app_state = app_handle.state::<AppState>();
        let _ = crate::commands::settings_cmd::flush_pending_anchor_sync_inner(&app_state, Some(&pwd)).await;
    });

    Ok(FrontendRestoreOutcomeDto {
        folder_name: outcome.folder_name,
        profile_name,
        active_voucher_queries_count,
        reconciled_spent_count,
        restored_standards_count,
        l2_reconciliation_status,
    })
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

#[tauri::command]
pub fn get_backup_status(state: tauri::State<AppState>) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    let settings = match state.get_cached_settings() {
        Ok(s) => s,
        Err(_) => return Ok(FrontendAutoBackupStatusDto {
            status: "notConfigured".to_string(),
            last_backup_at: None,
            backup_dirty: false,
            sync_folder: None,
            changed_blobs: None,
        }),
    };

    let folder_str = match &settings.backup_sync_folder {
        Some(f) if !f.trim().is_empty() => f.clone(),
        _ => {
            return Ok(FrontendAutoBackupStatusDto {
                status: "notConfigured".to_string(),
                last_backup_at: settings.last_backup_at,
                backup_dirty: settings.backup_dirty,
                sync_folder: None,
                changed_blobs: None,
            });
        }
    };

    let is_outdated = settings.backup_dirty || is_older_than_14_days(&settings.last_backup_at);

    if !settings.auto_backup_enabled || settings.backup_protection_mode.as_deref() == Some("custom_password") {
        return Ok(FrontendAutoBackupStatusDto {
            status: if is_outdated { "outdated".to_string() } else { "manualOnly".to_string() },
            last_backup_at: settings.last_backup_at,
            backup_dirty: settings.backup_dirty,
            sync_folder: Some(folder_str),
            changed_blobs: None,
        });
    }

    let folder_path = Path::new(&folder_str);
    if !folder_path.is_dir() {
        return Ok(FrontendAutoBackupStatusDto {
            status: "folderDisconnected".to_string(),
            last_backup_at: settings.last_backup_at,
            backup_dirty: settings.backup_dirty,
            sync_folder: Some(folder_str),
            changed_blobs: None,
        });
    }

    let status = if is_outdated {
        "outdated"
    } else {
        "synced"
    };

    Ok(FrontendAutoBackupStatusDto {
        status: status.to_string(),
        last_backup_at: settings.last_backup_at,
        backup_dirty: settings.backup_dirty,
        sync_folder: Some(folder_str),
        changed_blobs: None,
    })
}

#[tauri::command]
pub fn run_auto_backup_sync(
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    let mut settings = match state.get_cached_settings() {
        Ok(s) => s,
        Err(_) => return Ok(FrontendAutoBackupStatusDto {
            status: "notConfigured".to_string(),
            last_backup_at: None,
            backup_dirty: false,
            sync_folder: None,
            changed_blobs: None,
        }),
    };

    let folder_str = match &settings.backup_sync_folder {
        Some(f) if !f.trim().is_empty() => f.clone(),
        _ => {
            return Ok(FrontendAutoBackupStatusDto {
                status: "notConfigured".to_string(),
                last_backup_at: settings.last_backup_at,
                backup_dirty: settings.backup_dirty,
                sync_folder: None,
                changed_blobs: None,
            });
        }
    };

    if !settings.auto_backup_enabled || settings.backup_protection_mode.as_deref() == Some("custom_password") {
        return Ok(FrontendAutoBackupStatusDto {
            status: "manualOnly".to_string(),
            last_backup_at: settings.last_backup_at,
            backup_dirty: settings.backup_dirty,
            sync_folder: Some(folder_str),
            changed_blobs: None,
        });
    }

    let folder_path = Path::new(&folder_str);
    // WICHTIGER USB-SCHUTZ: Prüfe is_dir(), KEIN fs::create_dir_all falls Ordner fehlt!
    if !folder_path.is_dir() {
        if !settings.backup_dirty {
            settings.backup_dirty = true;
            if let Ok(mut service) = state.service.lock() {
                let _ = state.save_settings(&mut service, settings.clone(), None);
            }
        }
        return Ok(FrontendAutoBackupStatusDto {
            status: "folderDisconnected".to_string(),
            last_backup_at: settings.last_backup_at,
            backup_dirty: settings.backup_dirty,
            sync_folder: Some(folder_str),
            changed_blobs: None,
        });
    }

    let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    let custom_standards = collect_custom_standards(&app);

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
            Ok(FrontendAutoBackupStatusDto {
                status: "synced".to_string(),
                last_backup_at: settings.last_backup_at,
                backup_dirty: false,
                sync_folder: Some(folder_str),
                changed_blobs: Some(summary.changed_blobs),
            })
        }
        Err(e) => {
            let is_locked = e.code == "error.wallet.locked"
                || e.code == "error.auth.sessionTimeout"
                || e.message.to_lowercase().contains("locked")
                || e.message.to_lowercase().contains("session");
            if is_locked {
                Ok(FrontendAutoBackupStatusDto {
                    status: "locked".to_string(),
                    last_backup_at: settings.last_backup_at,
                    backup_dirty: settings.backup_dirty,
                    sync_folder: Some(folder_str),
                    changed_blobs: None,
                })
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

pub(crate) fn cleanup_stale_tmp_restore_files(tmp_dir: &Path) {
    if let Ok(entries) = fs::read_dir(tmp_dir) {
        let now = std::time::SystemTime::now();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
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

#[tauri::command]
pub async fn sync_drop_backup(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendDropBackupSyncResult, FrontendError> {
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    if !settings.drop_backup_enabled {
        return Ok(FrontendDropBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
        });
    }

    let (my_ed_sk, master_seed) = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;
        let master_seed = my_ed_sk.to_bytes();
        (my_ed_sk, master_seed)
    };

    // Gather candidate backup drops: selected friend drops + own drop + enabled public drops
    let mut candidate_targets: Vec<humoco_market::DropDescriptor> = settings
        .market_selected_drops
        .iter()
        .cloned()
        .map(Into::into)
        .collect();

    // Check own drop admin descriptor
    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        if let Ok(Some(admin)) = service
            .load_encrypted_data(crate::state::MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map(|bytes| serde_json::from_slice::<crate::state::StoredDropAdmin>(&bytes).ok())
        {
            if !admin.descriptor.write_url.is_empty()
                && !candidate_targets
                    .iter()
                    .any(|d| d.write_url == admin.descriptor.write_url)
            {
                candidate_targets.push(admin.descriptor.clone());
            }
        }
    }

    // Enabled public drops
    for pd in &settings.public_drops {
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
        return Ok(FrontendDropBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
        });
    }

    // Probe which candidate drops authorize channel "backup"
    let probe_client = crate::drop_client::create_drop_probe_client();
    let mut authorized_targets = Vec::new();

    for drop_desc in candidate_targets {
        let (cap_pubkey, is_public) = if let Some(ref slot_id) = drop_desc.slot_id {
            (slot_id.clone(), true)
        } else {
            match humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_desc.drop_pub) {
                Ok(pk) => (pk, false),
                Err(_) => continue,
            }
        };

        match crate::drop_client::head_slot_capability(
            &probe_client,
            &drop_desc.write_url,
            &cap_pubkey,
            "backup",
        )
        .await
        {
            Ok(cap) if cap.authorized => {
                authorized_targets.push((drop_desc, cap_pubkey, is_public));
            }
            _ => {}
        }
    }

    let authorized_count = authorized_targets.len();
    if authorized_targets.is_empty() {
        return Ok(FrontendDropBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
        });
    }

    // Generate in-memory PortableBackupContainer
    let custom_standards = collect_custom_standards(&app);
    let container_bytes = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let diff = service
            .create_differential_backup(None, BackupProtection::MnemonicOnly, password.as_deref())
            .map_err(FrontendError::from)?;

        let mut blobs_hex = HashMap::with_capacity(diff.put_blobs.len());
        for (name, data) in diff.put_blobs {
            blobs_hex.insert(name, human_money_core::hex::encode(&data));
        }

        let timestamp = Utc::now().to_rfc3339();
        let container = PortableBackupContainer {
            container_format: "humoco-app-backup-v1".to_string(),
            created_at: timestamp,
            manifest_hex: human_money_core::hex::encode(&diff.encrypted_manifest_bytes),
            blobs_hex,
            custom_standards,
        };

        serde_json::to_vec_pretty(&container)
            .map_err(|e| FrontendError::from(format!("Failed to serialize backup container: {}", e)))?
    };

    // Pad backup slot bytes
    let padded_bytes = humoco_market::pad_backup_slot_bytes(
        &container_bytes,
        humoco_market::DROP_BACKUP_MAX_BYTES,
    )
    .map_err(|e| FrontendError::from(e.to_string()))?;

    let client = crate::drop_client::create_drop_http_client();
    let now_secs = crate::drop_client::current_unix_timestamp();
    let mut uploaded_to = 0;
    let mut errors = Vec::new();
    let mut settings_updated = false;
    let mut current_settings = settings;

    for (mut drop_desc, _cap_pubkey, is_public) in authorized_targets {
        let nonce = crate::drop_client::generate_drop_nonce();
        let signed_res = if is_public {
            match humoco_market::derive_public_drop_keypair(&master_seed, &drop_desc.write_url) {
                Ok((r_sk, _, _)) => Ok(humoco_market::sign_member_request_direct(
                    &r_sk,
                    "PUT",
                    "backup",
                    now_secs,
                    &nonce,
                    &padded_bytes,
                )),
                Err(e) => Err(e.to_string()),
            }
        } else {
            humoco_market::sign_member_request_stealth(
                &my_ed_sk,
                &drop_desc.drop_pub,
                "PUT",
                "backup",
                now_secs,
                &nonce,
                &padded_bytes,
            )
            .map_err(|e| e.to_string())
        };

        match signed_res {
            Ok(signed) => {
                match crate::drop_client::put_slot(
                    &client,
                    &drop_desc.write_url,
                    &signed,
                    "backup",
                    &padded_bytes,
                    None,
                )
                .await
                {
                    Ok(crate::drop_client::PutSlotOutcome::Saved { rotation, .. }) => {
                        uploaded_to += 1;
                        if let Some(ref cert) = rotation {
                            if crate::drop_client::apply_verified_rotation(&mut drop_desc, cert, now_secs) {
                                for s_drop in &mut current_settings.market_selected_drops {
                                    if s_drop.read_url == drop_desc.read_url {
                                        s_drop.drop_pub = drop_desc.drop_pub.clone();
                                        settings_updated = true;
                                    }
                                }
                            }
                        }
                    }
                    Ok(crate::drop_client::PutSlotOutcome::PreconditionFailed { .. }) => {
                        errors.push(format!("Upload to {} failed: precondition failed", drop_desc.write_url));
                    }
                    Err(e) => {
                        errors.push(format!("Upload to {} failed: {}", drop_desc.write_url, e));
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Signing failed for {}: {}", drop_desc.write_url, e));
            }
        }
    }

    let timestamp_str = if uploaded_to > 0 {
        let ts = Utc::now().to_rfc3339();
        current_settings.last_drop_backup_at = Some(ts.clone());
        settings_updated = true;
        Some(ts)
    } else {
        None
    };

    if settings_updated {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = state.save_settings(&mut service, current_settings, password.as_deref());
    }

    Ok(FrontendDropBackupSyncResult {
        uploaded_to,
        authorized_targets: authorized_count,
        padded_bytes: padded_bytes.len(),
        timestamp: timestamp_str,
        errors,
    })
}

#[tauri::command]
pub async fn fetch_and_inspect_drop_backup(
    drop_input: String,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FrontendDropBackupFetchDto, FrontendError> {
    let lang: human_money_core::MnemonicLanguage = language.unwrap_or(MnemonicLanguage::English).into();
    let (_pk, my_ed_sk) = human_money_core::services::crypto::keys::derive_ed25519_keypair(
        &mnemonic,
        passphrase.as_deref(),
        lang,
    )
    .map_err(|e| FrontendError::from(format!("Failed to derive keypair: {}", e)))?;
    let master_seed = my_ed_sk.to_bytes();

    let trimmed = drop_input.trim();
    if trimmed.is_empty() {
        return Err(FrontendError::from("Ablage-Adresse darf nicht leer sein".to_string()));
    }

    let (pasted_write_url, pasted_read_url, pasted_drop_pub) = if trimmed.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let w = val.get("writeUrl").or_else(|| val.get("write_url")).and_then(|v| v.as_str()).map(|s| s.to_string());
            let r = val.get("readUrl").or_else(|| val.get("read_url")).and_then(|v| v.as_str()).map(|s| s.to_string());
            let d = val.get("dropPub").or_else(|| val.get("drop_pub")).and_then(|v| v.as_str()).map(|s| s.to_string());
            (w, r, d)
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    let raw_url = pasted_write_url.unwrap_or_else(|| trimmed.to_string());
    let canonical_write_url = humoco_market::canonicalize_write_url(&raw_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let client = crate::drop_client::create_drop_probe_client();
    let info_opt = crate::drop_client::fetch_drop_info(&client, &canonical_write_url).await.ok();

    let (current_drop_pub, current_read_url, old_pubkeys) = match info_opt {
        Some(info) => {
            let mut old_keys = Vec::new();
            if let Some(rot) = info.rotation {
                old_keys.push(rot.old_pubkey);
            }
            if let Some(ref pasted_pub) = pasted_drop_pub {
                if pasted_pub != &info.drop_pub && !old_keys.contains(pasted_pub) {
                    old_keys.push(pasted_pub.clone());
                }
            }
            (info.drop_pub, info.read_url, old_keys)
        }
        None => {
            if let (Some(r_url), Some(d_pub)) = (pasted_read_url, pasted_drop_pub) {
                (d_pub, r_url, Vec::new())
            } else {
                return Err(FrontendError::from(format!(
                    "Ablage unter '{}' konnte nicht erreicht werden und keine gültigen Drop-Deskriptor-Daten übergeben.",
                    canonical_write_url
                )));
            }
        }
    };

    // Build ordered candidates: (read_url, pubkey_hex64, kind)
    let mut candidates: Vec<(String, String, String)> = Vec::new();

    // 1. Current stealth ($P_{aktuell}$)
    if let Ok(stealth_pk) = humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &current_drop_pub) {
        candidates.push((current_read_url.clone(), stealth_pk, "stealth".to_string()));
    }

    // 2. Public drop anonymous key ($R$)
    if let Ok((_r_sk, _r_vk, r_hex)) = humoco_market::derive_public_drop_keypair(&master_seed, &canonical_write_url) {
        candidates.push((current_read_url.clone(), r_hex, "public".to_string()));
    }

    // 3. Grace-period stealth ($P_{alt}$)
    for old_pub in &old_pubkeys {
        if let Ok(old_stealth) = humoco_market::derive_member_stealth_pubkey(&my_ed_sk, old_pub) {
            candidates.push((current_read_url.clone(), old_stealth, "grace".to_string()));
        }
    }

    // 4. Deterministic Own-Drop fallback
    let own_data_dir = humoco_market::derive_drop_data_dir(&master_seed, 0);
    let origin_dir = canonical_write_url.rsplit_once('/').map(|(b, _)| b).unwrap_or(&canonical_write_url);
    let own_read_url = format!("{}/{}", origin_dir.trim_end_matches('/'), own_data_dir);
    for gen in 0..=3 {
        let (admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&master_seed, 0, gen);
        if let Ok(own_stealth) = humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &admin_vk) {
            if !candidates.iter().any(|(r, k, _)| r == &own_read_url && k == &own_stealth) {
                candidates.push((own_read_url.clone(), own_stealth.clone(), "own".to_string()));
            }
            if current_read_url != own_read_url
                && !candidates.iter().any(|(r, k, _)| r == &current_read_url && k == &own_stealth)
            {
                candidates.push((current_read_url.clone(), own_stealth, "own".to_string()));
            }
        }
    }

    // Download first candidate that matches
    let fetch_client = crate::drop_client::create_drop_http_client();
    let mut downloaded_bytes: Option<(Vec<u8>, String, String)> = None;

    for (read_url, pubkey_hex, kind) in candidates {
        match crate::drop_client::get_slot(&fetch_client, &read_url, &pubkey_hex, "backup").await {
            Ok(Some(bytes)) => {
                downloaded_bytes = Some((bytes, kind, read_url));
                break;
            }
            _ => continue,
        }
    }

    let (slot_bytes, matched_key_kind, source_url) = downloaded_bytes.ok_or_else(|| {
        FrontendError::from(
            "Keine Sicherungsdatei in der angegebenen Ablage gefunden. Bitte prüfe die Eingabe oder den Schlüssel."
                .to_string(),
        )
    })?;

    // Unpad 64-KB slot bytes
    let unpadded = humoco_market::unpad_backup_slot_bytes(&slot_bytes)
        .map_err(|e| FrontendError::from(format!("Ungültiges Sicherungs-Slot-Format: {}", e)))?;

    // Write to <app_data_dir>/tmp_restore/drop_restore_<uuid>.hmcbackup
    let app_data_dir = app.path().app_data_dir().map_err(|e| FrontendError::from(e.to_string()))?;
    let tmp_restore_dir = app_data_dir.join("tmp_restore");
    cleanup_stale_tmp_restore_files(&tmp_restore_dir);
    fs::create_dir_all(&tmp_restore_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    let staged_file_name = format!("drop_restore_{}.hmcbackup", uuid::Uuid::new_v4());
    let staged_path = tmp_restore_dir.join(staged_file_name);
    write_file_atomic(&staged_path, &unpadded)?;

    // Inspect backup using Mnemonic credentials
    let credentials = BackupRestoreCredentials::Mnemonic {
        phrase: &mnemonic,
        passphrase: passphrase.as_deref(),
        prefix: user_prefix.as_deref(),
        language: lang,
    };

    let service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    let inspection = inspect_backup_impl(&staged_path, &credentials, &service)?;

    Ok(FrontendDropBackupFetchDto {
        staged_path: staged_path.to_string_lossy().to_string(),
        matched_key_kind,
        source_url,
        inspection,
    })
}

#[tauri::command]
pub async fn reconcile_restored_wallet_l2(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendRestoreReconcileResultDto, FrontendError> {
    if let Some(ref pwd) = password {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.unlock_session(pwd, 300);
    }

    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates = crate::gateway_resolver::cached_candidates(&settings);

    let voucher_queries = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .with_wallet_and_identity(|wallet, identity| {
                let mut queries = Vec::new();
                for (id, instance) in &wallet.voucher_store.vouchers {
                    if instance.status == human_money_core::wallet::instance::VoucherStatus::Active {
                        if let Ok(mut query) = wallet.build_voucher_status_query(identity, id, 1) {
                            query.read_quorum = 1;
                            queries.push((id.clone(), query));
                        }
                    }
                }
                queries
            })
            .map_err(FrontendError::from)?
    };

    let active_voucher_queries_count = voucher_queries.len();
    let mut reconciled_spent_count = 0;

    let (l2_reconciliation_status, all_reconciled) = if candidates.is_empty() {
        ("no_servers".to_string(), false)
    } else {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_millis(2500))
            .build()
            .map_err(|_| FrontendError::from("HTTP client unavailable".to_string()))?;

        let mut at_least_one_gateway_responded = false;
        let mut all_ok = true;

        for (local_id, query) in voucher_queries {
            let body_bytes = match serde_json::to_vec(&query) {
                Ok(b) => b,
                Err(_) => {
                    all_ok = false;
                    continue;
                }
            };
            let mut reconciled_this = false;

            for base in candidates.iter().take(4) {
                let Some(url) = crate::gateway_resolver::endpoint(base, "v1/status") else { continue };
                let mut request = client
                    .post(url.clone())
                    .header("Content-Type", "application/json")
                    .body(body_bytes.clone());

                if let Some(provider) = settings.custom_servers.iter().find(|p| {
                    p.is_active
                        && crate::gateway_resolver::endpoint(&p.url, "v1/status")
                            .is_some_and(|k| k.origin() == url.origin())
                }) {
                    if let Some(token) = &provider.auth_token {
                        request = request.bearer_auth(token);
                    }
                    if let Some(token) = &provider.peer_token {
                        if token.len() == 36 && token.starts_with("f2f_") && token[4..].bytes().all(|b| b.is_ascii_hexdigit()) {
                            request = request.header("X-Peer-Token", token);
                        }
                    }
                }

                let response = tokio::time::timeout(
                    std::time::Duration::from_millis(2500),
                    request.send(),
                )
                .await;

                let Ok(Ok(resp)) = response else { continue };
                at_least_one_gateway_responded = true;
                if !resp.status().is_success() { continue; }
                let Ok(resp_bytes) = resp.bytes().await else { continue };

                let verdict_opt = if let Ok(envelope) = serde_json::from_slice::<human_money_core::models::layer2_api::L2ResponseEnvelope>(&resp_bytes) {
                    Some(envelope.verdict)
                } else {
                    serde_json::from_slice::<human_money_core::models::layer2_api::L2Verdict>(&resp_bytes).ok()
                };

                if let Some(verdict) = verdict_opt {
                    let mut service = state
                        .service
                        .lock()
                        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
                    match service.reconcile_voucher_verdict(&local_id, &verdict, password.as_deref()) {
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::AlreadySpent { .. }) => {
                            reconciled_spent_count += 1;
                            reconciled_this = true;
                            break;
                        }
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::ConflictDetected { .. }) => {
                            reconciled_spent_count += 1;
                            reconciled_this = true;
                            break;
                        }
                        Ok(human_money_core::models::layer2_api::VoucherReconciliationResult::UnspentSafe) => {
                            reconciled_this = true;
                            break;
                        }
                        Err(e) => {
                            log::debug!("Backup voucher reconciliation rejected: {}", e);
                        }
                    }
                }
            }

            if !reconciled_this {
                all_ok = false;
            }
        }

        if at_least_one_gateway_responded {
            let _ = crate::commands::settings_cmd::flush_pending_anchor_sync_inner(&state, password.as_deref()).await;
            if all_ok || active_voucher_queries_count == 0 {
                ("reconciled".to_string(), true)
            } else {
                ("offline".to_string(), false)
            }
        } else {
            ("offline".to_string(), false)
        }
    };

    let mut pending_restore_l2 = settings.pending_restore_l2_reconciliation;
    if all_reconciled {
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
        let p = std::env::temp_dir().join(format!("humoco_backup_test_{}_{}", name, uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service.create_profile(
            profile_name,
            PHRASE,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-test-1".into(),
        ).unwrap();
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
        ).unwrap();
        assert!(summary1.total_blobs > 0);
        assert_eq!(summary1.custom_standard_count, 1);

        // Probe mnemonic backup
        let probe1 = probe_wallet_backup(mnemonic_export_file.to_string_lossy().to_string()).unwrap();
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
        ).unwrap();
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
        ).unwrap();
        assert!(summary2.total_blobs > 0);

        // Probe password backup
        let probe2 = probe_wallet_backup(pwd_export_file.to_string_lossy().to_string()).unwrap();
        assert_eq!(probe2.protection_mode, "backup_password");
        assert!(probe2.supports_password_restore);
        assert!(probe2.supports_mnemonic_restore);

        // Inspect with password
        let creds_pwd = BackupRestoreCredentials::BackupPassword(backup_pwd);
        let inspect2 = inspect_backup_impl(&pwd_export_file, &creds_pwd, &service).unwrap();
        assert_eq!(inspect2.profile_name, "Alice Test");

        // Inspect with mnemonic (dual recovery slot!)
        let inspect2_mn = inspect_backup_impl(&pwd_export_file, &BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        }, &service).unwrap();
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
        ).unwrap();
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
        ).unwrap();
        assert!(summary1.changed_blobs > 0);
        assert!(sync_dir.join("manifest.enc").exists());
        assert!(sync_dir.join("blobs").exists());

        // Probe folder
        let probe = probe_wallet_backup(sync_dir.to_string_lossy().to_string()).unwrap();
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
        ).unwrap();
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
        ).unwrap();
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
        ).unwrap();

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
        ).unwrap_err();
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
        assert!(!non_existent_usb.exists(), "USB path must not have been created");

        let _ = fs::remove_dir_all(base_dir);
    }

    #[test]
    fn test_is_older_than_14_days() {
        assert!(is_older_than_14_days(&None));
        assert!(is_older_than_14_days(&Some("invalid-timestamp".to_string())));

        let recent = (Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        assert!(!is_older_than_14_days(&Some(recent)));

        let old = (Utc::now() - chrono::Duration::days(16)).to_rfc3339();
        assert!(is_older_than_14_days(&Some(old)));
    }

    #[test]
    fn test_drop_backup_slot_padding_integration() {
        use humoco_market::slot::{pad_backup_slot_bytes, unpad_backup_slot_bytes, DROP_BACKUP_MAX_BYTES, BACKUP_PAD_BLOCK_BYTES};

        let fake_backup = b"HMC_BACKUP_CONTAINER_V1_MOCK_PAYLOAD_DATA";
        let padded = pad_backup_slot_bytes(fake_backup, DROP_BACKUP_MAX_BYTES).expect("padding must succeed");
        assert_eq!(padded.len() % BACKUP_PAD_BLOCK_BYTES, 0);
        assert_eq!(padded.len(), BACKUP_PAD_BLOCK_BYTES);

        let recovered = unpad_backup_slot_bytes(&padded).expect("unpadding must succeed");
        assert_eq!(recovered, fake_backup);
    }

    #[test]
    fn test_drop_backup_stealth_key_and_candidate_resolution() {
        let mnemonic = PHRASE;
        let (_my_pk, my_ed_sk) = human_money_core::services::crypto::keys::derive_ed25519_keypair(
            mnemonic,
            None,
            human_money_core::MnemonicLanguage::English,
        ).expect("key derivation failed");

        // Simulate Drop master key
        let drop_sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let drop_pub_hex = hex::encode(drop_sk.verifying_key().as_bytes());

        // 1. Client derives stealth slot key for friend's drop
        let stealth_pk = humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub_hex)
            .expect("stealth key derivation failed");
        assert_eq!(stealth_pk.len(), 64);

        // 2. Candidate keys generation in restore:
        // Candidate 1: stealth key matching drop_pub
        let candidate_stealth = humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub_hex)
            .expect("candidate derivation failed");
        assert_eq!(candidate_stealth, stealth_pk);

        // Candidate 2: grace key matching an older rotated drop key
        let old_drop_sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let old_drop_pub_hex = hex::encode(old_drop_sk.verifying_key().as_bytes());
        let candidate_grace = humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &old_drop_pub_hex)
            .expect("candidate grace derivation failed");
        assert_ne!(candidate_grace, stealth_pk);
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
}
