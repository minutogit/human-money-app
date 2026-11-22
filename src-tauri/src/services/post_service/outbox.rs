use chrono::Utc;
use human_money_core::models::secure_container::{EncryptionType, PayloadType, SecureContainer};
use human_money_core::ValidatedUserId;
use humoco_market::{
    merge_and_pack_post_slot, pack_post_slot, validate_market_inquiry,
    validate_market_inquiry_message, validate_wot_handshake_assertion, DropDescriptor,
    MarketInquiry, MarketInquiryDecision, MarketInquiryMessage, PostBoxEntry,
    WotHandshakeAssertion, DROP_POST_MAX_BYTES, MAX_INQUIRY_MESSAGES_PER_THREAD,
    PAYLOAD_MARKET_INQUIRY_MESSAGE_V1, PAYLOAD_MARKET_INQUIRY_V1,
};
use log::warn;

use crate::models::{
    FrontendContact, FrontendError, FrontendInquiryChatMessage, FrontendMarketInquiryItem,
    FrontendPostDepositResult, FrontendSyncDescriptor, FrontendUserProfile,
};
use crate::services::common::{
    are_dids_equal, normalize_sync_url, DropChannelSigner, SyncIdentityContext,
};
use crate::session::SessionToken;
use crate::state::{AppState, StoredPostState};
use crate::sync_client::{self as sync_client, PutSlotOutcome};

use super::{
    collect_own_reply_syncs, collect_post_upload_targets, enqueue_post_envelope,
    inquiry_rate_limit_key, load_post_signing_context_for_service,
    load_post_signing_context_for_session, load_post_state_for_session,
    load_post_state_with_service, lookup_counterparty_syncs, merge_and_save_post_state,
    merge_post_state_with_service, resolve_counterparty_info_for_session, scoped_processed_msg_key,
    seal_post_container_bytes,
};
use crate::services::sync_endpoints::{
    canonical_drop_pub_hex, is_endpoint_revoked, sanitize_untrusted_frontend_sync_descriptors,
    sanitize_untrusted_sync_descriptors,
};

