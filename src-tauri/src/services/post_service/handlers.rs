use std::collections::HashMap;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use human_money_core::crypto::get_pubkey_from_user_id;
use human_money_core::models::profile::UserIdentity;
use human_money_core::models::secure_container::PayloadType;
use humoco_market::{
    validate_market_inquiry, validate_market_inquiry_message, validate_post_delivery_receipt,
    validate_wot_handshake_assertion, InquiryTarget, MarketInquiry, MarketInquiryDecision,
    MarketInquiryMessage, PostDeliveryReceipt, PostEnvelope, WotHandshakeAssertion,
    MAX_INQUIRY_MESSAGES_PER_THREAD, PAYLOAD_MARKET_INQUIRY_MESSAGE_V1, PAYLOAD_MARKET_INQUIRY_V1,
    PAYLOAD_POST_RECEIPT_V1,
};

use crate::models::{
    FrontendInquiryChatMessage, FrontendMarketInquiryItem, FrontendPendingSignRequest,
    FrontendPendingWotHandshake, FrontendSyncDescriptor, TransactionRecord,
};
use crate::services::common::{are_dids_equal, load_all_standard_tomls};
use crate::services::recovery_service::{
    load_guardian_vault_with_cache, load_recovery_owner_state_with_cache,
    save_guardian_vault_with_cache, save_recovery_owner_state_with_cache,
};
use crate::state::{AppState, StoredPostState};

use super::{check_inquiry_target, queue_receipt, queue_sealed_reply, resolve_counterparty_info};

/// Counters and notices accumulated while dispatching decrypted incoming Post envelopes.
#[derive(Default)]
pub(crate) struct PostDispatchStats {
    pub imported_payments: usize,
    pub imported_receipts: usize,
    pub imported_signatures: usize,
    pub new_sign_requests: usize,
    pub new_wot_handshakes: usize,
    pub new_inquiries: usize,
    pub new_inquiry_messages: usize,
    pub pruned_boxes: usize,
    pub last_inquiry_notice_target: Option<(String, Option<String>)>,
    pub errors: Vec<String>,
}

/// Borrowed context shared by all incoming Post payload handlers.
pub(crate) struct PostEnvelopeContext<'a> {
    pub state: &'a AppState,
    pub app: Option<&'a tauri::AppHandle>,
    pub extra_standards: Option<&'a HashMap<String, String>>,
    pub password: Option<&'a str>,
    pub my_identity: &'a UserIdentity,
    pub my_ed_sk: &'a SigningKey,
    pub own_reply_syncs: &'a [FrontendSyncDescriptor],
    pub sender_did: &'a str,
    pub envelope: &'a PostEnvelope,
    pub tag16: &'a [u8; 16],
    pub decrypted_bytes: &'a [u8],
    pub now_secs: u64,
}

