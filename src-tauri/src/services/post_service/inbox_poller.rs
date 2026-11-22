use std::collections::HashMap;

use ed25519_dalek::VerifyingKey;
use human_money_core::crypto::get_pubkey_from_user_id;
use human_money_core::models::profile::UserIdentity;
use human_money_core::models::secure_container::{PayloadType, SecureContainer};
use humoco_market::{
    is_own_dummy_post_box, prepare_post_senders, try_open_post_box_prepared,
    PAYLOAD_MARKET_INQUIRY_MESSAGE_V1, PAYLOAD_MARKET_INQUIRY_V1, PAYLOAD_POST_RECEIPT_V1,
    POST_SLOT_MAGIC, POST_TTL_SECS,
};
use log::warn;

use crate::models::{
    FrontendAppNotice, FrontendError, FrontendPendingTolerancePayment, FrontendPostInboxSummary,
    FrontendPostSyncResult, FrontendSyncDescriptor,
};
use crate::services::common::are_dids_equal;
use crate::settings::{SecurityPolicy, UiPreferences};
use crate::state::{AppState, StoredPostState, MARKET_CARD_STORE_KEY};
use crate::sync_client;

use super::handlers::{
    dispatch_post_payload, PostDispatchOutcome, PostDispatchStats, PostEnvelopeContext,
};
use super::{
    clamp_post_ts, load_post_state_read_only_for_session, merge_and_save_post_state,
    push_pruned_outbox_tag16, resolve_counterparty_info_for_session, save_post_state_with_service,
    scoped_processed_msg_key, CounterpartyInfoResolver,
};
use crate::services::post_service::outbox::flush_post_slots_impl;
use crate::services::sync_endpoints::{is_endpoint_revoked, sanitize_untrusted_sync_descriptors};

const MAX_POST_SLOTS_PER_SERVER: usize = 64;
const MAX_POST_SLOTS_TOTAL: usize = 256;
const MAX_POST_MSG_ID_LEN: usize = 128;

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
        save_post_state_with_service(state, &mut service, &post_state, pw)?;
    }
    Ok(())
}

