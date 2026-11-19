use ed25519_dalek::{SigningKey, VerifyingKey};
use human_money_core::crypto::get_pubkey_from_user_id;
use human_money_core::models::profile::UserIdentity;
use human_money_core::models::secure_container::{
    ContainerConfig, PayloadType, PrivacyMode, SecureContainer,
};
use humoco_market::{seal_post_box, PostDeliveryReceipt, PostEnvelope, PAYLOAD_POST_RECEIPT_V1};
use log::warn;

use crate::models::{FrontendError, FrontendSyncDescriptor};
use crate::services::common::{are_dids_equal, SyncIdentityContext};
use crate::settings::AppSettings;
use crate::state::{
    AppState, PostOutboxEntry, StoredPostState, MARKET_CARD_STORE_KEY, MARKET_POST_STATE_KEY,
};

mod handlers;
pub mod inbox_poller;
pub mod outbox;
#[cfg(test)]
mod tests;

pub use inbox_poller::{
    dismiss_pending_post_item_impl, get_post_inbox_summary_impl, mark_inquiry_read_impl,
    sync_post_channels_impl,
};
pub(crate) use outbox::flush_post_slots_impl;
pub use outbox::{
    accept_wot_handshake_impl, deposit_transfer_to_post_impl,
    reply_detached_signature_via_post_impl, send_market_inquiry_message_via_post_impl,
    send_market_inquiry_via_post_impl, send_signing_request_via_post_impl,
    send_wot_handshake_via_post_impl,
};

/// Extracted identity and reply-sync configuration required to seal and queue Post messages.
pub(crate) struct PostSigningContext {
    pub identity: UserIdentity,
    pub sync_ctx: SyncIdentityContext,
    pub own_reply_syncs: Vec<FrontendSyncDescriptor>,
}

/// Loads the encrypted Post outbox/inbox state, falling back to default only on `StorageError::NotFound`.
pub(crate) fn load_post_state(
    state: &AppState,
    password: Option<&str>,
) -> Result<StoredPostState, FrontendError> {
    let mut service = state.service.lock().unwrap();
    crate::state::load_encrypted_json_or_default(&mut service, MARKET_POST_STATE_KEY, password)
        .map_err(FrontendError::from)
}

fn merge_vec_by_id<T: Clone, F: Fn(&T) -> &str>(
    baseline: &[T],
    working: &mut Vec<T>,
    latest: &[T],
    id_fn: F,
) {
    let base_ids: std::collections::HashSet<String> =
        baseline.iter().map(|item| id_fn(item).to_string()).collect();
    let latest_ids: std::collections::HashSet<String> =
        latest.iter().map(|item| id_fn(item).to_string()).collect();

    // Respect concurrent removals of items that existed in the baseline snapshot.
    working.retain(|item| {
        let id = id_fn(item);
        !base_ids.contains(id) || latest_ids.contains(id)
    });

    let working_ids: std::collections::HashSet<String> =
        working.iter().map(|item| id_fn(item).to_string()).collect();

    for item in latest {
        let id = id_fn(item);
        if !base_ids.contains(id) && !working_ids.contains(id) {
            working.push(item.clone());
        }
    }
}

fn merge_btree_map_3way<K: Ord + Clone, V: Eq + Clone>(
    baseline: &std::collections::BTreeMap<K, V>,
    working: &mut std::collections::BTreeMap<K, V>,
    latest: &std::collections::BTreeMap<K, V>,
) {
    let all_keys: std::collections::BTreeSet<K> = baseline
        .keys()
        .chain(working.keys())
        .chain(latest.keys())
        .cloned()
        .collect();
    for key in all_keys {
        let b = baseline.get(&key);
        let w = working.get(&key);
        if w == b {
            match latest.get(&key) {
                Some(v) => {
                    working.insert(key, v.clone());
                }
                None => {
                    working.remove(&key);
                }
            }
        }
    }
}

