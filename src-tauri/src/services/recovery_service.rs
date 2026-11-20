use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

use chrono::Utc;
use human_money_core::app_service::AppService;
use human_money_core::models::backup::BackupRestoreCredentials;
use human_money_core::models::secure_container::{
    ContainerConfig, PayloadType, PrivacyMode, SecureContainer,
};
use human_money_core::services::mnemonic::MnemonicLanguage;
use human_money_core::UserIdentity;
use human_money_core::ValidatedUserId;
use humoco_market::{
    build_emergency_kit_uri, build_recovery_request_uri, canonicalize_owner_account_id,
    code_to_mnemonic_language, compute_recovery_sas, create_social_recovery_vault_bundle,
    encode_qr_svg, mnemonic_language_to_code, open_and_verify_guardian_release_reply,
    parse_emergency_kit_uri, parse_recovery_request_uri, recover_master_payload_from_shares,
    seal_guardian_release_reply, seal_post_box, DropDescriptor, GuardianLocatorUpdate,
    GuardianReleasePayload, GuardianRevoke, GuardianVaultEntry, GuardianVaultStore, PostEnvelope,
    RecoveryKdfParams, RecoveryMasterPayload, PAYLOAD_GUARDIAN_ASSIGN_V1,
    PAYLOAD_GUARDIAN_LOCATOR_V1, PAYLOAD_GUARDIAN_REVOKE_V1,
};

use crate::models::{
    ActiveRecoverySessionDto, ActiveRecoverySessionFile, ApproveGuardianRecoveryResultDto,
    ConfirmEmergencySheetResultDto, ExportedMnemonicKitDto, FrontendContact, FrontendError,
    GuardianAssignmentRecord, GuardianAssignmentStatus, GuardianRecoveryRequestInspectionDto,
    GuardianStatusDto, GuardianWardDto, InspectedEmergencyKitDto, RecoveryCompletionResultDto,
    RecoveryOwnerState, RecoveryProtectionStatusDto, RehearsalCheckResultDto,
    RevokeSocialRecoveryResultDto, SetupSocialRecoveryResultDto,
};
use crate::services::{
    backup_service::{
        finalize_restored_wallet_and_reconcile_l2, find_local_profile_collision_impl,
        probe_sync_backup_impl, sync_backup_impl, user_ids_from_metadata_dir,
    },
    common::are_dids_equal,
    fs_helpers::{record_profile_metadata_in_dir, write_file_atomic},
    post_service::{
        collect_own_reply_syncs, flush_post_slots_impl, load_post_state, merge_and_save_post_state,
        sync_post_channels_impl,
    },
    sync_engine::save_public_sync_impl,
};
use crate::state::{
    AppState, PostOutboxEntry, StoredPostState, RECOVERY_GUARDIAN_VAULT_KEY,
    RECOVERY_OWNER_STATE_KEY,
};

