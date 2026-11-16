use std::collections::HashMap;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use human_money_core::crypto::get_pubkey_from_user_id;
use human_money_core::models::secure_container::{
    ContainerConfig, PayloadType, PrivacyMode, SecureContainer,
};
use human_money_core::models::profile::UserIdentity;
use humoco_market::{
    is_own_dummy_post_box, merge_post_slot, seal_post_box, try_open_post_box_any_sender,
    validate_market_inquiry, validate_market_inquiry_reply, validate_post_delivery_receipt,
    validate_wot_handshake_assertion, InquiryTarget, MarketInquiry, MarketInquiryDecision,
    MarketInquiryReply, PostDeliveryReceipt, PostEnvelope, WotHandshakeAssertion,
    DROP_POST_MAX_BYTES, PAYLOAD_MARKET_INQUIRY_REPLY_V1, PAYLOAD_MARKET_INQUIRY_V1,
    PAYLOAD_POST_RECEIPT_V1, POST_SLOT_MAGIC, POST_TTL_SECS,
};
use log::warn;
use tauri::State;

use crate::sync_client::{self as sync_client, PutSlotOutcome};
use crate::models::{
    FrontendContact, FrontendSyncDescriptor, FrontendError, FrontendMarketInquiryItem,
    FrontendMarketInquiryReplyDto, FrontendPendingSignRequest, FrontendPendingWotHandshake,
    FrontendPostDepositResult, FrontendPostInboxSummary, FrontendPostSyncResult,
    FrontendUserProfile, TransactionRecord,
};
use crate::settings::AppSettings;
use crate::state::{AppState, PostOutboxEntry, StoredPostState, MARKET_POST_STATE_KEY};

// =========================================================================
//  HELPERS & UTILITIES
// =========================================================================

pub(crate) fn are_dids_equal(a: &str, b: &str) -> bool {
    let a_trim = a.trim();
    let b_trim = b.trim();
    if a_trim == b_trim {
        return true;
    }
    match (get_pubkey_from_user_id(a_trim), get_pubkey_from_user_id(b_trim)) {
        (Ok(pk_a), Ok(pk_b)) => pk_a.as_bytes() == pk_b.as_bytes(),
        _ => false,
    }
}

pub(crate) fn load_post_state(
    state: &AppState,
    password: Option<&str>,
) -> Result<StoredPostState, FrontendError> {
    let mut service = state.service.lock().unwrap();
    match service.load_encrypted_data(MARKET_POST_STATE_KEY, password) {
        Ok(bytes) => serde_json::from_slice::<StoredPostState>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse post state: {}", e))),
        Err(_) => Ok(StoredPostState::default()),
    }
}

pub(crate) fn save_post_state(
    state: &AppState,
    post_state: &StoredPostState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(post_state)
        .map_err(|e| FrontendError::from(format!("Failed to serialize post state: {}", e)))?;
    let mut service = state.service.lock().unwrap();
    service
        .save_encrypted_data(MARKET_POST_STATE_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

pub(crate) fn collect_own_reply_syncs(
    settings: &AppSettings,
    my_prefix: Option<&str>,
) -> Vec<FrontendSyncDescriptor> {
    let mut reply_syncs = Vec::new();
    for d in &settings.market_selected_syncs {
        if !d.read_url.is_empty() {
            reply_syncs.push(d.clone());
        }
    }
    for pd in &settings.public_syncs {
        if pd.enabled && !pd.read_url.is_empty() {
            let slot_id = humoco_market::format_slot_id(&pd.access_pubkey, my_prefix)
                .unwrap_or_else(|_| pd.access_pubkey.clone());
            reply_syncs.push(FrontendSyncDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(slot_id),
                drop_pub: pd.drop_pub.clone(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            });
        }
    }
    reply_syncs
}

pub(crate) fn collect_post_upload_targets(
    settings: &AppSettings,
    _master_seed: &[u8; 32],
) -> Vec<humoco_market::DropDescriptor> {
    let mut targets = Vec::new();
    for d in &settings.market_selected_syncs {
        if !d.write_url.is_empty() {
            targets.push(d.clone().into());
        }
    }
    for pd in &settings.public_syncs {
        if pd.enabled
            && !pd.write_url.is_empty()
            && (pd.authorized_channels.is_empty()
                || pd.authorized_channels.iter().any(|c| c == "post"))
        {
            targets.push(humoco_market::DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(pd.access_pubkey.clone()),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }
    targets
}

pub(crate) fn load_all_standard_tomls(
    app: Option<&tauri::AppHandle>,
    extra: Option<&HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let minuto_toml = include_str!("../../../voucher_standards/minuto_v1/standard.toml").to_string();
    if let Ok((std, _)) = human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(&minuto_toml) {
        map.insert(std.immutable.identity.uuid, minuto_toml);
    }
    let freetaler_toml = include_str!("../../../voucher_standards/freetaler_v1/standard.toml").to_string();
    if let Ok((std, _)) = human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(&freetaler_toml) {
        map.insert(std.immutable.identity.uuid, freetaler_toml);
    }
    if let Some(extra_map) = extra {
        map.extend(extra_map.clone());
    }
    if let Some(a) = app {
        let custom = crate::commands::backup::collect_custom_standards(a);
        for (_id, toml) in custom {
            if let Ok((std, _)) = human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(&toml) {
                map.insert(std.immutable.identity.uuid, toml);
            }
        }
    }
    map
}

pub(crate) fn open_wallet_archive(
    app: &tauri::AppHandle,
    service: &human_money_core::app_service::AppService,
) -> Option<human_money_core::FileVoucherArchive> {
    use tauri::Manager;
    let app_data_dir = app.path().app_data_dir().ok()?;
    let (user_id, key) = service
        .with_wallet_and_identity(|w, id| {
            let uid = w.get_user_id().to_string();
            let derived = blake3::derive_key("HUMOCO_APP_ARCHIVE_KEY_V1", &id.signing_key.to_bytes());
            (uid, derived)
        })
        .ok()?;
    if key.iter().all(|&b| b == 0) {
        return None;
    }
    let folder_hash = &blake3::hash(user_id.as_bytes()).to_hex()[..32];
    let archive_dir = app_data_dir.join("voucher_archives").join(folder_hash);
    Some(human_money_core::FileVoucherArchive::with_key(archive_dir, key))
}

pub(crate) fn get_contact_name(state: &AppState, did: &str, password: Option<&str>) -> Option<String> {
    let mut service = state.service.lock().unwrap();
    if let Ok(contacts) = state.load_contacts(&mut service, password) {
        let trimmed = did.trim();
        let format_name = |c: &crate::models::FrontendContact| -> Option<String> {
            let first = c.profile.first_name.as_deref().unwrap_or("").trim();
            let last = c.profile.last_name.as_deref().unwrap_or("").trim();
            let full = format!("{} {}", first, last).trim().to_string();
            if !full.is_empty() {
                Some(full)
            } else {
                None
            }
        };
        if let Some(exact) = contacts.iter().find(|c| c.did.trim() == trimmed) {
            if let Some(name) = format_name(exact) {
                return Some(name);
            }
        }
        if let Some(c) = contacts.iter().find(|c| are_dids_equal(&c.did, did)) {
            if let Some(name) = format_name(c) {
                return Some(name);
            }
        }
    }
    None
}

pub(crate) fn check_inquiry_target(
    state: &AppState,
    target: &humoco_market::InquiryTarget,
    now_secs: u64,
    password: Option<&str>,
) -> bool {
    let mut service = state.service.lock().unwrap();
    if let Ok(bytes) = service.load_encrypted_data(crate::state::MARKET_OWN_PROFILE_KEY, password) {
        if let Ok(stored) = serde_json::from_slice::<crate::models::market::StoredOwnMarketProfile>(&bytes) {
            match target {
                humoco_market::InquiryTarget::Ad { ad_id, .. } => {
                    return stored.ads.iter().any(|a| &a.id == ad_id && a.expires_at > now_secs && a.allow_inquiries);
                }
                humoco_market::InquiryTarget::Person { .. } => {
                    return stored.published && stored.allow_inquiries;
                }
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn queue_receipt(
    post_state: &mut StoredPostState,
    my_identity: &UserIdentity,
    my_ed_sk: &SigningKey,
    sender_did: &str,
    msg_id: &str,
    tag16: &[u8; 16],
    bundle_id: Option<&str>,
    own_reply_syncs: &[FrontendSyncDescriptor],
    now_secs: u64,
) {
    let receipt = PostDeliveryReceipt {
        acknowledged_msg_id: msg_id.to_string(),
        acknowledged_tag16_hex: hex::encode(tag16),
        bundle_id: bundle_id.map(ToString::to_string),
        received_at: now_secs,
    };
    let json_bytes = match serde_json::to_vec(&receipt) {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to serialize delivery receipt: {}", e);
            return;
        }
    };
    let container = match SecureContainer::seal(
        my_identity,
        &ContainerConfig::TargetDid(sender_did.to_string(), PrivacyMode::HashedRouting),
        &json_bytes,
        PayloadType::Generic(PAYLOAD_POST_RECEIPT_V1.to_string()),
    ) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to seal delivery receipt container: {}", e);
            return;
        }
    };
    let container_bytes = match serde_json::to_vec(&container) {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to serialize delivery receipt container: {}", e);
            return;
        }
    };
    let envelope = PostEnvelope::new(
        uuid::Uuid::new_v4().to_string(),
        now_secs,
        own_reply_syncs.iter().cloned().map(Into::into).collect(),
        container_bytes,
    );
    let recipient_pk = match get_pubkey_from_user_id(sender_did) {
        Ok(pk) => pk,
        Err(e) => {
            warn!("Invalid recipient DID for delivery receipt: {}", e);
            return;
        }
    };
    match seal_post_box(my_ed_sk, &recipient_pk, &envelope) {
        Ok(entry) => {
            post_state.processed_msg_ids.insert(envelope.msg_id.clone());
            post_state.outbox_entries.push(PostOutboxEntry {
                msg_id: envelope.msg_id.clone(),
                recipient_did: sender_did.to_string(),
                tag16_hex: hex::encode(entry.tag16),
                bundle_id: None,
                entry,
                created_at: now_secs,
            });
        }
        Err(e) => {
            warn!("Failed to seal post box for delivery receipt: {}", e);
        }
    }
}

pub(crate) async fn flush_post_slots_impl(
    state: &AppState,
    post_state: &mut StoredPostState,
    password: Option<&str>,
) -> (usize, Vec<String>) {
    let (my_ed_sk, master_seed, my_prefix, upload_targets, client, now_secs) = {
        let mut service = state.service.lock().unwrap();
        let (my_ed_sk, my_prefix) = match service.with_wallet_and_identity(|_w, id| {
            let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                .map(|s| s.to_string());
            (id.signing_key.clone(), pfx)
        }) {
            Ok(k) => k,
            Err(e) => return (0, vec![format!("Failed to access wallet signing key: {}", e)]),
        };
        let master_seed = my_ed_sk.to_bytes();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password))
            .unwrap_or_default();
        let mut targets = collect_post_upload_targets(&settings, &master_seed);

        // Include recipient Sync-Ablagen from address book
        if let Ok(contacts) = state.load_contacts(&mut service, password) {
            let recipient_dids: std::collections::HashSet<String> = post_state
                .outbox_entries
                .iter()
                .map(|e| crate::state::canonicalize_did(&e.recipient_did).unwrap_or_else(|_| e.recipient_did.clone()))
                .collect();

            for c in contacts {
                let can_c_did = crate::state::canonicalize_did(&c.did).unwrap_or_else(|_| c.did.clone());
                if recipient_dids.contains(&can_c_did) {
                    for d in c.syncs {
                        if !d.write_url.is_empty() {
                            targets.push(d.into());
                        }
                    }
                }
            }
        }

        // Deduplicate targets by normalized write_url
        let mut deduped_targets: Vec<humoco_market::DropDescriptor> = Vec::new();
        let mut seen_urls = std::collections::HashSet::new();
        for t in targets {
            let norm = t.write_url.trim().trim_end_matches('/').to_lowercase();
            if !norm.is_empty() && seen_urls.insert(norm) {
                deduped_targets.push(t);
            }
        }

        let client = sync_client::create_sync_http_client();
        let now_secs = sync_client::current_unix_timestamp();
        (my_ed_sk, master_seed, my_prefix, deduped_targets, client, now_secs)
    };

    if upload_targets.is_empty() {
        return (0, Vec::new());
    }

    let local_entries: Vec<humoco_market::PostBoxEntry> = post_state
        .outbox_entries
        .iter()
        .map(|e| e.entry.clone())
        .collect();

    let initial_blob = match humoco_market::encode_post_slot(
        &master_seed,
        &local_entries,
        DROP_POST_MAX_BYTES,
    ) {
        Ok(b) => b,
        Err(e) => return (0, vec![format!("Failed to encode post slot: {}", e)]),
    };

    let mut deposited_to = 0;
    let mut errors = Vec::new();
    let mut health_updates: Vec<(String, bool)> = Vec::new();

    for sync_desc in &upload_targets {
        let (id, signed_res) = if sync_desc.slot_id.is_some() {
            match humoco_market::derive_public_drop_keypair(&master_seed, &sync_desc.write_url) {
                Ok((r_sk, r_vk, _)) => {
                    let nonce = sync_client::generate_sync_nonce();
                    let signed = humoco_market::sign_member_request_direct(
                        &r_sk,
                        "PUT",
                        "post",
                        my_prefix.as_deref(),
                        now_secs,
                        &nonce,
                        &initial_blob,
                    );
                    (hex::encode(r_vk.as_bytes()), Ok(signed))
                }
                Err(e) => (String::new(), Err(format!("Public sync key derivation error: {}", e))),
            }
        } else {
            match humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &sync_desc.drop_pub) {
                Ok(p_hex) => {
                    let nonce = sync_client::generate_sync_nonce();
                    let signed = humoco_market::sign_member_request_stealth(
                        &my_ed_sk,
                        &sync_desc.drop_pub,
                        "PUT",
                        "post",
                        my_prefix.as_deref(),
                        now_secs,
                        &nonce,
                        &initial_blob,
                    );
                    (p_hex, signed.map_err(|e| format!("Stealth signing error: {}", e)))
                }
                Err(e) => (String::new(), Err(format!("Stealth derivation error: {}", e))),
            }
        };

        let signed = match signed_res {
            Ok(s) => s,
            Err(e) => {
                health_updates.push((sync_desc.write_url.clone(), false));
                errors.push(e);
                continue;
            }
        };

        let if_match = post_state
            .slot_etags_by_write_url
            .get(&sync_desc.write_url)
            .cloned();

        match sync_client::put_slot(
            &client,
            &sync_desc.write_url,
            &signed,
            "post",
            &initial_blob,
            if_match.as_deref(),
        )
        .await
        {
            Ok(PutSlotOutcome::Saved { etag, .. }) => {
                deposited_to += 1;
                health_updates.push((sync_desc.write_url.clone(), true));
                if let Some(et) = etag {
                    post_state
                        .slot_etags_by_write_url
                        .insert(sync_desc.write_url.clone(), et);
                }
            }
            Ok(PutSlotOutcome::PreconditionFailed { current_etag }) => {
                // Multi-device conflict! Fetch current remote slot, merge local additions, and retry.
                let remote_bytes = match sync_client::get_slot(&client, &sync_desc.read_url, &id, my_prefix.as_deref(), "post", None).await {
                    Ok(sync_client::SlotFetchResult::Modified { bytes, .. }) => Some(bytes),
                    Ok(sync_client::SlotFetchResult::NotFound | sync_client::SlotFetchResult::NotModified) => None,
                    Err(e) => {
                        health_updates.push((sync_desc.write_url.clone(), false));
                        errors.push(format!(
                            "Failed to fetch remote slot for 412 merge from {}: {}",
                            sync_desc.read_url, e
                        ));
                        continue;
                    }
                };

                let merged_blob = match merge_post_slot(
                    &master_seed,
                    remote_bytes.as_deref(),
                    &local_entries,
                    &[],
                    DROP_POST_MAX_BYTES,
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        health_updates.push((sync_desc.write_url.clone(), false));
                        errors.push(format!("Failed to merge post slot on 412: {}", e));
                        continue;
                    }
                };

                let retry_signed_res = if sync_desc.slot_id.is_some() {
                    match humoco_market::derive_public_drop_keypair(&master_seed, &sync_desc.write_url) {
                        Ok((r_sk, _, _)) => {
                            let nonce = sync_client::generate_sync_nonce();
                            Ok(humoco_market::sign_member_request_direct(
                                &r_sk,
                                "PUT",
                                "post",
                                my_prefix.as_deref(),
                                now_secs,
                                &nonce,
                                &merged_blob,
                            ))
                        }
                        Err(e) => Err(format!("Public sync key derivation error: {}", e)),
                    }
                } else {
                    let nonce = sync_client::generate_sync_nonce();
                    humoco_market::sign_member_request_stealth(
                        &my_ed_sk,
                        &sync_desc.drop_pub,
                        "PUT",
                        "post",
                        my_prefix.as_deref(),
                        now_secs,
                        &nonce,
                        &merged_blob,
                    )
                    .map_err(|e| format!("Stealth signing error: {}", e))
                };

                match retry_signed_res {
                    Ok(retry_signed) => {
                        match sync_client::put_slot(
                            &client,
                            &sync_desc.write_url,
                            &retry_signed,
                            "post",
                            &merged_blob,
                            current_etag.as_deref(),
                        )
                        .await
                        {
                            Ok(PutSlotOutcome::Saved { etag, .. }) => {
                                deposited_to += 1;
                                health_updates.push((sync_desc.write_url.clone(), true));
                                if let Some(et) = etag {
                                    post_state
                                        .slot_etags_by_write_url
                                        .insert(sync_desc.write_url.clone(), et);
                                }
                            }
                            Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                health_updates.push((sync_desc.write_url.clone(), false));
                                errors.push(format!(
                                    "Upload to {} failed 412 Precondition Failed twice",
                                    sync_desc.write_url
                                ));
                            }
                            Err(e) => {
                                health_updates.push((sync_desc.write_url.clone(), false));
                                errors.push(format!(
                                    "Upload retry to {} failed: {}",
                                    sync_desc.write_url, e
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        health_updates.push((sync_desc.write_url.clone(), false));
                        errors.push(e);
                    }
                }
            }
            Err(e) => {
                health_updates.push((sync_desc.write_url.clone(), false));
                errors.push(format!("Upload to {} failed: {}", sync_desc.write_url, e));
            }
        }
    }

    if !health_updates.is_empty() {
        let mut service = state.service.lock().unwrap();
        state.record_contact_sync_health_batch(&mut service, &health_updates, now_secs, password);
    }

    (deposited_to, errors)
}

// =========================================================================
//  COMMAND IMPLEMENTATIONS
// =========================================================================

pub async fn deposit_transfer_to_post_impl(
    bundle_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let record = {
        let mut service = state.service.lock().unwrap();
        let history = state
            .load_history_from_disk(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        history
            .into_iter()
            .find(|r| r.bundle_id == bundle_id || r.id == bundle_id)
            .ok_or_else(|| {
                FrontendError::from(format!(
                    "Transaction record with bundle_id {} not found",
                    bundle_id
                ))
            })?
    };

    if record.bundle_data.is_empty() {
        return Err(FrontendError::from(
            "Transaction record bundle_data is empty (expired or not retained)".to_string(),
        ));
    }

    let mut post_state = load_post_state(state, password.as_deref())?;

    let msg_id = if let Some(existing) = post_state
        .outbox_entries
        .iter()
        .find(|e| e.bundle_id.as_deref() == Some(&bundle_id) || e.bundle_id.as_deref() == Some(&record.bundle_id))
    {
        existing.msg_id.clone()
    } else {
        let (my_ed_sk, recipient_ed_pk, own_reply_syncs, now_secs) = {
            let mut service = state.service.lock().unwrap();
            let (my_ed_sk, my_prefix) = service
                .with_wallet_and_identity(|_w, id| {
                    let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                        .map(|s| s.to_string());
                    (id.signing_key.clone(), pfx)
                })
                .map_err(FrontendError::from)?;
            let recipient_ed_pk = get_pubkey_from_user_id(&record.recipient_id)
                .map_err(FrontendError::from)?;
            let settings = state
                .get_cached_settings()
                .or_else(|_| state.load_settings(&mut service, password.as_deref()))
                .unwrap_or_default();
            let now_secs = sync_client::current_unix_timestamp();
            let reply_syncs = collect_own_reply_syncs(&settings, my_prefix.as_deref());
            (my_ed_sk, recipient_ed_pk, reply_syncs, now_secs)
        };

        let fresh_msg_id = uuid::Uuid::new_v4().to_string();
        let envelope = PostEnvelope::new(
            fresh_msg_id.clone(),
            now_secs,
            own_reply_syncs.into_iter().map(Into::into).collect(),
            record.bundle_data.clone(), // EXACT bytes! Never call create_transfer_bundle!
        );

        let box_entry = seal_post_box(&my_ed_sk, &recipient_ed_pk, &envelope)
            .map_err(FrontendError::from)?;

        let outbox_entry = PostOutboxEntry {
            msg_id: fresh_msg_id.clone(),
            recipient_did: record.recipient_id.clone(),
            tag16_hex: hex::encode(box_entry.tag16),
            bundle_id: Some(record.bundle_id.clone()),
            entry: box_entry,
            created_at: now_secs,
        };
        post_state.processed_msg_ids.insert(fresh_msg_id.clone());
        post_state.outbox_entries.push(outbox_entry);
        fresh_msg_id
    };

    // Persist the sealed outbox entry BEFORE network flush so slow or unreachable
    // sync servers never lose an outgoing transfer box if the app closes.
    save_post_state(state, &post_state, password.as_deref())?;

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
    if !errors.is_empty() {
        warn!("Post deposit degraded: deposited={}, failed={}", deposited_to, errors.len());
    }
    save_post_state(state, &post_state, password.as_deref())?;

    Ok(FrontendPostDepositResult {
        deposited_to,
        msg_id,
        errors,
    })
}

#[tauri::command]
pub async fn deposit_transfer_to_post(
    bundle_id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    deposit_transfer_to_post_impl(bundle_id, password, &state).await
}

pub async fn send_signing_request_via_post_impl(
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let recipient_ed_pk = get_pubkey_from_user_id(&recipient_did).map_err(FrontendError::from)?;

    let (my_ed_sk, own_reply_syncs, now_secs) = {
        let mut service = state.service.lock().unwrap();
        let (my_ed_sk, my_prefix) = service
            .with_wallet_and_identity(|_w, id| {
                let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string());
                (id.signing_key.clone(), pfx)
            })
            .map_err(FrontendError::from)?;
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let reply_syncs = collect_own_reply_syncs(&settings, my_prefix.as_deref());
        let now_secs = sync_client::current_unix_timestamp();
        (my_ed_sk, reply_syncs, now_secs)
    };

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_syncs.into_iter().map(Into::into).collect(),
        container_bytes,
    );

    let box_entry = seal_post_box(&my_ed_sk, &recipient_ed_pk, &envelope)
        .map_err(FrontendError::from)?;

    let mut post_state = load_post_state(state, password.as_deref())?;
    post_state.processed_msg_ids.insert(msg_id.clone());
    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id: msg_id.clone(),
        recipient_did,
        tag16_hex: hex::encode(box_entry.tag16),
        bundle_id: None,
        entry: box_entry,
        created_at: now_secs,
    });

    // Persist the sealed outbox entry BEFORE network flush so slow or unreachable
    // sync servers never lose an outgoing transfer box if the app closes.
    save_post_state(state, &post_state, password.as_deref())?;

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
    if !errors.is_empty() {
        warn!("Post deposit degraded: deposited={}, failed={}", deposited_to, errors.len());
    }
    save_post_state(state, &post_state, password.as_deref())?;

    Ok(FrontendPostDepositResult {
        deposited_to,
        msg_id,
        errors,
    })
}