fn merge_inquiries_3way(
    baseline: &[crate::models::FrontendMarketInquiryItem],
    working: &mut Vec<crate::models::FrontendMarketInquiryItem>,
    latest: &[crate::models::FrontendMarketInquiryItem],
) {
    let base_ids: std::collections::HashSet<&str> =
        baseline.iter().map(|i| i.inquiry_id.as_str()).collect();
    let latest_ids: std::collections::HashSet<&str> =
        latest.iter().map(|i| i.inquiry_id.as_str()).collect();

    working.retain(|item| {
        !base_ids.contains(item.inquiry_id.as_str())
            || latest_ids.contains(item.inquiry_id.as_str())
    });

    for latest_item in latest {
        if let Some(w_item) = working
            .iter_mut()
            .find(|i| i.inquiry_id == latest_item.inquiry_id)
        {
            let base_opt = baseline
                .iter()
                .find(|i| i.inquiry_id == latest_item.inquiry_id);
            let mut existing_msg_ids: std::collections::HashSet<String> =
                w_item.messages.iter().map(|m| m.msg_id.clone()).collect();
            let mut added_msg = false;
            for msg in &latest_item.messages {
                if existing_msg_ids.insert(msg.msg_id.clone()) {
                    w_item.messages.push(msg.clone());
                    added_msg = true;
                }
            }
            if added_msg {
                w_item.messages.sort_by_key(|m| m.created_at);
            }
            w_item.updated_at = w_item.updated_at.max(latest_item.updated_at);
            if w_item.status == "open" && latest_item.status != "open" {
                w_item.status = latest_item.status.clone();
            }
            if w_item.payment_uri.is_none() && latest_item.payment_uri.is_some() {
                w_item.payment_uri = latest_item.payment_uri.clone();
            }
            if added_msg && latest_item.unread {
                w_item.unread = true;
            } else if base_opt.is_some_and(|b| b.unread == w_item.unread) {
                w_item.unread = latest_item.unread;
            }
        } else if !base_ids.contains(latest_item.inquiry_id.as_str()) {
            working.push(latest_item.clone());
        }
    }
}

/// Atomically loads the latest on-disk `StoredPostState`, 3-way merges any concurrent
/// modifications relative to `baseline` into `working`, and persists `working`.
pub(crate) fn merge_and_save_post_state(
    state: &AppState,
    baseline: &StoredPostState,
    working: &mut StoredPostState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();
    let latest: StoredPostState = crate::state::load_encrypted_json_or_default(
        &mut service,
        MARKET_POST_STATE_KEY,
        password,
    )
    .map_err(FrontendError::from)?;

    if &latest != baseline {
        merge_vec_by_id(
            &baseline.outbox_entries,
            &mut working.outbox_entries,
            &latest.outbox_entries,
            |e| e.msg_id.as_str(),
        );
        merge_btree_map_3way(
            &baseline.slot_etags_by_write_url,
            &mut working.slot_etags_by_write_url,
            &latest.slot_etags_by_write_url,
        );
        for id in &latest.processed_msg_ids {
            working.processed_msg_ids.insert(id.clone());
        }
        merge_vec_by_id(
            &baseline.pending_sign_requests,
            &mut working.pending_sign_requests,
            &latest.pending_sign_requests,
            |r| r.msg_id.as_str(),
        );
        merge_vec_by_id(
            &baseline.pending_wot_handshakes,
            &mut working.pending_wot_handshakes,
            &latest.pending_wot_handshakes,
            |h| h.msg_id.as_str(),
        );
        merge_btree_map_3way(
            &baseline.pending_wot_packages_by_msg_id,
            &mut working.pending_wot_packages_by_msg_id,
            &latest.pending_wot_packages_by_msg_id,
        );
        merge_inquiries_3way(&baseline.inquiries, &mut working.inquiries, &latest.inquiries);
        if working.inquiry_rate_tracker == baseline.inquiry_rate_tracker
            && latest.inquiry_rate_tracker != baseline.inquiry_rate_tracker
        {
            working.inquiry_rate_tracker = latest.inquiry_rate_tracker.clone();
        } else {
            for (target, senders) in &latest.inquiry_rate_tracker.inquiries {
                working
                    .inquiry_rate_tracker
                    .inquiries
                    .entry(target.clone())
                    .or_default()
                    .extend(senders.iter().cloned());
            }
        }
    }

    let bytes = serde_json::to_vec(working)
        .map_err(|e| FrontendError::from(format!("Failed to serialize post state: {}", e)))?;
    service
        .save_encrypted_data(MARKET_POST_STATE_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

/// Persists the encrypted Post outbox/inbox state directly (test helper).
#[cfg(test)]
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

/// Collects all readable Sync endpoints for our own identity so peers know where to send replies.
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

/// Collects all own writable Post Sync drop targets from user settings.
pub(crate) fn collect_post_upload_targets(
    settings: &AppSettings,
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

/// Loads identity, signing context, and own reply sync descriptors under `service -> settings`.
pub(crate) fn load_post_signing_context(
    state: &AppState,
    password: Option<&str>,
) -> Result<PostSigningContext, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let identity = service
        .with_wallet_and_identity(|_w, id| id.clone())
        .map_err(FrontendError::from)?;
    let sync_ctx = SyncIdentityContext::from_service(&service)?;
    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password))
        .unwrap_or_default();
    let own_reply_syncs = collect_own_reply_syncs(&settings, sync_ctx.my_prefix.as_deref());
    Ok(PostSigningContext {
        identity,
        sync_ctx,
        own_reply_syncs,
    })
}