const REHEARSAL_INTERVAL_SECS: [u64; 5] =
    [86_400, 7 * 86_400, 30 * 86_400, 90 * 86_400, 180 * 86_400];

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
            if password.is_none() {
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
            if password.is_none() {
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

fn save_post_and_recovery_owner_atomic(
    state: &AppState,
    session: &crate::session::SessionToken,
    baseline_post: &StoredPostState,
    working_post: &mut StoredPostState,
    owner_state: &RecoveryOwnerState,
    password: Option<&str>,
    conflict_context: &str,
) -> Result<(), FrontendError> {
    state.with_session_required(session, conflict_context, |service| {
        crate::services::post_service::merge_post_state_with_service(
            service,
            baseline_post,
            working_post,
            password,
        )?;
        let post_bytes =
            serde_json::to_vec(working_post).map_err(|e| FrontendError::from(e.to_string()))?;
        let owner_bytes =
            serde_json::to_vec(owner_state).map_err(|e| FrontendError::from(e.to_string()))?;
        service
            .save_encrypted_batch(
                &[
                    (crate::state::MARKET_POST_STATE_KEY, &post_bytes),
                    (RECOVERY_OWNER_STATE_KEY, &owner_bytes),
                ],
                password,
            )
            .map_err(FrontendError::from)?;
        state.set_cached_recovery_owner_for_service(service, Some(Some(owner_state.clone())));
        Ok(())
    })
}

fn is_same_account_id(a: &str, b: &str) -> bool {
    match (
        canonicalize_owner_account_id(a),
        canonicalize_owner_account_id(b),
    ) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a.trim() == b.trim(),
    }
}

pub fn collect_candidate_backup_sync_descriptors(
    state: &AppState,
    password: Option<&str>,
) -> Result<Vec<DropDescriptor>, FrontendError> {
    let security_policy = state.get_or_load_security_policy_locked(password)?;
    let mut candidate_targets: Vec<DropDescriptor> = security_policy
        .market_selected_syncs
        .iter()
        .cloned()
        .map(Into::into)
        .collect();

    let (admin_opt, my_prefix) = {
        let mut service = state.lock_service()?;
        let pfx = service
            .with_wallet_and_identity(|_w, i| {
                human_money_core::crypto::get_prefix_from_user_id(&i.user_id).map(|s| s.to_string())
            })
            .ok()
            .flatten();
        let admin = match crate::services::sync_engine::admin::load_stored_sync_admin_opt(
            &mut service,
            password,
        ) {
            Ok(admin) => admin,
            Err(err)
                if password.is_none()
                    && (err.code == "error.auth.sessionNotActive"
                        || err.code == "error.auth.sessionExpired") =>
            {
                None
            }
            Err(err) => return Err(err),
        };
        (admin, pfx)
    };
    if let Some(admin) = admin_opt {
        if !admin.descriptor.write_url.is_empty()
            && !candidate_targets
                .iter()
                .any(|d| d.write_url == admin.descriptor.write_url)
        {
            candidate_targets.push(admin.descriptor);
        }
    }

    for pd in &security_policy.public_syncs {
        if pd.enabled
            && !pd.write_url.is_empty()
            && !candidate_targets
                .iter()
                .any(|d| d.write_url == pd.write_url)
        {
            let slot_id = humoco_market::format_slot_id(&pd.access_pubkey, my_prefix.as_deref())
                .unwrap_or_else(|_| pd.access_pubkey.clone());
            candidate_targets.push(DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(slot_id),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }

    Ok(candidate_targets)
}

fn seal_recovery_container(
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
fn enqueue_recovery_container_to_outbox(
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

    post_state.processed_msg_ids.insert(msg_id.clone());
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

fn enqueue_guardian_revocations(
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
    password: Option<String>,
) -> Result<usize, FrontendError> {
    let _recovery_guard = state.recovery_op_lock.lock().await;
    let (session, my_identity, guardians, epoch) = {
        let mut service = state.lock_service()?;
        let session = state.session_token_for_service(&service)?;
        let owner_state =
            load_recovery_owner_state_with_cache(state, &mut service, password.as_deref())?;
        let Some(owner) = owner_state else {
            return Ok(0);
        };
        if owner.guardians.is_empty() {
            return Ok(0);
        }
        let id = service
            .with_wallet_and_identity(|_w, i| i.clone())
            .map_err(FrontendError::from)?;
        (session, id, owner.guardians, owner.rotation_epoch)
    };

    let candidate_descriptors =
        collect_candidate_backup_sync_descriptors(state, password.as_deref())?;
    let security_policy = state.get_or_load_security_policy_locked(password.as_deref())?;
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
    let now_secs = Utc::now().timestamp() as u64;
    let mut post_state = load_post_state(state, password.as_deref())?;
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
        if !state.is_generation_current(session.generation) {
            return Ok(0);
        }
        merge_and_save_post_state(
            state,
            &session,
            &baseline,
            &mut post_state,
            password.as_deref(),
        )?;
        baseline = post_state.clone();
        let _ = flush_post_slots_impl(
            state,
            &session,
            &mut post_state,
            None,
            &[],
            password.as_deref(),
        )
        .await;
        if !state.is_generation_current(session.generation) {
            return Ok(0);
        }
        merge_and_save_post_state(
            state,
            &session,
            &baseline,
            &mut post_state,
            password.as_deref(),
        )?;
    }

    Ok(enqueued)
}

// ============================================================================
// Active Session Helpers
// ============================================================================

pub(crate) fn get_session_file_path(app: &tauri::AppHandle) -> Result<PathBuf, FrontendError> {
    use tauri::Manager;
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let tmp_dir = app_dir.join("tmp_restore");
    fs::create_dir_all(&tmp_dir).map_err(|e| FrontendError::from(e.to_string()))?;
    Ok(tmp_dir.join("active_recovery_session.json"))
}

pub(crate) fn load_active_session_from_path(
    path: &Path,
) -> Result<Option<ActiveRecoverySessionFile>, FrontendError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|e| {
        FrontendError::from(format!("Failed to read active recovery session: {}", e))
    })?;
    serde_json::from_slice::<ActiveRecoverySessionFile>(&bytes)
        .map(Some)
        .map_err(|e| FrontendError::from(format!("Failed to parse active recovery session: {}", e)))
}

pub(crate) fn save_active_session_to_path(
    path: &Path,
    session: &ActiveRecoverySessionFile,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec_pretty(session).map_err(|e| {
        FrontendError::from(format!(
            "Failed to serialize active recovery session: {}",
            e
        ))
    })?;
    write_file_atomic(path, &bytes)
}

pub(crate) fn delete_active_session_from_path(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn format_contact_display_name(contact: &FrontendContact) -> Option<String> {
    let first = contact.profile.first_name.as_deref().unwrap_or("").trim();
    let last = contact.profile.last_name.as_deref().unwrap_or("").trim();
    let full = format!("{} {}", first, last).trim().to_string();
    (!full.is_empty()).then_some(full)
}

fn resolve_ward_display_name(
    owner_did: &str,
    fallback_name: &str,
    contacts: &[FrontendContact],
) -> String {
    let trimmed_owner = owner_did.trim();
    if let Some(exact_name) = contacts
        .iter()
        .find(|c| c.did.trim() == trimmed_owner)
        .and_then(format_contact_display_name)
    {
        return exact_name;
    }

    let trimmed_fallback = fallback_name.trim();
    let is_prefixed_subaccount =
        human_money_core::crypto::get_prefix_from_user_id(trimmed_owner).is_some();
    if is_prefixed_subaccount && !trimmed_fallback.is_empty() && trimmed_fallback != trimmed_owner {
        return trimmed_fallback.to_string();
    }

    contacts
        .iter()
        .find(|c| are_dids_equal(&c.did, trimmed_owner))
        .and_then(format_contact_display_name)
        .unwrap_or_else(|| fallback_name.to_string())
}

pub(crate) fn build_active_session_dto(
    session: &ActiveRecoverySessionFile,
) -> ActiveRecoverySessionDto {
    let mut groups: BTreeMap<(String, u32), Vec<&GuardianReleasePayload>> = BTreeMap::new();
    for reply in &session.replies {
        groups
            .entry((reply.entry.owner_did.clone(), reply.entry.rotation_epoch))
            .or_default()
            .push(reply);
    }

    let best_group = groups
        .into_iter()
        .max_by_key(|((_, epoch), list)| (*epoch, list.len()));

    if let Some(((_, epoch), list)) = best_group {
        let first = list[0];
        let threshold_m = first.entry.threshold_m;
        let total_n = first.entry.total_n;
        let mut collected_share_indices: Vec<u8> =
            list.iter().map(|r| r.entry.share.index).collect();
        collected_share_indices.sort_unstable();
        let mut discovered_sync_urls = Vec::new();
        for r in &list {
            for d in r
                .entry
                .backup_sync_descriptors
                .iter()
                .chain(r.extra_sync_descriptors.iter())
            {
                if !d.write_url.is_empty() && !discovered_sync_urls.contains(&d.write_url) {
                    discovered_sync_urls.push(d.write_url.clone());
                }
            }
        }

        ActiveRecoverySessionDto {
            sas_code: session.sas_code.clone(),
            created_at: session.created_at,
            name_hint: session.name_hint.clone(),
            request_uri: session.request_uri.clone(),
            session_mode: session.session_mode.clone(),
            target_folder_name: session.target_folder_name.clone(),
            owner_did: Some(first.entry.owner_did.clone()),
            owner_display_name: Some(first.entry.owner_display_name.clone()),
            rotation_epoch: Some(epoch),
            threshold_m: Some(threshold_m),
            total_n: Some(total_n),
            collected_count: list.len(),
            collected_share_indices,
            question_ids: Some(first.entry.question_ids),
            has_enough_shares: list.len() >= threshold_m as usize,
            discovered_sync_urls,
            last_import_status: None,
        }
    } else {
        ActiveRecoverySessionDto {
            sas_code: session.sas_code.clone(),
            created_at: session.created_at,
            name_hint: session.name_hint.clone(),
            request_uri: session.request_uri.clone(),
            session_mode: session.session_mode.clone(),
            target_folder_name: session.target_folder_name.clone(),
            owner_did: None,
            owner_display_name: None,
            rotation_epoch: None,
            threshold_m: None,
            total_n: None,
            collected_count: 0,
            collected_share_indices: Vec::new(),
            question_ids: None,
            has_enough_shares: false,
            discovered_sync_urls: Vec::new(),
            last_import_status: None,
        }
    }
}

fn core_to_frontend_language(lang: MnemonicLanguage) -> crate::models::MnemonicLanguage {
    match lang {
        MnemonicLanguage::English => crate::models::MnemonicLanguage::English,
        MnemonicLanguage::German => crate::models::MnemonicLanguage::German,
        MnemonicLanguage::Spanish => crate::models::MnemonicLanguage::Spanish,
        MnemonicLanguage::French => crate::models::MnemonicLanguage::French,
        MnemonicLanguage::Italian => crate::models::MnemonicLanguage::Italian,
        MnemonicLanguage::Japanese => crate::models::MnemonicLanguage::Japanese,
        MnemonicLanguage::Korean => crate::models::MnemonicLanguage::Korean,
        MnemonicLanguage::Portuguese => crate::models::MnemonicLanguage::Portuguese,
        MnemonicLanguage::Czech => crate::models::MnemonicLanguage::Czech,
        MnemonicLanguage::ChineseSimplified => crate::models::MnemonicLanguage::ChineseSimplified,
        MnemonicLanguage::ChineseTraditional => crate::models::MnemonicLanguage::ChineseTraditional,
    }
}

pub fn inspect_emergency_kit_uri_impl(
    uri: String,
) -> Result<InspectedEmergencyKitDto, FrontendError> {
    let kit = parse_emergency_kit_uri(&uri).map_err(FrontendError::from)?;
    let entropy_bytes =
        hex::decode(&kit.entropy_hex).map_err(|e| FrontendError::from(e.to_string()))?;
    let core_lang = code_to_mnemonic_language(kit.language_code).map_err(FrontendError::from)?;
    let phrase = human_money_core::services::mnemonic::MnemonicProcessor::from_entropy(
        &entropy_bytes,
        core_lang,
    )
    .map_err(|e| FrontendError::from(e.to_string()))?;
    let word_count = phrase.split_whitespace().count();
    let sync_urls: Vec<String> = kit
        .sync_descriptors
        .iter()
        .filter(|d| !d.write_url.trim().is_empty())
        .map(|d| d.write_url.clone())
        .collect();
    let sync_descriptors_json: Vec<String> = kit
        .sync_descriptors
        .iter()
        .filter(|d| !d.write_url.trim().is_empty())
        .filter_map(|d| serde_json::to_string(d).ok())
        .collect();

    Ok(InspectedEmergencyKitDto {
        phrase,
        word_count,
        language: core_to_frontend_language(core_lang),
        has_bip39_passphrase: kit.has_bip39_passphrase,
        user_prefix: kit.user_prefix,
        sync_urls,
        sync_descriptors_json,
    })
}

// ============================================================================
// Service Implementations
// ============================================================================

pub async fn export_profile_mnemonic_impl(
    password: String,
    state: &AppState,
) -> Result<Option<ExportedMnemonicKitDto>, FrontendError> {
    let (phrase, lang, has_bip39_passphrase, user_prefix) = {
        let service =
            state.authorize_and_lock(crate::state::RequiredAccess::Full, Some(&password))?;
        let exported = service
            .export_mnemonic(&password)
            .map_err(FrontendError::from)?;
        let Some((phrase, lang, has_bip39_passphrase)) = exported else {
            return Ok(None);
        };
        let user_prefix = service
            .with_wallet_and_identity(|_w, id| {
                human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string())
            })
            .ok()
            .flatten();
        (phrase, lang, has_bip39_passphrase, user_prefix)
    };

    let entropy_bytes =
        human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(&phrase, lang)
            .map_err(|e| FrontendError::from(e.to_string()))?;
    let entropy_hex = hex::encode(&entropy_bytes);

    let active_descriptors = collect_candidate_backup_sync_descriptors(state, Some(&password))?;

    let guardian_names = {
        let mut service = state.lock_service()?;
        let owner_state = load_recovery_owner_state(&mut service, Some(&password))?;
        owner_state
            .map(|s| s.guardians.into_iter().map(|g| g.display_name).collect())
            .unwrap_or_default()
    };

    let kit = humoco_market::EmergencyKitData {
        version: 1,
        entropy_hex,
        language_code: mnemonic_language_to_code(lang),
        user_prefix: user_prefix.clone(),
        has_bip39_passphrase,
        sync_descriptors: active_descriptors.clone(),
    };

    let kit_uri = build_emergency_kit_uri(&kit).map_err(FrontendError::from)?;
    let qr_svg = encode_qr_svg(kit_uri.as_bytes()).map_err(FrontendError::from)?;
    let sync_urls = active_descriptors
        .into_iter()
        .map(|d| d.write_url)
        .collect();

    Ok(Some(ExportedMnemonicKitDto {
        phrase,
        language_code: kit.language_code,
        has_bip39_passphrase,
        user_prefix,
        kit_uri,
        qr_svg,
        sync_urls,
        guardian_names,
    }))
}

pub async fn confirm_emergency_sheet_saved_impl(
    password: String,
    word_index: u8,
    control_word: String,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
) -> Result<ConfirmEmergencySheetResultDto, FrontendError> {
    let phrase = {
        let service =
            state.authorize_and_lock(crate::state::RequiredAccess::Full, Some(&password))?;
        let exp = service
            .export_mnemonic(&password)
            .map_err(FrontendError::from)?;
        let (ph, _, _) = exp.ok_or_else(|| {
            FrontendError::from(
                "Wiederherstellungsphrase nicht auf diesem Gerät gespeichert.".to_string(),
            )
        })?;
        ph
    };

    let words: Vec<&str> = phrase.split_whitespace().collect();
    if word_index < 1 || word_index as usize > words.len() {
        return Err(FrontendError::from(format!(
            "Ungültiger Wortindex {}: muss zwischen 1 und {} liegen",
            word_index,
            words.len()
        )));
    }

    let expected = words[(word_index - 1) as usize];
    if control_word.trim().to_lowercase() != expected.trim().to_lowercase() {
        return Err(FrontendError::from(
            "Das Kontrollwort stimmt nicht mit deinem Notfall-Blatt überein.".to_string(),
        ));
    }

    let now_secs = Utc::now().timestamp() as u64;
    {
        let _recovery_guard = state.recovery_op_lock.lock().await;
        let mut service = state.lock_service()?;
        let mut owner_state =
            load_recovery_owner_state_with_cache(state, &mut service, Some(&password))?
                .unwrap_or_default();
        owner_state.emergency_sheet_confirmed_at = Some(now_secs);
        save_recovery_owner_state_with_cache(state, &mut service, &owner_state, Some(&password))?;
    }

    let backup_sync_result = match app {
        Some(handle) => sync_backup_impl(Some(password), None, state, handle)
            .await
            .ok(),
        None => None,
    };

    Ok(ConfirmEmergencySheetResultDto {
        confirmed_at: now_secs,
        backup_sync_result,
    })
}

pub fn get_recovery_protection_status_impl(
    password: Option<&str>,
    state: &AppState,
) -> Result<RecoveryProtectionStatusDto, FrontendError> {
    let now_secs = Utc::now().timestamp() as u64;

    let (has_voucher, owner_state, guardian_vault, contacts) = {
        let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
        let has_voucher = service
            .with_wallet(|w| !w.voucher_store.vouchers.is_empty())
            .unwrap_or_else(|_| {
                state
                    .get_cached_history()
                    .map(|h| !h.is_empty())
                    .unwrap_or(false)
            });
        let owner = load_recovery_owner_state_with_cache(state, &mut service, password)?
            .unwrap_or_default();
        let vault = load_guardian_vault_with_cache(state, &mut service, password)?;
        let contacts = state
            .load_contacts(&mut service, password)
            .or_else(|_| state.get_cached_contacts())
            .unwrap_or_default();
        (has_voucher, owner, vault, contacts)
    };

    let confirmed_guardians_count = owner_state
        .guardians
        .iter()
        .filter(|g| matches!(g.status, GuardianAssignmentStatus::Confirmed { .. }))
        .count();

    let required_threshold = owner_state.threshold_m;

    let guardians: Vec<GuardianStatusDto> = owner_state
        .guardians
        .into_iter()
        .map(|g| {
            let last_contacted_at = match g.status {
                GuardianAssignmentStatus::Confirmed { acked_at } => Some(acked_at),
                _ => None,
            };
            let last_activity = last_contacted_at.unwrap_or(g.assigned_at);
            let is_stale_90d = now_secs.saturating_sub(last_activity) > 90 * 86400;
            GuardianStatusDto {
                user_id: g.user_id,
                display_name: g.display_name,
                share_index: g.share_index,
                status: g.status,
                assigned_at: g.assigned_at,
                last_contacted_at,
                is_stale_90d,
                updated_at: g.sealed_entry.updated_at,
            }
        })
        .collect();

    let emergency_sheet_confirmed_at = owner_state.emergency_sheet_confirmed_at;
    let has_identity_recovery = (confirmed_guardians_count >= required_threshold as usize
        && required_threshold >= 2)
        || emergency_sheet_confirmed_at.is_some();

    let security_policy = state.get_or_load_security_policy_locked(password)?;
    let lifecycle = state.get_or_load_internal_lifecycle_locked(password)?;
    let candidate_syncs = collect_candidate_backup_sync_descriptors(state, password)?;
    let sync_backup_count = candidate_syncs.len();
    let has_sync_backup =
        security_policy.sync_backup_enabled && lifecycle.last_sync_backup_at.is_some();
    let has_local_backup = security_policy
        .backup_sync_folder
        .as_ref()
        .is_some_and(|f| !f.trim().is_empty() && Path::new(f).is_dir());
    let has_voucher_backup = has_sync_backup || has_local_backup;

    let guardian_wards: Vec<GuardianWardDto> = guardian_vault
        .wards
        .values()
        .map(|e| GuardianWardDto {
            owner_did: e.owner_did.clone(),
            owner_display_name: resolve_ward_display_name(
                &e.owner_did,
                &e.owner_display_name,
                &contacts,
            ),
            rotation_epoch: e.rotation_epoch,
            threshold_m: e.threshold_m,
            total_n: e.total_n,
            updated_at: e.updated_at,
        })
        .collect();

    let rehearsal_due = !guardians.is_empty()
        && owner_state.answer_verifiers.is_some()
        && owner_state
            .next_rehearsal_at
            .is_some_and(|ts| now_secs >= ts);
    let question_ids = (!guardians.is_empty()).then_some(owner_state.question_ids);
    let rehearsal_stage = owner_state.rehearsal_stage;
    let next_rehearsal_at = owner_state.next_rehearsal_at;

    Ok(RecoveryProtectionStatusDto {
        has_voucher,
        confirmed_guardians_count,
        required_threshold,
        guardians,
        emergency_sheet_confirmed_at,
        has_identity_recovery,
        sync_backup_count,
        has_sync_backup,
        has_local_backup,
        has_voucher_backup,
        guardian_wards,
        rehearsal_due,
        question_ids,
        rehearsal_stage,
        next_rehearsal_at,
    })
}

pub fn verify_recovery_rehearsal_with_password_impl(
    answers: [String; 3],
    now_secs: u64,
    password: Option<&str>,
    state: &AppState,
) -> Result<RehearsalCheckResultDto, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::DailyPay, password)?;

    let mut owner_state = load_recovery_owner_state_with_cache(state, &mut service, password)?
        .ok_or_else(|| {
            FrontendError::from("Social recovery is not configured for this profile.".to_string())
        })?;
    let expected = owner_state.answer_verifiers.ok_or_else(|| {
        FrontendError::from(
            "Recovery rehearsal verifiers are not configured. Please update your recovery questions."
                .to_string(),
        )
    })?;

    let user_id = service
        .with_wallet_and_identity(|_, id| id.user_id.clone())
        .map_err(FrontendError::from)?;
    let owner_account_id = canonicalize_owner_account_id(&user_id).map_err(FrontendError::from)?;

    let kdf_params = RecoveryKdfParams {
        m_kib: owner_state.rehearsal_kdf_m_kib,
        t_cost: owner_state.rehearsal_kdf_t_cost,
        p_cost: 1,
        salt: [0u8; 16],
    };
    let ans_refs = [
        answers[0].as_str(),
        answers[1].as_str(),
        answers[2].as_str(),
    ];
    let question_matches = humoco_market::verify_rehearsal_answers(
        &owner_account_id,
        owner_state.rotation_epoch,
        &owner_state.question_ids,
        &ans_refs,
        &expected,
        &kdf_params,
    )?;

    let all_correct = question_matches.iter().all(|&m| m);
    if all_correct {
        let next_stage = owner_state
            .rehearsal_stage
            .saturating_add(1)
            .min((REHEARSAL_INTERVAL_SECS.len() - 1) as u8);
        let interval = REHEARSAL_INTERVAL_SECS[next_stage as usize];
        let next_at = now_secs.saturating_add(interval);
        owner_state.rehearsal_stage = next_stage;
        owner_state.next_rehearsal_at = Some(next_at);
        save_recovery_owner_state_with_cache(state, &mut service, &owner_state, password)?;
        Ok(RehearsalCheckResultDto {
            all_correct: true,
            question_matches,
            rehearsal_stage: next_stage,
            next_rehearsal_at: next_at,
        })
    } else {
        Ok(RehearsalCheckResultDto {
            all_correct: false,
            question_matches,
            rehearsal_stage: owner_state.rehearsal_stage,
            next_rehearsal_at: owner_state.next_rehearsal_at.unwrap_or(now_secs),
        })
    }
}

