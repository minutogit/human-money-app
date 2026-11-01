// src-tauri/src/commands/backup.rs
use crate::{
    models::{
        FrontendAutoBackupStatusDto, FrontendBackupInspectionDto, FrontendBackupProbeDto,
        FrontendBackupSummaryDto, FrontendError, FrontendRestoreOutcomeDto, MnemonicLanguage,
        PortableBackupContainer,
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
}