#[tauri::command]
pub async fn send_signing_request_via_post(
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    send_signing_request_via_post_impl(recipient_did, container_bytes, password, &state).await
}

pub async fn reply_detached_signature_via_post_impl(
    request_msg_id: Option<String>,
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let recipient_ed_pk = get_pubkey_from_user_id(&recipient_did).map_err(FrontendError::from)?;

    let (my_ed_sk, own_reply_syncs, now_secs) = {
        let mut service = state.service.lock().unwrap();
        let (my_ed_sk, my_prefix) = service
            .with_wallet_and_identity(|_w, id| {
                let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string());
                (id.signing_key.clone(), pfx)
            })
            .map_err(FrontendError::from)?;
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let reply_syncs = collect_own_reply_syncs(&settings, my_prefix.as_deref());
        let now_secs = sync_client::current_unix_timestamp();
        (my_ed_sk, reply_syncs, now_secs)
    };

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_syncs.into_iter().map(Into::into).collect(),
        container_bytes,
    );

    let box_entry = seal_post_box(&my_ed_sk, &recipient_ed_pk, &envelope)
        .map_err(FrontendError::from)?;

    let mut post_state = load_post_state(state, password.as_deref())?;
    post_state.processed_msg_ids.insert(msg_id.clone());
    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id: msg_id.clone(),
        recipient_did: recipient_did.clone(),
        tag16_hex: hex::encode(box_entry.tag16),
        bundle_id: None,
        entry: box_entry,
        created_at: now_secs,
    });

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;

    if deposited_to > 0 {
        if let Some(ref req_id) = request_msg_id {
            if let Some(idx) = post_state
                .pending_sign_requests
                .iter()
                .position(|r| &r.msg_id == req_id)
            {
                let req = post_state.pending_sign_requests.remove(idx);
                if !req.reply_syncs.is_empty() {
                    let mut service = state.service.lock().unwrap();
                    state.update_contact_syncs_from_card(
                        &mut service,
                        &recipient_did,
                        &req.reply_syncs,
                        password.as_deref(),
                    );
                }
            }
        }
    }

    save_post_state(state, &post_state, password.as_deref())?;

    Ok(FrontendPostDepositResult {
        deposited_to,
        msg_id,
        errors,
    })
}

#[tauri::command]
pub async fn reply_detached_signature_via_post(
    request_msg_id: Option<String>,
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    reply_detached_signature_via_post_impl(request_msg_id, recipient_did, container_bytes, password, &state).await
}

pub async fn send_wot_handshake_via_post_impl(
    recipient_did: String,
    proposed_level: u8,
    reply_requested: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    if proposed_level > 1 {
        return Err(FrontendError::from(format!(
            "proposed_level must be 0 or 1, got {}",
            proposed_level
        )));
    }

    let recipient_ed_pk = get_pubkey_from_user_id(&recipient_did).map_err(FrontendError::from)?;

    let (my_identity, my_ed_sk, own_reply_syncs, assertion, now_secs) = {
        let mut service = state.service.lock().unwrap();
        let my_identity = service
            .with_wallet_and_identity(|_w, id| id.clone())
            .map_err(FrontendError::from)?;
        let my_ed_sk = my_identity.signing_key.clone();
        let my_did = my_identity.user_id.clone();
        let my_name = service
            .with_wallet(|w| w.profile.first_name.clone())
            .ok()
            .flatten();

        let mut reg = state
            .get_or_load_wot_register(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;

        if reg.own_list_dirty {
            state
                .ensure_wot_list_signed(&mut service, &mut reg)
                .map_err(FrontendError::from)?;
            state
                .save_wot_register(&mut service, &reg, password.as_deref())
                .map_err(FrontendError::from)?;
        }

        let can_recipient = crate::state::canonicalize_did(&recipient_did).unwrap_or_else(|_| recipient_did.clone());

        let (pkg, _receipt) = reg
            .prepare_sync_package(Some(&can_recipient), None, true)
            .map_err(FrontendError::from)?;
        let wot_sync_json = serde_json::to_string(&pkg)
            .map_err(|e| FrontendError::from(format!("Failed to serialize sync package: {}", e)))?;

        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let reply_syncs = collect_own_reply_syncs(
            &settings,
            human_money_core::crypto::get_prefix_from_user_id(&my_did),
        );
        let now_secs = sync_client::current_unix_timestamp();

        let assertion = WotHandshakeAssertion {
            sender_did: my_did,
            sender_name: my_name,
            proposed_level,
            wot_sync_json,
            reply_requested,
            created_at: now_secs,
        };
        validate_wot_handshake_assertion(&assertion).map_err(FrontendError::from)?;

        (my_identity, my_ed_sk, reply_syncs, assertion, now_secs)
    };

    let assertion_bytes = serde_json::to_vec(&assertion)
        .map_err(|e| FrontendError::from(format!("Failed to serialize assertion: {}", e)))?;

    let container = SecureContainer::seal(
        &my_identity,
        &ContainerConfig::TargetDid(recipient_did.clone(), PrivacyMode::HashedRouting),
        &assertion_bytes,
        PayloadType::TrustAssertion,
    )
    .map_err(FrontendError::from)?;

    let container_bytes = serde_json::to_vec(&container)
        .map_err(|e| FrontendError::from(format!("Failed to serialize container: {}", e)))?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_syncs.into_iter().map(Into::into).collect(),
        container_bytes,
    );

    let box_entry = seal_post_box(&my_ed_sk, &recipient_ed_pk, &envelope)
        .map_err(FrontendError::from)?;

    let mut post_state = load_post_state(state, password.as_deref())?;
    post_state.processed_msg_ids.insert(msg_id.clone());
    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id: msg_id.clone(),
        recipient_did,
        tag16_hex: hex::encode(box_entry.tag16),
        bundle_id: None,
        entry: box_entry,
        created_at: now_secs,
    });

    // Persist the sealed outbox entry BEFORE network flush so slow or unreachable
    // sync servers never lose an outgoing transfer box if the app closes.
    save_post_state(state, &post_state, password.as_deref())?;

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
    if !errors.is_empty() {
        warn!("Post deposit degraded: deposited={}, failed={}", deposited_to, errors.len());
    }
    save_post_state(state, &post_state, password.as_deref())?;

    Ok(FrontendPostDepositResult {
        deposited_to,
        msg_id,
        errors,
    })
}

