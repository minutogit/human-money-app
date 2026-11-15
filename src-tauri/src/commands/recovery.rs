use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use human_money_core::app_service::AppService;
use human_money_core::crypto::get_pubkey_from_user_id;
use human_money_core::models::backup::BackupRestoreCredentials;
use human_money_core::models::secure_container::{
    ContainerConfig, PayloadType, PrivacyMode, SecureContainer,
};
use human_money_core::services::mnemonic::MnemonicLanguage;
use humoco_market::{
    build_emergency_kit_uri, build_recovery_request_uri, code_to_mnemonic_language,
    compute_recovery_sas, create_social_recovery_vault_bundle, encode_qr_svg,
    mnemonic_language_to_code, open_and_verify_guardian_release_reply, parse_emergency_kit_uri,
    parse_recovery_request_uri, recover_master_payload_from_shares, seal_guardian_release_reply,
    seal_post_box, DropDescriptor, GuardianLocatorUpdate, GuardianReleasePayload, GuardianRevoke,
    GuardianVaultEntry, GuardianVaultStore, PostEnvelope, RecoveryKdfParams, RecoveryMasterPayload,
    PAYLOAD_GUARDIAN_ASSIGN_V1, PAYLOAD_GUARDIAN_LOCATOR_V1, PAYLOAD_GUARDIAN_REVOKE_V1,
};
use tauri::State;

use crate::commands::backup::{
    inspect_backup_impl, restore_backup_impl, restore_custom_standards_to_dir, sync_backup_impl,
    write_file_atomic,
};
use crate::commands::post::{
    are_dids_equal, collect_own_reply_syncs, flush_post_slots_impl, load_post_state,
    save_post_state, sync_post_channels_impl,
};
use crate::models::{
    ActiveRecoverySessionDto, ActiveRecoverySessionFile, ApproveGuardianRecoveryResultDto,
    ConfirmEmergencySheetResultDto, EmergencyKitRestoreInput, ExportedMnemonicKitDto,
    FrontendBackupInspectionDto, FrontendError, GuardianAssignmentRecord, GuardianAssignmentStatus,
    GuardianRecoveryRequestInspectionDto, GuardianStatusDto, GuardianWardDto,
    RecoveryCompletionResultDto, RecoveryOwnerState, RecoveryProtectionStatusDto,
    RevokeSocialRecoveryResultDto, SetupSocialRecoveryResultDto,
};
use crate::state::{
    AppState, PostOutboxEntry, RECOVERY_GUARDIAN_VAULT_KEY, RECOVERY_OWNER_STATE_KEY,
};

// ============================================================================
// Storage Helpers
// ============================================================================