struct FlushContext {
    id_ctx: SyncIdentityContext,
    upload_targets: Vec<DropDescriptor>,
    own_write_urls: std::collections::HashSet<String>,
    recipient_write_urls: std::collections::HashSet<String>,
    recipient_dids_by_write_url:
        std::collections::HashMap<String, std::collections::HashSet<String>>,
    value_outflow_blocked: bool,
    blocked_offender_dids: std::collections::HashSet<String>,
    now_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlushTargetScope {
    All,
    RecipientOnly,
    OwnExcludingRecipient,
}

#[derive(Debug, Default)]
pub(crate) struct FlushPostSlotsOutcome {
    pub deposited_to: usize,
    pub recipient_deposited_to: usize,
    pub recipient_uploaded_tags: std::collections::HashSet<[u8; 16]>,
    pub uploaded_counts_by_tag: std::collections::HashMap<[u8; 16], usize>,
    pub recipient_uploaded_counts_by_tag: std::collections::HashMap<[u8; 16], usize>,
    pub deferred_tags: std::collections::HashSet<[u8; 16]>,
    pub deferred_count: usize,
    pub errors: Vec<String>,
}

fn verify_outgoing_post_container(
    container_bytes: &[u8],
    expected_payload: PayloadType,
    expected_sender_did: &str,
    label: &str,
) -> Result<(), FrontendError> {
    let container: SecureContainer = serde_json::from_slice(container_bytes)
        .map_err(|e| FrontendError::from(format!("Invalid {} container JSON: {}", label, e)))?;
    if container.et != EncryptionType::Asymmetric || container.c != expected_payload {
        return Err(FrontendError::from(format!(
            "{} must be an asymmetrically encrypted {:?} container",
            label, expected_payload
        )));
    }
    container.verify_integrity().map_err(FrontendError::from)?;
    container
        .verify_container_signature(expected_sender_did)
        .map_err(FrontendError::from)
}

fn record_recipient_target(
    recipient_write_urls: &mut std::collections::HashSet<String>,
    recipient_dids_by_write_url: &mut std::collections::HashMap<
        String,
        std::collections::HashSet<String>,
    >,
    targets: &mut Vec<DropDescriptor>,
    canonical_did: &str,
    descriptor: DropDescriptor,
) {
    if sync_client::validate_sync_url(&descriptor.write_url).is_err()
        || sync_client::validate_sync_read_url(&descriptor.read_url).is_err()
        || canonical_drop_pub_hex(&descriptor.drop_pub).is_err()
    {
        return;
    }
    let norm = normalize_sync_url(&descriptor.write_url);
    if !norm.is_empty() {
        recipient_write_urls.insert(norm.clone());
        recipient_dids_by_write_url
            .entry(norm)
            .or_default()
            .insert(canonical_did.to_string());
        targets.push(descriptor);
    }
}

fn map_seal_lock_error(reason: human_money_core::models::seal::SealLockReason) -> FrontendError {
    match reason {
        human_money_core::models::seal::SealLockReason::SupersededByHandover {
            target_epoch,
            is_seed_recovery,
            ..
        } => {
            let mut err = FrontendError::with_code(
                "error.security.supersededByHandover",
                format!(
                    "Wallet profile was handed over to another device (epoch #{}).",
                    target_epoch
                ),
            );
            err.details
                .insert("targetEpoch".to_string(), target_epoch.to_string());
            err.details
                .insert("isSeedRecovery".to_string(), is_seed_recovery.to_string());
            err
        }
        human_money_core::models::seal::SealLockReason::ConcurrentCloneDetected { .. } => {
            FrontendError::with_code(
                "error.security.concurrentCloneDetected",
                "Security Lockdown: Concurrent clone detected.",
            )
        }
        human_money_core::models::seal::SealLockReason::ForkDetected => FrontendError::with_code(
            "error.security.walletLockedDueToFork",
            "Security Lockdown: Wallet is locked due to a detected fork in the transaction history. Recovery required.",
        ),
    }
}

fn refresh_outbox_security_guards_with_service(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    post_state: &StoredPostState,
    password: Option<&str>,
) -> (bool, std::collections::HashSet<String>) {
    let mut value_outflow_blocked =
        crate::services::common::ensure_restore_l2_reconciled(state, service, password).is_err()
            || service
                .get_seal_lock_reason(password)
                .map_or(true, |reason| reason.is_some());

    let mut blocked_offender_dids = std::collections::HashSet::new();
    let guarded_recipients: Vec<String> = post_state
        .outbox_entries
        .iter()
        .filter(|e| e.message_class.requires_outflow_guard())
        .map(|e| e.recipient_did.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !value_outflow_blocked && !guarded_recipients.is_empty() {
        let check_res = service.with_wallet(|wallet| {
            if wallet
                .assert_not_known_offender(wallet.get_user_id())
                .is_err()
            {
                value_outflow_blocked = true;
            }
            for rcpt in &guarded_recipients {
                if wallet.assert_not_known_offender(rcpt).is_err() {
                    blocked_offender_dids.insert(rcpt.clone());
                }
            }
        });
        if check_res.is_err() {
            value_outflow_blocked = true;
        }
    }
    (value_outflow_blocked, blocked_offender_dids)
}

fn refresh_outbox_security_guards(
    state: &AppState,
    session: &SessionToken,
    post_state: &StoredPostState,
    password: Option<&str>,
) -> Result<(bool, std::collections::HashSet<String>), String> {
    state
        .with_session_required(
            session,
            "Active wallet session changed before outbox security check",
            |service| {
                Ok(refresh_outbox_security_guards_with_service(
                    state, service, post_state, password,
                ))
            },
        )
        .map_err(|e| e.message)
}

#[cfg(test)]
pub(crate) fn collect_flush_security_for_test(
    state: &AppState,
    session: &SessionToken,
    post_state: &StoredPostState,
    password: Option<&str>,
) -> (bool, std::collections::HashSet<String>) {
    refresh_outbox_security_guards(state, session, post_state, password)
        .unwrap_or((true, Default::default()))
}

fn collect_flush_context(
    state: &AppState,
    session: &SessionToken,
    post_state: &StoredPostState,
    extra_recipient_did: Option<&str>,
    extra_recipient_syncs: &[FrontendSyncDescriptor],
    password: Option<&str>,
) -> Result<FlushContext, String> {
    state
        .with_session_required(
            session,
            "Active wallet session changed before outbox flush could be prepared",
            |service| {
                let id_ctx = SyncIdentityContext::from_service(service, password).map_err(|e| {
                    FrontendError::from(format!(
                        "Failed to access wallet signing key: {}",
                        e.message
                    ))
                })?;
                let security_policy = state
                    .get_or_load_security_policy(service, password)
                    .map_err(|e| {
                        FrontendError::from(format!("Failed to load security policy: {}", e))
                    })?;
                let mut targets = collect_post_upload_targets(&security_policy);
                let own_write_urls: std::collections::HashSet<String> = targets
                    .iter()
                    .map(|d| normalize_sync_url(&d.write_url))
                    .filter(|u| !u.is_empty())
                    .collect();
                let mut recipient_write_urls = std::collections::HashSet::new();
                let mut recipient_dids_by_write_url: std::collections::HashMap<
                    String,
                    std::collections::HashSet<String>,
                > = std::collections::HashMap::new();

                let extra_trimmed = extra_recipient_did.map(str::trim).filter(|s| !s.is_empty());
                let mut exact_matched_recipients = std::collections::HashSet::new();

                if !extra_recipient_syncs.is_empty() {
                    for d in extra_recipient_syncs {
                        if is_endpoint_revoked(&d.status) {
                            continue;
                        }
                        if let Some(rcpt) = extra_trimmed {
                            exact_matched_recipients.insert(rcpt.to_string());
                            record_recipient_target(
                                &mut recipient_write_urls,
                                &mut recipient_dids_by_write_url,
                                &mut targets,
                                rcpt,
                                d.clone().into(),
                            );
                        } else if sync_client::validate_sync_url(&d.write_url).is_ok()
                            && sync_client::validate_sync_read_url(&d.read_url).is_ok()
                            && canonical_drop_pub_hex(&d.drop_pub).is_ok()
                        {
                            let norm = normalize_sync_url(&d.write_url);
                            if !norm.is_empty() {
                                recipient_write_urls.insert(norm);
                                targets.push(d.clone().into());
                            }
                        }
                    }
                }

                let queued_recipients: std::collections::HashSet<String> = post_state
                    .outbox_entries
                    .iter()
                    .map(|e| e.recipient_did.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();

                let contacts = state.load_contacts(service, password)?;
                let card_store = crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
                    service,
                    crate::state::MARKET_CARD_STORE_KEY,
                    password,
                )
                .map_err(|e| {
                    FrontendError::from(format!(
                        "Failed to load CardStore in collect_flush_context: {}",
                        e
                    ))
                })?;

                for rcpt in &queued_recipients {
                    let mut found_exact = false;
                    for c in &contacts {
                        if c.did.trim() == rcpt {
                            for d in &c.syncs {
                                if is_endpoint_revoked(&d.status) {
                                    continue;
                                }
                                found_exact = true;
                                record_recipient_target(
                                    &mut recipient_write_urls,
                                    &mut recipient_dids_by_write_url,
                                    &mut targets,
                                    rcpt,
                                    d.clone().into(),
                                );
                            }
                        }
                    }
                    for inq in &post_state.inquiries {
                        if inq.counterparty_did.trim() == rcpt {
                            for d in &inq.reply_syncs {
                                if is_endpoint_revoked(&d.status) {
                                    continue;
                                }
                                found_exact = true;
                                record_recipient_target(
                                    &mut recipient_write_urls,
                                    &mut recipient_dids_by_write_url,
                                    &mut targets,
                                    rcpt,
                                    d.clone().into(),
                                );
                            }
                        }
                    }
                    for req in &post_state.pending_sign_requests {
                        if req.sender_did.trim() == rcpt {
                            for d in &req.reply_syncs {
                                if is_endpoint_revoked(&d.status) {
                                    continue;
                                }
                                found_exact = true;
                                record_recipient_target(
                                    &mut recipient_write_urls,
                                    &mut recipient_dids_by_write_url,
                                    &mut targets,
                                    rcpt,
                                    d.clone().into(),
                                );
                            }
                        }
                    }
                    if let Some(ref store) = card_store {
                        if let Some(sc) = store.cards.get(rcpt.as_str()) {
                            for d in sanitize_untrusted_sync_descriptors(
                                &sc.card.payload.drops,
                                humoco_market::MAX_CARD_DROPS,
                            ) {
                                found_exact = true;
                                record_recipient_target(
                                    &mut recipient_write_urls,
                                    &mut recipient_dids_by_write_url,
                                    &mut targets,
                                    rcpt,
                                    d.into(),
                                );
                            }
                        }
                    }
                    if found_exact {
                        exact_matched_recipients.insert(rcpt.clone());
                    }
                }

                for rcpt in &queued_recipients {
                    if exact_matched_recipients.contains(rcpt) {
                        continue;
                    }
                    let can_rcpt =
                        crate::state::canonicalize_did(rcpt).unwrap_or_else(|_| rcpt.clone());
                    for c in &contacts {
                        let can_c = crate::state::canonicalize_did(&c.did)
                            .unwrap_or_else(|_| c.did.clone());
                        if can_c == can_rcpt {
                            for d in &c.syncs {
                                if is_endpoint_revoked(&d.status) {
                                    continue;
                                }
                                record_recipient_target(
                                    &mut recipient_write_urls,
                                    &mut recipient_dids_by_write_url,
                                    &mut targets,
                                    rcpt,
                                    d.clone().into(),
                                );
                            }
                        }
                    }
                    for inq in &post_state.inquiries {
                        let can_inq = crate::state::canonicalize_did(&inq.counterparty_did)
                            .unwrap_or_else(|_| inq.counterparty_did.clone());
                        if can_inq == can_rcpt {
                            for d in &inq.reply_syncs {
                                if is_endpoint_revoked(&d.status) {
                                    continue;
                                }
                                record_recipient_target(
                                    &mut recipient_write_urls,
                                    &mut recipient_dids_by_write_url,
                                    &mut targets,
                                    rcpt,
                                    d.clone().into(),
                                );
                            }
                        }
                    }
                    for req in &post_state.pending_sign_requests {
                        let can_req = crate::state::canonicalize_did(&req.sender_did)
                            .unwrap_or_else(|_| req.sender_did.clone());
                        if can_req == can_rcpt {
                            for d in &req.reply_syncs {
                                if is_endpoint_revoked(&d.status) {
                                    continue;
                                }
                                record_recipient_target(
                                    &mut recipient_write_urls,
                                    &mut recipient_dids_by_write_url,
                                    &mut targets,
                                    rcpt,
                                    d.clone().into(),
                                );
                            }
                        }
                    }
                    if let Some(ref store) = card_store {
                        for sc in store.cards.values() {
                            let can_card = crate::state::canonicalize_did(&sc.card.payload.did)
                                .unwrap_or_else(|_| sc.card.payload.did.clone());
                            if can_card == can_rcpt {
                                for d in sanitize_untrusted_sync_descriptors(
                                    &sc.card.payload.drops,
                                    humoco_market::MAX_CARD_DROPS,
                                ) {
                                    record_recipient_target(
                                        &mut recipient_write_urls,
                                        &mut recipient_dids_by_write_url,
                                        &mut targets,
                                        rcpt,
                                        d.into(),
                                    );
                                }
                            }
                        }
                    }
                }

                let (value_outflow_blocked, blocked_offender_dids) =
                    refresh_outbox_security_guards_with_service(
                        state, service, post_state, password,
                    );

                let mut recipient_targets = Vec::new();
                let mut own_targets = Vec::new();
                let mut seen_urls = std::collections::HashSet::new();
                for t in targets {
                    let norm = normalize_sync_url(&t.write_url);
                    if !norm.is_empty() && seen_urls.insert(norm.clone()) {
                        if recipient_write_urls.contains(&norm) {
                            recipient_targets.push(t);
                        } else {
                            own_targets.push(t);
                        }
                    }
                }
                recipient_targets.extend(own_targets);
                let deduped_targets = recipient_targets;

                Ok(FlushContext {
                    id_ctx,
                    upload_targets: deduped_targets,
                    own_write_urls,
                    recipient_write_urls,
                    recipient_dids_by_write_url,
                    value_outflow_blocked,
                    blocked_offender_dids,
                    now_secs: sync_client::current_unix_timestamp(),
                })
            },
        )
        .map_err(|e| e.message)
}

fn select_prioritized_entries_for_target(
    post_state: &StoredPostState,
    flush_ctx: &FlushContext,
    is_own_target: bool,
    target_recipient_dids: Option<&std::collections::HashSet<String>>,
) -> Vec<PostBoxEntry> {
    let mut small: Vec<&crate::state::PostOutboxEntry> = Vec::new();
    let mut large: Vec<&crate::state::PostOutboxEntry> = Vec::new();
    for entry in &post_state.outbox_entries {
        if entry.message_class.requires_outflow_guard()
            && (flush_ctx.value_outflow_blocked
                || flush_ctx
                    .blocked_offender_dids
                    .contains(entry.recipient_did.trim()))
        {
            continue;
        }
        if !is_own_target {
            if let Some(recipients) = target_recipient_dids {
                if !recipients.contains(entry.recipient_did.trim()) {
                    continue;
                }
            }
        }
        if entry.entry.ciphertext.len() >= 131_072 {
            large.push(entry);
        } else {
            small.push(entry);
        }
    }
    small.sort_by(|a, b| (a.created_at, a.msg_id.as_str()).cmp(&(b.created_at, b.msg_id.as_str())));
    large.sort_by(|a, b| (a.created_at, a.msg_id.as_str()).cmp(&(b.created_at, b.msg_id.as_str())));
    if large.len() > 1 {
        let shift = (post_state.outbox_rotation_tick as usize) % large.len();
        large.rotate_left(shift);
    }
    let mut ordered = Vec::with_capacity(large.len() + small.len());
    ordered.extend(large.into_iter().map(|e| e.entry.clone()));
    ordered.extend(small.into_iter().map(|e| e.entry.clone()));
    ordered
}

fn collect_remove_tags_for_flush(
    post_state: &StoredPostState,
    flush_ctx: &FlushContext,
) -> Vec<[u8; 16]> {
    let mut tags: Vec<[u8; 16]> = Vec::new();
    let mut seen: std::collections::HashSet<[u8; 16]> = std::collections::HashSet::new();
    for hex_str in &post_state.pruned_outbox_tag16s {
        if let Ok(tag) = human_money_core::humoco_proto::decode_hex_fixed::<16>(hex_str) {
            if seen.insert(tag) {
                tags.push(tag);
            }
        }
    }
    for entry in &post_state.outbox_entries {
        if entry.message_class.requires_outflow_guard()
            && (flush_ctx.value_outflow_blocked
                || flush_ctx
                    .blocked_offender_dids
                    .contains(entry.recipient_did.trim()))
            && seen.insert(entry.entry.tag16)
        {
            tags.push(entry.entry.tag16);
        }
    }
    tags
}

type UploadSlotResult = (Option<String>, Vec<[u8; 16]>, Vec<[u8; 16]>, Vec<[u8; 16]>);

#[allow(clippy::too_many_arguments)]
async fn upload_single_post_target(
    client: &reqwest::Client,
    id_ctx: &SyncIdentityContext,
    sync_desc: &DropDescriptor,
    local_entries: &[PostBoxEntry],
    remove_tags: &[[u8; 16]],
    packed: humoco_market::PackedPostSlot,
    if_match: Option<&str>,
    now_secs: u64,
) -> Result<UploadSlotResult, String> {
    let signer = DropChannelSigner {
        is_public: sync_desc.slot_id.is_some(),
        master_seed: &id_ctx.master_seed,
        my_ed_sk: &id_ctx.my_ed_sk,
        drop_desc: sync_desc,
    };
    let id = signer.derive_slot_pubkey()?;
    let nonce = sync_client::generate_sync_nonce();
    let signed = signer.sign_request(
        "PUT",
        "post",
        id_ctx.my_prefix.as_deref(),
        now_secs,
        &nonce,
        &packed.blob,
    )?;

    match sync_client::put_slot(
        client,
        &sync_desc.write_url,
        &signed,
        "post",
        &packed.blob,
        if_match,
    )
    .await
    {
        Ok(PutSlotOutcome::Saved { etag, .. }) => Ok((
            etag,
            packed.included_tags,
            packed.deferred_tags,
            packed.oversized_tags,
        )),
        Ok(PutSlotOutcome::PreconditionFailed { current_etag }) => {
            // Multi-device conflict: fetch remote slot, priority-merge local additions, and retry once.
            let (remote_bytes, fetched_etag) = match sync_client::get_slot(
                client,
                &sync_desc.read_url,
                &id,
                id_ctx.my_prefix.as_deref(),
                "post",
                None,
            )
            .await
            {
                Ok(sync_client::SlotFetchResult::Modified { bytes, etag }) => (Some(bytes), etag),
                Ok(
                    sync_client::SlotFetchResult::NotFound
                    | sync_client::SlotFetchResult::NotModified,
                ) => (None, None),
                Err(e) => {
                    return Err(format!(
                        "Failed to fetch remote slot for 412 merge from {}: {}",
                        sync_desc.read_url, e
                    ));
                }
            };

            let merged = merge_and_pack_post_slot(
                &id_ctx.master_seed,
                remote_bytes.as_deref(),
                local_entries,
                remove_tags,
                DROP_POST_MAX_BYTES,
            )
            .map_err(|e| format!("Failed to merge post slot on 412: {}", e))?;

            let retry_nonce = sync_client::generate_sync_nonce();
            let retry_signed = signer.sign_request(
                "PUT",
                "post",
                id_ctx.my_prefix.as_deref(),
                now_secs,
                &retry_nonce,
                &merged.blob,
            )?;
            let retry_if_match = fetched_etag.as_deref().or(current_etag.as_deref());

            match sync_client::put_slot(
                client,
                &sync_desc.write_url,
                &retry_signed,
                "post",
                &merged.blob,
                retry_if_match,
            )
            .await
            {
                Ok(PutSlotOutcome::Saved { etag, .. }) => Ok((
                    etag,
                    merged.included_tags,
                    merged.deferred_tags,
                    merged.oversized_tags,
                )),
                Ok(PutSlotOutcome::PreconditionFailed { .. }) => Err(format!(
                    "Upload to {} failed 412 Precondition Failed twice",
                    sync_desc.write_url
                )),
                Err(e) => Err(format!(
                    "Upload retry to {} failed: {}",
                    sync_desc.write_url, e
                )),
            }
        }
        Err(e) => Err(format!("Upload to {} failed: {}", sync_desc.write_url, e)),
    }
}

/// Encodes local outbox entries using priority-windowed packing up to `DROP_POST_MAX_BYTES`
/// and uploads them across own and recipient Post slots, performing an automatic 412 read-merge-retry
/// on multi-device ETag conflicts.
pub(crate) async fn flush_post_slots_impl(
    state: &AppState,
    session: &SessionToken,
    post_state: &mut StoredPostState,
    extra_recipient_did: Option<&str>,
    extra_recipient_syncs: &[FrontendSyncDescriptor],
    password: Option<&str>,
) -> FlushPostSlotsOutcome {
    flush_post_slots_with_scope(
        state,
        session,
        post_state,
        extra_recipient_did,
        extra_recipient_syncs,
        password,
        FlushTargetScope::All,
    )
    .await
}

async fn flush_post_slots_with_scope(
    state: &AppState,
    session: &SessionToken,
    post_state: &mut StoredPostState,
    extra_recipient_did: Option<&str>,
    extra_recipient_syncs: &[FrontendSyncDescriptor],
    password: Option<&str>,
    scope: FlushTargetScope,
) -> FlushPostSlotsOutcome {
    let mut flush_ctx = match collect_flush_context(
        state,
        session,
        post_state,
        extra_recipient_did,
        extra_recipient_syncs,
        password,
    ) {
        Ok(ctx) => ctx,
        Err(err) => {
            return FlushPostSlotsOutcome {
                errors: vec![err],
                ..Default::default()
            };
        }
    };

    if flush_ctx.upload_targets.is_empty() {
        return FlushPostSlotsOutcome::default();
    }

    let own_targets_total = flush_ctx
        .upload_targets
        .iter()
        .filter(|d| {
            let norm_url = normalize_sync_url(&d.write_url);
            let is_own = flush_ctx.own_write_urls.contains(&norm_url);
            let is_rcpt = flush_ctx.recipient_write_urls.contains(&norm_url);
            let matches_scope = match scope {
                FlushTargetScope::All => true,
                FlushTargetScope::RecipientOnly => is_rcpt,
                FlushTargetScope::OwnExcludingRecipient => is_own && !is_rcpt,
            };
            is_own && matches_scope
        })
        .count();

    let client = sync_client::create_sync_http_client();
    let mut deposited_to = 0;
    let mut recipient_deposited_to = 0;
    let mut own_targets_succeeded = 0usize;
    let mut errors = Vec::new();
    let mut health_updates: Vec<(String, bool)> = Vec::new();
    let mut recipient_uploaded_tags = std::collections::HashSet::new();
    let mut uploaded_counts_by_tag = std::collections::HashMap::new();
    let mut recipient_uploaded_counts_by_tag = std::collections::HashMap::new();
    let mut all_deferred_tags = std::collections::HashSet::new();
    let mut reported_oversize_tags = std::collections::HashSet::new();

    for sync_desc in &flush_ctx.upload_targets {
        if !state.is_generation_current(session.generation) {
            break;
        }
        let norm_url = normalize_sync_url(&sync_desc.write_url);
        let is_own_target = flush_ctx.own_write_urls.contains(&norm_url);
        let is_recipient_target = flush_ctx.recipient_write_urls.contains(&norm_url);
        match scope {
            FlushTargetScope::All => {}
            FlushTargetScope::RecipientOnly if !is_recipient_target => continue,
            FlushTargetScope::OwnExcludingRecipient if !is_own_target || is_recipient_target => {
                continue
            }
            _ => {}
        }
        if post_state
            .outbox_entries
            .iter()
            .any(|e| e.message_class.requires_outflow_guard())
        {
            match refresh_outbox_security_guards(state, session, post_state, password) {
                Ok((blocked, offenders)) => {
                    flush_ctx.value_outflow_blocked = blocked;
                    flush_ctx.blocked_offender_dids = offenders;
                }
                Err(err) => {
                    errors.push(err);
                    break;
                }
            }
        }
        let if_match = post_state
            .slot_etags_by_write_url
            .get(&norm_url)
            .or_else(|| post_state.slot_etags_by_write_url.get(&sync_desc.write_url))
            .cloned();
        let target_entries = select_prioritized_entries_for_target(
            post_state,
            &flush_ctx,
            is_own_target,
            flush_ctx.recipient_dids_by_write_url.get(&norm_url),
        );
        if target_entries.is_empty() && !is_own_target {
            continue;
        }

        let packed = match pack_post_slot(
            &flush_ctx.id_ctx.master_seed,
            &target_entries,
            DROP_POST_MAX_BYTES,
        ) {
            Ok(p) => p,
            Err(e) => {
                errors.push(format!("Failed to encode post slot: {}", e));
                continue;
            }
        };

        for t in &packed.oversized_tags {
            if reported_oversize_tags.insert(*t) {
                errors.push(format!(
                    "Post outbox entry {} exceeds {} byte slot limit and cannot be deposited",
                    hex::encode(t),
                    DROP_POST_MAX_BYTES
                ));
            }
        }
        if !packed.deferred_tags.is_empty() {
            warn!(
                "Post slot headroom reached for {}: packed={}, deferred={}",
                sync_desc.write_url,
                packed.included_tags.len(),
                packed.deferred_tags.len()
            );
        }

        let remove_tags = collect_remove_tags_for_flush(post_state, &flush_ctx);
        if state.is_origin_cooling_down(&sync_desc.write_url, std::time::Instant::now()) {
            errors.push(format!(
                "Upload to {} deferred: server origin cooling down after HTTP 429",
                sync_desc.write_url
            ));
            continue;
        }
        match upload_single_post_target(
            &client,
            &flush_ctx.id_ctx,
            sync_desc,
            &target_entries,
            &remove_tags,
            packed,
            if_match.as_deref(),
            flush_ctx.now_secs,
        )
        .await
        {
            Ok((etag, included_tags, deferred_tags, oversized_tags)) => {
                deposited_to += 1;
                if is_own_target {
                    own_targets_succeeded += 1;
                }
                if is_recipient_target {
                    recipient_deposited_to += 1;
                }
                for t in &included_tags {
                    *uploaded_counts_by_tag.entry(*t).or_insert(0) += 1;
                    if is_recipient_target {
                        recipient_uploaded_tags.insert(*t);
                        *recipient_uploaded_counts_by_tag.entry(*t).or_insert(0) += 1;
                    }
                }
                for t in deferred_tags {
                    all_deferred_tags.insert(t);
                }
                for t in &oversized_tags {
                    if reported_oversize_tags.insert(*t) {
                        errors.push(format!(
                            "Post outbox entry {} exceeds {} byte slot limit after merge",
                            hex::encode(t),
                            DROP_POST_MAX_BYTES
                        ));
                    }
                }
                health_updates.push((sync_desc.write_url.clone(), true));
                if let Some(et) = etag {
                    post_state
                        .slot_etags_by_write_url
                        .remove(&sync_desc.write_url);
                    post_state.slot_etags_by_write_url.insert(norm_url, et);
                }
            }
            Err(e) => {
                state.record_origin_cooldown_if_429(&sync_desc.write_url, &e);
                health_updates.push((sync_desc.write_url.clone(), false));
                errors.push(e);
            }
        }
    }

    if scope == FlushTargetScope::All
        && own_targets_total > 0
        && own_targets_succeeded == own_targets_total
    {
        post_state.pruned_outbox_tag16s.clear();
    }

    if !health_updates.is_empty() {
        let _ = state.with_session(session, |service| {
            state.record_contact_sync_health_batch(
                service,
                &health_updates,
                flush_ctx.now_secs,
                password,
            );
            Ok(())
        });
    }

    let deferred_tags: std::collections::HashSet<[u8; 16]> = all_deferred_tags
        .into_iter()
        .filter(|t| !uploaded_counts_by_tag.contains_key(t))
        .collect();
    let deferred_count = deferred_tags.len();
    if deferred_count > 0 {
        post_state.outbox_rotation_tick = post_state.outbox_rotation_tick.wrapping_add(1);
    }

    FlushPostSlotsOutcome {
        deposited_to,
        recipient_deposited_to,
        recipient_uploaded_tags,
        uploaded_counts_by_tag,
        recipient_uploaded_counts_by_tag,
        deferred_tags,
        deferred_count,
        errors,
    }
}

async fn persist_and_flush_post_box(
    state: &AppState,
    session: &SessionToken,
    baseline: &StoredPostState,
    post_state: &mut StoredPostState,
    msg_id: String,
    password: Option<&str>,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let target_tag16 = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == msg_id)
        .map(|e| e.entry.tag16);

    // Persist the sealed outbox entry BEFORE network flush so slow or unreachable
    // sync servers never lose an outgoing transfer box if the app closes.
    merge_and_save_post_state(state, session, baseline, post_state, password)?;
    let mid_baseline = post_state.clone();

    let outcome = flush_post_slots_impl(state, session, post_state, None, &[], password).await;
    if !outcome.errors.is_empty() || outcome.deferred_count > 0 {
        warn!(
            "Post deposit status: deposited={}, recipient_deposited={}, deferred={}, failed={}",
            outcome.deposited_to,
            outcome.recipient_deposited_to,
            outcome.deferred_count,
            outcome.errors.len()
        );
    }
    merge_and_save_post_state(state, session, &mid_baseline, post_state, password)?;

    let target_deposited_to = target_tag16
        .and_then(|t| outcome.uploaded_counts_by_tag.get(&t).copied())
        .unwrap_or(0);
    let target_recipient_deposited_to = target_tag16
        .and_then(|t| outcome.recipient_uploaded_counts_by_tag.get(&t).copied())
        .unwrap_or(0);
    let queued_for_next_flush = target_deposited_to == 0
        && target_tag16.is_some_and(|t| outcome.deferred_tags.contains(&t));
    let mut errors = outcome.errors;
    let (deposited_to, recipient_deposited_to) =
        if outcome.deposited_to > 0 && target_deposited_to == 0 && !queued_for_next_flush {
            errors.push(format!(
                "Message {} was excluded or omitted during PoST flush and not deposited",
                msg_id
            ));
            (0, 0)
        } else {
            (target_deposited_to, target_recipient_deposited_to)
        };

    Ok(FrontendPostDepositResult {
        deposited_to,
        recipient_deposited_to,
        msg_id,
        deferred_count: outcome.deferred_count,
        queued_for_next_flush,
        errors,
    })
}

/// Deposits a previously created outgoing transfer bundle (verbatim bytes) into Post slots
/// using an already-authorized session without re-consuming a one-shot PIN grant.
pub(crate) async fn deposit_transfer_to_post_with_session(
    session: &SessionToken,
    bundle_id: String,
    password: Option<&str>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let (record, bundle_bytes) = state.with_session_required(
        session,
        "Active wallet session changed before PoST deposit",
        |service| {
            let history = state
                .load_history_from_disk(service, password)
                .map_err(FrontendError::from)?;
            let record = history
                .into_iter()
                .find(|r| r.bundle_id == bundle_id || r.id == bundle_id)
                .ok_or_else(|| {
                    FrontendError::from(format!(
                        "Transaction record with bundle_id {} not found",
                        bundle_id
                    ))
                })?;
            let recipient_refs = [record.recipient_id.as_str()];
            let preflight = crate::services::preflight::ValueOutflowPreflight {
                require_restore_reconciled: true,
                check_known_offender_dids: &recipient_refs,
                check_blocking_intent_local_ids: &[],
                check_anchor_id_quorum_health: None,
                preverified_anchor_manifest: None,
            };
            crate::services::preflight::run_value_outflow_preflight(
                state, service, password, &preflight,
            )?;
            let bundle_bytes =
                crate::state::get_outgoing_bundle_bytes(service, &record.bundle_id, password)?
                    .filter(|b| !b.is_empty())
                    .ok_or_else(|| {
                        FrontendError::from(
                            "Outgoing bundle bytes not found (expired or not retained)".to_string(),
                        )
                    })?;
            Ok((record, bundle_bytes))
        },
    )?;

    let _post_guard = state.post_op_lock.lock().await;
    let mut post_state = load_post_state_for_session(state, session, password)?;
    let baseline = post_state.clone();

    let msg_id = if let Some(existing) = post_state.outbox_entries.iter().find(|e| {
        e.bundle_id.as_deref() == Some(&bundle_id)
            || e.bundle_id.as_deref() == Some(&record.bundle_id)
    }) {
        existing.msg_id.clone()
    } else {
        let validated_recipient =
            ValidatedUserId::parse(&record.recipient_id).map_err(FrontendError::from)?;
        let recipient_ed_pk = validated_recipient.public_key();
        let signing_ctx = load_post_signing_context_for_session(state, session, password)?;
        let now_secs = sync_client::current_unix_timestamp();
        let fresh_msg_id = uuid::Uuid::new_v4().to_string();

        enqueue_post_envelope(
            &mut post_state,
            &signing_ctx.identity.user_id,
            &signing_ctx.sync_ctx.my_ed_sk,
            validated_recipient.as_str(),
            recipient_ed_pk,
            fresh_msg_id.clone(),
            Some(record.bundle_id),
            crate::state::OutboxMessageClass::ValueTransfer,
            &signing_ctx.own_reply_syncs,
            bundle_bytes, // EXACT bytes! Never call create_transfer_bundle!
            now_secs,
        )?;
        fresh_msg_id
    };

    persist_and_flush_post_box(state, session, &baseline, &mut post_state, msg_id, password).await
}

/// Deposits a previously created outgoing transfer bundle (verbatim bytes) into Post slots.
pub async fn deposit_transfer_to_post_impl(
    bundle_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    deposit_transfer_to_post_with_session(&session, bundle_id, pw, state).await
}

/// Sends a voucher co-signing request container to a guarantor via Post.
///
/// The `container_bytes` payload is already a signed, AEAD-encrypted Core `SecureContainer`
/// (`SigningRequestBundle`) carrying the voucher together with its anchor manifest and
/// delegations inside the ciphertext.
pub async fn send_signing_request_via_post_impl(
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let validated_recipient =
        ValidatedUserId::parse(&recipient_did).map_err(FrontendError::from)?;
    let recipient_did = validated_recipient.as_str().to_string();
    let recipient_ed_pk = validated_recipient.public_key();
    let signing_ctx = load_post_signing_context_for_session(state, &session, pw)?;
    verify_outgoing_post_container(
        &container_bytes,
        PayloadType::VoucherForSigning,
        &signing_ctx.identity.user_id,
        "Signing request",
    )?;
    let now_secs = sync_client::current_unix_timestamp();

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state_for_session(state, &session, pw)?;
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
    )?;

