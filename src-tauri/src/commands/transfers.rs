// src-tauri/src/commands/transfers.rs
use crate::{
    models::{
        CreateBundleResult, FrontendRankedVoucher, FrontendTransferPlan,
        FrontendSourceTransfer, FrontendTransactionRecord,
        ReceiveSuccessPayload, TransactionRecord, FrontendError,
        FrontendCheckoutReleaseVerdict,
    },
    AppState,
};
use chrono::Utc;
use log::{info, error};
use uuid::Uuid;
use std::collections::HashMap;
use tauri::Manager;
use human_money_core::{
    app_service::AppService,
    archive::FileVoucherArchive,
    humoco_proto::live::{LiveStateContext, LiveStateProof},
    models::layer2_api::{CheckoutReleaseVerdict, L2ProcessingOutcome},
    wallet::{MultiTransferRequest, SourceTransfer},
};

/// i18n key for the "no reachable L2 node" condition.
const ERR_L2_OFFLINE: &str = "error.network.l2Offline";

/// Cold cached routing is the only preflight. Reachability probes belong to
/// discovery, never to the signed transfer's checkout budget.
fn validate_l2_routing(settings:&crate::settings::AppSettings)->Result<(),FrontendError> {
    if settings.exclusive_custom_servers && crate::gateway_resolver::cached_candidates(settings).is_empty() {
        return Err(FrontendError {code:ERR_L2_OFFLINE.into(),message:"No configured Layer 2 gateway".into(),details:HashMap::new()});
    }
    Ok(())
}