fn format_name_parts(
    first_name: Option<&str>,
    last_name: Option<&str>,
    organization: Option<&str>,
) -> Option<String> {
    let first = first_name.unwrap_or("").trim();
    let last = last_name.unwrap_or("").trim();
    let full = format!("{} {}", first, last).trim().to_string();
    if !full.is_empty() {
        return Some(full);
    }
    let org = organization.unwrap_or("").trim();
    if !org.is_empty() {
        return Some(org.to_string());
    }
    None
}

fn find_card_in_store<'a>(
    card_store: &'a humoco_market::CardStore,
    did: &str,
) -> Option<&'a humoco_market::StoredCard> {
    get_pubkey_from_user_id(did.trim())
        .ok()
        .and_then(|pk| card_store.cards.get(&pk.to_bytes()))
        .or_else(|| {
            card_store
                .cards
                .values()
                .find(|sc| are_dids_equal(&sc.card.payload.did, did))
        })
}

/// Resolves `(display_name, picture_url, is_contact)` for a DID across the
/// address book and the local marketplace business-card store.
pub(crate) fn resolve_counterparty_info(
    state: &AppState,
    did: &str,
    password: Option<&str>,
) -> (Option<String>, Option<String>, bool) {
    let mut name: Option<String> = None;
    let mut picture_url: Option<String> = None;
    let mut is_contact = false;
    let trimmed = did.trim();

    let mut service = state.service.lock().unwrap();
    if let Ok(contacts) = state.load_contacts(&mut service, password) {
        if let Some(c) = contacts
            .iter()
            .find(|c| c.did.trim() == trimmed)
            .or_else(|| contacts.iter().find(|c| are_dids_equal(&c.did, did)))
        {
            is_contact = true;
            name = format_name_parts(
                c.profile.first_name.as_deref(),
                c.profile.last_name.as_deref(),
                c.profile.organization.as_deref(),
            );
            picture_url = c
                .profile
                .picture_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
        }
    }

    if !is_contact || name.is_none() || picture_url.is_none() {
        if let Ok(bytes) = service.load_encrypted_data(MARKET_CARD_STORE_KEY, password) {
            if let Ok(card_store) = serde_json::from_slice::<humoco_market::CardStore>(&bytes) {
                if let Some(stored) = find_card_in_store(&card_store, trimmed) {
                    let card_prof = humoco_market::extract_public_profile(&stored.card);
                    if name.is_none() {
                        name = format_name_parts(
                            card_prof.first_name.as_deref(),
                            card_prof.last_name.as_deref(),
                            card_prof.organization.as_deref(),
                        );
                    }
                    if picture_url.is_none() {
                        picture_url = card_prof
                            .picture_url
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string);
                    }
                }
            }
        }
    }

    (name, picture_url, is_contact)
}

/// Looks up Sync endpoints for a counterparty across contacts and the local card store.
pub(crate) fn lookup_counterparty_syncs(
    state: &AppState,
    did: &str,
    password: Option<&str>,
) -> Vec<FrontendSyncDescriptor> {
    let mut result = Vec::new();
    let mut service = state.service.lock().unwrap();
    if let Ok(contacts) = state.load_contacts(&mut service, password) {
        if let Some(c) = contacts.iter().find(|c| are_dids_equal(&c.did, did)) {
            result.extend(c.syncs.iter().cloned());
        }
    }
    if result.is_empty() {
        if let Ok(bytes) = service.load_encrypted_data(MARKET_CARD_STORE_KEY, password) {
            if let Ok(card_store) = serde_json::from_slice::<humoco_market::CardStore>(&bytes) {
                if let Some(stored) = find_card_in_store(&card_store, did) {
                    result.extend(stored.card.payload.drops.iter().cloned().map(Into::into));
                }
            }
        }
    }
    result
}

/// Checks whether our local marketplace profile/ad currently allows the given inquiry target.
pub(crate) fn check_inquiry_target(
    state: &AppState,
    target: &humoco_market::InquiryTarget,
    now_secs: u64,
    password: Option<&str>,
) -> bool {
    let mut service = state.service.lock().unwrap();
    if let Ok(stored) =
        crate::services::market_service::load_own_market_profile_state(state, &mut service, password)
    {
        match target {
            humoco_market::InquiryTarget::Ad { ad_id, .. } => {
                return stored
                    .ads
                    .iter()
                    .any(|a| &a.id == ad_id && a.expires_at > now_secs && a.allow_inquiries);
            }
            humoco_market::InquiryTarget::Person { .. } => {
                return stored.published && stored.allow_inquiries;
            }
        }
    }
    false
}

