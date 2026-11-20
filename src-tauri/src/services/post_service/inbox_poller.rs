use std::collections::HashMap;

use ed25519_dalek::VerifyingKey;
use human_money_core::crypto::get_pubkey_from_user_id;
use human_money_core::models::profile::UserIdentity;
use human_money_core::models::secure_container::{PayloadType, SecureContainer};
use humoco_market::{
    is_own_dummy_post_box, try_open_post_box_any_sender, PAYLOAD_MARKET_INQUIRY_MESSAGE_V1,
    PAYLOAD_MARKET_INQUIRY_V1, PAYLOAD_POST_RECEIPT_V1, POST_SLOT_MAGIC, POST_TTL_SECS,
};
use log::warn;

use crate::models::{
    FrontendAppNotice, FrontendError, FrontendPostInboxSummary, FrontendPostSyncResult,
    FrontendSyncDescriptor,
};
use crate::settings::{SecurityPolicy, UiPreferences};
use crate::state::{AppState, StoredPostState, MARKET_CARD_STORE_KEY};
use crate::sync_client;

use super::handlers::{
    dispatch_post_payload, PostDispatchOutcome, PostDispatchStats, PostEnvelopeContext,
};
use super::{
    load_post_state, merge_and_save_post_state,
    resolve_counterparty_info, resolve_counterparty_info_for_session,
};
use crate::services::post_service::outbox::flush_post_slots_impl;

/// Marks a marketplace inquiry thread as read atomically under `state.service`.
pub fn mark_inquiry_read_impl(
    inquiry_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut post_state: StoredPostState = crate::state::load_encrypted_json_or_default(
        &mut service,
        crate::state::MARKET_POST_STATE_KEY,
        pw,
    )
    .map_err(FrontendError::from)?;
    let mut changed = false;
    for inq in &mut post_state.inquiries {
        if inq.inquiry_id == inquiry_id && inq.unread {
            inq.unread = false;
            changed = true;
        }
    }
    if changed {
        crate::state::save_encrypted_json(
            &mut service,
            crate::state::MARKET_POST_STATE_KEY,
            &post_state,
            pw,
        )
        .map_err(FrontendError::from)?;
    }
    Ok(())
}

/// Dismisses a pending sign request, WoT handshake, inquiry, or unsupported notice atomically under `state.service`.
pub fn dismiss_pending_post_item_impl(
    item_kind: String,
    id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut post_state: StoredPostState = crate::state::load_encrypted_json_or_default(
        &mut service,
        crate::state::MARKET_POST_STATE_KEY,
        pw,
    )
    .map_err(FrontendError::from)?;
    match item_kind.as_str() {
        "sign_request" => {
            post_state.pending_sign_requests.retain(|r| r.msg_id != id);
        }
        "wot_handshake" => {
            post_state.pending_wot_handshakes.retain(|h| h.msg_id != id);
            post_state.pending_wot_packages_by_msg_id.remove(&id);
        }
        "inquiry" => {
            post_state.inquiries.retain(|i| i.inquiry_id != id);
        }
        "unsupported" => {
            post_state.unsupported_messages.retain(|u| u.msg_id != id);
        }
        other => {
            return Err(FrontendError::from(format!(
                "Unknown pending post item kind: {}",
                other
            )));
        }
    }
    crate::state::save_encrypted_json(
        &mut service,
        crate::state::MARKET_POST_STATE_KEY,
        &post_state,
        pw,
    )
    .map_err(FrontendError::from)
}

/// Returns the current Post inbox summary enriched with counterparty contact metadata.
pub fn get_post_inbox_summary_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostInboxSummary, FrontendError> {
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    let mut post_state = load_post_state(state, password.as_deref())?;
    let unacknowledged_payments = post_state
        .outbox_entries
        .iter()
        .filter_map(|e| e.bundle_id.clone())
        .collect();

    for inq in &mut post_state.inquiries {
        let (name, pic, is_contact) =
            resolve_counterparty_info(state, &inq.counterparty_did, password.as_deref());
        inq.counterparty_name = name;
        inq.counterparty_picture_url = pic;
        inq.is_contact = is_contact;
    }
    post_state
        .inquiries
        .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    for item in &mut post_state.unsupported_messages {
        if item.sender_name.is_none() {
            let (name, _, _) =
                resolve_counterparty_info(state, &item.sender_did, password.as_deref());
            item.sender_name = name;
        }
    }

    Ok(FrontendPostInboxSummary {
        pending_sign_requests: post_state.pending_sign_requests,
        pending_wot_handshakes: post_state.pending_wot_handshakes,
        inquiries: post_state.inquiries,
        unacknowledged_payments,
        unsupported_messages: post_state.unsupported_messages,
    })
}