/// Confirms and commits a parked PoST payment that was held for recovery tolerance confirmation.
pub async fn confirm_pending_post_payment_impl(
    app: Option<&tauri::AppHandle>,
    msg_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<crate::models::ReceiveSuccessPayload, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let all_standards = crate::services::common::load_all_standard_tomls(app, None);

    let _post_guard = state.post_op_lock.lock().await;
    let now_secs = sync_client::current_unix_timestamp();

    enum ConfirmCommitOutcome {
        Committed {
            payload: Box<crate::models::ReceiveSuccessPayload>,
            is_conflict: bool,
            queued_ack: bool,
        },
        CoreError {
            err: human_money_core::Error,
            queued_ack: bool,
        },
    }

    let outcome = state.with_session_required(
        &session,
        "Active wallet session changed before pending tolerance payment could be confirmed",
        |service| {
            let mut post_state = super::load_post_state_with_service(service, pw)?;
            let baseline = post_state.clone();
            let item = post_state
                .pending_tolerance_payments
                .iter()
                .find(|p| p.msg_id == msg_id)
                .cloned()
                .ok_or_else(|| {
                    FrontendError::from("Pending tolerance payment not found".to_string())
                })?;

            use base64::Engine as _;
            let container_bytes = base64::engine::general_purpose::STANDARD
                .decode(&item.container_b64)
                .map_err(|e| {
                    FrontendError::from(format!(
                        "Corrupt stored pending tolerance payment bytes: {}",
                        e
                    ))
                })?;

            let tag16_opt: Option<[u8; 16]> = hex::decode(&item.tag16_hex)
                .ok()
                .and_then(|bytes| bytes.try_into().ok());
            let ack_ctx = tag16_opt.and_then(|tag16| {
                let my_identity = service.with_wallet_and_identity(|_, id| id.clone()).ok()?;
                let sync_ctx =
                    crate::services::common::SyncIdentityContext::from_service(service, pw).ok()?;
                let security_policy = state.get_or_load_security_policy(service, pw).ok()?;
                let own_reply_syncs = super::collect_own_reply_syncs(
                    &security_policy,
                    sync_ctx.my_prefix.as_deref(),
                );
                Some((my_identity, sync_ctx.my_ed_sk, tag16, own_reply_syncs))
            });
            let mut queued_ack = false;
            let queue_confirm_ack = |ps: &mut StoredPostState, bundle_id: Option<&str>| -> bool {
                let Some((my_identity, my_ed_sk, tag16, own_reply_syncs)) = &ack_ctx else {
                    return false;
                };
                super::queue_receipt(
                    ps,
                    my_identity,
                    my_ed_sk,
                    &item.sender_did,
                    &item.msg_id,
                    tag16,
                    bundle_id,
                    own_reply_syncs,
                    now_secs,
                );
                true
            };

            let is_conflict = item.l2_evidence.as_ref().is_some_and(|e| e.is_conflict);
            match super::handlers::commit_post_transaction_bundle(
                service,
                state,
                app,
                &container_bytes,
                &all_standards,
                item.l2_evidence.as_ref(),
                true,
                pw,
                &baseline,
                &post_state,
                |ps, result| {
                    ps.pending_tolerance_payments.retain(|p| {
                        p.msg_id != item.msg_id || !are_dids_equal(&p.sender_did, &item.sender_did)
                    });
                    ps.processed_msg_ids.insert(
                        scoped_processed_msg_key(&item.sender_did, &item.msg_id),
                        now_secs,
                    );
                    if queue_confirm_ack(ps, Some(&result.header.bundle_id)) {
                        queued_ack = true;
                    }
                },
            )? {
                super::handlers::PostBundleCommitResult::Committed { result, .. } => {
                    let payload =
                        crate::services::transfer_service::build_receive_success_payload_from_result(
                            service,
                            state,
                            &container_bytes,
                            *result,
                            format!("post-{}", item.msg_id),
                            pw,
                        )?;
                    Ok(ConfirmCommitOutcome::Committed {
                        payload: Box::new(payload),
                        is_conflict,
                        queued_ack,
                    })
                }
                super::handlers::PostBundleCommitResult::CoreError(e) => {
                    let is_duplicate =
                        matches!(e, human_money_core::Error::BundleAlreadyProcessed { .. });
                    let is_stale =
                        matches!(e, human_money_core::Error::BundlePredatesCurrentEpoch)
                            || is_duplicate;
                    if is_stale {
                        post_state.pending_tolerance_payments.retain(|p| {
                            p.msg_id != item.msg_id
                                || !are_dids_equal(&p.sender_did, &item.sender_did)
                        });
                        post_state.processed_msg_ids.insert(
                            scoped_processed_msg_key(&item.sender_did, &item.msg_id),
                            now_secs,
                        );
                        if is_duplicate && queue_confirm_ack(&mut post_state, None) {
                            queued_ack = true;
                        }
                        super::merge_post_state_with_service(
                            service,
                            &baseline,
                            &mut post_state,
                            pw,
                        )?;
                        save_post_state_with_service(state, service, &post_state, pw)?;
                    }
                    Ok(ConfirmCommitOutcome::CoreError { err: e, queued_ack })
                }
            }
        },
    )?;

    match outcome {
        ConfirmCommitOutcome::Committed {
            payload,
            is_conflict,
            queued_ack,
        } => {
            if queued_ack {
                state.wake_sync_coordinator(crate::state::SyncWakeRequest {
                    post: true,
                    ..Default::default()
                });
            }
            if is_conflict {
                if let Some(app_handle) = app {
                    use tauri::Emitter;
                    let _ = app_handle.emit("humoco://conflicts-updated", ());
                }
            }
            Ok(*payload)
        }
        ConfirmCommitOutcome::CoreError { err, queued_ack } => {
            if queued_ack {
                state.wake_sync_coordinator(crate::state::SyncWakeRequest {
                    post: true,
                    ..Default::default()
                });
            }
            Err(FrontendError::from(err))
        }
    }
}