    persist_and_flush_post_box(state, &session, &baseline, &mut post_state, msg_id, pw).await
}

/// Replies with a detached voucher signature container via Post and clears any matching request.
pub async fn reply_detached_signature_via_post_impl(
    request_msg_id: Option<String>,
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::Full,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let validated_recipient =
        ValidatedUserId::parse(&recipient_did).map_err(FrontendError::from)?;
    let recipient_did = validated_recipient.as_str().to_string();
    let recipient_ed_pk = validated_recipient.public_key();
    let signing_ctx = state.with_session_required(
        &session,
        "Active wallet session changed before detached signature reply could be prepared",
        |service| {
            if let Some(reason) = service
                .get_seal_lock_reason(pw)
                .map_err(FrontendError::from)?
            {
                return Err(map_seal_lock_error(reason));
            }
            let signing_ctx = load_post_signing_context_for_service(state, service, pw)?;
            let offender_dids = [
                recipient_did.as_str(),
                signing_ctx.identity.user_id.as_str(),
            ];
            let preflight = crate::services::preflight::ValueOutflowPreflight {
                require_restore_reconciled: true,
                check_known_offender_dids: &offender_dids,
                check_blocking_intent_local_ids: &[],
                check_anchor_id_quorum_health: None,
                preverified_anchor_manifest: None,
            };
            crate::services::preflight::run_value_outflow_preflight(
                state, service, pw, &preflight,
            )?;
            Ok(signing_ctx)
        },
    )?;
    verify_outgoing_post_container(
        &container_bytes,
        PayloadType::DetachedSignature,
        &signing_ctx.identity.user_id,
        "Detached signature reply",
    )?;
    let now_secs = sync_client::current_unix_timestamp();

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state_for_session(state, &session, pw)?;
    if let Some(ref req_id) = request_msg_id {
        let scoped_req_id = scoped_processed_msg_key(&recipient_did, req_id);
        if post_state.replied_sign_request_ids.contains(&scoped_req_id)
            || post_state.replied_sign_request_ids.contains(req_id)
        {
            return Err(FrontendError::with_code(
                "error.post.duplicateReply",
                "This signature request has already been answered.".to_string(),
            ));
        }
    }
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        crate::state::OutboxMessageClass::ClaimBearing,
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
    )?;
    let target_tag16 = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == msg_id)
        .map(|e| e.entry.tag16);

    merge_and_save_post_state(state, &session, &baseline, &mut post_state, pw)?;
    let mid_baseline = post_state.clone();

    let outcome = flush_post_slots_impl(state, &session, &mut post_state, None, &[], pw).await;
    let target_deposited_to = target_tag16
        .and_then(|t| outcome.uploaded_counts_by_tag.get(&t).copied())
        .unwrap_or(0);
    let target_recipient_deposited_to = target_tag16
        .and_then(|t| outcome.recipient_uploaded_counts_by_tag.get(&t).copied())
        .unwrap_or(0);
    let queued_for_next_flush = target_deposited_to == 0
        && target_tag16.is_some_and(|t| outcome.deferred_tags.contains(&t));
    let mut errors = outcome.errors;
    if outcome.deposited_to > 0 && target_deposited_to == 0 && !queued_for_next_flush {
        errors.push(format!(
            "Detached signature box {} was excluded or omitted during PoST flush and not deposited",
            msg_id
        ));
    }
    let delivered_any = target_deposited_to > 0;

    if delivered_any {
        if let Some(ref req_id) = request_msg_id {
            let scoped_req_id = scoped_processed_msg_key(&recipient_did, req_id);
            if post_state.replied_sign_request_ids.len() >= 512 {
                if let Some(first) = post_state.replied_sign_request_ids.iter().next().cloned() {
                    post_state.replied_sign_request_ids.remove(&first);
                }
            }
            post_state.replied_sign_request_ids.insert(scoped_req_id);
            if let Some(idx) = post_state
                .pending_sign_requests
                .iter()
                .position(|r| &r.msg_id == req_id && are_dids_equal(&r.sender_did, &recipient_did))
                .or_else(|| {
                    post_state
                        .pending_sign_requests
                        .iter()
                        .position(|r| &r.msg_id == req_id)
                })
            {
                let req = post_state.pending_sign_requests.remove(idx);
                let sanitized_replies = sanitize_untrusted_frontend_sync_descriptors(
                    &req.reply_syncs,
                    humoco_market::MAX_CARD_DROPS,
                );
                if !sanitized_replies.is_empty() {
                    state.with_session_required(
                        &session,
                        "Active wallet session changed before reply syncs could be updated",
                        |service| {
                            if let Err(e) = state.update_contact_from_card(
                                service,
                                &recipient_did,
                                &sanitized_replies,
                                None,
                                pw,
                            ) {
                                warn!(
                                    "Failed to update contact from signing request reply syncs: {}",
                                    e.message
                                );
                            }
                            Ok(())
                        },
                    )?;
                }
            }
        }
    }

    merge_and_save_post_state(state, &session, &mid_baseline, &mut post_state, pw)?;

    Ok(FrontendPostDepositResult {
        deposited_to: target_deposited_to,
        recipient_deposited_to: target_recipient_deposited_to,
        msg_id,
        deferred_count: outcome.deferred_count,
        queued_for_next_flush,
        errors,
    })
}

