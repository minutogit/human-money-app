use ed25519_dalek::{SigningKey, VerifyingKey};
use human_money_core::models::profile::UserIdentity;
use human_money_core::models::secure_container::{
    ContainerConfig, PayloadType, PrivacyMode, SecureContainer,
};
use human_money_core::ValidatedUserId;
use humoco_market::{seal_post_box, PostDeliveryReceipt, PostEnvelope, PAYLOAD_POST_RECEIPT_V1};
use log::warn;

use crate::models::{FrontendError, FrontendSyncDescriptor};
use crate::services::common::{are_dids_equal, SyncIdentityContext};
use crate::services::sync_endpoints::{
    canonical_drop_pub_hex, is_endpoint_revoked, sanitize_untrusted_sync_descriptors,
};
use crate::session::SessionToken;
use crate::settings::SecurityPolicy;
use crate::state::{
    merge_btree_map_3way, AppState, PostOutboxEntry, StoredPostState, MARKET_CARD_STORE_KEY,
    MARKET_POST_STATE_KEY,
};
use crate::sync_client;

mod handlers;
pub mod inbox_poller;
pub mod outbox;
#[cfg(test)]
mod tests;

pub use inbox_poller::{
    confirm_pending_post_payment_impl, dismiss_pending_post_item_impl, get_post_inbox_summary_impl,
    mark_inquiry_read_impl, sync_post_channels_impl,
};
#[cfg(test)]
pub use outbox::send_wot_handshake_via_post_impl;
pub use outbox::{
    accept_wot_handshake_impl, deposit_transfer_to_post_impl,
    reply_detached_signature_via_post_impl, send_market_inquiry_message_via_post_impl,
    send_market_inquiry_via_post_impl, send_signing_request_via_post_impl,
};
pub(crate) use outbox::{
    deposit_transfer_to_post_with_session, flush_post_slots_impl,
    send_wot_handshake_via_post_with_session,
};

pub(crate) const MAX_PRUNED_OUTBOX_TAG16S: usize = 512;
pub(crate) const MAX_POST_FUTURE_SKEW_SECS: u64 = 300;
pub(crate) const POST_DEFERRED_RETRY_WINDOW_SECS: u64 = 86_400;
/// Retain processed message IDs slightly longer than `POST_TTL_SECS` (30d vs 21d) to prevent replay after eviction.
pub(crate) const PROCESSED_MSG_RETENTION_SECS: u64 = humoco_market::POST_TTL_SECS + 9 * 86_400;

/// Prunes expired `processed_msg_ids` entries older than `PROCESSED_MSG_RETENTION_SECS`.
pub(crate) fn prune_expired_processed_msg_ids(
    map: &mut std::collections::BTreeMap<String, u64>,
    now_secs: u64,
) {
    map.retain(|_, ts| ts.saturating_add(PROCESSED_MSG_RETENTION_SECS) > now_secs);
}

/// Clamps an untrusted peer timestamp to at most `now_secs + MAX_POST_FUTURE_SKEW_SECS`.
pub(crate) fn clamp_post_ts(ts: u64, now_secs: u64) -> u64 {
    ts.min(now_secs.saturating_add(MAX_POST_FUTURE_SKEW_SECS))
}

/// Checks whether a peer envelope timestamp falls within bounded clock skew and the retry window.
pub(crate) fn is_within_retry_window(created_at: u64, now_secs: u64) -> bool {
    created_at <= now_secs.saturating_add(MAX_POST_FUTURE_SKEW_SECS)
        && now_secs.saturating_sub(created_at) <= POST_DEFERRED_RETRY_WINDOW_SECS
}

/// Builds the peer-scoped deduplication key `(canonical_peer_did, msg_id)` for processed incoming messages.
pub(crate) fn scoped_processed_msg_key(peer_did: &str, msg_id: &str) -> String {
    let can =
        crate::state::canonicalize_did(peer_did).unwrap_or_else(|_| peer_did.trim().to_string());
    format!("{}\x00{}", can, msg_id)
}