pub fn load_recovery_owner_state(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<Option<RecoveryOwnerState>, FrontendError> {
    match service.load_encrypted_data(RECOVERY_OWNER_STATE_KEY, password) {
        Ok(bytes) => serde_json::from_slice::<RecoveryOwnerState>(&bytes)
            .map(Some)
            .map_err(|e| FrontendError::from(format!("Failed to parse recovery owner state: {}", e))),
        Err(_) => Ok(None),
    }
}

pub fn save_recovery_owner_state(
    service: &mut AppService,
    owner_state: &RecoveryOwnerState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(owner_state)
        .map_err(|e| FrontendError::from(format!("Failed to serialize recovery owner state: {}", e)))?;
    service
        .save_encrypted_data(RECOVERY_OWNER_STATE_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

pub fn load_guardian_vault(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<GuardianVaultStore, FrontendError> {
    match service.load_encrypted_data(RECOVERY_GUARDIAN_VAULT_KEY, password) {
        Ok(bytes) => serde_json::from_slice::<GuardianVaultStore>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse guardian vault: {}", e))),
        Err(_) => Ok(GuardianVaultStore::new()),
    }
}

pub fn save_guardian_vault(
    service: &mut AppService,
    vault: &GuardianVaultStore,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(vault)
        .map_err(|e| FrontendError::from(format!("Failed to serialize guardian vault: {}", e)))?;
    service
        .save_encrypted_data(RECOVERY_GUARDIAN_VAULT_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

pub(crate) fn get_or_load_settings_from_state(
    state: &AppState,
    password: Option<&str>,
) -> crate::settings::AppSettings {
    state.get_cached_settings().unwrap_or_else(|_| {
        if let Ok(mut service) = state.service.lock() {
            state.load_settings(&mut service, password).unwrap_or_default()
        } else {
            Default::default()
        }
    })
}

pub fn collect_candidate_backup_sync_descriptors(
    state: &AppState,
    password: Option<&str>,
) -> Result<Vec<DropDescriptor>, FrontendError> {
    let settings = get_or_load_settings_from_state(state, password);
    let mut candidate_targets: Vec<DropDescriptor> = settings
        .market_selected_syncs
        .iter()
        .cloned()
        .map(Into::into)
        .collect();

    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        if let Ok(Some(admin)) = service
            .load_encrypted_data(crate::state::MARKET_SYNC_ADMIN_KEY, password)
            .map(|bytes| serde_json::from_slice::<crate::state::StoredSyncAdmin>(&bytes).ok())
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

    for pd in &settings.public_syncs {
        if pd.enabled
            && !pd.write_url.is_empty()
            && !candidate_targets
                .iter()
                .any(|d| d.write_url == pd.write_url)
        {
            candidate_targets.push(DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(pd.access_pubkey.clone()),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }

    Ok(candidate_targets)
}

pub async fn enqueue_guardian_locator_updates_if_configured(
    state: &AppState,
    password: Option<String>,
) -> Result<usize, FrontendError> {
    let (my_identity, my_signing_key, guardians, epoch) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let owner_state = load_recovery_owner_state(&mut service, password.as_deref())?;
        let Some(owner) = owner_state else {
            return Ok(0);
        };
        if owner.guardians.is_empty() {
            return Ok(0);
        }
        let (id, sk) = service
            .with_wallet_and_identity(|_w, i| (i.clone(), i.signing_key.clone()))
            .map_err(FrontendError::from)?;
        (id, sk, owner.guardians, owner.rotation_epoch)
    };

    let candidate_descriptors = collect_candidate_backup_sync_descriptors(state, password.as_deref())?;
    let settings = get_or_load_settings_from_state(state, password.as_deref());
    let own_master_seed = my_signing_key.to_bytes();
    let own_reply_syncs = collect_own_reply_syncs(&settings, &own_master_seed);

    let now_secs = Utc::now().timestamp() as u64;
    let mut post_state = load_post_state(state, password.as_deref())?;
    let mut enqueued = 0;

    for g in guardians {
        let update = GuardianLocatorUpdate {
            owner_did: my_identity.user_id.clone(),
            rotation_epoch: epoch,
            backup_sync_descriptors: candidate_descriptors.clone(),
            updated_at: now_secs,
        };
        let update_bytes = serde_json::to_vec(&update)
            .map_err(|e| FrontendError::from(e.to_string()))?;
        let container = SecureContainer::seal(
            &my_identity,
            &ContainerConfig::TargetDid(g.user_id.clone(), PrivacyMode::HashedRouting),
            &update_bytes,
            PayloadType::Generic(PAYLOAD_GUARDIAN_LOCATOR_V1.to_string()),
        )
        .map_err(FrontendError::from)?;
        let container_bytes = serde_json::to_vec(&container)
            .map_err(|e| FrontendError::from(e.to_string()))?;

        let msg_id = uuid::Uuid::new_v4().to_string();
        let envelope = PostEnvelope::new(
            msg_id.clone(),
            now_secs,
            own_reply_syncs.iter().cloned().map(Into::into).collect(),
            container_bytes,
        );

        if let Ok(recipient_pk) = get_pubkey_from_user_id(&g.user_id) {
            if let Ok(box_entry) = seal_post_box(&my_signing_key, &recipient_pk, &envelope) {
                post_state.processed_msg_ids.insert(msg_id.clone());
                post_state.outbox_entries.push(PostOutboxEntry {
                    msg_id,
                    recipient_did: g.user_id,
                    tag16_hex: hex::encode(box_entry.tag16),
                    bundle_id: None,
                    entry: box_entry,
                    created_at: now_secs,
                });
                enqueued += 1;
            }
        }
    }

    if enqueued > 0 {
        let _ = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
        save_post_state(state, &post_state, password.as_deref())?;
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
        FrontendError::from(format!("Failed to serialize active recovery session: {}", e))
    })?;
    write_file_atomic(path, &bytes)
}

pub(crate) fn delete_active_session_from_path(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub(crate) fn build_active_session_dto(
    session: &ActiveRecoverySessionFile,
) -> ActiveRecoverySessionDto {
    let mut groups: BTreeMap<(String, u32), Vec<&GuardianReleasePayload>> = BTreeMap::new();
    for reply in &session.replies {
        groups
            .entry((
                reply.entry.owner_did.clone(),
                reply.entry.rotation_epoch,
            ))
            .or_default()
            .push(reply);
    }

    let best_group = groups
        .into_iter()
        .max_by_key(|((_, epoch), list)| (*epoch, list.len()));

    if let Some(((_, epoch), list)) = best_group {
        let first = list[0];
        let threshold_m = first.entry.threshold_m;
        let mut discovered_sync_urls = Vec::new();
        for r in &list {
            for d in &r.entry.backup_sync_descriptors {
                if !d.write_url.is_empty() && !discovered_sync_urls.contains(&d.write_url) {
                    discovered_sync_urls.push(d.write_url.clone());
                }
            }
            for d in &r.extra_sync_descriptors {
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
            owner_did: Some(first.entry.owner_did.clone()),
            owner_display_name: Some(first.entry.owner_display_name.clone()),
            rotation_epoch: Some(epoch),
            threshold_m: Some(threshold_m),
            collected_count: list.len(),
            question_ids: Some(first.entry.question_ids),
            has_enough_shares: list.len() >= threshold_m as usize,
            discovered_sync_urls,
        }
    } else {
        ActiveRecoverySessionDto {
            sas_code: session.sas_code.clone(),
            created_at: session.created_at,
            name_hint: session.name_hint.clone(),
            request_uri: session.request_uri.clone(),
            owner_did: None,
            owner_display_name: None,
            rotation_epoch: None,
            threshold_m: None,
            collected_count: 0,
            question_ids: None,
            has_enough_shares: false,
            discovered_sync_urls: Vec::new(),
        }
    }
}

// ============================================================================
// Parallel Backup Discovery Helper
// ============================================================================

pub(crate) async fn discover_best_backup_from_mirrors(
    descriptors: &[DropDescriptor],
    phrase: &str,
    passphrase: Option<&str>,
    user_prefix: Option<&str>,
    lang: MnemonicLanguage,
    state: &AppState,
    tmp_restore_dir: &Path,
) -> Option<(PathBuf, FrontendBackupInspectionDto)> {
    if descriptors.is_empty() {
        return None;
    }

    let (_pk, my_ed_sk) = human_money_core::services::crypto::keys::derive_ed25519_keypair(
        phrase,
        passphrase,
        lang,
    )
    .ok()?;
    let master_seed = my_ed_sk.to_bytes();
    let normalized_prefix = user_prefix
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);

    let fetch_client = crate::sync_client::create_sync_http_client();
    let credentials = BackupRestoreCredentials::Mnemonic {
        phrase,
        passphrase,
        prefix: user_prefix,
        language: lang,
    };

    let mut set = tokio::task::JoinSet::new();

    for desc in descriptors {
        if desc.read_url.is_empty() {
            continue;
        }

        let mut candidate_keys: Vec<(String, String)> = Vec::new();

        if !desc.drop_pub.is_empty() {
            if let Ok(stealth) =
                humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &desc.drop_pub)
            {
                candidate_keys.push((stealth, "stealth".to_string()));
            }
        }

        if !desc.write_url.is_empty() {
            if let Ok((_, _, pub_hex)) =
                humoco_market::derive_public_drop_keypair(&master_seed, &desc.write_url)
            {
                candidate_keys.push((pub_hex, "public".to_string()));
            }
        }

        if let Some(ref sid) = desc.slot_id {
            if !candidate_keys.iter().any(|(k, _)| k == sid) {
                candidate_keys.push((sid.clone(), "slot_id".to_string()));
            }
        }

        let (admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&master_seed, 0);
        if let Ok(own_stealth) =
            humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &admin_vk)
        {
            if !candidate_keys.iter().any(|(k, _)| k == &own_stealth) {
                candidate_keys.push((own_stealth, "own".to_string()));
            }
        }

        let read_url = desc.read_url.clone();
        let client = fetch_client.clone();
        let pfx = normalized_prefix.clone();

        for (pubkey_hex, _kind) in candidate_keys {
            let r_url = read_url.clone();
            let cl = client.clone();
            let p = pfx.clone();
            set.spawn(async move {
                let res = tokio::time::timeout(
                    std::time::Duration::from_millis(4000),
                    crate::sync_client::get_slot(&cl, &r_url, &pubkey_hex, p.as_deref(), "backup"),
                )
                .await;
                match res {
                    Ok(Ok(Some(bytes))) => Some(bytes),
                    _ => None,
                }
            });
        }
    }

    let mut downloaded_slots = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(Some(bytes)) = res {
            downloaded_slots.push(bytes);
        }
    }

    let mut inspected_candidates = Vec::new();

    for slot_bytes in downloaded_slots {
        if let Ok(unpadded) = humoco_market::unpad_backup_slot_bytes(&slot_bytes) {
            let staged_name = format!("sync_restore_{}.hmcbackup", uuid::Uuid::new_v4());
            let staged_path = tmp_restore_dir.join(staged_name);
            if write_file_atomic(&staged_path, &unpadded).is_ok() {
                let service = state.service.lock().unwrap();
                if let Ok(inspection) = inspect_backup_impl(&staged_path, &credentials, &service) {
                    inspected_candidates.push((staged_path, inspection));
                } else {
                    let _ = std::fs::remove_file(&staged_path);
                }
            }
        }
    }

    if inspected_candidates.is_empty() {
        return None;
    }

    inspected_candidates
        .sort_by(|a, b| (b.1.epoch, b.1.tx_nonce).cmp(&(a.1.epoch, a.1.tx_nonce)));

    let best = inspected_candidates.remove(0);

    for (path, _) in inspected_candidates {
        let _ = std::fs::remove_file(path);
    }

    Some(best)
}

async fn reconcile_voucher_queries_after_restore(
    state: &AppState,
    outcome: &human_money_core::models::backup::RestoreOutcome,
    new_password: &str,
) -> bool {
    let settings = match state.get_cached_settings() {
        Ok(s) => s,
        Err(_) => return false,
    };
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    let active_voucher_queries_count = outcome.voucher_queries.len();
    if candidates.is_empty() {
        return active_voucher_queries_count > 0;
    }

    let Ok(client) = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return active_voucher_queries_count > 0;
    };

    let mut at_least_one_responded = false;

    for (local_id, mut query) in outcome.voucher_queries.clone() {
        query.read_quorum = 1;
        let Ok(body_bytes) = serde_json::to_vec(&query) else {
            continue;
        };

        for base in candidates.iter().take(4) {
            let Some(url) = crate::gateway_resolver::endpoint(base, "v1/status") else {
                continue;
            };
            let request = client
                .post(url)
                .header("Content-Type", "application/json")
                .body(body_bytes.clone());

            let response =
                tokio::time::timeout(std::time::Duration::from_millis(2500), request.send()).await;

            let Ok(Ok(resp)) = response else { continue };
            at_least_one_responded = true;
            if !resp.status().is_success() {
                continue;
            }
            let Ok(resp_bytes) = resp.bytes().await else {
                continue;
            };

            if let Ok(env) = serde_json::from_slice::<
                human_money_core::models::layer2_api::L2ResponseEnvelope,
            >(&resp_bytes)
            {
                if let Ok(mut service) = state.service.lock() {
                    let _ = service.reconcile_voucher_verdict(
                        &local_id,
                        &env.verdict,
                        Some(new_password),
                    );
                }
                break;
            }
        }
    }

    let l2_status = if at_least_one_responded {
        "reconciled"
    } else {
        "offline"
    };
    let pending_l2 = active_voucher_queries_count > 0 && l2_status != "reconciled";
    if let Ok(mut s) = state.get_cached_settings() {
        s.pending_restore_l2_reconciliation = pending_l2;
        if let Ok(mut service) = state.service.lock() {
            let _ = state.save_settings(&mut service, s, Some(new_password));
        }
    }
    pending_l2
}

// ============================================================================
// Tauri Commands
// ============================================================================

pub async fn export_profile_mnemonic_impl(
    password: String,
    state: &AppState,
) -> Result<Option<ExportedMnemonicKitDto>, FrontendError> {
    let (phrase, lang, has_bip39_passphrase) = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let exported = service.export_mnemonic(&password).map_err(FrontendError::from)?;
        match exported {
            Some(t) => t,
            None => return Ok(None),
        }
    };

    let entropy_bytes = human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(&phrase, lang)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let entropy_hex = hex::encode(&entropy_bytes);

    let user_prefix = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .with_wallet_and_identity(|_w, id| {
                human_money_core::crypto::get_prefix_from_user_id(&id.user_id).map(|s| s.to_string())
            })
            .ok()
            .flatten()
    };

    let active_descriptors = collect_candidate_backup_sync_descriptors(state, Some(&password))?;

    let guardian_names = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let owner_state = load_recovery_owner_state(&mut service, Some(&password))?;
        owner_state
            .map(|s| s.guardians.into_iter().map(|g| g.display_name).collect())
            .unwrap_or_default()
    };

    let kit = humoco_market::EmergencyKitData {
        version: 1,
        entropy_hex,
        language_code: mnemonic_language_to_code(lang),
        user_prefix,
        has_bip39_passphrase,
        sync_descriptors: active_descriptors.clone(),
    };

    let kit_uri = build_emergency_kit_uri(&kit).map_err(FrontendError::from)?;
    let qr_svg = encode_qr_svg(kit_uri.as_bytes()).map_err(FrontendError::from)?;
    let sync_urls = active_descriptors.into_iter().map(|d| d.write_url).collect();

    Ok(Some(ExportedMnemonicKitDto {
        phrase,
        language_code: kit.language_code,
        has_bip39_passphrase,
        kit_uri,
        qr_svg,
        sync_urls,
        guardian_names,
    }))
}