/// Dispatches a verified and decrypted Post container payload to its domain handler.
/// Returns `true` if `msg_id` may be marked processed, or `false` when a transient/locked-state
/// error occurred so the message stays eligible for retry after unlock.
pub(crate) fn dispatch_post_payload(
    ctx: &PostEnvelopeContext<'_>,
    payload_type: &PayloadType,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    match payload_type {
        PayloadType::TransactionBundle => handle_transaction_bundle(ctx, post_state, stats),
        PayloadType::VoucherForSigning => {
            handle_voucher_for_signing(ctx, post_state, stats);
            true
        }
        PayloadType::DetachedSignature => handle_detached_signature(ctx, post_state, stats),
        PayloadType::TrustAssertion => {
            handle_trust_assertion(ctx, post_state, stats);
            true
        }
        PayloadType::Generic(ref s) if s == PAYLOAD_POST_RECEIPT_V1 => {
            handle_post_receipt(ctx, post_state, stats);
            true
        }
        PayloadType::Generic(ref s) if s == PAYLOAD_MARKET_INQUIRY_V1 => {
            handle_market_inquiry(ctx, post_state, stats);
            true
        }
        PayloadType::Generic(ref s) if s == PAYLOAD_MARKET_INQUIRY_MESSAGE_V1 => {
            handle_market_inquiry_message(ctx, post_state, stats);
            true
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_ASSIGN_V1 => {
            handle_guardian_assign(ctx, post_state, stats)
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_ACK_V1 => {
            handle_guardian_ack(ctx, post_state, stats)
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1 => {
            handle_guardian_locator(ctx, post_state, stats)
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_REVOKE_V1 => {
            handle_guardian_revoke(ctx, post_state, stats)
        }
        _ => true,
    }
}

fn queue_ack_receipt(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    bundle_id: Option<&str>,
) {
    queue_receipt(
        post_state,
        ctx.my_identity,
        ctx.my_ed_sk,
        ctx.sender_did,
        &ctx.envelope.msg_id,
        ctx.tag16,
        bundle_id,
        ctx.own_reply_syncs,
        ctx.now_secs,
    );
}

fn handle_transaction_bundle(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    let all_standards = load_all_standard_tomls(ctx.app, ctx.extra_standards);
    let archive = ctx.app.and_then(|a| {
        let service = ctx.state.service.lock().unwrap();
        crate::services::common::open_wallet_archive(a, &service)
    });

    let receive_outcome = {
        let mut service = ctx.state.service.lock().unwrap();
        service.receive_bundle(
            &ctx.envelope.container_bytes,
            &all_standards,
            archive.as_ref(),
            ctx.password,
            false,
        )
    };

    let (bundle_id, should_mark_processed) = match receive_outcome {
        Ok(result) => {
            stats.imported_payments += 1;
            let mut service = ctx.state.service.lock().unwrap();
            let notes = result
                .header
                .notes
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let record = TransactionRecord {
                id: uuid::Uuid::new_v4().to_string(),
                direction: "received".to_string(),
                recipient_id: result.header.recipient_id.clone(),
                sender_id: result.header.sender_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts: result.transfer_summary.summable_amounts.clone(),
                countable_items: result.transfer_summary.countable_items.clone(),
                involved_vouchers: result.involved_vouchers.clone(),
                involved_sources_details: Some(
                    result
                        .involved_vouchers_details
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect(),
                ),
                bundle_data: Vec::new(),
                bundle_id: result.header.bundle_id.clone(),
                notes,
                sender_profile_name: result.header.sender_profile_name.clone(),
            };
            let _ = ctx
                .state
                .append_to_history(&mut service, record, ctx.password);
            let _ = ctx.state.refresh_events_cache(&mut service, ctx.password);

            if let Some(ref wot_json) = result.wot_sync_json {
                ctx.state.try_import_piggybacked_wot(
                    &mut service,
                    wot_json,
                    &result.header.sender_id,
                    ctx.password,
                );
            }
            if let Some(ref card_json) = result.contact_card_json {
                ctx.state.try_import_piggybacked_contact_card(
                    &mut service,
                    Some(card_json),
                    ctx.password,
                );
            }
            (Some(result.header.bundle_id), true)
        }
        Err(e) => {
            let is_sibling_subaccount = matches!(
                &e,
                human_money_core::VoucherCoreError::BundleRecipientMismatch { expected, found }
                    if crate::state::canonicalize_did(expected)
                        .ok()
                        .is_some_and(|ce| crate::state::canonicalize_did(found).ok().as_ref() == Some(&ce))
            );
            let err_lower = e.to_string().to_ascii_lowercase();
            let is_duplicate =
                err_lower.contains("already processed") || err_lower.contains("duplicate");
            if is_sibling_subaccount || is_duplicate {
                (None, true)
            } else {
                stats
                    .errors
                    .push(format!("Receive payment bundle failed: {}", e));
                (None, false)
            }
        }
    };

    if let Some(ref bid) = bundle_id {
        queue_ack_receipt(ctx, post_state, Some(bid));
    }
    should_mark_processed
}

fn handle_voucher_for_signing(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) {
    let (sender_name, _, _) = resolve_counterparty_info(ctx.state, ctx.sender_did, ctx.password);
    post_state
        .pending_sign_requests
        .push(FrontendPendingSignRequest {
            msg_id: ctx.envelope.msg_id.clone(),
            sender_did: ctx.sender_did.to_string(),
            sender_name,
            created_at: ctx.envelope.created_at,
            reply_syncs: ctx
                .envelope
                .reply_drops
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            container_bytes: ctx.envelope.container_bytes.clone(),
            anchor_manifest_json: ctx.envelope.anchor_manifest_json.clone(),
        });
    stats.new_sign_requests += 1;
    queue_ack_receipt(ctx, post_state, None);
}

fn is_retryable_post_attach_error(err: &human_money_core::app_service::AppFacadeError) -> bool {
    matches!(
        err,
        human_money_core::app_service::AppFacadeError::WalletLocked
            | human_money_core::app_service::AppFacadeError::SessionExpired(_)
            | human_money_core::app_service::AppFacadeError::SessionNotActive(_)
            | human_money_core::app_service::AppFacadeError::Storage(_)
            | human_money_core::app_service::AppFacadeError::Io(_)
            | human_money_core::app_service::AppFacadeError::App(
                human_money_core::error::AppError::InsufficientAccessMode { .. }
            )
    )
}

fn handle_detached_signature(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    let all_standards = load_all_standard_tomls(ctx.app, ctx.extra_standards);
    let mut attached = false;
    let mut retryable_error: Option<String> = None;
    {
        let mut service = ctx.state.service.lock().unwrap();
        if !service.is_wallet_unlocked() {
            stats.errors.push(
                "Detached signature attachment deferred: wallet session is locked".to_string(),
            );
            return false;
        }
        for toml in all_standards.values() {
            match service.process_and_attach_signature(
                &ctx.envelope.container_bytes,
                toml,
                None,
                ctx.password,
            ) {
                Ok(_) => {
                    attached = true;
                    retryable_error = None;
                    break;
                }
                Err(e) => {
                    if is_retryable_post_attach_error(&e) {
                        retryable_error = Some(e.to_string());
                        break;
                    }
                }
            }
        }
    }
    if attached {
        stats.imported_signatures += 1;
        {
            let mut service = ctx.state.service.lock().unwrap();
            let _ = ctx.state.refresh_events_cache(&mut service, ctx.password);
        }
        queue_ack_receipt(ctx, post_state, None);
        return true;
    }
    if let Some(err) = retryable_error {
        stats
            .errors
            .push(format!("Detached signature attachment deferred: {}", err));
        return false;
    }
    true
}

fn handle_trust_assertion(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) {
    let assertion = match serde_json::from_slice::<WotHandshakeAssertion>(ctx.decrypted_bytes) {
        Ok(a) => a,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse WotHandshakeAssertion: {}", e));
            return;
        }
    };
    if let Err(e) = validate_wot_handshake_assertion(&assertion) {
        stats
            .errors
            .push(format!("Invalid WoT handshake assertion: {}", e));
        return;
    }

    let is_known_trusted_contact = {
        let mut service = ctx.state.service.lock().unwrap();
        ctx.state
            .load_contacts(&mut service, ctx.password)
            .map(|list| {
                list.iter().any(|c| {
                    are_dids_equal(&c.did, &assertion.sender_did) && c.trust_level.unwrap_or(0) >= 1
                })
            })
            .unwrap_or(false)
    };

    if is_known_trusted_contact {
        // Silent auto-accept for established contacts (Zero-Click).
        if let Ok(pkg) =
            serde_json::from_str::<humoco_wot::WotSyncPackage>(&assertion.wot_sync_json)
        {
            let mut service = ctx.state.service.lock().unwrap();
            if let Ok(mut reg) = ctx
                .state
                .get_or_load_wot_register(&mut service, ctx.password)
            {
                let can_sender_did = crate::state::canonicalize_did(&assertion.sender_did)
                    .unwrap_or_else(|_| assertion.sender_did.clone());
                let _ = reg.import_sync_package(&pkg, Some(&can_sender_did), ctx.now_secs);
                let _ = ctx.state.ensure_wot_list_signed(&mut service, &mut reg);
                let _ = ctx
                    .state
                    .save_wot_register(&mut service, &reg, ctx.password);
            }
        }

        queue_ack_receipt(ctx, post_state, None);

        // If counterparty requested a mutual sync package, automatically queue a return package.
        if assertion.reply_requested {
            let can_recipient = crate::state::canonicalize_did(ctx.sender_did)
                .unwrap_or_else(|_| ctx.sender_did.to_string());
            let reply_data = {
                let mut service = ctx.state.service.lock().unwrap();
                ctx.state
                    .get_or_load_wot_register(&mut service, ctx.password)
                    .ok()
                    .and_then(|reg| {
                        reg.prepare_sync_package(Some(&can_recipient), None, true)
                            .ok()
                    })
                    .and_then(|(pkg, _)| serde_json::to_string(&pkg).ok())
                    .map(|wot_sync_json| {
                        let my_name = service
                            .with_wallet(|w| w.profile.first_name.clone())
                            .ok()
                            .flatten();
                        (wot_sync_json, my_name)
                    })
            };
            if let Some((wot_sync_json, my_name)) = reply_data {
                let auto_reply_assertion = WotHandshakeAssertion {
                    sender_did: ctx.my_identity.user_id.clone(),
                    sender_name: my_name,
                    proposed_level: 1,
                    wot_sync_json,
                    reply_requested: false,
                    created_at: ctx.now_secs,
                };
                queue_sealed_reply(
                    post_state,
                    ctx.my_identity,
                    ctx.my_ed_sk,
                    ctx.sender_did,
                    PayloadType::TrustAssertion,
                    &auto_reply_assertion,
                    uuid::Uuid::new_v4().to_string(),
                    ctx.own_reply_syncs,
                    ctx.now_secs,
                    "auto-reply assertion",
                );
            }
        }
    } else {
        // Unknown or unrated contact: require conscious user confirmation.
        post_state
            .pending_wot_packages_by_msg_id
            .insert(ctx.envelope.msg_id.clone(), assertion.wot_sync_json.clone());
        post_state
            .pending_wot_handshakes
            .push(FrontendPendingWotHandshake {
                msg_id: ctx.envelope.msg_id.clone(),
                sender_did: assertion.sender_did,
                sender_name: assertion.sender_name,
                proposed_level: assertion.proposed_level,
                reply_requested: assertion.reply_requested,
                created_at: assertion.created_at,
            });
        stats.new_wot_handshakes += 1;
        queue_ack_receipt(ctx, post_state, None);
    }
}

fn handle_post_receipt(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) {
    let receipt = match serde_json::from_slice::<PostDeliveryReceipt>(ctx.decrypted_bytes) {
        Ok(r) => r,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse PostDeliveryReceipt: {}", e));
            return;
        }
    };
    if let Err(e) = validate_post_delivery_receipt(&receipt) {
        stats
            .errors
            .push(format!("Invalid delivery receipt: {}", e));
        return;
    }
    let before_len = post_state.outbox_entries.len();
    post_state.outbox_entries.retain(|e| {
        let matches_sender = are_dids_equal(&e.recipient_did, ctx.sender_did);
        let matches_msg = e.msg_id == receipt.acknowledged_msg_id
            || e.tag16_hex == receipt.acknowledged_tag16_hex;
        !(matches_sender && matches_msg)
    });
    let removed = before_len - post_state.outbox_entries.len();
    if removed > 0 {
        stats.imported_receipts += removed;
        stats.pruned_boxes += removed;
    }
}

fn handle_market_inquiry(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) {
    let inquiry = match serde_json::from_slice::<MarketInquiry>(ctx.decrypted_bytes) {
        Ok(i) => i,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse MarketInquiry: {}", e));
            return;
        }
    };
    if let Err(e) = validate_market_inquiry(&inquiry) {
        stats.errors.push(format!("Invalid market inquiry: {}", e));
        return;
    }

    let is_target_valid =
        check_inquiry_target(ctx.state, &inquiry.target, ctx.now_secs, ctx.password);
    if !is_target_valid {
        match &inquiry.target {
            InquiryTarget::Ad { ad_id, .. } => {
                if get_pubkey_from_user_id(ctx.sender_did).is_ok() {
                    let decline_msg = MarketInquiryMessage {
                        inquiry_id: inquiry.inquiry_id.clone(),
                        msg_id: uuid::Uuid::new_v4().to_string(),
                        text: Some("Inserat nicht mehr verfügbar".to_string()),
                        decision: Some(MarketInquiryDecision::Decline),
                        payment_uri: None,
                        amount_hint: None,
                        standard_uuid: None,
                        created_at: ctx.now_secs,
                    };
                    queue_sealed_reply(
                        post_state,
                        ctx.my_identity,
                        ctx.my_ed_sk,
                        ctx.sender_did,
                        PayloadType::Generic(PAYLOAD_MARKET_INQUIRY_MESSAGE_V1.to_string()),
                        &decline_msg,
                        decline_msg.msg_id.clone(),
                        ctx.own_reply_syncs,
                        ctx.now_secs,
                        "inquiry auto-decline",
                    );
                    queue_ack_receipt(ctx, post_state, None);
                }
                stats.errors.push(format!(
                    "Inquiry for unknown or inquiries-disabled ad {}",
                    ad_id
                ));
            }
            InquiryTarget::Person { .. } => {
                stats.errors.push(
                    "Inquiry for person profile rejected (profile unpublished or inquiries disabled)"
                        .to_string(),
                );
            }
        }
        return;
    }

    let sender_pk_hex = get_pubkey_from_user_id(ctx.sender_did)
        .map(|pk| hex::encode(pk.as_bytes()))
        .unwrap_or_default();
    let target_key = inquiry.target.target_key();
    if let Err(e) = post_state
        .inquiry_rate_tracker
        .record_inquiry(&sender_pk_hex, &target_key)
    {
        stats.errors.push(format!(
            "Inquiry rate limit rejected for {}: {}",
            ctx.sender_did, e
        ));
        return;
    }

    let (sender_name, sender_pic, is_contact) =
        resolve_counterparty_info(ctx.state, ctx.sender_did, ctx.password);
    if !post_state
        .inquiries
        .iter()
        .any(|i| i.inquiry_id == inquiry.inquiry_id)
    {
        stats.last_inquiry_notice_target = Some((inquiry.inquiry_id.clone(), sender_name.clone()));
        post_state.inquiries.push(FrontendMarketInquiryItem {
            inquiry_id: inquiry.inquiry_id.clone(),
            role: "seller".to_string(),
            target: inquiry.target.clone(),
            counterparty_did: ctx.sender_did.to_string(),
            counterparty_name: sender_name,
            counterparty_picture_url: sender_pic,
            is_contact,
            status: "open".to_string(),
            payment_uri: None,
            amount_hint: None,
            standard_uuid: None,
            created_at: inquiry.created_at,
            updated_at: inquiry.created_at,
            unread: true,
            reply_syncs: ctx
                .envelope
                .reply_drops
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            messages: vec![FrontendInquiryChatMessage {
                msg_id: ctx.envelope.msg_id.clone(),
                sender: "them".to_string(),
                text: Some(inquiry.message),
                decision: None,
                created_at: inquiry.created_at,
            }],
        });
        stats.new_inquiries += 1;
    }
    queue_ack_receipt(ctx, post_state, None);
}

fn handle_market_inquiry_message(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) {
    let msg = match serde_json::from_slice::<MarketInquiryMessage>(ctx.decrypted_bytes) {
        Ok(m) => m,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse MarketInquiryMessage: {}", e));
            return;
        }
    };
    if let Err(e) = validate_market_inquiry_message(&msg) {
        stats
            .errors
            .push(format!("Invalid market inquiry message: {}", e));
        return;
    }
    let Some(item) = post_state.inquiries.iter_mut().find(|i| {
        i.inquiry_id == msg.inquiry_id && are_dids_equal(&i.counterparty_did, ctx.sender_did)
    }) else {
        return;
    };

    let mut notice_inquiry_id: Option<String> = None;
    if !item.messages.iter().any(|m| m.msg_id == msg.msg_id)
        && item.messages.len() < MAX_INQUIRY_MESSAGES_PER_THREAD
    {
        let valid_decision = match msg.decision {
            Some(d) => {
                if item.role == "buyer" && item.status == "open" {
                    Some(match d {
                        MarketInquiryDecision::Confirm => "confirm".to_string(),
                        MarketInquiryDecision::Decline => "decline".to_string(),
                    })
                } else {
                    stats.errors.push(format!(
                        "Ignored invalid inquiry decision on thread {}",
                        msg.inquiry_id
                    ));
                    None
                }
            }
            None => None,
        };

        if let Some(ref dec_str) = valid_decision {
            item.status = if dec_str == "confirm" {
                "confirmed".to_string()
            } else {
                "declined".to_string()
            };
            if msg.payment_uri.is_some() {
                item.payment_uri = msg.payment_uri.clone();
            }
            if msg.amount_hint.is_some() {
                item.amount_hint = msg.amount_hint.clone();
            }
            if msg.standard_uuid.is_some() {
                item.standard_uuid = msg.standard_uuid.clone();
            }
        }

        if !ctx.envelope.reply_drops.is_empty() {
            item.reply_syncs = ctx
                .envelope
                .reply_drops
                .iter()
                .cloned()
                .map(Into::into)
                .collect();
        }

        item.messages.push(FrontendInquiryChatMessage {
            msg_id: msg.msg_id.clone(),
            sender: "them".to_string(),
            text: msg.text.clone(),
            decision: valid_decision,
            created_at: msg.created_at,
        });
        item.messages
            .sort_by(|a, b| a.created_at.cmp(&b.created_at));
        item.updated_at = item.updated_at.max(msg.created_at);
        item.unread = true;
        stats.new_inquiry_messages += 1;
        notice_inquiry_id = Some(item.inquiry_id.clone());
    }

    if let Some(inq_id) = notice_inquiry_id {
        let cp_name = resolve_counterparty_info(ctx.state, ctx.sender_did, ctx.password).0;
        stats.last_inquiry_notice_target = Some((inq_id, cp_name));
    }
    queue_ack_receipt(ctx, post_state, None);
}