#[tauri::command]
pub async fn send_wot_handshake_via_post(
    recipient_did: String,
    proposed_level: u8,
    reply_requested: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    send_wot_handshake_via_post_impl(recipient_did, proposed_level, reply_requested, password, &state).await
}

pub async fn accept_wot_handshake_impl(
    msg_id: String,
    trust_level: u8,
    send_return_handshake: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    if trust_level > 3 {
        return Err(FrontendError::from(format!(
            "trust_level must be 0..=3, got {}",
            trust_level
        )));
    }

    let mut post_state = load_post_state(state, password.as_deref())?;
    let idx = post_state
        .pending_wot_handshakes
        .iter()
        .position(|h| h.msg_id == msg_id)
        .ok_or_else(|| {
            FrontendError::from(format!("Pending WoT handshake msg_id {} not found", msg_id))
        })?;
    let handshake = post_state.pending_wot_handshakes.remove(idx);
    let sender_did = handshake.sender_did.clone();

    let now_secs = sync_client::current_unix_timestamp();

    // Update / insert contact with trust level
    {
        let mut service = state.service.lock().unwrap();
        let mut contact = match state.load_contacts(&mut service, password.as_deref()) {
            Ok(list) => list
                .into_iter()
                .find(|c| are_dids_equal(&c.did, &sender_did))
                .unwrap_or_else(|| FrontendContact {
                    did: sender_did.clone(),
                    profile: FrontendUserProfile {
                        first_name: handshake.sender_name.clone(),
                        ..Default::default()
                    },
                    tags: vec![],
                    added_at: Utc::now().to_rfc3339(),
                    notes: None,
                    trust_level: Some(trust_level),
                    desired_hops: Some(3),
                    has_peer_list: false,
                    peer_list_updated_at: None,
                    syncs: vec![],
                }),
            Err(_) => FrontendContact {
                did: sender_did.clone(),
                profile: FrontendUserProfile {
                    first_name: handshake.sender_name.clone(),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(trust_level),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            },
        };
        contact.trust_level = Some(trust_level);
        state
            .save_contact(&mut service, contact, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    // Import stored wot_sync_json
    if let Some(json_str) = post_state.pending_wot_packages_by_msg_id.remove(&msg_id) {
        if let Ok(pkg) = serde_json::from_str::<humoco_wot::WotSyncPackage>(&json_str) {
            let mut service = state.service.lock().unwrap();
            if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
                let can_sender_did = crate::state::canonicalize_did(&sender_did).unwrap_or_else(|_| sender_did.clone());
                let _ = reg.import_sync_package(&pkg, Some(&can_sender_did), now_secs);
                let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
                let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
            }
        }
    }

    save_post_state(state, &post_state, password.as_deref())?;

    if send_return_handshake {
        send_wot_handshake_via_post_impl(
            sender_did,
            trust_level.min(1),
            false,
            password,
            state,
        )
        .await
    } else {
        Ok(FrontendPostDepositResult {
            deposited_to: 0,
            msg_id,
            errors: Vec::new(),
        })
    }
}

#[tauri::command]
pub async fn accept_wot_handshake(
    msg_id: String,
    trust_level: u8,
    send_return_handshake: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    accept_wot_handshake_impl(msg_id, trust_level, send_return_handshake, password, &state).await
}

pub async fn send_market_inquiry_via_post_impl(
    recipient_did: String,
    target: humoco_market::InquiryTarget,
    message: String,
    recipient_syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    let now_secs = sync_client::current_unix_timestamp();
    let inquiry_id = uuid::Uuid::new_v4().to_string();

    let inquiry = MarketInquiry {
        inquiry_id: inquiry_id.clone(),
        target: target.clone(),
        message: message.clone(),
        created_at: now_secs,
    };
    validate_market_inquiry(&inquiry).map_err(FrontendError::from)?;

    if !recipient_syncs.is_empty() {
        let mut service = state.service.lock().unwrap();
        state.update_contact_syncs_from_card(
            &mut service,
            &recipient_did,
            &recipient_syncs,
            password.as_deref(),
        );
    }

    let recipient_ed_pk = get_pubkey_from_user_id(&recipient_did).map_err(FrontendError::from)?;

    let (my_identity, my_ed_sk, own_reply_syncs) = {
        let mut service = state.service.lock().unwrap();
        let my_identity = service
            .with_wallet_and_identity(|_w, id| id.clone())
            .map_err(FrontendError::from)?;
        let my_ed_sk = my_identity.signing_key.clone();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let reply_syncs = collect_own_reply_syncs(
            &settings,
            human_money_core::crypto::get_prefix_from_user_id(&my_identity.user_id),
        );
        (my_identity, my_ed_sk, reply_syncs)
    };

    let inquiry_bytes = serde_json::to_vec(&inquiry)
        .map_err(|e| FrontendError::from(format!("Failed to serialize inquiry: {}", e)))?;

    let container = SecureContainer::seal(
        &my_identity,
        &ContainerConfig::TargetDid(recipient_did.clone(), PrivacyMode::HashedRouting),
        &inquiry_bytes,
        PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_V1.to_string()),
    )
    .map_err(FrontendError::from)?;

    let container_bytes = serde_json::to_vec(&container)
        .map_err(|e| FrontendError::from(format!("Failed to serialize container: {}", e)))?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_syncs.into_iter().map(Into::into).collect(),
        container_bytes,
    );

    let box_entry = seal_post_box(&my_ed_sk, &recipient_ed_pk, &envelope)
        .map_err(FrontendError::from)?;

    let mut post_state = load_post_state(state, password.as_deref())?;
    post_state.processed_msg_ids.insert(msg_id.clone());
    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id: msg_id.clone(),
        recipient_did: recipient_did.clone(),
        tag16_hex: hex::encode(box_entry.tag16),
        bundle_id: None,
        entry: box_entry,
        created_at: now_secs,
    });

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
    if deposited_to == 0 {
        return Err(FrontendError::from(format!(
            "An active connected sync service is required to deposit marketplace inquiries. Errors: {:?}",
            errors
        )));
    }

    let item = FrontendMarketInquiryItem {
        inquiry_id,
        target,
        counterparty_did: recipient_did,
        counterparty_name: None,
        message,
        created_at: now_secs,
        reply_syncs: recipient_syncs,
        reply: None,
    };
    post_state.buyer_inquiries.push(item.clone());
    save_post_state(state, &post_state, password.as_deref())?;

    Ok(item)
}

#[tauri::command]
pub async fn send_market_inquiry_via_post(
    recipient_did: String,
    target: humoco_market::InquiryTarget,
    message: String,
    recipient_syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    send_market_inquiry_via_post_impl(recipient_did, target, message, recipient_syncs, password, &state).await
}

#[allow(clippy::too_many_arguments)]
pub async fn reply_market_inquiry_via_post_impl(
    inquiry_id: String,
    decision: String,
    note: Option<String>,
    payment_uri: Option<String>,
    amount_hint: Option<String>,
    standard_uuid: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    let dec = match decision.trim().to_ascii_lowercase().as_str() {
        "confirm" => MarketInquiryDecision::Confirm,
        "decline" => MarketInquiryDecision::Decline,
        other => return Err(FrontendError::from(format!("Invalid decision: {}", other))),
    };

    let now_secs = sync_client::current_unix_timestamp();
    let mut post_state = load_post_state(state, password.as_deref())?;

    let (item_idx, counterparty_did, target) = {
        let (idx, item) = post_state
            .seller_inquiries
            .iter()
            .enumerate()
            .find(|(_, i)| i.inquiry_id == inquiry_id)
            .ok_or_else(|| {
                FrontendError::from(format!("Seller inquiry {} not found", inquiry_id))
            })?;

        if item.reply.is_some() {
            return Err(FrontendError::from(
                "Inquiry has already been replied to (one-step cycle only)".to_string(),
            ));
        }
        (idx, item.counterparty_did.clone(), item.target.clone())
    };

    let reply_payload = MarketInquiryReply {
        inquiry_id: inquiry_id.clone(),
        decision: dec,
        note: note.clone(),
        payment_uri: payment_uri.clone(),
        amount_hint: amount_hint.clone(),
        standard_uuid: standard_uuid.clone(),
        replied_at: now_secs,
    };
    validate_market_inquiry_reply(&reply_payload).map_err(FrontendError::from)?;

    if let Ok(sender_vk) = get_pubkey_from_user_id(&counterparty_did) {
        post_state
            .inquiry_rate_tracker
            .resolve_inquiry(&hex::encode(sender_vk.as_bytes()), &target.target_key());
    }

    let recipient_ed_pk = get_pubkey_from_user_id(&counterparty_did).map_err(FrontendError::from)?;

    let (my_identity, my_ed_sk, own_reply_syncs) = {
        let mut service = state.service.lock().unwrap();
        let my_identity = service
            .with_wallet_and_identity(|_w, id| id.clone())
            .map_err(FrontendError::from)?;
        let my_ed_sk = my_identity.signing_key.clone();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let reply_syncs = collect_own_reply_syncs(
            &settings,
            human_money_core::crypto::get_prefix_from_user_id(&my_identity.user_id),
        );
        (my_identity, my_ed_sk, reply_syncs)
    };

    let reply_bytes = serde_json::to_vec(&reply_payload)
        .map_err(|e| FrontendError::from(format!("Failed to serialize reply: {}", e)))?;

    let container = SecureContainer::seal(
        &my_identity,
        &ContainerConfig::TargetDid(counterparty_did.clone(), PrivacyMode::HashedRouting),
        &reply_bytes,
        PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_REPLY_V1.to_string()),
    )
    .map_err(FrontendError::from)?;

    let container_bytes = serde_json::to_vec(&container)
        .map_err(|e| FrontendError::from(format!("Failed to serialize container: {}", e)))?;

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_syncs.into_iter().map(Into::into).collect(),
        container_bytes,
    );

    let box_entry = seal_post_box(&my_ed_sk, &recipient_ed_pk, &envelope)
        .map_err(FrontendError::from)?;

    post_state.processed_msg_ids.insert(msg_id.clone());
    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id: msg_id.clone(),
        recipient_did: counterparty_did,
        tag16_hex: hex::encode(box_entry.tag16),
        bundle_id: None,
        entry: box_entry,
        created_at: now_secs,
    });

    let _ = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;

    let reply_dto = FrontendMarketInquiryReplyDto {
        inquiry_id,
        decision,
        note,
        payment_uri,
        amount_hint,
        standard_uuid,
        replied_at: now_secs,
    };
    post_state.seller_inquiries[item_idx].reply = Some(reply_dto);
    let updated_item = post_state.seller_inquiries[item_idx].clone();

    save_post_state(state, &post_state, password.as_deref())?;

    Ok(updated_item)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn reply_market_inquiry_via_post(
    inquiry_id: String,
    decision: String,
    note: Option<String>,
    payment_uri: Option<String>,
    amount_hint: Option<String>,
    standard_uuid: Option<String>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    reply_market_inquiry_via_post_impl(inquiry_id, decision, note, payment_uri, amount_hint, standard_uuid, password, &state).await
}

pub fn dismiss_pending_post_item_impl(
    item_kind: String,
    id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut post_state = load_post_state(state, password.as_deref())?;
    match item_kind.as_str() {
        "sign_request" => {
            post_state.pending_sign_requests.retain(|r| r.msg_id != id);
        }
        "wot_handshake" => {
            post_state.pending_wot_handshakes.retain(|h| h.msg_id != id);
            post_state.pending_wot_packages_by_msg_id.remove(&id);
        }
        "seller_inquiry" => {
            post_state.seller_inquiries.retain(|i| i.inquiry_id != id);
        }
        "buyer_inquiry" => {
            post_state.buyer_inquiries.retain(|i| i.inquiry_id != id);
        }
        other => {
            return Err(FrontendError::from(format!(
                "Unknown pending post item kind: {}",
                other
            )));
        }
    }
    save_post_state(state, &post_state, password.as_deref())
}

#[tauri::command]
pub fn dismiss_pending_post_item(
    item_kind: String,
    id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    dismiss_pending_post_item_impl(item_kind, id, password, &state)
}

pub fn get_post_inbox_summary_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostInboxSummary, FrontendError> {
    let post_state = load_post_state(state, password.as_deref())?;
    let unacknowledged_payments = post_state
        .outbox_entries
        .iter()
        .filter_map(|e| e.bundle_id.clone())
        .collect();

    Ok(FrontendPostInboxSummary {
        pending_sign_requests: post_state.pending_sign_requests,
        pending_wot_handshakes: post_state.pending_wot_handshakes,
        seller_inquiries: post_state.seller_inquiries,
        buyer_inquiries: post_state.buyer_inquiries,
        unacknowledged_payments,
    })
}