/// Serializes `payload`, seals it into a `SecureContainer` targeted at `recipient_did`,
/// and returns the JSON-encoded container bytes.
pub(crate) fn seal_post_container_bytes<T: serde::Serialize>(
    my_identity: &UserIdentity,
    recipient_did: &str,
    payload_type: PayloadType,
    payload: &T,
    label: &str,
) -> Result<Vec<u8>, FrontendError> {
    let payload_bytes = serde_json::to_vec(payload)
        .map_err(|e| FrontendError::from(format!("Failed to serialize {}: {}", label, e)))?;
    let container = SecureContainer::seal(
        my_identity,
        &ContainerConfig::TargetDid(recipient_did.to_string(), PrivacyMode::HashedRouting),
        &payload_bytes,
        payload_type,
    )
    .map_err(FrontendError::from)?;
    serde_json::to_vec(&container)
        .map_err(|e| FrontendError::from(format!("Failed to serialize container: {}", e)))
}

/// Wraps `container_bytes` in a `PostEnvelope`, seals a `PostBoxEntry`, and queues it
/// into `post_state.outbox_entries` (also recording `msg_id` in `processed_msg_ids`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn enqueue_post_envelope(
    post_state: &mut StoredPostState,
    my_ed_sk: &SigningKey,
    recipient_did: &str,
    recipient_ed_pk: &VerifyingKey,
    msg_id: String,
    bundle_id: Option<String>,
    own_reply_syncs: &[FrontendSyncDescriptor],
    container_bytes: Vec<u8>,
    now_secs: u64,
    anchor_manifest_json: Option<String>,
) -> Result<(), FrontendError> {
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        now_secs,
        own_reply_syncs.iter().cloned().map(Into::into).collect(),
        container_bytes,
    )
    .with_anchor_manifest_json(anchor_manifest_json);
    let entry = seal_post_box(my_ed_sk, recipient_ed_pk, &envelope).map_err(FrontendError::from)?;
    post_state.processed_msg_ids.insert(msg_id.clone());
    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id,
        recipient_did: recipient_did.to_string(),
        tag16_hex: hex::encode(entry.tag16),
        bundle_id,
        entry,
        created_at: now_secs,
    });
    Ok(())
}

/// Best-effort helper to seal and queue an automatic reply payload into `post_state.outbox_entries`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn queue_sealed_reply<T: serde::Serialize>(
    post_state: &mut StoredPostState,
    my_identity: &UserIdentity,
    my_ed_sk: &SigningKey,
    recipient_did: &str,
    payload_type: PayloadType,
    payload: &T,
    msg_id: String,
    own_reply_syncs: &[FrontendSyncDescriptor],
    now_secs: u64,
    label: &str,
) {
    let Ok(recipient_pk) = get_pubkey_from_user_id(recipient_did) else {
        return;
    };
    let Ok(container_bytes) =
        seal_post_container_bytes(my_identity, recipient_did, payload_type, payload, label)
    else {
        return;
    };
    let _ = enqueue_post_envelope(
        post_state,
        my_ed_sk,
        recipient_did,
        &recipient_pk,
        msg_id,
        None,
        own_reply_syncs,
        container_bytes,
        now_secs,
        None,
    );
}

/// Queues a signed `PostDeliveryReceipt` acknowledging an incoming Post message.
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
    let container_bytes = match seal_post_container_bytes(
        my_identity,
        sender_did,
        PayloadType::Generic(PAYLOAD_POST_RECEIPT_V1.to_string()),
        &receipt,
        "delivery receipt",
    ) {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to prepare delivery receipt: {}", e.message);
            return;
        }
    };
    let recipient_pk = match get_pubkey_from_user_id(sender_did) {
        Ok(pk) => pk,
        Err(e) => {
            warn!("Invalid recipient DID for delivery receipt: {}", e);
            return;
        }
    };
    if let Err(e) = enqueue_post_envelope(
        post_state,
        my_ed_sk,
        sender_did,
        &recipient_pk,
        uuid::Uuid::new_v4().to_string(),
        None,
        own_reply_syncs,
        container_bytes,
        now_secs,
        None,
    ) {
        warn!(
            "Failed to seal post box for delivery receipt: {}",
            e.message
        );
    }
}