/// Dismisses a pending sign request, WoT handshake, inquiry, unsupported notice, or tolerance payment atomically under `state.service`.
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
            let scoped_to_remove: Vec<String> = post_state
                .pending_wot_handshakes
                .iter()
                .filter(|h| {
                    h.msg_id == id || scoped_processed_msg_key(&h.sender_did, &h.msg_id) == id
                })
                .map(|h| scoped_processed_msg_key(&h.sender_did, &h.msg_id))
                .collect();
            for key in scoped_to_remove {
                post_state.pending_wot_packages_by_msg_id.remove(&key);
            }
            post_state.pending_wot_packages_by_msg_id.remove(&id);
            post_state.pending_wot_handshakes.retain(|h| {
                h.msg_id != id && scoped_processed_msg_key(&h.sender_did, &h.msg_id) != id
            });
        }
        "inquiry" => {
            let keys_to_resolve: Vec<(String, String)> = post_state
                .inquiries
                .iter()
                .filter(|i| i.inquiry_id == id && i.role == "seller" && i.status == "open")
                .filter_map(|i| {
                    get_pubkey_from_user_id(&i.counterparty_did).ok().map(|pk| {
                        (
                            hex::encode(pk.as_bytes()),
                            super::inquiry_rate_limit_key(&i.target),
                        )
                    })
                })
                .collect();
            for (sender_hex, target_key) in keys_to_resolve {
                post_state
                    .inquiry_rate_tracker
                    .resolve_inquiry(&sender_hex, &target_key);
            }
            post_state.inquiries.retain(|i| i.inquiry_id != id);
        }
        "unsupported" => {
            post_state.unsupported_messages.retain(|u| u.msg_id != id);
        }
        "tolerance_payment" => {
            post_state
                .pending_tolerance_payments
                .retain(|p| p.msg_id != id);
        }
        other => {
            return Err(FrontendError::from(format!(
                "Unknown pending post item kind: {}",
                other
            )));
        }
    }
    save_post_state_with_service(state, &mut service, &post_state, pw)
}

/// Returns the current Post inbox summary enriched with counterparty contact metadata.
pub fn get_post_inbox_summary_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostInboxSummary, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::Unlocked,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut post_state = load_post_state_read_only_for_session(state, &session, pw)?;
    let unacknowledged_payments = post_state
        .outbox_entries
        .iter()
        .filter_map(|e| e.bundle_id.clone())
        .collect();
    let resolver = CounterpartyInfoResolver::load_for_session_read_only(state, &session, pw)?;

    for inq in &mut post_state.inquiries {
        let (name, pic, is_contact) = resolver.resolve(&inq.counterparty_did);
        inq.counterparty_name = name;
        inq.counterparty_picture_url = pic;
        inq.is_contact = is_contact;
    }
    post_state
        .inquiries
        .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    for item in &mut post_state.unsupported_messages {
        if item.sender_name.is_none() {
            let (name, _, _) = resolver.resolve(&item.sender_did);
            item.sender_name = name;
        }
    }

    let pending_tolerance_payments = post_state
        .pending_tolerance_payments
        .into_iter()
        .map(|p| {
            let sender_name = p.sender_name.or_else(|| resolver.resolve(&p.sender_did).0);
            FrontendPendingTolerancePayment {
                msg_id: p.msg_id,
                sender_did: p.sender_did,
                sender_name,
                created_at: p.created_at,
                zone: p.zone,
                error_message: p.error_message,
                has_conflict: p.l2_evidence.is_some_and(|e| e.is_conflict),
            }
        })
        .collect();

    Ok(FrontendPostInboxSummary {
        pending_sign_requests: post_state.pending_sign_requests,
        pending_wot_handshakes: post_state.pending_wot_handshakes,
        inquiries: post_state.inquiries,
        unacknowledged_payments,
        unsupported_messages: post_state.unsupported_messages,
        pending_tolerance_payments,
    })
}

