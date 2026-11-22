use super::*;
use crate::{
    models::{
        FrontendBackupInspectionDto, FrontendBackupSummaryDto, LocalProfileCollisionDto,
        MnemonicIdentityPreviewDto, MnemonicLanguage,
    },
    services::fs_helpers::{
        cleanup_stale_tmp_restore_files, collect_custom_standards, get_wallet_data_dir,
        load_profile_metadata_map_from_dir, write_file_atomic,
    },
    AppState,
};
use chrono::Utc;
use human_money_core::{
    app_service::{AppService, ProfileInfo},
    models::backup::EncryptedBackupManifest,
    AuthMethod, FileStorage, UserIdentity, Wallet,
};

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
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(crate::state::RequiredAccess::Full, password)?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let summary = export_backup_file_impl(
        Path::new(destination_path),
        protection,
        pw,
        &mut service,
        custom_standards,
    )?;

    state
        .mutate_internal_lifecycle(&mut service, pw, |lc| {
            lc.last_backup_at = Some(Utc::now().to_rfc3339());
            lc.backup_dirty = false;
        })
        .map_err(FrontendError::from)?;

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
        if !is_safe_filename_component(name) {
            return Err(FrontendError::from(format!(
                "Unsafe blob name in diff: {}",
                name
            )));
        }
        write_file_atomic(&blobs_dir.join(name), data)?;
    }

    for (id, content) in &custom_standards {
        if !is_safe_filename_component(id) {
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
        if is_safe_filename_component(del_name) {
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
    // H2: Changing backup destination folder / policy requires Full access even for encrypted_wallet mode.
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Full, password)?;
    let summary = sync_backup_folder_impl(
        Path::new(&folder_path),
        expected_mode,
        protection,
        password,
        force_full,
        &mut service,
        custom_standards,
    )?;

    state
        .mutate_security_policy(&mut service, password, |policy| {
            policy.backup_sync_folder = Some(folder_path);
            policy.backup_protection_mode = Some(protection_mode.as_str().to_string());
        })
        .map_err(FrontendError::from)?;
    state
        .mutate_internal_lifecycle(&mut service, password, |lc| {
            lc.last_backup_at = Some(Utc::now().to_rfc3339());
            lc.backup_dirty = false;
        })
        .map_err(FrontendError::from)?;

    Ok(summary)
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
    let source_meta = fs::symlink_metadata(source_path).map_err(|_| {
        FrontendError::from(format!("Backup path not found: {}", source_path.display()))
    })?;
    let (manifest_bytes, blob_count, custom_standard_count) = if source_meta.file_type().is_file() {
        let bytes = read_regular_file_bounded(source_path, MAX_DROPPED_BACKUP_BYTES)?;
        let container = parse_portable_backup_container(&bytes)?;
        let m_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        (
            m_bytes,
            container.blobs_hex.len(),
            container.custom_standards.len(),
        )
    } else if source_meta.file_type().is_dir() {
        let manifest_path = source_path.join("manifest.enc");
        let m_bytes = read_regular_file_bounded(&manifest_path, MAX_DROPPED_BACKUP_BYTES)?;
        let b_count = fs::read_dir(source_path.join("blobs"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let s_count = fs::read_dir(source_path.join("standards"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        (m_bytes, b_count, s_count)
    } else {
        return Err(FrontendError::from(format!(
            "Backup path is not a regular file or directory: {}",
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
