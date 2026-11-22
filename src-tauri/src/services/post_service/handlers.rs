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
    load_guardian_vault, load_recovery_owner_state, save_guardian_vault_with_cache,
    save_recovery_owner_state_with_cache,
};
use crate::state::{AppState, StoredPostState};

use super::{
    check_inquiry_target, queue_receipt, queue_sealed_reply, resolve_counterparty_info_for_session,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PostDispatchOutcome {
    Handled,
    RetryLater,
    UnsupportedPayload { payload_type: String },
}

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
    pub new_tolerance_payments: usize,
    pub pruned_boxes: usize,
    pub last_inquiry_notice_target: Option<(String, Option<String>)>,
    pub errors: Vec<String>,
}

/// Borrowed context shared by all incoming Post payload handlers.
pub(crate) struct PostEnvelopeContext<'a> {
    pub state: &'a AppState,
    pub session: &'a crate::session::SessionToken,
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

fn payload_type_label(payload_type: &PayloadType) -> String {
    match payload_type {
        PayloadType::Generic(s) => s.clone(),
        other => format!("{:?}", other),
    }
}

/// Dispatches a verified and decrypted Post container payload to its domain handler.
/// Returns `Handled` when `msg_id` may be marked processed, `RetryLater` when a transient/locked-state
/// error occurred so the message stays eligible for retry, or `UnsupportedPayload` when the payload
/// type is not recognized by this client version.
pub(crate) async fn dispatch_post_payload(
    ctx: &PostEnvelopeContext<'_>,
    payload_type: &PayloadType,
    post_state: &mut StoredPostState,
    baseline_post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> PostDispatchOutcome {
    let as_outcome = |ok: bool| {
        if ok {
            PostDispatchOutcome::Handled
        } else {
            PostDispatchOutcome::RetryLater
        }
    };
    match payload_type {
        PayloadType::TransactionBundle => {
            handle_transaction_bundle(ctx, post_state, baseline_post_state, stats).await
        }
        PayloadType::VoucherForSigning => {
            as_outcome(handle_voucher_for_signing(ctx, post_state, stats))
        }
        PayloadType::DetachedSignature => as_outcome(handle_detached_signature(
            ctx,
            post_state,
            baseline_post_state,
            stats,
        )),
        PayloadType::TrustAssertion => as_outcome(handle_trust_assertion(ctx, post_state, stats)),
        PayloadType::Generic(ref s) if s == PAYLOAD_POST_RECEIPT_V1 => {
            handle_post_receipt(ctx, post_state, stats);
            PostDispatchOutcome::Handled
        }
        PayloadType::Generic(ref s) if s == PAYLOAD_MARKET_INQUIRY_V1 => {
            handle_market_inquiry(ctx, post_state, stats);
            PostDispatchOutcome::Handled
        }
        PayloadType::Generic(ref s) if s == PAYLOAD_MARKET_INQUIRY_MESSAGE_V1 => {
            handle_market_inquiry_message(ctx, post_state, stats)
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_ASSIGN_V1 => {
            as_outcome(handle_guardian_assign(ctx, post_state, stats))
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_ACK_V1 => {
            as_outcome(handle_guardian_ack(ctx, post_state, stats))
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1 => {
            as_outcome(handle_guardian_locator(ctx, post_state, stats))
        }
        PayloadType::Generic(ref s) if s == humoco_market::PAYLOAD_GUARDIAN_REVOKE_V1 => {
            as_outcome(handle_guardian_revoke(ctx, post_state, stats))
        }
        _ => PostDispatchOutcome::UnsupportedPayload {
            payload_type: payload_type_label(payload_type),
        },
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

const MAX_PENDING_TOLERANCE_PAYMENTS: usize = 20;
const MAX_PENDING_TOLERANCE_TOTAL_B64_BYTES: usize = 4 * 1024 * 1024;
const MAX_PENDING_SIGN_REQUESTS: usize = 32;
const MAX_PENDING_WOT_HANDSHAKES: usize = 50;
const MAX_PENDING_QUEUE_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn is_retryable_bundle_error(err: &human_money_core::Error) -> bool {
    matches!(
        err,
        human_money_core::Error::App(
            human_money_core::error::AppError::WalletLocked
                | human_money_core::error::AppError::SessionExpired { .. }
                | human_money_core::error::AppError::SessionNotActive { .. }
                | human_money_core::error::AppError::InsufficientAccessMode { .. }
        ) | human_money_core::Error::Storage(_)
            | human_money_core::Error::Io(_)
            | human_money_core::Error::Wallet(
                human_money_core::error::WalletError::StandardNotFound { .. }
            )
    )
}

pub(crate) enum PostBundleCommitResult {
    Committed {
        result: Box<human_money_core::wallet::types::ProcessBundleResult>,
        post_state: Box<StoredPostState>,
    },
    CoreError(human_money_core::Error),
}

/// Commits an incoming PoST transaction bundle together with its history and `market_post_state`
/// companions and stored/precommit L2 evidence on an already-locked `AppService`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_post_transaction_bundle(
    service: &mut human_money_core::app_service::AppService,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
    container_bytes: &[u8],
    all_standards: &HashMap<String, String>,
    l2_evidence: Option<&crate::state::StoredPrecommitL2Evidence>,
    force_accept_tolerance_bundle: bool,
    password: Option<&str>,
    baseline_post_state: &StoredPostState,
    working_post_state: &StoredPostState,
    update_post_state: impl FnOnce(
        &mut StoredPostState,
        &human_money_core::wallet::types::ProcessBundleResult,
    ),
) -> Result<PostBundleCommitResult, crate::models::FrontendError> {
    let archive = app.and_then(|a| crate::services::common::open_wallet_archive(a, service));
    let baseline_history = state
        .load_history_from_disk(service, password)
        .map_err(crate::models::FrontendError::from)?;
    let mut candidate_post_state = working_post_state.clone();
    super::merge_post_state_with_service(
        service,
        baseline_post_state,
        &mut candidate_post_state,
        password,
    )?;
    let core_l2_evidence = l2_evidence.map(|ev| human_money_core::ReceiveL2Evidence {
        quorum_certificates: ev.quorum_certificates.as_slice(),
        competing_entries: ev.competing_entries.as_slice(),
    });
    let next_history_cell = std::cell::RefCell::new(None::<Vec<TransactionRecord>>);
    let post_state_cell = std::cell::RefCell::new(Some(candidate_post_state));
    let update_cell = std::cell::RefCell::new(Some(update_post_state));
    let committed_post_cell = std::cell::RefCell::new(None::<StoredPostState>);
    match service.receive_bundle_with_companions(
        container_bytes,
        all_standards,
        archive.as_ref(),
        password,
        force_accept_tolerance_bundle,
        core_l2_evidence.as_ref(),
        |result, _wallet| {
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
            let (mut blobs, next_hist) =
                crate::state::prepare_receive_history_companion(&baseline_history, record)
                    .map_err(|e| {
                        human_money_core::Error::Storage(
                            human_money_core::StorageError::InvalidFormat(e.message),
                        )
                    })?;
            let mut next_post = post_state_cell.borrow_mut().take().ok_or_else(|| {
                human_money_core::Error::Storage(human_money_core::StorageError::InvalidFormat(
                    "Missing post_state companion buffer".to_string(),
                ))
            })?;
            if let Some(update_fn) = update_cell.borrow_mut().take() {
                update_fn(&mut next_post, result);
            }
            let post_bytes = serde_json::to_vec(&next_post).map_err(|e| {
                human_money_core::Error::Storage(human_money_core::StorageError::InvalidFormat(
                    format!(
                        "Failed to serialize {}: {}",
                        crate::state::MARKET_POST_STATE_KEY,
                        e
                    ),
                ))
            })?;
            blobs.push((crate::state::MARKET_POST_STATE_KEY.to_string(), post_bytes));
            *next_history_cell.borrow_mut() = Some(next_hist);
            *committed_post_cell.borrow_mut() = Some(next_post);
            Ok(blobs)
        },
    ) {
        Ok(result) => {
            if let Some(next_hist) = next_history_cell.into_inner() {
                state.write_cache_slot_for_service(service, |c| &mut c.history, Some(next_hist));
            }
            let post_state = committed_post_cell.into_inner().ok_or_else(|| {
                crate::models::FrontendError::from(
                    "Missing committed post_state after receive_bundle_with_companions".to_string(),
                )
            })?;
            state.write_cache_slot_for_service(
                service,
                |c| &mut c.post_state,
                Some(post_state.clone()),
            );
            Ok(PostBundleCommitResult::Committed {
                result: Box::new(result),
                post_state: Box::new(post_state),
            })
        }
        Err(e) => Ok(PostBundleCommitResult::CoreError(e)),
    }
}

async fn handle_transaction_bundle(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    baseline_post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> PostDispatchOutcome {
    let all_standards = load_all_standard_tomls(ctx.app, ctx.extra_standards);
    let verification = match crate::checkout::verify_bundle_l2_precommit(
        ctx.state,
        ctx.session,
        &ctx.envelope.container_bytes,
        ctx.password,
    )
    .await
    {
        Ok(crate::checkout::PrecommitL2Verification::RetryLater { reason }) => {
            stats.errors.push(reason);
            return PostDispatchOutcome::RetryLater;
        }
        Ok(v) => v,
        Err(err) => {
            stats.errors.push(format!(
                "Pre-commit L2 verification failed: {}",
                err.message
            ));
            return PostDispatchOutcome::RetryLater;
        }
    };

    let l2_evidence_owned = match verification {
        crate::checkout::PrecommitL2Verification::OfflineOnly => None,
        crate::checkout::PrecommitL2Verification::CleanQuorum {
            quorum_certificates,
        } => Some(crate::state::StoredPrecommitL2Evidence {
            quorum_certificates,
            competing_entries: Vec::new(),
            is_conflict: false,
        }),
        crate::checkout::PrecommitL2Verification::ConflictDetected {
            quorum_certificates,
            competing_entries,
        } => Some(crate::state::StoredPrecommitL2Evidence {
            quorum_certificates,
            competing_entries,
            is_conflict: true,
        }),
        crate::checkout::PrecommitL2Verification::RetryLater { .. } => unreachable!(),
    };
    let is_conflict = l2_evidence_owned.as_ref().is_some_and(|e| e.is_conflict);

    enum HandleCommitOutcome {
        Committed(Box<StoredPostState>),
        CoreError(human_money_core::Error),
    }

    let commit_res =
        ctx.state
            .with_session(ctx.session, |service| match commit_post_transaction_bundle(
                service,
                ctx.state,
                ctx.app,
                &ctx.envelope.container_bytes,
                &all_standards,
                l2_evidence_owned.as_ref(),
                false,
                ctx.password,
                baseline_post_state,
                post_state,
                |ps, result| {
                    ps.processed_msg_ids.insert(
                        super::scoped_processed_msg_key(ctx.sender_did, &ctx.envelope.msg_id),
                        ctx.now_secs,
                    );
                    queue_ack_receipt(ctx, ps, Some(&result.header.bundle_id));
                },
            )? {
                PostBundleCommitResult::Committed {
                    result,
                    post_state: committed_post_state,
                } => {
                    crate::services::transfer_service::apply_receive_companion_side_effects(
                        service,
                        ctx.state,
                        &result,
                        ctx.password,
                    );
                    Ok(HandleCommitOutcome::Committed(committed_post_state))
                }
                PostBundleCommitResult::CoreError(e) => Ok(HandleCommitOutcome::CoreError(e)),
            });

    let outcome = match commit_res {
        Ok(Some(res)) => res,
        Ok(None) => {
            stats
                .errors
                .push("Receive payment bundle deferred: active wallet session changed".to_string());
            return PostDispatchOutcome::RetryLater;
        }
        Err(err) => {
            stats
                .errors
                .push(format!("Receive payment bundle failed: {}", err.message));
            return PostDispatchOutcome::RetryLater;
        }
    };

    match outcome {
        HandleCommitOutcome::Committed(committed_post_state) => {
            *post_state = *committed_post_state;
            *baseline_post_state = post_state.clone();
            stats.imported_payments += 1;
            if is_conflict {
                if let Some(app_handle) = ctx.app {
                    use tauri::Emitter;
                    let _ = app_handle.emit("humoco://conflicts-updated", ());
                }
            }
            PostDispatchOutcome::Handled
        }
        HandleCommitOutcome::CoreError(e) => {
            let tolerance_zone = match &e {
                human_money_core::Error::BundleInRecoveryToleranceZone => Some("soft"),
                human_money_core::Error::BundleInExtendedRecoveryToleranceZone => Some("critical"),
                _ => None,
            };
            if let Some(zone) = tolerance_zone {
                if post_state.pending_tolerance_payments.iter().any(|p| {
                    p.msg_id == ctx.envelope.msg_id && are_dids_equal(&p.sender_did, ctx.sender_did)
                }) {
                    return PostDispatchOutcome::Handled;
                }
                use base64::Engine as _;
                let container_b64 =
                    base64::engine::general_purpose::STANDARD.encode(&ctx.envelope.container_bytes);
                let current_total_b64: usize = post_state
                    .pending_tolerance_payments
                    .iter()
                    .map(|p| p.container_b64.len())
                    .sum();
                if post_state.pending_tolerance_payments.len() >= MAX_PENDING_TOLERANCE_PAYMENTS
                    || current_total_b64.saturating_add(container_b64.len())
                        > MAX_PENDING_TOLERANCE_TOTAL_B64_BYTES
                {
                    stats.errors.push(
                        "Pending tolerance payment queue limit reached; confirm or dismiss pending payments first".to_string(),
                    );
                    return PostDispatchOutcome::RetryLater;
                }
                let (sender_name, _, _) = resolve_counterparty_info_for_session(
                    ctx.state,
                    ctx.session,
                    ctx.sender_did,
                    ctx.password,
                );
                post_state.pending_tolerance_payments.push(
                    crate::state::StoredPendingTolerancePayment {
                        msg_id: ctx.envelope.msg_id.clone(),
                        sender_did: ctx.sender_did.to_string(),
                        sender_name,
                        created_at: super::clamp_post_ts(ctx.envelope.created_at, ctx.now_secs),
                        zone: zone.to_string(),
                        error_message: e.to_string(),
                        tag16_hex: hex::encode(ctx.tag16),
                        container_b64,
                        l2_evidence: l2_evidence_owned,
                    },
                );
                stats.new_tolerance_payments += 1;
                return PostDispatchOutcome::Handled;
            }
            if matches!(e, human_money_core::Error::BundlePredatesCurrentEpoch) {
                stats.errors.push(format!(
                    "Receive payment bundle rejected (predates current recovery epoch): {}",
                    e
                ));
                return PostDispatchOutcome::Handled;
            }
            let is_sibling_subaccount = matches!(
                &e,
                human_money_core::Error::BundleRecipientMismatch { expected, found }
                    if crate::state::canonicalize_did(expected)
                        .ok()
                        .is_some_and(|ce| {
                            crate::state::canonicalize_did(found).ok().as_ref() == Some(&ce)
                        })
            );
            let is_duplicate = matches!(e, human_money_core::Error::BundleAlreadyProcessed { .. });
            if is_duplicate {
                queue_ack_receipt(ctx, post_state, None);
                PostDispatchOutcome::Handled
            } else if is_sibling_subaccount {
                PostDispatchOutcome::Handled
            } else {
                stats
                    .errors
                    .push(format!("Receive payment bundle failed: {}", e));
                if is_retryable_bundle_error(&e) {
                    PostDispatchOutcome::RetryLater
                } else {
                    PostDispatchOutcome::Handled
                }
            }
        }
    }
}

fn handle_voucher_for_signing(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    if post_state
        .pending_sign_requests
        .iter()
        .any(|r| r.msg_id == ctx.envelope.msg_id && are_dids_equal(&r.sender_did, ctx.sender_did))
    {
        queue_ack_receipt(ctx, post_state, None);
        return true;
    }
    let current_bytes: usize = post_state
        .pending_sign_requests
        .iter()
        .map(|r| r.container_bytes.len())
        .sum();
    if post_state.pending_sign_requests.len() >= MAX_PENDING_SIGN_REQUESTS
        || current_bytes.saturating_add(ctx.envelope.container_bytes.len())
            > MAX_PENDING_QUEUE_BYTES
    {
        stats.errors.push(
            "Pending sign request queue limit reached; answer or dismiss requests first"
                .to_string(),
        );
        return false;
    }
    let (sender_name, _, _) =
        resolve_counterparty_info_for_session(ctx.state, ctx.session, ctx.sender_did, ctx.password);
    post_state
        .pending_sign_requests
        .push(FrontendPendingSignRequest {
            msg_id: ctx.envelope.msg_id.clone(),
            sender_did: ctx.sender_did.to_string(),
            sender_name,
            created_at: super::clamp_post_ts(ctx.envelope.created_at, ctx.now_secs),
            reply_syncs: super::sanitize_sync_descriptors(
                &ctx.envelope.reply_drops,
                humoco_market::MAX_CARD_DROPS,
            ),
            container_bytes: ctx.envelope.container_bytes.clone(),
        });
    stats.new_sign_requests += 1;
    queue_ack_receipt(ctx, post_state, None);
    true
}

fn is_retryable_post_attach_error(err: &human_money_core::app_service::AppFacadeError) -> bool {
    matches!(
        err,
        human_money_core::app_service::AppFacadeError::WalletLocked
            | human_money_core::app_service::AppFacadeError::SessionExpired(_)
            | human_money_core::app_service::AppFacadeError::SessionNotActive(_)
            | human_money_core::app_service::AppFacadeError::Storage(_)
            | human_money_core::app_service::AppFacadeError::Io(_)
            | human_money_core::app_service::AppFacadeError::VoucherNotFound(_)
            | human_money_core::app_service::AppFacadeError::Wallet(
                human_money_core::error::WalletError::StandardNotFound { .. }
            )
            | human_money_core::app_service::AppFacadeError::App(
                human_money_core::error::AppError::InsufficientAccessMode { .. }
            )
    )
}

fn handle_detached_signature(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    baseline_post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> bool {
    let all_standards = load_all_standard_tomls(ctx.app, ctx.extra_standards);
    let mut attached = false;
    let mut duplicate_already_attached = false;
    let mut retryable_error: Option<String> = None;
    let baseline_snapshot = baseline_post_state.clone();
    let working_snapshot = post_state.clone();
    let committed_post_cell = std::cell::RefCell::new(None::<StoredPostState>);

    let lock_res = ctx.state.with_session(ctx.session, |service| {
        if !service.is_wallet_unlocked() {
            return Ok(Some(
                "Detached signature attachment deferred: wallet session is locked".to_string(),
            ));
        }
        let mut candidate_ps = working_snapshot.clone();
        super::merge_post_state_with_service(
            service,
            &baseline_snapshot,
            &mut candidate_ps,
            ctx.password,
        )?;
        queue_ack_receipt(ctx, &mut candidate_ps, None);
        candidate_ps.processed_msg_ids.insert(
            super::scoped_processed_msg_key(ctx.sender_did, &ctx.envelope.msg_id),
            ctx.now_secs,
        );

        let companion_ps = std::cell::RefCell::new(Some(candidate_ps));
        match service.process_and_attach_signature_with_companions(
            &ctx.envelope.container_bytes,
            &all_standards,
            None,
            ctx.password,
            |_updated_id, _wallet| {
                let next_ps = companion_ps.borrow_mut().take().ok_or_else(|| {
                    human_money_core::Error::Storage(human_money_core::StorageError::InvalidFormat(
                        "Missing post_state companion buffer".to_string(),
                    ))
                })?;
                let bytes = serde_json::to_vec(&next_ps).map_err(|e| {
                    human_money_core::Error::Storage(human_money_core::StorageError::InvalidFormat(
                        format!(
                            "Failed to serialize {}: {}",
                            crate::state::MARKET_POST_STATE_KEY,
                            e
                        ),
                    ))
                })?;
                *committed_post_cell.borrow_mut() = Some(next_ps);
                Ok(vec![(
                    crate::state::MARKET_POST_STATE_KEY.to_string(),
                    bytes,
                )])
            },
        ) {
            Ok(_) => {
                attached = true;
                retryable_error = None;
                let _ = ctx.state.refresh_events_cache(service, ctx.password);
                if let Some(saved_ps) = committed_post_cell.borrow().as_ref() {
                    ctx.state.write_cache_slot_for_service(
                        service,
                        |c| &mut c.post_state,
                        Some(saved_ps.clone()),
                    );
                }
            }
            Err(e) => {
                let is_already_attached = matches!(
                    &e,
                    human_money_core::app_service::AppFacadeError::MismatchedSignatureData(msg)
                        if msg.to_ascii_lowercase().contains("already attached")
                );
                if is_already_attached {
                    duplicate_already_attached = true;
                } else if is_retryable_post_attach_error(&e) {
                    retryable_error = Some(e.to_string());
                }
            }
        }
        Ok(None)
    });
    match lock_res {
        Ok(Some(Some(deferred_msg))) => {
            stats.errors.push(deferred_msg);
            return false;
        }
        Ok(None) => {
            stats
                .errors
                .push("Detached signature attachment deferred: wallet session changed".to_string());
            return false;
        }
        Err(e) => {
            stats.errors.push(format!(
                "Detached signature attachment deferred: {}",
                e.message
            ));
            return false;
        }
        Ok(Some(None)) => {}
    }
    if attached {
        if let Some(saved_ps) = committed_post_cell.into_inner() {
            *post_state = saved_ps;
            *baseline_post_state = post_state.clone();
        }
        stats.imported_signatures += 1;
        return true;
    }
    if duplicate_already_attached {
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
) -> bool {
    let assertion = match serde_json::from_slice::<WotHandshakeAssertion>(ctx.decrypted_bytes) {
        Ok(a) => a,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse WotHandshakeAssertion: {}", e));
            return true;
        }
    };
    if let Err(e) = validate_wot_handshake_assertion(&assertion) {
        stats
            .errors
            .push(format!("Invalid WoT handshake assertion: {}", e));
        return true;
    }
    if !are_dids_equal(&assertion.sender_did, ctx.sender_did) {
        stats.errors.push(format!(
            "Rejected WoT handshake assertion with spoofed sender DID: {} != {}",
            assertion.sender_did, ctx.sender_did
        ));
        return true;
    }

    let is_known_trusted_contact = match ctx.state.with_session(ctx.session, |service| {
        ctx.state
            .load_contacts(service, ctx.password)
            .map(|list| {
                list.iter().any(|c| {
                    are_dids_equal(&c.did, ctx.sender_did) && c.trust_level.unwrap_or(0) >= 1
                })
            })
            .map_err(crate::models::FrontendError::from)
    }) {
        Ok(Some(trusted)) => trusted,
        Ok(None) => {
            stats
                .errors
                .push("WoT assertion deferred: active wallet session changed".to_string());
            return false;
        }
        Err(e) => {
            stats.errors.push(format!(
                "WoT assertion deferred (failed to load contacts): {}",
                e.message
            ));
            return false;
        }
    };

    if is_known_trusted_contact {
        // Silent auto-accept for established contacts (Zero-Click).
        match serde_json::from_str::<humoco_wot::WotSyncPackage>(&assertion.wot_sync_json) {
            Ok(pkg) => {
                match ctx.state.with_session(ctx.session, |service| {
                    let mut reg = ctx
                        .state
                        .get_or_load_wot_register(service, ctx.password)
                        .map_err(crate::models::FrontendError::from)?;
                    let can_sender_did = crate::state::canonicalize_did(ctx.sender_did)
                        .unwrap_or_else(|_| ctx.sender_did.to_string());
                    if let Err(e) =
                        reg.import_sync_package(&pkg, Some(&can_sender_did), ctx.now_secs)
                    {
                        return Ok(Some(format!(
                            "Rejected incoming WoT sync package from {}: {}",
                            ctx.sender_did, e
                        )));
                    }
                    ctx.state
                        .ensure_wot_list_signed(service, &mut reg)
                        .map_err(crate::models::FrontendError::from)?;
                    ctx.state
                        .save_wot_register(service, &reg, ctx.password)
                        .map_err(crate::models::FrontendError::from)?;
                    Ok(None)
                }) {
                    Ok(Some(None)) => {}
                    Ok(Some(Some(pkg_err))) => {
                        stats.errors.push(pkg_err);
                    }
                    Ok(None) => {
                        stats.errors.push(
                            "WoT assertion deferred: active wallet session changed".to_string(),
                        );
                        return false;
                    }
                    Err(e) => {
                        stats.errors.push(format!(
                            "WoT assertion deferred (WoT register error): {}",
                            e.message
                        ));
                        return false;
                    }
                }
            }
            Err(e) => {
                stats
                    .errors
                    .push(format!("Invalid incoming WoT sync package JSON: {}", e));
            }
        }

        // If counterparty requested a mutual sync package, automatically queue a return package.
        if assertion.reply_requested {
            let can_recipient = crate::state::canonicalize_did(ctx.sender_did)
                .unwrap_or_else(|_| ctx.sender_did.to_string());
            match ctx.state.with_session(ctx.session, |service| {
                let mut reg = ctx
                    .state
                    .get_or_load_wot_register(service, ctx.password)
                    .map_err(crate::models::FrontendError::from)?;
                if reg.own_list_dirty || reg.own_signed_list.is_none() {
                    ctx.state
                        .ensure_wot_list_signed(service, &mut reg)
                        .map_err(crate::models::FrontendError::from)?;
                }
                let wot_sync_json = match reg.prepare_sync_package(Some(&can_recipient), None, true)
                {
                    Ok((pkg, _)) => serde_json::to_string(&pkg).ok(),
                    Err(_) => None,
                };
                let my_name = service
                    .with_wallet(|w| w.profile.first_name.clone())
                    .ok()
                    .flatten();
                Ok(wot_sync_json.map(|j| (j, my_name)))
            }) {
                Ok(Some(Some((wot_sync_json, my_name)))) => {
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
                Ok(Some(None)) => {}
                Ok(None) => {
                    stats
                        .errors
                        .push("WoT auto-reply deferred: wallet session changed".to_string());
                    return false;
                }
                Err(e) => {
                    stats
                        .errors
                        .push(format!("WoT auto-reply deferred: {}", e.message));
                    return false;
                }
            }
        }
        queue_ack_receipt(ctx, post_state, None);
        true
    } else {
        if post_state.pending_wot_handshakes.iter().any(|h| {
            h.msg_id == ctx.envelope.msg_id && are_dids_equal(&h.sender_did, ctx.sender_did)
        }) {
            queue_ack_receipt(ctx, post_state, None);
            return true;
        }
        let current_pkg_bytes: usize = post_state
            .pending_wot_packages_by_msg_id
            .values()
            .map(String::len)
            .sum();
        if post_state.pending_wot_handshakes.len() >= MAX_PENDING_WOT_HANDSHAKES
            || current_pkg_bytes.saturating_add(assertion.wot_sync_json.len())
                > MAX_PENDING_QUEUE_BYTES
        {
            stats.errors.push(
                "Pending WoT handshake queue limit reached; accept or dismiss handshakes first"
                    .to_string(),
            );
            return false;
        }
        let scoped_key = super::scoped_processed_msg_key(ctx.sender_did, &ctx.envelope.msg_id);
        post_state
            .pending_wot_packages_by_msg_id
            .insert(scoped_key, assertion.wot_sync_json.clone());
        post_state
            .pending_wot_handshakes
            .push(FrontendPendingWotHandshake {
                msg_id: ctx.envelope.msg_id.clone(),
                sender_did: ctx.sender_did.to_string(),
                sender_name: assertion.sender_name,
                proposed_level: assertion.proposed_level,
                reply_requested: assertion.reply_requested,
                created_at: super::clamp_post_ts(assertion.created_at, ctx.now_secs),
            });
        stats.new_wot_handshakes += 1;
        queue_ack_receipt(ctx, post_state, None);
        true
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
    let mut removed = 0usize;
    post_state.outbox_entries.retain(|e| {
        let matches_sender = are_dids_equal(&e.recipient_did, ctx.sender_did);
        let matches_msg = e.msg_id == receipt.acknowledged_msg_id
            || e.tag16_hex == receipt.acknowledged_tag16_hex;
        if matches_sender && matches_msg {
            removed += 1;
            super::push_pruned_outbox_tag16(
                &mut post_state.pruned_outbox_tag16s,
                e.tag16_hex.clone(),
            );
            false
        } else {
            true
        }
    });
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

    let (canonical_target_opt, ad_exists_in_store) = check_inquiry_target(
        ctx.state,
        ctx.session,
        &inquiry.target,
        ctx.now_secs,
        ctx.password,
    );
    let Some(canonical_target) = canonical_target_opt else {
        match &inquiry.target {
            InquiryTarget::Ad { ad_id, .. } => {
                if ad_exists_in_store && get_pubkey_from_user_id(ctx.sender_did).is_ok() {
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
    };

    if let Some(existing) = post_state
        .inquiries
        .iter()
        .find(|i| i.inquiry_id == inquiry.inquiry_id)
    {
        if are_dids_equal(&existing.counterparty_did, ctx.sender_did) {
            queue_ack_receipt(ctx, post_state, None);
        } else {
            stats.errors.push(format!(
                "Rejected duplicate inquiry_id {} from different counterparty {}",
                inquiry.inquiry_id, ctx.sender_did
            ));
        }
        return;
    }

    let sender_pk_hex = get_pubkey_from_user_id(ctx.sender_did)
        .map(|pk| hex::encode(pk.as_bytes()))
        .unwrap_or_default();
    let target_key = super::inquiry_rate_limit_key(&canonical_target);
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

    let ts = super::clamp_post_ts(inquiry.created_at, ctx.now_secs);
    let (sender_name, sender_pic, is_contact) =
        resolve_counterparty_info_for_session(ctx.state, ctx.session, ctx.sender_did, ctx.password);
    stats.last_inquiry_notice_target = Some((inquiry.inquiry_id.clone(), sender_name.clone()));
    post_state.inquiries.push(FrontendMarketInquiryItem {
        inquiry_id: inquiry.inquiry_id.clone(),
        role: "seller".to_string(),
        target: canonical_target,
        counterparty_did: ctx.sender_did.to_string(),
        counterparty_name: sender_name,
        counterparty_picture_url: sender_pic,
        is_contact,
        status: "open".to_string(),
        payment_uri: None,
        amount_hint: None,
        standard_uuid: None,
        created_at: ts,
        updated_at: ts,
        unread: true,
        reply_syncs: super::sanitize_sync_descriptors(
            &ctx.envelope.reply_drops,
            humoco_market::MAX_CARD_DROPS,
        ),
        messages: vec![FrontendInquiryChatMessage {
            msg_id: ctx.envelope.msg_id.clone(),
            sender: "them".to_string(),
            text: Some(inquiry.message),
            decision: None,
            created_at: ts,
        }],
    });
    stats.new_inquiries += 1;
    queue_ack_receipt(ctx, post_state, None);
}

fn extract_payment_uri_recipient(uri: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(uri.trim()).ok()?;
    if !parsed.scheme().eq_ignore_ascii_case("humoco") {
        return None;
    }
    let is_pay = parsed
        .host_str()
        .is_some_and(|h| h.eq_ignore_ascii_case("pay"))
        || parsed.path().trim_matches('/').eq_ignore_ascii_case("pay");
    if !is_pay {
        return None;
    }
    let mut tos = parsed.query_pairs().filter(|(k, _)| k == "to");
    let (_, first) = tos.next()?;
    if tos.next().is_some() {
        return None;
    }
    let trimmed = first.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn handle_market_inquiry_message(
    ctx: &PostEnvelopeContext<'_>,
    post_state: &mut StoredPostState,
    stats: &mut PostDispatchStats,
) -> PostDispatchOutcome {
    let msg = match serde_json::from_slice::<MarketInquiryMessage>(ctx.decrypted_bytes) {
        Ok(m) => m,
        Err(e) => {
            stats
                .errors
                .push(format!("Failed to parse MarketInquiryMessage: {}", e));
            return PostDispatchOutcome::Handled;
        }
    };
    if let Err(e) = validate_market_inquiry_message(&msg) {
        stats
            .errors
            .push(format!("Invalid market inquiry message: {}", e));
        return PostDispatchOutcome::Handled;
    }
    let Some(item) = post_state.inquiries.iter_mut().find(|i| {
        i.inquiry_id == msg.inquiry_id && are_dids_equal(&i.counterparty_did, ctx.sender_did)
    }) else {
        return if super::is_within_retry_window(ctx.envelope.created_at, ctx.now_secs) {
            PostDispatchOutcome::RetryLater
        } else {
            PostDispatchOutcome::Handled
        };
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
            if dec_str == "confirm" {
                item.status = "confirmed".to_string();
                if let Some(ref raw_uri) = msg.payment_uri {
                    if extract_payment_uri_recipient(raw_uri)
                        .is_some_and(|to_did| are_dids_equal(&to_did, &item.counterparty_did))
                    {
                        item.payment_uri = Some(raw_uri.clone());
                    } else {
                        stats.errors.push(format!(
                            "Dropped mismatched inquiry payment URI for thread {}",
                            msg.inquiry_id
                        ));
                    }
                }
                if msg.amount_hint.is_some() {
                    item.amount_hint = msg.amount_hint.clone();
                }
                if msg.standard_uuid.is_some() {
                    item.standard_uuid = msg.standard_uuid.clone();
                }
            } else {
                item.status = "declined".to_string();
            }
        }

        if !ctx.envelope.reply_drops.is_empty() {
            let sanitized_reply = super::sanitize_sync_descriptors(
                &ctx.envelope.reply_drops,
                humoco_market::MAX_CARD_DROPS,
            );
            if !sanitized_reply.is_empty() {
                item.reply_syncs = sanitized_reply;
            }
        }

        let msg_ts = super::clamp_post_ts(msg.created_at, ctx.now_secs);
        item.messages.push(FrontendInquiryChatMessage {
            msg_id: msg.msg_id.clone(),
            sender: "them".to_string(),
            text: msg.text.clone(),
            decision: valid_decision,
            created_at: msg_ts,
        });
        item.messages
            .sort_by(|a, b| a.created_at.cmp(&b.created_at));
        item.updated_at = item.updated_at.max(msg_ts);
        item.unread = true;
        stats.new_inquiry_messages += 1;
        notice_inquiry_id = Some(item.inquiry_id.clone());
    }

    if let Some(inq_id) = notice_inquiry_id {
        let cp_name = resolve_counterparty_info_for_session(
            ctx.state,
            ctx.session,
            ctx.sender_did,
            ctx.password,
        )
        .0;
        stats.last_inquiry_notice_target = Some((inq_id, cp_name));
    }
    queue_ack_receipt(ctx, post_state, None);
    PostDispatchOutcome::Handled
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

    let trust_check: Result<bool, String> = match ctx.state.with_session(ctx.session, |service| {
        let is_offender = match service.with_wallet(|w| {
            matches!(
                w.check_reputation(ctx.sender_did),
                human_money_core::models::conflict::TrustStatus::KnownOffender(_)
            )
        }) {
            Ok(v) => v,
            Err(e) => {
                return Ok(Err(format!(
                    "Guardian assign deferred (wallet unavailable): {}",
                    e
                )));
            }
        };
        if is_offender {
            return Ok(Ok(false));
        }
        match ctx.state.load_contacts(service, ctx.password) {
            Ok(list) => Ok(Ok(list.iter().any(|c| {
                are_dids_equal(&c.did, ctx.sender_did) && c.trust_level.unwrap_or(0) >= 1
            }))),
            Err(e) => Ok(Err(format!(
                "Guardian assign deferred (failed to load contacts): {}",
                e
            ))),
        }
    }) {
        Ok(Some(res)) => res,
        Ok(None) => Err("Guardian assign deferred: active wallet session changed".to_string()),
        Err(e) => Err(format!("Guardian assign deferred: {}", e.message)),
    };

    let is_trusted_non_offender = match trust_check {
        Ok(trusted) => trusted,
        Err(err_msg) => {
            stats.errors.push(err_msg);
            return false;
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

    let save_res: Result<(), (String, bool)> =
        match ctx.state.with_session(ctx.session, |service| {
            Ok(match load_guardian_vault(service, ctx.password) {
                Ok(mut vault) => match vault.upsert_ward(entry.clone()) {
                    Ok(()) => {
                        save_guardian_vault_with_cache(ctx.state, service, &vault, ctx.password)
                            .map_err(|e| {
                                (
                                    format!("Failed to save guardian vault: {}", e.message),
                                    false,
                                )
                            })
                    }
                    Err(e) => Err((format!("Failed to upsert guardian ward: {}", e), true)),
                },
                Err(e) => Err((
                    format!("Failed to load guardian vault: {}", e.message),
                    false,
                )),
            })
        }) {
            Ok(Some(res)) => res,
            Ok(None) => Err((
                "Guardian assign deferred: active wallet session changed".to_string(),
                false,
            )),
            Err(e) => Err((format!("Guardian assign failed: {}", e.message), false)),
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
    match ctx.state.with_session(ctx.session, |service| {
        let owner_opt = match load_recovery_owner_state(service, ctx.password) {
            Ok(opt) => opt,
            Err(e) => {
                return Ok(Err(format!(
                    "Failed to load recovery owner state: {}",
                    e.message
                )));
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
                            acked_at: super::clamp_post_ts(ack.acked_at, ctx.now_secs),
                        };
                        modified = true;
                    }
                }
                if modified {
                    if let Err(e) = save_recovery_owner_state_with_cache(
                        ctx.state,
                        service,
                        &owner_state,
                        ctx.password,
                    ) {
                        return Ok(Err(format!(
                            "Failed to save recovery owner state: {}",
                            e.message
                        )));
                    }
                }
            }
        }
        Ok(Ok(()))
    }) {
        Ok(Some(Ok(()))) => {}
        Ok(Some(Err(msg))) => {
            stats.errors.push(msg);
            return false;
        }
        Ok(None) => return false,
        Err(e) => {
            stats.errors.push(e.message);
            return false;
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

    let can_owner = match humoco_market::canonicalize_owner_account_id(&update.owner_did) {
        Ok(k) => k,
        Err(e) => {
            stats
                .errors
                .push(format!("Invalid GuardianLocatorUpdate owner DID: {}", e));
            return true;
        }
    };

    let update_res: Result<(), (String, bool)> =
        match ctx.state.with_session(ctx.session, |service| {
            Ok(match load_guardian_vault(service, ctx.password) {
                Ok(mut vault) => {
                    let blocked_by_epoch = vault
                        .revoked_epoch_by_owner
                        .get(&can_owner)
                        .is_some_and(|&rev| update.rotation_epoch <= rev)
                        || vault
                            .highest_epoch_by_owner
                            .get(&can_owner)
                            .is_some_and(|&hi| update.rotation_epoch < hi);
                    let awaiting_assign = !blocked_by_epoch
                        && match vault.wards.get(&can_owner) {
                            None => true,
                            Some(entry) => update.rotation_epoch > entry.rotation_epoch,
                        };
                    if awaiting_assign {
                        let within_window =
                            super::is_within_retry_window(ctx.envelope.created_at, ctx.now_secs);
                        Err((
                            "Guardian locator deferred: awaiting matching guardian assignment"
                                .to_string(),
                            !within_window,
                        ))
                    } else {
                        match vault.update_locator(&update) {
                            Ok(updated) => {
                                if updated {
                                    save_guardian_vault_with_cache(
                                        ctx.state,
                                        service,
                                        &vault,
                                        ctx.password,
                                    )
                                    .map_err(|e| {
                                        (
                                            format!("Failed to save guardian vault: {}", e.message),
                                            false,
                                        )
                                    })
                                } else {
                                    Ok(())
                                }
                            }
                            Err(e) => {
                                Err((format!("Failed to update guardian locator: {}", e), true))
                            }
                        }
                    }
                }
                Err(e) => Err((
                    format!("Failed to load guardian vault: {}", e.message),
                    false,
                )),
            })
        }) {
            Ok(Some(res)) => res,
            Ok(None) => Err((
                "Guardian locator update deferred: wallet session changed".to_string(),
                false,
            )),
            Err(e) => Err((
                format!("Guardian locator update failed: {}", e.message),
                false,
            )),
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

    let revoke_res: Result<(), (String, bool)> =
        match ctx.state.with_session(ctx.session, |service| {
            Ok(match load_guardian_vault(service, ctx.password) {
                Ok(mut vault) => match vault.revoke_ward(&revoke) {
                    Ok(revoked) => {
                        if revoked {
                            save_guardian_vault_with_cache(ctx.state, service, &vault, ctx.password)
                                .map_err(|e| {
                                    (
                                        format!("Failed to save guardian vault: {}", e.message),
                                        false,
                                    )
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
                Err(e) => Err((
                    format!("Failed to load guardian vault: {}", e.message),
                    false,
                )),
            })
        }) {
            Ok(Some(res)) => res,
            Ok(None) => Err((
                "Guardian revoke deferred: wallet session changed".to_string(),
                false,
            )),
            Err(e) => Err((format!("Guardian revoke failed: {}", e.message), false)),
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

    #[tokio::test]
    async fn test_corrupt_guardian_vault_returns_retry_later_and_preserves_bytes() {
        let base_dir = temp_test_dir("handlers_corrupt_guardian_vault");
        let password = "TestPassword123!";
        let service = setup_test_app(&base_dir, "Alice", ALICE_PHRASE, password);
        let state = create_test_state(service);

        let (session, my_identity, sender_did) = {
            let mut s = state.service.lock().unwrap();
            let session = state.session_token_for_service(&s).unwrap();
            crate::state::save_encrypted_bytes(
                &mut s,
                RECOVERY_GUARDIAN_VAULT_KEY,
                b"{corrupt-vault",
                Some(password),
            )
            .unwrap();
            let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
            let did = id.user_id.clone();
            (session, id, did)
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
            sender_did.clone(),
            1_750_000_000,
            Vec::new(),
            Vec::new(),
        );
        let tag16 = [7u8; 16];
        let my_ed_sk = my_identity.signing_key.clone();
        let ctx = PostEnvelopeContext {
            state: &state,
            session: &session,
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
        let mut baseline_post_state = post_state.clone();
        let mut stats = PostDispatchStats::default();
        let outcome = dispatch_post_payload(
            &ctx,
            &PayloadType::Generic(humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1.to_string()),
            &mut post_state,
            &mut baseline_post_state,
            &mut stats,
        )
        .await;

        assert_eq!(
            outcome,
            PostDispatchOutcome::RetryLater,
            "Corrupt guardian vault store must return RetryLater so msg_id stays retryable"
        );
        assert!(!stats.errors.is_empty());

        let stored_raw = {
            let mut s = state.service.lock().unwrap();
            crate::state::load_encrypted_bytes_opt(
                &mut s,
                RECOVERY_GUARDIAN_VAULT_KEY,
                Some(password),
            )
            .unwrap()
            .unwrap()
        };
        assert_eq!(stored_raw, b"{corrupt-vault");

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn h14_unknown_post_payload_records_unsupported_and_dismisses() {
        let base_dir = temp_test_dir("h14_unsupported_payload");
        let password = "TestPassword123!";
        let service = setup_test_app(&base_dir, "Alice", ALICE_PHRASE, password);
        let state = create_test_state(service);

        let (session, my_identity, sender_did) = {
            let s = state.service.lock().unwrap();
            let session = state.session_token_for_service(&s).unwrap();
            let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
            let did = id.user_id.clone();
            (session, id, did)
        };

        let envelope = humoco_market::PostEnvelope::new(
            "msg-unsupported-v99",
            sender_did.clone(),
            1_750_000_001,
            Vec::new(),
            Vec::new(),
        );
        let tag16 = [3u8; 16];
        let my_ed_sk = my_identity.signing_key.clone();
        let ctx = PostEnvelopeContext {
            state: &state,
            session: &session,
            app: None,
            extra_standards: None,
            password: Some(password),
            my_identity: &my_identity,
            my_ed_sk: &my_ed_sk,
            own_reply_syncs: &[],
            sender_did: &sender_did,
            envelope: &envelope,
            tag16: &tag16,
            decrypted_bytes: br#"{"future":true}"#,
            now_secs: 1_750_000_001,
        };

        let mut post_state = StoredPostState::default();
        let mut baseline_post_state = post_state.clone();
        let mut stats = PostDispatchStats::default();
        let outcome = dispatch_post_payload(
            &ctx,
            &PayloadType::Generic("future-feature+v99".to_string()),
            &mut post_state,
            &mut baseline_post_state,
            &mut stats,
        )
        .await;

        assert_eq!(
            outcome,
            PostDispatchOutcome::UnsupportedPayload {
                payload_type: "future-feature+v99".to_string()
            }
        );

        // Mirror inbox_poller persistence when UnsupportedPayload is returned
        post_state
            .unsupported_messages
            .push(crate::models::FrontendUnsupportedPostItem {
                msg_id: envelope.msg_id.clone(),
                sender_did: sender_did.clone(),
                sender_name: None,
                payload_type: "future-feature+v99".to_string(),
                created_at: envelope.created_at,
            });
        {
            let mut s = state.service.lock().unwrap();
            crate::state::save_encrypted_json(
                &mut s,
                crate::state::MARKET_POST_STATE_KEY,
                &post_state,
                Some(password),
            )
            .unwrap();
        }

        let summary = super::super::inbox_poller::get_post_inbox_summary_impl(
            Some(password.to_string()),
            &state,
        )
        .unwrap();
        assert_eq!(summary.unsupported_messages.len(), 1);
        assert_eq!(
            summary.unsupported_messages[0].payload_type,
            "future-feature+v99"
        );

        super::super::inbox_poller::dismiss_pending_post_item_impl(
            "unsupported".to_string(),
            envelope.msg_id.clone(),
            Some(password.to_string()),
            &state,
        )
        .unwrap();
        let after = super::super::inbox_poller::get_post_inbox_summary_impl(
            Some(password.to_string()),
            &state,
        )
        .unwrap();
        assert!(after.unsupported_messages.is_empty());

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