#[tauri::command]
pub fn get_post_inbox_summary(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostInboxSummary, FrontendError> {
    get_post_inbox_summary_impl(password, &state)
}

pub async fn sync_post_channels_impl(
    app: Option<&tauri::AppHandle>,
    is_wifi: Option<bool>,
    extra_standards: Option<&HashMap<String, String>>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostSyncResult, FrontendError> {
    let settings = {
        let mut service = state.service.lock().unwrap();
        state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default()
    };

    if settings.market_poll_wifi_only && is_wifi == Some(false) {
        return Ok(FrontendPostSyncResult {
            uploaded_to: 0,
            polled_syncs: 0,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: 0,
            new_inquiry_replies: 0,
            pruned_boxes: 0,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }

    let client = sync_client::create_sync_http_client();
    let mut post_state = load_post_state(state, password.as_deref())?;
    let now_secs = sync_client::current_unix_timestamp();

    let mut errors = Vec::new();
    let mut polled_syncs = 0;
    let mut imported_payments = 0;
    let mut imported_receipts = 0;
    let mut imported_signatures = 0;
    let mut new_sign_requests = 0;
    let mut new_wot_handshakes = 0;
    let mut new_inquiries = 0;
    let mut new_inquiry_replies = 0;
    let mut pruned_boxes = 0;

    let (my_identity, my_ed_sk, master_seed, my_prefix, own_reply_syncs) = {
        let service = state.service.lock().unwrap();
        let my_identity = service
            .with_wallet_and_identity(|_w, id| id.clone())
            .map_err(FrontendError::from)?;
        let my_ed_sk = my_identity.signing_key.clone();
        let master_seed = my_ed_sk.to_bytes();
        let my_prefix = human_money_core::crypto::get_prefix_from_user_id(&my_identity.user_id)
            .map(|s| s.to_string());
        let reply_syncs = collect_own_reply_syncs(&settings, my_prefix.as_deref());
        (my_identity, my_ed_sk, master_seed, my_prefix, reply_syncs)
    };

    // 1. Pruning pass (§6.2)
    {
        let before_count = post_state.outbox_entries.len();
        post_state.outbox_entries.retain(|e| {
            if now_secs.saturating_sub(e.created_at) > POST_TTL_SECS {
                return false;
            }
            true
        });
        pruned_boxes += before_count - post_state.outbox_entries.len();
    }

    // 2. Build candidate direct fetches and index URLs
    let (direct_slot_fetches, candidate_read_urls) = {
        let mut service = state.service.lock().unwrap();
        let mut direct: Vec<(String, String, Option<String>)> = Vec::new();
        let mut index_urls = Vec::new();

        // Selected drops
        for d in &settings.market_selected_syncs {
            if !d.read_url.is_empty() {
                if let Some(ref sid) = d.slot_id {
                    direct.push((d.read_url.clone(), sid.clone(), my_prefix.clone()));
                } else {
                    index_urls.push(d.read_url.clone());
                }
            }
        }

        // Public drops
        for pd in &settings.public_syncs {
            if pd.enabled && !pd.read_url.is_empty() {
                direct.push((pd.read_url.clone(), pd.access_pubkey.clone(), my_prefix.clone()));
            }
        }

        // Contacts drops
        let own_can_did = crate::state::canonicalize_did(&my_identity.user_id).ok();
        if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
            for c in contacts {
                let c_prefix = human_money_core::crypto::get_prefix_from_user_id(&c.did)
                    .map(|s| s.to_string());
                if own_can_did.is_some()
                    && crate::state::canonicalize_did(&c.did).ok() == own_can_did
                {
                    // Also poll sibling own-subaccount compartments on configured O(1) public syncs
                    for pd in &settings.public_syncs {
                        if pd.enabled && !pd.read_url.is_empty() {
                            direct.push((pd.read_url.clone(), pd.access_pubkey.clone(), c_prefix.clone()));
                        }
                    }
                }
                for d in c.syncs {
                    if !d.read_url.is_empty() {
                        if let Some(ref sid) = d.slot_id {
                            direct.push((d.read_url, sid.clone(), c_prefix.clone()));
                        } else {
                            index_urls.push(d.read_url);
                        }
                    }
                }
            }
        }

        // CardStore drops
        if let Ok(bytes) = service.load_encrypted_data(crate::state::MARKET_CARD_STORE_KEY, password.as_deref()) {
            if let Ok(store) = serde_json::from_slice::<humoco_market::CardStore>(&bytes) {
                for sc in store.cards.values() {
                    for d in &sc.card.payload.drops {
                        if !d.read_url.is_empty() {
                            if let Some(ref sid) = d.slot_id {
                                direct.push((d.read_url.clone(), sid.clone(), None));
                            } else {
                                index_urls.push(d.read_url.clone());
                            }
                        }
                    }
                }
            }
        }

        // Reply drops of pending items
        for req in &post_state.pending_sign_requests {
            for d in &req.reply_syncs {
                if !d.read_url.is_empty() {
                    if let Some(ref sid) = d.slot_id {
                        direct.push((d.read_url.clone(), sid.clone(), None));
                    } else {
                        index_urls.push(d.read_url.clone());
                    }
                }
            }
        }

        for inq in &post_state.seller_inquiries {
            for d in &inq.reply_syncs {
                if !d.read_url.is_empty() {
                    if let Some(ref sid) = d.slot_id {
                        direct.push((d.read_url.clone(), sid.clone(), None));
                    } else {
                        index_urls.push(d.read_url.clone());
                    }
                }
            }
        }

        for inq in &post_state.buyer_inquiries {
            for d in &inq.reply_syncs {
                if !d.read_url.is_empty() {
                    if let Some(ref sid) = d.slot_id {
                        direct.push((d.read_url.clone(), sid.clone(), None));
                    } else {
                        index_urls.push(d.read_url.clone());
                    }
                }
            }
        }

        direct.sort();
        direct.dedup();
        index_urls.sort();
        index_urls.dedup();

        (direct, index_urls)
    };

    // 3. Build authorized_senders and inquiry_only_senders
    let (authorized_senders, inquiry_only_senders) = {
        let mut service = state.service.lock().unwrap();
        let mut auth = Vec::new();
        let mut inquiry = Vec::new();

        if settings.post_accept_from_contacts {
            if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
                for c in contacts {
                    // Allow all explicitly saved AddressBook contacts (including unrated)
                    // so initial WotHandshakeAssertion packages reach pending_wot_handshakes.
                    if let Ok(vk) = get_pubkey_from_user_id(&c.did) {
                        auth.push((c.did.clone(), vk));
                    }
                }
            }
        }

        for entry in &post_state.outbox_entries {
            if let Ok(vk) = get_pubkey_from_user_id(&entry.recipient_did) {
                auth.push((entry.recipient_did.clone(), vk));
            }
        }

        for item in &post_state.buyer_inquiries {
            if item.reply.is_none() {
                if let Ok(vk) = get_pubkey_from_user_id(&item.counterparty_did) {
                    auth.push((item.counterparty_did.clone(), vk));
                }
            }
        }

        let has_inquiry_listing = service
            .load_encrypted_data(crate::state::MARKET_OWN_PROFILE_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<crate::models::market::StoredOwnMarketProfile>(&bytes).ok())
            .map(|stored| stored.published && (stored.allow_inquiries || stored.ads.iter().any(|a| a.expires_at > now_secs && a.allow_inquiries)))
            .unwrap_or(false);

        if has_inquiry_listing {
            if let Ok(bytes) = service.load_encrypted_data(crate::state::MARKET_CARD_STORE_KEY, password.as_deref()) {
                if let Ok(cs) = serde_json::from_slice::<humoco_market::CardStore>(&bytes) {
                    for sc in cs.cards.values() {
                        if let Ok(vk) = get_pubkey_from_user_id(&sc.card.payload.did) {
                            inquiry.push((sc.card.payload.did.clone(), vk));
                        }
                    }
                }
            }
        }

        auth.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
        auth.dedup_by(|a, b| a.1.as_bytes() == b.1.as_bytes());

        inquiry.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
        inquiry.dedup_by(|a, b| a.1.as_bytes() == b.1.as_bytes());

        (auth, inquiry)
    };

    // 4. Download slots
    let mut fetched_slots: Vec<Vec<u8>> = Vec::new();
    let mut post_sync_health_updates: Vec<(String, bool)> = Vec::new();

    for (read_url, slot_id, prefix_opt) in direct_slot_fetches {
        polled_syncs += 1;
        let (base_slot_pk, slot_prefix) =
            humoco_market::parse_slot_id(&slot_id).unwrap_or_else(|_| (slot_id.clone(), None));
        let post_prefix = slot_prefix.as_deref().or(prefix_opt.as_deref());
        match sync_client::get_slot(&client, &read_url, &base_slot_pk, post_prefix, "post", None).await {
            Ok(sync_client::SlotFetchResult::Modified { bytes, .. }) => {
                post_sync_health_updates.push((read_url.clone(), true));
                fetched_slots.push(bytes);
            }
            Ok(sync_client::SlotFetchResult::NotFound | sync_client::SlotFetchResult::NotModified) => {
                post_sync_health_updates.push((read_url.clone(), true));
            }
            Err(e) => {
                post_sync_health_updates.push((read_url.clone(), false));
                errors.push(format!("Failed to fetch direct post slot from {}: {}", read_url, e));
            }
        }
    }

    for read_url in candidate_read_urls {
        polled_syncs += 1;
        let index_res = sync_client::list_index(&client, &read_url, None, None).await;
        match index_res {
            Ok(sync_client::IndexFetchResult::Modified { entries, .. }) => {
                post_sync_health_updates.push((read_url.clone(), true));
                for entry in entries {
                    if entry.channel != "post" {
                        continue;
                    }
                    match sync_client::get_slot(&client, &read_url, &entry.id, entry.prefix.as_deref(), "post", None).await {
                        Ok(sync_client::SlotFetchResult::Modified { bytes, .. }) => fetched_slots.push(bytes),
                        Ok(sync_client::SlotFetchResult::NotFound | sync_client::SlotFetchResult::NotModified) => {}
                        Err(e) => errors.push(format!("Failed to fetch post slot {} from {}: {}", entry.id, read_url, e)),
                    }
                }
            }
            Ok(sync_client::IndexFetchResult::NotModified) => {
                post_sync_health_updates.push((read_url.clone(), true));
            }
            Err(e) => {
                post_sync_health_updates.push((read_url.clone(), false));
                errors.push(format!("Failed to fetch post slot index from {}: {}", read_url, e));
            }
        }
    }

    if !post_sync_health_updates.is_empty() {
        let mut service = state.service.lock().unwrap();
        state.record_contact_sync_health_batch(&mut service, &post_sync_health_updates, now_secs, password.as_deref());
    }

    // 5. Decrypt and process entries
    for slot_bytes in fetched_slots {
        if slot_bytes.len() < 10 || &slot_bytes[0..8] != POST_SLOT_MAGIC {
            continue;
        }

        let entries = match humoco_market::decode_post_slot(&slot_bytes) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Failed to decode post slot: {}", e));
                continue;
            }
        };

        for box_entry in entries {
            if is_own_dummy_post_box(&master_seed, &box_entry) {
                continue;
            }
            let tag16_hex = hex::encode(box_entry.tag16);
            if post_state.outbox_entries.iter().any(|e| e.tag16_hex == tag16_hex) {
                continue;
            }

            let open_attempt = try_open_post_box_any_sender(&my_ed_sk, &authorized_senders, &box_entry);
            let (sender_did, envelope, is_inquiry_only) = match open_attempt {
                Ok(Some((did, env))) => (did, env, false),
                Ok(None) => {
                    if !inquiry_only_senders.is_empty() {
                        match try_open_post_box_any_sender(&my_ed_sk, &inquiry_only_senders, &box_entry) {
                            Ok(Some((did, env))) => (did, env, true),
                            _ => continue,
                        }
                    } else {
                        continue;
                    }
                }
                Err(e) => {
                    warn!("Error opening post box: {}", e);
                    continue;
                }
            };

            if post_state.processed_msg_ids.contains(&envelope.msg_id) {
                continue;
            }

            let container: SecureContainer = match serde_json::from_slice(&envelope.container_bytes) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(format!("Failed to parse SecureContainer: {}", e));
                    continue;
                }
            };

            if let Err(e) = container.verify_integrity() {
                errors.push(format!("Integrity check failed: {}", e));
                continue;
            }

            if !container.signature.is_empty() {
                if let Err(e) = container.verify_container_signature(&sender_did) {
                    errors.push(format!("Container signature check failed: {}", e));
                    continue;
                }
            }

            let decrypted_bytes = match container.open(&my_identity, None) {
                Ok(b) => b,
                Err(e) => {
                    let allow_canonical_fallback = my_identity.user_id.contains('@')
                        && !matches!(container.c, PayloadType::TransactionBundle)
                        && container.c != PayloadType::Generic(PAYLOAD_POST_RECEIPT_V1.to_string());
                    let alt_attempt = if allow_canonical_fallback {
                        if let Ok(can) = crate::state::canonicalize_did(&my_identity.user_id) {
                            let mut alt = my_identity.clone();
                            alt.user_id = can;
                            container.open(&alt, None).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    match alt_attempt {
                        Some(b) => b,
                        None => {
                            if matches!(e, human_money_core::VoucherCoreError::NotAnIntendedRecipient) {
                                // Outer PostBox shared our X25519 key (sibling subaccount/root profile),
                                // but inner SecureContainer is addressed to a different user_id/prefix.
                                continue;
                            }
                            errors.push(format!("Failed to open container payload: {}", e));
                            continue;
                        }
                    }
                }
            };

            if is_inquiry_only && container.c != PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_V1.to_string()) {
                continue;
            }

            if !envelope.reply_drops.is_empty() {
                let reply_dtos: Vec<FrontendSyncDescriptor> =
                    envelope.reply_drops.iter().cloned().map(Into::into).collect();
                let mut service = state.service.lock().unwrap();
                state.update_contact_syncs_from_card(
                    &mut service,
                    &sender_did,
                    &reply_dtos,
                    password.as_deref(),
                );
            }

            match container.c {
                PayloadType::TransactionBundle => {
                    let all_standards = load_all_standard_tomls(app, extra_standards);
                    let archive = app.and_then(|a| {
                        let service = state.service.lock().unwrap();
                        open_wallet_archive(a, &service)
                    });

                    let receive_outcome = {
                        let mut service = state.service.lock().unwrap();
                        service.receive_bundle(
                            &envelope.container_bytes,
                            &all_standards,
                            archive.as_ref(),
                            password.as_deref(),
                            false,
                        )
                    };

                    let bundle_id = match receive_outcome {
                        Ok(result) => {
                            imported_payments += 1;
                            let mut service = state.service.lock().unwrap();
                            let conflict_count = result.check_result.verifiable_conflicts.len();
                            let warning_count = result.check_result.unverifiable_warnings.len();
                            let conflict_summary = if conflict_count > 0 || warning_count > 0 {
                                Some(format!(
                                    "Conflicts: {}, Warnings: {}",
                                    conflict_count, warning_count
                                ))
                            } else {
                                None
                            };
                            let notes = match (
                                result
                                    .header
                                    .notes
                                    .as_deref()
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty()),
                                conflict_summary,
                            ) {
                                (Some(n), Some(cs)) => Some(format!("{} ({})", n, cs)),
                                (Some(n), None) => Some(n.to_string()),
                                (None, Some(cs)) => Some(cs),
                                (None, None) => None,
                            };
                            let record = TransactionRecord {
                                id: uuid::Uuid::new_v4().to_string(),
                                direction: "received".to_string(),
                                recipient_id: result.header.recipient_id.clone(),
                                sender_id: result.header.sender_id.clone(),
                                timestamp: Utc::now().to_rfc3339(),
                                summable_amounts: result.transfer_summary.summable_amounts.clone(),
                                countable_items: result.transfer_summary.countable_items.clone(),
                                involved_vouchers: result.involved_vouchers.clone(),
                                involved_sources_details: Some(result.involved_vouchers_details.clone()),
                                bundle_data: Vec::new(),
                                bundle_id: result.header.bundle_id.clone(),
                                notes,
                                sender_profile_name: result.header.sender_profile_name.clone(),
                            };
                            let _ = state.append_to_history(&mut service, record, password.as_deref());
                            let _ = state.refresh_events_cache(&mut service, password.as_deref());

                            if let Some(ref wot_json) = result.wot_sync_json {
                                state.try_import_piggybacked_wot(
                                    &mut service,
                                    wot_json,
                                    &result.header.sender_id,
                                    password.as_deref(),
                                );
                            }
                            if let Some(ref card_json) = result.contact_card_json {
                                state.try_import_piggybacked_contact_card(
                                    &mut service,
                                    Some(card_json),
                                    password.as_deref(),
                                );
                            }
                            Some(result.header.bundle_id)
                        }
                        Err(e) => {
                            let is_sibling_subaccount = matches!(
                                &e,
                                human_money_core::VoucherCoreError::BundleRecipientMismatch { expected, found }
                                    if crate::state::canonicalize_did(expected)
                                        .ok()
                                        .is_some_and(|ce| crate::state::canonicalize_did(found).ok().as_ref() == Some(&ce))
                            );
                            let err_str = e.to_string();
                            if is_sibling_subaccount || err_str.to_ascii_lowercase().contains("already processed") {
                                None
                            } else {
                                errors.push(format!("Receive payment bundle failed: {}", e));
                                None
                            }
                        }
                    };

                    if let Some(ref bid) = bundle_id {
                        queue_receipt(
                            &mut post_state,
                            &my_identity,
                            &my_ed_sk,
                            &sender_did,
                            &envelope.msg_id,
                            &box_entry.tag16,
                            Some(bid),
                            &own_reply_syncs,
                            now_secs,
                        );
                    }
                }
                PayloadType::VoucherForSigning => {
                    let sender_name = get_contact_name(state, &sender_did, password.as_deref());
                    post_state.pending_sign_requests.push(FrontendPendingSignRequest {
                        msg_id: envelope.msg_id.clone(),
                        sender_did: sender_did.clone(),
                        sender_name,
                        created_at: envelope.created_at,
                        reply_syncs: envelope.reply_drops.iter().cloned().map(Into::into).collect(),
                        container_bytes: envelope.container_bytes.clone(),
                    });
                    new_sign_requests += 1;
                    queue_receipt(
                        &mut post_state,
                        &my_identity,
                        &my_ed_sk,
                        &sender_did,
                        &envelope.msg_id,
                        &box_entry.tag16,
                        None,
                        &own_reply_syncs,
                        now_secs,
                    );
                }
                PayloadType::DetachedSignature => {
                    let all_standards = load_all_standard_tomls(app, extra_standards);
                    let mut attached = false;
                    {
                        let mut service = state.service.lock().unwrap();
                        for toml in all_standards.values() {
                            if service
                                .process_and_attach_signature(
                                    &envelope.container_bytes,
                                    toml,
                                    None,
                                    password.as_deref(),
                                )
                                .is_ok()
                            {
                                attached = true;
                                break;
                            }
                        }
                    }
                    if attached {
                        imported_signatures += 1;
                        let mut service = state.service.lock().unwrap();
                        let _ = state.refresh_events_cache(&mut service, password.as_deref());
                        queue_receipt(
                            &mut post_state,
                            &my_identity,
                            &my_ed_sk,
                            &sender_did,
                            &envelope.msg_id,
                            &box_entry.tag16,
                            None,
                            &own_reply_syncs,
                            now_secs,
                        );
                    }
                }
                PayloadType::TrustAssertion => {
                    match serde_json::from_slice::<WotHandshakeAssertion>(&decrypted_bytes) {
                        Ok(assertion) => {
                            if let Err(e) = validate_wot_handshake_assertion(&assertion) {
                                errors.push(format!("Invalid WoT handshake assertion: {}", e));
                            } else {
                                let is_known_trusted_contact = {
                                    let mut service = state.service.lock().unwrap();
                                    state
                                        .load_contacts(&mut service, password.as_deref())
                                        .map(|list| {
                                            list.iter().any(|c| {
                                                are_dids_equal(&c.did, &assertion.sender_did)
                                                    && c.trust_level.unwrap_or(0) >= 1
                                            })
                                        })
                                        .unwrap_or(false)
                                };

                                if is_known_trusted_contact {
                                    // Silent auto-accept for established contacts (KISS / Zero-Click)
                                    if let Ok(pkg) = serde_json::from_str::<humoco_wot::WotSyncPackage>(&assertion.wot_sync_json) {
                                        let mut service = state.service.lock().unwrap();
                                        if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
                                            let can_sender_did = crate::state::canonicalize_did(&assertion.sender_did)
                                                .unwrap_or_else(|_| assertion.sender_did.clone());
                                            let _ = reg.import_sync_package(&pkg, Some(&can_sender_did), now_secs);
                                            let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
                                            let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
                                        }
                                    }

                                    queue_receipt(
                                        &mut post_state,
                                        &my_identity,
                                        &my_ed_sk,
                                        &sender_did,
                                        &envelope.msg_id,
                                        &box_entry.tag16,
                                        None,
                                        &own_reply_syncs,
                                        now_secs,
                                    );

                                    // If counterparty requested mutual sync package, automatically queue return package
                                    if assertion.reply_requested {
                                        let can_recipient = crate::state::canonicalize_did(&sender_did)
                                            .unwrap_or_else(|_| sender_did.clone());
                                        let mut service = state.service.lock().unwrap();
                                        if let Ok(reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
                                            if let Ok((pkg, _)) = reg.prepare_sync_package(Some(&can_recipient), None, true) {
                                                if let Ok(wot_sync_json) = serde_json::to_string(&pkg) {
                                                    let my_name = service
                                                        .with_wallet(|w| w.profile.first_name.clone())
                                                        .ok()
                                                        .flatten();
                                                    let auto_reply_assertion = WotHandshakeAssertion {
                                                        sender_did: my_identity.user_id.clone(),
                                                        sender_name: my_name,
                                                        proposed_level: 1,
                                                        wot_sync_json,
                                                        reply_requested: false,
                                                        created_at: now_secs,
                                                    };
                                                    if let Ok(assertion_bytes) = serde_json::to_vec(&auto_reply_assertion) {
                                                        if let Ok(container) = SecureContainer::seal(
                                                            &my_identity,
                                                            &ContainerConfig::TargetDid(sender_did.clone(), PrivacyMode::HashedRouting),
                                                            &assertion_bytes,
                                                            PayloadType::TrustAssertion,
                                                        ) {
                                                            if let Ok(container_bytes) = serde_json::to_vec(&container) {
                                                                if let Ok(sender_ed_pk) = get_pubkey_from_user_id(&sender_did) {
                                                                    let return_msg_id = uuid::Uuid::new_v4().to_string();
                                                                    let env = PostEnvelope::new(
                                                                        return_msg_id.clone(),
                                                                        now_secs,
                                                                        own_reply_syncs.iter().cloned().map(Into::into).collect(),
                                                                        container_bytes,
                                                                    );
                                                                    if let Ok(return_box) = seal_post_box(&my_ed_sk, &sender_ed_pk, &env) {
                                                                        post_state.processed_msg_ids.insert(return_msg_id.clone());
                                                                        post_state.outbox_entries.push(PostOutboxEntry {
                                                                            msg_id: return_msg_id,
                                                                            recipient_did: sender_did.clone(),
                                                                            tag16_hex: hex::encode(return_box.tag16),
                                                                            bundle_id: None,
                                                                            entry: return_box,
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
                                    }
                                } else {
                                    // Unknown or unrated contact: require conscious user confirmation
                                    post_state.pending_wot_packages_by_msg_id.insert(
                                        envelope.msg_id.clone(),
                                        assertion.wot_sync_json.clone(),
                                    );
                                    post_state.pending_wot_handshakes.push(FrontendPendingWotHandshake {
                                        msg_id: envelope.msg_id.clone(),
                                        sender_did: assertion.sender_did.clone(),
                                        sender_name: assertion.sender_name.clone(),
                                        proposed_level: assertion.proposed_level,
                                        reply_requested: assertion.reply_requested,
                                        created_at: assertion.created_at,
                                    });
                                    new_wot_handshakes += 1;
                                    queue_receipt(
                                        &mut post_state,
                                        &my_identity,
                                        &my_ed_sk,
                                        &sender_did,
                                        &envelope.msg_id,
                                        &box_entry.tag16,
                                        None,
                                        &own_reply_syncs,
                                        now_secs,
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse WotHandshakeAssertion: {}", e));
                        }
                    }
                }
                PayloadType::Generic(ref s) if s == PAYLOAD_POST_RECEIPT_V1 => {
                    match serde_json::from_slice::<PostDeliveryReceipt>(&decrypted_bytes) {
                        Ok(receipt) => {
                            if let Err(e) = validate_post_delivery_receipt(&receipt) {
                                errors.push(format!("Invalid delivery receipt: {}", e));
                            } else {
                                let before_len = post_state.outbox_entries.len();
                                post_state.outbox_entries.retain(|e| {
                                    let matches_sender = are_dids_equal(&e.recipient_did, &sender_did);
                                    let matches_msg = e.msg_id == receipt.acknowledged_msg_id
                                        || e.tag16_hex == receipt.acknowledged_tag16_hex;
                                    !(matches_sender && matches_msg)
                                });
                                let removed = before_len - post_state.outbox_entries.len();
                                if removed > 0 {
                                    imported_receipts += removed;
                                    pruned_boxes += removed;
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse PostDeliveryReceipt: {}", e));
                        }
                    }
                }
                PayloadType::Generic(ref s) if s == PAYLOAD_MARKET_INQUIRY_V1 => {
                    match serde_json::from_slice::<MarketInquiry>(&decrypted_bytes) {
                        Ok(inquiry) => {
                            if let Err(e) = validate_market_inquiry(&inquiry) {
                                errors.push(format!("Invalid market inquiry: {}", e));
                            } else {
                                let is_target_valid = check_inquiry_target(
                                    state,
                                    &inquiry.target,
                                    now_secs,
                                    password.as_deref(),
                                );
                                if !is_target_valid {
                                    match &inquiry.target {
                                        InquiryTarget::Ad { ad_id, .. } => {
                                            if let Ok(sender_ed_pk) = get_pubkey_from_user_id(&sender_did) {
                                                let decline_reply = MarketInquiryReply {
                                                    inquiry_id: inquiry.inquiry_id.clone(),
                                                    decision: MarketInquiryDecision::Decline,
                                                    note: Some("Inserat nicht mehr verfügbar".to_string()),
                                                    payment_uri: None,
                                                    amount_hint: None,
                                                    standard_uuid: None,
                                                    replied_at: now_secs,
                                                };
                                                if let Ok(reply_bytes) = serde_json::to_vec(&decline_reply) {
                                                    if let Ok(container) = SecureContainer::seal(
                                                        &my_identity,
                                                        &ContainerConfig::TargetDid(sender_did.clone(), PrivacyMode::HashedRouting),
                                                        &reply_bytes,
                                                        PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_REPLY_V1.to_string()),
                                                    ) {
                                                        if let Ok(container_bytes) = serde_json::to_vec(&container) {
                                                            let auto_msg_id = uuid::Uuid::new_v4().to_string();
                                                            let env = PostEnvelope::new(
                                                                auto_msg_id.clone(),
                                                                now_secs,
                                                                own_reply_syncs.iter().cloned().map(Into::into).collect(),
                                                                container_bytes,
                                                            );
                                                            if let Ok(auto_box) = seal_post_box(&my_ed_sk, &sender_ed_pk, &env) {
                                                                post_state.processed_msg_ids.insert(auto_msg_id.clone());
                                                                post_state.outbox_entries.push(PostOutboxEntry {
                                                                    msg_id: auto_msg_id,
                                                                    recipient_did: sender_did.clone(),
                                                                    tag16_hex: hex::encode(auto_box.tag16),
                                                                    bundle_id: None,
                                                                    entry: auto_box,
                                                                    created_at: now_secs,
                                                                });
                                                            }
                                                        }
                                                    }
                                                }
                                                queue_receipt(
                                                    &mut post_state,
                                                    &my_identity,
                                                    &my_ed_sk,
                                                    &sender_did,
                                                    &envelope.msg_id,
                                                    &box_entry.tag16,
                                                    None,
                                                    &own_reply_syncs,
                                                    now_secs,
                                                );
                                            }
                                            errors.push(format!(
                                                "Inquiry for unknown or inquiries-disabled ad {}",
                                                ad_id
                                            ));
                                        }
                                        InquiryTarget::Person { .. } => {
                                            errors.push("Inquiry for person profile rejected (profile unpublished or inquiries disabled)".to_string());
                                        }
                                    }
                                } else {
                                    let sender_pk_hex = get_pubkey_from_user_id(&sender_did)
                                        .map(|pk| hex::encode(pk.as_bytes()))
                                        .unwrap_or_default();
                                    let target_key = inquiry.target.target_key();
                                    match post_state
                                        .inquiry_rate_tracker
                                        .record_inquiry(&sender_pk_hex, &target_key)
                                    {
                                        Ok(()) => {
                                            let sender_name =
                                                get_contact_name(state, &sender_did, password.as_deref());
                                            post_state.seller_inquiries.push(FrontendMarketInquiryItem {
                                                inquiry_id: inquiry.inquiry_id.clone(),
                                                target: inquiry.target.clone(),
                                                counterparty_did: sender_did.clone(),
                                                counterparty_name: sender_name,
                                                message: inquiry.message.clone(),
                                                created_at: inquiry.created_at,
                                                reply_syncs: envelope
                                                    .reply_drops
                                                    .iter()
                                                    .cloned()
                                                    .map(Into::into)
                                                    .collect(),
                                                reply: None,
                                            });
                                            new_inquiries += 1;
                                            queue_receipt(
                                                &mut post_state,
                                                &my_identity,
                                                &my_ed_sk,
                                                &sender_did,
                                                &envelope.msg_id,
                                                &box_entry.tag16,
                                                None,
                                                &own_reply_syncs,
                                                now_secs,
                                            );
                                        }
                                        Err(e) => {
                                            errors.push(format!(
                                                "Inquiry rate limit rejected for {}: {}",
                                                sender_did, e
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse MarketInquiry: {}", e));
                        }
                    }
                }
                PayloadType::Generic(ref s) if s == PAYLOAD_MARKET_INQUIRY_REPLY_V1 => {
                    match serde_json::from_slice::<MarketInquiryReply>(&decrypted_bytes) {
                        Ok(reply) => {
                            if let Err(e) = validate_market_inquiry_reply(&reply) {
                                errors.push(format!("Invalid market inquiry reply: {}", e));
                            } else if let Some(item) = post_state.buyer_inquiries.iter_mut().find(|i| {
                                i.inquiry_id == reply.inquiry_id
                                    && are_dids_equal(&i.counterparty_did, &sender_did)
                                    && i.reply.is_none()
                            }) {
                                let decision_str = match reply.decision {
                                    MarketInquiryDecision::Confirm => "confirm".to_string(),
                                    MarketInquiryDecision::Decline => "decline".to_string(),
                                };
                                item.reply = Some(FrontendMarketInquiryReplyDto {
                                    inquiry_id: reply.inquiry_id.clone(),
                                    decision: decision_str,
                                    note: reply.note.clone(),
                                    payment_uri: reply.payment_uri.clone(),
                                    amount_hint: reply.amount_hint.clone(),
                                    standard_uuid: reply.standard_uuid.clone(),
                                    replied_at: reply.replied_at,
                                });
                                new_inquiry_replies += 1;
                                queue_receipt(
                                    &mut post_state,
                                    &my_identity,
                                    &my_ed_sk,
                                    &sender_did,
                                    &envelope.msg_id,
                                    &box_entry.tag16,
                                    None,
                                    &own_reply_syncs,
                                    now_secs,
                                );
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse MarketInquiryReply: {}", e));
                        }
                    }
                }
                PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_ASSIGN_V1 => {
                    match serde_json::from_slice::<humoco_market::GuardianVaultEntry>(&decrypted_bytes) {
                        Ok(entry) => {
                            if !are_dids_equal(&sender_did, &entry.owner_did) {
                                errors.push(format!(
                                    "Guardian assign sender DID mismatch: {} != {}",
                                    sender_did, entry.owner_did
                                ));
                            } else {
                                let is_trusted_non_offender = {
                                    let mut service = state.service.lock().unwrap();
                                    let is_offender = service
                                        .with_wallet(|w| {
                                            matches!(
                                                w.check_reputation(&sender_did),
                                                human_money_core::models::conflict::TrustStatus::KnownOffender(_)
                                            )
                                        })
                                        .unwrap_or(false);
                                    if is_offender {
                                        false
                                    } else {
                                        state
                                            .load_contacts(&mut service, password.as_deref())
                                            .map(|list| {
                                                list.iter().any(|c| {
                                                    are_dids_equal(&c.did, &sender_did)
                                                        && c.trust_level.unwrap_or(0) >= 1
                                                })
                                            })
                                            .unwrap_or(false)
                                    }
                                };

                                if !is_trusted_non_offender {
                                    errors.push(format!(
                                        "Guardian assign from untrusted sender or known offender: {}",
                                        sender_did
                                    ));
                                } else {
                                    match humoco_market::verify_guardian_vault_entry(&entry) {
                                        Ok(()) => {
                                            let mut service = state.service.lock().unwrap();
                                            let mut vault = crate::commands::recovery::load_guardian_vault(
                                                &mut service,
                                                password.as_deref(),
                                            )
                                            .unwrap_or_else(|_| humoco_market::GuardianVaultStore::new());

                                            match vault.upsert_ward(entry.clone()) {
                                                Ok(()) => {
                                                    if let Err(e) = crate::commands::recovery::save_guardian_vault(
                                                        &mut service,
                                                        &vault,
                                                        password.as_deref(),
                                                    ) {
                                                        errors.push(format!("Failed to save guardian vault: {:?}", e));
                                                    } else {
                                                        let ack = humoco_market::GuardianVaultAck {
                                                            owner_did: entry.owner_did.clone(),
                                                            rotation_epoch: entry.rotation_epoch,
                                                            acked_at: now_secs,
                                                        };
                                                        if let Ok(ack_bytes) = serde_json::to_vec(&ack) {
                                                            if let Ok(container) = SecureContainer::seal(
                                                                &my_identity,
                                                                &ContainerConfig::TargetDid(
                                                                    sender_did.clone(),
                                                                    PrivacyMode::HashedRouting,
                                                                ),
                                                                &ack_bytes,
                                                                PayloadType::Generic(
                                                                    humoco_market::PAYLOAD_GUARDIAN_ACK_V1.to_string(),
                                                                ),
                                                            ) {
                                                                if let Ok(container_bytes) = serde_json::to_vec(&container) {
                                                                    let auto_msg_id = uuid::Uuid::new_v4().to_string();
                                                                    let env = PostEnvelope::new(
                                                                        auto_msg_id.clone(),
                                                                        now_secs,
                                                                        own_reply_syncs.iter().cloned().map(Into::into).collect(),
                                                                        container_bytes,
                                                                    );
                                                                    if let Ok(sender_ed_pk) = get_pubkey_from_user_id(&sender_did) {
                                                                        if let Ok(auto_box) = seal_post_box(&my_ed_sk, &sender_ed_pk, &env) {
                                                                            post_state.processed_msg_ids.insert(auto_msg_id.clone());
                                                                            post_state.outbox_entries.push(PostOutboxEntry {
                                                                                msg_id: auto_msg_id,
                                                                                recipient_did: sender_did.clone(),
                                                                                tag16_hex: hex::encode(auto_box.tag16),
                                                                                bundle_id: None,
                                                                                entry: auto_box,
                                                                                created_at: now_secs,
                                                                            });
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        queue_receipt(
                                                            &mut post_state,
                                                            &my_identity,
                                                            &my_ed_sk,
                                                            &sender_did,
                                                            &envelope.msg_id,
                                                            &box_entry.tag16,
                                                            None,
                                                            &own_reply_syncs,
                                                            now_secs,
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    errors.push(format!("Failed to upsert guardian ward: {}", e));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            errors.push(format!("Invalid guardian vault entry: {}", e));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse GuardianVaultEntry: {}", e));
                        }
                    }
                }
                PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_ACK_V1 => {
                    match serde_json::from_slice::<humoco_market::GuardianVaultAck>(&decrypted_bytes) {
                        Ok(ack) => {
                            let mut service = state.service.lock().unwrap();
                            if let Ok(Some(mut owner_state)) = crate::commands::recovery::load_recovery_owner_state(
                                &mut service,
                                password.as_deref(),
                            ) {
                                if ack.rotation_epoch == owner_state.rotation_epoch
                                    && are_dids_equal(&ack.owner_did, &my_identity.user_id)
                                {
                                    let mut modified = false;
                                    for g in &mut owner_state.guardians {
                                        if are_dids_equal(&g.user_id, &sender_did) {
                                            g.status = crate::models::GuardianAssignmentStatus::Confirmed {
                                                acked_at: ack.acked_at,
                                            };
                                            modified = true;
                                        }
                                    }
                                    if modified {
                                        let _ = crate::commands::recovery::save_recovery_owner_state(
                                            &mut service,
                                            &owner_state,
                                            password.as_deref(),
                                        );
                                    }
                                }
                            }
                            queue_receipt(
                                &mut post_state,
                                &my_identity,
                                &my_ed_sk,
                                &sender_did,
                                &envelope.msg_id,
                                &box_entry.tag16,
                                None,
                                &own_reply_syncs,
                                now_secs,
                            );
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse GuardianVaultAck: {}", e));
                        }
                    }
                }
                PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1 => {
                    match serde_json::from_slice::<humoco_market::GuardianLocatorUpdate>(&decrypted_bytes) {
                        Ok(update) => {
                            if !are_dids_equal(&sender_did, &update.owner_did) {
                                errors.push(format!(
                                    "Guardian locator update sender mismatch: {} != {}",
                                    sender_did, update.owner_did
                                ));
                            } else {
                                let mut service = state.service.lock().unwrap();
                                let mut vault = crate::commands::recovery::load_guardian_vault(
                                    &mut service,
                                    password.as_deref(),
                                )
                                .unwrap_or_else(|_| humoco_market::GuardianVaultStore::new());
                                match vault.update_locator(&update) {
                                    Ok(updated) => {
                                        if updated {
                                            let _ = crate::commands::recovery::save_guardian_vault(
                                                &mut service,
                                                &vault,
                                                password.as_deref(),
                                            );
                                        }
                                        queue_receipt(
                                            &mut post_state,
                                            &my_identity,
                                            &my_ed_sk,
                                            &sender_did,
                                            &envelope.msg_id,
                                            &box_entry.tag16,
                                            None,
                                            &own_reply_syncs,
                                            now_secs,
                                        );
                                    }
                                    Err(e) => {
                                        errors.push(format!("Failed to update guardian locator: {}", e));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse GuardianLocatorUpdate: {}", e));
                        }
                    }
                }
                PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_REVOKE_V1 => {
                    match serde_json::from_slice::<humoco_market::GuardianRevoke>(&decrypted_bytes) {
                        Ok(revoke) => {
                            if !are_dids_equal(&sender_did, &revoke.owner_did) {
                                errors.push(format!(
                                    "Guardian revoke sender mismatch: {} != {}",
                                    sender_did, revoke.owner_did
                                ));
                            } else {
                                let mut service = state.service.lock().unwrap();
                                let mut vault = crate::commands::recovery::load_guardian_vault(
                                    &mut service,
                                    password.as_deref(),
                                )
                                .unwrap_or_else(|_| humoco_market::GuardianVaultStore::new());
                                match vault.revoke_ward(&revoke) {
                                    Ok(revoked) => {
                                        if revoked {
                                            let _ = crate::commands::recovery::save_guardian_vault(
                                                &mut service,
                                                &vault,
                                                password.as_deref(),
                                            );
                                        }
                                        queue_receipt(
                                            &mut post_state,
                                            &my_identity,
                                            &my_ed_sk,
                                            &sender_did,
                                            &envelope.msg_id,
                                            &box_entry.tag16,
                                            None,
                                            &own_reply_syncs,
                                            now_secs,
                                        );
                                    }
                                    Err(e) => {
                                        errors.push(format!("Failed to revoke ward from guardian vault: {}", e));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse GuardianRevoke: {}", e));
                        }
                    }
                }
                _ => {
                    // Unknown or unhandled payload type
                }
            }

            post_state.processed_msg_ids.insert(envelope.msg_id.clone());
        }
    }

    // 6. Flush pass
    let (uploaded_to, flush_errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
    errors.extend(flush_errors);

    save_post_state(state, &post_state, password.as_deref())?;

    let mut retry_after_secs: Option<u64> = None;
    for e in &errors {
        if let Some(secs) = sync_client::extract_retry_after_from_error_str(e) {
            retry_after_secs = Some(retry_after_secs.map_or(secs, |prev| prev.max(secs)));
        }
    }

    if !errors.is_empty() {
        warn!(
            "Post sync degraded: error_count={}, imported_payments={}",
            errors.len(),
            imported_payments
        );
    }

    Ok(FrontendPostSyncResult {
        uploaded_to,
        polled_syncs,
        imported_payments,
        imported_receipts,
        imported_signatures,
        new_sign_requests,
        new_wot_handshakes,
        new_inquiries,
        new_inquiry_replies,
        pruned_boxes,
        errors,
        retry_after_secs,
    })
}

#[tauri::command]
pub async fn sync_post_channels(
    app: tauri::AppHandle,
    is_wifi: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostSyncResult, FrontendError> {
    sync_post_channels_impl(Some(&app), is_wifi, None, password, &state).await
}

// =========================================================================
//  INTEGRATION TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use human_money_core::test_utils;
    use crate::commands::sync::tests::{
        create_test_state, setup_test_app, temp_test_dir, MockSyncServer, ALICE_PHRASE, BOB_PHRASE,
    };
    use crate::commands::sync::{probe_public_sync_impl, save_public_sync_impl};
    use crate::commands::market::{
        save_my_market_profile_impl, set_card_published_impl, upsert_ad_impl,
    };
    use crate::models::{
        FrontendAdInput, FrontendContact, FrontendSaveMyMarketProfileRequest,
        FrontendUserProfile,
    };

    const DROP_PUB_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DROP_PUB_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const ADMIN_A: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
    const ADMIN_B: &str = "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1";

    async fn setup_test_sync(
        server: &MockSyncServer,
        drop_pub: &str,
        password: &str,
        state: &AppState,
    ) -> (String, crate::models::FrontendPublicSyncInfo) {
        let write_url = format!("{}/sync.php", server.base_url);
        let read_url = format!("{}/{}", server.base_url, server.data_dir);
        let info = probe_public_sync_impl(write_url.clone(), Some(password.to_string()), state).await.unwrap();
        server.members.lock().unwrap().push(info.access_pubkey.clone());
        let saved = save_public_sync_impl(
            write_url.clone(),
            read_url,
            drop_pub.to_string(),
            None,
            true,
            Some(password.to_string()),
            state,
        ).await.unwrap();
        (write_url, saved)
    }

    #[tokio::test]
    async fn test_dual_delivery_payment_and_receipt_idempotency_sst_v4() {
        let base_dir = temp_test_dir("dual_delivery_test");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // Start Drop Server A for Alice & Drop Server B for Bob
        let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

        let (server_a_url, alice_drop) = setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
        let (server_b_url, bob_drop) = setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

        // Alice and Bob add each other to address book with Level 2 and their respective drop
        {
            let mut s_alice = alice_state.service.lock().unwrap();
            let contact_bob = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_b_url.clone(),
                    read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                    slot_id: Some(humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_B.to_string(),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
            };
            alice_state.save_contact(&mut s_alice, contact_bob, Some(password)).unwrap();
        }

        {
            let mut s_bob = bob_state.service.lock().unwrap();
            let contact_alice = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_a_url.clone(),
                    read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                    slot_id: Some(humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_A.to_string(),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
            };
            bob_state.save_contact(&mut s_bob, contact_alice, Some(password)).unwrap();
        }

        // 1. Alice creates standard and voucher
        let standard_toml = test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let mut std_map = HashMap::new();
        std_map.insert("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), standard_toml.clone());

        {
            let mut s_alice = alice_state.service.lock().unwrap();
            s_alice.create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        first_name: Some("Alice".to_string()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "100".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    non_redeemable_test_voucher: false,
                    collateral: None,
                },
                Some(password),
            ).unwrap();
        }

        let local_id = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet_and_identity(|w, id| {
                w.list_vouchers(Some(id), None, None, None)
            }).unwrap()[0].local_instance_id.clone()
        };

        // 2. Alice transfers to Bob
        let transfer_req = human_money_core::wallet::types::MultiTransferRequest {
            recipient_id: bob_did.clone(),
            sources: vec![human_money_core::wallet::types::SourceTransfer {
                local_instance_id: local_id.clone(),
                amount_to_send: "20".to_string(),
            }],
            notes: Some("Dual delivery payment".to_string()),
            sender_profile_name: Some("Alice".to_string()),
            use_privacy_mode: Some(true),
            wot_sync_json: None,
            contact_card_json: None,
        };

        let bundle_res = {
            let mut s = alice_state.service.lock().unwrap();
            s.create_transfer_bundle_with_l2_intents(transfer_req, &std_map, None, Some(password), 0).unwrap()
        };
        let bundle_bytes = bundle_res.bundle_bytes;
        let bundle_id = bundle_res.bundle_id;

        // Save transaction record to Alice's history
        let record = TransactionRecord {
            id: bundle_id.clone(),
            direction: "sent".to_string(),
            recipient_id: bob_did.clone(),
            sender_id: alice_did.clone(),
            timestamp: Utc::now().to_rfc3339(),
            summable_amounts: HashMap::new(),
            countable_items: HashMap::new(),
            involved_vouchers: vec![local_id.clone()],
            involved_sources_details: None,
            bundle_data: bundle_bytes.clone(),
            bundle_id: bundle_id.clone(),
            notes: None,
            sender_profile_name: Some("Alice".to_string()),
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.append_to_history(&mut s, record, Some(password)).unwrap();
        }

        // 3. Alice deposits to post twice: must be identical bytes, NEVER calling create_transfer_bundle again!
        let dep1 = deposit_transfer_to_post_impl(bundle_id.clone(), Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(dep1.deposited_to, 1);
        let dep2 = deposit_transfer_to_post_impl(bundle_id.clone(), Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(dep1.msg_id, dep2.msg_id);

        let post_state_alice = load_post_state(&alice_state, Some(password)).unwrap();
        assert_eq!(post_state_alice.outbox_entries.len(), 1);
        assert_eq!(post_state_alice.outbox_entries[0].bundle_id.as_deref(), Some(bundle_id.as_str()));

        // 4. Bob syncs post drops: receives payment & queues receipt
        let bob_sync = sync_post_channels_impl(None, None, Some(&std_map), Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync.imported_payments, 1);

        // 5. Bob tries to import the same bundle offline/manually: strictly idempotent!
        let offline_import_attempt = {
            let mut s = bob_state.service.lock().unwrap();
            s.receive_bundle(&bundle_bytes, &std_map, None, Some(password), false)
        };
        assert!(offline_import_attempt.is_err(), "Duplicate import must return BundleAlreadyProcessed");

        // 6. Alice syncs post drops: downloads receipt from Bob, prunes acknowledged outbox entry!
        let alice_sync = sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(alice_sync.imported_receipts, 1);
        assert_eq!(alice_sync.pruned_boxes, 1);

        let post_state_alice_final = load_post_state(&alice_state, Some(password)).unwrap();
        assert!(post_state_alice_final.outbox_entries.is_empty(), "Outbox entry must be acknowledged and pruned");
    }

    #[tokio::test]
    async fn test_asymmetry_alice_has_sync_bob_has_none() {
        let base_dir = temp_test_dir("asymmetry_test");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // Server for Alice only; Bob has NO drop server
        let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let (server_a_url, alice_drop) = setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;

        // Bob has Alice in his address book with Alice's drop
        {
            let mut s_bob = bob_state.service.lock().unwrap();
            let contact_alice = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_a_url.clone(),
                    read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                    slot_id: Some(humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_A.to_string(),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
            };
            bob_state.save_contact(&mut s_bob, contact_alice, Some(password)).unwrap();
        }

        // Alice creates and deposits payment
        let standard_toml = test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let mut std_map = HashMap::new();
        std_map.insert("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), standard_toml.clone());

        {
            let mut s = alice_state.service.lock().unwrap();
            s.create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "50".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    non_redeemable_test_voucher: false,
                    collateral: None,
                },
                Some(password),
            ).unwrap();
        }

        let local_id = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet_and_identity(|w, id| {
                w.list_vouchers(Some(id), None, None, None)
            }).unwrap()[0].local_instance_id.clone()
        };

        let transfer_req = human_money_core::wallet::types::MultiTransferRequest {
            recipient_id: bob_did.clone(),
            sources: vec![human_money_core::wallet::types::SourceTransfer {
                local_instance_id: local_id.clone(),
                amount_to_send: "10".to_string(),
            }],
            notes: Some("Asymmetry test".to_string()),
            sender_profile_name: Some("Alice".to_string()),
            use_privacy_mode: Some(true),
            wot_sync_json: None,
            contact_card_json: None,
        };

        let bundle_res = {
            let mut s = alice_state.service.lock().unwrap();
            s.create_transfer_bundle_with_l2_intents(transfer_req, &std_map, None, Some(password), 0).unwrap()
        };

        let record = TransactionRecord {
            id: bundle_res.bundle_id.clone(),
            direction: "sent".to_string(),
            recipient_id: bob_did.clone(),
            sender_id: alice_did.clone(),
            timestamp: Utc::now().to_rfc3339(),
            summable_amounts: HashMap::new(),
            countable_items: HashMap::new(),
            involved_vouchers: vec![local_id.clone()],
            involved_sources_details: None,
            bundle_data: bundle_res.bundle_bytes,
            bundle_id: bundle_res.bundle_id.clone(),
            notes: None,
            sender_profile_name: Some("Alice".to_string()),
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.append_to_history(&mut s, record, Some(password)).unwrap();
        }

        deposit_transfer_to_post_impl(bundle_res.bundle_id, Some(password.to_string()), &alice_state).await.unwrap();

        // Bob syncs: Bob has 0 write drops, but can read Alice statically and import payment!
        let bob_sync = sync_post_channels_impl(None, None, Some(&std_map), Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync.imported_payments, 1);
        assert_eq!(bob_sync.uploaded_to, 0, "Bob has no write drops, 0 uploads");
    }

    #[tokio::test]
    async fn test_asymmetry_bob_has_sync_alice_has_none() {
        let base_dir = temp_test_dir("asymmetry_bob_has_drop");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // Server for Bob only; Alice has NO drop server configured in settings
        let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;
        let (server_b_url, bob_drop) = setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

        // Alice registers as a member on the drop server, but does NOT save it to her settings
        let alice_drop_probe = probe_public_sync_impl(server_b_url.clone(), Some(password.to_string()), &alice_state).await.unwrap();
        server_b.members.lock().unwrap().push(alice_drop_probe.access_pubkey.clone());

        // Verify Alice has 0 drops in settings
        {
            let mut s = alice_state.service.lock().unwrap();
            let s_settings = alice_state.load_settings(&mut s, Some(password)).unwrap();
            assert_eq!(s_settings.market_selected_syncs.len(), 0);
            assert_eq!(s_settings.public_syncs.len(), 0);
        }

        // Alice has Bob in her address book with Bob's drop
        {
            let mut s_alice = alice_state.service.lock().unwrap();
            let contact_bob = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_b_url.clone(),
                    read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                    slot_id: Some(humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_B.to_string(),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
            };
            alice_state.save_contact(&mut s_alice, contact_bob, Some(password)).unwrap();
        }

        // Bob has Alice in his address book with level 2
        {
            let mut s_bob = bob_state.service.lock().unwrap();
            let contact_alice = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_b_url.clone(),
                    read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                    slot_id: Some(humoco_market::format_slot_id(&alice_drop_probe.access_pubkey, Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_B.to_string(),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
            };
            bob_state.save_contact(&mut s_bob, contact_alice, Some(password)).unwrap();
        }

        // Alice creates standard and voucher
        let standard_toml = test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let mut std_map = HashMap::new();
        std_map.insert("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), standard_toml.clone());

        {
            let mut s = alice_state.service.lock().unwrap();
            s.create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "50".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    non_redeemable_test_voucher: false,
                    collateral: None,
                },
                Some(password),
            ).unwrap();
        }

        let local_id = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet_and_identity(|w, id| {
                w.list_vouchers(Some(id), None, None, None)
            }).unwrap()[0].local_instance_id.clone()
        };

        let transfer_req = human_money_core::wallet::types::MultiTransferRequest {
            recipient_id: bob_did.clone(),
            sources: vec![human_money_core::wallet::types::SourceTransfer {
                local_instance_id: local_id.clone(),
                amount_to_send: "10".to_string(),
            }],
            notes: Some("Alice uploads to Bob's drop".to_string()),
            sender_profile_name: Some("Alice".to_string()),
            use_privacy_mode: Some(true),
            wot_sync_json: None,
            contact_card_json: None,
        };

        let bundle_res = {
            let mut s = alice_state.service.lock().unwrap();
            s.create_transfer_bundle_with_l2_intents(transfer_req, &std_map, None, Some(password), 0).unwrap()
        };

        let record = TransactionRecord {
            id: bundle_res.bundle_id.clone(),
            direction: "sent".to_string(),
            recipient_id: bob_did.clone(),
            sender_id: alice_did.clone(),
            timestamp: Utc::now().to_rfc3339(),
            summable_amounts: HashMap::new(),
            countable_items: HashMap::new(),
            involved_vouchers: vec![local_id.clone()],
            involved_sources_details: None,
            bundle_data: bundle_res.bundle_bytes,
            bundle_id: bundle_res.bundle_id.clone(),
            notes: None,
            sender_profile_name: Some("Alice".to_string()),
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.append_to_history(&mut s, record, Some(password)).unwrap();
        }

        // Alice has no drops in settings, but deposit_transfer_to_post_impl successfully uploads to Bob's drop!
        let deposit_res = deposit_transfer_to_post_impl(bundle_res.bundle_id, Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(deposit_res.deposited_to, 1, "Must upload to recipient's drop even when sender has no drops");

        // Bob syncs from his own drop and imports payment!
        let bob_sync = sync_post_channels_impl(None, None, Some(&std_map), Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync.imported_payments, 1);
    }

    #[tokio::test]
    async fn test_multi_device_if_match_412_merge() {
        let base_dir = temp_test_dir("multi_device_merge");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        let _alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let (server_a_url, alice_drop) = setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;

        // Pre-populate server with a slot containing dummy/remote entries and a higher ETag
        let (_my_ed_sk, master_seed) = {
            let s = alice_state.service.lock().unwrap();
            let sk = s.with_wallet_and_identity(|_w, id| id.signing_key.clone()).unwrap();
            let seed = sk.to_bytes();
            (sk, seed)
        };
        let dummy_slot = humoco_market::encode_post_slot(&master_seed, &[], DROP_POST_MAX_BYTES).unwrap();
        let slot_key = format!("{}_pc.post", alice_drop.access_pubkey);
        server_a.slots.lock().unwrap().insert(slot_key.clone(), dummy_slot);
        server_a.etags.lock().unwrap().insert(slot_key, "\"etag_device_2\"".to_string());

        // Alice local state has an out-of-date or empty ETag
        let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();
        post_state.slot_etags_by_write_url.insert(server_a_url.clone(), "\"etag_stale\"".to_string());
        save_post_state(&alice_state, &post_state, Some(password)).unwrap();

        // Flush post slots: PUT with stale ETag -> 412 Precondition Failed -> fetch remote -> merge -> retry with "etag_device_2" -> saved!
        let (uploaded, errors) = flush_post_slots_impl(&alice_state, &mut post_state, Some(password)).await;
        assert_eq!(uploaded, 1, "Must successfully merge and upload on 412: {:?}", errors);
        assert!(errors.is_empty());
    }

    #[tokio::test]
    async fn test_async_co_signing_voucher_for_signing_to_detached_signature() {
        let base_dir = temp_test_dir("co_signing_test");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

        let (server_a_url, alice_drop) = setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
        let (server_b_url, bob_drop) = setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

        {
            let mut s_alice = alice_state.service.lock().unwrap();
            let contact_bob = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_b_url.clone(),
                    read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                    slot_id: Some(humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_B.to_string(),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
            };
            alice_state.save_contact(&mut s_alice, contact_bob, Some(password)).unwrap();
        }

        {
            let mut s_bob = bob_state.service.lock().unwrap();
            let contact_alice = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: server_a_url.clone(),
                    read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                    slot_id: Some(humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_A.to_string(),
                    last_seen_at: None,
                    last_error_at: None,
                    fail_count: 0,
                    status: "active".to_string(),
                    last_probed_at: None,
                    ..Default::default()
                }],
            };
            bob_state.save_contact(&mut s_bob, contact_alice, Some(password)).unwrap();
        }

        // Alice creates voucher
        let standard_toml = test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let mut std_map = HashMap::new();
        std_map.insert("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), standard_toml.clone());
        {
            let mut s_alice = alice_state.service.lock().unwrap();
            s_alice.create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        first_name: Some("Alice".to_string()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "100".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    non_redeemable_test_voucher: false,
                    collateral: None,
                },
                Some(password),
            ).unwrap();
        }

        let local_id = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet_and_identity(|w, id| {
                w.list_vouchers(Some(id), None, None, None)
            }).unwrap()[0].local_instance_id.clone()
        };

        let signing_req_bytes = {
            let s = alice_state.service.lock().unwrap();
            let config = ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::HashedRouting);
            s.create_signing_request_bundle(&local_id, config).unwrap()
        };

        // Alice sends signing request via post
        send_signing_request_via_post_impl(bob_did.clone(), signing_req_bytes, Some(password.to_string()), &alice_state).await.unwrap();

        // Bob syncs post drops: receives sign request
        let bob_sync = sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync.new_sign_requests, 1);

        let bob_post = load_post_state(&bob_state, Some(password)).unwrap();
        let sign_req = &bob_post.pending_sign_requests[0];

        // Bob signs and creates detached signature response
        let sig_response_bytes = {
            let mut bob_s = bob_state.service.lock().unwrap();
            let voucher = bob_s.open_voucher_signing_request(&sign_req.container_bytes, None).unwrap();
            let config = ContainerConfig::TargetDid(alice_did.clone(), PrivacyMode::HashedRouting);
            bob_s.create_detached_signature_response_bundle(
                &voucher,
                "guarantor",
                false,
                config,
                Some(password),
            ).unwrap()
        };

        // Bob replies with detached signature via post
        reply_detached_signature_via_post_impl(
            Some(sign_req.msg_id.clone()),
            alice_did.clone(),
            sig_response_bytes,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();

        // Alice syncs post drops: detached signature is automatically attached!
        let alice_sync = sync_post_channels_impl(None, None, Some(&std_map), Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(alice_sync.imported_signatures, 1);

        let updated_voucher = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| {
                w.voucher_store.vouchers.get(&local_id).unwrap().clone()
            }).unwrap()
        };
        assert!(updated_voucher.voucher.signatures.iter().any(|s| s.role == "guarantor"));
    }

    #[tokio::test]
    async fn test_async_wot_handshake_and_1_step_marketplace_inquiry_cycle() {
        let base_dir = temp_test_dir("wot_and_market_test");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

        let (server_a_url, alice_drop) = setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
        let (server_b_url, bob_drop) = setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

        let alice_probe_b = probe_public_sync_impl(server_b_url.clone(), Some(password.to_string()), &alice_state).await.unwrap();
        server_b.members.lock().unwrap().push(alice_probe_b.access_pubkey.clone());
        let bob_probe_a = probe_public_sync_impl(server_a_url.clone(), Some(password.to_string()), &bob_state).await.unwrap();
        server_a.members.lock().unwrap().push(bob_probe_a.access_pubkey.clone());

        let alice_syncs = vec![FrontendSyncDescriptor {
            write_url: server_a_url.clone(),
            read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
            slot_id: Some(humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap()),
            drop_pub: DROP_PUB_A.to_string(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        }];

        let bob_syncs = vec![FrontendSyncDescriptor {
            write_url: server_b_url.clone(),
            read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
            slot_id: Some(humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap()),
            drop_pub: DROP_PUB_B.to_string(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        }];

        {
            let mut s_alice = alice_state.service.lock().unwrap();
            let contact_bob = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(1),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: bob_syncs.clone(),
            };
            alice_state.save_contact(&mut s_alice, contact_bob, Some(password)).unwrap();
        }

        {
            let mut s_bob = bob_state.service.lock().unwrap();
            let contact_alice = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(1),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: alice_syncs.clone(),
            };
            bob_state.save_contact(&mut s_bob, contact_alice, Some(password)).unwrap();
        }

        // Part A: WoT Handshake
        send_wot_handshake_via_post_impl(bob_did.clone(), 1, true, Some(password.to_string()), &alice_state).await.unwrap();

        // Bob syncs post drops: since Bob already has Alice in contacts as trust_level 1, it is SILENTLY auto-accepted! (Zero-Click)
        let bob_sync_1 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync_1.new_wot_handshakes, 0, "Known trusted contacts must be silently auto-accepted without pending handshake banner");

        // Verify Bob's WoT register now contains Alice's list
        {
            let mut s_bob = bob_state.service.lock().unwrap();
            let reg = bob_state.get_or_load_wot_register(&mut s_bob, Some(password)).unwrap();
            assert!(reg.raw_list_store.contains_key(&alice_did) || reg.raw_list_store.contains_key(&crate::state::canonicalize_did(&alice_did).unwrap()));
        }

        // Alice already has Bob rated as trust_level 1, so Bob's auto-replied handshake is also silently auto-accepted!
        let alice_sync_1 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(alice_sync_1.new_wot_handshakes, 0);

        // Verify Alice's WoT register now contains Bob's list
        {
            let mut s_alice = alice_state.service.lock().unwrap();
            let reg = alice_state.get_or_load_wot_register(&mut s_alice, Some(password)).unwrap();
            assert!(reg.raw_list_store.contains_key(&bob_did) || reg.raw_list_store.contains_key(&crate::state::canonicalize_did(&bob_did).unwrap()));
        }

        // Part B: 1-Step Marketplace Inquiry
        // Bob creates and saves profile with allow_inquiries: true and adds an ad
        let save_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![humoco_market::Skill {
                title: "Honey".to_string(),
                text: None,
                pricing: None,
                is_remote: false,
            }],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
        };
        save_my_market_profile_impl(save_req, Some(password.to_string()), &bob_state).unwrap();
        set_card_published_impl(true, false, Some(password.to_string()), &bob_state).await.unwrap();

        let ad_input = FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Forest Honey".to_string(),
            text: Some("500g Jar".to_string()),
            pricing: Some("10 Minuto".to_string()),
            duration_days: 30,
            allow_inquiries: true,
            is_remote: false,
            image_webp_data_uri: None,
        };
        let updated_bob_profile = upsert_ad_impl(ad_input, Some(password.to_string()), &bob_state).unwrap();
        let ad_id = updated_bob_profile.ads[0].id.clone();

        // Alice sends inquiry
        let inq_item = send_market_inquiry_via_post_impl(
            bob_did.clone(),
            InquiryTarget::Ad {
                ad_id,
                ad_title: "Forest Honey".to_string(),
            },
            "Do you have 2 jars available?".to_string(),
            bob_syncs.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Bob syncs: receives inquiry
        let bob_sync_2 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync_2.new_inquiries, 1);

        let bob_post_2 = load_post_state(&bob_state, Some(password)).unwrap();
        let seller_inq = &bob_post_2.seller_inquiries[0];

        // Bob replies: confirm + payment link
        let rep_res = reply_market_inquiry_via_post_impl(
            seller_inq.inquiry_id.clone(),
            "confirm".to_string(),
            Some("Yes, 2 jars reserved!".to_string()),
            Some("humoco://pay/123".to_string()),
            Some("20 Minuto".to_string()),
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert!(rep_res.reply.is_some());

        // Second reply is strictly forbidden (1-step only, NO chat!)
        let second_reply = reply_market_inquiry_via_post_impl(
            seller_inq.inquiry_id.clone(),
            "decline".to_string(),
            None,
            None,
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await;
        assert!(second_reply.is_err(), "Second reply must be rejected");

        // Alice syncs: receives reply
        let alice_sync_2 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(alice_sync_2.new_inquiry_replies, 1);

        let alice_post_2 = load_post_state(&alice_state, Some(password)).unwrap();
        assert!(alice_post_2.buyer_inquiries[0].reply.is_some());
        assert_eq!(
            alice_post_2.buyer_inquiries[0].reply.as_ref().unwrap().decision,
            "confirm"
        );

        // Dismiss pending post item works
        dismiss_pending_post_item_impl(
            "buyer_inquiry".to_string(),
            inq_item.inquiry_id.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        let alice_post_3 = load_post_state(&alice_state, Some(password)).unwrap();
        assert!(alice_post_3.buyer_inquiries.is_empty());
    }

    #[tokio::test]
    async fn test_own_subaccount_post_compartment_and_sibling_isolation() {
        let base_dir = temp_test_dir("subaccount_post_isolation");
        let password = "TestPassword123!";

        let create_sub = |dir: &std::path::Path, name: &str, prefix: Option<&str>| {
            let mut svc = human_money_core::app_service::AppService::new(dir).unwrap();
            svc.create_profile(
                name,
                ALICE_PHRASE,
                None,
                prefix,
                password,
                human_money_core::MnemonicLanguage::English,
                format!("inst-{}", name),
                true,
            )
            .unwrap();
            svc.unlock_session(password, 3600).unwrap();
            create_test_state(svc)
        };

        let root_state = create_sub(&base_dir.join("root"), "Alice Root", None);
        let laden_state = create_sub(&base_dir.join("laden"), "Alice Laden", Some("laden"));
        let privat_state = create_sub(&base_dir.join("privat"), "Alice Privat", Some("privat"));

        let root_did = root_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let laden_did = laden_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let privat_did = privat_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let server = MockSyncServer::start("d-shared".to_string(), ADMIN_A.to_string()).await;
        let (_url_r, _drop_r) = setup_test_sync(&server, DROP_PUB_A, password, &root_state).await;
        let (server_url, laden_drop) = setup_test_sync(&server, DROP_PUB_A, password, &laden_state).await;
        let (_url_p, _drop_p) = setup_test_sync(&server, DROP_PUB_A, password, &privat_state).await;

        // Root saves laden subaccount as contact (no trust level), and laden + privat save Root so post_accept_from_contacts accepts Root
        let make_contact = |did: String, name: &str| FrontendContact {
            did,
            profile: FrontendUserProfile {
                first_name: Some(name.to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_url.clone(),
                read_url: format!("{}/{}", server.base_url, server.data_dir),
                slot_id: None,
                drop_pub: DROP_PUB_A.to_string(),
                status: "active".to_string(),
                ..Default::default()
            }],
        };
        {
            let mut s = root_state.service.lock().unwrap();
            root_state.save_contact(&mut s, make_contact(laden_did.clone(), "Laden"), Some(password)).unwrap();
        }
        {
            let mut s = laden_state.service.lock().unwrap();
            laden_state.save_contact(&mut s, make_contact(root_did.clone(), "Root"), Some(password)).unwrap();
        }
        {
            let mut s = privat_state.service.lock().unwrap();
            privat_state.save_contact(&mut s, make_contact(root_did.clone(), "Root"), Some(password)).unwrap();
        }

        // Root creates voucher and sends to Laden subaccount
        let standard_toml = test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let mut std_map = HashMap::new();
        std_map.insert("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), standard_toml.clone());

        {
            let mut s = root_state.service.lock().unwrap();
            s.create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(root_did.clone()),
                        first_name: Some("Root".to_string()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "50".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    non_redeemable_test_voucher: false,
                    collateral: None,
                },
                Some(password),
            )
            .unwrap();
        }
        let local_id = {
            let s = root_state.service.lock().unwrap();
            s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None)).unwrap()[0]
                .local_instance_id
                .clone()
        };
        let bundle_res = {
            let mut s = root_state.service.lock().unwrap();
            s.create_transfer_bundle_with_l2_intents(
                human_money_core::wallet::types::MultiTransferRequest {
                    recipient_id: laden_did.clone(),
                    sources: vec![human_money_core::wallet::types::SourceTransfer {
                        local_instance_id: local_id.clone(),
                        amount_to_send: "15".to_string(),
                    }],
                    notes: Some("To shop subaccount".to_string()),
                    sender_profile_name: Some("Root".to_string()),
                    use_privacy_mode: Some(true),
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &std_map,
                None,
                Some(password),
                0,
            )
            .unwrap()
        };
        {
            let mut s = root_state.service.lock().unwrap();
            root_state
                .append_to_history(
                    &mut s,
                    TransactionRecord {
                        id: bundle_res.bundle_id.clone(),
                        direction: "sent".to_string(),
                        recipient_id: laden_did.clone(),
                        sender_id: root_did.clone(),
                        timestamp: Utc::now().to_rfc3339(),
                        summable_amounts: HashMap::new(),
                        countable_items: HashMap::new(),
                        involved_vouchers: vec![local_id],
                        involved_sources_details: None,
                        bundle_data: bundle_res.bundle_bytes,
                        bundle_id: bundle_res.bundle_id.clone(),
                        notes: None,
                        sender_profile_name: Some("Root".to_string()),
                    },
                    Some(password),
                )
                .unwrap();
        }

        let dep = deposit_transfer_to_post_impl(bundle_res.bundle_id.clone(), Some(password.to_string()), &root_state)
            .await
            .unwrap();
        assert_eq!(dep.deposited_to, 1);

        // 1. Sibling subaccount ("privat") polls: must NOT fail with NotAnIntendedRecipient or import Laden's payment
        let privat_sync = sync_post_channels_impl(None, None, Some(&std_map), Some(password.to_string()), &privat_state)
            .await
            .unwrap();
        assert_eq!(privat_sync.imported_payments, 0);
        assert!(privat_sync.errors.is_empty(), "Sibling subaccount poll must stay error-free, got {:?}", privat_sync.errors);

        // 2. Target subaccount ("laden") polls: receives payment cleanly and writes receipt to its own prefixed compartment
        let laden_sync = sync_post_channels_impl(None, None, Some(&std_map), Some(password.to_string()), &laden_state)
            .await
            .unwrap();
        assert_eq!(laden_sync.imported_payments, 1);
        assert!(laden_sync.errors.is_empty(), "Target subaccount poll must succeed without errors, got {:?}", laden_sync.errors);

        // Flush laden's receipt and verify its prefixed compartment key exists on server
        let _ = sync_post_channels_impl(None, None, Some(&std_map), Some(password.to_string()), &laden_state)
            .await
            .unwrap();
        let expected_laden_slot = format!("{}_laden.post", laden_drop.access_pubkey);
        assert!(
            server.slots.lock().unwrap().contains_key(&expected_laden_slot),
            "Subaccount must store its Post slot in dedicated prefixed compartment '{}'",
            expected_laden_slot
        );
        let _ = privat_did;
        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn test_deposit_transfer_to_post_persists_outbox_when_sync_server_unreachable_and_sync_survives_outage() {
        let base_dir = temp_test_dir("post_sync_outage_test");
        let password = "TestPassword123!";

        let alice_state = create_test_state(setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password));
        let bob_state = create_test_state(setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password));

        let alice_did = alice_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let bob_did = bob_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();

        // Bind and drop a loopback listener to obtain a guaranteed unreachable port
        let closed_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_port = closed_listener.local_addr().unwrap().port();
        drop(closed_listener);
        let dead_write_url = format!("http://127.0.0.1:{}/sync.php", closed_port);
        let dead_read_url = format!("http://127.0.0.1:{}/data", closed_port);

        // Configure Bob's dead sync target in Alice's address book
        {
            let mut s_alice = alice_state.service.lock().unwrap();
            let contact_bob = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![FrontendSyncDescriptor {
                    write_url: dead_write_url.clone(),
                    read_url: dead_read_url.clone(),
                    slot_id: Some(humoco_market::format_slot_id("1111111111111111111111111111111111111111111111111111111111111111", Some("pc")).unwrap()),
                    drop_pub: DROP_PUB_B.to_string(),
                    status: "active".to_string(),
                    ..Default::default()
                }],
            };
            alice_state.save_contact(&mut s_alice, contact_bob, Some(password)).unwrap();
        }

        // Configure Bob's own public sync to the dead port as well
        {
            let mut s_bob = bob_state.service.lock().unwrap();
            let mut bob_settings = bob_state
                .load_settings(&mut s_bob, Some(password))
                .unwrap_or_default();
            bob_settings.public_syncs.push(crate::settings::StoredPublicSync {
                write_url: dead_write_url,
                read_url: dead_read_url,
                drop_pub: DROP_PUB_B.to_string(),
                access_pubkey: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                enabled: true,
                authorized_channels: vec!["pc".to_string()],
                ..Default::default()
            });
            bob_state.save_settings(&mut s_bob, bob_settings, Some(password)).unwrap();
        }

        // Alice creates a voucher and transfer bundle
        let standard_toml = test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let mut std_map = HashMap::new();
        std_map.insert("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), standard_toml.clone());

        {
            let mut s_alice = alice_state.service.lock().unwrap();
            alice_state
                .save_settings(&mut s_alice, crate::settings::AppSettings::default(), Some(password))
                .unwrap();
            s_alice.create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "100".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(password),
            ).unwrap();
        }

        let local_id = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
                .unwrap()[0]
                .local_instance_id
                .clone()
        };

        let bundle_res = crate::commands::transfers::create_transfer_bundle_impl(
            None,
            bob_did.clone(),
            vec![crate::models::FrontendSourceTransfer {
                local_instance_id: local_id,
                amount_to_send: "25".to_string(),
            }],
            Some("Sync outage test".to_string()),
            Some("Alice".to_string()),
            std_map.clone(),
            Some(false),
            Some(false),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .expect("L1 transfer creation must succeed even when Sync and L2 are down");

        {
            let mut s_alice = alice_state.service.lock().unwrap();
            let record = crate::models::TransactionRecord {
                id: uuid::Uuid::new_v4().to_string(),
                direction: "sent".to_string(),
                recipient_id: bob_did,
                sender_id: alice_did,
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts: HashMap::from([("Taler".to_string(), "25".to_string())]),
                countable_items: HashMap::new(),
                involved_vouchers: bundle_res.involved_sources_details.iter().map(|d| d.local_instance_id.clone()).collect(),
                involved_sources_details: None,
                bundle_data: bundle_res.bundle_data.clone(),
                bundle_id: bundle_res.bundle_id.clone(),
                notes: None,
                sender_profile_name: Some("Alice".to_string()),
            };
            alice_state.append_to_history(&mut s_alice, record, Some(password)).unwrap();
        }

        // Deposit against dead sync server returns degraded result, and outbox entry is persisted on disk
        let dep = deposit_transfer_to_post_impl(
            bundle_res.bundle_id.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .expect("deposit_transfer_to_post_impl must degrade gracefully without Err");
        assert_eq!(dep.deposited_to, 0);
        assert!(!dep.errors.is_empty());

        let persisted_post_state = load_post_state(&alice_state, Some(password)).unwrap();
        assert_eq!(persisted_post_state.outbox_entries.len(), 1);
        assert_eq!(
            persisted_post_state.outbox_entries[0].bundle_id.as_deref(),
            Some(bundle_res.bundle_id.as_str())
        );

        // Bob polling dead sync server returns cleanly with error count, and manual offline receive succeeds
        let bob_sync = sync_post_channels_impl(
            None,
            None,
            Some(&std_map),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .expect("sync_post_channels_impl must not crash on dead sync server");
        assert_eq!(bob_sync.imported_payments, 0);
        assert!(!bob_sync.errors.is_empty());

        crate::commands::transfers::receive_bundle_impl(
            None,
            bundle_res.bundle_data,
            std_map,
            Some(password.to_string()),
            false,
            &bob_state,
        )
        .expect("Manual offline receive_bundle_impl must succeed while sync is down");

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
