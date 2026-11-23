use human_money_core::app_service::AppService;
use human_money_core::models::secure_container::{
    ContainerConfig, PayloadType, PrivacyMode, SecureContainer,
};
use human_money_core::UserIdentity;
use human_money_core::ValidatedUserId;
use humoco_market::{
    canonicalize_owner_account_id, seal_post_box, DropDescriptor, GuardianLocatorUpdate,
    GuardianRevoke, GuardianVaultStore, PostEnvelope, MAX_RECOVERY_SYNC_DESCRIPTORS,
    PAYLOAD_GUARDIAN_LOCATOR_V1, PAYLOAD_GUARDIAN_REVOKE_V1,
};

use crate::models::{FrontendError, RecoveryOwnerState};
use crate::services::{
    common::collect_backup_sync_targets_with_service,
    post_service::{
        collect_own_reply_syncs, flush_post_slots_impl, merge_post_state_with_service,
        save_post_state_with_service,
    },
};
use crate::state::{
    AppState, PostOutboxEntry, StoredPostState, RECOVERY_GUARDIAN_VAULT_KEY,
    RECOVERY_OWNER_STATE_KEY,
};

// ============================================================================
// Storage Helpers
// ============================================================================

pub fn load_recovery_owner_state(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<Option<RecoveryOwnerState>, FrontendError> {
    crate::state::load_encrypted_json_opt(service, RECOVERY_OWNER_STATE_KEY, password)
        .map_err(FrontendError::from)
}

pub fn save_recovery_owner_state(
    service: &mut AppService,
    owner_state: &RecoveryOwnerState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, RECOVERY_OWNER_STATE_KEY, owner_state, password)
        .map_err(FrontendError::from)
}

pub fn load_guardian_vault(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<GuardianVaultStore, FrontendError> {
    crate::state::load_encrypted_json_or_default(service, RECOVERY_GUARDIAN_VAULT_KEY, password)
        .map_err(FrontendError::from)
}

pub fn save_guardian_vault(
    service: &mut AppService,
    vault: &GuardianVaultStore,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, RECOVERY_GUARDIAN_VAULT_KEY, vault, password)
        .map_err(FrontendError::from)
}

pub(crate) fn load_recovery_owner_state_with_cache(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
) -> Result<Option<RecoveryOwnerState>, FrontendError> {
    match load_recovery_owner_state(service, password) {
        Ok(res) => {
            state.set_cached_recovery_owner_for_service(service, Some(res.clone()));
            Ok(res)
        }
        Err(err) => {
            if password.is_none()
                && matches!(
                    err.code.as_str(),
                    "error.auth.sessionNotActive" | "error.auth.sessionTimeout"
                )
            {
                if let Some(cached) = state.cache_recovery_owner() {
                    return Ok(cached);
                }
            }
            Err(err)
        }
    }
}

pub(crate) fn save_recovery_owner_state_with_cache(
    state: &AppState,
    service: &mut AppService,
    owner_state: &RecoveryOwnerState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    save_recovery_owner_state(service, owner_state, password)?;
    state.set_cached_recovery_owner_for_service(service, Some(Some(owner_state.clone())));
    Ok(())
}

pub(crate) fn load_guardian_vault_with_cache(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
) -> Result<GuardianVaultStore, FrontendError> {
    match load_guardian_vault(service, password) {
        Ok(vault) => {
            state.set_cached_guardian_vault_for_service(service, Some(vault.clone()));
            Ok(vault)
        }
        Err(err) => {
            if password.is_none()
                && matches!(
                    err.code.as_str(),
                    "error.auth.sessionNotActive" | "error.auth.sessionTimeout"
                )
            {
                if let Some(cached) = state.cache_guardian_vault() {
                    return Ok(cached);
                }
            }
            Err(err)
        }
    }
}

pub(crate) fn save_guardian_vault_with_cache(
    state: &AppState,
    service: &mut AppService,
    vault: &GuardianVaultStore,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    save_guardian_vault(service, vault, password)?;
    state.set_cached_guardian_vault_for_service(service, Some(vault.clone()));
    Ok(())
}

pub(super) fn save_post_and_recovery_owner_atomic(
    state: &AppState,
    session: &crate::session::SessionToken,
    baseline_post: &StoredPostState,
    working_post: &mut StoredPostState,
    owner_state: &RecoveryOwnerState,
    password: Option<&str>,
    conflict_context: &str,
) -> Result<(), FrontendError> {
    state.with_session_required(session, conflict_context, |service| {
        merge_post_state_with_service(service, baseline_post, working_post, password)?;
        let mut batch = crate::state::EncryptedBatchCommit::new();
        batch
            .post_state(working_post.clone())
            .recovery_owner(owner_state.clone());
        batch
            .commit(state, service, password)
            .map_err(FrontendError::from)
    })
}

pub(super) fn is_same_account_id(a: &str, b: &str) -> bool {
    match (
        canonicalize_owner_account_id(a),
        canonicalize_owner_account_id(b),
    ) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a.trim() == b.trim(),
    }
}

