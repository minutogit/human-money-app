use super::storage::*;
use super::*;

use human_money_core::ValidatedUserId;
use humoco_market::{
    canonicalize_owner_account_id, create_social_recovery_vault_bundle, DropDescriptor,
    RecoveryKdfParams, RecoveryMasterPayload, MAX_RECOVERY_SYNC_DESCRIPTORS,
    PAYLOAD_GUARDIAN_ASSIGN_V1,
};
use tauri::Manager;

use crate::models::{
    FrontendError, GuardianAssignmentRecord, GuardianAssignmentStatus, RecoveryOwnerState,
    RevokeSocialRecoveryResultDto, SetupSocialRecoveryResultDto,
};
use crate::services::{
    backup_service::sync_backup_impl,
    common::{are_dids_equal, collect_backup_sync_targets_with_service},
    post_service::{collect_own_reply_syncs, sync_post_channels_impl},
};
use crate::state::AppState;

#[allow(clippy::too_many_arguments)]
pub async fn setup_social_recovery_guardians_with_params(
    password: String,
    guardian_dids: Vec<String>,
    threshold_m: u8,
    question_ids: [u8; 3],
    answers: [String; 3],
    kdf_params: RecoveryKdfParams,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
) -> Result<SetupSocialRecoveryResultDto, FrontendError> {
    if guardian_dids.len() < 3 || guardian_dids.len() > 16 {
        return Err(FrontendError::from(format!(
            "Number of guardians must be between 3 and 16, got {}",
            guardian_dids.len()
        )));
    }
    if threshold_m < 2 || threshold_m > guardian_dids.len() as u8 {
        return Err(FrontendError::from(format!(
            "Threshold m must be between 2 and {}, got {}",
            guardian_dids.len(),
            threshold_m
        )));
    }

    // Validate every guardian DID strictly and deduplicate by canonical Ed25519 root public key bytes
    // so two subaccounts or formatting variants of the same person cannot hold multiple shares.
    let mut normalized_guardian_dids = Vec::with_capacity(guardian_dids.len());
    let mut unique_root_keys = std::collections::HashSet::<[u8; 32]>::new();
    for did in &guardian_dids {
        let trimmed = did.trim();
        let validated = ValidatedUserId::parse(trimmed).map_err(|e| {
            FrontendError::from(format!("Invalid guardian DID '{}': {}", trimmed, e))
        })?;
        let root_key_bytes = validated.public_key().to_bytes();
        if !unique_root_keys.insert(root_key_bytes) {
            return Err(FrontendError::from(format!(
                "Duplicate guardian identity in setup: {}",
                trimmed
            )));
        }
        normalized_guardian_dids.push(validated.as_str().to_string());
    }
    let guardian_dids = normalized_guardian_dids;

    // Serialize concurrent setup/revocation operations under `recovery_op_lock` BEFORE reading epoch & committing.
    let (new_epoch, guardians_count, setup_session) = {
        let _recovery_guard = state.recovery_op_lock.lock().await;

        // Freshly verify password, export mnemonic, and load contacts + identity
        let (
            session,
            phrase,
            lang,
            has_bip39_passphrase,
            contacts,
            my_identity,
            my_prefix,
            my_profile_name,
            old_owner_state,
        ) = {
            let mut service =
                state.authorize_and_lock(crate::state::RequiredAccess::Full, Some(&password))?;
            let session = state.session_token_for_service(&service)?;
            let exp = service
                .export_mnemonic(&password)
                .map_err(FrontendError::from)?;
            let (phrase, lang, has_bip39_passphrase) = exp.ok_or_else(|| {
                FrontendError::from("Mnemonic phrase required to setup social recovery".to_string())
            })?;
            let contacts = state.load_contacts(&mut service, Some(&password))?;

            let local_fallback_name = {
                let profiles = service.list_profiles().unwrap_or_default();
                if let Some(app_handle) = app {
                    if let Ok(app_dir) = app_handle.path().app_data_dir() {
                        let wallet_dir = app_dir.join("wallet_data");
                        crate::commands::auth::resolve_active_profile_folder(
                            &service,
                            None,
                            &wallet_dir,
                        )
                        .ok()
                        .and_then(|folder| {
                            profiles
                                .into_iter()
                                .find(|p| p.folder_name == folder)
                                .map(|p| p.profile_name)
                        })
                    } else {
                        None
                    }
                } else if profiles.len() == 1 {
                    Some(profiles[0].profile_name.clone())
                } else {
                    None
                }
            }
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

            let (my_identity, my_prefix, my_profile_name) = service
                .with_wallet_and_identity(|w, id| {
                    let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                        .map(|s| s.to_string());
                    let first = w.profile.first_name.as_deref().unwrap_or("").trim();
                    let last = w.profile.last_name.as_deref().unwrap_or("").trim();
                    let full = format!("{} {}", first, last).trim().to_string();
                    let name = if !full.is_empty() {
                        full
                    } else if let Some(ref local_name) = local_fallback_name {
                        local_name.clone()
                    } else {
                        id.user_id.clone()
                    };
                    (id.clone(), pfx, name)
                })
                .map_err(|e| FrontendError::from(e.to_string()))?;

            for g_did in &guardian_dids {
                if are_dids_equal(g_did, &my_identity.user_id) {
                    return Err(FrontendError::from(
                        "Cannot assign your own identity as a recovery guardian".to_string(),
                    ));
                }
                let contact = contacts
                    .iter()
                    .find(|c| are_dids_equal(&c.did, g_did))
                    .ok_or_else(|| {
                        FrontendError::from(format!("Guardian {} is not in your contacts", g_did))
                    })?;
                let contact_name =
                    format_contact_display_name(contact).unwrap_or_else(|| contact.did.clone());
                if contact.trust_level.unwrap_or(0) < 1 {
                    return Err(FrontendError::from(format!(
                        "Guardian {} does not have trust level >= 1 (Acquaintance/Bekannt)",
                        contact_name
                    )));
                }
                let is_offender = service
                    .with_wallet(|w| {
                        matches!(
                            w.check_reputation(g_did),
                            human_money_core::models::conflict::TrustStatus::KnownOffender(_)
                        )
                    })
                    .unwrap_or(false);
                if is_offender {
                    return Err(FrontendError::from(format!(
                        "Guardian {} is flagged as KnownOffender",
                        contact_name
                    )));
                }
            }

            let old_owner_state = load_recovery_owner_state(&mut service, Some(&password))?;
            (
                session,
                phrase,
                lang,
                has_bip39_passphrase,
                contacts,
                my_identity,
                my_prefix,
                my_profile_name,
                old_owner_state,
            )
        };

        let entropy = zeroize::Zeroizing::new(
            human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(&phrase, lang)
                .map_err(|e| FrontendError::from(e.to_string()))?,
        );

        let old_epoch = old_owner_state
            .as_ref()
            .map(|s| s.rotation_epoch)
            .unwrap_or(0);
        let new_epoch = old_epoch + 1;
        let now_secs = crate::sync_client::current_unix_timestamp();

        let (security_policy, backup_descriptors) = state.with_session_required(
            &session,
            "Active wallet session changed during social recovery setup",
            |service| {
                let sp = state
                    .get_or_load_security_policy(service, Some(password.as_str()))
                    .map_err(FrontendError::from)?;
                let bd = collect_backup_sync_targets_with_service(
                    state,
                    service,
                    Some(&password),
                    Some(MAX_RECOVERY_SYNC_DESCRIPTORS),
                )?;
                Ok((sp, bd))
            },
        )?;
        let reply_drops: Vec<DropDescriptor> = collect_own_reply_syncs(
            &security_policy,
            human_money_core::crypto::get_prefix_from_user_id(&my_identity.user_id),
        )
        .into_iter()
        .map(Into::into)
        .collect();

        let owner_account_id =
            canonicalize_owner_account_id(&my_identity.user_id).map_err(FrontendError::from)?;

        let master_payload = RecoveryMasterPayload::new(
            &owner_account_id,
            entropy.to_vec(),
            lang,
            my_prefix,
            has_bip39_passphrase,
        );

        let total_n = guardian_dids.len() as u8;
        let kdf_signing_key = my_identity.signing_key.clone();
        let kdf_owner_id = owner_account_id.clone();
        let kdf_profile_name = my_profile_name.clone();
        let kdf_backup_descriptors = backup_descriptors;
        let kdf_params_for_task = kdf_params.clone();
        let kdf_answers = answers;

        let (entries, answer_verifiers) = tokio::task::spawn_blocking(move || {
            let ans_refs = [
                kdf_answers[0].as_str(),
                kdf_answers[1].as_str(),
                kdf_answers[2].as_str(),
            ];
            let entries = create_social_recovery_vault_bundle(
                &kdf_signing_key,
                &kdf_owner_id,
                &kdf_profile_name,
                new_epoch,
                threshold_m,
                total_n,
                &question_ids,
                &ans_refs,
                &kdf_params_for_task,
                &master_payload,
                &kdf_backup_descriptors,
                now_secs,
            )
            .map_err(FrontendError::from)?;

            let verifiers = humoco_market::compute_rehearsal_verifiers(
                &kdf_owner_id,
                new_epoch,
                &question_ids,
                &ans_refs,
                &kdf_params_for_task,
            )
            .map_err(FrontendError::from)?;
            Ok::<_, FrontendError>((entries, Some(verifiers)))
        })
        .await
        .map_err(|e| FrontendError::from(format!("Recovery setup KDF task failed: {}", e)))??;

        let mut assignment_records = Vec::with_capacity(guardian_dids.len());
        {
            let _post_guard = state.post_op_lock.lock().await;
            let mut post_state = state.with_session_required(
                &session,
                "Active wallet session changed before writing recovery outbox",
                |service| {
                    crate::services::post_service::load_post_state_with_service(
                        service,
                        Some(&password),
                    )
                },
            )?;
            let baseline = post_state.clone();

            // Send revoke to any old guardians being removed
            if let Some(ref old) = old_owner_state {
                let removed_dids: Vec<String> = old
                    .guardians
                    .iter()
                    .filter(|old_g| {
                        !guardian_dids
                            .iter()
                            .any(|d| are_dids_equal(d, &old_g.user_id))
                    })
                    .map(|old_g| old_g.user_id.clone())
                    .collect();
                enqueue_guardian_revocations(
                    &mut post_state,
                    &my_identity,
                    &owner_account_id,
                    new_epoch,
                    removed_dids,
                    now_secs,
                    &reply_drops,
                );
            }

            for (i, g_did) in guardian_dids.iter().enumerate() {
                let entry = &entries[i];
                let entry_bytes =
                    serde_json::to_vec(entry).map_err(|e| FrontendError::from(e.to_string()))?;
                let container_bytes = seal_recovery_container(
                    &my_identity,
                    g_did,
                    PAYLOAD_GUARDIAN_ASSIGN_V1,
                    &entry_bytes,
                )?;
                enqueue_recovery_container_to_outbox(
                    &mut post_state,
                    &my_identity.user_id,
                    &my_identity.signing_key,
                    g_did,
                    container_bytes,
                    now_secs,
                    &reply_drops,
                )?;

                let display_name = contacts
                    .iter()
                    .find(|c| are_dids_equal(&c.did, g_did))
                    .and_then(format_contact_display_name)
                    .unwrap_or_else(|| g_did.clone());

                assignment_records.push(GuardianAssignmentRecord {
                    user_id: g_did.clone(),
                    display_name,
                    share_index: entry.share.index,
                    status: GuardianAssignmentStatus::Pending,
                    assigned_at: now_secs,
                    sealed_entry: entry.clone(),
                });
            }

            let new_owner_state = RecoveryOwnerState {
                rotation_epoch: new_epoch,
                threshold_m,
                question_ids,
                guardians: assignment_records,
                emergency_sheet_confirmed_at: old_owner_state
                    .as_ref()
                    .and_then(|s| s.emergency_sheet_confirmed_at),
                answer_verifiers,
                rehearsal_stage: 0,
                next_rehearsal_at: Some(now_secs.saturating_add(REHEARSAL_INTERVAL_SECS[0])),
                rehearsal_kdf_m_kib: kdf_params.m_kib,
                rehearsal_kdf_t_cost: kdf_params.t_cost,
            };

            save_post_and_recovery_owner_atomic(
                state,
                &session,
                &baseline,
                &mut post_state,
                &new_owner_state,
                Some(&password),
                "Active wallet changed before recovery setup could be saved",
            )?;
        }

        (new_epoch, guardian_dids.len(), session)
    };

    if !state.is_generation_current(setup_session.generation) {
        return Err(FrontendError::from(
            "Active wallet session changed before recovery sync could run".to_string(),
        ));
    }
    let post_sync_result =
        match sync_post_channels_impl(app, None, None, Some(password.clone()), state).await {
            Ok(res) => res,
            Err(err) => crate::models::FrontendPostSyncResult {
                errors: vec![err.message],
                ..Default::default()
            },
        };
    let backup_sync_result = if state.is_generation_current(setup_session.generation) {
        match app {
            Some(handle) => sync_backup_impl(Some(password), None, state, handle)
                .await
                .ok(),
            None => None,
        }
    } else {
        None
    };

    Ok(SetupSocialRecoveryResultDto {
        rotation_epoch: new_epoch,
        threshold_m,
        guardians_count,
        post_sync_result,
        backup_sync_result,
    })
}