/// Canonical rate-limit key per inquiry target (`"ad:<id>"` or `"person"`).
pub(crate) fn inquiry_rate_limit_key(target: &crate::models::FrontendInquiryTarget) -> String {
    match target {
        crate::models::FrontendInquiryTarget::Ad { ad_id, .. } => format!("ad:{}", ad_id),
        crate::models::FrontendInquiryTarget::Person { .. } => "person".to_string(),
    }
}

/// Inserts a deleted/pruned outbox tag16 hex into the bounded tombstone set (`<= 512`).
pub(crate) fn push_pruned_outbox_tag16(
    set: &mut std::collections::BTreeSet<String>,
    tag16_hex: String,
) {
    if set.contains(&tag16_hex) {
        return;
    }
    while set.len() >= MAX_PRUNED_OUTBOX_TAG16S {
        if let Some(first) = set.iter().next().cloned() {
            set.remove(&first);
        } else {
            break;
        }
    }
    set.insert(tag16_hex);
}

/// Extracted identity and reply-sync configuration required to seal and queue Post messages.
pub(crate) struct PostSigningContext {
    pub identity: UserIdentity,
    pub sync_ctx: SyncIdentityContext,
    pub own_reply_syncs: Vec<FrontendSyncDescriptor>,
}

/// Loads the encrypted Post outbox/inbox state via an already-locked `AppService`, falling back to default only on `StorageError::NotFound`.
pub(crate) fn load_post_state_with_service(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<StoredPostState, FrontendError> {
    crate::state::load_encrypted_json_or_default(service, MARKET_POST_STATE_KEY, password)
        .map_err(FrontendError::from)
}

/// Persists `StoredPostState` and synchronizes the active profile cache.
pub(crate) fn save_post_state_with_service(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    post_state: &StoredPostState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, MARKET_POST_STATE_KEY, post_state, password)
        .map_err(FrontendError::from)?;
    state.write_cache_slot_for_service(service, |c| &mut c.post_state, Some(post_state.clone()));
    Ok(())
}

/// Reads `StoredPostState` for read-only views (using the session cache when `session_timeout_seconds == 0`).
pub(crate) fn load_post_state_read_only_for_session(
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
) -> Result<StoredPostState, FrontendError> {
    state.with_session_required(
        session,
        "Active wallet session changed before post state could be loaded",
        |service| {
            if password.is_none() && !service.is_session_active() {
                if let Some(cached) = state.read_cache_slot_for_service(service, |c| &c.post_state)
                {
                    return Ok(cached);
                }
            }
            let loaded = load_post_state_with_service(service, password)?;
            state.write_cache_slot_for_service(
                service,
                |c| &mut c.post_state,
                Some(loaded.clone()),
            );
            Ok(loaded)
        },
    )
}

/// Loads the encrypted Post outbox/inbox state only if `session` is still active.
pub(crate) fn load_post_state_for_session(
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
) -> Result<StoredPostState, FrontendError> {
    state.with_session_required(
        session,
        "Active wallet session changed before post state could be loaded",
        |service| {
            let loaded = load_post_state_with_service(service, password)?;
            state.write_cache_slot_for_service(
                service,
                |c| &mut c.post_state,
                Some(loaded.clone()),
            );
            Ok(loaded)
        },
    )
}

/// Loads the encrypted Post outbox/inbox state, falling back to default only on `StorageError::NotFound`.
#[cfg(test)]
pub(crate) fn load_post_state(
    state: &AppState,
    password: Option<&str>,
) -> Result<StoredPostState, FrontendError> {
    let mut service = state.service.lock().unwrap();
    load_post_state_with_service(&mut service, password)
}

fn merge_vec_by_id<T: Clone, F: Fn(&T) -> String>(
    baseline: &[T],
    working: &mut Vec<T>,
    latest: &[T],
    id_fn: F,
) {
    let base_ids: std::collections::HashSet<String> = baseline.iter().map(&id_fn).collect();
    let latest_ids: std::collections::HashSet<String> = latest.iter().map(&id_fn).collect();

    // Respect concurrent removals of items that existed in the baseline snapshot.
    working.retain(|item| {
        let id = id_fn(item);
        !base_ids.contains(&id) || latest_ids.contains(&id)
    });

    let working_ids: std::collections::HashSet<String> = working.iter().map(&id_fn).collect();

    for item in latest {
        let id = id_fn(item);
        if !base_ids.contains(&id) && !working_ids.contains(&id) {
            working.push(item.clone());
        }
    }
}