pub fn verify_recovery_rehearsal_impl(
    answers: [String; 3],
    now_secs: u64,
    state: &AppState,
) -> Result<RehearsalCheckResultDto, FrontendError> {
    verify_recovery_rehearsal_with_password_impl(answers, now_secs, None, state)
}

pub fn snooze_recovery_rehearsal_with_password_impl(
    hours: Option<u64>,
    now_secs: u64,
    password: Option<&str>,
    state: &AppState,
) -> Result<u64, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::DailyPay, password)?;

    let mut owner_state = load_recovery_owner_state_with_cache(state, &mut service, password)?
        .ok_or_else(|| {
            FrontendError::from("Social recovery is not configured for this profile.".to_string())
        })?;
    let snooze_secs = hours.unwrap_or(24).clamp(1, 168) * 3600;
    let next_at = now_secs.saturating_add(snooze_secs);
    owner_state.next_rehearsal_at = Some(next_at);
    save_recovery_owner_state_with_cache(state, &mut service, &owner_state, password)?;
    Ok(next_at)
}

pub fn snooze_recovery_rehearsal_impl(
    hours: Option<u64>,
    now_secs: u64,
    state: &AppState,
) -> Result<u64, FrontendError> {
    snooze_recovery_rehearsal_with_password_impl(hours, now_secs, None, state)
}

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

    // Check pairwise distinct
    let mut unique_dids = std::collections::HashSet::new();
    for did in &guardian_dids {
        if !unique_dids.insert(did) {
            return Err(FrontendError::from(format!(
                "Duplicate guardian DID in setup: {}",
                did
            )));
        }
    }

    // Serialize concurrent setup/revocation operations under `recovery_op_lock` BEFORE reading epoch & committing.
    let (new_epoch, guardians_count) = {
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
                FrontendError::from(
                    "Mnemonic phrase required to setup social recovery".to_string(),
                )
            })?;
            let contacts = state.load_contacts(&mut service, Some(&password))?;

            for g_did in &guardian_dids {
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

            let old_owner_state =
                load_recovery_owner_state_with_cache(state, &mut service, Some(&password))?;
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

        let entropy =
            human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(&phrase, lang)
                .map_err(|e| FrontendError::from(e.to_string()))?;

        let old_epoch = old_owner_state
            .as_ref()
            .map(|s| s.rotation_epoch)
            .unwrap_or(0);
        let new_epoch = old_epoch + 1;
        let now_secs = Utc::now().timestamp() as u64;

        let security_policy = state.get_or_load_security_policy_locked(Some(password.as_str()))?;
        let reply_drops: Vec<DropDescriptor> = collect_own_reply_syncs(
            &security_policy,
            human_money_core::crypto::get_prefix_from_user_id(&my_identity.user_id),
        )
        .into_iter()
        .map(Into::into)
        .collect();

        let owner_account_id =
            canonicalize_owner_account_id(&my_identity.user_id).map_err(FrontendError::from)?;
        let backup_descriptors =
            collect_candidate_backup_sync_descriptors(state, Some(&password))?;

        let master_payload = RecoveryMasterPayload::new(
            &owner_account_id,
            entropy,
            lang,
            my_prefix,
            has_bip39_passphrase,
        );

        let ans_refs = [
            answers[0].as_str(),
            answers[1].as_str(),
            answers[2].as_str(),
        ];

        let entries = create_social_recovery_vault_bundle(
            &my_identity.signing_key,
            &owner_account_id,
            &my_profile_name,
            new_epoch,
            threshold_m,
            guardian_dids.len() as u8,
            &question_ids,
            &ans_refs,
            &kdf_params,
            &master_payload,
            &backup_descriptors,
            now_secs,
        )
        .map_err(FrontendError::from)?;

        let answer_verifiers = Some(
            humoco_market::compute_rehearsal_verifiers(
                &owner_account_id,
                new_epoch,
                &question_ids,
                &ans_refs,
                &kdf_params,
            )
            .map_err(FrontendError::from)?,
        );

        let mut assignment_records = Vec::with_capacity(guardian_dids.len());
        {
            let _post_guard = state.post_op_lock.lock().await;
            let mut post_state = load_post_state(state, Some(&password))?;
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
                next_rehearsal_at: Some(now_secs + REHEARSAL_INTERVAL_SECS[0]),
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

        (new_epoch, guardian_dids.len())
    };

    let post_sync_result =
        sync_post_channels_impl(app, None, None, Some(password.clone()), state).await?;
    let backup_sync_result = match app {
        Some(handle) => sync_backup_impl(Some(password), None, state, handle)
            .await
            .ok(),
        None => None,
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
    let (new_epoch, revoked_count) = {
        let _recovery_guard = state.recovery_op_lock.lock().await;
        let (session, my_identity, old_guardians, old_epoch, emergency_confirmed) = {
            let mut service = state
                .authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
            let session = state.session_token_for_service(&service)?;
            let owner =
                load_recovery_owner_state_with_cache(state, &mut service, password.as_deref())?
                    .unwrap_or_default();
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
        let now_secs = Utc::now().timestamp() as u64;
        let security_policy = state.get_or_load_security_policy_locked(password.as_deref())?;
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
            let mut post_state = load_post_state(state, password.as_deref())?;
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

        (new_epoch, revoked_count)
    };

    let post_sync_result =
        sync_post_channels_impl(app, None, None, password.clone(), state).await?;
    let backup_sync_result = match app {
        Some(handle) => sync_backup_impl(password, None, state, handle).await.ok(),
        None => None,
    };

    Ok(RevokeSocialRecoveryResultDto {
        rotation_epoch: new_epoch,
        revoked_guardians_count: revoked_count,
        post_sync_result,
        backup_sync_result,
    })
}

// ============================================================================
// Disaster Mode Session Logic (Pre-login compatible)
// ============================================================================

pub fn start_social_recovery_session_impl(
    name_hint: Option<String>,
    session_mode: Option<String>,
    target_folder_name: Option<String>,
    session_file_path: &Path,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let sk_temp = x25519_dalek::StaticSecret::random_from_rng(rand::rngs::OsRng);
    let pk_temp = x25519_dalek::PublicKey::from(&sk_temp);

    let sas_code = compute_recovery_sas(pk_temp.as_bytes());
    let now_secs = Utc::now().timestamp() as u64;
    let request_uri =
        build_recovery_request_uri(pk_temp.as_bytes(), now_secs, name_hint.as_deref());
    let resolved_mode = match session_mode.as_deref() {
        Some("reset_local_password") => "reset_local_password".to_string(),
        _ => "restore_new_device".to_string(),
    };

    let session = ActiveRecoverySessionFile {
        sk_temp: sk_temp.to_bytes(),
        pk_temp: *pk_temp.as_bytes(),
        sas_code,
        created_at: now_secs,
        name_hint,
        request_uri,
        session_mode: resolved_mode,
        target_folder_name,
        replies: Vec::new(),
    };

    save_active_session_to_path(session_file_path, &session)?;
    Ok(build_active_session_dto(&session))
}

pub fn get_active_recovery_session_impl(
    session_file_path: &Path,
) -> Result<Option<ActiveRecoverySessionDto>, FrontendError> {
    let session_opt = load_active_session_from_path(session_file_path)?;
    Ok(session_opt.as_ref().map(build_active_session_dto))
}

pub fn cancel_social_recovery_session_impl(session_file_path: &Path) -> Result<(), FrontendError> {
    delete_active_session_from_path(session_file_path);
    Ok(())
}

pub fn import_guardian_recovery_reply_impl(
    reply_uri: String,
    session_file_path: &Path,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let mut session = load_active_session_from_path(session_file_path)?.ok_or_else(|| {
        FrontendError::from("Keine aktive Wiederherstellungssitzung gefunden.".to_string())
    })?;

    let payload = open_and_verify_guardian_release_reply(&session.sk_temp, &reply_uri)
        .map_err(FrontendError::from)?;

    let mut superseded_epoch = false;
    if !session.replies.is_empty() {
        if !is_same_account_id(
            &session.replies[0].entry.owner_did,
            &payload.entry.owner_did,
        ) {
            return Err(FrontendError::from(format!(
                "Diese Freigabe gehört zu einem anderen Profil ({}) als die bereits gesammelten Freigaben ({}). Bitte prüfe, ob dein Kontakt das richtige Profil ausgewählt hat.",
                payload.entry.owner_display_name, session.replies[0].entry.owner_display_name
            )));
        }
        let max_epoch = session
            .replies
            .iter()
            .map(|r| r.entry.rotation_epoch)
            .max()
            .unwrap_or(0);
        if payload.entry.rotation_epoch < max_epoch {
            return Err(FrontendError::from(format!(
                "Dieser Kontakt hat noch einen veralteten Wiederherstellungs-Stand (Epoche {} statt {}). Bitte ihn, kurz sein Postfach zu synchronisieren und erneut freizugeben.",
                payload.entry.rotation_epoch, max_epoch
            )));
        }
        if payload.entry.rotation_epoch > max_epoch {
            superseded_epoch = !session.replies.is_empty();
            session
                .replies
                .retain(|r| r.entry.rotation_epoch == payload.entry.rotation_epoch);
        }
    }

    // Deduplicate reply
    let already_present = session.replies.iter().any(|r| {
        is_same_account_id(&r.entry.owner_did, &payload.entry.owner_did)
            && r.entry.rotation_epoch == payload.entry.rotation_epoch
            && r.entry.share.index == payload.entry.share.index
    });

    if !already_present {
        session.replies.push(payload);
        save_active_session_to_path(session_file_path, &session)?;
    }

    let mut dto = build_active_session_dto(&session);
    dto.last_import_status = Some(if superseded_epoch {
        "superseded_epoch".to_string()
    } else if already_present {
        "duplicate".to_string()
    } else {
        "added".to_string()
    });
    Ok(dto)
}

// ============================================================================
// Friend-Side Logic (Logged in)
// ============================================================================

pub fn inspect_guardian_recovery_request_impl(
    request_uri: String,
    password: Option<&str>,
    state: &AppState,
) -> Result<GuardianRecoveryRequestInspectionDto, FrontendError> {
    let link = parse_recovery_request_uri(&request_uri).map_err(FrontendError::from)?;
    let now_secs = Utc::now().timestamp() as u64;
    let is_stale_48h = link.is_stale_48h(now_secs);
    let sas_code = compute_recovery_sas(&link.pk_temp);

    let (vault, contacts) = {
        let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
        let vault = load_guardian_vault_with_cache(state, &mut service, password)?;
        let contacts = state
            .load_contacts(&mut service, password)
            .or_else(|_| state.get_cached_contacts())
            .unwrap_or_default();
        (vault, contacts)
    };

    let wards: Vec<GuardianWardDto> = vault
        .wards
        .values()
        .map(|e| GuardianWardDto {
            owner_did: e.owner_did.clone(),
            owner_display_name: resolve_ward_display_name(
                &e.owner_did,
                &e.owner_display_name,
                &contacts,
            ),
            rotation_epoch: e.rotation_epoch,
            threshold_m: e.threshold_m,
            total_n: e.total_n,
            updated_at: e.updated_at,
        })
        .collect();

    Ok(GuardianRecoveryRequestInspectionDto {
        sas_code,
        requested_at: link.ts,
        is_stale_48h,
        name_hint: link.name_hint,
        wards,
    })
}

pub fn approve_guardian_recovery_request_impl(
    request_uri: String,
    owner_did: String,
    password: Option<&str>,
    state: &AppState,
) -> Result<ApproveGuardianRecoveryResultDto, FrontendError> {
    let link = parse_recovery_request_uri(&request_uri).map_err(FrontendError::from)?;
    let sas_code = compute_recovery_sas(&link.pk_temp);

    let (entry, extra_descriptors, resolved_owner_name) = {
        let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Full, password)?;
        let vault = load_guardian_vault_with_cache(state, &mut service, password)?;
        let entry = vault
            .wards
            .values()
            .find(|e| is_same_account_id(&e.owner_did, &owner_did))
            .cloned()
            .ok_or_else(|| {
                FrontendError::from("Kein Schützling mit dieser ID im Tresor gefunden.".to_string())
            })?;
        let contacts = state
            .load_contacts(&mut service, password)
            .unwrap_or_default();
        let resolved_name =
            resolve_ward_display_name(&entry.owner_did, &entry.owner_display_name, &contacts);

        // Collect extra known sync descriptors from own hosted sync
        let mut extras = Vec::new();
        if let Some(admin) =
            crate::services::sync_engine::admin::load_stored_sync_admin_opt(&mut service, password)?
        {
            if !admin.descriptor.write_url.is_empty() {
                extras.push(admin.descriptor.clone());
            }
        }

        (entry, extras, resolved_name)
    };

    let now_secs = Utc::now().timestamp() as u64;
    let payload = GuardianReleasePayload {
        entry,
        extra_sync_descriptors: extra_descriptors,
        released_at: now_secs,
    };

    let reply_uri =
        seal_guardian_release_reply(&link.pk_temp, &payload).map_err(FrontendError::from)?;
    let reply_qr_svg = encode_qr_svg(reply_uri.as_bytes()).map_err(FrontendError::from)?;

    Ok(ApproveGuardianRecoveryResultDto {
        reply_uri,
        reply_qr_svg,
        sas_code,
        owner_display_name: resolved_owner_name,
    })
}

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
    let session = load_active_session_from_path(session_file_path)?.ok_or_else(|| {
        FrontendError::from("Keine aktive Wiederherstellungssitzung gefunden.".to_string())
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

    let ans_refs = [
        answers[0].as_str(),
        answers[1].as_str(),
        answers[2].as_str(),
    ];

    // NOTE: On error, session_file_path is NOT deleted so user can retry!
    let master_payload =
        recover_master_payload_from_shares(&matching_entries, &ans_refs).map_err(|e| {
            FrontendError::with_code(
                "error.recovery.wrongAnswers",
                format!(
                    "Deine Antworten stimmen noch nicht – bitte in Ruhe prüfen: {}",
                    e
                ),
            )
        })?;

    if master_payload.has_bip39_passphrase
        && bip39_passphrase.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(FrontendError::with_code(
            "error.recovery.bip39PassphraseRequired",
            "BIP39_PASSPHRASE_REQUIRED: Für dieses Profil wurde bei der Erstellung eine Passphrase (25. Wort) festgelegt.",
        ));
    }

    let lang = master_payload.language().map_err(FrontendError::from)?;
    let phrase = human_money_core::services::mnemonic::MnemonicProcessor::from_entropy(
        &master_payload.mnemonic_entropy,
        lang,
    )
    .map_err(|e| FrontendError::from(e.to_string()))?;

    if mode == "reset_local_password" {
        let target_folder = target_folder_name.ok_or_else(|| {
            FrontendError::from(
                "target_folder_name ist erforderlich für lokalen Passwort-Reset.".to_string(),
            )
        })?;

        let pending_lock = state.acquire_profile_lock(&target_folder)?;
        {
            let mut service = state.lock_service()?;
            service
                .recover_wallet_and_set_new_password(
                    &target_folder,
                    &phrase,
                    bip39_passphrase.as_deref(),
                    &new_password,
                    lang,
                    local_instance_id.clone(),
                )
                .map_err(FrontendError::from)?;

            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            let _ = record_profile_metadata_in_dir(wallet_data_dir, &target_folder, user_id);
            let _ = state.initialize_profile_session(&mut service, &new_password);
            state.commit_profile_lock(pending_lock);
            if let Some(app) = app_opt {
                if let Ok(config_dir) = app.path().app_config_dir() {
                    let _ = crate::pin_keystore::remove_entry(&config_dir, &target_folder);
                }
            }

            if let Ok(Some(mut local_state)) =
                load_recovery_owner_state_with_cache(state, &mut service, Some(&new_password))
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
        }

        if let Some(app) = app_opt {
            crate::services::common::spawn_pending_l2_reconciliation(app, &new_password);
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
        bip39_passphrase.as_deref().filter(|p| !p.is_empty()),
        master_payload.user_prefix.as_deref(),
        lang,
        "social_check".to_string(),
        false,
    )
    .map_err(FrontendError::from)?;

    let sync_inputs: Vec<String> = candidate_descriptors
        .iter()
        .filter_map(|d| serde_json::to_string(d).ok())
        .collect();
    let has_sync_inputs = !sync_inputs.is_empty();
    let fe_lang = core_to_frontend_language(lang);
    let allow_empty = allow_empty_fallback.unwrap_or(false);

    let (resolved_targets, local_collision) = if has_sync_inputs {
        let probe = probe_sync_backup_impl(
            sync_inputs,
            phrase.clone(),
            bip39_passphrase.clone(),
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
                    let _ = fs::remove_file(&staged_path);
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
                    passphrase: bip39_passphrase.as_deref(),
                    prefix: master_payload.user_prefix.as_deref(),
                    language: lang,
                };
                let standards_dir = app_data_dir.join("voucher_standards");
                let restore_outcome = finalize_restored_wallet_and_reconcile_l2(
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
                .await?;

                {
                    let mut service = state.lock_service()?;
                    if let Ok(Some(mut local_state)) = load_recovery_owner_state_with_cache(
                        state,
                        &mut service,
                        Some(&new_password),
                    ) {
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
                }

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
    let created_folder = {
        let mut service = state.lock_service()?;
        service
            .create_profile(
                &final_name,
                &phrase,
                bip39_passphrase.as_deref(),
                master_payload.user_prefix.as_deref(),
                &new_password,
                lang,
                local_instance_id,
                true,
            )
            .map_err(FrontendError::from)?;

        let folder = service
            .list_profiles()
            .ok()
            .and_then(|profiles| {
                profiles
                    .into_iter()
                    .find(|p| p.profile_name == final_name)
                    .map(|p| p.folder_name)
            })
            .unwrap_or_default();

        let _ = crate::state::save_encrypted_json(
            &mut service,
            crate::settings::UI_PREFERENCES_KEY,
            &crate::settings::UiPreferences::default(),
            Some(&new_password),
        );
        let _ = crate::state::save_encrypted_json(
            &mut service,
            crate::settings::SECURITY_POLICY_KEY,
            &crate::settings::SecurityPolicy::default(),
            Some(&new_password),
        );
        let _ = crate::state::save_local_sidecar_json(
            &mut service,
            crate::settings::INTERNAL_LIFECYCLE_STATE_KEY,
            &crate::settings::InternalLifecycleState::default(),
            Some(&new_password),
        );

        let initial_state = RecoveryOwnerState {
            rotation_epoch,
            ..Default::default()
        };
        let _ = save_recovery_owner_state_with_cache(
            state,
            &mut service,
            &initial_state,
            Some(&new_password),
        );

        if !folder.is_empty() {
            let _ = record_profile_metadata_in_dir(
                wallet_data_dir,
                &folder,
                Some(derived_identity.user_id.clone()),
            );
        }
        let _ = state.initialize_profile_session(&mut service, &new_password);
        if !folder.is_empty() {
            if let Ok(pending_lock) = state.acquire_profile_lock(&folder) {
                state.commit_profile_lock(pending_lock);
            }
        }
        folder
    };

    for target in &resolved_targets {
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

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FrontendContact, FrontendUserProfile, GuardianAssignmentStatus};
    use crate::test_support::{
        create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE, CHARLIE_PHRASE,
        READER1_PHRASE, READER2_PHRASE,
    };
    use chrono::Utc;
    use human_money_core::app_service::AppService;
    use humoco_market::{parse_emergency_kit_uri, RecoveryKdfParams};
    use std::path::Path;

    fn make_test_contact(did: &str, first_name: &str, trust_level: Option<u8>) -> FrontendContact {
        FrontendContact {
            did: did.to_string(),
            profile: FrontendUserProfile {
                id: Some(did.to_string()),
                first_name: Some(first_name.to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        }
    }

    fn create_wallet_state(
        base_dir: &Path,
        name: &str,
        phrase: &str,
        pass: &str,
    ) -> (AppState, String) {
        let dir = base_dir.join(name);
        let actual_phrase = if phrase.trim().is_empty() {
            human_money_core::services::mnemonic::MnemonicProcessor::generate(
                12,
                human_money_core::MnemonicLanguage::English,
            )
            .unwrap()
        } else {
            phrase.to_string()
        };
        let service = setup_test_app(&dir, name, &actual_phrase, pass);
        let state = create_test_state(service);
        let did = {
            let s = state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        (state, did)
    }

    #[tokio::test]
    async fn test_emergency_sheet_confirmation_and_status() {
        let base_dir = temp_test_dir("emergency_sheet_test");
        let password = "TestPassword123!";
        let (alice_state, _alice_did) =
            create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);

        // 1. Initial status has no emergency confirmation
        let initial_status =
            get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
        assert!(initial_status.emergency_sheet_confirmed_at.is_none());
        assert!(!initial_status.has_identity_recovery);

        // 2. Wrong control word is rejected
        let wrong_res = confirm_emergency_sheet_saved_impl(
            password.to_string(),
            4,
            "incorrect_word".to_string(),
            &alice_state,
            None,
        )
        .await;
        assert!(wrong_res.is_err());

        // 3. Right control word succeeds (Word 4 of ALICE_PHRASE is "abandon")
        let confirm_res = confirm_emergency_sheet_saved_impl(
            password.to_string(),
            4,
            "abandon".to_string(),
            &alice_state,
            None,
        )
        .await
        .unwrap();
        assert!(confirm_res.confirmed_at > 0);

        // 4. Protection status reflects confirmation
        let updated_status =
            get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
        assert!(updated_status.emergency_sheet_confirmed_at.is_some());
        assert!(updated_status.has_identity_recovery);
    }

    #[tokio::test]
    async fn test_export_profile_mnemonic_kit() {
        let base_dir = temp_test_dir("mnemonic_kit_test");
        let password = "TestPassword123!";
        let (alice_state, _alice_did) =
            create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);

        let kit = export_profile_mnemonic_impl(password.to_string(), &alice_state)
            .await
            .unwrap()
            .expect("Mnemonic kit DTO should be generated");

        assert_eq!(kit.phrase.split_whitespace().count(), 12);
        assert!(kit.kit_uri.starts_with("humoco://kit?"));
        assert!(kit.qr_svg.starts_with("<svg"));

        let parsed = parse_emergency_kit_uri(&kit.kit_uri).expect("URI must parse successfully");
        assert_eq!(parsed.language_code, 1);
        assert!(!parsed.entropy_hex.is_empty());
    }

    #[tokio::test]
    async fn test_setup_social_recovery_validation_and_guards() {
        let base_dir = temp_test_dir("validation_guards_test");
        let password = "TestPassword123!";
        let (alice_state, _alice_did) =
            create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
        let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
        let (_carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
        let (_dave_state, dave_did) =
            create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

        // 1. Less than 3 guardians rejected
        let res_few = setup_social_recovery_guardians_with_params(
            password.to_string(),
            vec![bob_did.clone(), carol_did.clone()],
            2,
            [1, 2, 3],
            ["A".to_string(), "B".to_string(), "C".to_string()],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await;
        assert!(res_few.is_err());

        // 2. Duplicate guardians rejected
        let res_dup = setup_social_recovery_guardians_with_params(
            password.to_string(),
            vec![bob_did.clone(), bob_did.clone(), carol_did.clone()],
            2,
            [1, 2, 3],
            ["A".to_string(), "B".to_string(), "C".to_string()],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await;
        assert!(res_dup.is_err());

        // 3. Guardians not in AddressBook or trust level < 1 rejected
        let res_untrusted = setup_social_recovery_guardians_with_params(
            password.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            ["A".to_string(), "B".to_string(), "C".to_string()],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await;
        assert!(res_untrusted.is_err());
    }

    #[tokio::test]
    async fn test_3_of_5_social_recovery_lifecycle_and_security() {
        let base_dir = temp_test_dir("social_recovery_lifecycle");
        let password = "TestPassword123!";

        // 1. Initialize Alice and 5 friends
        let (alice_state, alice_did) =
            create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
        let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
        let (carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
        let (dave_state, dave_did) =
            create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);
        let (_eve_state, eve_did) = create_wallet_state(&base_dir, "Eve", READER2_PHRASE, password);
        let (_frank_state, frank_did) = create_wallet_state(&base_dir, "Frank", "", password);

        // 2. Alice adds all 5 friends to contacts with Trust Level 2
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(2)),
                    Some(password),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(password),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(2)),
                    Some(password),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&eve_did, "Eve", Some(2)),
                    Some(password),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&frank_did, "Frank", Some(2)),
                    Some(password),
                )
                .unwrap();
        }

        // 3. Alice sets up 3-of-5 social recovery
        let setup_res = setup_social_recovery_guardians_with_params(
            password.to_string(),
            vec![
                bob_did.clone(),
                carol_did.clone(),
                dave_did.clone(),
                eve_did.clone(),
                frank_did.clone(),
            ],
            3,
            [1, 2, 3],
            [
                "Berlin".to_string(),
                "Fluffy".to_string(),
                "Pizza".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await
        .unwrap();

        assert_eq!(setup_res.rotation_epoch, 1);
        assert_eq!(setup_res.threshold_m, 3);
        assert_eq!(setup_res.guardians_count, 5);

        // 4. Verify Alice's outbox contains 5 assign envelopes
        let post_state = load_post_state(&alice_state, Some(password)).unwrap();
        assert_eq!(post_state.outbox_entries.len(), 5);

        // 5. Deliver sealed entries to Bob, Carol, Dave's vaults
        let owner_state = {
            let mut s = alice_state.service.lock().unwrap();
            load_recovery_owner_state(&mut s, Some(password))
                .unwrap()
                .unwrap()
        };

        // Bob stores his assigned vault entry
        {
            let bob_rec = owner_state
                .guardians
                .iter()
                .find(|g| g.user_id == bob_did)
                .unwrap();
            let mut s = bob_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
            let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
        }

        // Carol stores her assigned vault entry
        {
            let carol_rec = owner_state
                .guardians
                .iter()
                .find(|g| g.user_id == carol_did)
                .unwrap();
            let mut s = carol_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
            let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
        }

        // Dave stores his assigned vault entry
        {
            let dave_rec = owner_state
                .guardians
                .iter()
                .find(|g| g.user_id == dave_did)
                .unwrap();
            let mut s = dave_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
            let _ = vault.upsert_ward(dave_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
        }

        // 6. Simulate receipt of ACKs from Bob, Carol, Dave in Alice's state
        {
            let mut s = alice_state.service.lock().unwrap();
            let mut os = load_recovery_owner_state(&mut s, Some(password))
                .unwrap()
                .unwrap();
            for g in &mut os.guardians {
                if g.user_id == bob_did || g.user_id == carol_did || g.user_id == dave_did {
                    g.status = GuardianAssignmentStatus::Confirmed {
                        acked_at: Utc::now().timestamp() as u64,
                    };
                }
            }
            save_recovery_owner_state(&mut s, &os, Some(password)).unwrap();
        }

        // Protection status now reflects 3 confirmed guardians
        let prot_status =
            get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
        assert_eq!(prot_status.confirmed_guardians_count, 3);
        assert!(prot_status.has_identity_recovery);

        // 7. Disaster mode: fresh device session
        let session_dir = temp_test_dir("disaster_session");
        let session_file = session_dir.join("active_recovery_session.json");

        let active_sess = start_social_recovery_session_impl(
            Some("Alice".to_string()),
            Some("restore_new_device".to_string()),
            None,
            &session_file,
        )
        .unwrap();
        assert_eq!(active_sess.collected_count, 0);
        assert!(!active_sess.has_enough_shares);
        assert_eq!(active_sess.session_mode, "restore_new_device");

        // Bob inspects request
        let bob_inspection = inspect_guardian_recovery_request_impl(
            active_sess.request_uri.clone(),
            Some(password),
            &bob_state,
        )
        .unwrap();
        assert_eq!(bob_inspection.sas_code, active_sess.sas_code);
        assert_eq!(bob_inspection.wards.len(), 1);
        assert!(are_dids_equal(
            &bob_inspection.wards[0].owner_did,
            &alice_did
        ));

        // Bob, Carol, Dave approve
        let bob_approval = approve_guardian_recovery_request_impl(
            active_sess.request_uri.clone(),
            alice_did.clone(),
            Some(password),
            &bob_state,
        )
        .unwrap();

        let carol_approval = approve_guardian_recovery_request_impl(
            active_sess.request_uri.clone(),
            alice_did.clone(),
            Some(password),
            &carol_state,
        )
        .unwrap();

        let dave_approval = approve_guardian_recovery_request_impl(
            active_sess.request_uri.clone(),
            alice_did.clone(),
            Some(password),
            &dave_state,
        )
        .unwrap();

        // 8. Import release replies into session
        import_guardian_recovery_reply_impl(bob_approval.reply_uri, &session_file).unwrap();
        import_guardian_recovery_reply_impl(carol_approval.reply_uri, &session_file).unwrap();
        let progress =
            import_guardian_recovery_reply_impl(dave_approval.reply_uri, &session_file).unwrap();

        assert_eq!(progress.collected_count, 3);
        assert!(progress.has_enough_shares);

        // 9. WRONG answers: fails, and active_recovery_session.json MUST NOT be deleted!
        let restored_base = temp_test_dir("restored_wallet");
        let restored_wallet_dir = restored_base.join("wallet");
        let empty_service = AppService::new(&restored_wallet_dir).unwrap();
        let restored_state = create_test_state(empty_service);

        let wrong_err = complete_social_recovery_impl(
            alice_did.clone(),
            1,
            [
                "WrongCity".to_string(),
                "Fluffy".to_string(),
                "Pizza".to_string(),
            ],
            None,
            "restore_new_device".to_string(),
            "NewPassword456!".to_string(),
            Some("Alice Restored".to_string()),
            None,
            "device-restored-1".to_string(),
            None,
            None,
            &session_file,
            &restored_state,
            &restored_base,
            &restored_wallet_dir,
            None,
        )
        .await;

        assert!(
            wrong_err.is_err(),
            "Wrong answers must fail AEAD decryption"
        );
        assert!(
            session_file.exists(),
            "Active recovery session MUST NOT be deleted when answers are incorrect!"
        );

        // 10. RIGHT answers: first without allow_empty_fallback requests seed-only confirmation
        let confirm_res = complete_social_recovery_impl(
            alice_did.clone(),
            1,
            [
                "Berlin".to_string(),
                "Fluffy".to_string(),
                "Pizza".to_string(),
            ],
            None,
            "restore_new_device".to_string(),
            "NewPassword456!".to_string(),
            Some("Alice Restored".to_string()),
            None,
            "device-restored-1".to_string(),
            Some(false),
            None,
            &session_file,
            &restored_state,
            &restored_base,
            &restored_wallet_dir,
            None,
        )
        .await
        .unwrap();
        assert_eq!(confirm_res.outcome, "sync_empty_requires_confirm");
        assert!(
            session_file.exists(),
            "Session file must remain while awaiting seed-only confirmation"
        );

        // Now confirm seed-only setup explicitly with allow_empty_fallback: Some(true)
        let right_res = complete_social_recovery_impl(
            alice_did.clone(),
            1,
            [
                "Berlin".to_string(),
                "Fluffy".to_string(),
                "Pizza".to_string(),
            ],
            None,
            "restore_new_device".to_string(),
            "NewPassword456!".to_string(),
            Some("Alice Restored".to_string()),
            None,
            "device-restored-1".to_string(),
            Some(true),
            None,
            &session_file,
            &restored_state,
            &restored_base,
            &restored_wallet_dir,
            None,
        )
        .await
        .unwrap();

        assert_eq!(right_res.outcome, "restored_seed_only");
        assert!(
            !session_file.exists(),
            "Active recovery session MUST be cleaned up on successful recovery!"
        );

        // 11. Verify restored wallet identity matches Alice
        let restored_did = {
            let s = restored_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        assert_eq!(restored_did, alice_did);
    }

    #[tokio::test]
    async fn test_local_password_reset_mode() {
        let base_dir = temp_test_dir("password_reset_test");
        let old_pass = "OldPassword123!";
        let new_pass = "NewPassword456!";

        let (alice_state, alice_did) =
            create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, old_pass);
        let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, old_pass);
        let (carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, old_pass);
        let (_dave_state, dave_did) =
            create_wallet_state(&base_dir, "Dave", READER1_PHRASE, old_pass);

        // Add 3 guardians and setup recovery
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(2)),
                    Some(old_pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(old_pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(2)),
                    Some(old_pass),
                )
                .unwrap();
        }

        setup_social_recovery_guardians_with_params(
            old_pass.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            [
                "AnswerOne1".to_string(),
                "AnswerTwo2".to_string(),
                "AnswerThree3".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await
        .unwrap();

        // Deliver entries to Bob & Carol
        let owner_state = {
            let mut s = alice_state.service.lock().unwrap();
            load_recovery_owner_state(&mut s, Some(old_pass))
                .unwrap()
                .unwrap()
        };

        {
            let bob_rec = owner_state
                .guardians
                .iter()
                .find(|g| g.user_id == bob_did)
                .unwrap();
            let mut s = bob_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(old_pass)).unwrap();
            let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(old_pass)).unwrap();
        }

        {
            let carol_rec = owner_state
                .guardians
                .iter()
                .find(|g| g.user_id == carol_did)
                .unwrap();
            let mut s = carol_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(old_pass)).unwrap();
            let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(old_pass)).unwrap();
        }

        // Start session & approve
        let session_file = base_dir.join("active_recovery_session.json");
        let session_dto = start_social_recovery_session_impl(
            None,
            Some("reset_local_password".to_string()),
            Some("dummy".to_string()),
            &session_file,
        )
        .unwrap();
        assert_eq!(session_dto.session_mode, "reset_local_password");

        let b_rep = approve_guardian_recovery_request_impl(
            session_dto.request_uri.clone(),
            alice_did.clone(),
            Some(old_pass),
            &bob_state,
        )
        .unwrap();
        let c_rep = approve_guardian_recovery_request_impl(
            session_dto.request_uri.clone(),
            alice_did.clone(),
            Some(old_pass),
            &carol_state,
        )
        .unwrap();

        import_guardian_recovery_reply_impl(b_rep.reply_uri, &session_file).unwrap();
        import_guardian_recovery_reply_impl(c_rep.reply_uri, &session_file).unwrap();

        // Identify Alice's folder name
        let folder_name = {
            let s = alice_state.service.lock().unwrap();
            let profiles = s.list_profiles().unwrap();
            profiles[0].folder_name.clone()
        };

        // Reset local password
        let alice_wallet_dir = base_dir.join("Alice");
        let outcome = complete_social_recovery_impl(
            alice_did.clone(),
            1,
            [
                "AnswerOne1".to_string(),
                "AnswerTwo2".to_string(),
                "AnswerThree3".to_string(),
            ],
            None,
            "reset_local_password".to_string(),
            new_pass.to_string(),
            None,
            Some(folder_name.clone()),
            "device-reset-1".to_string(),
            None,
            None,
            &session_file,
            &alice_state,
            &base_dir,
            &alice_wallet_dir,
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.outcome, "password_reset");
        assert!(!session_file.exists());

        // Verify login with new password succeeds and old password fails
        {
            let mut s = alice_state.service.lock().unwrap();
            s.logout();
            let old_login = s.login(&folder_name, old_pass, false, "device-reset-1".to_string());
            assert!(
                old_login.is_err(),
                "Old password must not unlock after reset"
            );

            let new_login = s.login(&folder_name, new_pass, false, "device-reset-1".to_string());
            assert!(new_login.is_ok(), "New password must unlock the wallet");
        }
    }

    #[tokio::test]
    async fn test_inspect_emergency_kit_uri_12_and_24_words() {
        let base_dir = temp_test_dir("inspect_emergency_kit");
        let pass = "TestPassword123!";
        let (alice_state, _) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);

        let kit_12 = export_profile_mnemonic_impl(pass.to_string(), &alice_state)
            .await
            .unwrap()
            .unwrap();
        let inspected_12 = inspect_emergency_kit_uri_impl(kit_12.kit_uri).unwrap();
        assert_eq!(inspected_12.word_count, 12);
        assert_eq!(inspected_12.phrase, ALICE_PHRASE);
        assert!(!inspected_12.has_bip39_passphrase);

        // 24-word German emergency kit URI
        let phrase_24 = human_money_core::services::mnemonic::MnemonicProcessor::generate(
            24,
            human_money_core::MnemonicLanguage::German,
        )
        .unwrap();
        let entropy_24 = human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(
            &phrase_24,
            human_money_core::MnemonicLanguage::German,
        )
        .unwrap();
        let kit_24_uri = humoco_market::build_emergency_kit_uri(&humoco_market::EmergencyKitData {
            version: 1,
            entropy_hex: hex::encode(entropy_24),
            language_code: 2, // German
            user_prefix: Some("verein".to_string()),
            has_bip39_passphrase: true,
            sync_descriptors: vec![DropDescriptor {
                write_url: "https://sync.example.org/sync.php".to_string(),
                read_url: "https://sync.example.org/data".to_string(),
                slot_id: None,
                drop_pub: "00".repeat(32),
            }],
        })
        .unwrap();

        let inspected_24 = inspect_emergency_kit_uri_impl(kit_24_uri).unwrap();
        assert_eq!(inspected_24.word_count, 24);
        assert_eq!(inspected_24.phrase, phrase_24);
        assert_eq!(
            inspected_24.language,
            crate::models::MnemonicLanguage::German
        );
        assert!(inspected_24.has_bip39_passphrase);
        assert_eq!(inspected_24.user_prefix.as_deref(), Some("verein"));
        assert_eq!(
            inspected_24.sync_urls,
            vec!["https://sync.example.org/sync.php".to_string()]
        );
        assert_eq!(inspected_24.sync_descriptors_json.len(), 1);
    }

    #[tokio::test]
    async fn test_revoke_social_recovery_clears_guardians() {
        let base_dir = temp_test_dir("revoke_recovery_test");
        let pass = "TestPassword123!";

        let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
        let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
        let (_carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(2)),
                    Some(pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(2)),
                    Some(pass),
                )
                .unwrap();
        }

        setup_social_recovery_guardians_with_params(
            pass.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            [
                "Alpha123".to_string(),
                "Bravo456".to_string(),
                "Charlie789".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await
        .unwrap();

        // Call revoke
        let revoke_res = revoke_social_recovery_impl(Some(pass.to_string()), &alice_state, None)
            .await
            .unwrap();

        assert_eq!(revoke_res.rotation_epoch, 2);
        assert_eq!(revoke_res.revoked_guardians_count, 3);

        // Verify owner state is now cleared
        let owner_state = {
            let mut s = alice_state.service.lock().unwrap();
            load_recovery_owner_state(&mut s, Some(pass))
                .unwrap()
                .unwrap()
        };
        assert_eq!(owner_state.rotation_epoch, 2);
        assert!(owner_state.guardians.is_empty());

        // Verify post outbox contains revoke messages
        let post_state = load_post_state(&alice_state, Some(pass)).unwrap();
        let has_revokes = post_state
            .outbox_entries
            .iter()
            .any(|e| !e.entry.ciphertext.is_empty());
        assert!(has_revokes);
    }

    #[test]
    fn test_ward_vault_max_limit_and_epoch_guards() {
        use humoco_market::GuardianVaultStore;

        let mut vault = GuardianVaultStore::default();
        let password = "TestPassword123!";
        let base_dir = temp_test_dir("ward_vault_test");

        // Create a valid entry from Alice
        let (alice_state, _alice_did) =
            create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
        let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
        let (_carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
        let (_dave_state, dave_did) =
            create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(2)),
                    Some(password),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(password),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(2)),
                    Some(password),
                )
                .unwrap();
        }

        let setup_res = tokio::runtime::Runtime::new().unwrap().block_on(async {
            setup_social_recovery_guardians_with_params(
                password.to_string(),
                vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
                2,
                [1, 2, 3],
                [
                    "AnswerOne1".to_string(),
                    "AnswerTwo2".to_string(),
                    "AnswerThree3".to_string(),
                ],
                RecoveryKdfParams::new_fast_test(),
                &alice_state,
                None,
            )
            .await
            .unwrap()
        });
        assert_eq!(setup_res.rotation_epoch, 1);

        let owner_state = {
            let mut s = alice_state.service.lock().unwrap();
            load_recovery_owner_state(&mut s, Some(password))
                .unwrap()
                .unwrap()
        };
        let valid_entry = owner_state.guardians[0].sealed_entry.clone();

        // 1. Upsert succeeds for epoch 1
        assert!(vault.upsert_ward(valid_entry.clone()).is_ok());
        assert_eq!(vault.wards.len(), 1);

        // 2. Re-upserting same valid entry succeeds
        assert!(vault.upsert_ward(valid_entry.clone()).is_ok());

        // 3. Fill up to 64 wards and verify an authentic 65th ward is rejected on capacity
        vault.wards.clear();
        for i in 0..64 {
            let mut dummy = valid_entry.clone();
            dummy.owner_did = format!("did:key:dummy_owner_{}", i);
            let _ = vault.wards.insert(dummy.owner_did.clone(), dummy);
        }
        assert_eq!(vault.wards.len(), 64);

        // valid_entry has authentic signature but capacity is full
        let res_excess = vault.upsert_ward(valid_entry);
        assert!(res_excess.is_err());
        assert!(res_excess
            .unwrap_err()
            .to_string()
            .contains("capacity reached"));
    }

    #[tokio::test]
    async fn test_probe_sync_backup_unreachable_and_empty_guard() {
        let base_dir = temp_test_dir("mirror_discovery_test");
        let wallet_dir = base_dir.join("empty");
        let empty_service = AppService::new(&wallet_dir).unwrap();
        let state = create_test_state(empty_service);
        let tmp_restore = base_dir.join("tmp_restore");
        std::fs::create_dir_all(&tmp_restore).unwrap();

        // 1. Empty inputs return error
        let empty_res = probe_sync_backup_impl(
            Vec::new(),
            ALICE_PHRASE.to_string(),
            None,
            None,
            Some(crate::models::MnemonicLanguage::English),
            &wallet_dir,
            &tmp_restore,
            &state,
        )
        .await;
        assert!(empty_res.is_err());

        // 2. Unreachable descriptor returns status = "unreachable" cleanly (never silent seed-only)
        let unreachable_desc = serde_json::to_string(&DropDescriptor {
            write_url: "http://127.0.0.1:9".to_string(),
            read_url: "http://127.0.0.1:9".to_string(),
            slot_id: Some("deadslot".to_string()),
            drop_pub: "deadkey".to_string(),
        })
        .unwrap();
        let unreach_res = probe_sync_backup_impl(
            vec![unreachable_desc],
            ALICE_PHRASE.to_string(),
            None,
            None,
            Some(crate::models::MnemonicLanguage::English),
            &wallet_dir,
            &tmp_restore,
            &state,
        )
        .await
        .unwrap();
        assert_eq!(unreach_res.status, "unreachable");
        assert!(unreach_res.error.is_some());
    }

    #[tokio::test]
    async fn test_reply_import_guards_and_ward_contact_name_resolution() {
        let base_dir = temp_test_dir("reply_import_guards_test");
        let pass = "TestPassword123!";

        let (alice_state, alice_did) = create_wallet_state(&base_dir, "Privat", ALICE_PHRASE, pass);
        let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
        let (carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);
        let (eve_state, eve_did) = create_wallet_state(&base_dir, "Eve", READER2_PHRASE, pass);

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(2)),
                    Some(pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(2)),
                    Some(pass),
                )
                .unwrap();
        }
        {
            let mut s = eve_state.service.lock().unwrap();
            eve_state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(2)),
                    Some(pass),
                )
                .unwrap();
            eve_state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(pass),
                )
                .unwrap();
            eve_state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(2)),
                    Some(pass),
                )
                .unwrap();
        }
        // Bob saves Alice in his AddressBook as "Hans Müller"
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut hans_contact = make_test_contact(&alice_did, "Hans", Some(2));
            hans_contact.profile.last_name = Some("Müller".to_string());
            bob_state
                .save_contact(&mut s, hans_contact, Some(pass))
                .unwrap();
        }

        // Alice sets up epoch 1, then rotates to epoch 2
        setup_social_recovery_guardians_with_params(
            pass.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            [
                "Alpha123".to_string(),
                "Bravo456".to_string(),
                "Charlie789".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await
        .unwrap();

        let owner_e1 = {
            let mut s = alice_state.service.lock().unwrap();
            load_recovery_owner_state(&mut s, Some(pass))
                .unwrap()
                .unwrap()
        };

        // Carol keeps epoch 1 (stale)
        {
            let carol_rec = owner_e1
                .guardians
                .iter()
                .find(|g| g.user_id == carol_did)
                .unwrap();
            let mut s = carol_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
            let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
        }

        // Alice rotates to epoch 2 and delivers to Bob
        setup_social_recovery_guardians_with_params(
            pass.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            [
                "Alpha123".to_string(),
                "Bravo456".to_string(),
                "Charlie789".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await
        .unwrap();

        let owner_e2 = {
            let mut s = alice_state.service.lock().unwrap();
            load_recovery_owner_state(&mut s, Some(pass))
                .unwrap()
                .unwrap()
        };

        {
            let bob_rec = owner_e2
                .guardians
                .iter()
                .find(|g| g.user_id == bob_did)
                .unwrap();
            let mut s = bob_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
            let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
        }

        // Eve also adds Dave so Bob holds a second ward for mismatch test
        setup_social_recovery_guardians_with_params(
            pass.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            [
                "Delta111".to_string(),
                "Echo2222".to_string(),
                "Foxtrot3".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &eve_state,
            None,
        )
        .await
        .unwrap();
        let eve_owner = {
            let mut s = eve_state.service.lock().unwrap();
            load_recovery_owner_state(&mut s, Some(pass))
                .unwrap()
                .unwrap()
        };
        {
            let bob_eve_rec = eve_owner
                .guardians
                .iter()
                .find(|g| g.user_id == bob_did)
                .unwrap();
            let mut s = bob_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
            let _ = vault.upsert_ward(bob_eve_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
        }

        let session_file = base_dir.join("active_recovery_session.json");
        let sess = start_social_recovery_session_impl(None, None, None, &session_file).unwrap();

        // Bob inspects -> Alice's ward uses AddressBook name "Hans Müller" instead of "Privat"
        let inspection = inspect_guardian_recovery_request_impl(
            sess.request_uri.clone(),
            Some(pass),
            &bob_state,
        )
        .unwrap();
        let hans_ward = inspection
            .wards
            .iter()
            .find(|w| are_dids_equal(&w.owner_did, &alice_did))
            .unwrap();
        assert_eq!(hans_ward.owner_display_name, "Hans Müller");

        // Bob approves Alice (epoch 2)
        let bob_alice_rep = approve_guardian_recovery_request_impl(
            sess.request_uri.clone(),
            alice_did.clone(),
            Some(pass),
            &bob_state,
        )
        .unwrap();
        assert_eq!(bob_alice_rep.owner_display_name, "Hans Müller");

        let added =
            import_guardian_recovery_reply_impl(bob_alice_rep.reply_uri.clone(), &session_file)
                .unwrap();
        assert_eq!(added.last_import_status.as_deref(), Some("added"));
        assert_eq!(added.total_n, Some(3));
        assert_eq!(added.collected_share_indices.len(), 1);

        // Re-importing same reply yields duplicate status
        let dup =
            import_guardian_recovery_reply_impl(bob_alice_rep.reply_uri, &session_file).unwrap();
        assert_eq!(dup.last_import_status.as_deref(), Some("duplicate"));
        assert_eq!(dup.collected_count, 1);

        // Stale epoch reply from Carol (epoch 1 < epoch 2) is rejected with clear error
        let carol_stale_rep = approve_guardian_recovery_request_impl(
            sess.request_uri.clone(),
            alice_did.clone(),
            Some(pass),
            &carol_state,
        )
        .unwrap();
        let stale_err =
            import_guardian_recovery_reply_impl(carol_stale_rep.reply_uri, &session_file)
                .unwrap_err();
        assert!(stale_err.message.contains("veralteten"));

        // Mismatched ward reply from Bob (Eve instead of Alice) is rejected
        let bob_eve_rep = approve_guardian_recovery_request_impl(
            sess.request_uri,
            eve_did,
            Some(pass),
            &bob_state,
        )
        .unwrap();
        let mismatch_err =
            import_guardian_recovery_reply_impl(bob_eve_rep.reply_uri, &session_file).unwrap_err();
        assert!(mismatch_err.message.contains("anderen Profil"));
    }

    #[tokio::test]
    async fn test_12_question_setup_and_spaced_repetition_rehearsal() {
        let base_dir = temp_test_dir("recovery_rehearsal");
        let pass = "RehearsalPass!99";
        let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
        let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
        let (_carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

        {
            let mut s = alice_state.service.lock().unwrap();
            s.unlock_session(pass, 300).unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(3)),
                    Some(pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(pass),
                )
                .unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(1)),
                    Some(pass),
                )
                .unwrap();
        }

        setup_social_recovery_guardians_with_params(
            pass.to_string(),
            vec![bob_did, carol_did, dave_did],
            2,
            [7, 10, 12],
            [
                "Sandra".to_string(),
                "Nokia 3210".to_string(),
                "Blauer Pinguin".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await
        .unwrap();

        let status_now = get_recovery_protection_status_impl(None, &alice_state).unwrap();
        assert_eq!(status_now.question_ids, Some([7, 10, 12]));
        assert_eq!(status_now.rehearsal_stage, 0);
        assert!(!status_now.rehearsal_due);
        let first_due = status_now.next_rehearsal_at.unwrap();

        // Wrong middle answer pinpoints question index 1
        let wrong_res = verify_recovery_rehearsal_impl(
            [
                "sandra".to_string(),
                "Siemens C35".to_string(),
                "blauer pinguin".to_string(),
            ],
            first_due + 10,
            &alice_state,
        )
        .unwrap();
        assert!(!wrong_res.all_correct);
        assert_eq!(wrong_res.question_matches, [true, false, true]);
        assert_eq!(wrong_res.rehearsal_stage, 0);

        // Snooze pushes next_rehearsal_at forward by 24h
        let snoozed_at =
            snooze_recovery_rehearsal_impl(Some(24), first_due + 10, &alice_state).unwrap();
        assert_eq!(snoozed_at, first_due + 10 + 24 * 3600);

        // Exact (normalized) match advances stage 0 -> 1 (7 days)
        let ok_res = verify_recovery_rehearsal_impl(
            [
                " sandra ".to_string(),
                "nokia-3210".to_string(),
                "BLAUER_pinguin".to_string(),
            ],
            snoozed_at + 10,
            &alice_state,
        )
        .unwrap();
        assert!(ok_res.all_correct);
        assert_eq!(ok_res.question_matches, [true, true, true]);
        assert_eq!(ok_res.rehearsal_stage, 1);
        assert_eq!(ok_res.next_rehearsal_at, snoozed_at + 10 + 7 * 86_400);
    }

    #[tokio::test]
    async fn test_subaccount_social_recovery_vault_isolation_and_locked_session_status_cache() {
        let base_dir = temp_test_dir("subaccount_recovery_isolation_and_cache");
        let pass = "TestPassword123!";

        let create_sub = |name: &str, dir_name: &str, prefix: Option<&str>| {
            let dir = base_dir.join(dir_name);
            let mut svc = AppService::new(&dir).unwrap();
            svc.create_profile(
                name,
                ALICE_PHRASE,
                None,
                prefix,
                pass,
                human_money_core::MnemonicLanguage::English,
                format!("inst-{}", dir_name),
                true,
            )
            .unwrap();
            svc.unlock_session(pass, 300).unwrap();
            let state = create_test_state(svc);
            let did = state
                .service
                .lock()
                .unwrap()
                .with_wallet(|w| w.get_user_id().to_string())
                .unwrap();
            (dir, state, did)
        };

        let (laden_dir, laden_state, laden_did) = create_sub("Alice Laden", "laden", Some("laden"));
        let (_privat_dir, privat_state, privat_did) =
            create_sub("Alice Privat", "privat", Some("privat"));
        let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
        let (_carol_state, carol_did) =
            create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

        for state in [&laden_state, &privat_state] {
            let mut s = state.service.lock().unwrap();
            state
                .save_contact(
                    &mut s,
                    make_test_contact(&bob_did, "Bob", Some(2)),
                    Some(pass),
                )
                .unwrap();
            state
                .save_contact(
                    &mut s,
                    make_test_contact(&carol_did, "Carol", Some(2)),
                    Some(pass),
                )
                .unwrap();
            state
                .save_contact(
                    &mut s,
                    make_test_contact(&dave_did, "Dave", Some(2)),
                    Some(pass),
                )
                .unwrap();
        }

        // Both subaccounts configure Social Recovery with epoch 1 and the same guardian Bob
        let laden_setup = setup_social_recovery_guardians_with_params(
            pass.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            [
                "Berlin".to_string(),
                "Fluffy".to_string(),
                "Pizza".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &laden_state,
            None,
        )
        .await
        .unwrap();
        let privat_setup = setup_social_recovery_guardians_with_params(
            pass.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [4, 5, 6],
            [
                "Hamburg".to_string(),
                "Rex".to_string(),
                "Pasta".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &privat_state,
            None,
        )
        .await
        .unwrap();
        assert_eq!(laden_setup.rotation_epoch, 1);
        assert_eq!(privat_setup.rotation_epoch, 1);

        let laden_owner = {
            let mut s = laden_state.service.lock().unwrap();
            load_recovery_owner_state_with_cache(&laden_state, &mut s, Some(pass))
                .unwrap()
                .unwrap()
        };
        let privat_owner = {
            let mut s = privat_state.service.lock().unwrap();
            load_recovery_owner_state_with_cache(&privat_state, &mut s, Some(pass))
                .unwrap()
                .unwrap()
        };
        assert_eq!(laden_owner.guardians[0].sealed_entry.owner_did, laden_did);
        assert_eq!(privat_owner.guardians[0].sealed_entry.owner_did, privat_did);

        // Bob stores guardian entries for both subaccounts — second setup must NOT overwrite first
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut vault = load_guardian_vault_with_cache(&bob_state, &mut s, Some(pass)).unwrap();
            vault
                .upsert_ward(
                    laden_owner
                        .guardians
                        .iter()
                        .find(|g| g.user_id == bob_did)
                        .unwrap()
                        .sealed_entry
                        .clone(),
                )
                .unwrap();
            vault
                .upsert_ward(
                    privat_owner
                        .guardians
                        .iter()
                        .find(|g| g.user_id == bob_did)
                        .unwrap()
                        .sealed_entry
                        .clone(),
                )
                .unwrap();
            save_guardian_vault_with_cache(&bob_state, &mut s, &vault, Some(pass)).unwrap();
            assert_eq!(vault.wards.len(), 2);
            assert!(vault.get_ward(&laden_did).is_some());
            assert!(vault.get_ward(&privat_did).is_some());
        }

        // Finding 11: Simulate timeout = 0 (unlocked session absent after initialize_profile_session)
        {
            let mut s = laden_state.service.lock().unwrap();
            let mut policy = laden_state
                .load_security_policy(&mut s, Some(pass))
                .unwrap();
            policy.session_timeout_seconds = 0;
            laden_state
                .save_security_policy(&mut s, policy, Some(pass))
                .unwrap();
        }
        let mut reloaded_svc = AppService::new(&laden_dir).unwrap();
        let folder_name = reloaded_svc.list_profiles().unwrap()[0].folder_name.clone();
        reloaded_svc
            .login(&folder_name, pass, false, "inst-laden".to_string())
            .unwrap();
        let reloaded_state = create_test_state(AppService::new(&laden_dir).unwrap());
        {
            reloaded_state
                .initialize_profile_session(&mut reloaded_svc, pass)
                .unwrap();
            *reloaded_state.service.lock().unwrap() = reloaded_svc;
        }
        // Session has no cached password (timeout = 0), yet status reads cleanly from cache!
        let status = get_recovery_protection_status_impl(None, &reloaded_state).unwrap();
        assert_eq!(status.guardians.len(), 3);
        assert_eq!(status.required_threshold, 2);
        assert_eq!(status.question_ids, Some([1, 2, 3]));
    }
}