fn push_sync_slot_targets(
    direct: &mut Vec<(String, String, Option<String>)>,
    index_urls: &mut Vec<String>,
    descriptors: &[FrontendSyncDescriptor],
    prefix: Option<&str>,
) {
    for d in descriptors {
        if !d.read_url.is_empty() {
            if let Some(ref sid) = d.slot_id {
                direct.push((d.read_url.clone(), sid.clone(), prefix.map(str::to_string)));
            } else {
                index_urls.push(d.read_url.clone());
            }
        }
    }
}

type DirectSlotTarget = (String, String, Option<String>);
type PollSlotTargets = (Vec<DirectSlotTarget>, Vec<String>);
type AuthorizedSenderKey = (String, VerifyingKey);
struct AuthorizedSenders {
    full: Vec<AuthorizedSenderKey>,
    card_signers: Vec<AuthorizedSenderKey>,
    inquiry: Vec<AuthorizedSenderKey>,
}

fn collect_candidate_slot_sources(
    state: &AppState,
    session: &crate::session::SessionToken,
    security_policy: &SecurityPolicy,
    my_did: &str,
    my_prefix: Option<&str>,
    post_state: &StoredPostState,
    password: Option<&str>,
) -> Result<PollSlotTargets, FrontendError> {
    state.with_session_required(
        session,
        "Active wallet session changed while collecting post slot sources",
        |service| {
            let mut direct: Vec<(String, String, Option<String>)> = Vec::new();
            let mut index_urls = Vec::new();

            push_sync_slot_targets(
                &mut direct,
                &mut index_urls,
                &security_policy.market_selected_syncs,
                my_prefix,
            );

            for pd in &security_policy.public_syncs {
                if pd.enabled && !pd.read_url.is_empty() {
                    direct.push((
                        pd.read_url.clone(),
                        pd.access_pubkey.clone(),
                        my_prefix.map(str::to_string),
                    ));
                }
            }

            let own_can_did = crate::state::canonicalize_did(my_did).ok();
            if let Ok(contacts) = state.load_contacts(service, password) {
                for c in contacts {
                    let c_prefix = human_money_core::crypto::get_prefix_from_user_id(&c.did);
                    if own_can_did.is_some()
                        && crate::state::canonicalize_did(&c.did).ok() == own_can_did
                    {
                        // Also poll sibling own-subaccount compartments on configured O(1) public syncs.
                        for pd in &security_policy.public_syncs {
                            if pd.enabled && !pd.read_url.is_empty() {
                                direct.push((
                                    pd.read_url.clone(),
                                    pd.access_pubkey.clone(),
                                    c_prefix.map(str::to_string),
                                ));
                            }
                        }
                    }
                    push_sync_slot_targets(&mut direct, &mut index_urls, &c.syncs, c_prefix);
                }
            }

            match crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
                service,
                MARKET_CARD_STORE_KEY,
                password,
            ) {
                Ok(Some(store)) => {
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
                Ok(None) => {}
                Err(e) => {
                    log::warn!(
                        "Failed to load CardStore in collect_post_poll_targets: {}",
                        e
                    );
                }
            }

            for req in &post_state.pending_sign_requests {
                push_sync_slot_targets(&mut direct, &mut index_urls, &req.reply_syncs, None);
            }
            for inq in &post_state.inquiries {
                push_sync_slot_targets(&mut direct, &mut index_urls, &inq.reply_syncs, None);
            }

            direct.sort();
            direct.dedup();
            index_urls.sort();
            index_urls.dedup();

            Ok((direct, index_urls))
        },
    )
}