fn merge_string_set_3way(
    baseline: &std::collections::BTreeSet<String>,
    working: &mut std::collections::BTreeSet<String>,
    latest: &std::collections::BTreeSet<String>,
) {
    for k in baseline {
        if !latest.contains(k) {
            working.remove(k);
        }
    }
    for k in latest {
        if !baseline.contains(k) {
            working.insert(k.clone());
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
            let working_added_msgs =
                w_item.messages.len() > base_opt.map_or(0, |b| b.messages.len());
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
            if base_opt.is_none_or(|b| b.amount_hint == w_item.amount_hint)
                && base_opt.is_none_or(|b| b.amount_hint != latest_item.amount_hint)
            {
                w_item.amount_hint = latest_item.amount_hint.clone();
            }
            if base_opt.is_none_or(|b| b.standard_uuid == w_item.standard_uuid)
                && base_opt.is_none_or(|b| b.standard_uuid != latest_item.standard_uuid)
            {
                w_item.standard_uuid = latest_item.standard_uuid.clone();
            }
            if base_opt.is_none_or(|b| b.reply_syncs == w_item.reply_syncs)
                && base_opt.is_none_or(|b| b.reply_syncs != latest_item.reply_syncs)
            {
                w_item.reply_syncs = latest_item.reply_syncs.clone();
            }
            if (added_msg && latest_item.unread) || (working_added_msgs && w_item.unread) {
                w_item.unread = true;
            } else if base_opt.is_some_and(|b| b.unread == w_item.unread) {
                w_item.unread = latest_item.unread;
            }
        } else if !base_ids.contains(latest_item.inquiry_id.as_str()) {
            working.push(latest_item.clone());
        }
    }
}

