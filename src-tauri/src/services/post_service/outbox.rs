use chrono::Utc;
use human_money_core::models::secure_container::PayloadType;
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
use crate::services::common::{normalize_sync_url, DropChannelSigner, SyncIdentityContext};
use crate::state::{AppState, StoredPostState};
use crate::sync_client::{self as sync_client, PutSlotOutcome};

use super::{
    collect_own_reply_syncs, collect_post_upload_targets, enqueue_post_envelope,
    load_post_signing_context, load_post_state, lookup_counterparty_syncs,
    merge_and_save_post_state, resolve_counterparty_info, seal_post_container_bytes,
};

struct FlushContext {
    id_ctx: SyncIdentityContext,
    upload_targets: Vec<DropDescriptor>,
    own_write_urls: std::collections::HashSet<String>,
    recipient_write_urls: std::collections::HashSet<String>,
    recipient_dids_by_write_url:
        std::collections::HashMap<String, std::collections::HashSet<String>>,
    now_secs: u64,
}

#[derive(Debug, Default)]
pub(crate) struct FlushPostSlotsOutcome {
    pub deposited_to: usize,
    pub recipient_deposited_to: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    pub uploaded_tags: std::collections::HashSet<[u8; 16]>,
    pub recipient_uploaded_tags: std::collections::HashSet<[u8; 16]>,
    pub deferred_tags: std::collections::HashSet<[u8; 16]>,
    pub deferred_count: usize,
    pub errors: Vec<String>,
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

fn collect_flush_context(
    state: &AppState,
    post_state: &StoredPostState,
    extra_recipient_did: Option<&str>,
    extra_recipient_syncs: &[FrontendSyncDescriptor],
    password: Option<&str>,
) -> Result<FlushContext, String> {
    let mut service = state.service.lock().unwrap();
    let id_ctx = SyncIdentityContext::from_service(&service)
        .map_err(|e| format!("Failed to access wallet signing key: {}", e.message))?;
    let security_policy = state
        .get_or_load_security_policy(&mut service, password)
        .map_err(|e| format!("Failed to load security policy: {}", e))?;
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
            if let Some(rcpt) = extra_trimmed {
                exact_matched_recipients.insert(rcpt.to_string());
                record_recipient_target(
                    &mut recipient_write_urls,
                    &mut recipient_dids_by_write_url,
                    &mut targets,
                    rcpt,
                    d.clone().into(),
                );
            } else {
                let norm = normalize_sync_url(&d.write_url);
                if !norm.is_empty() {
                    recipient_write_urls.insert(norm);
                    targets.push(d.clone().into());
                }
            }
        }
    }

    // Include recipient Sync endpoints from contacts, inquiries, pending sign requests, and known cards.
    // Match by exact prefixed recipient_did first; fall back to canonical root DID only when no exact target exists.
    let queued_recipients: std::collections::HashSet<String> = post_state
        .outbox_entries
        .iter()
        .map(|e| e.recipient_did.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let contacts = state
        .load_contacts(&mut service, password)
        .unwrap_or_default();
    let card_store = crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
        &mut service,
        crate::state::MARKET_CARD_STORE_KEY,
        password,
    )
    .map_err(|e| format!("Failed to load CardStore in collect_flush_context: {}", e))?;

    for rcpt in &queued_recipients {
        let mut found_exact = false;
        for c in &contacts {
            if c.did.trim() == rcpt {
                for d in &c.syncs {
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
                for d in &sc.card.payload.drops {
                    found_exact = true;
                    record_recipient_target(
                        &mut recipient_write_urls,
                        &mut recipient_dids_by_write_url,
                        &mut targets,
                        rcpt,
                        d.clone(),
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
        let can_rcpt = crate::state::canonicalize_did(rcpt).unwrap_or_else(|_| rcpt.clone());
        for c in &contacts {
            let can_c = crate::state::canonicalize_did(&c.did).unwrap_or_else(|_| c.did.clone());
            if can_c == can_rcpt {
                for d in &c.syncs {
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
                    for d in &sc.card.payload.drops {
                        record_recipient_target(
                            &mut recipient_write_urls,
                            &mut recipient_dids_by_write_url,
                            &mut targets,
                            rcpt,
                            d.clone(),
                        );
                    }
                }
            }
        }
    }

    // Deduplicate targets by normalized write_url.
    let mut deduped_targets = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();
    for t in targets {
        let norm = normalize_sync_url(&t.write_url);
        if !norm.is_empty() && seen_urls.insert(norm) {
            deduped_targets.push(t);
        }
    }

    Ok(FlushContext {
        id_ctx,
        upload_targets: deduped_targets,
        own_write_urls,
        recipient_write_urls,
        recipient_dids_by_write_url,
        now_secs: sync_client::current_unix_timestamp(),
    })
}

fn select_prioritized_entries_for_target(
    post_state: &StoredPostState,
    is_own_target: bool,
    target_recipient_dids: Option<&std::collections::HashSet<String>>,
) -> Vec<PostBoxEntry> {
    let mut selected: Vec<&crate::state::PostOutboxEntry> = post_state
        .outbox_entries
        .iter()
        .filter(|entry| {
            if is_own_target {
                return true;
            }
            let Some(recipients) = target_recipient_dids else {
                return true;
            };
            recipients.contains(entry.recipient_did.trim())
        })
        .collect();
    selected
        .sort_by(|a, b| (a.created_at, a.msg_id.as_str()).cmp(&(b.created_at, b.msg_id.as_str())));
    selected.into_iter().map(|e| e.entry.clone()).collect()
}

type UploadSlotResult = (Option<String>, Vec<[u8; 16]>, Vec<[u8; 16]>, Vec<[u8; 16]>);

async fn upload_single_post_target(
    client: &reqwest::Client,
    id_ctx: &SyncIdentityContext,
    sync_desc: &DropDescriptor,
    local_entries: &[PostBoxEntry],
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
            let remote_bytes = match sync_client::get_slot(
                client,
                &sync_desc.read_url,
                &id,
                id_ctx.my_prefix.as_deref(),
                "post",
                None,
            )
            .await
            {
                Ok(sync_client::SlotFetchResult::Modified { bytes, .. }) => Some(bytes),
                Ok(
                    sync_client::SlotFetchResult::NotFound
                    | sync_client::SlotFetchResult::NotModified,
                ) => None,
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
                &[],
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

            match sync_client::put_slot(
                client,
                &sync_desc.write_url,
                &retry_signed,
                "post",
                &merged.blob,
                current_etag.as_deref(),
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
    post_state: &mut StoredPostState,
    extra_recipient_did: Option<&str>,
    extra_recipient_syncs: &[FrontendSyncDescriptor],
    password: Option<&str>,
) -> FlushPostSlotsOutcome {
    let flush_ctx = match collect_flush_context(
        state,
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

    let client = sync_client::create_sync_http_client();
    let mut deposited_to = 0;
    let mut recipient_deposited_to = 0;
    let mut errors = Vec::new();
    let mut health_updates: Vec<(String, bool)> = Vec::new();
    let mut uploaded_tags = std::collections::HashSet::new();
    let mut recipient_uploaded_tags = std::collections::HashSet::new();
    let mut all_deferred_tags = std::collections::HashSet::new();
    let mut reported_oversize_tags = std::collections::HashSet::new();

    for sync_desc in &flush_ctx.upload_targets {
        let if_match = post_state
            .slot_etags_by_write_url
            .get(&sync_desc.write_url)
            .cloned();
        let norm_url = normalize_sync_url(&sync_desc.write_url);
        let is_own_target = flush_ctx.own_write_urls.contains(&norm_url);
        let is_recipient_target = flush_ctx.recipient_write_urls.contains(&norm_url);
        let target_entries = select_prioritized_entries_for_target(
            post_state,
            is_own_target,
            flush_ctx.recipient_dids_by_write_url.get(&norm_url),
        );

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

        match upload_single_post_target(
            &client,
            &flush_ctx.id_ctx,
            sync_desc,
            &target_entries,
            packed,
            if_match.as_deref(),
            flush_ctx.now_secs,
        )
        .await
        {
            Ok((etag, included_tags, deferred_tags, oversized_tags)) => {
                deposited_to += 1;
                if is_recipient_target {
                    recipient_deposited_to += 1;
                }
                for t in &included_tags {
                    uploaded_tags.insert(*t);
                    if is_recipient_target {
                        recipient_uploaded_tags.insert(*t);
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
                        .insert(sync_desc.write_url.clone(), et);
                }
            }
            Err(e) => {
                health_updates.push((sync_desc.write_url.clone(), false));
                errors.push(e);
            }
        }
    }

    if !health_updates.is_empty() {
        let mut service = state.service.lock().unwrap();
        state.record_contact_sync_health_batch(
            &mut service,
            &health_updates,
            flush_ctx.now_secs,
            password,
        );
    }

    let deferred_tags: std::collections::HashSet<[u8; 16]> = all_deferred_tags
        .into_iter()
        .filter(|t| !uploaded_tags.contains(t))
        .collect();
    let deferred_count = deferred_tags.len();

    FlushPostSlotsOutcome {
        deposited_to,
        recipient_deposited_to,
        uploaded_tags,
        recipient_uploaded_tags,
        deferred_tags,
        deferred_count,
        errors,
    }
}

async fn persist_and_flush_post_box(
    state: &AppState,
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
    merge_and_save_post_state(state, baseline, post_state, password)?;
    let mid_baseline = post_state.clone();

    let outcome = flush_post_slots_impl(state, post_state, None, &[], password).await;
    if !outcome.errors.is_empty() || outcome.deferred_count > 0 {
        warn!(
            "Post deposit status: deposited={}, recipient_deposited={}, deferred={}, failed={}",
            outcome.deposited_to,
            outcome.recipient_deposited_to,
            outcome.deferred_count,
            outcome.errors.len()
        );
    }
    merge_and_save_post_state(state, &mid_baseline, post_state, password)?;

    let queued_for_next_flush = target_tag16.is_some_and(|t| outcome.deferred_tags.contains(&t));

    Ok(FrontendPostDepositResult {
        deposited_to: outcome.deposited_to,
        recipient_deposited_to: outcome.recipient_deposited_to,
        msg_id,
        deferred_count: outcome.deferred_count,
        queued_for_next_flush,
        errors: outcome.errors,
    })
}

/// Deposits a previously created outgoing transfer bundle (verbatim bytes) into Post slots.
pub async fn deposit_transfer_to_post_impl(
    bundle_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    let record = {
        let mut service = state
            .authorize_and_lock(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
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

    let _post_guard = state.post_op_lock.lock().await;
    let mut post_state = load_post_state(state, password.as_deref())?;
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
        let signing_ctx = load_post_signing_context(state, password.as_deref())?;
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
            &signing_ctx.own_reply_syncs,
            record.bundle_data, // EXACT bytes! Never call create_transfer_bundle!
            now_secs,
            None,
        )?;
        fresh_msg_id
    };

    persist_and_flush_post_box(
        state,
        &baseline,
        &mut post_state,
        msg_id,
        password.as_deref(),
    )
    .await
}

fn resolve_signing_request_anchor_manifest(
    state: &AppState,
    local_instance_id: Option<&str>,
) -> Option<String> {
    let service = state.service.lock().ok()?;
    service
        .with_wallet(|w| {
            let aid_hex =
                if let Some(lid) = local_instance_id.map(str::trim).filter(|s| !s.is_empty()) {
                    w.get_voucher_instance(lid)
                        .and_then(|inst| inst.voucher.layer2_anchor)
                        .map(hex::encode)
                } else {
                    let mut candidates = std::collections::BTreeSet::new();
                    for inst in w.voucher_store.vouchers.values() {
                        if let Some(aid) = inst.voucher.layer2_anchor {
                            let hx = hex::encode(aid);
                            if w.profile.l2_anchor_manifests.contains_key(&hx) {
                                candidates.insert(hx);
                            }
                        }
                    }
                    if candidates.len() == 1 {
                        candidates.into_iter().next()
                    } else {
                        None
                    }
                };
            aid_hex
                .and_then(|hx| w.profile.l2_anchor_manifests.get(&hx))
                .and_then(|m| serde_json::to_string(m).ok())
        })
        .ok()
        .flatten()
}

/// Sends a voucher co-signing request container to a guarantor via Post.
pub async fn send_signing_request_via_post_impl(
    recipient_did: String,
    container_bytes: Vec<u8>,
    local_instance_id: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    state.authorize(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let validated_recipient =
        ValidatedUserId::parse(&recipient_did).map_err(FrontendError::from)?;
    let recipient_did = validated_recipient.as_str().to_string();
    let recipient_ed_pk = validated_recipient.public_key();
    let signing_ctx = load_post_signing_context(state, password.as_deref())?;
    let anchor_manifest_json =
        resolve_signing_request_anchor_manifest(state, local_instance_id.as_deref());
    let now_secs = sync_client::current_unix_timestamp();

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state(state, password.as_deref())?;
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
        anchor_manifest_json,
    )?;

    persist_and_flush_post_box(
        state,
        &baseline,
        &mut post_state,
        msg_id,
        password.as_deref(),
    )
    .await
}

/// Replies with a detached voucher signature container via Post and clears any matching request.
pub async fn reply_detached_signature_via_post_impl(
    request_msg_id: Option<String>,
    recipient_did: String,
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    state.authorize(crate::state::RequiredAccess::Full, password.as_deref())?;
    let validated_recipient =
        ValidatedUserId::parse(&recipient_did).map_err(FrontendError::from)?;
    let recipient_did = validated_recipient.as_str().to_string();
    let recipient_ed_pk = validated_recipient.public_key();
    let signing_ctx = load_post_signing_context(state, password.as_deref())?;
    let now_secs = sync_client::current_unix_timestamp();

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state(state, password.as_deref())?;
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
        None,
    )?;
    let target_tag16 = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == msg_id)
        .map(|e| e.entry.tag16);

    merge_and_save_post_state(state, &baseline, &mut post_state, password.as_deref())?;
    let mid_baseline = post_state.clone();

    let outcome =
        flush_post_slots_impl(state, &mut post_state, None, &[], password.as_deref()).await;
    let delivered_to_recipient =
        target_tag16.is_some_and(|t| outcome.recipient_uploaded_tags.contains(&t));

    if delivered_to_recipient {
        if let Some(ref req_id) = request_msg_id {
            if let Some(idx) = post_state
                .pending_sign_requests
                .iter()
                .position(|r| &r.msg_id == req_id)
            {
                let req = post_state.pending_sign_requests.remove(idx);
                if !req.reply_syncs.is_empty() {
                    let mut service = state.service.lock().unwrap();
                    state.update_contact_from_card(
                        &mut service,
                        &recipient_did,
                        &req.reply_syncs,
                        None,
                        password.as_deref(),
                    );
                }
            }
        }
    }

    merge_and_save_post_state(state, &mid_baseline, &mut post_state, password.as_deref())?;

    let queued_for_next_flush = target_tag16.is_some_and(|t| outcome.deferred_tags.contains(&t));

    Ok(FrontendPostDepositResult {
        deposited_to: outcome.deposited_to,
        recipient_deposited_to: outcome.recipient_deposited_to,
        msg_id,
        deferred_count: outcome.deferred_count,
        queued_for_next_flush,
        errors: outcome.errors,
    })
}

/// Sends a WoT trust handshake assertion with an embedded `WotSyncPackage` via Post.
pub async fn send_wot_handshake_via_post_impl(
    recipient_did: String,
    proposed_level: u8,
    reply_requested: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPostDepositResult, FrontendError> {
    state.authorize(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
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

        let can_recipient = crate::state::canonicalize_did(&recipient_did)
            .unwrap_or_else(|_| recipient_did.clone());

        let (pkg, _receipt) = reg
            .prepare_sync_package(Some(&can_recipient), None, true)
            .map_err(FrontendError::from)?;
        let wot_sync_json = serde_json::to_string(&pkg)
            .map_err(|e| FrontendError::from(format!("Failed to serialize sync package: {}", e)))?;

        let security_policy = state
            .get_or_load_security_policy(&mut service, password.as_deref())
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

        (my_identity, my_ed_sk, reply_syncs, assertion, now_secs)
    };

    let container_bytes = seal_post_container_bytes(
        &my_identity,
        &recipient_did,
        PayloadType::TrustAssertion,
        &assertion,
        "assertion",
    )?;

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state(state, password.as_deref())?;
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &my_identity.user_id,
        &my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        &own_reply_syncs,
        container_bytes,
        now_secs,
        None,
    )?;

    persist_and_flush_post_box(
        state,
        &baseline,
        &mut post_state,
        msg_id,
        password.as_deref(),
    )
    .await
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
    state.authorize(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    if trust_level > 3 {
        return Err(FrontendError::from(format!(
            "trust_level must be 0..=3, got {}",
            trust_level
        )));
    }

    let sender_did = {
        let _post_guard = state.post_op_lock.lock().await;
        let mut post_state = load_post_state(state, password.as_deref())?;
        let baseline = post_state.clone();
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

        {
            let mut service = state.service.lock().unwrap();
            let mut contact = state
                .load_contacts(&mut service, password.as_deref())
                .ok()
                .and_then(|list| list.into_iter().find(|c| c.did.trim() == sender_did.trim()))
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
                .save_contact(&mut service, contact, password.as_deref())
                .map_err(FrontendError::from)?;
        }

        if let Some(json_str) = post_state.pending_wot_packages_by_msg_id.remove(&msg_id) {
            if let Ok(pkg) = serde_json::from_str::<humoco_wot::WotSyncPackage>(&json_str) {
                let mut service = state.service.lock().unwrap();
                if let Ok(mut reg) =
                    state.get_or_load_wot_register(&mut service, password.as_deref())
                {
                    let can_sender_did = crate::state::canonicalize_did(&sender_did)
                        .unwrap_or_else(|_| sender_did.clone());
                    let _ = reg.import_sync_package(&pkg, Some(&can_sender_did), now_secs);
                    let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
                    let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
                }
            }
        }

        merge_and_save_post_state(state, &baseline, &mut post_state, password.as_deref())?;
        sender_did
    };

    if send_return_handshake {
        send_wot_handshake_via_post_impl(sender_did, trust_level.min(1), false, password, state)
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
    state.authorize(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
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

    if !recipient_syncs.is_empty() {
        let mut service = state.service.lock().unwrap();
        state.update_contact_from_card(
            &mut service,
            &recipient_did,
            &recipient_syncs,
            None,
            password.as_deref(),
        );
    }

    let signing_ctx = load_post_signing_context(state, password.as_deref())?;

    let container_bytes = seal_post_container_bytes(
        &signing_ctx.identity,
        &recipient_did,
        PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_V1.to_string()),
        &inquiry,
        "inquiry",
    )?;

    let _post_guard = state.post_op_lock.lock().await;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let mut post_state = load_post_state(state, password.as_deref())?;
    let baseline = post_state.clone();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &recipient_did,
        recipient_ed_pk,
        msg_id.clone(),
        None,
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
        None,
    )?;
    let target_tag16 = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == msg_id)
        .map(|e| e.entry.tag16);

    let flush_outcome = flush_post_slots_impl(
        state,
        &mut post_state,
        Some(&recipient_did),
        &recipient_syncs,
        password.as_deref(),
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

    let (counterparty_name, counterparty_picture_url, is_contact) =
        resolve_counterparty_info(state, &recipient_did, password.as_deref());

    let item = FrontendMarketInquiryItem {
        inquiry_id,
        role: "buyer".to_string(),
        target,
        counterparty_did: recipient_did,
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
        reply_syncs: recipient_syncs,
        messages: vec![FrontendInquiryChatMessage {
            msg_id,
            sender: "me".to_string(),
            text: Some(message),
            decision: None,
            created_at: now_secs,
        }],
    };
    post_state.inquiries.push(item.clone());
    merge_and_save_post_state(state, &baseline, &mut post_state, password.as_deref())?;

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
    state.authorize(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
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
    let mut post_state = load_post_state(state, password.as_deref())?;
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
        let discovered_syncs =
            lookup_counterparty_syncs(state, &counterparty_did, password.as_deref());
        if discovered_syncs.is_empty() {
            return Err(FrontendError::from(
                "Counterparty has no sync service configured for replies".to_string(),
            ));
        }
        post_state.inquiries[item_idx].reply_syncs = discovered_syncs;
    }

    let msg_id = uuid::Uuid::new_v4().to_string();
    let msg_payload = MarketInquiryMessage {
        inquiry_id: inquiry_id.clone(),
        msg_id: msg_id.clone(),
        text: clean_text.clone(),
        decision: dec,
        payment_uri: payment_uri.clone(),
        amount_hint: amount_hint.clone(),
        standard_uuid: standard_uuid.clone(),
        created_at: now_secs,
    };
    validate_market_inquiry_message(&msg_payload).map_err(FrontendError::from)?;

    let validated_counterparty =
        ValidatedUserId::parse(&counterparty_did).map_err(FrontendError::from)?;
    let counterparty_did = validated_counterparty.as_str().to_string();
    let recipient_ed_pk = validated_counterparty.public_key();

    if dec.is_some() {
        post_state.inquiry_rate_tracker.resolve_inquiry(
            &hex::encode(recipient_ed_pk.as_bytes()),
            &target.target_key(),
        );
    }

    let signing_ctx = load_post_signing_context(state, password.as_deref())?;

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
        &signing_ctx.own_reply_syncs,
        container_bytes,
        now_secs,
        None,
    )?;
    let target_tag16 = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == msg_id)
        .map(|e| e.entry.tag16);

    let flush_outcome =
        flush_post_slots_impl(state, &mut post_state, None, &[], password.as_deref()).await;
    if !target_tag16.is_some_and(|t| flush_outcome.recipient_uploaded_tags.contains(&t)) {
        return Err(FrontendError::with_code(
            "error.post.recipientUnreachable",
            format!(
                "Recipient sync inbox unreachable for inquiry message. Errors: {:?}",
                flush_outcome.errors
            ),
        ));
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
        item.status = if d == "confirm" {
            "confirmed".to_string()
        } else {
            "declined".to_string()
        };
    }
    if payment_uri.is_some() {
        item.payment_uri = payment_uri;
    }
    if amount_hint.is_some() {
        item.amount_hint = amount_hint;
    }
    if standard_uuid.is_some() {
        item.standard_uuid = standard_uuid;
    }
    item.updated_at = now_secs;
    item.unread = false;

    let mut updated_item = item.clone();
    merge_and_save_post_state(state, &baseline, &mut post_state, password.as_deref())?;

    let (name, pic, is_contact) =
        resolve_counterparty_info(state, &updated_item.counterparty_did, password.as_deref());
    updated_item.counterparty_name = name;
    updated_item.counterparty_picture_url = pic;
    updated_item.is_contact = is_contact;

    Ok(updated_item)
}