/// Sends a WoT trust handshake assertion with an embedded `WotSyncPackage` via Post.
#[cfg(test)]
pub async fn send_wot_handshake_via_post_impl(
    recipient_did: String,
    proposed_level: u8,
    reply_requested: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    send_wot_handshake_via_post_with_session(
        &session,
        recipient_did,
        proposed_level,
        reply_requested,
        pw,
        state,
    )
    .await
}

pub(crate) async fn send_wot_handshake_via_post_with_session(
    session: &SessionToken,
    recipient_did: String,
    proposed_level: u8,
    reply_requested: bool,
    pw: Option<&str>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    if proposed_level > 1 {
        return Err(FrontendError::from(format!(
            "proposed_level must be 0 or 1, got {}",
            proposed_level
        )));
    }

    let validated_recipient =
        ValidatedUserId::parse(&recipient_did).map_err(FrontendError::from)?;
    let recipient_did = validated_recipient.as_str().to_string();
    let recipient_ed_pk = validated_recipient.public_key();

    let (my_identity, my_ed_sk, own_reply_syncs, assertion, now_secs) = state
        .with_session_required(
            session,
            "Wallet session changed before preparing WoT handshake",
            |service| {
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
                    .get_or_load_wot_register(service, pw)
                    .map_err(FrontendError::from)?;

                if reg.own_list_dirty {
                    state
                        .ensure_wot_list_signed(service, &mut reg)
                        .map_err(FrontendError::from)?;
                    state
                        .save_wot_register(service, &reg, pw)
                        .map_err(FrontendError::from)?;
                }

                let can_recipient = crate::state::canonicalize_did(&recipient_did)
                    .unwrap_or_else(|_| recipient_did.clone());

                let (pkg, _receipt) = reg
                    .prepare_sync_package(Some(&can_recipient), None, true)
                    .map_err(FrontendError::from)?;
                let wot_sync_json = serde_json::to_string(&pkg).map_err(|e| {
                    FrontendError::from(format!("Failed to serialize sync package: {}", e))
                })?;

                let security_policy = state
                    .get_or_load_security_policy(service, pw)
                    .map_err(FrontendError::from)?;
                let reply_syncs = collect_own_reply_syncs(
                    &security_policy,
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

                Ok((my_identity, my_ed_sk, reply_syncs, assertion, now_secs))
            },
        )?;

    let container_bytes = seal_post_container_bytes(
        &my_identity,
        &recipient_did,
        PayloadType::TrustAssertion,
        &assertion,
        "assertion",
    )?;

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state_for_session(state, session, pw)?;
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &my_identity.user_id,
        &my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        crate::state::OutboxMessageClass::Control,
        &own_reply_syncs,
        container_bytes,
        now_secs,
    )?;

    persist_and_flush_post_box(state, session, &baseline, &mut post_state, msg_id, pw).await
}