fn handle_guardian_assign(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    let entry =
        match serde_json::from_slice::<humoco_market::GuardianVaultEntry>(ctx.decrypted_bytes) {
            Ok(e) => e,
            Err(e) => {
                stats
                    .errors
                    .push(format!("Failed to parse GuardianVaultEntry: {}", e));
                return true;
            }
        };
    if !are_dids_equal(ctx.sender_did, &entry.owner_did) {
        stats.errors.push(format!(
            "Guardian assign sender DID mismatch: {} != {}",
            ctx.sender_did, entry.owner_did
        ));
        return true;
    }

    let is_trusted_non_offender = {
        let mut service = ctx.state.service.lock().unwrap();
        let is_offender = service
            .with_wallet(|w| {
                matches!(
                    w.check_reputation(ctx.sender_did),
                    human_money_core::models::conflict::TrustStatus::KnownOffender(_)
                )
            })
            .unwrap_or(false);
        if is_offender {
            false
        } else {
            ctx.state
                .load_contacts(&mut service, ctx.password)
                .map(|list| {
                    list.iter().any(|c| {
                        are_dids_equal(&c.did, ctx.sender_did) && c.trust_level.unwrap_or(0) >= 1
                    })
                })
                .unwrap_or(false)
        }
    };

    if !is_trusted_non_offender {
        stats.errors.push(format!(
            "Guardian assign from untrusted sender or known offender: {}",
            ctx.sender_did
        ));
        return true;
    }

    if let Err(e) = humoco_market::verify_guardian_vault_entry(&entry) {
        stats
            .errors
            .push(format!("Invalid guardian vault entry: {}", e));
        return true;
    }

    let save_res: Result<(), (String, bool)> = {
        let mut service = ctx.state.service.lock().unwrap();
        match load_guardian_vault_with_cache(ctx.state, &mut service, ctx.password) {
            Ok(mut vault) => match vault.upsert_ward(entry.clone()) {
                Ok(()) => save_guardian_vault_with_cache(
                    ctx.state,
                    &mut service,
                    &vault,
                    ctx.password,
                )
                .map_err(|e| (format!("Failed to save guardian vault: {}", e.message), false)),
                Err(e) => Err((format!("Failed to upsert guardian ward: {}", e), true)),
            },
            Err(e) => Err((format!("Failed to load guardian vault: {}", e.message), false)),
        }
    };

    match save_res {
        Ok(()) => {
            let ack = humoco_market::GuardianVaultAck {
                owner_did: entry.owner_did,
                rotation_epoch: entry.rotation_epoch,
                acked_at: ctx.now_secs,
            };
            queue_sealed_reply(
                post_state,
                ctx.my_identity,
                ctx.my_ed_sk,
                ctx.sender_did,
                PayloadType::Generic(humoco_market::PAYLOAD_GUARDIAN_ACK_V1.to_string()),
                &ack,
                uuid::Uuid::new_v4().to_string(),
                ctx.own_reply_syncs,
                ctx.now_secs,
                "guardian ack",
            );
            queue_ack_receipt(ctx, post_state, None);
            true
        }
        Err((err_msg, mark_processed)) => {
            stats.errors.push(err_msg);
            mark_processed
        }
    }
}