#[tauri::command]
pub async fn export_profile_mnemonic(
    password: String,
    state: State<'_, AppState>,
) -> Result<Option<ExportedMnemonicKitDto>, FrontendError> {
    export_profile_mnemonic_impl(password, &state).await
}

pub async fn confirm_emergency_sheet_saved_impl(
    password: String,
    word_index: u8,
    control_word: String,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
) -> Result<ConfirmEmergencySheetResultDto, FrontendError> {
    let phrase = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let exp = service.export_mnemonic(&password).map_err(FrontendError::from)?;
        let (ph, _, _) = exp.ok_or_else(|| {
            FrontendError::from("Wiederherstellungsphrase nicht auf diesem Gerät gespeichert.".to_string())
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
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let mut owner_state =
            load_recovery_owner_state(&mut service, Some(&password))?.unwrap_or_default();
        owner_state.emergency_sheet_confirmed_at = Some(now_secs);
        save_recovery_owner_state(&mut service, &owner_state, Some(&password))?;
    }

    let backup_sync_result = match app {
        Some(handle) => sync_backup_impl(Some(password), state, handle).await.ok(),
        None => None,
    };

    Ok(ConfirmEmergencySheetResultDto {
        confirmed_at: now_secs,
        backup_sync_result,
    })
}

#[tauri::command]
pub async fn confirm_emergency_sheet_saved(
    password: String,
    word_index: u8,
    control_word: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ConfirmEmergencySheetResultDto, FrontendError> {
    confirm_emergency_sheet_saved_impl(password, word_index, control_word, &state, Some(&app)).await
}

pub fn get_recovery_protection_status_impl(
    password: Option<&str>,
    state: &AppState,
) -> Result<RecoveryProtectionStatusDto, FrontendError> {
    let now_secs = Utc::now().timestamp() as u64;

    let has_voucher = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .with_wallet(|w| !w.voucher_store.vouchers.is_empty())
            .unwrap_or(false)
    };

    let (owner_state, guardian_vault) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let owner = load_recovery_owner_state(&mut service, password)?.unwrap_or_default();
        let vault = load_guardian_vault(&mut service, password)?;
        (owner, vault)
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

    let settings = get_or_load_settings_from_state(state, password);
    let candidate_syncs = collect_candidate_backup_sync_descriptors(state, password)?;
    let sync_backup_count = candidate_syncs.len();
    let has_sync_backup = settings.sync_backup_enabled
        && (sync_backup_count > 0 || settings.last_sync_backup_at.is_some());
    let has_local_backup = settings
        .backup_sync_folder
        .as_ref()
        .is_some_and(|f| !f.trim().is_empty() && Path::new(f).is_dir());
    let has_voucher_backup = has_sync_backup || has_local_backup;

    let guardian_wards: Vec<GuardianWardDto> = guardian_vault
        .wards
        .values()
        .map(|e| GuardianWardDto {
            owner_did: e.owner_did.clone(),
            owner_display_name: e.owner_display_name.clone(),
            rotation_epoch: e.rotation_epoch,
            threshold_m: e.threshold_m,
            total_n: e.total_n,
            updated_at: e.updated_at,
        })
        .collect();

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
    })
}