fn collect_authorized_post_senders(
    state: &AppState,
    session: &crate::session::SessionToken,
    ui_prefs: &UiPreferences,
    post_state: &StoredPostState,
    now_secs: u64,
    password: Option<&str>,
) -> Result<AuthorizedSenders, FrontendError> {
    state.with_session_required(
        session,
        "Active wallet session changed while collecting authorized post senders",
        |service| {
            let mut auth = Vec::new();
            let mut card_signers = Vec::new();
            let mut inquiry = Vec::new();

            if ui_prefs.post_accept_from_contacts {
                if let Ok(contacts) = state.load_contacts(service, password) {
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

            for item in &post_state.inquiries {
                if let Ok(vk) = get_pubkey_from_user_id(&item.counterparty_did) {
                    auth.push((item.counterparty_did.clone(), vk));
                }
            }

            let has_inquiry_listing =
                crate::services::market_service::load_own_market_profile_state(
                    state,
                    service,
                    password,
                )
                .ok()
                .map(|stored| {
                    stored.published
                        && (stored.allow_inquiries
                            || stored
                                .ads
                                .iter()
                                .any(|a| a.expires_at > now_secs && a.allow_inquiries))
                })
                .unwrap_or(false);

            match crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
                service,
                MARKET_CARD_STORE_KEY,
                password,
            ) {
                Ok(Some(cs)) => {
                    for sc in cs.cards.values() {
                        if let Ok(vk) = get_pubkey_from_user_id(&sc.card.payload.did) {
                            if ui_prefs.post_accept_from_contacts && sc.hops <= 2 {
                                card_signers.push((sc.card.payload.did.clone(), vk));
                            }
                            if has_inquiry_listing {
                                inquiry.push((sc.card.payload.did.clone(), vk));
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    log::warn!(
                        "Failed to load CardStore in collect_authorized_post_senders: {}",
                        e
                    );
                }
            }

            auth.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
            auth.dedup_by(|a, b| a.1.as_bytes() == b.1.as_bytes());

            card_signers.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
            card_signers.dedup_by(|a, b| a.1.as_bytes() == b.1.as_bytes());

            inquiry.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
            inquiry.dedup_by(|a, b| a.1.as_bytes() == b.1.as_bytes());

            Ok(AuthorizedSenders {
                full: auth,
                card_signers,
                inquiry,
            })
        },
    )
}

async fn download_post_slots(
    client: &reqwest::Client,
    direct_slot_fetches: Vec<(String, String, Option<String>)>,
    candidate_read_urls: Vec<String>,
) -> (Vec<Vec<u8>>, Vec<(String, bool)>, usize, Vec<String>) {
    let mut fetched_slots = Vec::new();
    let mut health_updates = Vec::new();
    let mut polled_syncs = 0;
    let mut errors = Vec::new();

    for (read_url, slot_id, prefix_opt) in direct_slot_fetches {
        polled_syncs += 1;
        let (base_slot_pk, slot_prefix) =
            humoco_market::parse_slot_id(&slot_id).unwrap_or_else(|_| (slot_id.clone(), None));
        let post_prefix = slot_prefix.as_deref().or(prefix_opt.as_deref());
        match sync_client::get_slot(client, &read_url, &base_slot_pk, post_prefix, "post", None)
            .await
        {
            Ok(sync_client::SlotFetchResult::Modified { bytes, .. }) => {
                health_updates.push((read_url.clone(), true));
                fetched_slots.push(bytes);
            }
            Ok(
                sync_client::SlotFetchResult::NotFound | sync_client::SlotFetchResult::NotModified,
            ) => {
                health_updates.push((read_url.clone(), true));
            }
            Err(e) => {
                health_updates.push((read_url.clone(), false));
                errors.push(format!(
                    "Failed to fetch direct post slot from {}: {}",
                    read_url, e
                ));
            }
        }
    }

    for read_url in candidate_read_urls {
        polled_syncs += 1;
        match sync_client::list_index(client, &read_url, None, None).await {
            Ok(sync_client::IndexFetchResult::Modified { entries, .. }) => {
                health_updates.push((read_url.clone(), true));
                for entry in entries {
                    if entry.channel != "post" {
                        continue;
                    }
                    match sync_client::get_slot(
                        client,
                        &read_url,
                        &entry.id,
                        entry.prefix.as_deref(),
                        "post",
                        None,
                    )
                    .await
                    {
                        Ok(sync_client::SlotFetchResult::Modified { bytes, .. }) => {
                            fetched_slots.push(bytes)
                        }
                        Ok(
                            sync_client::SlotFetchResult::NotFound
                            | sync_client::SlotFetchResult::NotModified,
                        ) => {}
                        Err(e) => errors.push(format!(
                            "Failed to fetch post slot {} from {}: {}",
                            entry.id, read_url, e
                        )),
                    }
                }
            }
            Ok(sync_client::IndexFetchResult::NotModified) => {
                health_updates.push((read_url.clone(), true));
            }
            Err(e) => {
                health_updates.push((read_url.clone(), false));
                errors.push(format!(
                    "Failed to fetch post slot index from {}: {}",
                    read_url, e
                ));
            }
        }
    }

    (fetched_slots, health_updates, polled_syncs, errors)
}

fn open_verified_container(
    container: &SecureContainer,
    my_identity: &UserIdentity,
    sender_did: &str,
    errors: &mut Vec<String>,
) -> Option<Vec<u8>> {
    if let Err(e) = container.verify_integrity() {
        errors.push(format!("Integrity check failed: {}", e));
        return None;
    }

    if !container.signature.is_empty() {
        if let Err(e) = container.verify_container_signature(sender_did) {
            errors.push(format!("Container signature check failed: {}", e));
            return None;
        }
    }

    match container.open(my_identity, None) {
        Ok(b) => Some(b),
        Err(e) => {
            let allow_canonical_fallback = my_identity.user_id.contains('@')
                && !matches!(container.c, PayloadType::TransactionBundle)
                && container.c != PayloadType::Generic(PAYLOAD_POST_RECEIPT_V1.to_string());
            let alt_attempt = if allow_canonical_fallback {
                crate::state::canonicalize_did(&my_identity.user_id)
                    .ok()
                    .and_then(|can| {
                        let mut alt = my_identity.clone();
                        alt.user_id = can;
                        container.open(&alt, None).ok()
                    })
            } else {
                None
            };
            if alt_attempt.is_some() {
                return alt_attempt;
            }
            if !matches!(
                e,
                human_money_core::VoucherCoreError::NotAnIntendedRecipient
            ) {
                // Suppress NotAnIntendedRecipient: outer PostBox shared our X25519 key
                // (sibling subaccount/root profile), while inner SecureContainer targets another prefix.
                errors.push(format!("Failed to open container payload: {}", e));
            }
            None
        }
    }
}

fn build_post_notices(stats: &PostDispatchStats, now_secs: u64) -> Vec<FrontendAppNotice> {
    let mut post_notices = Vec::new();
    if stats.imported_payments > 0 {
        post_notices.push(FrontendAppNotice {
            id: uuid::Uuid::new_v4().to_string(),
            kind: "payment".to_string(),
            created_at: now_secs,
            read: false,
            imported_payments: stats.imported_payments,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: 0,
            new_inquiry_messages: 0,
            imported_cards: 0,
            new_ads: 0,
            counterparty_name: None,
            inquiry_id: None,
        });
    }
    if stats.new_inquiries > 0 || stats.new_inquiry_messages > 0 {
        let (inq_id, cp_name) = if stats.new_inquiries + stats.new_inquiry_messages == 1 {
            match &stats.last_inquiry_notice_target {
                Some((id, name)) => (Some(id.clone()), name.clone()),
                None => (None, None),
            }
        } else {
            (None, None)
        };
        post_notices.push(FrontendAppNotice {
            id: uuid::Uuid::new_v4().to_string(),
            kind: "inquiry".to_string(),
            created_at: now_secs,
            read: false,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: stats.new_inquiries,
            new_inquiry_messages: stats.new_inquiry_messages,
            imported_cards: 0,
            new_ads: 0,
            counterparty_name: cp_name,
            inquiry_id: inq_id,
        });
    }
    if stats.new_wot_handshakes > 0 || stats.new_sign_requests > 0 || stats.imported_signatures > 0
    {
        post_notices.push(FrontendAppNotice {
            id: uuid::Uuid::new_v4().to_string(),
            kind: "trust".to_string(),
            created_at: now_secs,
            read: false,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: stats.imported_signatures,
            new_sign_requests: stats.new_sign_requests,
            new_wot_handshakes: stats.new_wot_handshakes,
            new_inquiries: 0,
            new_inquiry_messages: 0,
            imported_cards: 0,
            new_ads: 0,
            counterparty_name: None,
            inquiry_id: None,
        });
    }
    post_notices
}

/// Synchronizes Post channels: prunes expired outbox entries, polls peer and own Post slots,
/// decrypts and dispatches incoming payloads, flushes receipts/outbox, and records notices.
pub async fn sync_post_channels_impl(
    app: Option<&tauri::AppHandle>,
    is_wifi: Option<bool>,
    extra_standards: Option<&HashMap<String, String>>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostSyncResult, FrontendError> {
    let (session, ui_prefs, security_policy) = {
        let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
        let session = state.session_token_for_service(&service)?;
        let ui_prefs = state
            .get_or_load_ui_preferences(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let security_policy = state
            .get_or_load_security_policy(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        (session, ui_prefs, security_policy)
    };

    if ui_prefs.market_poll_wifi_only && is_wifi == Some(false) {
        return Ok(FrontendPostSyncResult {
            uploaded_to: 0,
            polled_syncs: 0,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: 0,
            new_inquiry_messages: 0,
            pruned_boxes: 0,
            deferred_outbox_boxes: 0,
            skipped_wifi_only: true,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }

    let _post_guard = state.post_op_lock.lock().await;
    let client = sync_client::create_sync_http_client();
    let (signing_ctx, mut post_state) = state.with_session_required(
        &session,
        "Active wallet session changed while preparing post sync",
        |service| {
            let ps = crate::services::post_service::load_post_state_with_service(
                service,
                password.as_deref(),
            )?;
            let my_identity = service
                .with_wallet_and_identity(|_w, id| id.clone())
                .map_err(FrontendError::from)?;
            let sync_ctx = crate::services::common::SyncIdentityContext::from_service(service)?;
            let own_reply_syncs = crate::services::post_service::collect_own_reply_syncs(
                &security_policy,
                sync_ctx.my_prefix.as_deref(),
            );
            Ok((
                super::PostSigningContext {
                    identity: my_identity,
                    sync_ctx,
                    own_reply_syncs,
                },
                ps,
            ))
        },
    )?;
    let mut baseline_post_state = post_state.clone();
    let now_secs = sync_client::current_unix_timestamp();

    let mut stats = PostDispatchStats::default();

    // 1. Pruning pass (§6.2)
    {
        let before_count = post_state.outbox_entries.len();
        post_state
            .outbox_entries
            .retain(|e| now_secs.saturating_sub(e.created_at) <= POST_TTL_SECS);
        stats.pruned_boxes += before_count - post_state.outbox_entries.len();
    }

    // 2. Build candidate direct fetches and index URLs
    let (direct_slot_fetches, candidate_read_urls) = collect_candidate_slot_sources(
        state,
        &session,
        &security_policy,
        &signing_ctx.identity.user_id,
        signing_ctx.sync_ctx.my_prefix.as_deref(),
        &post_state,
        password.as_deref(),
    )?;

    // 3. Build authorized_senders, card_signers, and inquiry_only_senders
    let authorized_groups = collect_authorized_post_senders(
        state,
        &session,
        &ui_prefs,
        &post_state,
        now_secs,
        password.as_deref(),
    )?;
    let authorized_senders = authorized_groups.full;
    let card_signer_senders = authorized_groups.card_signers;
    let inquiry_only_senders = authorized_groups.inquiry;

    // 4. Download slots (no mutex held across `.await`)
    let (fetched_slots, post_sync_health_updates, polled_syncs, download_errors) =
        download_post_slots(&client, direct_slot_fetches, candidate_read_urls).await;
    if !state.is_generation_current(session.generation) {
        return Ok(FrontendPostSyncResult {
            uploaded_to: 0,
            polled_syncs: 0,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: 0,
            new_inquiry_messages: 0,
            pruned_boxes: 0,
            deferred_outbox_boxes: 0,
            skipped_wifi_only: false,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }
    stats.errors.extend(download_errors);

    if !post_sync_health_updates.is_empty() {
        let committed = state.with_session(&session, |service| {
            state.record_contact_sync_health_batch(
                service,
                &post_sync_health_updates,
                now_secs,
                password.as_deref(),
            );
            Ok(())
        })?;
        if committed.is_none() {
            return Ok(FrontendPostSyncResult {
                uploaded_to: 0,
                polled_syncs: 0,
                imported_payments: 0,
                imported_receipts: 0,
                imported_signatures: 0,
                new_sign_requests: 0,
                new_wot_handshakes: 0,
                new_inquiries: 0,
                new_inquiry_messages: 0,
                pruned_boxes: 0,
                deferred_outbox_boxes: 0,
                skipped_wifi_only: false,
                errors: Vec::new(),
                retry_after_secs: None,
            });
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
                stats
                    .errors
                    .push(format!("Failed to decode post slot: {}", e));
                continue;
            }
        };

        for box_entry in entries {
            if is_own_dummy_post_box(&signing_ctx.sync_ctx.master_seed, &box_entry) {
                continue;
            }
            let tag16_hex = hex::encode(box_entry.tag16);
            if post_state
                .outbox_entries
                .iter()
                .any(|e| e.tag16_hex == tag16_hex)
            {
                continue;
            }

            let open_attempt = try_open_post_box_any_sender(
                &signing_ctx.sync_ctx.my_ed_sk,
                &authorized_senders,
                &box_entry,
            );
            enum SenderPrivilege {
                Full,
                CardSignerAndMaybeInquiry(bool),
                InquiryOnly,
            }
            let (matched_sender_did, envelope, privilege) = match open_attempt {
                Ok(Some((did, env))) => (did, env, SenderPrivilege::Full),
                Ok(None) => {
                    let card_attempt = if !card_signer_senders.is_empty() {
                        try_open_post_box_any_sender(
                            &signing_ctx.sync_ctx.my_ed_sk,
                            &card_signer_senders,
                            &box_entry,
                        )
                        .ok()
                        .flatten()
                    } else {
                        None
                    };
                    if let Some((did, env)) = card_attempt {
                        let also_inquiry = inquiry_only_senders.iter().any(|(d, _)| d == &did);
                        (
                            did,
                            env,
                            SenderPrivilege::CardSignerAndMaybeInquiry(also_inquiry),
                        )
                    } else if !inquiry_only_senders.is_empty() {
                        match try_open_post_box_any_sender(
                            &signing_ctx.sync_ctx.my_ed_sk,
                            &inquiry_only_senders,
                            &box_entry,
                        ) {
                            Ok(Some((did, env))) => (did, env, SenderPrivilege::InquiryOnly),
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

            let sender_did = {
                let Ok(parsed_sender) =
                    human_money_core::ValidatedUserId::parse(&envelope.sender_did)
                else {
                    warn!("Rejected PostEnvelope with invalid sender_did");
                    continue;
                };
                let Ok(matched_pk) = get_pubkey_from_user_id(&matched_sender_did) else {
                    continue;
                };
                if parsed_sender.public_key() != &matched_pk {
                    warn!("Rejected PostEnvelope with mismatched sender_did public key");
                    continue;
                }
                parsed_sender.into_string()
            };

            if post_state.processed_msg_ids.contains(&envelope.msg_id) {
                continue;
            }

            let container: SecureContainer = match serde_json::from_slice(&envelope.container_bytes)
            {
                Ok(c) => c,
                Err(e) => {
                    stats
                        .errors
                        .push(format!("Failed to parse SecureContainer: {}", e));
                    continue;
                }
            };

            let Some(decrypted_bytes) = open_verified_container(
                &container,
                &signing_ctx.identity,
                &sender_did,
                &mut stats.errors,
            ) else {
                continue;
            };

            let is_inquiry_payload = container.c
                == PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_V1.to_string())
                || container.c
                    == PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_MESSAGE_V1.to_string());
            let allowed_payload = match privilege {
                SenderPrivilege::Full => true,
                SenderPrivilege::CardSignerAndMaybeInquiry(also_inquiry) => {
                    container.c == PayloadType::VoucherForSigning
                        || (also_inquiry && is_inquiry_payload)
                }
                SenderPrivilege::InquiryOnly => is_inquiry_payload,
            };
            if !allowed_payload {
                continue;
            }

            if !envelope.reply_drops.is_empty() {
                let reply_dtos: Vec<FrontendSyncDescriptor> = envelope
                    .reply_drops
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect();
                let committed = state.with_session(&session, |service| {
                    state.update_contact_from_card(
                        service,
                        &sender_did,
                        &reply_dtos,
                        None,
                        password.as_deref(),
                    );
                    Ok(())
                })?;
                if committed.is_none() {
                    break;
                }
            }

            let env_ctx = PostEnvelopeContext {
                state,
                session: &session,
                app,
                extra_standards,
                password: password.as_deref(),
                my_identity: &signing_ctx.identity,
                my_ed_sk: &signing_ctx.sync_ctx.my_ed_sk,
                own_reply_syncs: &signing_ctx.own_reply_syncs,
                sender_did: &sender_did,
                envelope: &envelope,
                tag16: &box_entry.tag16,
                decrypted_bytes: &decrypted_bytes,
                now_secs,
            };
            match dispatch_post_payload(&env_ctx, &container.c, &mut post_state, &mut stats).await
            {
                PostDispatchOutcome::Handled => {
                    post_state.processed_msg_ids.insert(envelope.msg_id.clone());
                }
                PostDispatchOutcome::UnsupportedPayload { payload_type } => {
                    let (sender_name, _, _) = resolve_counterparty_info_for_session(
                        state,
                        &session,
                        &sender_did,
                        password.as_deref(),
                    );
                    if !post_state
                        .unsupported_messages
                        .iter()
                        .any(|u| u.msg_id == envelope.msg_id)
                    {
                        if post_state.unsupported_messages.len() >= 50 {
                            post_state.unsupported_messages.remove(0);
                        }
                        post_state
                            .unsupported_messages
                            .push(crate::models::FrontendUnsupportedPostItem {
                                msg_id: envelope.msg_id.clone(),
                                sender_did: sender_did.clone(),
                                sender_name,
                                payload_type,
                                created_at: envelope.created_at,
                            });
                    }
                    post_state.processed_msg_ids.insert(envelope.msg_id.clone());
                }
                PostDispatchOutcome::RetryLater => {}
            }
        }
    }

    // 6. Merge any concurrent local writes before flushing, then flush and persist atomically.
    if !state.is_generation_current(session.generation) {
        return Ok(FrontendPostSyncResult {
            uploaded_to: 0,
            polled_syncs: 0,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: 0,
            new_inquiry_messages: 0,
            pruned_boxes: 0,
            deferred_outbox_boxes: 0,
            skipped_wifi_only: false,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }
    merge_and_save_post_state(
        state,
        &session,
        &baseline_post_state,
        &mut post_state,
        password.as_deref(),
    )?;
    baseline_post_state = post_state.clone();

    let flush_outcome =
        flush_post_slots_impl(state, &session, &mut post_state, None, &[], password.as_deref())
            .await;
    let uploaded_to = flush_outcome.deposited_to;
    stats.errors.extend(flush_outcome.errors);

    if !state.is_generation_current(session.generation) {
        return Ok(FrontendPostSyncResult {
            uploaded_to: 0,
            polled_syncs: 0,
            imported_payments: 0,
            imported_receipts: 0,
            imported_signatures: 0,
            new_sign_requests: 0,
            new_wot_handshakes: 0,
            new_inquiries: 0,
            new_inquiry_messages: 0,
            pruned_boxes: 0,
            deferred_outbox_boxes: 0,
            skipped_wifi_only: false,
            errors: Vec::new(),
            retry_after_secs: None,
        });
    }
    merge_and_save_post_state(
        state,
        &session,
        &baseline_post_state,
        &mut post_state,
        password.as_deref(),
    )?;

    if polled_syncs > 0 || uploaded_to > 0 {
        let post_notices = build_post_notices(&stats, now_secs);
        let _ = state.with_session(&session, |service| {
            state.append_app_notices_with_service(
                service,
                Some(now_secs),
                post_notices,
                password.as_deref(),
            );
            Ok(())
        });
    }

    let mut retry_after_secs: Option<u64> = None;
    for e in &stats.errors {
        if let Some(secs) = sync_client::extract_retry_after_from_error_str(e) {
            retry_after_secs = Some(retry_after_secs.map_or(secs, |prev| prev.max(secs)));
        }
    }

    if !stats.errors.is_empty() {
        warn!(
            "Post sync degraded: error_count={}, imported_payments={}",
            stats.errors.len(),
            stats.imported_payments
        );
    }

    Ok(FrontendPostSyncResult {
        uploaded_to,
        polled_syncs,
        imported_payments: stats.imported_payments,
        imported_receipts: stats.imported_receipts,
        imported_signatures: stats.imported_signatures,
        new_sign_requests: stats.new_sign_requests,
        new_wot_handshakes: stats.new_wot_handshakes,
        new_inquiries: stats.new_inquiries,
        new_inquiry_messages: stats.new_inquiry_messages,
        pruned_boxes: stats.pruned_boxes,
        deferred_outbox_boxes: flush_outcome.deferred_count,
        skipped_wifi_only: false,
        errors: stats.errors,
        retry_after_secs,
    })
}