/// Accepts a pending incoming WoT handshake, updates contact trust, imports the peer's
/// WoT package, and optionally sends a mutual return handshake.
pub async fn accept_wot_handshake_impl(
    msg_id: String,
    trust_level: u8,
    send_return_handshake: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    if trust_level > 3 {
        return Err(FrontendError::from(format!(
            "trust_level must be 0..=3, got {}",
            trust_level
        )));
    }

    let sender_did = {
        let _post_guard = state.post_op_lock.lock().await;
        let now_secs = sync_client::current_unix_timestamp();

        state.with_session_required(
            &session,
            "Active wallet session changed before WoT handshake could be saved",
            |service| {
                let mut post_state = load_post_state_with_service(service, pw)?;
                let baseline = post_state.clone();
                let idx = post_state
                    .pending_wot_handshakes
                    .iter()
                    .position(|h| {
                        h.msg_id == msg_id
                            || scoped_processed_msg_key(&h.sender_did, &h.msg_id) == msg_id
                    })
                    .ok_or_else(|| {
                        FrontendError::from(format!(
                            "Pending WoT handshake msg_id {} not found",
                            msg_id
                        ))
                    })?;
                let handshake = post_state.pending_wot_handshakes.remove(idx);
                let sender_did = handshake.sender_did.clone();
                let raw_msg_id = handshake.msg_id;
                let scoped_pkg_key = scoped_processed_msg_key(&sender_did, &raw_msg_id);
                let pkg_opt = post_state
                    .pending_wot_packages_by_msg_id
                    .remove(&scoped_pkg_key)
                    .or_else(|| post_state.pending_wot_packages_by_msg_id.remove(&raw_msg_id))
                    .and_then(|json_str| {
                        serde_json::from_str::<humoco_wot::WotSyncPackage>(&json_str).ok()
                    });

                let contacts = state.load_contacts(service, pw).map_err(FrontendError::from)?;
                let mut contact = contacts
                    .iter()
                    .find(|c| c.did.trim() == sender_did.trim())
                    .or_else(|| contacts.iter().find(|c| are_dids_equal(&c.did, &sender_did)))
                    .cloned()
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
                    });
                contact.trust_level = Some(trust_level);

                state
                    .save_contact_with_companions(
                        service,
                        contact,
                        pw,
                        false,
                        |svc, updated_wot_reg, batch| {
                            if let Some(ref pkg) = pkg_opt {
                                let reg = match updated_wot_reg {
                                    Some(r) => r,
                                    None => {
                                        let loaded = state.get_or_load_wot_register(svc, pw)?;
                                        *updated_wot_reg = Some(loaded);
                                        updated_wot_reg.as_mut().unwrap()
                                    }
                                };
                                let can_sender_did = crate::state::canonicalize_did(&sender_did)
                                    .unwrap_or_else(|_| sender_did.clone());
                                if let Err(e) =
                                    reg.import_sync_package(pkg, Some(&can_sender_did), now_secs)
                                {
                                    warn!(
                                        "Rejected invalid WoT sync package while accepting handshake from {}: {}",
                                        sender_did, e
                                    );
                                }
                            }
                            merge_post_state_with_service(svc, &baseline, &mut post_state, pw)
                                .map_err(|e| {
                                    human_money_core::app_service::AppFacadeError::App(
                                        human_money_core::error::AppError::StateConflict {
                                            message: e.message,
                                        },
                                    )
                                })?;
                            batch.post_state(post_state);
                            Ok(())
                        },
                    )
                    .map_err(FrontendError::from)?;

                Ok(sender_did)
            },
        )?
    };

    if send_return_handshake {
        send_wot_handshake_via_post_with_session(
            &session,
            sender_did,
            trust_level.min(1),
            false,
            pw,
            state,
        )
        .await
    } else {
        Ok(FrontendPostDepositResult {
            deposited_to: 0,
            recipient_deposited_to: 0,
            msg_id,
            deferred_count: 0,
            queued_for_next_flush: false,
            errors: Vec::new(),
        })
    }
}