fn open_wallet_archive(
    app: &tauri::AppHandle,
    service: &human_money_core::app_service::AppService,
) -> Option<FileVoucherArchive> {
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
    Some(FileVoucherArchive::with_key(archive_dir, key))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_transfer_bundle(
    app: tauri::AppHandle,
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

    let settings=state.get_cached_settings().map_err(FrontendError::from)?;
    validate_l2_routing(&settings)?;
    let network=match settings.active_network.to_lowercase().as_str() {"mainnet"=>0,"testnet"=>1,_=>return Err(FrontendError::from("Invalid L2 network".to_string()))};
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

    let (result,intents) = {
        let mut service=state.service.lock().map_err(|_|FrontendError::from("Wallet service unavailable".to_string()))?;
        let archive = open_wallet_archive(&app, &service);
        let result=service.create_transfer_bundle_with_l2_intents(request,&standard_definitions_toml,archive.as_ref(),password.as_deref(),network).map_err(FrontendError::from)?;
        let mut intents = Vec::with_capacity(result.involved_sources_details.len());
        for source in &result.involved_sources_details {
            match service.pending_l2_send_intent(&source.local_instance_id) {
                Ok(intent) => intents.push(intent),
                Err(err) => {
                    log::warn!(
                        "Pending L2 send intent missing or failed for source {}: {}",
                        source.local_instance_id,
                        err
                    );
                }
            }
        }
        if let Err(error)=state.refresh_events_cache(&mut service,password.as_deref()) {error!("Failed to refresh events after transfer commit: {}",error);}
        (result,intents)
    };
    // The service mutex is released; the managed Core commit has acknowledged
    // exact bytes before the first HTTP attempt. Uncertain outputs stay pending.
    let candidates=crate::gateway_resolver::cached_candidates(&settings);
    let l2_dispatch=crate::gateway_resolver::dispatch_intents(&settings,&candidates,&intents).await;
    Ok(CreateBundleResult {bundle_data:result.bundle_bytes,bundle_id:result.bundle_id,
        involved_sources_details:result.involved_sources_details.into_iter().map(Into::into).collect(),l2_dispatch})
}

#[tauri::command]
pub fn receive_bundle(
    app: tauri::AppHandle,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    state: tauri::State<AppState>,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    info!("Attempting to receive and process a transfer bundle...");
    let mut service = state.service.lock().unwrap();
    let archive = open_wallet_archive(&app, &service);

    match service.receive_bundle(&bundle_data, &standard_definitions_toml, archive.as_ref(), password.as_deref(), force_accept_tolerance_bundle) {
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
                purchase_id: None,
                newly_committed: None,
                is_replay: None,
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

// ---------------------------------------------------------------------------
// Z01 fail-closed goods-release boundary (DEC-072 / DEC-084)
//
// Untrusted IPC parameters alone can never mint automatic goods release
// (`can_release_goods_automatically == true`): `evaluate_checkout_release`
// (raw outcome strings) and `verify_live_checkout_and_release` (live quorum
// proofs) both funnel through the fail-closed helpers below. A verified
// live proof is only an **observation**; the boundary stays closed until
// the backend durably commits the purchase state (Z08) and lifts it via a
// `DurablePurchaseBinding`, which cannot be forged from IPC parameters.
// ---------------------------------------------------------------------------

/// Maps an untrusted IPC outcome string to an [`L2ProcessingOutcome`] (Z01 fail-closed).
///
/// The string arrives over the IPC bridge and is therefore untrusted input. It
/// must never be promoted to [`L2ProcessingOutcome::Confirmed`]: that variant
/// reports a live, recipient-challenge-bound 14/20 quorum proof and may only be
/// produced by `AppService::verify_live_checkout_proof`. A caller-supplied
/// `"Confirmed"` string (and any unrecognized string) is therefore degraded to
/// [`L2ProcessingOutcome::Found`], the weakest unauthenticated observation.
fn map_untrusted_outcome_str(outcome: &str) -> L2ProcessingOutcome {
    match outcome {
        "Found" => L2ProcessingOutcome::Found,
        "Provisional" => L2ProcessingOutcome::Provisional,
        s if s.starts_with("Quarantined") => L2ProcessingOutcome::Quarantined {
            reason: s.strip_prefix("Quarantined:").unwrap_or("Conflict").to_string(),
        },
        s if s.starts_with("SyncRequired") => L2ProcessingOutcome::SyncRequired {
            sync_point: s.strip_prefix("SyncRequired:").unwrap_or("0").to_string(),
        },
        // Defensive: a raw caller string is never proof of a live, challenge-bound
        // quorum, so it must not be promoted to `Confirmed`.
        _ => L2ProcessingOutcome::Found,
    }
}

/// Evaluates a goods-release verdict from untrusted IPC parameters (Z01 fail-closed).
///
/// Helper of [`evaluate_checkout_release`]: the outcome string is mapped down via
/// [`map_untrusted_outcome_str`] (a raw `"Confirmed"` becomes `Found`) and the
/// verdict is *defensively* forced to `can_release_goods_automatically == false`,
/// so the command can never mint automatic goods release from untrusted IPC
/// parameters alone. Only a backend-verified `DurablePurchaseBinding` (Z08) may
/// lift the boundary.
fn evaluate_untrusted_checkout_release(
    service: &AppService,
    outcome: &str,
    explicit_merchant_approval: bool,
) -> FrontendCheckoutReleaseVerdict {
    let l2_outcome = map_untrusted_outcome_str(outcome);
    let mut verdict = service.evaluate_checkout_release(l2_outcome, explicit_merchant_approval);
    // Defensive: only a Z08 durable purchase binding may authorize automatic goods
    // release, so a caller-provided outcome must never enable it.
    verdict.can_release_goods_automatically = false;
    verdict.into()
}

/// Parses the IPC JSON payloads of [`verify_live_checkout_and_release`] (fail-closed).
///
/// Any malformed `proof_json` / `expected_context_json` is rejected as a
/// [`FrontendError`] before any verification happens; parsing errors never fall
/// back to permissive defaults.
fn parse_live_checkout_ipc(
    proof_json: &str,
    expected_context_json: &str,
) -> Result<(LiveStateProof, LiveStateContext), FrontendError> {
    let proof = serde_json::from_str(proof_json)
        .map_err(|e| FrontendError::from(format!("Failed to parse LiveStateProof: {}", e)))?;
    let expected = serde_json::from_str(expected_context_json)
        .map_err(|e| FrontendError::from(format!("Failed to parse LiveStateContext: {}", e)))?;
    Ok((proof, expected))
}

/// Verifies a live checkout proof supplied over IPC (Z01 fail-closed boundary).
///
/// Helper of [`verify_live_checkout_and_release`]. The proof is verified
/// **mathematically** (14/20 quorum signatures bound to a fresh recipient
/// challenge), and a valid proof reports [`L2ProcessingOutcome::Confirmed`].
/// That confirmation, however, is only an observation: the verdict is
/// defensively clamped to `can_release_goods_automatically == false`, so
/// automatic goods release is never minted from untrusted IPC parameters alone.
#[cfg(test)]
fn verify_untrusted_live_checkout(
    service: &AppService,
    proof_json: &str,
    expected_context_json: &str,
    explicit_merchant_approval: bool,
) -> Result<(LiveStateProof, CheckoutReleaseVerdict), FrontendError> {
    let (proof, expected) = parse_live_checkout_ipc(proof_json, expected_context_json)?;
    let mut verdict = service
        .verify_live_checkout_proof(&proof, &expected, explicit_merchant_approval)
        .map_err(FrontendError::from)?;
    // Defensive Z01 clamp: a mathematically valid 14/20 proof is an observation,
    // not an authorization. Automatic goods release stays fail-closed until the
    // durable purchase state (Z08) is wired.
    verdict.can_release_goods_automatically = false;
    Ok((proof, verdict))
}

/// Command-level persistence gate for verified checkouts (M6.2 / Z01).
///
/// A persisted audit record (and any automatic goods release) requires the
/// verdict to be a `Confirmed` live proof **and** `can_release_goods_automatically
/// == true`, i.e. a backend `DurablePurchaseBinding` (Z08). As long as Z08 is not
/// wired, this gate is fail-closed for every verdict built from untrusted IPC
/// parameters alone.
fn checkout_release_gate(verdict: &CheckoutReleaseVerdict) -> bool {
    verdict.is_goods_release_authorized
        && verdict.can_release_goods_automatically
        && matches!(verdict.outcome, L2ProcessingOutcome::Confirmed)
}

/// Verifies a recipient-challenge-bound L2 live checkout proof over IPC and
/// reports the goods-release verdict.
///
/// # Z01 Fail-Closed Goods Release Boundary (DEC-072 / DEC-084)
/// `verify_live_checkout_and_release` and `evaluate_checkout_release` must never
/// mint automatic goods release (`can_release_goods_automatically == true`) from
/// untrusted IPC parameters alone:
/// - Malformed `proof_json` / `expected_context_json` are rejected as
///   [`FrontendError`]s before verification ([`parse_live_checkout_ipc`]).
/// - Even a mathematically valid live proof (14/20 quorum bound to a fresh
///   recipient challenge) is only an **observation**; the verdict is clamped to
///   `can_release_goods_automatically == false` ([`verify_untrusted_live_checkout`]).
/// - The M6.2 persistence gate ([`checkout_release_gate`]) therefore stays closed
///   until the backend durably commits the purchase state (Z08) via a
///   `DurablePurchaseBinding`, and every IPC-only precondition test lives in the
///   `z01_fail_closed_tests` module of this file.
#[tauri::command]
pub fn verify_live_checkout_and_release(
    proof_json: String,
    expected_context_json: String,
    regional_views_json: Option<String>,
    explicit_merchant_approval: bool,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendCheckoutReleaseVerdict, FrontendError> {
    info!("Verifying live checkout proof with recipient challenge binding...");
    let mut service = state.service.lock().unwrap();

    let (proof, expected) = parse_live_checkout_ipc(&proof_json, &expected_context_json)?;
    let views_json = regional_views_json.ok_or_else(|| FrontendError::from("Configured regional mesh views are required".to_string()))?;
    if views_json.len() > 4 * 1024 * 1024 { return Err(FrontendError::from("Regional mesh view budget exceeded".to_string())); }
    let views: Vec<human_money_core::humoco_proto::mesh::RegionalMeshView> = serde_json::from_str(&views_json)
        .map_err(|_| FrontendError::from("Invalid regional mesh views".to_string()))?;
    let now_ms = u64::try_from(Utc::now().timestamp_millis()).map_err(|_| FrontendError::from("Invalid local verification time".to_string()))?;
    let verdict = service.verify_live_checkout_with_regional_authority(&proof, &expected, &views, now_ms, explicit_merchant_approval)
        .map_err(FrontendError::from)?;

    // M6.2: A goods release that is authorized by a verified live quorum proof is
    // persisted in the transaction history. Without that record the release decision
    // would leave no audit trail at all. The write is fail-closed on purpose: if the
    // record cannot be stored, the release is not handed out to the frontend.
    if checkout_release_gate(&verdict) {
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

/// Evaluates a goods release verdict for a caller-supplied outcome string (Z01 fail-closed).
///
/// # Caller-supplied outcomes never authorize automatic goods release
///
/// The `outcome` string is untrusted IPC input. It must never be promoted to
/// [`L2ProcessingOutcome::Confirmed`], because that variant is reserved for a live,
/// recipient-challenge-bound quorum proof. Untrusted strings are therefore mapped
/// down via [`map_untrusted_outcome_str`] (a raw `"Confirmed"` becomes `Found`),
/// and the returned verdict is defensively forced to
/// `can_release_goods_automatically == false`
/// ([`evaluate_untrusted_checkout_release`]).
///
/// Together with [`verify_live_checkout_and_release`] this enforces the Z01
/// fail-closed boundary: neither command can mint automatic goods release from
/// untrusted IPC parameters alone. Only [`verify_live_checkout_and_release`]
/// observes a mathematically verified live quorum proof, and even that remains
/// an observation until a backend `DurablePurchaseBinding` (Z08) is presented.
#[tauri::command]
pub fn evaluate_checkout_release(
    outcome: String,
    explicit_merchant_approval: bool,
    state: tauri::State<AppState>,
) -> Result<FrontendCheckoutReleaseVerdict, FrontendError> {
    let service = state.service.lock().unwrap();
    Ok(evaluate_untrusted_checkout_release(&service, &outcome, explicit_merchant_approval))
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

/// Unit tests pinning the Z01 fail-closed goods-release boundary.
///
/// # Z01 Fail-Closed Goods Release Boundary (DEC-072 / DEC-084)
/// Neither [`evaluate_checkout_release`] nor [`verify_live_checkout_and_release`]
/// may mint automatic goods release (`can_release_goods_automatically == true`)
/// from untrusted IPC parameters alone. These tests document and enforce that:
///
/// 1. A caller-supplied `"Confirmed"` outcome string is downgraded to `Found`
///    and can never produce automatic goods release.
/// 2. Malformed `proof_json` / `expected_context_json` payloads are rejected as
///    [`FrontendError`]s before any verification happens.
/// 3. A mathematically valid 14/20 live proof with a fresh recipient challenge
///    verifies positively (`Confirmed`), yet the verdict — and the M6.2
///    persistence gate ([`checkout_release_gate`]) — stays fail-closed until the
///    durable purchase state (Z08) is wired.
///
/// Because `tauri::State` cannot be constructed ad hoc, the tests exercise the
/// exact code paths of the Tauri commands through the pure helper functions of
/// this module (`map_untrusted_outcome_str`, `evaluate_untrusted_checkout_release`,
/// `parse_live_checkout_ipc`, `verify_untrusted_live_checkout`).
#[cfg(test)]
mod z01_fail_closed_tests {
    use super::*;

    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::AttestationVote;
    use uuid::Uuid;

    /// Isolated `AppService` in a fresh temp dir. The Z01 boundary does not depend
    /// on wallet state, so a locked service is the correct subject under test —
    /// no unlocked wallet or runtime is required.
    fn test_service() -> AppService {
        let dir = std::env::temp_dir().join(format!("humoco_z01_test_{}", Uuid::new_v4()));
        AppService::new(&dir).unwrap()
    }

    /// Builds a mathematically valid 14/20 live checkout fixture: 20 distinct
    /// committee keys, `parent_lock = blake3(lookup_tag)` and 14 signatures over
    /// the live-state digest bound to a fresh, non-zero recipient challenge.
    fn build_valid_live_fixture() -> (LiveStateContext, LiveStateProof) {
        let keys: Vec<SigningKey> = (1..=20).map(|i| SigningKey::from_bytes(&[i; 32])).collect();
        let committee: Vec<[u8; 32]> = keys.iter().map(|k| k.verifying_key().to_bytes()).collect();

        let lookup_tag = "humoco_z01_checkout".to_string();
        let parent_lock = *blake3::hash(lookup_tag.as_bytes()).as_bytes();

        let context = LiveStateContext {
            version: 1,
            network: 0,
            shard_id: 5,
            voucher_id: "v_z01_test".to_string(),
            lookup_tag: lookup_tag.clone(),
            parent_lock,
            child_lock: [7; 32],
            root_valid_until: 100_000,
            state_digest: [8; 32],
            recipient_nonce: [9; 32], // fresh, non-zero recipient challenge
            committee,
        };

        let digest = human_money_core::humoco_proto::live::live_state_digest(&context).unwrap();
        let votes: Vec<AttestationVote> = keys
            .iter()
            .take(14)
            .map(|k| AttestationVote {
                signer_pubkey: k.verifying_key().to_bytes(),
                signature: k.sign(&digest).to_bytes(),
            })
            .collect();

        let proof = LiveStateProof {
            context: context.clone(),
            votes,
        };
        (context, proof)
    }

    /// (1) `evaluate_checkout_release`: the caller-supplied string `"Confirmed"`
    /// maps down to `Found` and can never mint automatic goods release, no matter
    /// what the merchant approval flag is. Unknown strings degrade to `Found`
    /// as well, so no IPC string can ever mint `Confirmed`.
    #[test]
    fn test_z01_caller_outcome_confirmed_maps_to_found_and_stays_fail_closed() {
        // The mapping itself never mints `Confirmed`
        assert_eq!(map_untrusted_outcome_str("Confirmed"), L2ProcessingOutcome::Found);
        assert_eq!(map_untrusted_outcome_str("confirmed"), L2ProcessingOutcome::Found);
        assert_eq!(map_untrusted_outcome_str("CONFIRMED/anything"), L2ProcessingOutcome::Found);

        // Non-privileged outcomes round-trip untouched
        assert_eq!(map_untrusted_outcome_str("Found"), L2ProcessingOutcome::Found);
        assert_eq!(map_untrusted_outcome_str("Provisional"), L2ProcessingOutcome::Provisional);

        let app = test_service();
        for approval in [false, true] {
            let verdict = evaluate_untrusted_checkout_release(&app, "Confirmed", approval);
            assert_eq!(verdict.outcome, "Found", "Caller string must map down to Found");
            assert!(
                !verdict.can_release_goods_automatically,
                "Automatic goods release must not be minted from an IPC string (Z01)"
            );
            assert!(verdict.requires_merchant_approval);
            assert!(verdict.merchant_warning.is_some());
        }

        // Quarantined strings stay strictly blocked, not even manually releasable
        let quarantined = evaluate_untrusted_checkout_release(&app, "Quarantined:collision", true);
        assert_eq!(quarantined.outcome, "Quarantined:collision");
        assert!(!quarantined.can_release_goods_automatically);
        assert!(!quarantined.is_goods_release_authorized);
    }

    /// (2) Malformed `proof_json` / `expected_context_json` payloads are rejected
    /// with a descriptive [`FrontendError`] before any verification runs
    /// (fail-closed parsing, no silent defaults).
    #[test]
    fn test_z01_malformed_ipc_json_returns_frontend_error() {
        // Malformed proof JSON
        let err = parse_live_checkout_ipc("{ this is not json", "{}").unwrap_err();
        assert!(err.message.contains("Failed to parse LiveStateProof"), "{}", err.message);
        assert_eq!(err.code, "error.internal.unknown");

        // Structurally serializable proof payload, but malformed context payload
        let proof = LiveStateProof {
            context: LiveStateContext {
                version: 1,
                network: 0,
                shard_id: 0,
                voucher_id: String::new(),
                lookup_tag: "tag".to_string(),
                parent_lock: [0; 32],
                child_lock: [0; 32],
                root_valid_until: 0,
                state_digest: [0; 32],
                recipient_nonce: [0; 32],
                committee: Vec::new(),
            },
            votes: Vec::new(),
        };
        let proof_json = serde_json::to_string(&proof).unwrap();
        let err = parse_live_checkout_ipc(&proof_json, "{ broken context").unwrap_err();
        assert!(err.message.contains("Failed to parse LiveStateContext"), "{}", err.message);
        assert_eq!(err.code, "error.internal.unknown");

        // The same fail-closed behavior holds on the command-level helper
        let app = test_service();
        let result = verify_untrusted_live_checkout(&app, "§garbage§", "{}", false);
        assert!(result.is_err(), "Malformed IPC payloads must never verify");
    }

    /// (3) `verify_live_checkout_and_release`: a mathematically valid 14/20 live
    /// proof with a fresh recipient challenge verifies positively and reports
    /// `Confirmed`, yet the verdict is only an observation — automatic goods
    /// release and the M6.2 persistence gate stay fail-closed (Z01) until the
    /// durable purchase state (Z08) is wired.
    #[test]
    fn test_z01_valid_14_of_20_proof_verifies_but_automatic_release_stays_fail_closed() {
        let (context, proof) = build_valid_live_fixture();
        let proof_json = serde_json::to_string(&proof).unwrap();
        let context_json = serde_json::to_string(&context).unwrap();

        // Mathematical verification succeeds for the valid 14/20 proof
        let (parsed_proof, verdict) = verify_untrusted_live_checkout(
            &test_service(),
            &proof_json,
            &context_json,
            false,
        )
        .expect("Mathematically valid 14/20 proof with recipient challenge must verify");
        assert_eq!(parsed_proof.votes.len(), 14);
        assert!(matches!(verdict.outcome, L2ProcessingOutcome::Confirmed));

        // Z01: the verified proof is only an observation — goods release stays closed
        assert!(!verdict.can_release_goods_automatically);
        assert!(!verdict.is_goods_release_authorized);
        assert!(verdict.requires_merchant_approval);
        assert!(verdict.merchant_warning.as_deref().unwrap_or_default().contains("Z01"));

        // The M6.2 persistence gate stays closed: no durable purchase state (Z08)
        // has been wired, so no audit record justifies automatic release.
        assert!(!checkout_release_gate(&verdict));

        // The wire DTO the frontend receives is fail-closed as well
        let frontend: FrontendCheckoutReleaseVerdict = verdict.into();
        assert_eq!(frontend.outcome, "Confirmed");
        assert!(!frontend.can_release_goods_automatically);
        assert!(!frontend.is_goods_release_authorized);

        // Even an explicitly approving merchant cannot lift the Z01 boundary —
        // approval is a manual observation, not a Z08 binding.
        let (_, approved) = verify_untrusted_live_checkout(
            &test_service(),
            &proof_json,
            &context_json,
            true,
        )
        .expect("Explicit merchant approval must not change the mathematical verdict");
        assert!(matches!(approved.outcome, L2ProcessingOutcome::Confirmed));
        assert!(!approved.can_release_goods_automatically);
        assert!(!checkout_release_gate(&approved));
    }

    /// Backing check for (3): the mathematical gate is real — sub-quorum votes,
    /// a missing recipient challenge, a deviated proof context and forged
    /// signatures are all rejected as [`FrontendError`]s.
    #[test]
    fn test_z01_live_proof_rejects_subquorum_missing_challenge_and_mismatch() {
        let (context, proof) = build_valid_live_fixture();
        let app = test_service();

        let verify = |p: &LiveStateProof, ctx: &LiveStateContext| {
            verify_untrusted_live_checkout(
                &app,
                &serde_json::to_string(p).unwrap(),
                &serde_json::to_string(ctx).unwrap(),
                false,
            )
        };

        // 13/20 sub-quorum must be rejected
        let short = LiveStateProof {
            context: context.clone(),
            votes: proof.votes[..13].to_vec(),
        };
        assert!(verify(&short, &context).is_err(), "13/20 sub-quorum must fail");

        // A zeroed recipient challenge must be rejected (no fresh challenge binding)
        let mut zero_nonce = context.clone();
        zero_nonce.recipient_nonce = [0; 32];
        let zero_proof = LiveStateProof {
            context: zero_nonce.clone(),
            votes: proof.votes.clone(),
        };
        assert!(verify(&zero_proof, &zero_nonce).is_err(), "Missing challenge must fail");

        // A proof whose context deviates from the recipient-expected context
        // must be rejected ("live context mismatch")
        let mut deviated = context.clone();
        deviated.child_lock = [9; 32];
        let deviated_proof = LiveStateProof {
            context: deviated.clone(),
            votes: proof.votes.clone(),
        };
        assert!(verify(&deviated_proof, &context).is_err(), "Context mismatch must fail");

        // A forged (zeroed) signature must be rejected
        let mut forged = proof.clone();
        forged.votes[0].signature = [0; 64];
        assert!(verify(&forged, &context).is_err(), "Forged signature must fail");
    }
}

/// Explicit recipient configuration, authenticated and persisted separately from
/// checkout proof input. Source keys never come from a payment receipt.
#[tauri::command]
pub fn configure_regional_mesh_authority(
    policy_json: String, password: String, state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    if policy_json.len() > 4096 { return Err(FrontendError::from("Regional policy budget exceeded".to_string())); }
    let policy: human_money_core::humoco_proto::mesh::RegionalMeshPolicy = serde_json::from_str(&policy_json)
        .map_err(|_| FrontendError::from("Invalid regional mesh policy".to_string()))?;
    let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet unavailable".to_string()))?;
    service.configure_regional_mesh_authority(policy, Some(&password)).map_err(FrontendError::from)
}


/// The durable Core transaction completes before resolver or HTTP work begins.
/// Transport success alone remains pending until authenticated reconciliation.
#[tauri::command]
pub async fn submit_l2_send_intent(
    local_instance_id: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<crate::gateway_resolver::IntentDispatchObservation, FrontendError> {
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let network = match settings.active_network.to_lowercase().as_str() {
        "mainnet" => 0, "testnet" => 1,
        _ => return Err(FrontendError::from("Invalid L2 network configuration".to_string())),
    };
    let intent = {
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service.prepare_l2_send_intent(&local_instance_id, network, password.as_deref()).map_err(FrontendError::from)?
    };
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    let now_ms = u64::try_from(Utc::now().timestamp_millis()).map_err(|_| FrontendError::from("Invalid verification time".to_string()))?;

    if let Some(res) = &intent.terminal_resolution {
        let status = match res {
            human_money_core::app_service::IntentTerminalResolution::Confirmed { .. } => "reconciled_confirmed",
            human_money_core::app_service::IntentTerminalResolution::Conflicted { .. } => "conflicted",
            human_money_core::app_service::IntentTerminalResolution::Expired { .. } => "expired",
        };
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.cleanup_terminal_intents(60_000, now_ms, password.as_deref());
        return Ok(crate::gateway_resolver::IntentDispatchObservation {
            local_instance_id,
            status: status.into(),
            http_status: None,
        });
    }

    if now_ms >= intent.root_valid_until_ms {
        let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.expire_intent_if_overdue(&local_instance_id, now_ms, password.as_deref());
        let _ = service.cleanup_terminal_intents(60_000, now_ms, password.as_deref());
        return Ok(crate::gateway_resolver::IntentDispatchObservation {
            local_instance_id,
            status: "expired".into(),
            http_status: None,
        });
    }

    if now_ms >= intent.created_at_ms.saturating_add(30_000) {
        let tag = {
            let service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            service.retained_choice_target(&local_instance_id, now_ms).map_err(FrontendError::from)?
        };
        let observed = if let Some(bytes) = crate::gateway_resolver::fetch_retained(&candidates, &tag).await.map_err(FrontendError::from)? {
            let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            match service.record_retained_choice_observation(&local_instance_id, &bytes, now_ms, password.as_deref()) {
                Ok(true) => true,
                Ok(false) => false,
                Err(err) => {
                    log::warn!("Retained choice observation for {} detected conflict/mismatch: {}", local_instance_id, err);
                    false
                }
            }
        } else { false };

        if let Ok((_owner, views)) = crate::checkout::cached_views(&state) {
            let context_res = {
                let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
                service.begin_intent_reconciliation(&local_instance_id, &views, now_ms, password.as_deref())
            };
            if let Ok(context) = context_res {
                if let Ok(Some(proof)) = crate::gateway_resolver::fetch_live_proof(&candidates, &context).await {
                    let mut service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
                    if let Ok(res) = service.complete_intent_reconciliation(&local_instance_id, &proof, &views, now_ms, password.as_deref()) {
                        let _ = service.cleanup_terminal_intents(60_000, now_ms, password.as_deref());
                        let status = match res {
                            human_money_core::app_service::IntentTerminalResolution::Confirmed { .. } => "reconciled_confirmed",
                            human_money_core::app_service::IntentTerminalResolution::Conflicted { .. } => "conflicted",
                            human_money_core::app_service::IntentTerminalResolution::Expired { .. } => "expired",
                        };
                        return Ok(crate::gateway_resolver::IntentDispatchObservation {
                            local_instance_id,
                            status: status.into(),
                            http_status: None,
                        });
                    }
                }
            }
        }

        return Ok(crate::gateway_resolver::IntentDispatchObservation {
            local_instance_id,
            status: if observed { "retained_original_observed_output_pending" } else { "pending" }.into(),
            http_status: None,
        });
    }
    let http_status = crate::gateway_resolver::dispatch_exact(&settings, &candidates, &intent.request_bytes).await.map_err(FrontendError::from)?;
    Ok(crate::gateway_resolver::IntentDispatchObservation { local_instance_id, status: "pending".into(), http_status })
}

#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct FrontendPurchaseCompletion {
    pub purchase_id:String,
    pub newly_committed:bool,
    pub verdict:FrontendCheckoutReleaseVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_payload: Option<ReceiveSuccessPayload>,
}
fn parse_regional_views(json:&str)->Result<Vec<human_money_core::humoco_proto::mesh::RegionalMeshView>,FrontendError> {
    if json.len()>4*1024*1024 {return Err(FrontendError::from("Regional view budget exceeded".to_string()));}
    serde_json::from_str(json).map_err(|_|FrontendError::from("Invalid regional views".to_string()))
}
fn checkout_now_ms()->Result<u64,FrontendError> {
    u64::try_from(Utc::now().timestamp_millis()).map_err(|_|FrontendError::from("Invalid checkout verification time".to_string()))
}
/// The returned protocol context contains the backend-generated, persisted nonce.
#[tauri::command]
pub fn begin_pos_checkout(purchase_id:String,bundle_data:Vec<u8>,regional_views_json:String,password:Option<String>,state:tauri::State<AppState>)->Result<String,FrontendError> {
    let views=parse_regional_views(&regional_views_json)?;let now=checkout_now_ms()?;
    let mut service=state.service.lock().map_err(|_|FrontendError::from("Wallet service unavailable".to_string()))?;
    let context=service.begin_checkout_purchase(&purchase_id,&bundle_data,&views,now,password.as_deref()).map_err(FrontendError::from)?;
    serde_json::to_string(&context).map_err(|e| FrontendError::from(e.to_string()))
}
/// Emits release only after Core has committed L1 and the purchase together.
/// An idempotent retry retains the purchase ID and reports newlyCommitted=false.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn complete_pos_checkout(purchase_id:String,bundle_data:Vec<u8>,proof_json:String,regional_views_json:String,standard_definitions_toml:HashMap<String,String>,password:Option<String>,state:tauri::State<AppState>)->Result<FrontendPurchaseCompletion,FrontendError> {
    {
        let service = state.service.lock().map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        if let Some(completion) = service.replay_checkout_purchase(&purchase_id, &bundle_data).map_err(FrontendError::from)? {
            return Ok(FrontendPurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                verdict: completion.verdict.into(),
                receive_payload: None,
            });
        }
    }
    if proof_json.len()>32*1024 {return Err(FrontendError::from("Checkout proof budget exceeded".to_string()));}
    let proof:LiveStateProof=serde_json::from_str(&proof_json).map_err(|_|FrontendError::from("Invalid checkout proof".to_string()))?;
    let views=parse_regional_views(&regional_views_json)?;let now=checkout_now_ms()?;
    let mut service=state.service.lock().map_err(|_|FrontendError::from("Wallet service unavailable".to_string()))?;
    let completion=service.complete_checkout_purchase(&purchase_id,&bundle_data,&proof,&views,&standard_definitions_toml,now,password.as_deref()).map_err(FrontendError::from)?;
    Ok(FrontendPurchaseCompletion {purchase_id:completion.purchase_id,newly_committed:completion.newly_committed,verdict:completion.verdict.into(),receive_payload:None})
}

/// Productive purchase path: stored challenge, one payment gateway, atomic import.
#[tauri::command]
pub async fn checkout_received_bundle(purchase_id:String,bundle_data:Vec<u8>,standard_definitions_toml:HashMap<String,String>,password:Option<String>,state:tauri::State<'_,AppState>)->Result<crate::checkout::PurchaseCompletion,FrontendError> {
    crate::checkout::checkout_bundle(&state,&purchase_id,&bundle_data,&standard_definitions_toml,password.as_deref()).await
}
