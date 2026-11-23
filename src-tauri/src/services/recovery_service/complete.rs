use super::storage::*;
use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

use human_money_core::models::backup::BackupRestoreCredentials;
use humoco_market::{recover_master_payload_from_shares, DropDescriptor, GuardianVaultEntry};

use crate::models::{FrontendError, RecoveryCompletionResultDto, RecoveryOwnerState};
use crate::services::{
    backup_service::{
        finalize_restored_wallet_and_reconcile_l2, find_local_profile_collision_impl,
        probe_sync_backup_impl, user_ids_from_metadata_dir,
    },
    fs_helpers::{
        is_canonical_child_of, is_safe_filename_component, record_profile_metadata_in_dir,
        write_file_atomic,
    },
    sync_engine::save_public_sync_impl,
};
use crate::state::AppState;

// ============================================================================
// Completion Logic
// ============================================================================

#[allow(clippy::too_many_arguments)]
pub async fn complete_social_recovery_impl(
    owner_did: String,
    rotation_epoch: u32,
    answers: [String; 3],
    bip39_passphrase: Option<String>,
    mode: String,
    new_password: String,
    profile_name: Option<String>,
    target_folder_name: Option<String>,
    local_instance_id: String,
    allow_empty_fallback: Option<bool>,
    replace_folder_name: Option<String>,
    session_file_path: &Path,
    state: &AppState,
    app_data_dir: &Path,
    wallet_data_dir: &Path,
    app_opt: Option<&tauri::AppHandle>,
) -> Result<RecoveryCompletionResultDto, FrontendError> {
    if mode != "restore_new_device" && mode != "reset_local_password" {
        return Err(FrontendError::from(format!(
            "Invalid recovery mode: {}",
            mode
        )));
    }
    if new_password.is_empty() {
        return Err(FrontendError::from(
            "New password cannot be empty".to_string(),
        ));
    }
    if local_instance_id.trim().is_empty() {
        return Err(FrontendError::from(
            "Device instance ID is required".to_string(),
        ));
    }
    let normalized_bip39_passphrase: Option<&str> =
        bip39_passphrase.as_deref().filter(|p| !p.is_empty());

    let _recovery_guard = state.recovery_op_lock.lock().await;

    let session = load_active_session_from_path(session_file_path)?.ok_or_else(|| {
        FrontendError::with_code(
            "error.recovery.noActiveSession",
            "No active recovery session found.",
        )
    })?;

    let matching_entries: Vec<GuardianVaultEntry> = session
        .replies
        .iter()
        .filter(|r| {
            is_same_account_id(&r.entry.owner_did, &owner_did)
                && r.entry.rotation_epoch == rotation_epoch
        })
        .map(|r| r.entry.clone())
        .collect();

    if matching_entries.is_empty() {
        return Err(FrontendError::from(
            "Keine passenden Freigaben für dieses Profil und diese Epoche gefunden.".to_string(),
        ));
    }

    let required_m = matching_entries[0].threshold_m;
    if matching_entries.len() < required_m as usize {
        return Err(FrontendError::from(format!(
            "Nicht genügend Freigaben gesammelt: {} von {} vorhanden.",
            matching_entries.len(),
            required_m
        )));
    }

    // NOTE: On error, session_file_path is NOT deleted so user can retry!
    // Offload heavy Argon2id derivation to the blocking threadpool.
    let kdf_entries = matching_entries;
    let kdf_answers = answers;
    let master_payload = tokio::task::spawn_blocking(move || {
        let ans_refs = [
            kdf_answers[0].as_str(),
            kdf_answers[1].as_str(),
            kdf_answers[2].as_str(),
        ];
        recover_master_payload_from_shares(&kdf_entries, &ans_refs)
    })
    .await
    .map_err(|e| FrontendError::from(format!("Recovery KDF task failed: {}", e)))?
    .map_err(|err| match err {
        humoco_market::MarketError::Crypto(ref msg)
            if msg.starts_with("AEAD master unsealing failed") =>
        {
            FrontendError::with_code(
                "error.recovery.wrongAnswers",
                format!(
                    "Deine Antworten stimmen noch nicht – bitte in Ruhe prüfen: {}",
                    err
                ),
            )
        }
        other => FrontendError::from(other),
    })?;

    if master_payload.has_bip39_passphrase && normalized_bip39_passphrase.is_none() {
        return Err(FrontendError::with_code(
            "error.recovery.bip39PassphraseRequired",
            "BIP39_PASSPHRASE_REQUIRED: Für dieses Profil wurde bei der Erstellung eine Passphrase (25. Wort) festgelegt.",
        ));
    }

    let lang = master_payload.language().map_err(FrontendError::from)?;
    let phrase = zeroize::Zeroizing::new(
        human_money_core::services::mnemonic::MnemonicProcessor::from_entropy(
            &master_payload.mnemonic_entropy,
            lang,
        )
        .map_err(|e| FrontendError::from(e.to_string()))?,
    );

    if mode == "reset_local_password" {
        let target_folder = target_folder_name.ok_or_else(|| {
            FrontendError::from(
                "target_folder_name ist erforderlich für lokalen Passwort-Reset.".to_string(),
            )
        })?;

        let pending_lock = state.acquire_profile_lock(&target_folder)?;
        let reset_session = {
            let mut service = state.lock_service()?;
            service
                .recover_wallet_and_set_new_password(
                    &target_folder,
                    &phrase,
                    normalized_bip39_passphrase,
                    &new_password,
                    lang,
                    local_instance_id.clone(),
                )
                .map_err(FrontendError::from)?;

            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            let _ = record_profile_metadata_in_dir(wallet_data_dir, &target_folder, user_id);
            let session_res = (|| -> Result<crate::session::SessionToken, FrontendError> {
                state
                    .initialize_profile_session(&mut service, &new_password)
                    .map_err(FrontendError::from)?;
                state.session_token_for_service(&service)
            })();
            let session = match session_res {
                Ok(sess) => {
                    state.commit_profile_lock(pending_lock);
                    sess
                }
                Err(err) => {
                    state.rollback_failed_session(&mut service);
                    return Err(err);
                }
            };
            if let Some(app) = app_opt {
                if let Ok(config_dir) = app.path().app_config_dir() {
                    let _ = crate::pin_keystore::remove_entry(&config_dir, &target_folder);
                }
            }

            if let Some(mut local_state) =
                load_recovery_owner_state(&mut service, Some(&new_password))?
            {
                if local_state.rotation_epoch < rotation_epoch {
                    local_state.rotation_epoch = rotation_epoch;
                    let _ = save_recovery_owner_state_with_cache(
                        state,
                        &mut service,
                        &local_state,
                        Some(&new_password),
                    );
                }
            }
            session
        };

        if let Some(app) = app_opt {
            crate::services::common::spawn_pending_l2_reconciliation(
                app,
                reset_session,
                &new_password,
            );
        }

        delete_active_session_from_path(session_file_path);

        return Ok(RecoveryCompletionResultDto {
            folder_name: target_folder.clone(),
            profile_name: profile_name.unwrap_or(target_folder),
            mode,
            outcome: "password_reset".to_string(),
            backup_epoch: None,
            backup_tx_nonce: None,
            active_voucher_queries_count: 0,
            reconciled_spent_count: 0,
            restored_standards_count: 0,
            l2_reconciliation_status: "reconciled".to_string(),
            pending_restore_l2_reconciliation: false,
            local_collision: None,
            resolved_sync_targets: Vec::new(),
        });
    }

    // mode == "restore_new_device"
    let mut candidate_descriptors = Vec::new();
    for r in &session.replies {
        if is_same_account_id(&r.entry.owner_did, &owner_did)
            && r.entry.rotation_epoch == rotation_epoch
        {
            for d in r
                .entry
                .backup_sync_descriptors
                .iter()
                .chain(r.extra_sync_descriptors.iter())
            {
                if !d.write_url.is_empty()
                    && !candidate_descriptors
                        .iter()
                        .any(|c: &DropDescriptor| c.write_url == d.write_url)
                {
                    candidate_descriptors.push(d.clone());
                }
            }
        }
    }

    let tmp_restore_dir = app_data_dir.join("tmp_restore");
    fs::create_dir_all(&tmp_restore_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    let (_, derived_identity) = human_money_core::Wallet::new_from_mnemonic(
        &phrase,
        normalized_bip39_passphrase,
        master_payload.user_prefix.as_deref(),
        lang,
        "social_check".to_string(),
        false,
    )
    .map_err(FrontendError::from)?;

    if !is_same_account_id(&derived_identity.user_id, &master_payload.target_did) {
        return Err(FrontendError::with_code(
            "error.recovery.wrongBip39Passphrase",
            "Die eingegebene BIP39-Passphrase stimmt nicht mit dem wiederherzustellenden Profil überein.",
        ));
    }

    let sync_inputs: Vec<String> = candidate_descriptors
        .iter()
        .filter_map(|d| serde_json::to_string(d).ok())
        .collect();
    let has_sync_inputs = !sync_inputs.is_empty();
    let fe_lang: crate::models::MnemonicLanguage = lang.into();
    let allow_empty = allow_empty_fallback.unwrap_or(false);

    let start_generation = state.current_session_generation();
    let (resolved_targets, local_collision) = if has_sync_inputs {
        let probe = probe_sync_backup_impl(
            sync_inputs,
            (*phrase).clone(),
            normalized_bip39_passphrase.map(str::to_string),
            master_payload.user_prefix.clone(),
            Some(fe_lang),
            wallet_data_dir,
            &tmp_restore_dir,
            state,
        )
        .await?;

        match probe.status.as_str() {
            "found" => {
                let staged_path = probe
                    .staged_path
                    .as_deref()
                    .map(PathBuf::from)
                    .ok_or_else(|| FrontendError::from("Missing staged backup path".to_string()))?;

                if probe.local_collision.is_some() && replace_folder_name.is_none() {
                    if is_canonical_child_of(&staged_path, &tmp_restore_dir)
                        && staged_path.is_file()
                    {
                        let _ = fs::remove_file(&staged_path);
                    }
                    return Ok(RecoveryCompletionResultDto {
                        folder_name: String::new(),
                        profile_name: probe
                            .inspection
                            .as_ref()
                            .map(|i| i.profile_name.clone())
                            .unwrap_or_default(),
                        mode,
                        outcome: "collision_requires_choice".to_string(),
                        backup_epoch: probe.inspection.as_ref().map(|i| i.epoch),
                        backup_tx_nonce: probe.inspection.as_ref().map(|i| i.tx_nonce),
                        active_voucher_queries_count: 0,
                        reconciled_spent_count: 0,
                        restored_standards_count: 0,
                        l2_reconciliation_status: "no_servers".to_string(),
                        pending_restore_l2_reconciliation: false,
                        local_collision: probe.local_collision,
                        resolved_sync_targets: probe.resolved_sync_targets,
                    });
                }

                let credentials = BackupRestoreCredentials::Mnemonic {
                    phrase: &phrase,
                    passphrase: normalized_bip39_passphrase,
                    prefix: master_payload.user_prefix.as_deref(),
                    language: lang,
                };
                let standards_dir = app_data_dir.join("voucher_standards");
                let (restore_outcome, restored_session) =
                    match finalize_restored_wallet_and_reconcile_l2(
                        &staged_path,
                        credentials,
                        &new_password,
                        local_instance_id,
                        replace_folder_name.as_deref(),
                        wallet_data_dir,
                        &standards_dir,
                        &tmp_restore_dir,
                        state,
                        app_opt,
                    )
                    .await
                    {
                        Ok(res) => res,
                        Err(err) => {
                            if is_canonical_child_of(&staged_path, &tmp_restore_dir)
                                && staged_path.is_file()
                            {
                                let _ = fs::remove_file(&staged_path);
                            }
                            return Err(err);
                        }
                    };

                let expected_owner_id = derived_identity.user_id.clone();
                state.with_session_required(
                    &restored_session,
                    "Restored wallet session changed before updating recovery rotation epoch",
                    |service| {
                        let current_owner_id = service
                            .with_wallet(|w| w.get_user_id().to_string())
                            .map_err(FrontendError::from)?;
                        if !is_same_account_id(&current_owner_id, &expected_owner_id) {
                            return Err(FrontendError::from(
                                "Restored wallet account changed before updating recovery epoch"
                                    .to_string(),
                            ));
                        }
                        let existing_opt = load_recovery_owner_state(service, Some(&new_password))?;
                        let mut local_state = existing_opt.unwrap_or_else(|| RecoveryOwnerState {
                            rotation_epoch,
                            ..Default::default()
                        });
                        if local_state.rotation_epoch < rotation_epoch {
                            local_state.rotation_epoch = rotation_epoch;
                        }
                        let _ = save_recovery_owner_state_with_cache(
                            state,
                            service,
                            &local_state,
                            Some(&new_password),
                        );
                        Ok(())
                    },
                )?;

                delete_active_session_from_path(session_file_path);

                let pending_l2 = restore_outcome.active_voucher_queries_count > 0
                    && restore_outcome.l2_reconciliation_status != "reconciled";

                return Ok(RecoveryCompletionResultDto {
                    folder_name: restore_outcome.folder_name,
                    profile_name: restore_outcome.profile_name,
                    mode,
                    outcome: "restored_from_sync".to_string(),
                    backup_epoch: probe.inspection.as_ref().map(|i| i.epoch),
                    backup_tx_nonce: probe.inspection.as_ref().map(|i| i.tx_nonce),
                    active_voucher_queries_count: restore_outcome.active_voucher_queries_count,
                    reconciled_spent_count: restore_outcome.reconciled_spent_count,
                    restored_standards_count: restore_outcome.restored_standards_count,
                    l2_reconciliation_status: restore_outcome.l2_reconciliation_status,
                    pending_restore_l2_reconciliation: pending_l2,
                    local_collision: None,
                    resolved_sync_targets: probe.resolved_sync_targets,
                });
            }
            "unreachable" => {
                if !allow_empty {
                    return Err(FrontendError::with_code(
                        "error.recovery.syncUnreachable",
                        format!(
                            "SYNC_UNREACHABLE: Sync-Dienst nicht erreichbar: {}",
                            probe.error.unwrap_or_else(|| "Netzwerkfehler".to_string())
                        ),
                    ));
                }
                (probe.resolved_sync_targets, probe.local_collision)
            }
            _ => {
                // "not_found"
                if !allow_empty {
                    return Ok(RecoveryCompletionResultDto {
                        folder_name: String::new(),
                        profile_name: profile_name.unwrap_or_default(),
                        mode,
                        outcome: "sync_empty_requires_confirm".to_string(),
                        backup_epoch: None,
                        backup_tx_nonce: None,
                        active_voucher_queries_count: 0,
                        reconciled_spent_count: 0,
                        restored_standards_count: 0,
                        l2_reconciliation_status: "no_servers".to_string(),
                        pending_restore_l2_reconciliation: false,
                        local_collision: probe.local_collision,
                        resolved_sync_targets: probe.resolved_sync_targets,
                    });
                }
                (probe.resolved_sync_targets, probe.local_collision)
            }
        }
    } else {
        if !state.is_generation_current(start_generation) {
            return Err(FrontendError::from(
                "Wallet session changed during social recovery".to_string(),
            ));
        }
        let user_ids = user_ids_from_metadata_dir(wallet_data_dir);
        let service = state.lock_service()?;
        let coll = find_local_profile_collision_impl(
            wallet_data_dir,
            &service,
            &derived_identity.user_id,
            Some(&derived_identity),
            &user_ids,
        );
        (Vec::new(), coll)
    };

    if local_collision.is_some() && replace_folder_name.is_none() {
        return Ok(RecoveryCompletionResultDto {
            folder_name: String::new(),
            profile_name: profile_name.unwrap_or_default(),
            mode,
            outcome: "collision_requires_choice".to_string(),
            backup_epoch: None,
            backup_tx_nonce: None,
            active_voucher_queries_count: 0,
            reconciled_spent_count: 0,
            restored_standards_count: 0,
            l2_reconciliation_status: "no_servers".to_string(),
            pending_restore_l2_reconciliation: false,
            local_collision,
            resolved_sync_targets: resolved_targets,
        });
    }

    if let Some(ref rf) = replace_folder_name {
        if !rf.trim().is_empty() {
            return Err(FrontendError::from(
                "Cannot replace local profile without a backup; use reset_local_password instead."
                    .to_string(),
            ));
        }
    }

    if !has_sync_inputs && !allow_empty {
        return Ok(RecoveryCompletionResultDto {
            folder_name: String::new(),
            profile_name: profile_name.unwrap_or_default(),
            mode,
            outcome: "sync_empty_requires_confirm".to_string(),
            backup_epoch: None,
            backup_tx_nonce: None,
            active_voucher_queries_count: 0,
            reconciled_spent_count: 0,
            restored_standards_count: 0,
            l2_reconciliation_status: "no_servers".to_string(),
            pending_restore_l2_reconciliation: false,
            local_collision: None,
            resolved_sync_targets: resolved_targets,
        });
    }

    // Confirmed seed-only profile setup (either no sync descriptors existed or user explicitly confirmed empty start)
    let final_name = profile_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Wiederhergestelltes Profil".to_string());
    if !state.is_generation_current(start_generation) {
        return Err(FrontendError::from(
            "Wallet session changed before creating recovered profile".to_string(),
        ));
    }
    let (created_folder, post_login_session) = {
        let mut service = state.lock_service()?;
        let existing_folders: std::collections::HashSet<String> = service
            .list_profiles()
            .map(|list| list.into_iter().map(|p| p.folder_name).collect())
            .unwrap_or_default();
        service
            .create_profile(
                &final_name,
                &phrase,
                normalized_bip39_passphrase,
                master_payload.user_prefix.as_deref(),
                &new_password,
                lang,
                local_instance_id,
                true,
            )
            .map_err(FrontendError::from)?;

        let created_folder_opt = service.list_profiles().ok().and_then(|profiles| {
            profiles
                .into_iter()
                .find(|p| {
                    !existing_folders.contains(&p.folder_name) || p.profile_name == final_name
                })
                .map(|p| p.folder_name)
        });

        let setup_res = (|| -> Result<(String, crate::session::SessionToken), FrontendError> {
            let folder = created_folder_opt.clone().unwrap_or_default();

            let pending_lock = if !folder.is_empty() {
                state.acquire_profile_lock(&folder)?
            } else {
                crate::state::PendingProfileLock::None
            };

            crate::state::save_encrypted_json(
                &mut service,
                crate::settings::UI_PREFERENCES_KEY,
                &crate::settings::UiPreferences::default(),
                Some(&new_password),
            )
            .map_err(FrontendError::from)?;
            crate::state::save_encrypted_json(
                &mut service,
                crate::settings::SECURITY_POLICY_KEY,
                &crate::settings::SecurityPolicy::default(),
                Some(&new_password),
            )
            .map_err(FrontendError::from)?;
            crate::state::save_local_sidecar_json(
                &mut service,
                crate::settings::INTERNAL_LIFECYCLE_STATE_KEY,
                &crate::settings::InternalLifecycleState::default(),
                Some(&new_password),
            )
            .map_err(FrontendError::from)?;

            let initial_state = RecoveryOwnerState {
                rotation_epoch,
                ..Default::default()
            };
            save_recovery_owner_state_with_cache(
                state,
                &mut service,
                &initial_state,
                Some(&new_password),
            )?;

            if !folder.is_empty() {
                let _ = record_profile_metadata_in_dir(
                    wallet_data_dir,
                    &folder,
                    Some(derived_identity.user_id.clone()),
                );
            }
            state
                .initialize_profile_session(&mut service, &new_password)
                .map_err(FrontendError::from)?;
            let post_login_session = state.session_token_for_service(&service)?;
            state.commit_profile_lock(pending_lock);
            Ok((folder, post_login_session))
        })();
        match setup_res {
            Ok((folder, post_login_session)) => (folder, post_login_session),
            Err(err) => {
                state.rollback_failed_session(&mut service);
                if let Some(ref folder) = created_folder_opt {
                    if !existing_folders.contains(folder) && is_safe_filename_component(folder) {
                        let _ = std::fs::remove_dir_all(wallet_data_dir.join(folder));
                        let _ = crate::services::fs_helpers::remove_profile_metadata_entry_in_dir(
                            wallet_data_dir,
                            folder,
                        );
                        let profiles_path = wallet_data_dir.join("profiles.json");
                        if let Ok(bytes) = std::fs::read(&profiles_path) {
                            if let Ok(mut profiles) = serde_json::from_slice::<
                                Vec<human_money_core::app_service::ProfileInfo>,
                            >(&bytes)
                            {
                                let before_len = profiles.len();
                                profiles.retain(|p| p.folder_name != *folder);
                                if profiles.len() != before_len {
                                    if let Ok(updated) = serde_json::to_vec_pretty(&profiles) {
                                        let _ = write_file_atomic(&profiles_path, &updated);
                                    }
                                }
                            }
                        }
                    }
                }
                return Err(err);
            }
        }
    };

    for target in &resolved_targets {
        if !state.is_generation_current(post_login_session.generation) {
            break;
        }
        let _ = save_public_sync_impl(
            target.write_url.clone(),
            target.read_url.clone(),
            target.drop_pub.clone(),
            target.register_url.clone(),
            true,
            Some(new_password.clone()),
            state,
        )
        .await;
    }

    delete_active_session_from_path(session_file_path);

    Ok(RecoveryCompletionResultDto {
        folder_name: created_folder,
        profile_name: final_name,
        mode,
        outcome: "restored_seed_only".to_string(),
        backup_epoch: None,
        backup_tx_nonce: None,
        active_voucher_queries_count: 0,
        reconciled_spent_count: 0,
        restored_standards_count: 0,
        l2_reconciliation_status: "no_servers".to_string(),
        pending_restore_l2_reconciliation: false,
        local_collision: None,
        resolved_sync_targets: resolved_targets,
    })
}