fn push_sync_slot_targets(
    direct: &mut Vec<(String, String, Option<String>)>,
    index_urls: &mut Vec<String>,
    descriptors: &[FrontendSyncDescriptor],
    prefix: Option<&str>,
) {
    for d in descriptors {
        if is_endpoint_revoked(&d.status)
            || sync_client::validate_sync_read_url(&d.read_url).is_err()
        {
            continue;
        }
        let read_url = d.read_url.trim().to_string();
        if let Some(ref sid) = d.slot_id {
            let sid_trimmed = sid.trim();
            if humoco_market::parse_slot_id(sid_trimmed).is_err() {
                continue;
            }
            direct.push((
                read_url,
                sid_trimmed.to_string(),
                prefix.map(str::to_string),
            ));
        } else {
            index_urls.push(read_url);
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

fn has_active_inquiry_listing(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    now_secs: u64,
    password: Option<&str>,
) -> bool {
    crate::services::market_service::load_own_market_profile_state(state, service, password)
        .ok()
        .map(|stored| {
            stored.published
                && (stored.allow_inquiries
                    || stored
                        .ads
                        .iter()
                        .any(|a| a.expires_at > now_secs && a.allow_inquiries))
        })
        .unwrap_or(false)
}

struct PollSourceParams<'a> {
    security_policy: &'a SecurityPolicy,
    ui_prefs: &'a UiPreferences,
    my_did: &'a str,
    my_prefix: Option<&'a str>,
    post_state: &'a StoredPostState,
    now_secs: u64,
    password: Option<&'a str>,
}

fn collect_candidate_slot_sources(
    state: &AppState,
    session: &crate::session::SessionToken,
    params: &PollSourceParams<'_>,
) -> Result<PollSlotTargets, FrontendError> {
    let PollSourceParams {
        security_policy,
        ui_prefs,
        my_did,
        my_prefix,
        post_state,
        now_secs,
        password,
    } = *params;
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
                if pd.enabled
                    && !is_endpoint_revoked(&pd.status)
                    && !pd.read_url.is_empty()
                    && (pd.authorized_channels.is_empty()
                        || pd.authorized_channels.iter().any(|c| c == "post"))
                {
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
                            if pd.enabled
                                && !is_endpoint_revoked(&pd.status)
                                && !pd.read_url.is_empty()
                                && (pd.authorized_channels.is_empty()
                                    || pd.authorized_channels.iter().any(|c| c == "post"))
                            {
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

            let has_inquiry_listing =
                has_active_inquiry_listing(state, service, now_secs, password);
            if ui_prefs.post_accept_from_contacts || has_inquiry_listing {
                match crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
                    service,
                    MARKET_CARD_STORE_KEY,
                    password,
                ) {
                    Ok(Some(store)) => {
                        for sc in store.cards.values() {
                            if !((ui_prefs.post_accept_from_contacts && sc.hops <= 2)
                                || has_inquiry_listing)
                            {
                                continue;
                            }
                            let card_drops = sanitize_untrusted_sync_descriptors(
                                &sc.card.payload.drops,
                                humoco_market::MAX_CARD_DROPS,
                            );
                            push_sync_slot_targets(&mut direct, &mut index_urls, &card_drops, None);
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
                has_active_inquiry_listing(state, service, now_secs, password);

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

struct PostSlotDownloadOutcome {
    fetched_slots: Vec<Vec<u8>>,
    health_updates: Vec<(String, bool)>,
    polled_syncs: usize,
    errors: Vec<String>,
    max_retry_after_secs: Option<u64>,
}

fn record_post_sync_error(
    state: &AppState,
    url: &str,
    err: &sync_client::SyncClientError,
    max_retry_after_secs: &mut Option<u64>,
) {
    state.record_origin_cooldown_for_sync_error(url, err);
    if let Some(secs) = err.retry_after_secs() {
        *max_retry_after_secs = Some(max_retry_after_secs.map_or(secs, |prev| prev.max(secs)));
    }
}

async fn download_post_slots(
    state: &AppState,
    client: &reqwest::Client,
    direct_slot_fetches: Vec<(String, String, Option<String>)>,
    candidate_read_urls: Vec<String>,
) -> PostSlotDownloadOutcome {
    let mut fetched_slots = Vec::new();
    let mut health_updates = Vec::new();
    let mut polled_syncs = 0;
    let mut errors = Vec::new();
    let mut max_retry_after_secs: Option<u64> = None;

    for (read_url, slot_id, prefix_opt) in direct_slot_fetches {
        if fetched_slots.len() >= MAX_POST_SLOTS_TOTAL {
            break;
        }
        if state.is_origin_cooling_down(&read_url, std::time::Instant::now()) {
            errors.push(format!(
                "Poll of {} deferred: server origin cooling down after HTTP 429",
                read_url
            ));
            continue;
        }
        let Ok((base_slot_pk, slot_prefix)) = humoco_market::parse_slot_id(&slot_id) else {
            errors.push(format!(
                "Skipped direct post slot poll due to invalid slot_id for {}",
                read_url
            ));
            continue;
        };
        polled_syncs += 1;
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
                record_post_sync_error(state, &read_url, &e, &mut max_retry_after_secs);
                if !e.is_local_validation() {
                    health_updates.push((read_url.clone(), false));
                }
                errors.push(format!(
                    "Failed to fetch direct post slot from {}: {}",
                    read_url, e
                ));
            }
        }
    }

    for read_url in candidate_read_urls {
        if fetched_slots.len() >= MAX_POST_SLOTS_TOTAL {
            break;
        }
        if state.is_origin_cooling_down(&read_url, std::time::Instant::now()) {
            errors.push(format!(
                "Poll of {} deferred: server origin cooling down after HTTP 429",
                read_url
            ));
            continue;
        }
        polled_syncs += 1;
        match sync_client::list_index(client, &read_url, None, None).await {
            Ok(sync_client::IndexFetchResult::Modified { entries, .. }) => {
                health_updates.push((read_url.clone(), true));
                let mut server_post_count = 0usize;
                for entry in entries {
                    if entry.channel != "post" {
                        continue;
                    }
                    if server_post_count >= MAX_POST_SLOTS_PER_SERVER
                        || fetched_slots.len() >= MAX_POST_SLOTS_TOTAL
                    {
                        break;
                    }
                    server_post_count += 1;
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
                        Err(e) => {
                            record_post_sync_error(state, &read_url, &e, &mut max_retry_after_secs);
                            errors.push(format!(
                                "Failed to fetch post slot {} from {}: {}",
                                entry.id, read_url, e
                            ));
                        }
                    }
                }
            }
            Ok(sync_client::IndexFetchResult::NotModified) => {
                health_updates.push((read_url.clone(), true));
            }
            Err(e) => {
                record_post_sync_error(state, &read_url, &e, &mut max_retry_after_secs);
                if !e.is_local_validation() {
                    health_updates.push((read_url.clone(), false));
                }
                errors.push(format!(
                    "Failed to fetch post slot index from {}: {}",
                    read_url, e
                ));
            }
        }
    }

    PostSlotDownloadOutcome {
        fetched_slots,
        health_updates,
        polled_syncs,
        errors,
        max_retry_after_secs,
    }
}

fn open_verified_container(
    container: &SecureContainer,
    my_identity: &UserIdentity,
    sender_did: &str,
    errors: &mut Vec<String>,
) -> Option<Vec<u8>> {
    if container.et != human_money_core::models::secure_container::EncryptionType::Asymmetric {
        errors.push(format!(
            "Rejected PoST container with non-asymmetric encryption type: {:?}",
            container.et
        ));
        return None;
    }

    if let Err(e) = container.verify_integrity() {
        errors.push(format!("Integrity check failed: {}", e));
        return None;
    }

    // Anonymous/stealth `TransactionBundle` containers omit the outer signature per
    // `HMC-SEC-06-02` to prevent a permanent-key de-anonymization oracle; Core's
    // `open_and_verify_bundle` (`S05-F3`) enforces that only anonymous bundles omit
    // it and verifies `bundle.sender_signature`. All other payload types and signed
    // bundles MUST carry a valid container signature from `sender_did`.
    if container.c != PayloadType::TransactionBundle || !container.signature.is_empty() {
        if let Err(e) = container.verify_container_signature(sender_did) {
            errors.push(format!("Container signature check failed: {}", e));
            return None;
        }
    }

    match container.open(Some(my_identity), None) {
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
                        container.open(Some(&alt), None).ok()
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
    let (session, resolved_pw, ui_prefs, security_policy) = {
        let (mut service, resolved_pw) = state.authorize_and_lock_secret(
            crate::state::RequiredAccess::Unlocked,
            password.as_deref(),
        )?;
        let pw = resolved_pw.as_deref().map(String::as_str);
        let session = state.session_token_for_service(&service)?;
        let ui_prefs = state
            .get_or_load_ui_preferences(&mut service, pw)
            .map_err(FrontendError::from)?;
        let security_policy = state
            .get_or_load_security_policy(&mut service, pw)
            .map_err(FrontendError::from)?;
        (session, resolved_pw, ui_prefs, security_policy)
    };
    let pw = resolved_pw.as_deref().map(String::as_str);

    if ui_prefs.market_poll_wifi_only && is_wifi == Some(false) {
        return Ok(FrontendPostSyncResult {
            skipped_wifi_only: true,
            ..Default::default()
        });
    }

    let _post_guard = state.post_op_lock.lock().await;
    let client = sync_client::create_sync_http_client();
    let (signing_ctx, mut post_state) = state.with_session_required(
        &session,
        "Active wallet session changed while preparing post sync",
        |service| {
            let ps = crate::services::post_service::load_post_state_with_service(service, pw)?;
            let my_identity = service
                .with_wallet_and_identity(|_w, id| id.clone())
                .map_err(FrontendError::from)?;
            let sync_ctx = crate::services::common::SyncIdentityContext::from_service(service, pw)?;
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
        let mut pruned_tags = std::mem::take(&mut post_state.pruned_outbox_tag16s);
        post_state.outbox_entries.retain(|e| {
            if now_secs.saturating_sub(e.created_at) <= POST_TTL_SECS {
                true
            } else {
                push_pruned_outbox_tag16(&mut pruned_tags, e.tag16_hex.clone());
                false
            }
        });
        post_state.pruned_outbox_tag16s = pruned_tags;
        stats.pruned_boxes += before_count - post_state.outbox_entries.len();
    }

    // 2. Build candidate direct fetches and index URLs
    let poll_params = PollSourceParams {
        security_policy: &security_policy,
        ui_prefs: &ui_prefs,
        my_did: &signing_ctx.identity.user_id,
        my_prefix: signing_ctx.sync_ctx.my_prefix.as_deref(),
        post_state: &post_state,
        now_secs,
        password: pw,
    };
    let (direct_slot_fetches, candidate_read_urls) =
        collect_candidate_slot_sources(state, &session, &poll_params)?;

    // 3. Build authorized_senders, card_signers, and inquiry_only_senders
    let authorized_groups =
        collect_authorized_post_senders(state, &session, &ui_prefs, &post_state, now_secs, pw)?;
    let authorized_senders = authorized_groups.full;
    let card_signer_senders = authorized_groups.card_signers;
    let inquiry_only_senders = authorized_groups.inquiry;

    // 4. Download slots (no mutex held across `.await`)
    let PostSlotDownloadOutcome {
        fetched_slots,
        health_updates: post_sync_health_updates,
        polled_syncs,
        errors: download_errors,
        max_retry_after_secs: download_retry_after,
    } = download_post_slots(state, &client, direct_slot_fetches, candidate_read_urls).await;
    if !state.is_generation_current(session.generation) {
        return Ok(FrontendPostSyncResult::default());
    }
    stats.errors.extend(download_errors);

    if !post_sync_health_updates.is_empty() {
        let committed = state.with_session(&session, |service| {
            state.record_contact_sync_health_batch(
                service,
                &post_sync_health_updates,
                now_secs,
                pw,
            );
            Ok(())
        })?;
        if committed.is_none() {
            return Ok(FrontendPostSyncResult::default());
        }
    }

    // 5. Decrypt all entries, sort by dependency rank & created_at, and dispatch
    enum SenderPrivilege {
        Full,
        CardSignerAndMaybeInquiry(bool),
        InquiryOnly,
    }
    struct DecryptedEnvelopeItem {
        dep_rank: u8,
        sender_did: String,
        tag16: [u8; 16],
        envelope: humoco_market::PostEnvelope,
        payload_type: PayloadType,
        decrypted_bytes: Vec<u8>,
    }

    let mut decrypted_items: Vec<DecryptedEnvelopeItem> = Vec::new();
    let (prepared_authorized, prepared_card_signers, prepared_inquiry_only) =
        if fetched_slots.is_empty() {
            (Vec::new(), Vec::new(), Vec::new())
        } else {
            (
                prepare_post_senders(&signing_ctx.sync_ctx.my_ed_sk, &authorized_senders),
                prepare_post_senders(&signing_ctx.sync_ctx.my_ed_sk, &card_signer_senders),
                prepare_post_senders(&signing_ctx.sync_ctx.my_ed_sk, &inquiry_only_senders),
            )
        };
    for slot_bytes in fetched_slots {
        if slot_bytes.len() > humoco_market::DROP_POST_MAX_BYTES {
            stats.errors.push(format!(
                "Rejected oversized post slot ({} > {} bytes)",
                slot_bytes.len(),
                humoco_market::DROP_POST_MAX_BYTES
            ));
            continue;
        }
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

            let open_attempt = try_open_post_box_prepared(&prepared_authorized, &box_entry);
            let (matched_sender_did, envelope, privilege) = match open_attempt {
                Ok(Some((did, env))) => (did, env, SenderPrivilege::Full),
                Ok(None) => {
                    let card_attempt = if !prepared_card_signers.is_empty() {
                        try_open_post_box_prepared(&prepared_card_signers, &box_entry)
                            .ok()
                            .flatten()
                    } else {
                        None
                    };
                    if let Some((did, env)) = card_attempt {
                        let also_inquiry = prepared_inquiry_only
                            .iter()
                            .any(|s| are_dids_equal(&s.sender_did, &did));
                        (
                            did,
                            env,
                            SenderPrivilege::CardSignerAndMaybeInquiry(also_inquiry),
                        )
                    } else if !prepared_inquiry_only.is_empty() {
                        match try_open_post_box_prepared(&prepared_inquiry_only, &box_entry) {
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

            let msg_id_trimmed = envelope.msg_id.trim();
            if msg_id_trimmed.is_empty()
                || envelope.msg_id.len() > MAX_POST_MSG_ID_LEN
                || envelope.msg_id.chars().any(char::is_control)
            {
                stats
                    .errors
                    .push("Rejected PostEnvelope with invalid msg_id".to_string());
                continue;
            }

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

            let scoped_key = scoped_processed_msg_key(&sender_did, &envelope.msg_id);
            if post_state.processed_msg_ids.contains_key(&scoped_key) {
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

            let dep_rank = match &container.c {
                PayloadType::TrustAssertion => 0,
                PayloadType::Generic(ref s)
                    if s == PAYLOAD_MARKET_INQUIRY_V1
                        || s == humoco_market::PAYLOAD_GUARDIAN_ASSIGN_V1 =>
                {
                    0
                }
                PayloadType::Generic(ref s)
                    if s == PAYLOAD_MARKET_INQUIRY_MESSAGE_V1
                        || s == humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1 =>
                {
                    2
                }
                _ => 1,
            };

            decrypted_items.push(DecryptedEnvelopeItem {
                dep_rank,
                sender_did,
                tag16: box_entry.tag16,
                envelope,
                payload_type: container.c.clone(),
                decrypted_bytes,
            });
        }
    }

    decrypted_items.sort_by(|a, b| {
        a.dep_rank
            .cmp(&b.dep_rank)
            .then(a.envelope.created_at.cmp(&b.envelope.created_at))
            .then_with(|| a.envelope.msg_id.cmp(&b.envelope.msg_id))
    });

    for item in decrypted_items {
        let scoped_key = scoped_processed_msg_key(&item.sender_did, &item.envelope.msg_id);
        if post_state.processed_msg_ids.contains_key(&scoped_key) {
            continue;
        }

        let env_ctx = PostEnvelopeContext {
            state,
            session: &session,
            app,
            extra_standards,
            password: pw,
            my_identity: &signing_ctx.identity,
            my_ed_sk: &signing_ctx.sync_ctx.my_ed_sk,
            own_reply_syncs: &signing_ctx.own_reply_syncs,
            sender_did: &item.sender_did,
            envelope: &item.envelope,
            tag16: &item.tag16,
            decrypted_bytes: &item.decrypted_bytes,
            now_secs,
        };
        match dispatch_post_payload(
            &env_ctx,
            &item.payload_type,
            &mut post_state,
            &mut baseline_post_state,
            &mut stats,
        )
        .await
        {
            PostDispatchOutcome::Handled => {
                post_state.processed_msg_ids.insert(scoped_key, now_secs);
                if !item.envelope.reply_drops.is_empty() {
                    let reply_dtos = sanitize_untrusted_sync_descriptors(
                        &item.envelope.reply_drops,
                        humoco_market::MAX_CARD_DROPS,
                    );
                    if !reply_dtos.is_empty() {
                        let committed = state.with_session(&session, |service| {
                            if let Err(e) = state.update_contact_from_card(
                                service,
                                &item.sender_did,
                                &reply_dtos,
                                None,
                                pw,
                            ) {
                                warn!(
                                    "Failed to update contact from PoST reply syncs: {}",
                                    e.message
                                );
                            }
                            Ok(())
                        })?;
                        if committed.is_none() {
                            break;
                        }
                    }
                }
            }
            PostDispatchOutcome::UnsupportedPayload { payload_type } => {
                let (sender_name, _, _) =
                    resolve_counterparty_info_for_session(state, &session, &item.sender_did, pw);
                if !post_state.unsupported_messages.iter().any(|u| {
                    u.msg_id == item.envelope.msg_id
                        && are_dids_equal(&u.sender_did, &item.sender_did)
                }) {
                    if post_state.unsupported_messages.len() >= 50 {
                        post_state.unsupported_messages.remove(0);
                    }
                    post_state.unsupported_messages.push(
                        crate::models::FrontendUnsupportedPostItem {
                            msg_id: item.envelope.msg_id.clone(),
                            sender_did: item.sender_did.clone(),
                            sender_name,
                            payload_type,
                            created_at: clamp_post_ts(item.envelope.created_at, now_secs),
                        },
                    );
                }
                post_state.processed_msg_ids.insert(scoped_key, now_secs);
            }
            PostDispatchOutcome::RetryLater => {}
        }
    }

    // 6. Merge any concurrent local writes before flushing, then flush and persist atomically.
    if !state.is_generation_current(session.generation) {
        return Ok(FrontendPostSyncResult::default());
    }
    super::prune_expired_processed_msg_ids(&mut post_state.processed_msg_ids, now_secs);
    merge_and_save_post_state(state, &session, &baseline_post_state, &mut post_state, pw)?;
    baseline_post_state = post_state.clone();

    let flush_outcome =
        flush_post_slots_impl(state, &session, &mut post_state, None, &[], pw).await;
    let uploaded_to = flush_outcome.deposited_to;
    let retry_after_secs = match (download_retry_after, flush_outcome.max_retry_after_secs) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    stats.errors.extend(flush_outcome.errors);

    if !state.is_generation_current(session.generation) {
        return Ok(FrontendPostSyncResult::default());
    }
    merge_and_save_post_state(state, &session, &baseline_post_state, &mut post_state, pw)?;

    if polled_syncs > 0 || uploaded_to > 0 {
        let post_notices = build_post_notices(&stats, now_secs);
        let _ = state.with_session(&session, |service| {
            state.append_app_notices_with_service(service, Some(now_secs), post_notices, pw);
            Ok(())
        });
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
        imported_wot_lists: stats.imported_wot_lists,
        new_inquiries: stats.new_inquiries,
        new_inquiry_messages: stats.new_inquiry_messages,
        new_tolerance_payments: stats.new_tolerance_payments,
        pruned_boxes: stats.pruned_boxes,
        deferred_outbox_boxes: flush_outcome.deferred_count,
        skipped_wifi_only: false,
        errors: stats.errors,
        retry_after_secs,
    })
}
