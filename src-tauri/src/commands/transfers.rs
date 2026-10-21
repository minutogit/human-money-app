// src-tauri/src/commands/transfers.rs
use crate::{
    commands::settings_cmd,
    models::{
        CreateBundleResult, FrontendRankedVoucher, FrontendTransferPlan,
        FrontendSourceTransfer, FrontendTransactionRecord,
        ReceiveSuccessPayload, TransactionRecord, FrontendError,
        FrontendCheckoutReleaseVerdict,
    },
    AppState,
};
use chrono::Utc;
use log::{info, error, warn};
use uuid::Uuid;
use std::collections::HashMap;
use human_money_core::{
    app_service::AppService,
    archive::FileVoucherArchive,
    humoco_proto::live::{LiveStateContext, LiveStateProof},
    models::layer2_api::L2ProcessingOutcome,
    wallet::{MultiTransferRequest, SourceTransfer},
};

/// i18n key for the "no reachable L2 node" condition.
const ERR_L2_OFFLINE: &str = "error.network.l2Offline";

/// Pre-flight Layer 2 connectivity gate (M6.1).
///
/// The wallet is offline-first: a transfer bundle is created locally and stays
/// cryptographically valid without any network. Therefore the connectivity check
/// is only *enforced* when the user pinned the app to their own L2 servers
/// (`exclusive_custom_servers`). In that mode the pinned infrastructure is the
/// only possible source of confirmations — an unreachable set means the payment
/// could never be confirmed by the network, so the user is told about the
/// offline state instead of silently degrading to an unconfirmable transfer.
///
/// In every other mode the PEX/seed fallback of the resolver stays available and
/// offline transfers remain possible, so no error is raised.
async fn ensure_l2_connectivity(state: &AppState) -> Result<(), FrontendError> {
    let settings = match state.get_cached_settings() {
        Ok(settings) => settings,
        Err(e) => {
            // Settings cache is not initialized (e.g. no active session yet).
            // The wallet lock produces the proper error later, so do not block here.
            warn!("L2 connectivity check skipped, settings unavailable: {}", e);
            return Ok(());
        }
    };

    if !settings.exclusive_custom_servers {
        // Not pinned to custom servers: offline transfers are explicitly supported.
        return Ok(());
    }

    let reachable = settings_cmd::resolve_active_l2_nodes_internal(&settings).await;

    if reachable.is_empty() {
        let configured = settings
            .custom_servers
            .iter()
            .filter(|s| s.is_active && !s.url.trim().is_empty())
            .count();
        error!(
            "L2 connectivity check failed: no reachable node in exclusive custom server mode ({} configured)",
            configured
        );
        let mut details = HashMap::new();
        details.insert("configuredServers".to_string(), configured.to_string());
        return Err(FrontendError {
            code: ERR_L2_OFFLINE.to_string(),
            message: format!(
                "Offline: No Layer 2 node is reachable ({} configured custom server(s) in exclusive mode). \
                 The transfer cannot be confirmed by the network. Check your server settings or disable the exclusive custom server mode.",
                configured
            ),
            details,
        });
    }

    info!(
        "L2 connectivity check passed: {} node(s) reachable",
        reachable.len()
    );
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_transfer_bundle(
    recipient_id: String,
    sources: Vec<FrontendSourceTransfer>,
    notes: Option<String>,
    sender_profile_name: Option<String>,
    standard_definitions_toml: HashMap<String, String>,
    use_privacy_mode: Option<bool>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<CreateBundleResult, FrontendError> {
    let recipient_id = recipient_id.trim().to_string();
    info!(
        "Attempting to create a transfer bundle for recipient: {}",
        recipient_id
    );

    // M6.1: Pre-flight L2 connectivity gate. This must happen before the service
    // lock is taken, because the resolver performs network requests.
    ensure_l2_connectivity(&state).await?;

    let mut service = state.service.lock().unwrap();

    let source_transfers = sources
        .into_iter()
        .map(|s| SourceTransfer {
            local_instance_id: s.local_instance_id,
            amount_to_send: s.amount_to_send,
        })
        .collect();

    let request = MultiTransferRequest {
        recipient_id,
        sources: source_transfers,
        notes,
        sender_profile_name,
        use_privacy_mode,
    };

    let archive: Option<&FileVoucherArchive> = None;

    match service.create_transfer_bundle(request, &standard_definitions_toml, archive, password.as_deref()) {
        Ok(result) => {
            if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
                error!("Failed to refresh events cache after create_transfer_bundle: {}", e);
            }

            Ok(CreateBundleResult {
                bundle_data: result.bundle_bytes,
                bundle_id: result.bundle_id,
                involved_sources_details: result.involved_sources_details.into_iter().map(|s| s.into()).collect(),
            })
        }
        Err(e) => Err(FrontendError::from(e)),
    }
}

#[tauri::command]
pub fn receive_bundle(
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    state: tauri::State<AppState>,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    info!("Attempting to receive and process a transfer bundle...");
    let mut service = state.service.lock().unwrap();
    let archive: Option<&FileVoucherArchive> = None;

    match service.receive_bundle(&bundle_data, &standard_definitions_toml, archive, password.as_deref(), force_accept_tolerance_bundle) {
        Ok(result) => {
            info!("Bundle processed successfully. Creating transaction record.");

            let conflict_count = result.check_result.verifiable_conflicts.len();
            let warning_count = result.check_result.unverifiable_warnings.len();

            if conflict_count > 0 || warning_count > 0 {
                info!(
                    "FRAUD DETECTION: Processed bundle from {} | Confirmed Conflicts: {} | Gossip Warnings: {}",
                    result.header.sender_id, conflict_count, warning_count
                );
            }

            let transfer_summary = result.transfer_summary.clone();
            let involved_vouchers_details = result.involved_vouchers_details.clone();
            let involved_vouchers_ids: Vec<String> = involved_vouchers_details.iter().map(|d| d.local_instance_id.clone()).collect();
            let notes = result.header.notes.clone();
            let sender_profile_name = result.header.sender_profile_name.clone();
            let bundle_id = result.header.bundle_id.clone();
            let recipient_id = service.with_wallet(|w| w.get_user_id().to_string()).map_err(FrontendError::from)?;

            let record = TransactionRecord {
                id: Uuid::new_v4().to_string(),
                direction: "received".to_string(),
                recipient_id,
                sender_id: result.header.sender_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts: transfer_summary.summable_amounts.clone(),
                countable_items: transfer_summary.countable_items.clone(),
                involved_vouchers: involved_vouchers_ids.clone(),
                involved_sources_details: Some(involved_vouchers_details.clone()),
                bundle_data: Vec::new(),
                bundle_id,
                notes: notes.clone(),
                sender_profile_name: sender_profile_name.clone(),
            };

            state.append_to_history(&mut service, record.clone(), password.as_deref()).map_err(FrontendError::from)?;

            if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
                error!("Failed to refresh events cache after receive_bundle: {}", e);
            }

            Ok(ReceiveSuccessPayload {
                sender_id: result.header.sender_id,
                sender_profile_name,
                notes,
                transfer_summary: result.transfer_summary.into(),
                involved_vouchers: involved_vouchers_ids,
                involved_vouchers_details: involved_vouchers_details.into_iter().map(|d| d.into()).collect(),
                verifiable_conflicts: result.check_result.verifiable_conflicts,
                conflict_summaries: service.list_conflicts().map_err(FrontendError::from)?.into_iter().map(|s| s.into()).collect(),
                checkout_verdict: None,
            })
        }
        Err(e) => {
            error!("Failed to process bundle: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

#[tauri::command]
pub fn save_transaction_record(
    record: FrontendTransactionRecord,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    let record: TransactionRecord = record.into();
    info!("Saving new transaction record with id: {}", record.id);
    let mut service = state.service.lock().unwrap();
    state.append_to_history(&mut service, record, password.as_deref()).map_err(FrontendError::from)
}

#[tauri::command]
pub fn get_transaction_history(state: tauri::State<AppState>) -> Result<Vec<FrontendTransactionRecord>, FrontendError> {
    info!("Loading transaction history...");
    let history = state.get_cached_history().map_err(FrontendError::from)?;
    Ok(history.iter().map(|r| r.into()).collect())
}

#[tauri::command]
pub fn prepare_transfer_plan(
    target_amount: String,
    standard_uuid_filter: Option<String>,
    recipient_id: String,
    state: tauri::State<AppState>,
) -> Result<FrontendTransferPlan, FrontendError> {
    let recipient_id = recipient_id.trim().to_string();
    info!(
        "Preparing transfer plan for recipient {} amount {} filter {:?}",
        recipient_id, target_amount, standard_uuid_filter
    );
    let service = state.service.lock().unwrap();
    let plan = service
        .with_wallet(|w| w.prepare_transfer(&target_amount, standard_uuid_filter.as_deref(), &recipient_id))
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;
    Ok(plan.into())
}

#[tauri::command]
pub fn rank_vouchers_for_recipient(
    target_amount: String,
    standard_uuid_filter: Option<String>,
    recipient_id: String,
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendRankedVoucher>, FrontendError> {
    let recipient_id = recipient_id.trim().to_string();
    info!(
        "Ranking vouchers for recipient {} amount {} filter {:?}",
        recipient_id, target_amount, standard_uuid_filter
    );
    let service = state.service.lock().unwrap();
    let ranked = service
        .with_wallet(|w| w.rank_vouchers(&target_amount, standard_uuid_filter.as_deref(), &recipient_id))
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;
    Ok(ranked.into_iter().map(|r| r.into()).collect())
}

/// Builds the transaction-history record for a verified live checkout (M6.2).
///
/// The record is the audit trail of a goods release that was authorized by a
/// live, recipient-challenge-bound L2 quorum proof. The locally stored voucher
/// (resolved through the L2 voucher id of the live context) contributes amount,
/// unit and counterparty, while the live context itself contributes the
/// confirmed successor transaction (`child_lock`).
fn build_verified_checkout_record(
    service: &AppService,
    proof: &LiveStateProof,
) -> TransactionRecord {
    let context: &LiveStateContext = &proof.context;

    // Resolve the local voucher instance the live proof refers to.
    let resolved = service
        .with_wallet(|w| {
            w.voucher_store
                .vouchers
                .iter()
                .find(|(_, instance)| {
                    instance
                        .voucher
                        .transactions
                        .first()
                        .and_then(|tx| {
                            human_money_core::services::l2_gateway::calculate_layer2_voucher_id(tx).ok()
                        })
                        .map(|l2_voucher_id| l2_voucher_id == context.voucher_id)
                        .unwrap_or(false)
                })
                .map(|(local_id, instance)| (local_id.clone(), instance.voucher.clone()))
        })
        .ok()
        .flatten();

    let own_user_id = service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap_or_default();

    // The immediate sender (payer) of the voucher, if it can be resolved locally.
    let sender_id = resolved.as_ref().and_then(|(local_id, _)| {
        service
            .with_wallet_and_identity(|w, identity| w.get_voucher_source_sender(local_id, identity))
            .ok()
            .and_then(|sender| sender.ok())
            .flatten()
    });

    let (amount, unit) = resolved
        .as_ref()
        .and_then(|(_, voucher)| {
            let amount = voucher.transactions.last().map(|tx| tx.amount.clone())?;
            let unit = voucher.nominal_value.unit.clone();
            Some((amount, unit))
        })
        .unwrap_or_else(|| (String::new(), String::new()));

    let mut summable_amounts = HashMap::new();
    if !amount.is_empty() && !unit.is_empty() {
        summable_amounts.insert(unit, amount);
    }

    let notes = format!(
        "Verified live checkout: goods release authorized by L2 quorum proof ({}/20 votes, shard {}).",
        proof.votes.len(),
        context.shard_id
    );

    TransactionRecord {
        id: Uuid::new_v4().to_string(),
        direction: "received".to_string(),
        recipient_id: own_user_id,
        sender_id: sender_id.unwrap_or_default(),
        timestamp: Utc::now().to_rfc3339(),
        summable_amounts,
        countable_items: HashMap::new(),
        involved_vouchers: resolved
            .as_ref()
            .map(|(local_id, _)| vec![local_id.clone()])
            .unwrap_or_default(),
        involved_sources_details: None,
        bundle_data: Vec::new(),
        bundle_id: human_money_core::hex::encode(context.child_lock),
        notes: Some(notes),
        sender_profile_name: None,
    }
}

#[tauri::command]
pub fn verify_live_checkout_and_release(
    proof_json: String,
    expected_context_json: String,
    explicit_merchant_approval: bool,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendCheckoutReleaseVerdict, FrontendError> {
    info!("Verifying live checkout proof with recipient challenge binding...");
    let mut service = state.service.lock().unwrap();

    let proof: LiveStateProof = serde_json::from_str(&proof_json)
        .map_err(|e| FrontendError::from(format!("Failed to parse LiveStateProof: {}", e)))?;
    let expected: LiveStateContext = serde_json::from_str(&expected_context_json)
        .map_err(|e| FrontendError::from(format!("Failed to parse LiveStateContext: {}", e)))?;

    let verdict = service.verify_live_checkout_proof(&proof, &expected, explicit_merchant_approval)
        .map_err(FrontendError::from)?;

    // M6.2: A goods release that is authorized by a verified live quorum proof is
    // persisted in the transaction history. Without that record the release decision
    // would leave no audit trail at all. The write is fail-closed on purpose: if the
    // record cannot be stored, the release is not handed out to the frontend.
    let goods_release_authorized = verdict.is_goods_release_authorized
        && verdict.can_release_goods_automatically
        && matches!(verdict.outcome, L2ProcessingOutcome::Confirmed);

    if goods_release_authorized {
        let record = build_verified_checkout_record(&service, &proof);
        info!(
            "Persisting verified checkout record {} in the transaction history",
            record.id
        );
        state
            .append_to_history(&mut service, record, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    Ok(verdict.into())
}

/// Evaluates a goods release verdict for a caller-supplied outcome string.
///
/// # Caller-supplied outcomes never authorize automatic goods release
///
/// The `outcome` string is untrusted input. It must never be promoted to
/// [`L2ProcessingOutcome::Confirmed`](human_money_core::models::layer2_api::L2ProcessingOutcome::Confirmed),
/// because that variant is reserved for a live, recipient-challenge-bound quorum proof.
/// Therefore `"Confirmed"` is mapped to `Found`, and the returned verdict is defensively
/// forced to `can_release_goods_automatically == false`.
///
/// Only [`verify_live_checkout_and_release`] may authorize automatic goods release.
#[tauri::command]
pub fn evaluate_checkout_release(
    outcome: String,
    explicit_merchant_approval: bool,
    state: tauri::State<AppState>,
) -> Result<FrontendCheckoutReleaseVerdict, FrontendError> {
    let service = state.service.lock().unwrap();
    let l2_outcome = match outcome.as_str() {
        // Defensive: a raw caller string is never proof of a live, challenge-bound
        // quorum, so it must not be promoted to `Confirmed`.
        "Confirmed" => human_money_core::models::layer2_api::L2ProcessingOutcome::Found,
        "Found" => human_money_core::models::layer2_api::L2ProcessingOutcome::Found,
        "Provisional" => human_money_core::models::layer2_api::L2ProcessingOutcome::Provisional,
        s if s.starts_with("Quarantined") => human_money_core::models::layer2_api::L2ProcessingOutcome::Quarantined {
            reason: s.strip_prefix("Quarantined:").unwrap_or("Conflict").to_string(),
        },
        s if s.starts_with("SyncRequired") => human_money_core::models::layer2_api::L2ProcessingOutcome::SyncRequired {
            sync_point: s.strip_prefix("SyncRequired:").unwrap_or("0").to_string(),
        },
        _ => human_money_core::models::layer2_api::L2ProcessingOutcome::Found,
    };

    let mut verdict = service.evaluate_checkout_release(l2_outcome, explicit_merchant_approval);
    // Defensive: only `verify_live_checkout_and_release` may authorize automatic goods
    // release, so a caller-provided outcome must never enable it.
    verdict.can_release_goods_automatically = false;
    Ok(verdict.into())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn verify_checkout_response_with_authority(
    local_id: String,
    response_bytes: Vec<u8>,
    network: u8,
    shard_id: u16,
    parent_lock_hex: String,
    child_lock_hex: String,
    committee_pubkeys_hex: Vec<String>,
    explicit_merchant_approval: bool,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendCheckoutReleaseVerdict, FrontendError> {
    info!("Verifying checkout response with authority for voucher {}", local_id);
    let mut service = state.service.lock().unwrap();

    let parent_lock = human_money_core::hex::decode(&parent_lock_hex)
        .map_err(|e| FrontendError::from(format!("Invalid parent_lock hex: {}", e)))?;
    let child_lock = human_money_core::hex::decode(&child_lock_hex)
        .map_err(|e| FrontendError::from(format!("Invalid child_lock hex: {}", e)))?;
    if parent_lock.len() != 32 || child_lock.len() != 32 {
        return Err(FrontendError::from("Locks must be 32 bytes".to_string()));
    }

    let mut committee = Vec::new();
    for pk_hex in committee_pubkeys_hex {
        let pk_bytes = human_money_core::hex::decode(&pk_hex)
            .map_err(|e| FrontendError::from(format!("Invalid committee pubkey hex: {}", e)))?;
        if pk_bytes.len() != 32 {
            return Err(FrontendError::from("Committee keys must be 32 bytes".to_string()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&pk_bytes);
        committee.push(arr);
    }

    let mut p_arr = [0u8; 32];
    p_arr.copy_from_slice(&parent_lock);
    let mut c_arr = [0u8; 32];
    c_arr.copy_from_slice(&child_lock);

    let authority = human_money_core::services::l2_gateway::CheckoutAuthority {
        network,
        shard_id,
        parent_lock: p_arr,
        child_lock: c_arr,
        committee,
    };

    let verdict = service.verify_checkout_response_with_authority(
        local_id,
        &response_bytes,
        password.as_deref(),
        authority,
        explicit_merchant_approval,
    ).map_err(FrontendError::from)?;

    Ok(verdict.into())
}