/// Loads the latest on-disk `StoredPostState` and 3-way merges any concurrent
/// modifications relative to `baseline` into `working` without persisting yet.
pub(crate) fn merge_post_state_with_service(
    service: &mut human_money_core::app_service::AppService,
    baseline: &StoredPostState,
    working: &mut StoredPostState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let latest: StoredPostState =
        crate::state::load_encrypted_json_or_default(service, MARKET_POST_STATE_KEY, password)
            .map_err(FrontendError::from)?;

    if &latest != baseline {
        merge_vec_by_id(
            &baseline.outbox_entries,
            &mut working.outbox_entries,
            &latest.outbox_entries,
            |e| e.tag16_hex.clone(),
        );
        merge_btree_map_3way(
            &baseline.slot_etags_by_write_url,
            &mut working.slot_etags_by_write_url,
            &latest.slot_etags_by_write_url,
        );
        let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
        prune_expired_processed_msg_ids(&mut working.processed_msg_ids, now_secs);
        merge_btree_map_3way(
            &baseline.processed_msg_ids,
            &mut working.processed_msg_ids,
            &latest.processed_msg_ids,
        );
        prune_expired_processed_msg_ids(&mut working.processed_msg_ids, now_secs);
        merge_vec_by_id(
            &baseline.pending_sign_requests,
            &mut working.pending_sign_requests,
            &latest.pending_sign_requests,
            |r| scoped_processed_msg_key(&r.sender_did, &r.msg_id),
        );
        merge_vec_by_id(
            &baseline.pending_wot_handshakes,
            &mut working.pending_wot_handshakes,
            &latest.pending_wot_handshakes,
            |h| scoped_processed_msg_key(&h.sender_did, &h.msg_id),
        );
        merge_btree_map_3way(
            &baseline.pending_wot_packages_by_msg_id,
            &mut working.pending_wot_packages_by_msg_id,
            &latest.pending_wot_packages_by_msg_id,
        );
        merge_inquiries_3way(
            &baseline.inquiries,
            &mut working.inquiries,
            &latest.inquiries,
        );
        merge_vec_by_id(
            &baseline.unsupported_messages,
            &mut working.unsupported_messages,
            &latest.unsupported_messages,
            |u| scoped_processed_msg_key(&u.sender_did, &u.msg_id),
        );
        merge_vec_by_id(
            &baseline.pending_tolerance_payments,
            &mut working.pending_tolerance_payments,
            &latest.pending_tolerance_payments,
            |p| scoped_processed_msg_key(&p.sender_did, &p.msg_id),
        );
        {
            let base_map = &baseline.inquiry_rate_tracker.inquiries;
            let latest_map = &latest.inquiry_rate_tracker.inquiries;
            let mut keys: std::collections::HashSet<String> = base_map.keys().cloned().collect();
            keys.extend(working.inquiry_rate_tracker.inquiries.keys().cloned());
            keys.extend(latest_map.keys().cloned());
            let empty = std::collections::BTreeSet::new();
            for k in keys {
                let b_set = base_map.get(&k).unwrap_or(&empty);
                let l_set = latest_map.get(&k).unwrap_or(&empty);
                let w_set = working
                    .inquiry_rate_tracker
                    .inquiries
                    .entry(k.clone())
                    .or_default();
                merge_string_set_3way(b_set, w_set, l_set);
                if w_set.is_empty() {
                    working.inquiry_rate_tracker.inquiries.remove(&k);
                }
            }
        }
        working.outbox_rotation_tick = working
            .outbox_rotation_tick
            .max(latest.outbox_rotation_tick);
        merge_string_set_3way(
            &baseline.replied_sign_request_ids,
            &mut working.replied_sign_request_ids,
            &latest.replied_sign_request_ids,
        );
        merge_string_set_3way(
            &baseline.pruned_outbox_tag16s,
            &mut working.pruned_outbox_tag16s,
            &latest.pruned_outbox_tag16s,
        );
        while working.pruned_outbox_tag16s.len() > MAX_PRUNED_OUTBOX_TAG16S {
            if let Some(first) = working.pruned_outbox_tag16s.iter().next().cloned() {
                working.pruned_outbox_tag16s.remove(&first);
            } else {
                break;
            }
        }
    }
    Ok(())
}

/// Atomically loads the latest on-disk `StoredPostState` for `session`, 3-way merges any
/// concurrent modifications relative to `baseline` into `working`, and persists `working`.
pub(crate) fn merge_and_save_post_state(
    state: &AppState,
    session: &SessionToken,
    baseline: &StoredPostState,
    working: &mut StoredPostState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    state.with_session_required(
        session,
        "Active wallet session changed before post state could be saved",
        |service| {
            merge_post_state_with_service(service, baseline, working, password)?;
            save_post_state_with_service(state, service, working, password)
        },
    )
}

/// Persists the encrypted Post outbox/inbox state directly (test helper).
#[cfg(test)]
pub(crate) fn save_post_state(
    state: &AppState,
    post_state: &StoredPostState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();
    save_post_state_with_service(state, &mut service, post_state, password)
}

