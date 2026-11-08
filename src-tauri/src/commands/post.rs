use std::collections::HashMap;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use human_money_core::crypto::{create_user_id, get_pubkey_from_user_id};
use human_money_core::models::secure_container::{
    ContainerConfig, PayloadType, PrivacyMode, SecureContainer,
};
use human_money_core::models::profile::UserIdentity;
use humoco_market::{
    is_own_dummy_post_box, merge_post_slot, seal_post_box, try_open_post_box_any_sender,
    validate_market_inquiry, validate_market_inquiry_reply, validate_post_delivery_receipt,
    validate_wot_handshake_assertion, MarketInquiry, MarketInquiryDecision, MarketInquiryReply,
    PostDeliveryReceipt, PostEnvelope, WotHandshakeAssertion, DROP_POST_MAX_BYTES,
    PAYLOAD_MARKET_INQUIRY_REPLY_V1, PAYLOAD_MARKET_INQUIRY_V1, PAYLOAD_POST_RECEIPT_V1,
    POST_SLOT_MAGIC, POST_TTL_SECS,
};
use log::warn;
use tauri::State;

use crate::drop_client::{self, PutSlotOutcome};
use crate::models::{
    FrontendContact, FrontendDropDescriptor, FrontendError, FrontendMarketInquiryItem,
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

pub(crate) fn canonicalize_did_post(did: &str) -> String {
    let trimmed = did.trim();
    match get_pubkey_from_user_id(trimmed) {
        Ok(pk) => create_user_id(&pk, None).unwrap_or_else(|_| trimmed.to_string()),
        Err(_) => trimmed.to_string(),
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

pub(crate) fn collect_own_reply_drops(
    settings: &AppSettings,
    _master_seed: &[u8; 32],
) -> Vec<FrontendDropDescriptor> {
    let mut reply_drops = Vec::new();
    for d in &settings.market_selected_drops {
        if !d.read_url.is_empty() {
            reply_drops.push(d.clone());
        }
    }
    for pd in &settings.public_drops {
        if pd.enabled && !pd.read_url.is_empty() {
            reply_drops.push(FrontendDropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(pd.access_pubkey.clone()),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }
    reply_drops
}

pub(crate) fn collect_post_upload_targets(
    settings: &AppSettings,
    _master_seed: &[u8; 32],
) -> Vec<humoco_market::DropDescriptor> {
    let mut targets = Vec::new();
    for d in &settings.market_selected_drops {
        if !d.write_url.is_empty() {
            targets.push(d.clone().into());
        }
    }
    for pd in &settings.public_drops {
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
        if let Some(c) = contacts.into_iter().find(|c| are_dids_equal(&c.did, did)) {
            let first = c.profile.first_name.unwrap_or_default();
            let last = c.profile.last_name.unwrap_or_default();
            let full = format!("{} {}", first, last).trim().to_string();
            if !full.is_empty() {
                return Some(full);
            }
        }
    }
    None
}

pub(crate) fn check_has_valid_listing_for_inquiry(
    state: &AppState,
    listing_id: &str,
    password: Option<&str>,
) -> bool {
    let mut service = state.service.lock().unwrap();
    let my_did = match service.with_wallet(|w| w.get_user_id().to_string()) {
        Ok(d) => d,
        Err(_) => return false,
    };
    if let Ok(bytes) = service.load_encrypted_data(crate::state::MARKET_OWN_CARD_KEY, password) {
        if let Ok(oc) = serde_json::from_slice::<crate::state::StoredOwnCard>(&bytes) {
            for l in oc.card.listings {
                let id = humoco_market::compute_listing_id(&my_did, l.listing_type, &l.title);
                if id == listing_id && l.allow_inquiries {
                    return true;
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
    own_reply_drops: &[FrontendDropDescriptor],
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
        own_reply_drops.iter().cloned().map(Into::into).collect(),
        container_bytes,
    );
    let recipient_pk = match get_pubkey_from_user_id(sender_did) {
        Ok(pk) => pk,
        Err(e) => {
            warn!("Invalid recipient DID for delivery receipt {}: {}", sender_did, e);
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
    let (my_ed_sk, master_seed, upload_targets, client, now_secs) = {
        let mut service = state.service.lock().unwrap();
        let my_ed_sk = match service.with_wallet_and_identity(|_w, id| id.signing_key.clone()) {
            Ok(k) => k,
            Err(e) => return (0, vec![format!("Failed to access wallet signing key: {}", e)]),
        };
        let master_seed = my_ed_sk.to_bytes();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password))
            .unwrap_or_default();
        let targets = collect_post_upload_targets(&settings, &master_seed);
        let client = drop_client::create_drop_http_client();
        let now_secs = drop_client::current_unix_timestamp();
        (my_ed_sk, master_seed, targets, client, now_secs)
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

    for drop_desc in &upload_targets {
        let (id, signed_res) = if drop_desc.slot_id.is_some() {
            match humoco_market::derive_public_drop_keypair(&master_seed, &drop_desc.write_url) {
                Ok((r_sk, r_vk, _)) => {
                    let nonce = drop_client::generate_drop_nonce();
                    let signed = humoco_market::sign_member_request_direct(
                        &r_sk,
                        "PUT",
                        "post",
                        now_secs,
                        &nonce,
                        &initial_blob,
                    );
                    (hex::encode(r_vk.as_bytes()), Ok(signed))
                }
                Err(e) => (String::new(), Err(format!("Public drop key derivation error: {}", e))),
            }
        } else {
            match humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_desc.drop_pub) {
                Ok(p_hex) => {
                    let nonce = drop_client::generate_drop_nonce();
                    let signed = humoco_market::sign_member_request_stealth(
                        &my_ed_sk,
                        &drop_desc.drop_pub,
                        "PUT",
                        "post",
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
                errors.push(e);
                continue;
            }
        };

        let if_match = post_state
            .slot_etags_by_write_url
            .get(&drop_desc.write_url)
            .cloned();

        match drop_client::put_slot(
            &client,
            &drop_desc.write_url,
            &signed,
            "post",
            &initial_blob,
            if_match.as_deref(),
        )
        .await
        {
            Ok(PutSlotOutcome::Saved { etag, .. }) => {
                deposited_to += 1;
                if let Some(et) = etag {
                    post_state
                        .slot_etags_by_write_url
                        .insert(drop_desc.write_url.clone(), et);
                }
            }
            Ok(PutSlotOutcome::PreconditionFailed { current_etag }) => {
                // Multi-device conflict! Fetch current remote slot, merge local additions, and retry.
                let remote_bytes = match drop_client::get_slot(&client, &drop_desc.read_url, &id, "post").await {
                    Ok(b) => b,
                    Err(e) => {
                        errors.push(format!(
                            "Failed to fetch remote slot for 412 merge from {}: {}",
                            drop_desc.read_url, e
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
                        errors.push(format!("Failed to merge post slot on 412: {}", e));
                        continue;
                    }
                };

                let retry_signed_res = if drop_desc.slot_id.is_some() {
                    match humoco_market::derive_public_drop_keypair(&master_seed, &drop_desc.write_url) {
                        Ok((r_sk, _, _)) => {
                            let nonce = drop_client::generate_drop_nonce();
                            Ok(humoco_market::sign_member_request_direct(
                                &r_sk,
                                "PUT",
                                "post",
                                now_secs,
                                &nonce,
                                &merged_blob,
                            ))
                        }
                        Err(e) => Err(format!("Public drop key derivation error: {}", e)),
                    }
                } else {
                    let nonce = drop_client::generate_drop_nonce();
                    humoco_market::sign_member_request_stealth(
                        &my_ed_sk,
                        &drop_desc.drop_pub,
                        "PUT",
                        "post",
                        now_secs,
                        &nonce,
                        &merged_blob,
                    )
                    .map_err(|e| format!("Stealth signing error: {}", e))
                };

                match retry_signed_res {
                    Ok(retry_signed) => {
                        match drop_client::put_slot(
                            &client,
                            &drop_desc.write_url,
                            &retry_signed,
                            "post",
                            &merged_blob,
                            current_etag.as_deref(),
                        )
                        .await
                        {
                            Ok(PutSlotOutcome::Saved { etag, .. }) => {
                                deposited_to += 1;
                                if let Some(et) = etag {
                                    post_state
                                        .slot_etags_by_write_url
                                        .insert(drop_desc.write_url.clone(), et);
                                }
                            }
                            Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                errors.push(format!(
                                    "Upload to {} failed 412 Precondition Failed twice",
                                    drop_desc.write_url
                                ));
                            }
                            Err(e) => {
                                errors.push(format!(
                                    "Upload retry to {} failed: {}",
                                    drop_desc.write_url, e
                                ));
                            }
                        }
                    }
                    Err(e) => errors.push(e),
                }
            }
            Err(e) => {
                errors.push(format!("Upload to {} failed: {}", drop_desc.write_url, e));
            }
        }
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
        let (my_ed_sk, recipient_ed_pk, own_reply_drops, now_secs) = {
            let mut service = state.service.lock().unwrap();
            let my_ed_sk = service
                .with_wallet_and_identity(|_w, id| id.signing_key.clone())
                .map_err(FrontendError::from)?;
            let recipient_ed_pk = get_pubkey_from_user_id(&record.recipient_id)
                .map_err(FrontendError::from)?;
            let settings = state
                .get_cached_settings()
                .or_else(|_| state.load_settings(&mut service, password.as_deref()))
                .unwrap_or_default();
            let now_secs = drop_client::current_unix_timestamp();
            let master_seed = my_ed_sk.to_bytes();
            let drops = collect_own_reply_drops(&settings, &master_seed);
            (my_ed_sk, recipient_ed_pk, drops, now_secs)
        };

        let fresh_msg_id = uuid::Uuid::new_v4().to_string();
        let envelope = PostEnvelope::new(
            fresh_msg_id.clone(),
            now_secs,
            own_reply_drops.into_iter().map(Into::into).collect(),
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

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
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

    let (my_ed_sk, own_reply_drops, now_secs) = {
        let mut service = state.service.lock().unwrap();
        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let master_seed = my_ed_sk.to_bytes();
        let drops = collect_own_reply_drops(&settings, &master_seed);
        let now_secs = drop_client::current_unix_timestamp();
        (my_ed_sk, drops, now_secs)
    };

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_drops.into_iter().map(Into::into).collect(),
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

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
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

    let (my_ed_sk, own_reply_drops, now_secs) = {
        let mut service = state.service.lock().unwrap();
        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let master_seed = my_ed_sk.to_bytes();
        let drops = collect_own_reply_drops(&settings, &master_seed);
        let now_secs = drop_client::current_unix_timestamp();
        (my_ed_sk, drops, now_secs)
    };

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_drops.into_iter().map(Into::into).collect(),
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
                if !req.reply_drops.is_empty() {
                    let mut service = state.service.lock().unwrap();
                    state.update_contact_drops_from_card(
                        &mut service,
                        &recipient_did,
                        &req.reply_drops,
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

    let (my_identity, my_ed_sk, own_reply_drops, assertion, now_secs) = {
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
        let master_seed = my_ed_sk.to_bytes();
        let drops = collect_own_reply_drops(&settings, &master_seed);
        let now_secs = drop_client::current_unix_timestamp();

        let assertion = WotHandshakeAssertion {
            sender_did: my_did,
            sender_name: my_name,
            proposed_level,
            wot_sync_json,
            reply_requested,
            created_at: now_secs,
        };
        validate_wot_handshake_assertion(&assertion).map_err(FrontendError::from)?;

        (my_identity, my_ed_sk, drops, assertion, now_secs)
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
        own_reply_drops.into_iter().map(Into::into).collect(),
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

    let (deposited_to, errors) = flush_post_slots_impl(state, &mut post_state, password.as_deref()).await;
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

    let now_secs = drop_client::current_unix_timestamp();

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
                    drops: vec![],
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
                drops: vec![],
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
    listing_id: String,
    listing_title: String,
    message: String,
    recipient_drops: Vec<FrontendDropDescriptor>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    let now_secs = drop_client::current_unix_timestamp();
    let inquiry_id = uuid::Uuid::new_v4().to_string();

    let inquiry = MarketInquiry {
        inquiry_id: inquiry_id.clone(),
        listing_id: listing_id.clone(),
        listing_title: listing_title.clone(),
        message: message.clone(),
        created_at: now_secs,
    };
    validate_market_inquiry(&inquiry).map_err(FrontendError::from)?;

    if !recipient_drops.is_empty() {
        let mut service = state.service.lock().unwrap();
        state.update_contact_drops_from_card(
            &mut service,
            &recipient_did,
            &recipient_drops,
            password.as_deref(),
        );
    }

    let recipient_ed_pk = get_pubkey_from_user_id(&recipient_did).map_err(FrontendError::from)?;

    let (my_identity, my_ed_sk, own_reply_drops) = {
        let mut service = state.service.lock().unwrap();
        let my_identity = service
            .with_wallet_and_identity(|_w, id| id.clone())
            .map_err(FrontendError::from)?;
        let my_ed_sk = my_identity.signing_key.clone();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let master_seed = my_ed_sk.to_bytes();
        let drops = collect_own_reply_drops(&settings, &master_seed);
        (my_identity, my_ed_sk, drops)
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
        own_reply_drops.into_iter().map(Into::into).collect(),
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
            "An active connected drop is required to deposit marketplace inquiries. Errors: {:?}",
            errors
        )));
    }

    let item = FrontendMarketInquiryItem {
        inquiry_id,
        listing_id,
        listing_title,
        counterparty_did: recipient_did,
        counterparty_name: None,
        message,
        created_at: now_secs,
        reply_drops: recipient_drops,
        reply: None,
    };
    post_state.buyer_inquiries.push(item.clone());
    save_post_state(state, &post_state, password.as_deref())?;

    Ok(item)
}

#[tauri::command]
pub async fn send_market_inquiry_via_post(
    recipient_did: String,
    listing_id: String,
    listing_title: String,
    message: String,
    recipient_drops: Vec<FrontendDropDescriptor>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    send_market_inquiry_via_post_impl(recipient_did, listing_id, listing_title, message, recipient_drops, password, &state).await
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

    let now_secs = drop_client::current_unix_timestamp();
    let mut post_state = load_post_state(state, password.as_deref())?;

    let (item_idx, counterparty_did, listing_id) = {
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
        (idx, item.counterparty_did.clone(), item.listing_id.clone())
    };

    let reply_payload = MarketInquiryReply {
        inquiry_id: inquiry_id.clone(),
        listing_id: listing_id.clone(),
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
            .resolve_inquiry(&hex::encode(sender_vk.as_bytes()), &listing_id);
    }

    let recipient_ed_pk = get_pubkey_from_user_id(&counterparty_did).map_err(FrontendError::from)?;

    let (my_identity, my_ed_sk, own_reply_drops) = {
        let mut service = state.service.lock().unwrap();
        let my_identity = service
            .with_wallet_and_identity(|_w, id| id.clone())
            .map_err(FrontendError::from)?;
        let my_ed_sk = my_identity.signing_key.clone();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let master_seed = my_ed_sk.to_bytes();
        let drops = collect_own_reply_drops(&settings, &master_seed);
        (my_identity, my_ed_sk, drops)
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
        own_reply_drops.into_iter().map(Into::into).collect(),
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
        listing_id,
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

pub async fn sync_post_drops_impl(
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
            polled_drops: 0,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: 0,
            new_inquiry_replies: 0,
            pruned_boxes: 0,
            errors: Vec::new(),
        });
    }

    let client = drop_client::create_drop_http_client();
    let mut post_state = load_post_state(state, password.as_deref())?;
    let now_secs = drop_client::current_unix_timestamp();

    let mut errors = Vec::new();
    let mut polled_drops = 0;
    let mut imported_payments = 0;
    let mut imported_receipts = 0;
    let mut imported_signatures = 0;
    let mut new_sign_requests = 0;
    let mut new_wot_handshakes = 0;
    let mut new_inquiries = 0;
    let mut new_inquiry_replies = 0;
    let mut pruned_boxes = 0;

    let (my_identity, my_ed_sk, master_seed, own_reply_drops) = {
        let service = state.service.lock().unwrap();
        let my_identity = service
            .with_wallet_and_identity(|_w, id| id.clone())
            .map_err(FrontendError::from)?;
        let my_ed_sk = my_identity.signing_key.clone();
        let master_seed = my_ed_sk.to_bytes();
        let drops = collect_own_reply_drops(&settings, &master_seed);
        (my_identity, my_ed_sk, master_seed, drops)
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
        let mut direct = Vec::new();
        let mut index_urls = Vec::new();

        // Selected drops
        for d in &settings.market_selected_drops {
            if !d.read_url.is_empty() {
                if let Some(ref sid) = d.slot_id {
                    direct.push((d.read_url.clone(), sid.clone()));
                } else {
                    index_urls.push(d.read_url.clone());
                }
            }
        }

        // Public drops
        for pd in &settings.public_drops {
            if pd.enabled && !pd.read_url.is_empty() {
                direct.push((pd.read_url.clone(), pd.access_pubkey.clone()));
            }
        }

        // Contacts drops
        if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
            for c in contacts {
                for d in c.drops {
                    if !d.read_url.is_empty() {
                        if let Some(ref sid) = d.slot_id {
                            direct.push((d.read_url, sid.clone()));
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
                    for d in &sc.card.drops {
                        if !d.read_url.is_empty() {
                            if let Some(ref sid) = d.slot_id {
                                direct.push((d.read_url.clone(), sid.clone()));
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
            for d in &req.reply_drops {
                if !d.read_url.is_empty() {
                    if let Some(ref sid) = d.slot_id {
                        direct.push((d.read_url.clone(), sid.clone()));
                    } else {
                        index_urls.push(d.read_url.clone());
                    }
                }
            }
        }

        for inq in &post_state.seller_inquiries {
            for d in &inq.reply_drops {
                if !d.read_url.is_empty() {
                    if let Some(ref sid) = d.slot_id {
                        direct.push((d.read_url.clone(), sid.clone()));
                    } else {
                        index_urls.push(d.read_url.clone());
                    }
                }
            }
        }

        for inq in &post_state.buyer_inquiries {
            for d in &inq.reply_drops {
                if !d.read_url.is_empty() {
                    if let Some(ref sid) = d.slot_id {
                        direct.push((d.read_url.clone(), sid.clone()));
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
        let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();
        let mut auth = Vec::new();
        let mut inquiry = Vec::new();

        if settings.post_accept_from_contacts {
            if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
                for c in contacts {
                    let lvl = c.trust_level.unwrap_or_else(|| {
                        if let Some(ref r) = reg {
                            let can = canonicalize_did_post(&c.did);
                            r.own_contacts.get(&can).map(|e| e.level).unwrap_or(0)
                        } else {
                            0
                        }
                    });
                    if lvl >= 1 {
                        if let Ok(vk) = get_pubkey_from_user_id(&c.did) {
                            auth.push((c.did.clone(), vk));
                        }
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
            .load_encrypted_data(crate::state::MARKET_OWN_CARD_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<crate::state::StoredOwnCard>(&bytes).ok())
            .map(|oc| oc.card.valid_until > now_secs && oc.card.listings.iter().any(|l| l.allow_inquiries))
            .unwrap_or(false);

        if has_inquiry_listing {
            if let Ok(bytes) = service.load_encrypted_data(crate::state::MARKET_CARD_STORE_KEY, password.as_deref()) {
                if let Ok(cs) = serde_json::from_slice::<humoco_market::CardStore>(&bytes) {
                    for sc in cs.cards.values() {
                        if let Ok(vk) = get_pubkey_from_user_id(&sc.card.did) {
                            inquiry.push((sc.card.did.clone(), vk));
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

    for (read_url, slot_id) in direct_slot_fetches {
        polled_drops += 1;
        match drop_client::get_slot(&client, &read_url, &slot_id, "post").await {
            Ok(Some(bytes)) => fetched_slots.push(bytes),
            Ok(None) => {}
            Err(e) => errors.push(format!("Failed to fetch direct post slot from {}: {}", read_url, e)),
        }
    }

    for read_url in candidate_read_urls {
        polled_drops += 1;
        let index_res = drop_client::list_index(&client, &read_url, None, None).await;
        if let Ok(drop_client::IndexFetchResult::Modified { entries, .. }) = index_res {
            for entry in entries {
                if entry.channel != "post" {
                    continue;
                }
                match drop_client::get_slot(&client, &read_url, &entry.id, "post").await {
                    Ok(Some(bytes)) => fetched_slots.push(bytes),
                    Ok(None) => {}
                    Err(e) => errors.push(format!("Failed to fetch post slot {} from {}: {}", entry.id, read_url, e)),
                }
            }
        }
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
                    let alt_attempt = if my_identity.user_id.contains('@') {
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
                let reply_dtos: Vec<FrontendDropDescriptor> =
                    envelope.reply_drops.iter().cloned().map(Into::into).collect();
                let mut service = state.service.lock().unwrap();
                state.update_contact_drops_from_card(
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
                            let notes = if conflict_count > 0 || warning_count > 0 {
                                Some(format!(
                                    "Conflicts: {}, Warnings: {}",
                                    conflict_count, warning_count
                                ))
                            } else {
                                None
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
                            let err_str = e.to_string();
                            if err_str.to_ascii_lowercase().contains("already processed") {
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
                            &own_reply_drops,
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
                        reply_drops: envelope.reply_drops.iter().cloned().map(Into::into).collect(),
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
                        &own_reply_drops,
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
                            &own_reply_drops,
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
                                    &own_reply_drops,
                                    now_secs,
                                );
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
                                let has_valid_listing = check_has_valid_listing_for_inquiry(
                                    state,
                                    &inquiry.listing_id,
                                    password.as_deref(),
                                );
                                if !has_valid_listing {
                                    errors.push(format!(
                                        "Inquiry for unknown or inquiries-disabled listing {}",
                                        inquiry.listing_id
                                    ));
                                } else {
                                    let sender_pk_hex = get_pubkey_from_user_id(&sender_did)
                                        .map(|pk| hex::encode(pk.as_bytes()))
                                        .unwrap_or_default();
                                    match post_state
                                        .inquiry_rate_tracker
                                        .record_inquiry(&sender_pk_hex, &inquiry.listing_id)
                                    {
                                        Ok(()) => {
                                            let sender_name =
                                                get_contact_name(state, &sender_did, password.as_deref());
                                            post_state.seller_inquiries.push(FrontendMarketInquiryItem {
                                                inquiry_id: inquiry.inquiry_id.clone(),
                                                listing_id: inquiry.listing_id.clone(),
                                                listing_title: inquiry.listing_title.clone(),
                                                counterparty_did: sender_did.clone(),
                                                counterparty_name: sender_name,
                                                message: inquiry.message.clone(),
                                                created_at: inquiry.created_at,
                                                reply_drops: envelope
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
                                                &own_reply_drops,
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
                                    listing_id: reply.listing_id.clone(),
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
                                    &own_reply_drops,
                                    now_secs,
                                );
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to parse MarketInquiryReply: {}", e));
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

    Ok(FrontendPostSyncResult {
        uploaded_to,
        polled_drops,
        imported_payments,
        imported_receipts,
        imported_signatures,
        new_sign_requests,
        new_wot_handshakes,
        new_inquiries,
        new_inquiry_replies,
        pruned_boxes,
        errors,
    })
}

#[tauri::command]
pub async fn sync_post_drops(
    app: tauri::AppHandle,
    is_wifi: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPostSyncResult, FrontendError> {
    sync_post_drops_impl(Some(&app), is_wifi, None, password, &state).await
}

// =========================================================================
//  INTEGRATION TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use human_money_core::test_utils;
    use crate::commands::drops::tests::{
        create_test_state, setup_test_app, temp_test_dir, MockDropServer, ALICE_PHRASE, BOB_PHRASE,
    };
    use crate::commands::drops::{probe_public_drop_impl, save_public_drop_impl};
    use crate::commands::market::save_own_contact_card_impl;
    use crate::models::{
        FrontendCardListingInput, FrontendContact, FrontendListingType,
        FrontendSaveOwnCardRequest, FrontendUserProfile,
    };

    const DROP_PUB_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DROP_PUB_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const ADMIN_A: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
    const ADMIN_B: &str = "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1";

    async fn setup_test_drop(
        server: &MockDropServer,
        drop_pub: &str,
        password: &str,
        state: &AppState,
    ) -> (String, crate::models::FrontendPublicDropInfo) {
        let write_url = format!("{}/drop.php", server.base_url);
        let read_url = format!("{}/{}", server.base_url, server.data_dir);
        let info = probe_public_drop_impl(write_url.clone(), Some(password.to_string()), state).await.unwrap();
        server.members.lock().unwrap().push(info.access_pubkey.clone());
        let saved = save_public_drop_impl(
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
        let server_a = MockDropServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let server_b = MockDropServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

        let (server_a_url, alice_drop) = setup_test_drop(&server_a, DROP_PUB_A, password, &alice_state).await;
        let (server_b_url, bob_drop) = setup_test_drop(&server_b, DROP_PUB_B, password, &bob_state).await;

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
                drops: vec![FrontendDropDescriptor {
                    write_url: server_b_url.clone(),
                    read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                    slot_id: Some(bob_drop.access_pubkey.clone()),
                    drop_pub: DROP_PUB_B.to_string(),
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
                drops: vec![FrontendDropDescriptor {
                    write_url: server_a_url.clone(),
                    read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                    slot_id: Some(alice_drop.access_pubkey.clone()),
                    drop_pub: DROP_PUB_A.to_string(),
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
        let bob_sync = sync_post_drops_impl(None, None, Some(&std_map), Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync.imported_payments, 1);

        // 5. Bob tries to import the same bundle offline/manually: strictly idempotent!
        let offline_import_attempt = {
            let mut s = bob_state.service.lock().unwrap();
            s.receive_bundle(&bundle_bytes, &std_map, None, Some(password), false)
        };
        assert!(offline_import_attempt.is_err(), "Duplicate import must return BundleAlreadyProcessed");

        // 6. Alice syncs post drops: downloads receipt from Bob, prunes acknowledged outbox entry!
        let alice_sync = sync_post_drops_impl(None, None, None, Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(alice_sync.imported_receipts, 1);
        assert_eq!(alice_sync.pruned_boxes, 1);

        let post_state_alice_final = load_post_state(&alice_state, Some(password)).unwrap();
        assert!(post_state_alice_final.outbox_entries.is_empty(), "Outbox entry must be acknowledged and pruned");
    }

    #[tokio::test]
    async fn test_asymmetry_alice_has_drop_bob_has_none() {
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
        let server_a = MockDropServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let (server_a_url, alice_drop) = setup_test_drop(&server_a, DROP_PUB_A, password, &alice_state).await;

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
                drops: vec![FrontendDropDescriptor {
                    write_url: server_a_url.clone(),
                    read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                    slot_id: Some(alice_drop.access_pubkey.clone()),
                    drop_pub: DROP_PUB_A.to_string(),
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
        let bob_sync = sync_post_drops_impl(None, None, Some(&std_map), Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync.imported_payments, 1);
        assert_eq!(bob_sync.uploaded_to, 0, "Bob has no write drops, 0 uploads");
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

        let server_a = MockDropServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let (server_a_url, alice_drop) = setup_test_drop(&server_a, DROP_PUB_A, password, &alice_state).await;

        // Pre-populate server with a slot containing dummy/remote entries and a higher ETag
        let (_my_ed_sk, master_seed) = {
            let s = alice_state.service.lock().unwrap();
            let sk = s.with_wallet_and_identity(|_w, id| id.signing_key.clone()).unwrap();
            let seed = sk.to_bytes();
            (sk, seed)
        };
        let dummy_slot = humoco_market::encode_post_slot(&master_seed, &[], DROP_POST_MAX_BYTES).unwrap();
        let slot_key = format!("{}.post", alice_drop.access_pubkey);
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

        let server_a = MockDropServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let server_b = MockDropServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

        let (server_a_url, alice_drop) = setup_test_drop(&server_a, DROP_PUB_A, password, &alice_state).await;
        let (server_b_url, bob_drop) = setup_test_drop(&server_b, DROP_PUB_B, password, &bob_state).await;

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
                drops: vec![FrontendDropDescriptor {
                    write_url: server_b_url.clone(),
                    read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                    slot_id: Some(bob_drop.access_pubkey.clone()),
                    drop_pub: DROP_PUB_B.to_string(),
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
                drops: vec![FrontendDropDescriptor {
                    write_url: server_a_url.clone(),
                    read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                    slot_id: Some(alice_drop.access_pubkey.clone()),
                    drop_pub: DROP_PUB_A.to_string(),
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
        let bob_sync = sync_post_drops_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
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
        let alice_sync = sync_post_drops_impl(None, None, Some(&std_map), Some(password.to_string()), &alice_state).await.unwrap();
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

        let server_a = MockDropServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
        let server_b = MockDropServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

        let (server_a_url, alice_drop) = setup_test_drop(&server_a, DROP_PUB_A, password, &alice_state).await;
        let (server_b_url, bob_drop) = setup_test_drop(&server_b, DROP_PUB_B, password, &bob_state).await;

        let alice_drops = vec![FrontendDropDescriptor {
            write_url: server_a_url.clone(),
            read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
            slot_id: Some(alice_drop.access_pubkey.clone()),
            drop_pub: DROP_PUB_A.to_string(),
        }];

        let bob_drops = vec![FrontendDropDescriptor {
            write_url: server_b_url.clone(),
            read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
            slot_id: Some(bob_drop.access_pubkey.clone()),
            drop_pub: DROP_PUB_B.to_string(),
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
                drops: bob_drops.clone(),
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
                drops: alice_drops.clone(),
            };
            bob_state.save_contact(&mut s_bob, contact_alice, Some(password)).unwrap();
        }

        // Part A: WoT Handshake
        send_wot_handshake_via_post_impl(bob_did.clone(), 1, true, Some(password.to_string()), &alice_state).await.unwrap();

        let bob_sync_1 = sync_post_drops_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
        assert_eq!(bob_sync_1.new_wot_handshakes, 1);

        let bob_post = load_post_state(&bob_state, Some(password)).unwrap();
        let handshake = &bob_post.pending_wot_handshakes[0];

        accept_wot_handshake_impl(handshake.msg_id.clone(), 1, true, Some(password.to_string()), &bob_state).await.unwrap();

        let alice_sync_1 = sync_post_drops_impl(None, None, None, Some(password.to_string()), &alice_state).await.unwrap();
        assert_eq!(alice_sync_1.new_wot_handshakes, 1);

        // Part B: 1-Step Marketplace Inquiry
        // Bob creates and saves card with allow_inquiries: true
        let save_req = FrontendSaveOwnCardRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                service_offer: Some("Honey".to_string()),
                ..Default::default()
            },
            listings: vec![FrontendCardListingInput {
                listing_type: FrontendListingType::Offer,
                title: "Forest Honey".to_string(),
                text: Some("500g Jar".to_string()),
                pricing: Some("10 Minuto".to_string()),
                allow_inquiries: true,
            }],
            max_hops: 2,
            validity_days: Some(30),
        };
        save_own_contact_card_impl(save_req, Some(password.to_string()), &bob_state).unwrap();
        let listing_id = humoco_market::compute_listing_id(&bob_did, humoco_market::ListingType::Offer, "Forest Honey");

        // Alice sends inquiry
        let inq_item = send_market_inquiry_via_post_impl(
            bob_did.clone(),
            listing_id.clone(),
            "Forest Honey".to_string(),
            "Do you have 2 jars available?".to_string(),
            bob_drops.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Bob syncs: receives inquiry
        let bob_sync_2 = sync_post_drops_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
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
        let alice_sync_2 = sync_post_drops_impl(None, None, None, Some(password.to_string()), &alice_state).await.unwrap();
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
}