fn handle_guardian_ack(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    let ack = match serde_json::from_slice::<humoco_market::GuardianVaultAck>(ctx.decrypted_bytes) {
        Ok(a) => a,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse GuardianVaultAck: {}", e));
            return true;
        }
    };
    {
        let mut service = ctx.state.service.lock().unwrap();
        let owner_opt =
            match load_recovery_owner_state_with_cache(ctx.state, &mut service, ctx.password) {
                Ok(opt) => opt,
                Err(e) => {
                    stats
                        .errors
                        .push(format!("Failed to load recovery owner state: {}", e.message));
                    return false;
                }
            };
        if let Some(mut owner_state) = owner_opt {
            let same_account = matches!(
                (
                    humoco_market::canonicalize_owner_account_id(&ack.owner_did),
                    humoco_market::canonicalize_owner_account_id(&ctx.my_identity.user_id),
                ),
                (Ok(a), Ok(b)) if a == b
            );
            if ack.rotation_epoch == owner_state.rotation_epoch && same_account {
                let mut modified = false;
                for g in &mut owner_state.guardians {
                    if are_dids_equal(&g.user_id, ctx.sender_did) {
                        g.status = crate::models::GuardianAssignmentStatus::Confirmed {
                            acked_at: ack.acked_at,
                        };
                        modified = true;
                    }
                }
                if modified {
                    if let Err(e) = save_recovery_owner_state_with_cache(
                        ctx.state,
                        &mut service,
                        &owner_state,
                        ctx.password,
                    ) {
                        stats
                            .errors
                            .push(format!("Failed to save recovery owner state: {}", e.message));
                        return false;
                    }
                }
            }
        }
    }
    queue_ack_receipt(ctx, post_state, None);
    true
}