/// Initiates a 1-step marketplace inquiry thread to a seller or person via Post.
pub async fn send_market_inquiry_via_post_impl(
    recipient_did: String,
    target: humoco_market::InquiryTarget,
    message: String,
    recipient_syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let validated_recipient =
        ValidatedUserId::parse(&recipient_did).map_err(FrontendError::from)?;
    let recipient_did = validated_recipient.as_str().to_string();
    let recipient_ed_pk = validated_recipient.public_key();
    let now_secs = sync_client::current_unix_timestamp();
    let inquiry_id = uuid::Uuid::new_v4().to_string();

    let inquiry = MarketInquiry {
        inquiry_id: inquiry_id.clone(),
        target: target.clone(),
        message: message.clone(),
        created_at: now_secs,
    };
    validate_market_inquiry(&inquiry).map_err(FrontendError::from)?;
    let recipient_syncs = sanitize_untrusted_frontend_sync_descriptors(
        &recipient_syncs,
        humoco_market::MAX_CARD_DROPS,
    );

    let signing_ctx = load_post_signing_context_for_session(state, &session, pw)?;

    let container_bytes = seal_post_container_bytes(
        &signing_ctx.identity,
        &recipient_did,
        PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_V1.to_string()),
        &inquiry,
        "inquiry",
    )?;

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state_for_session(state, &session, pw)?;
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
    )?;
    let target_tag16 = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == msg_id)
        .map(|e| e.entry.tag16);

    let flush_outcome = flush_post_slots_with_scope(
        state,
        &session,
        &mut post_state,
        Some(&recipient_did),
        &recipient_syncs,
        pw,
        FlushTargetScope::RecipientOnly,
    )
    .await;
    if !target_tag16.is_some_and(|t| flush_outcome.recipient_uploaded_tags.contains(&t)) {
        return Err(FrontendError::with_code(
            "error.post.recipientUnreachable",
            format!(
                "Recipient sync inbox unreachable for marketplace inquiry. Errors: {:?}",
                flush_outcome.errors
            ),
        ));
    }

    if !recipient_syncs.is_empty() {
        state.with_session_required(
            &session,
            "Wallet session changed before saving inquiry recipient syncs",
            |service| {
                if let Err(e) = state.update_contact_from_card(
                    service,
                    &recipient_did,
                    &recipient_syncs,
                    None,
                    pw,
                ) {
                    warn!(
                        "Failed to save inquiry recipient syncs on contact: {}",
                        e.message
                    );
                }
                Ok(())
            },
        )?;
    }

    let (counterparty_name, counterparty_picture_url, is_contact) =
        resolve_counterparty_info_for_session(state, &session, &recipient_did, pw);

    let item = FrontendMarketInquiryItem {
        inquiry_id,
        role: "buyer".to_string(),
        target,
        counterparty_did: recipient_did.clone(),
        counterparty_name,
        counterparty_picture_url,
        is_contact,
        status: "open".to_string(),
        payment_uri: None,
        amount_hint: None,
        standard_uuid: None,
        created_at: now_secs,
        updated_at: now_secs,
        unread: false,
        reply_syncs: recipient_syncs.clone(),
        messages: vec![FrontendInquiryChatMessage {
            msg_id,
            sender: "me".to_string(),
            text: Some(message),
            decision: None,
            created_at: now_secs,
        }],
    };
    post_state.inquiries.push(item.clone());
    merge_and_save_post_state(state, &session, &baseline, &mut post_state, pw)?;

    let own_baseline = post_state.clone();
    let _own_outcome = flush_post_slots_with_scope(
        state,
        &session,
        &mut post_state,
        Some(&recipient_did),
        &recipient_syncs,
        pw,
        FlushTargetScope::OwnExcludingRecipient,
    )
    .await;
    if post_state.slot_etags_by_write_url != own_baseline.slot_etags_by_write_url
        || post_state.outbox_rotation_tick != own_baseline.outbox_rotation_tick
    {
        let _ = merge_and_save_post_state(state, &session, &own_baseline, &mut post_state, pw);
    }

    Ok(item)
}