pub(super) fn seal_recovery_container(
    my_identity: &UserIdentity,
    recipient_did: &str,
    payload_type: &str,
    payload_bytes: &[u8],
) -> Result<Vec<u8>, FrontendError> {
    let container = SecureContainer::seal(
        my_identity,
        &ContainerConfig::TargetDid(recipient_did.to_string(), PrivacyMode::HashedRouting),
        payload_bytes,
        PayloadType::Generic(payload_type.to_string()),
    )
    .map_err(FrontendError::from)?;
    serde_json::to_vec(&container).map_err(|e| FrontendError::from(e.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn enqueue_recovery_container_to_outbox(
    post_state: &mut StoredPostState,
    my_user_id: &str,
    my_signing_key: &ed25519_dalek::SigningKey,
    recipient_did: &str,
    container_bytes: Vec<u8>,
    now_secs: u64,
    reply_drops: &[DropDescriptor],
) -> Result<(), FrontendError> {
    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        my_user_id,
        now_secs,
        reply_drops.to_vec(),
        container_bytes,
    );
    let validated_recipient = ValidatedUserId::parse(recipient_did)
        .map_err(|e| FrontendError::from(format!("Invalid guardian DID: {}", e)))?;
    let recipient_did = validated_recipient.as_str();
    let recipient_pk = validated_recipient.public_key();
    let box_entry =
        seal_post_box(my_signing_key, recipient_pk, &envelope).map_err(FrontendError::from)?;

    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id,
        recipient_did: recipient_did.to_string(),
        tag16_hex: hex::encode(box_entry.tag16),
        bundle_id: None,
        message_class: crate::state::OutboxMessageClass::Control,
        entry: box_entry,
        created_at: now_secs,
    });
    Ok(())
}

pub(super) fn enqueue_guardian_revocations(
    post_state: &mut StoredPostState,
    my_identity: &UserIdentity,
    owner_did: &str,
    new_epoch: u32,
    revoked_dids: impl IntoIterator<Item = String>,
    now_secs: u64,
    reply_drops: &[DropDescriptor],
) {
    for g_did in revoked_dids {
        let revoke = GuardianRevoke {
            owner_did: owner_did.to_string(),
            rotation_epoch: new_epoch,
        };
        if let Ok(revoke_bytes) = serde_json::to_vec(&revoke) {
            if let Ok(container_bytes) = seal_recovery_container(
                my_identity,
                &g_did,
                PAYLOAD_GUARDIAN_REVOKE_V1,
                &revoke_bytes,
            ) {
                let _ = enqueue_recovery_container_to_outbox(
                    post_state,
                    &my_identity.user_id,
                    &my_identity.signing_key,
                    &g_did,
                    container_bytes,
                    now_secs,
                    reply_drops,
                );
            }
        }
    }
}

pub async fn enqueue_guardian_locator_updates_if_configured(
    state: &AppState,
    session: &crate::session::SessionToken,
    password: Option<&str>,
) -> Result<usize, FrontendError> {
    if !state.is_generation_current(session.generation) {
        return Ok(0);
    }
    let _recovery_guard = state.recovery_op_lock.lock().await;
    let Some(Some((my_identity, guardians, epoch))) = state.with_session(session, |service| {
        let owner_state = load_recovery_owner_state(service, password)?;
        let Some(owner) = owner_state else {
            return Ok(None);
        };
        if owner.guardians.is_empty() {
            return Ok(None);
        }
        let id = service
            .with_wallet_and_identity(|_w, i| i.clone())
            .map_err(FrontendError::from)?;
        Ok(Some((id, owner.guardians, owner.rotation_epoch)))
    })?
    else {
        return Ok(0);
    };

    let Some((candidate_descriptors, security_policy)) =
        state.with_session(session, |service| {
            let descriptors = collect_backup_sync_targets_with_service(
                state,
                service,
                password,
                Some(MAX_RECOVERY_SYNC_DESCRIPTORS),
            )?;
            let policy = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            Ok((descriptors, policy))
        })?
    else {
        return Ok(0);
    };
    let reply_drops: Vec<DropDescriptor> = collect_own_reply_syncs(
        &security_policy,
        human_money_core::crypto::get_prefix_from_user_id(&my_identity.user_id),
    )
    .into_iter()
    .map(Into::into)
    .collect();

    let owner_account_id =
        canonicalize_owner_account_id(&my_identity.user_id).map_err(FrontendError::from)?;

    let _post_guard = state.post_op_lock.lock().await;
    let now_secs = crate::sync_client::current_unix_timestamp();
    let Some(mut post_state) = state.with_session(session, |service| {
        crate::services::post_service::load_post_state_with_service(service, password)
    })?
    else {
        return Ok(0);
    };
    let mut baseline = post_state.clone();
    let mut enqueued = 0;

    for g in guardians {
        let update = GuardianLocatorUpdate {
            owner_did: owner_account_id.clone(),
            rotation_epoch: epoch,
            backup_sync_descriptors: candidate_descriptors.clone(),
            updated_at: now_secs,
        };
        let update_bytes =
            serde_json::to_vec(&update).map_err(|e| FrontendError::from(e.to_string()))?;
        let container_bytes = seal_recovery_container(
            &my_identity,
            &g.user_id,
            PAYLOAD_GUARDIAN_LOCATOR_V1,
            &update_bytes,
        )?;

        if enqueue_recovery_container_to_outbox(
            &mut post_state,
            &my_identity.user_id,
            &my_identity.signing_key,
            &g.user_id,
            container_bytes,
            now_secs,
            &reply_drops,
        )
        .is_ok()
        {
            enqueued += 1;
        }
    }

    if enqueued > 0 {
        let saved_before_flush = state.with_session(session, |service| {
            merge_post_state_with_service(service, &baseline, &mut post_state, password)?;
            save_post_state_with_service(state, service, &post_state, password)
        })?;
        if saved_before_flush.is_none() {
            return Ok(0);
        }
        baseline = post_state.clone();
        let _ = flush_post_slots_impl(state, session, &mut post_state, None, &[], password).await;
        let saved_after_flush = state.with_session(session, |service| {
            merge_post_state_with_service(service, &baseline, &mut post_state, password)?;
            save_post_state_with_service(state, service, &post_state, password)
        })?;
        if saved_after_flush.is_none() {
            return Ok(0);
        }
    }

    Ok(enqueued)
}