pub async fn revoke_social_recovery_impl(
    password: Option<String>,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
) -> Result<RevokeSocialRecoveryResultDto, FrontendError> {
    let (new_epoch, revoked_count, revoke_session) = {
        let _recovery_guard = state.recovery_op_lock.lock().await;
        let (session, my_identity, old_guardians, old_epoch, emergency_confirmed) = {
            let mut service = state
                .authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
            let session = state.session_token_for_service(&service)?;
            let owner =
                load_recovery_owner_state(&mut service, password.as_deref())?.unwrap_or_default();
            let id = service
                .with_wallet_and_identity(|_w, i| i.clone())
                .map_err(FrontendError::from)?;
            (
                session,
                id,
                owner.guardians,
                owner.rotation_epoch,
                owner.emergency_sheet_confirmed_at,
            )
        };

        let new_epoch = old_epoch + 1;
        let now_secs = crate::sync_client::current_unix_timestamp();
        let security_policy = state.with_session_required(
            &session,
            "Active wallet session changed before recovery revocation could load policy",
            |service| {
                state
                    .get_or_load_security_policy(service, password.as_deref())
                    .map_err(FrontendError::from)
            },
        )?;
        let reply_drops: Vec<DropDescriptor> = collect_own_reply_syncs(
            &security_policy,
            human_money_core::crypto::get_prefix_from_user_id(&my_identity.user_id),
        )
        .into_iter()
        .map(Into::into)
        .collect();
        let revoked_count = old_guardians.len();
        let owner_account_id =
            canonicalize_owner_account_id(&my_identity.user_id).map_err(FrontendError::from)?;

        {
            let _post_guard = state.post_op_lock.lock().await;
            let mut post_state = state.with_session_required(
                &session,
                "Active wallet session changed before loading post state for recovery revocation",
                |service| {
                    crate::services::post_service::load_post_state_with_service(
                        service,
                        password.as_deref(),
                    )
                },
            )?;
            let baseline = post_state.clone();

            enqueue_guardian_revocations(
                &mut post_state,
                &my_identity,
                &owner_account_id,
                new_epoch,
                old_guardians.into_iter().map(|g| g.user_id),
                now_secs,
                &reply_drops,
            );

            let updated_owner_state = RecoveryOwnerState {
                rotation_epoch: new_epoch,
                emergency_sheet_confirmed_at: emergency_confirmed,
                ..Default::default()
            };

            save_post_and_recovery_owner_atomic(
                state,
                &session,
                &baseline,
                &mut post_state,
                &updated_owner_state,
                password.as_deref(),
                "Active wallet changed before recovery revocation could be saved",
            )?;
        }

        (new_epoch, revoked_count, session)
    };

    if !state.is_generation_current(revoke_session.generation) {
        return Err(FrontendError::from(
            "Active wallet session changed before recovery revocation sync could run".to_string(),
        ));
    }
    let post_sync_result =
        match sync_post_channels_impl(app, None, None, password.clone(), state).await {
            Ok(res) => res,
            Err(err) => crate::models::FrontendPostSyncResult {
                errors: vec![err.message],
                ..Default::default()
            },
        };
    let backup_sync_result = if state.is_generation_current(revoke_session.generation) {
        match app {
            Some(handle) => sync_backup_impl(password, None, state, handle).await.ok(),
            None => None,
        }
    } else {
        None
    };

    Ok(RevokeSocialRecoveryResultDto {
        rotation_epoch: new_epoch,
        revoked_guardians_count: revoked_count,
        post_sync_result,
        backup_sync_result,
    })
}