/// Appends a chat message and/or seller decision (`confirm`/`decline`) to an existing inquiry thread.
#[allow(clippy::too_many_arguments)]
pub async fn send_market_inquiry_message_via_post_impl(
    inquiry_id: String,
    text: Option<String>,
    decision: Option<String>,
    payment_uri: Option<String>,
    amount_hint: Option<String>,
    standard_uuid: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketInquiryItem, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let dec = match decision
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("confirm") => Some(MarketInquiryDecision::Confirm),
        Some("decline") => Some(MarketInquiryDecision::Decline),
        Some(other) => return Err(FrontendError::from(format!("Invalid decision: {}", other))),
        None => None,
    };

    let clean_text = text.and_then(|s| {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let _post_guard = state.post_op_lock.lock().await;
    let now_secs = sync_client::current_unix_timestamp();
    let mut post_state = load_post_state_for_session(state, &session, pw)?;
    let baseline = post_state.clone();

    let (item_idx, counterparty_did, target, has_reply_syncs) = {
        let (idx, item) = post_state
            .inquiries
            .iter()
            .enumerate()
            .find(|(_, i)| i.inquiry_id == inquiry_id)
            .ok_or_else(|| FrontendError::from(format!("Inquiry {} not found", inquiry_id)))?;

        if item.messages.len() >= MAX_INQUIRY_MESSAGES_PER_THREAD {
            return Err(FrontendError::from(format!(
                "Inquiry thread reached maximum message limit ({})",
                MAX_INQUIRY_MESSAGES_PER_THREAD
            )));
        }
        if dec.is_some() {
            if item.role != "seller" {
                return Err(FrontendError::from(
                    "Only the seller can confirm or decline an inquiry".to_string(),
                ));
            }
            if item.status != "open" {
                return Err(FrontendError::from(
                    "Inquiry status has already been decided".to_string(),
                ));
            }
        }
        (
            idx,
            item.counterparty_did.clone(),
            item.target.clone(),
            !item.reply_syncs.is_empty(),
        )
    };

    if !has_reply_syncs {
        let discovered_syncs = lookup_counterparty_syncs(state, &session, &counterparty_did, pw);
        if discovered_syncs.is_empty() {
            return Err(FrontendError::from(
                "Counterparty has no sync service configured for replies".to_string(),
            ));
        }
        post_state.inquiries[item_idx].reply_syncs = discovered_syncs;
    }

    let (wire_payment_uri, wire_amount_hint, wire_standard_uuid) =
        if dec == Some(MarketInquiryDecision::Confirm) {
            (payment_uri, amount_hint, standard_uuid)
        } else {
            (None, None, None)
        };

    let msg_id = uuid::Uuid::new_v4().to_string();
    let msg_payload = MarketInquiryMessage {
        inquiry_id: inquiry_id.clone(),
        msg_id: msg_id.clone(),
        text: clean_text.clone(),
        decision: dec,
        payment_uri: wire_payment_uri.clone(),
        amount_hint: wire_amount_hint.clone(),
        standard_uuid: wire_standard_uuid.clone(),
        created_at: now_secs,
    };
    validate_market_inquiry_message(&msg_payload).map_err(FrontendError::from)?;

    let validated_counterparty =
        ValidatedUserId::parse(&counterparty_did).map_err(FrontendError::from)?;
    let counterparty_did = validated_counterparty.as_str().to_string();
    let recipient_ed_pk = validated_counterparty.public_key();

    let signing_ctx = load_post_signing_context_for_session(state, &session, pw)?;

    let container_bytes = seal_post_container_bytes(
        &signing_ctx.identity,
        &counterparty_did,
        PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_MESSAGE_V1.to_string()),
        &msg_payload,
        "inquiry message",
    )?;

    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &counterparty_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
    )?;
    let target_tag16 = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == msg_id)
        .map(|e| e.entry.tag16);

    let flush_outcome = flush_post_slots_with_scope(
        state,
        &session,
        &mut post_state,
        None,
        &[],
        pw,
        FlushTargetScope::RecipientOnly,
    )
    .await;
    if !target_tag16.is_some_and(|t| flush_outcome.recipient_uploaded_tags.contains(&t)) {
        return Err(FrontendError::with_code(
            "error.post.recipientUnreachable",
            format!(
                "Recipient sync inbox unreachable for inquiry message. Errors: {:?}",
                flush_outcome.errors
            ),
        ));
    }

    if dec.is_some() {
        post_state.inquiry_rate_tracker.resolve_inquiry(
            &hex::encode(recipient_ed_pk.as_bytes()),
            &inquiry_rate_limit_key(&target),
        );
    }

    let dec_str = dec.map(|d| match d {
        MarketInquiryDecision::Confirm => "confirm".to_string(),
        MarketInquiryDecision::Decline => "decline".to_string(),
    });

    let item = &mut post_state.inquiries[item_idx];
    item.messages.push(FrontendInquiryChatMessage {
        msg_id,
        sender: "me".to_string(),
        text: clean_text,
        decision: dec_str.clone(),
        created_at: now_secs,
    });
    if let Some(ref d) = dec_str {
        if d == "confirm" {
            item.status = "confirmed".to_string();
            if wire_payment_uri.is_some() {
                item.payment_uri = wire_payment_uri;
            }
            if wire_amount_hint.is_some() {
                item.amount_hint = wire_amount_hint;
            }
            if wire_standard_uuid.is_some() {
                item.standard_uuid = wire_standard_uuid;
            }
        } else {
            item.status = "declined".to_string();
        }
    }
    item.updated_at = now_secs;
    item.unread = false;

    let mut updated_item = item.clone();
    merge_and_save_post_state(state, &session, &baseline, &mut post_state, pw)?;

    let own_baseline = post_state.clone();
    let _own_outcome = flush_post_slots_with_scope(
        state,
        &session,
        &mut post_state,
        None,
        &[],
        pw,
        FlushTargetScope::OwnExcludingRecipient,
    )
    .await;
    if post_state.slot_etags_by_write_url != own_baseline.slot_etags_by_write_url
        || post_state.outbox_rotation_tick != own_baseline.outbox_rotation_tick
    {
        let _ = merge_and_save_post_state(state, &session, &own_baseline, &mut post_state, pw);
    }

    let (name, pic, is_contact) =
        resolve_counterparty_info_for_session(state, &session, &updated_item.counterparty_did, pw);
    updated_item.counterparty_name = name;
    updated_item.counterparty_picture_url = pic;
    updated_item.is_contact = is_contact;

    Ok(updated_item)
}
