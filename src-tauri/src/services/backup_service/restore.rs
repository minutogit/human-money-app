use super::*;
use crate::{
    models::{
        FrontendBackupProbeDto, FrontendRestoreOutcomeDto, FrontendRestoreReconcileResultDto,
        MnemonicLanguage,
    },
    services::fs_helpers::{
        is_canonical_child_of, record_profile_metadata_in_dir,
        remove_profile_metadata_entry_in_dir, restore_custom_standards_to_dir, write_file_atomic,
        TempDirGuard,
    },
    AppState,
};
use human_money_core::{
    app_service::{AppService, ProfileInfo},
    models::{
        backup::EncryptedBackupManifest,
        seal::{LocalSealRecord, SyncStatus, WalletSeal},
    },
    storage::{seal_service::SealService, WalletLockGuard},
    AuthMethod, FileStorage, UserIdentity, Wallet,
};

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
    let path_meta = fs::symlink_metadata(path)
        .map_err(|_| FrontendError::from(format!("Backup path not found: {}", source_path)))?;
    let (source_kind, manifest_bytes, blob_count, custom_standard_count) = if path_meta
        .file_type()
        .is_file()
    {
        let bytes = read_regular_file_bounded(path, MAX_DROPPED_BACKUP_BYTES)?;
        let container = parse_portable_backup_container(&bytes)?;
        let manifest_bytes = human_money_core::hex::decode(&container.manifest_hex)
            .map_err(|e| FrontendError::from(format!("Invalid manifest hex encoding: {}", e)))?;
        (
            "file",
            manifest_bytes,
            container.blobs_hex.len(),
            container.custom_standards.len(),
        )
    } else if path_meta.file_type().is_dir() {
        let manifest_path = path.join("manifest.enc");
        if fs::symlink_metadata(&manifest_path).is_err() {
            return Err(FrontendError::from(format!(
                "Directory does not contain a valid backup: manifest.enc missing at {}",
                manifest_path.display()
            )));
        }
        let manifest_bytes = read_regular_file_bounded(&manifest_path, MAX_DROPPED_BACKUP_BYTES)?;
        let blob_count = fs::read_dir(path.join("blobs"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let custom_standard_count = fs::read_dir(path.join("standards"))
            .map(|d| d.flatten().filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        ("folder", manifest_bytes, blob_count, custom_standard_count)
    } else {
        return Err(FrontendError::from(format!(
            "Backup path is not a regular file or directory: {}",
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

pub(super) fn replace_local_profile_with_restored(
    wallet_data_dir: &Path,
    old_folder: &str,
    new_folder: &str,
    restored_identity: &UserIdentity,
    current_profile_name: String,
) -> Result<String, FrontendError> {
    if old_folder == new_folder {
        return Ok(current_profile_name);
    }
    if !is_safe_folder_name(old_folder) {
        return Err(FrontendError::from(format!(
            "Invalid profile folder name to replace: {}",
            old_folder
        )));
    }

    let old_dir = wallet_data_dir.join(old_folder);
    let old_storage = FileStorage::new(old_dir.clone());
    let old_wallet_lock =
        human_money_core::storage::WalletLockGuard::new(&old_storage).map_err(|e| {
            FrontendError::from(format!(
                "Refusing to replace folder '{}' because wallet lock could not be acquired: {}",
                old_folder, e
            ))
        })?;
    let seal_matches = old_storage
        .load_seal(&AuthMethod::RecoveryIdentity(restored_identity))
        .ok()
        .flatten()
        .is_some_and(|rec| rec.seal.payload.user_id == restored_identity.user_id);
    let integrity_matches =
        human_money_core::crypto::get_prefix_from_user_id(&restored_identity.user_id).is_none()
            && old_storage
                .load_integrity()
                .ok()
                .flatten()
                .is_some_and(|ir| {
                    verify_integrity_record_signature(&ir, &restored_identity.user_id)
                });

    if !(seal_matches || integrity_matches) {
        return Err(FrontendError::from(format!(
            "Refusing to replace folder '{}' because cryptographic ownership could not be verified",
            old_folder
        )));
    }

    let profiles_path = wallet_data_dir.join("profiles.json");
    let profiles_bytes = fs::read(&profiles_path).map_err(|e| {
        FrontendError::from(format!(
            "Failed to read profiles.json for replacement: {}",
            e
        ))
    })?;
    let mut profiles =
        serde_json::from_slice::<Vec<ProfileInfo>>(&profiles_bytes).map_err(|e| {
            FrontendError::from(format!(
                "Failed to parse profiles.json for replacement: {}",
                e
            ))
        })?;

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

    let updated_json = serde_json::to_vec_pretty(&profiles).map_err(|e| {
        FrontendError::from(format!(
            "Failed to serialize profiles.json for replacement: {}",
            e
        ))
    })?;
    write_file_atomic(&profiles_path, &updated_json)?;

    #[cfg(not(windows))]
    {
        if let Err(e) = fs::remove_dir_all(&old_dir) {
            log::warn!(
                "Failed to remove replaced profile directory '{}': {}",
                old_folder,
                e
            );
        }
        drop(old_wallet_lock);
    }
    #[cfg(windows)]
    {
        if let Ok(entries) = fs::read_dir(&old_dir) {
            for entry in entries.flatten() {
                if entry.file_name() == ".wallet.transaction.lock" {
                    continue;
                }
                let path = entry.path();
                let _ = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
            }
        }
        drop(old_wallet_lock);
        if let Err(e) = fs::remove_dir_all(&old_dir) {
            log::warn!(
                "Failed to remove replaced profile directory '{}': {}",
                old_folder,
                e
            );
        }
    }
    if let Err(e) = remove_profile_metadata_entry_in_dir(wallet_data_dir, old_folder) {
        log::warn!(
            "Failed to remove replaced profile metadata entry '{}': {}",
            old_folder,
            e
        );
    }

    Ok(final_name)
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
) -> Result<(FrontendRestoreOutcomeDto, crate::session::SessionToken), FrontendError> {
    let mut replace_lock = replace_folder_name
        .map(|old_folder| state.acquire_profile_lock(old_folder))
        .transpose()?;

    let (restored_session, outcome, mut profile_name, custom_standards) = {
        let mut service = state.lock_service()?;
        let (outcome, profile_name, custom_standards) = restore_backup_impl(
            source_path,
            credentials,
            new_password,
            local_instance_id,
            &mut service,
        )?;
        let setup_res = (|| -> Result<_, FrontendError> {
            let pending_lock = if replace_folder_name == Some(outcome.folder_name.as_str()) {
                replace_lock
                    .take()
                    .unwrap_or_else(|| crate::state::PendingProfileLock::None)
            } else {
                state.acquire_profile_lock(&outcome.folder_name)?
            };

            let restored_identity = service
                .with_wallet_and_identity(|_, id| id.clone())
                .map_err(FrontendError::from)?;

            let resolved_name = if let Some(old_folder) = replace_folder_name {
                let name = replace_local_profile_with_restored(
                    wallet_data_dir,
                    old_folder,
                    &outcome.folder_name,
                    &restored_identity,
                    profile_name,
                )?;
                if let Some(app) = app_opt {
                    if let Ok(config_dir) = app.path().app_config_dir() {
                        let _ = crate::pin_keystore::remove_entry(&config_dir, old_folder);
                        let _ =
                            crate::pin_keystore::remove_entry(&config_dir, &outcome.folder_name);
                    }
                }
                if old_folder != outcome.folder_name {
                    state.release_profile_lock_if_matching(old_folder);
                }
                name
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

            state
                .initialize_profile_session(&mut service, new_password)
                .map_err(FrontendError::from)?;

            // Fail-closed SST-V4 offline-spend lock (§5): persist pending_restore_l2_reconciliation
            // BEFORE dropping `service` so no spend can slip through during `.await` or after a crash.
            let has_active_anchored = !outcome.voucher_ids_to_check.is_empty()
                || service
                    .with_wallet(|w| {
                        w.voucher_store.vouchers.values().any(|inst| {
                            inst.status == human_money_core::wallet::instance::VoucherStatus::Active
                                && inst.voucher.layer2_anchor.is_some()
                        })
                    })
                    .unwrap_or(true);
            if has_active_anchored {
                state
                    .mutate_internal_lifecycle(&mut service, Some(new_password), |lc| {
                        lc.pending_restore_l2_reconciliation = true;
                    })
                    .map_err(FrontendError::from)?;
            }

            let restored_session = state.session_token_for_service(&service)?;

            Ok((
                restored_session,
                outcome,
                resolved_name,
                custom_standards,
                pending_lock,
            ))
        })();
        match setup_res {
            Ok((restored_session, outcome, resolved_name, custom_standards, pending_lock)) => {
                state.commit_profile_lock(pending_lock);
                drop(service);
                (restored_session, outcome, resolved_name, custom_standards)
            }
            Err(err) => {
                state.rollback_failed_session(&mut service);
                return Err(err);
            }
        }
    };

    if profile_name.is_empty() {
        profile_name = "Restored Profile".to_string();
    }

    let restored_standards_count =
        restore_custom_standards_to_dir(standards_dir, &custom_standards)?;

    let filter_ids = (!outcome.voucher_ids_to_check.is_empty())
        .then_some(outcome.voucher_ids_to_check.as_slice());
    let summary = crate::services::l2_anchor_service::reconcile_vouchers_via_static_locks(
        state,
        &restored_session,
        filter_ids,
        Some(new_password),
    )
    .await?;

    let active_voucher_queries_count = summary.active_anchored_count;
    let reconciled_spent_count = summary.reconciled_spent_count;
    let l2_reconciliation_status = summary.status;

    // Mandatory SST-V4 offline-spend lock (§5): only clear after reconciliation resolves all active anchored vouchers.
    if active_voucher_queries_count == 0 || summary.all_resolved {
        let updated_lc = state.mutate_internal_lifecycle_for_session(
            &restored_session,
            Some(new_password),
            |lc| {
                if lc.pending_restore_l2_reconciliation {
                    lc.pending_restore_l2_reconciliation = false;
                    true
                } else {
                    false
                }
            },
        )?;
        if updated_lc.is_none() {
            return Err(FrontendError::from(
                "Restored wallet session changed before saving L2 reconciliation state".to_string(),
            ));
        }
    }

    // Clean up temporary staged file ONLY if source_path is strictly a canonical non-symlink child of tmp_restore_dir.
    if is_canonical_child_of(source_path, tmp_restore_dir) && source_path.is_file() {
        let _ = fs::remove_file(source_path);
    }

    if let Some(app) = app_opt {
        crate::services::common::spawn_pending_l2_reconciliation(
            app,
            restored_session.clone(),
            new_password,
        );
    }

    Ok((
        FrontendRestoreOutcomeDto {
            folder_name: outcome.folder_name,
            profile_name,
            active_voucher_queries_count,
            reconciled_spent_count,
            restored_standards_count,
            l2_reconciliation_status,
        },
        restored_session,
    ))
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

    let (outcome, _session) = finalize_restored_wallet_and_reconcile_l2(
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
    .await?;
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn recover_local_profile_with_mnemonic(
    target_folder_name: &str,
    phrase: &str,
    passphrase: Option<&str>,
    new_password: &str,
    core_lang: human_money_core::MnemonicLanguage,
    local_instance_id: String,
    wallet_data_dir: &Path,
    state: &AppState,
    app_opt: Option<&tauri::AppHandle>,
) -> Result<crate::session::SessionToken, FrontendError> {
    let pending_lock = state.acquire_profile_lock(target_folder_name)?;
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
    let session_res = (|| -> Result<crate::session::SessionToken, FrontendError> {
        state
            .initialize_profile_session(&mut service, new_password)
            .map_err(FrontendError::from)?;
        state.session_token_for_service(&service)
    })();
    match session_res {
        Ok(session) => {
            state.commit_profile_lock(pending_lock);
            drop(service);
            Ok(session)
        }
        Err(err) => {
            state.rollback_failed_session(&mut service);
            Err(err)
        }
    }
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

    let restored_session = if credential_mode == "mnemonic" {
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
        )?
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
            )?
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

            let mut pending_lock = state.acquire_profile_lock(target_folder_name)?;
            let mut service = state.lock_service()?;
            let was_already_held =
                matches!(pending_lock, crate::state::PendingProfileLock::AlreadyHeld);
            state.rollback_failed_session(&mut service);
            if was_already_held {
                pending_lock = state.acquire_profile_lock(target_folder_name)?;
            }

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

            let reset_identity = if local_identity.mnemonic_entropy.is_none()
                && backup_identity.mnemonic_entropy.is_some()
            {
                &backup_identity
            } else {
                &local_identity
            };
            Wallet::reset_password(&mut local_storage, reset_identity, new_password)
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

            let mut recent_seals = existing_seal
                .as_ref()
                .map(|r| r.recent_seals.clone())
                .unwrap_or_default();
            let preserved_lock_reason = existing_seal.as_ref().and_then(|r| r.lock_reason.clone());
            let mut new_record = LocalSealRecord {
                seal: recovered_seal.clone(),
                sync_status: SyncStatus::PendingUpload,
                lock_reason: preserved_lock_reason.clone(),
                recent_seals: Vec::new(),
            };
            new_record.recent_seals = std::mem::take(&mut recent_seals);
            if let Some(prev) = existing_seal.as_ref() {
                new_record.push_seal(prev.seal.clone());
            }
            new_record.push_seal(recovered_seal.clone());
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
            let session_res = (|| -> Result<crate::session::SessionToken, FrontendError> {
                state
                    .initialize_profile_session(&mut service, new_password)
                    .map_err(FrontendError::from)?;
                if preserved_lock_reason.is_some() {
                    state
                        .mutate_internal_lifecycle(&mut service, Some(new_password), |lc| {
                            lc.pending_restore_l2_reconciliation = true;
                        })
                        .map_err(FrontendError::from)?;
                }
                state.session_token_for_service(&service)
            })();
            match session_res {
                Ok(session) => {
                    state.commit_profile_lock(pending_lock);
                    drop(service);
                    session
                }
                Err(err) => {
                    state.rollback_failed_session(&mut service);
                    return Err(err);
                }
            }
        }
    } else {
        return Err(FrontendError::from(format!(
            "Invalid credential mode: {}",
            credential_mode
        )));
    };

    if let Some(app) = app_opt {
        crate::services::common::spawn_pending_l2_reconciliation(
            app,
            restored_session,
            new_password,
        );
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

pub(crate) async fn reconcile_restored_wallet_l2_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendRestoreReconcileResultDto, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let _backup_guard = state.backup_op_lock.lock().await;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let reconciled_ids_vec: Vec<String> = state.with_session_required(
        &session,
        "Active wallet session changed before restore reconciliation",
        |service| {
            service
                .with_wallet(|wallet| {
                    wallet
                        .voucher_store
                        .vouchers
                        .iter()
                        .filter(|(_, inst)| {
                            inst.status == human_money_core::wallet::instance::VoucherStatus::Active
                                && inst.voucher.layer2_anchor.is_some()
                        })
                        .map(|(id, _)| id.clone())
                        .collect()
                })
                .map_err(FrontendError::from)
        },
    )?;
    let reconciled_ids: std::collections::HashSet<String> =
        reconciled_ids_vec.iter().cloned().collect();

    let summary = crate::services::l2_anchor_service::reconcile_vouchers_via_static_locks(
        state,
        &session,
        Some(&reconciled_ids_vec),
        pw,
    )
    .await?;

    let active_voucher_queries_count = summary.active_anchored_count;
    let reconciled_spent_count = summary.reconciled_spent_count;
    let l2_reconciliation_status = summary.status;

    let pending_restore_l2 =
        state.with_session_required(
            &session,
            "Active wallet session changed during restore reconciliation",
            |service| {
                let mut lc = state
                    .get_or_load_internal_lifecycle(service, pw)
                    .map_err(FrontendError::from)?;
                let (fork_or_stale_alert, has_active_offline_vouchers) =
                crate::services::l2_anchor_service::has_fork_or_stale_alert_and_offline_vouchers(
                    service, &lc, pw,
                );
                let has_new_unreconciled_anchored = service
                    .with_wallet(|wallet| {
                        wallet.voucher_store.vouchers.iter().any(|(id, inst)| {
                            inst.status == human_money_core::wallet::instance::VoucherStatus::Active
                                && inst.voucher.layer2_anchor.is_some()
                                && !reconciled_ids.contains(id)
                        })
                    })
                    .unwrap_or(true);
                let can_clear = !(fork_or_stale_alert && has_active_offline_vouchers)
                    && (active_voucher_queries_count == 0 || summary.all_resolved)
                    && !has_new_unreconciled_anchored;
                if can_clear {
                    crate::services::l2_anchor_service::clear_fork_lock_for_service(service, pw)?;
                    lc.pending_restore_l2_reconciliation = false;
                    if lc.wallet_sync_alert.as_ref().is_some_and(|a| {
                        a.kind == "stale_local_backup" || a.kind == "fork_detected"
                    }) {
                        lc.wallet_sync_alert = None;
                    }
                    state
                        .save_internal_lifecycle(service, lc, pw)
                        .map_err(FrontendError::from)?;
                    Ok(false)
                } else {
                    Ok(lc.pending_restore_l2_reconciliation)
                }
            },
        )?;

    Ok(FrontendRestoreReconcileResultDto {
        active_voucher_queries_count,
        reconciled_spent_count,
        l2_reconciliation_status,
        pending_restore_l2_reconciliation: pending_restore_l2,
    })
}

pub(crate) fn dismiss_wallet_sync_alert_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    state
        .mutate_internal_lifecycle(&mut service, pw, |lc| {
            if let Some(ref alert) = lc.wallet_sync_alert {
                if alert.kind == "server_rollback" || alert.kind == "unexpected_data_loss" {
                    lc.wallet_sync_alert = None;
                }
            }
        })
        .map_err(FrontendError::from)?;
    Ok(())
}