fn handle_guardian_locator(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    let update =
        match serde_json::from_slice::<humoco_market::GuardianLocatorUpdate>(ctx.decrypted_bytes) {
            Ok(u) => u,
            Err(e) => {
                stats
                    .errors
                    .push(format!("Failed to parse GuardianLocatorUpdate: {}", e));
                return true;
            }
        };
    if !are_dids_equal(ctx.sender_did, &update.owner_did) {
        stats.errors.push(format!(
            "Guardian locator update sender mismatch: {} != {}",
            ctx.sender_did, update.owner_did
        ));
        return true;
    }

    let update_res: Result<(), (String, bool)> = {
        let mut service = ctx.state.service.lock().unwrap();
        match load_guardian_vault_with_cache(ctx.state, &mut service, ctx.password) {
            Ok(mut vault) => match vault.update_locator(&update) {
                Ok(updated) => {
                    if updated {
                        save_guardian_vault_with_cache(
                            ctx.state,
                            &mut service,
                            &vault,
                            ctx.password,
                        )
                        .map_err(|e| {
                            (format!("Failed to save guardian vault: {}", e.message), false)
                        })
                    } else {
                        Ok(())
                    }
                }
                Err(e) => Err((format!("Failed to update guardian locator: {}", e), true)),
            },
            Err(e) => Err((format!("Failed to load guardian vault: {}", e.message), false)),
        }
    };

    match update_res {
        Ok(()) => {
            queue_ack_receipt(ctx, post_state, None);
            true
        }
        Err((err_msg, mark_processed)) => {
            stats.errors.push(err_msg);
            mark_processed
        }
    }
}