/// Collects all readable Sync endpoints for our own identity so peers know where to send replies.
pub(crate) fn collect_own_reply_syncs(
    security_policy: &SecurityPolicy,
    my_prefix: Option<&str>,
) -> Vec<FrontendSyncDescriptor> {
    let mut reply_syncs = Vec::new();
    for d in &security_policy.market_selected_syncs {
        if !is_endpoint_revoked(&d.status) && !d.read_url.is_empty() {
            reply_syncs.push(d.clone());
        }
    }
    for pd in &security_policy.public_syncs {
        if pd.enabled
            && !is_endpoint_revoked(&pd.status)
            && !pd.read_url.is_empty()
            && pd.authorized_channels.iter().any(|c| c == "post")
        {
            let Ok(clean_access_pk) = canonical_drop_pub_hex(&pd.access_pubkey) else {
                continue;
            };
            let Ok(slot_id) = humoco_market::format_slot_id(&clean_access_pk, my_prefix) else {
                continue;
            };
            let Ok(clean_drop_pub) = canonical_drop_pub_hex(&pd.drop_pub) else {
                continue;
            };
            let write_url = pd.write_url.trim().to_string();
            let read_url = pd.read_url.trim().to_string();
            if sync_client::validate_sync_url(&write_url).is_err()
                || sync_client::validate_sync_read_url(&read_url).is_err()
            {
                continue;
            }
            reply_syncs.push(FrontendSyncDescriptor {
                write_url,
                read_url,
                slot_id: Some(slot_id),
                drop_pub: clean_drop_pub,
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

/// Collects all own writable Post Sync drop targets from user security policy.
pub(crate) fn collect_post_upload_targets(
    security_policy: &SecurityPolicy,
) -> Vec<humoco_market::DropDescriptor> {
    let mut targets = Vec::new();
    for d in &security_policy.market_selected_syncs {
        if !is_endpoint_revoked(&d.status) && !d.write_url.is_empty() {
            targets.push(d.clone().into());
        }
    }
    for pd in &security_policy.public_syncs {
        if pd.enabled
            && !is_endpoint_revoked(&pd.status)
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

/// Loads identity, signing context, and own reply sync descriptors under `service -> security_policy`.
pub(crate) fn load_post_signing_context_for_service(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<PostSigningContext, FrontendError> {
    let identity = service
        .with_wallet_and_identity(|_w, id| id.clone())
        .map_err(FrontendError::from)?;
    let sync_ctx = SyncIdentityContext::from_service(service, password)?;
    let security_policy = state
        .get_or_load_security_policy(service, password)
        .map_err(FrontendError::from)?;
    let own_reply_syncs = collect_own_reply_syncs(&security_policy, sync_ctx.my_prefix.as_deref());
    Ok(PostSigningContext {
        identity,
        sync_ctx,
        own_reply_syncs,
    })
}

/// Loads identity, signing context, and own reply sync descriptors only if `session` is still active.
pub(crate) fn load_post_signing_context_for_session(
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
) -> Result<PostSigningContext, FrontendError> {
    state.with_session_required(
        session,
        "Active wallet session changed before post signing context could be loaded",
        |service| load_post_signing_context_for_service(state, service, password),
    )
}

/// Loads identity, signing context, and own reply sync descriptors (test helper).
#[cfg(test)]
pub(crate) fn load_post_signing_context(
    state: &AppState,
    password: Option<&str>,
) -> Result<PostSigningContext, FrontendError> {
    let mut service = state.service.lock().unwrap();
    load_post_signing_context_for_service(state, &mut service, password)
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
    card_store.cards.get(did.trim()).or_else(|| {
        card_store
            .cards
            .values()
            .find(|sc| are_dids_equal(&sc.card.payload.did, did))
    })
}

/// Batch counterparty resolver loaded once per session call to avoid N+1 disk decryption.
pub(crate) struct CounterpartyInfoResolver {
    contacts: Vec<crate::models::FrontendContact>,
    card_store: Option<humoco_market::CardStore>,
}

impl CounterpartyInfoResolver {
    pub(crate) fn load_for_session_read_only(
        state: &AppState,
        session: &SessionToken,
        password: Option<&str>,
    ) -> Result<Self, FrontendError> {
        state.with_session_required(
            session,
            "Active wallet session changed before counterparty metadata could be loaded",
            |service| {
                let can_decrypt_disk = password.is_some() || service.is_session_active();
                let contacts = if can_decrypt_disk {
                    state.load_contacts(service, password).unwrap_or_else(|e| {
                        warn!("Failed to load contacts in CounterpartyInfoResolver: {}", e);
                        Vec::new()
                    })
                } else if let Some(ab) = state.read_cache_slot_for_service(service, |c| &c.contacts)
                {
                    ab.contacts.into_values().collect()
                } else {
                    Vec::new()
                };

                let card_store = if can_decrypt_disk {
                    match crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
                        service,
                        MARKET_CARD_STORE_KEY,
                        password,
                    ) {
                        Ok(opt) => opt,
                        Err(e) => {
                            warn!(
                                "Failed to load CardStore in CounterpartyInfoResolver: {}",
                                e
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                Ok(Self {
                    contacts,
                    card_store,
                })
            },
        )
    }

    pub(crate) fn resolve(&self, did: &str) -> (Option<String>, Option<String>, bool) {
        let mut name: Option<String> = None;
        let mut picture_url: Option<String> = None;
        let mut is_contact = false;
        let trimmed = did.trim();

        if let Some(c) = self
            .contacts
            .iter()
            .find(|c| c.did.trim() == trimmed)
            .or_else(|| self.contacts.iter().find(|c| are_dids_equal(&c.did, did)))
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

        if !is_contact || name.is_none() || picture_url.is_none() {
            if let Some(ref card_store) = self.card_store {
                if let Some(stored) = find_card_in_store(card_store, trimmed) {
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

        (name, picture_url, is_contact)
    }
}

/// Resolves `(display_name, picture_url, is_contact)` for a DID across the
/// address book and local marketplace card store using an already-locked `AppService`.
pub(crate) fn resolve_counterparty_info_with_service(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    did: &str,
    password: Option<&str>,
) -> (Option<String>, Option<String>, bool) {
    let mut name: Option<String> = None;
    let mut picture_url: Option<String> = None;
    let mut is_contact = false;
    let trimmed = did.trim();

    if let Ok(contacts) = state.load_contacts(service, password) {
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
        match crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
            service,
            MARKET_CARD_STORE_KEY,
            password,
        ) {
            Ok(Some(card_store)) => {
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
            Ok(None) => {}
            Err(e) => {
                log::warn!(
                    "Failed to load CardStore in resolve_counterparty_info: {}",
                    e
                );
            }
        }
    }

    (name, picture_url, is_contact)
}

/// Resolves `(display_name, picture_url, is_contact)` for a DID pinned to `session`.
pub(crate) fn resolve_counterparty_info_for_session(
    state: &AppState,
    session: &SessionToken,
    did: &str,
    password: Option<&str>,
) -> (Option<String>, Option<String>, bool) {
    state
        .with_session(session, |service| {
            Ok(resolve_counterparty_info_with_service(
                state, service, did, password,
            ))
        })
        .ok()
        .flatten()
        .unwrap_or((None, None, false))
}

/// Looks up Sync endpoints for a counterparty across contacts and the local card store.
pub(crate) fn lookup_counterparty_syncs(
    state: &AppState,
    session: &SessionToken,
    did: &str,
    password: Option<&str>,
) -> Vec<FrontendSyncDescriptor> {
    state
        .with_session(session, |service| {
            let mut result = Vec::new();
            if let Ok(contacts) = state.load_contacts(service, password) {
                let trimmed = did.trim();
                if let Some(c) = contacts
                    .iter()
                    .find(|c| c.did.trim() == trimmed)
                    .or_else(|| contacts.iter().find(|c| are_dids_equal(&c.did, trimmed)))
                {
                    result.extend(
                        c.syncs
                            .iter()
                            .filter(|d| !is_endpoint_revoked(&d.status))
                            .cloned(),
                    );
                }
            }
            if result.is_empty() {
                match crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
                    service,
                    MARKET_CARD_STORE_KEY,
                    password,
                ) {
                    Ok(Some(card_store)) => {
                        if let Some(stored) = find_card_in_store(&card_store, did) {
                            result.extend(sanitize_untrusted_sync_descriptors(
                                &stored.card.payload.drops,
                                humoco_market::MAX_CARD_DROPS,
                            ));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::warn!(
                            "Failed to load CardStore in lookup_counterparty_syncs: {}",
                            e
                        );
                    }
                }
            }
            Ok(result)
        })
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Checks whether our local marketplace profile/ad currently allows the given inquiry target
/// and returns `(canonical_target, ad_exists_in_store)`.
pub(crate) fn check_inquiry_target(
    state: &AppState,
    session: &SessionToken,
    target: &humoco_market::InquiryTarget,
    now_secs: u64,
    password: Option<&str>,
) -> (Option<humoco_market::InquiryTarget>, bool) {
    state
        .with_session(session, |service| {
            if let Ok(stored) = crate::services::market_service::load_own_market_profile_state(
                state, service, password,
            ) {
                match target {
                    humoco_market::InquiryTarget::Ad { ad_id, .. } => {
                        if let Some(ad) = stored.ads.iter().find(|a| &a.id == ad_id) {
                            if stored.published && ad.expires_at > now_secs && ad.allow_inquiries {
                                return Ok((
                                    Some(humoco_market::InquiryTarget::Ad {
                                        ad_id: ad.id.clone(),
                                        ad_title: ad.title.clone(),
                                    }),
                                    true,
                                ));
                            }
                            return Ok((None, true));
                        }
                        return Ok((None, false));
                    }
                    humoco_market::InquiryTarget::Person { skill_title } => {
                        if stored.published && stored.allow_inquiries {
                            return Ok((
                                Some(humoco_market::InquiryTarget::Person {
                                    skill_title: skill_title.clone(),
                                }),
                                false,
                            ));
                        }
                        return Ok((None, false));
                    }
                }
            }
            Ok((None, false))
        })
        .ok()
        .flatten()
        .unwrap_or((None, false))
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
/// into `post_state.outbox_entries`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enqueue_post_envelope(
    post_state: &mut StoredPostState,
    sender_did: &str,
    my_ed_sk: &SigningKey,
    recipient_did: &str,
    recipient_ed_pk: &VerifyingKey,
    msg_id: String,
    bundle_id: Option<String>,
    message_class: crate::state::OutboxMessageClass,
    own_reply_syncs: &[FrontendSyncDescriptor],
    container_bytes: Vec<u8>,
    now_secs: u64,
) -> Result<(), FrontendError> {
    let envelope = PostEnvelope::new(
        msg_id.clone(),
        sender_did,
        now_secs,
        own_reply_syncs.iter().cloned().map(Into::into).collect(),
        container_bytes,
    );
    let entry = seal_post_box(my_ed_sk, recipient_ed_pk, &envelope).map_err(FrontendError::from)?;
    post_state.outbox_entries.push(PostOutboxEntry {
        msg_id,
        recipient_did: recipient_did.to_string(),
        tag16_hex: hex::encode(entry.tag16),
        bundle_id,
        message_class,
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
    let Ok(validated_recipient) = ValidatedUserId::parse(recipient_did) else {
        return;
    };
    let recipient_did = validated_recipient.as_str();
    let recipient_pk = validated_recipient.public_key();
    let Ok(container_bytes) =
        seal_post_container_bytes(my_identity, recipient_did, payload_type, payload, label)
    else {
        return;
    };
    let _ = enqueue_post_envelope(
        post_state,
        &my_identity.user_id,
        my_ed_sk,
        recipient_did,
        recipient_pk,
        msg_id,
        None,
        crate::state::OutboxMessageClass::Control,
        own_reply_syncs,
        container_bytes,
        now_secs,
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
    let receipt_msg_id = format!("ack-{}", msg_id);
    if post_state
        .outbox_entries
        .iter()
        .any(|e| e.msg_id == receipt_msg_id && are_dids_equal(&e.recipient_did, sender_did))
    {
        return;
    }
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
    let validated_sender = match ValidatedUserId::parse(sender_did) {
        Ok(v) => v,
        Err(e) => {
            warn!("Invalid recipient DID for delivery receipt: {}", e);
            return;
        }
    };
    let sender_did = validated_sender.as_str();
    let recipient_pk = validated_sender.public_key();
    if let Err(e) = enqueue_post_envelope(
        post_state,
        &my_identity.user_id,
        my_ed_sk,
        sender_did,
        recipient_pk,
        receipt_msg_id,
        None,
        crate::state::OutboxMessageClass::Control,
        own_reply_syncs,
        container_bytes,
        now_secs,
    ) {
        warn!(
            "Failed to seal post box for delivery receipt: {}",
            e.message
        );
    }
}