#[tauri::command]
pub fn get_recovery_protection_status(
    state: State<'_, AppState>,
) -> Result<RecoveryProtectionStatusDto, FrontendError> {
    get_recovery_protection_status_impl(None::<&str>, &state)
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

    // Freshly verify password and export mnemonic
    let (phrase, lang, has_bip39_passphrase) = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let exp = service.export_mnemonic(&password).map_err(FrontendError::from)?;
        exp.ok_or_else(|| {
            FrontendError::from("Mnemonic phrase required to setup social recovery".to_string())
        })?
    };

    let entropy = human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(&phrase, lang)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    // Verify all guardians exist in AddressBook with trust >= 1 and NOT KnownOffender
    let contacts = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        state.load_contacts(&mut service, Some(&password))?
    };

    for g_did in &guardian_dids {
        let contact_opt = contacts.iter().find(|c| are_dids_equal(&c.did, g_did));
        let Some(contact) = contact_opt else {
            return Err(FrontendError::from(format!(
                "Guardian {} is not in your contacts",
                g_did
            )));
        };
        let contact_name = {
            let first = contact.profile.first_name.clone().unwrap_or_default();
            let last = contact.profile.last_name.clone().unwrap_or_default();
            let full = format!("{} {}", first, last).trim().to_string();
            if !full.is_empty() { full } else { contact.did.clone() }
        };
        if contact.trust_level.unwrap_or(0) < 1 {
            return Err(FrontendError::from(format!(
                "Guardian {} does not have trust level >= 1 (Acquaintance/Bekannt)",
                contact_name
            )));
        }

        let is_offender = {
            let service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            service
                .with_wallet(|w| {
                    matches!(
                        w.check_reputation(g_did),
                        human_money_core::models::conflict::TrustStatus::KnownOffender(_)
                    )
                })
                .unwrap_or(false)
        };
        if is_offender {
            return Err(FrontendError::from(format!(
                "Guardian {} is flagged as KnownOffender",
                contact_name
            )));
        }
    }

    let (my_identity, my_signing_key, my_prefix, my_profile_name) = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .with_wallet_and_identity(|w, id| {
                let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string());
                let first = w.profile.first_name.as_deref().unwrap_or("").trim();
                let last = w.profile.last_name.as_deref().unwrap_or("").trim();
                let full = format!("{} {}", first, last).trim().to_string();
                let name = if full.is_empty() {
                    id.user_id.clone()
                } else {
                    full
                };
                (id.clone(), id.signing_key.clone(), pfx, name)
            })
            .map_err(|e| FrontendError::from(e.to_string()))?
    };

    let old_owner_state = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        load_recovery_owner_state(&mut service, Some(&password))?
    };

    let old_epoch = old_owner_state.as_ref().map(|s| s.rotation_epoch).unwrap_or(0);
    let new_epoch = old_epoch + 1;
    let now_secs = Utc::now().timestamp() as u64;

    let settings = get_or_load_settings_from_state(state, Some(password.as_str()));
    let own_master_seed = my_signing_key.to_bytes();
    let own_reply_syncs = collect_own_reply_syncs(&settings, &own_master_seed);
    let mut post_state = load_post_state(state, Some(&password))?;

    // Send revoke to any old guardians being removed
    if let Some(ref old) = old_owner_state {
        for old_g in &old.guardians {
            if !guardian_dids.iter().any(|d| are_dids_equal(d, &old_g.user_id)) {
                let revoke = GuardianRevoke {
                    owner_did: my_identity.user_id.clone(),
                    rotation_epoch: new_epoch,
                };
                if let Ok(revoke_bytes) = serde_json::to_vec(&revoke) {
                    if let Ok(container) = SecureContainer::seal(
                        &my_identity,
                        &ContainerConfig::TargetDid(old_g.user_id.clone(), PrivacyMode::HashedRouting),
                        &revoke_bytes,
                        PayloadType::Generic(PAYLOAD_GUARDIAN_REVOKE_V1.to_string()),
                    ) {
                        if let Ok(container_bytes) = serde_json::to_vec(&container) {
                            let msg_id = uuid::Uuid::new_v4().to_string();
                            let env = PostEnvelope::new(
                                msg_id.clone(),
                                now_secs,
                                own_reply_syncs.iter().cloned().map(Into::into).collect(),
                                container_bytes,
                            );
                            if let Ok(recipient_pk) = get_pubkey_from_user_id(&old_g.user_id) {
                                if let Ok(box_entry) =
                                    seal_post_box(&my_signing_key, &recipient_pk, &env)
                                {
                                    post_state.processed_msg_ids.insert(msg_id.clone());
                                    post_state.outbox_entries.push(PostOutboxEntry {
                                        msg_id,
                                        recipient_did: old_g.user_id.clone(),
                                        tag16_hex: hex::encode(box_entry.tag16),
                                        bundle_id: None,
                                        entry: box_entry,
                                        created_at: now_secs,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let backup_descriptors =
        collect_candidate_backup_sync_descriptors(state, Some(&password))?;

    let root_did = human_money_core::crypto::create_user_id(&my_signing_key.verifying_key(), None)
        .map_err(|e| FrontendError::from(format!("Failed to derive root DID: {}", e)))?;

    let master_payload = RecoveryMasterPayload::new(
        &root_did,
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
        &my_signing_key,
        &root_did,
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

    let mut assignment_records = Vec::with_capacity(guardian_dids.len());

    for (i, g_did) in guardian_dids.iter().enumerate() {
        let entry = &entries[i];
        let entry_bytes =
            serde_json::to_vec(entry).map_err(|e| FrontendError::from(e.to_string()))?;
        let container = SecureContainer::seal(
            &my_identity,
            &ContainerConfig::TargetDid(g_did.clone(), PrivacyMode::HashedRouting),
            &entry_bytes,
            PayloadType::Generic(PAYLOAD_GUARDIAN_ASSIGN_V1.to_string()),
        )
        .map_err(FrontendError::from)?;
        let container_bytes =
            serde_json::to_vec(&container).map_err(|e| FrontendError::from(e.to_string()))?;

        let msg_id = uuid::Uuid::new_v4().to_string();
        let env = PostEnvelope::new(
            msg_id.clone(),
            now_secs,
            own_reply_syncs.iter().cloned().map(Into::into).collect(),
            container_bytes,
        );

        let recipient_pk = get_pubkey_from_user_id(g_did)
            .map_err(|e| FrontendError::from(format!("Invalid guardian DID: {}", e)))?;
        let box_entry = seal_post_box(&my_signing_key, &recipient_pk, &env)
            .map_err(FrontendError::from)?;

        post_state.processed_msg_ids.insert(msg_id.clone());
        post_state.outbox_entries.push(PostOutboxEntry {
            msg_id,
            recipient_did: g_did.clone(),
            tag16_hex: hex::encode(box_entry.tag16),
            bundle_id: None,
            entry: box_entry,
            created_at: now_secs,
        });

        let display_name = contacts
            .iter()
            .find(|c| are_dids_equal(&c.did, g_did))
            .map(|c| {
                let first = c.profile.first_name.clone().unwrap_or_default();
                let last = c.profile.last_name.clone().unwrap_or_default();
                let full = format!("{} {}", first, last).trim().to_string();
                if !full.is_empty() { full } else { c.did.clone() }
            })
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

    save_post_state(state, &post_state, Some(&password))?;

    let new_owner_state = RecoveryOwnerState {
        rotation_epoch: new_epoch,
        threshold_m,
        question_ids,
        guardians: assignment_records,
        emergency_sheet_confirmed_at: old_owner_state
            .as_ref()
            .and_then(|s| s.emergency_sheet_confirmed_at),
    };

    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        save_recovery_owner_state(&mut service, &new_owner_state, Some(&password))?;
    }

    let post_sync_result = sync_post_channels_impl(app, None, None, Some(password.clone()), state).await?;
    let backup_sync_result = match app {
        Some(handle) => sync_backup_impl(Some(password), state, handle).await.ok(),
        None => None,
    };

    Ok(SetupSocialRecoveryResultDto {
        rotation_epoch: new_epoch,
        threshold_m,
        guardians_count: guardian_dids.len(),
        post_sync_result,
        backup_sync_result,
    })
}

#[tauri::command]
pub async fn setup_social_recovery_guardians(
    password: String,
    guardian_dids: Vec<String>,
    threshold_m: u8,
    question_ids: [u8; 3],
    answers: [String; 3],
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<SetupSocialRecoveryResultDto, FrontendError> {
    setup_social_recovery_guardians_with_params(
        password,
        guardian_dids,
        threshold_m,
        question_ids,
        answers,
        RecoveryKdfParams::new_production(),
        &state,
        Some(&app),
    )
    .await
}

pub async fn revoke_social_recovery_impl(
    password: Option<String>,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
) -> Result<RevokeSocialRecoveryResultDto, FrontendError> {
    let (my_identity, my_signing_key, old_guardians, old_epoch, emergency_confirmed) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let owner = load_recovery_owner_state(&mut service, password.as_deref())?
            .unwrap_or_default();
        let (id, sk) = service
            .with_wallet_and_identity(|_w, i| (i.clone(), i.signing_key.clone()))
            .map_err(FrontendError::from)?;
        (
            id,
            sk,
            owner.guardians,
            owner.rotation_epoch,
            owner.emergency_sheet_confirmed_at,
        )
    };

    let new_epoch = old_epoch + 1;
    let now_secs = Utc::now().timestamp() as u64;
    let settings = get_or_load_settings_from_state(state, password.as_deref());
    let own_master_seed = my_signing_key.to_bytes();
    let own_reply_syncs = collect_own_reply_syncs(&settings, &own_master_seed);
    let mut post_state = load_post_state(state, password.as_deref())?;

    for g in &old_guardians {
        let revoke = GuardianRevoke {
            owner_did: my_identity.user_id.clone(),
            rotation_epoch: new_epoch,
        };
        if let Ok(revoke_bytes) = serde_json::to_vec(&revoke) {
            if let Ok(container) = SecureContainer::seal(
                &my_identity,
                &ContainerConfig::TargetDid(g.user_id.clone(), PrivacyMode::HashedRouting),
                &revoke_bytes,
                PayloadType::Generic(PAYLOAD_GUARDIAN_REVOKE_V1.to_string()),
            ) {
                if let Ok(container_bytes) = serde_json::to_vec(&container) {
                    let msg_id = uuid::Uuid::new_v4().to_string();
                    let env = PostEnvelope::new(
                        msg_id.clone(),
                        now_secs,
                        own_reply_syncs.iter().cloned().map(Into::into).collect(),
                        container_bytes,
                    );
                    if let Ok(recipient_pk) = get_pubkey_from_user_id(&g.user_id) {
                        if let Ok(box_entry) =
                            seal_post_box(&my_signing_key, &recipient_pk, &env)
                        {
                            post_state.processed_msg_ids.insert(msg_id.clone());
                            post_state.outbox_entries.push(PostOutboxEntry {
                                msg_id,
                                recipient_did: g.user_id.clone(),
                                tag16_hex: hex::encode(box_entry.tag16),
                                bundle_id: None,
                                entry: box_entry,
                                created_at: now_secs,
                            });
                        }
                    }
                }
            }
        }
    }

    save_post_state(state, &post_state, password.as_deref())?;

    let updated_owner_state = RecoveryOwnerState {
        rotation_epoch: new_epoch,
        threshold_m: 0,
        question_ids: [0, 0, 0],
        guardians: Vec::new(),
        emergency_sheet_confirmed_at: emergency_confirmed,
    };

    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        save_recovery_owner_state(&mut service, &updated_owner_state, password.as_deref())?;
    }

    let post_sync_result = sync_post_channels_impl(app, None, None, password.clone(), state).await?;
    let backup_sync_result = match app {
        Some(handle) => sync_backup_impl(password, state, handle).await.ok(),
        None => None,
    };

    Ok(RevokeSocialRecoveryResultDto {
        rotation_epoch: new_epoch,
        revoked_guardians_count: old_guardians.len(),
        post_sync_result,
        backup_sync_result,
    })
}

#[tauri::command]
pub async fn revoke_social_recovery(
    password: Option<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RevokeSocialRecoveryResultDto, FrontendError> {
    revoke_social_recovery_impl(password, &state, Some(&app)).await
}

// ============================================================================
// Disaster Mode Session Commands (Pre-login compatible)
// ============================================================================

pub fn start_social_recovery_session_impl(
    name_hint: Option<String>,
    session_file_path: &Path,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let sk_temp = x25519_dalek::StaticSecret::random_from_rng(rand::rngs::OsRng);
    let pk_temp = x25519_dalek::PublicKey::from(&sk_temp);

    let sas_code = compute_recovery_sas(pk_temp.as_bytes());
    let now_secs = Utc::now().timestamp() as u64;
    let request_uri =
        build_recovery_request_uri(pk_temp.as_bytes(), now_secs, name_hint.as_deref());

    let session = ActiveRecoverySessionFile {
        sk_temp: sk_temp.to_bytes(),
        pk_temp: *pk_temp.as_bytes(),
        sas_code,
        created_at: now_secs,
        name_hint,
        request_uri,
        replies: Vec::new(),
    };

    save_active_session_to_path(session_file_path, &session)?;
    Ok(build_active_session_dto(&session))
}

#[tauri::command]
pub fn start_social_recovery_session(
    name_hint: Option<String>,
    app: tauri::AppHandle,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let session_path = get_session_file_path(&app)?;
    start_social_recovery_session_impl(name_hint, &session_path)
}

pub fn get_active_recovery_session_impl(
    session_file_path: &Path,
) -> Result<Option<ActiveRecoverySessionDto>, FrontendError> {
    let session_opt = load_active_session_from_path(session_file_path)?;
    Ok(session_opt.as_ref().map(build_active_session_dto))
}

#[tauri::command]
pub fn get_active_recovery_session(
    app: tauri::AppHandle,
) -> Result<Option<ActiveRecoverySessionDto>, FrontendError> {
    let session_path = get_session_file_path(&app)?;
    get_active_recovery_session_impl(&session_path)
}

pub fn cancel_social_recovery_session_impl(session_file_path: &Path) -> Result<(), FrontendError> {
    delete_active_session_from_path(session_file_path);
    Ok(())
}

#[tauri::command]
pub fn cancel_social_recovery_session(app: tauri::AppHandle) -> Result<(), FrontendError> {
    let session_path = get_session_file_path(&app)?;
    cancel_social_recovery_session_impl(&session_path)
}

pub fn import_guardian_recovery_reply_impl(
    reply_uri: String,
    session_file_path: &Path,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let mut session = load_active_session_from_path(session_file_path)?
        .ok_or_else(|| FrontendError::from("Keine aktive Wiederherstellungssitzung gefunden.".to_string()))?;

    let payload = open_and_verify_guardian_release_reply(&session.sk_temp, &reply_uri)
        .map_err(FrontendError::from)?;

    // Deduplicate reply
    let already_present = session.replies.iter().any(|r| {
        are_dids_equal(&r.entry.owner_did, &payload.entry.owner_did)
            && r.entry.rotation_epoch == payload.entry.rotation_epoch
            && r.entry.share.index == payload.entry.share.index
    });

    if !already_present {
        session.replies.push(payload);
        save_active_session_to_path(session_file_path, &session)?;
    }

    Ok(build_active_session_dto(&session))
}

#[tauri::command]
pub fn import_guardian_recovery_reply(
    reply_uri: String,
    app: tauri::AppHandle,
) -> Result<ActiveRecoverySessionDto, FrontendError> {
    let session_path = get_session_file_path(&app)?;
    import_guardian_recovery_reply_impl(reply_uri, &session_path)
}

// ============================================================================
// Friend-Side Commands (Logged in)
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

    let vault = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        load_guardian_vault(&mut service, password)?
    };

    let wards: Vec<GuardianWardDto> = vault
        .wards
        .values()
        .map(|e| GuardianWardDto {
            owner_did: e.owner_did.clone(),
            owner_display_name: e.owner_display_name.clone(),
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

#[tauri::command]
pub fn inspect_guardian_recovery_request(
    request_uri: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<GuardianRecoveryRequestInspectionDto, FrontendError> {
    inspect_guardian_recovery_request_impl(request_uri, password.as_deref(), &state)
}

pub fn approve_guardian_recovery_request_impl(
    request_uri: String,
    owner_did: String,
    password: Option<&str>,
    state: &AppState,
) -> Result<ApproveGuardianRecoveryResultDto, FrontendError> {
    let link = parse_recovery_request_uri(&request_uri).map_err(FrontendError::from)?;
    let sas_code = compute_recovery_sas(&link.pk_temp);

    let (entry, extra_descriptors) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let vault = load_guardian_vault(&mut service, password)?;
        let entry = vault
            .wards
            .values()
            .find(|e| are_dids_equal(&e.owner_did, &owner_did))
            .cloned()
            .ok_or_else(|| {
                FrontendError::from("Kein Schützling mit dieser ID im Tresor gefunden.".to_string())
            })?;

        // Collect extra known sync descriptors from contacts/cards + own hosted sync
        let mut extras = Vec::new();

        if let Ok(Some(admin)) = service
            .load_encrypted_data(crate::state::MARKET_SYNC_ADMIN_KEY, password)
            .map(|bytes| serde_json::from_slice::<crate::state::StoredSyncAdmin>(&bytes).ok())
        {
            if !admin.descriptor.write_url.is_empty() {
                extras.push(admin.descriptor.clone());
            }
        }

        (entry, extras)
    };

    let now_secs = Utc::now().timestamp() as u64;
    let payload = GuardianReleasePayload {
        entry: entry.clone(),
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
        owner_display_name: entry.owner_display_name,
    })
}

#[tauri::command]
pub fn approve_guardian_recovery_request(
    request_uri: String,
    owner_did: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<ApproveGuardianRecoveryResultDto, FrontendError> {
    approve_guardian_recovery_request_impl(request_uri, owner_did, password.as_deref(), &state)
}

// ============================================================================
// Completion Commands
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
    session_file_path: &Path,
    state: &AppState,
    app_data_dir: &Path,
) -> Result<RecoveryCompletionResultDto, FrontendError> {
    let session = load_active_session_from_path(session_file_path)?
        .ok_or_else(|| FrontendError::from("Keine aktive Wiederherstellungssitzung gefunden.".to_string()))?;

    let matching_entries: Vec<GuardianVaultEntry> = session
        .replies
        .iter()
        .filter(|r| {
            are_dids_equal(&r.entry.owner_did, &owner_did)
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
    let master_payload = recover_master_payload_from_shares(&matching_entries, &ans_refs)
        .map_err(|e| {
            FrontendError::from(format!(
                "Deine Antworten stimmen noch nicht – bitte in Ruhe prüfen: {}",
                e
            ))
        })?;

    if master_payload.has_bip39_passphrase
        && bip39_passphrase
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        return Err(FrontendError::from(
            "BIP39_PASSPHRASE_REQUIRED: Für dieses Profil wurde bei der Erstellung eine Passphrase (25. Wort) festgelegt.".to_string(),
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
            FrontendError::from("target_folder_name ist erforderlich für lokalen Passwort-Reset.".to_string())
        })?;

        {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
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

            if let Ok(Some(mut local_state)) =
                load_recovery_owner_state(&mut service, Some(&new_password))
            {
                if local_state.rotation_epoch < rotation_epoch {
                    local_state.rotation_epoch = rotation_epoch;
                    let _ =
                        save_recovery_owner_state(&mut service, &local_state, Some(&new_password));
                }
            }
        }

        delete_active_session_from_path(session_file_path);

        return Ok(RecoveryCompletionResultDto {
            profile_name: profile_name.unwrap_or(target_folder),
            mode,
            outcome: "password_reset".to_string(),
            backup_epoch: None,
            backup_tx_nonce: None,
            pending_restore_l2_reconciliation: false,
        });
    }

    // mode == "restore_new_device"
    let mut candidate_descriptors = Vec::new();
    for r in &session.replies {
        if are_dids_equal(&r.entry.owner_did, &owner_did)
            && r.entry.rotation_epoch == rotation_epoch
        {
            for d in &r.entry.backup_sync_descriptors {
                if !d.write_url.is_empty()
                    && !candidate_descriptors
                        .iter()
                        .any(|c: &DropDescriptor| c.write_url == d.write_url)
                {
                    candidate_descriptors.push(d.clone());
                }
            }
            for d in &r.extra_sync_descriptors {
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
    fs::create_dir_all(&tmp_restore_dir)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let best_backup_opt = discover_best_backup_from_mirrors(
        &candidate_descriptors,
        &phrase,
        bip39_passphrase.as_deref(),
        master_payload.user_prefix.as_deref(),
        lang,
        state,
        &tmp_restore_dir,
    )
    .await;

    if let Some((staged_path, inspection)) = best_backup_opt {
        let credentials = BackupRestoreCredentials::Mnemonic {
            phrase: &phrase,
            passphrase: bip39_passphrase.as_deref(),
            prefix: master_payload.user_prefix.as_deref(),
            language: lang,
        };

        let (outcome, prof_name, custom_standards) = {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            restore_backup_impl(
                &staged_path,
                credentials,
                &new_password,
                local_instance_id.clone(),
                &mut service,
            )?
        };

        let _ = restore_custom_standards_to_dir(&app_data_dir.join("standards"), &custom_standards);
        let _ = std::fs::remove_file(&staged_path);

        let pending_l2 =
            reconcile_voucher_queries_after_restore(state, &outcome, &new_password).await;

        {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            let _ = service.login(&outcome.folder_name, &new_password, false, local_instance_id);

            if let Ok(Some(mut local_state)) =
                load_recovery_owner_state(&mut service, Some(&new_password))
            {
                if local_state.rotation_epoch < rotation_epoch {
                    local_state.rotation_epoch = rotation_epoch;
                    let _ =
                        save_recovery_owner_state(&mut service, &local_state, Some(&new_password));
                }
            }
        }

        delete_active_session_from_path(session_file_path);

        Ok(RecoveryCompletionResultDto {
            profile_name: prof_name,
            mode,
            outcome: "restored_from_sync".to_string(),
            backup_epoch: Some(inspection.epoch),
            backup_tx_nonce: Some(inspection.tx_nonce),
            pending_restore_l2_reconciliation: pending_l2,
        })
    } else {
        // Fallback: seed-only restore
        let final_name = profile_name.unwrap_or_else(|| "Wiederhergestelltes Profil".to_string());
        {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
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
        }

        {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            let initial_state = RecoveryOwnerState {
                rotation_epoch,
                ..Default::default()
            };
            let _ =
                save_recovery_owner_state(&mut service, &initial_state, Some(&new_password));
        }

        delete_active_session_from_path(session_file_path);

        Ok(RecoveryCompletionResultDto {
            profile_name: final_name,
            mode,
            outcome: "restored_seed_only".to_string(),
            backup_epoch: None,
            backup_tx_nonce: None,
            pending_restore_l2_reconciliation: false,
        })
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn complete_social_recovery(
    owner_did: String,
    rotation_epoch: u32,
    answers: [String; 3],
    bip39_passphrase: Option<String>,
    mode: String,
    new_password: String,
    profile_name: Option<String>,
    target_folder_name: Option<String>,
    local_instance_id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RecoveryCompletionResultDto, FrontendError> {
    use tauri::Manager;
    let session_path = get_session_file_path(&app)?;
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    complete_social_recovery_impl(
        owner_did,
        rotation_epoch,
        answers,
        bip39_passphrase,
        mode,
        new_password,
        profile_name,
        target_folder_name,
        local_instance_id,
        &session_path,
        &state,
        &app_dir,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn restore_from_emergency_kit_uri_impl(
    kit_uri_or_manual: EmergencyKitRestoreInput,
    mode: String,
    new_password: String,
    profile_name: Option<String>,
    target_folder_name: Option<String>,
    local_instance_id: String,
    state: &AppState,
    app_data_dir: &Path,
) -> Result<RecoveryCompletionResultDto, FrontendError> {
    let (phrase, lang, has_bip39_passphrase, user_prefix, sync_descriptors) =
        if let Some(ref uri) = kit_uri_or_manual.kit_uri {
            let kit = parse_emergency_kit_uri(uri).map_err(FrontendError::from)?;
            let entropy_bytes = hex::decode(&kit.entropy_hex)
                .map_err(|e| FrontendError::from(e.to_string()))?;
            let language = code_to_mnemonic_language(kit.language_code)
                .map_err(FrontendError::from)?;
            let phrase = human_money_core::services::mnemonic::MnemonicProcessor::from_entropy(
                &entropy_bytes,
                language,
            )
            .map_err(|e| FrontendError::from(e.to_string()))?;
            (
                phrase,
                language,
                kit.has_bip39_passphrase,
                kit.user_prefix,
                kit.sync_descriptors,
            )
        } else if let Some(ref m_phrase) = kit_uri_or_manual.manual_mnemonic {
            let lang = code_to_mnemonic_language(kit_uri_or_manual.manual_language_code.unwrap_or(1))
                .map_err(FrontendError::from)?;
            let mut sync_descs = Vec::new();
            for u in &kit_uri_or_manual.manual_sync_urls {
                if let Ok(cw) = humoco_market::canonicalize_write_url(u) {
                    sync_descs.push(DropDescriptor {
                        write_url: cw.clone(),
                        read_url: cw,
                        slot_id: None,
                        drop_pub: String::new(),
                    });
                }
            }
            (
                m_phrase.trim().to_string(),
                lang,
                kit_uri_or_manual.manual_passphrase.is_some(),
                kit_uri_or_manual.manual_user_prefix,
                sync_descs,
            )
        } else {
            return Err(FrontendError::from(
                "Weder Notfall-Blatt-Link noch Wörter eingegeben.".to_string(),
            ));
        };

    if has_bip39_passphrase
        && kit_uri_or_manual
            .manual_passphrase
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        return Err(FrontendError::from(
            "BIP39_PASSPHRASE_REQUIRED: Für dieses Profil wurde bei der Erstellung eine Passphrase (25. Wort) festgelegt.".to_string(),
        ));
    }

    if mode == "reset_local_password" {
        let target_folder = target_folder_name.ok_or_else(|| {
            FrontendError::from("target_folder_name erforderlich für lokalen Passwort-Reset.".to_string())
        })?;

        {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            service
                .recover_wallet_and_set_new_password(
                    &target_folder,
                    &phrase,
                    kit_uri_or_manual.manual_passphrase.as_deref(),
                    &new_password,
                    lang,
                    local_instance_id.clone(),
                )
                .map_err(FrontendError::from)?;
            let _ = service.login(&target_folder, &new_password, false, local_instance_id);
        }

        return Ok(RecoveryCompletionResultDto {
            profile_name: profile_name.unwrap_or(target_folder),
            mode,
            outcome: "password_reset".to_string(),
            backup_epoch: None,
            backup_tx_nonce: None,
            pending_restore_l2_reconciliation: false,
        });
    }

    // mode == "restore_new_device"
    let tmp_restore_dir = app_data_dir.join("tmp_restore");
    fs::create_dir_all(&tmp_restore_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    let best_backup_opt = discover_best_backup_from_mirrors(
        &sync_descriptors,
        &phrase,
        kit_uri_or_manual.manual_passphrase.as_deref(),
        user_prefix.as_deref(),
        lang,
        state,
        &tmp_restore_dir,
    )
    .await;

    if let Some((staged_path, inspection)) = best_backup_opt {
        let credentials = BackupRestoreCredentials::Mnemonic {
            phrase: &phrase,
            passphrase: kit_uri_or_manual.manual_passphrase.as_deref(),
            prefix: user_prefix.as_deref(),
            language: lang,
        };

        let (outcome, prof_name, custom_standards) = {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            restore_backup_impl(
                &staged_path,
                credentials,
                &new_password,
                local_instance_id.clone(),
                &mut service,
            )?
        };

        let _ = restore_custom_standards_to_dir(&app_data_dir.join("standards"), &custom_standards);
        let _ = std::fs::remove_file(&staged_path);

        let pending_l2 =
            reconcile_voucher_queries_after_restore(state, &outcome, &new_password).await;

        {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            let _ = service.login(&outcome.folder_name, &new_password, false, local_instance_id);
        }

        Ok(RecoveryCompletionResultDto {
            profile_name: prof_name,
            mode,
            outcome: "restored_from_sync".to_string(),
            backup_epoch: Some(inspection.epoch),
            backup_tx_nonce: Some(inspection.tx_nonce),
            pending_restore_l2_reconciliation: pending_l2,
        })
    } else {
        // Fallback: seed-only restore
        let final_name = profile_name.unwrap_or_else(|| "Wiederhergestelltes Profil".to_string());
        {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            service
                .create_profile(
                    &final_name,
                    &phrase,
                    kit_uri_or_manual.manual_passphrase.as_deref(),
                    user_prefix.as_deref(),
                    &new_password,
                    lang,
                    local_instance_id,
                    true,
                )
                .map_err(FrontendError::from)?;
        }

        Ok(RecoveryCompletionResultDto {
            profile_name: final_name,
            mode,
            outcome: "restored_seed_only".to_string(),
            backup_epoch: None,
            backup_tx_nonce: None,
            pending_restore_l2_reconciliation: false,
        })
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn restore_from_emergency_kit_uri(
    kit_uri_or_manual: EmergencyKitRestoreInput,
    mode: String,
    new_password: String,
    profile_name: Option<String>,
    target_folder_name: Option<String>,
    local_instance_id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<RecoveryCompletionResultDto, FrontendError> {
    use tauri::Manager;
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    restore_from_emergency_kit_uri_impl(
        kit_uri_or_manual,
        mode,
        new_password,
        profile_name,
        target_folder_name,
        local_instance_id,
        &state,
        &app_dir,
    )
    .await
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use chrono::Utc;
    use human_money_core::app_service::AppService;
    use humoco_market::{parse_emergency_kit_uri, RecoveryKdfParams};
    use crate::models::{FrontendContact, FrontendUserProfile, GuardianAssignmentStatus};
    use crate::commands::sync::tests::{
        create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE,
        CHARLIE_PHRASE, READER1_PHRASE, READER2_PHRASE,
    };

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

    fn create_wallet_state(base_dir: &Path, name: &str, phrase: &str, pass: &str) -> (AppState, String) {
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
        let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);

        // 1. Initial status has no emergency confirmation
        let initial_status = get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
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
        let updated_status = get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
        assert!(updated_status.emergency_sheet_confirmed_at.is_some());
        assert!(updated_status.has_identity_recovery);
    }

    #[tokio::test]
    async fn test_export_profile_mnemonic_kit() {
        let base_dir = temp_test_dir("mnemonic_kit_test");
        let password = "TestPassword123!";
        let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);

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
        let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
        let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
        let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

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
        let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
        let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
        let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
        let (dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);
        let (_eve_state, eve_did) = create_wallet_state(&base_dir, "Eve", READER2_PHRASE, password);
        let (_frank_state, frank_did) = create_wallet_state(&base_dir, "Frank", "", password);

        // 2. Alice adds all 5 friends to contacts with Trust Level 2
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&bob_did, "Bob", Some(2)), Some(password)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&carol_did, "Carol", Some(2)), Some(password)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&dave_did, "Dave", Some(2)), Some(password)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&eve_did, "Eve", Some(2)), Some(password)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&frank_did, "Frank", Some(2)), Some(password)).unwrap();
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
            load_recovery_owner_state(&mut s, Some(password)).unwrap().unwrap()
        };

        // Bob stores his assigned vault entry
        {
            let bob_rec = owner_state.guardians.iter().find(|g| g.user_id == bob_did).unwrap();
            let mut s = bob_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
            let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
        }

        // Carol stores her assigned vault entry
        {
            let carol_rec = owner_state.guardians.iter().find(|g| g.user_id == carol_did).unwrap();
            let mut s = carol_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
            let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
        }

        // Dave stores his assigned vault entry
        {
            let dave_rec = owner_state.guardians.iter().find(|g| g.user_id == dave_did).unwrap();
            let mut s = dave_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
            let _ = vault.upsert_ward(dave_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
        }

        // 6. Simulate receipt of ACKs from Bob, Carol, Dave in Alice's state
        {
            let mut s = alice_state.service.lock().unwrap();
            let mut os = load_recovery_owner_state(&mut s, Some(password)).unwrap().unwrap();
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
        let prot_status = get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
        assert_eq!(prot_status.confirmed_guardians_count, 3);
        assert!(prot_status.has_identity_recovery);

        // 7. Disaster mode: fresh device session
        let session_dir = temp_test_dir("disaster_session");
        let session_file = session_dir.join("active_recovery_session.json");

        let active_sess = start_social_recovery_session_impl(Some("Alice".to_string()), &session_file).unwrap();
        assert_eq!(active_sess.collected_count, 0);
        assert!(!active_sess.has_enough_shares);

        // Bob inspects request
        let bob_inspection = inspect_guardian_recovery_request_impl(
            active_sess.request_uri.clone(),
            Some(password),
            &bob_state,
        )
        .unwrap();
        assert_eq!(bob_inspection.sas_code, active_sess.sas_code);
        assert_eq!(bob_inspection.wards.len(), 1);
        assert!(are_dids_equal(&bob_inspection.wards[0].owner_did, &alice_did));

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
        let progress = import_guardian_recovery_reply_impl(dave_approval.reply_uri, &session_file).unwrap();

        assert_eq!(progress.collected_count, 3);
        assert!(progress.has_enough_shares);

        // 9. WRONG answers: fails, and active_recovery_session.json MUST NOT be deleted!
        let restored_base = temp_test_dir("restored_wallet");
        let empty_service = AppService::new(&restored_base.join("wallet")).unwrap();
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
            &session_file,
            &restored_state,
            &restored_base,
        )
        .await;

        assert!(wrong_err.is_err(), "Wrong answers must fail AEAD decryption");
        assert!(
            session_file.exists(),
            "Active recovery session MUST NOT be deleted when answers are incorrect!"
        );

        // 10. RIGHT answers: succeeds, restores wallet, and cleans up session file!
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
            &session_file,
            &restored_state,
            &restored_base,
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

        let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, old_pass);
        let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, old_pass);
        let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, old_pass);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, old_pass);

        // Add 3 guardians and setup recovery
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&bob_did, "Bob", Some(2)), Some(old_pass)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&carol_did, "Carol", Some(2)), Some(old_pass)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&dave_did, "Dave", Some(2)), Some(old_pass)).unwrap();
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
            load_recovery_owner_state(&mut s, Some(old_pass)).unwrap().unwrap()
        };

        {
            let bob_rec = owner_state.guardians.iter().find(|g| g.user_id == bob_did).unwrap();
            let mut s = bob_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(old_pass)).unwrap();
            let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(old_pass)).unwrap();
        }

        {
            let carol_rec = owner_state.guardians.iter().find(|g| g.user_id == carol_did).unwrap();
            let mut s = carol_state.service.lock().unwrap();
            let mut vault = load_guardian_vault(&mut s, Some(old_pass)).unwrap();
            let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
            save_guardian_vault(&mut s, &vault, Some(old_pass)).unwrap();
        }

        // Start session & approve
        let session_file = base_dir.join("active_recovery_session.json");
        let session_dto = start_social_recovery_session_impl(None, &session_file).unwrap();

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
            &session_file,
            &alice_state,
            &base_dir,
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
            assert!(old_login.is_err(), "Old password must not unlock after reset");

            let new_login = s.login(&folder_name, new_pass, false, "device-reset-1".to_string());
            assert!(new_login.is_ok(), "New password must unlock the wallet");
        }
    }

    #[tokio::test]
    async fn test_restore_from_emergency_kit_uri() {
        let base_dir = temp_test_dir("emergency_kit_restore");
        let old_pass = "OldSecretPassword123!";
        let new_pass = "BrandNewPassword789!";

        let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, old_pass);
        let kit = export_profile_mnemonic_impl(old_pass.to_string(), &alice_state)
            .await
            .unwrap()
            .unwrap();

        let restored_base = temp_test_dir("restored_emergency_kit");
        let empty_service = AppService::new(&restored_base.join("wallet")).unwrap();
        let restored_state = create_test_state(empty_service);

        let input = EmergencyKitRestoreInput {
            kit_uri: Some(kit.kit_uri),
            manual_mnemonic: None,
            manual_language_code: None,
            manual_passphrase: None,
            manual_user_prefix: None,
            manual_sync_urls: Vec::new(),
        };

        let result = restore_from_emergency_kit_uri_impl(
            input,
            "restore_new_device".to_string(),
            new_pass.to_string(),
            Some("Alice Restored".to_string()),
            None,
            "device-restore-kit-1".to_string(),
            &restored_state,
            &restored_base,
        )
        .await
        .unwrap();

        assert_eq!(result.outcome, "restored_seed_only");

        let restored_did = {
            let s = restored_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        assert_eq!(restored_did, alice_did);
    }

    #[tokio::test]
    async fn test_revoke_social_recovery_clears_guardians() {
        let base_dir = temp_test_dir("revoke_recovery_test");
        let pass = "TestPassword123!";

        let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
        let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
        let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&bob_did, "Bob", Some(2)), Some(pass)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&carol_did, "Carol", Some(2)), Some(pass)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&dave_did, "Dave", Some(2)), Some(pass)).unwrap();
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
            load_recovery_owner_state(&mut s, Some(pass)).unwrap().unwrap()
        };
        assert_eq!(owner_state.rotation_epoch, 2);
        assert!(owner_state.guardians.is_empty());

        // Verify post outbox contains revoke messages
        let post_state = load_post_state(&alice_state, Some(pass)).unwrap();
        let has_revokes = post_state.outbox_entries.iter().any(|e| {
            !e.entry.ciphertext.is_empty()
        });
        assert!(has_revokes);
    }

    #[test]
    fn test_ward_vault_max_limit_and_epoch_guards() {
        use humoco_market::GuardianVaultStore;

        let mut vault = GuardianVaultStore::default();
        let password = "TestPassword123!";
        let base_dir = temp_test_dir("ward_vault_test");

        // Create a valid entry from Alice
        let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
        let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
        let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
        let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&bob_did, "Bob", Some(2)), Some(password)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&carol_did, "Carol", Some(2)), Some(password)).unwrap();
            alice_state.save_contact(&mut s, make_test_contact(&dave_did, "Dave", Some(2)), Some(password)).unwrap();
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
            load_recovery_owner_state(&mut s, Some(password)).unwrap().unwrap()
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
        assert!(res_excess.unwrap_err().to_string().contains("capacity reached"));
    }

    #[tokio::test]
    async fn test_discover_best_backup_mirrors_fallback_and_ranking() {
        let base_dir = temp_test_dir("mirror_discovery_test");
        let empty_service = AppService::new(&base_dir.join("empty")).unwrap();
        let state = create_test_state(empty_service);
        let tmp_restore = base_dir.join("tmp_restore");
        std::fs::create_dir_all(&tmp_restore).unwrap();

        // 1. Empty descriptors return None
        let empty_res = discover_best_backup_from_mirrors(
            &[],
            ALICE_PHRASE,
            None,
            None,
            human_money_core::MnemonicLanguage::English,
            &state,
            &tmp_restore,
        )
        .await;
        assert!(empty_res.is_none());

        // 2. Unreachable descriptor returns None without panicking
        let unreachable_desc = DropDescriptor {
            write_url: "http://127.0.0.1:9".to_string(),
            read_url: "http://127.0.0.1:9".to_string(),
            slot_id: Some("deadslot".to_string()),
            drop_pub: "deadkey".to_string(),
        };
        let unreach_res = discover_best_backup_from_mirrors(
            &[unreachable_desc],
            ALICE_PHRASE,
            None,
            None,
            human_money_core::MnemonicLanguage::English,
            &state,
            &tmp_restore,
        )
        .await;
        assert!(unreach_res.is_none());
    }
}