fn handle_guardian_revoke(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    let revoke = match serde_json::from_slice::<humoco_market::GuardianRevoke>(ctx.decrypted_bytes)
    {
        Ok(r) => r,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse GuardianRevoke: {}", e));
            return true;
        }
    };
    if !are_dids_equal(ctx.sender_did, &revoke.owner_did) {
        stats.errors.push(format!(
            "Guardian revoke sender mismatch: {} != {}",
            ctx.sender_did, revoke.owner_did
        ));
        return true;
    }

    let revoke_res: Result<(), (String, bool)> = {
        let mut service = ctx.state.service.lock().unwrap();
        match load_guardian_vault_with_cache(ctx.state, &mut service, ctx.password) {
            Ok(mut vault) => match vault.revoke_ward(&revoke) {
                Ok(revoked) => {
                    if revoked {
                        save_guardian_vault_with_cache(
                            ctx.state,
                            &mut service,
                            &vault,
                            ctx.password,
                        )
                        .map_err(|e| {
                            (format!("Failed to save guardian vault: {}", e.message), false)
                        })
                    } else {
                        Ok(())
                    }
                }
                Err(e) => Err((
                    format!("Failed to revoke ward from guardian vault: {}", e),
                    true,
                )),
            },
            Err(e) => Err((format!("Failed to load guardian vault: {}", e.message), false)),
        }
    };

    match revoke_res {
        Ok(()) => {
            queue_ack_receipt(ctx, post_state, None);
            true
        }
        Err((err_msg, mark_processed)) => {
            stats.errors.push(err_msg);
            mark_processed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RECOVERY_GUARDIAN_VAULT_KEY;
    use crate::test_support::{create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE};

    #[test]
    fn test_corrupt_guardian_vault_returns_false_and_preserves_bytes() {
        let base_dir = temp_test_dir("handlers_corrupt_guardian_vault");
        let password = "TestPassword123!";
        let service = setup_test_app(&base_dir, "Alice", ALICE_PHRASE, password);
        let state = create_test_state(service);

        let (my_identity, sender_did) = {
            let mut s = state.service.lock().unwrap();
            crate::state::save_encrypted_bytes(
                &mut s,
                RECOVERY_GUARDIAN_VAULT_KEY,
                b"{corrupt-vault",
                Some(password),
            )
            .unwrap();
            let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
            let did = id.user_id.clone();
            (id, did)
        };

        let update = humoco_market::GuardianLocatorUpdate {
            owner_did: sender_did.clone(),
            rotation_epoch: 1,
            backup_sync_descriptors: Vec::new(),
            updated_at: 1_750_000_000,
        };
        let payload_bytes = serde_json::to_vec(&update).unwrap();
        let envelope = humoco_market::PostEnvelope::new(
            "msg-corrupt-vault-1",
            1_750_000_000,
            Vec::new(),
            Vec::new(),
        );
        let tag16 = [7u8; 16];
        let my_ed_sk = my_identity.signing_key.clone();
        let ctx = PostEnvelopeContext {
            state: &state,
            app: None,
            extra_standards: None,
            password: Some(password),
            my_identity: &my_identity,
            my_ed_sk: &my_ed_sk,
            own_reply_syncs: &[],
            sender_did: &sender_did,
            envelope: &envelope,
            tag16: &tag16,
            decrypted_bytes: &payload_bytes,
            now_secs: 1_750_000_000,
        };

        let mut post_state = StoredPostState::default();
        let mut stats = PostDispatchStats::default();
        let mark_processed = dispatch_post_payload(
            &ctx,
            &PayloadType::Generic(humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1.to_string()),
            &mut post_state,
            &mut stats,
        );

        assert!(
            !mark_processed,
            "Corrupt guardian vault store must return false so msg_id stays retryable"
        );
        assert!(!stats.errors.is_empty());

        let stored_raw = {
            let mut s = state.service.lock().unwrap();
            crate::state::load_encrypted_bytes_opt(&mut s, RECOVERY_GUARDIAN_VAULT_KEY, Some(password))
                .unwrap()
                .unwrap()
        };
        assert_eq!(stored_raw, b"{corrupt-vault");

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
