// src-tauri/src/commands/transfers.rs
use crate::{
    models::{
        CreateBundleResult, FrontendCheckoutReleaseVerdict, FrontendError, FrontendRankedVoucher,
        FrontendSourceTransfer, FrontendTransactionRecord, FrontendTransferPlan,
        ReceiveSuccessPayload, TransactionRecord,
    },
    AppState,
};
use chrono::Utc;
use human_money_core::{
    archive::FileVoucherArchive,
    humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS,
    models::layer2_api::{CheckoutReleaseVerdict, L2ProcessingOutcome},
    wallet::{MultiTransferRequest, SourceTransfer},
};
use log::{error, info};
use rand::RngCore;
use std::collections::HashMap;
use std::time::Duration;
use tauri::Manager;
use uuid::Uuid;

fn random_non_zero_nonce() -> [u8; 32] {
    loop {
        let mut nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        if nonce != [0u8; 32] {
            return nonce;
        }
    }
}

fn open_wallet_archive(
    app: Option<&tauri::AppHandle>,
    service: &human_money_core::app_service::AppService,
) -> Option<FileVoucherArchive> {
    let app = app?;
    let app_data_dir = app.path().app_data_dir().ok()?;
    let (user_id, key) = service
        .with_wallet_and_identity(|w, id| {
            let uid = w.get_user_id().to_string();
            let derived =
                blake3::derive_key("HUMOCO_APP_ARCHIVE_KEY_V1", &id.signing_key.to_bytes());
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

fn determine_pow_base_bits_for_sources(
    service: &human_money_core::app_service::AppService,
    settings: &crate::settings::AppSettings,
    sources: &[SourceTransfer],
) -> (bool, u32) {
    let mut all_anchored = !sources.is_empty();
    let mut pow_bits = DEFAULT_LOCK_POW_BASE_BITS;
    let _ = service.with_wallet(|wallet| {
        for src in sources {
            match wallet
                .get_voucher_instance(&src.local_instance_id)
                .and_then(|inst| inst.voucher.layer2_anchor)
            {
                Some(anchor_id) => {
                    let anchor_hex = hex::encode(anchor_id);
                    if let Some(manifest) = wallet.profile.l2_anchor_manifests.get(&anchor_hex) {
                        for srv in &manifest.servers {
                            let norm = crate::checkout::normalize_sync_write_url(&srv.write_url);
                            if let Some(probe) = settings.l2_probe_cache.get(&norm) {
                                pow_bits = pow_bits.max(probe.pow_base_bits);
                            }
                        }
                    }
                }
                None => {
                    all_anchored = false;
                }
            }
        }
    });
    (all_anchored, pow_bits)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_transfer_bundle_impl(
    app: Option<&tauri::AppHandle>,
    recipient_id: String,
    sources: Vec<FrontendSourceTransfer>,
    notes: Option<String>,
    sender_profile_name: Option<String>,
    standard_definitions_toml: HashMap<String, String>,
    use_privacy_mode: Option<bool>,
    send_card_override: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<CreateBundleResult, FrontendError> {
    let recipient_id = recipient_id.trim().to_string();
    log::debug!("Creating transfer bundle (sources={})", sources.len());

    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    if settings.pending_restore_l2_reconciliation {
        return Err(FrontendError::from(
            "Nach einer Wiederherstellung aus der Sicherung ist vor dem Ausgeben von Gutscheinen ein einmaliger Online-Sicherheitsabgleich erforderlich.".to_string(),
        ));
    }

    let source_transfers: Vec<SourceTransfer> = sources
        .into_iter()
        .map(|s| SourceTransfer {
            local_instance_id: s.local_instance_id,
            amount_to_send: s.amount_to_send,
        })
        .collect();

    let (result, intents, is_durable, pow_base_bits) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;

        let mut wot_sync_json: Option<String> = None;
        let mut pending_receipt: Option<humoco_wot::SyncReceipt> = None;

        let mut contact_card_json: Option<String> = None;
        let mut pending_card_sent: Option<([u8; 32], u64)> = None;

        let loaded_settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let should_send_card =
            send_card_override.unwrap_or(loaded_settings.market_send_card_on_payment);
        if should_send_card {
            let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
            let own_profile_opt: Option<crate::models::StoredOwnMarketProfile> = service
                .load_encrypted_data(crate::state::MARKET_OWN_PROFILE_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());

            if let Some(own_profile) = own_profile_opt {
                if own_profile.published {
                    if let Some(card) = own_profile.last_card {
                        if now_secs < card.payload.valid_until {
                            if let Ok(peer_pk) =
                                human_money_core::crypto::get_pubkey_from_user_id(&recipient_id)
                            {
                                let peer_pk_bytes = *peer_pk.as_bytes();
                                let store: humoco_market::CardStore = service
                                    .load_encrypted_data(
                                        crate::state::MARKET_CARD_STORE_KEY,
                                        password.as_deref(),
                                    )
                                    .ok()
                                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                                    .unwrap_or_default();

                                if humoco_market::should_attach_card(
                                    &store,
                                    &peer_pk_bytes,
                                    card.payload.v,
                                ) {
                                    let wire_opt = match humoco_market::render_wire(
                                        &card,
                                        1,
                                        humoco_market::WireFilter::NoAds,
                                    ) {
                                        Some(wire)
                                            if wire.len()
                                                <= humoco_market::MAX_BUNDLE_CARD_WIRE_BYTES =>
                                        {
                                            Some(wire)
                                        }
                                        _ => humoco_market::render_wire(
                                            &card,
                                            1,
                                            humoco_market::WireFilter::FitBytes(
                                                humoco_market::MAX_BUNDLE_CARD_WIRE_BYTES,
                                            ),
                                        ),
                                    };
                                    if let Some(wire) = wire_opt {
                                        let env = humoco_market::own_envelope(wire);
                                        if let Ok(json) = serde_json::to_string(&env) {
                                            contact_card_json = Some(json);
                                            pending_card_sent =
                                                Some((peer_pk_bytes, card.payload.v));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Ok(can_recipient) = crate::state::canonicalize_did(&recipient_id) {
            if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
                if reg
                    .own_contacts
                    .get(&can_recipient)
                    .is_some_and(|e| e.level >= 1)
                {
                    if let Ok(changed) = state.ensure_wot_list_signed(&mut service, &mut reg) {
                        if changed {
                            let _ =
                                state.save_wot_register(&mut service, &reg, password.as_deref());
                        }
                    }
                    if let Ok((pkg, receipt)) =
                        reg.prepare_sync_package(Some(&can_recipient), Some(48_000), false)
                    {
                        if !pkg.lists.is_empty() {
                            if let Ok(json) = serde_json::to_string(&pkg) {
                                wot_sync_json = Some(json);
                                pending_receipt = Some(receipt);
                            }
                        }
                    }
                }
            }
        }

        let (all_anchored, pow_bits) =
            determine_pow_base_bits_for_sources(&service, &loaded_settings, &source_transfers);
        let source_count = source_transfers.len();
        let request = MultiTransferRequest {
            recipient_id,
            sources: source_transfers,
            notes,
            sender_profile_name,
            use_privacy_mode,
            wot_sync_json,
            contact_card_json,
        };

        let archive = open_wallet_archive(app, &service);
        let (result, intents) = if all_anchored {
            let result = service
                .create_transfer_bundle_with_l2_intents(
                    request,
                    &standard_definitions_toml,
                    archive.as_ref(),
                    password.as_deref(),
                    pow_bits,
                )
                .map_err(|e| {
                    error!(
                        "create_transfer_bundle failed (durable_sync_lock, sources={}): {}",
                        source_count, e
                    );
                    FrontendError::from(e)
                })?;

            let mut intents = Vec::with_capacity(result.involved_sources_details.len());
            for source in &result.involved_sources_details {
                if let Ok(intent) = service.pending_l2_send_intent(&source.local_instance_id) {
                    intents.push(intent);
                }
            }
            (result, intents)
        } else {
            service
                .create_transfer_bundle_with_best_effort_l2(
                    request,
                    &standard_definitions_toml,
                    archive.as_ref(),
                    password.as_deref(),
                    pow_bits,
                )
                .map_err(|e| {
                    error!(
                        "create_transfer_bundle failed (offline_or_mixed, sources={}): {}",
                        source_count, e
                    );
                    FrontendError::from(e)
                })?
        };

        if let Some((peer_pk_bytes, own_v)) = pending_card_sent {
            let mut store: humoco_market::CardStore = service
                .load_encrypted_data(crate::state::MARKET_CARD_STORE_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            humoco_market::mark_sent(&mut store, peer_pk_bytes, own_v);
            if let Ok(bytes) = serde_json::to_vec(&store) {
                if let Err(e) = service.save_encrypted_data(
                    crate::state::MARKET_CARD_STORE_KEY,
                    &bytes,
                    password.as_deref(),
                ) {
                    log::warn!("Failed to save CardStore after sending card: {}", e);
                }
            }
        }

        if let Some(receipt) = pending_receipt {
            if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
                let now_secs = chrono::Utc::now().timestamp() as u64;
                reg.commit_sync_receipt(receipt, now_secs);
                if let Err(e) = state.save_wot_register(&mut service, &reg, password.as_deref()) {
                    log::warn!(
                        "Failed to save WoT register after committing sync receipt: {}",
                        e
                    );
                }
            }
        }

        if let Err(error) = state.refresh_events_cache(&mut service, password.as_deref()) {
            error!("Failed to refresh events after transfer commit: {}", error);
        }
        (result, intents, all_anchored, pow_bits)
    };

    // Service mutex released; L1 + durable L2 intents are committed before network fan-out.
    let client = crate::sync_client::create_sync_http_client();
    let now_ms = u64::try_from(Utc::now().timestamp_millis())
        .map_err(|_| FrontendError::from("Invalid verification time".to_string()))?;
    let mut l2_dispatch = Vec::with_capacity(intents.len());

    for intent in &intents {
        let nonce = random_non_zero_nonce();
        let reqs_res = if is_durable {
            let service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            service
                .build_intent_committee_requests(
                    &intent.local_instance_id,
                    &nonce,
                    &HashMap::new(),
                    true,
                )
                .map_err(FrontendError::from)
        } else {
            human_money_core::services::l2_gateway::build_committee_batch_requests(
                &intent.anchor_id,
                &intent.manifest,
                &intent.genesis_tickets,
                &intent.entries,
                &nonce,
                &HashMap::new(),
            )
            .map_err(FrontendError::from)
        };

        let Ok(requests) = reqs_res else {
            l2_dispatch.push(crate::gateway_resolver::IntentDispatchObservation {
                local_instance_id: intent.local_instance_id.clone(),
                status: "pending".into(),
                http_status: None,
            });
            continue;
        };

        let dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
            &client,
            &requests,
            Duration::from_secs(4),
        )
        .await;

        let target_slot = intent.entries.last().map(|e| e.slot);
        let target_receipts: Vec<_> = dispatch
            .receipts
            .iter()
            .filter(|r| target_slot == Some(r.slot))
            .cloned()
            .collect();

        if is_durable && (!target_receipts.is_empty() || !dispatch.competing_entries.is_empty()) {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            if let Err(err) = service.record_intent_server_receipts(
                &intent.local_instance_id,
                &nonce,
                &target_receipts,
                dispatch.competing_entries.first(),
                pow_base_bits,
                now_ms,
                password.as_deref(),
            ) {
                log::warn!("Recording L2 send intent receipts failed: {}", err);
            }
        }

        l2_dispatch.push(crate::gateway_resolver::summarize_intent_dispatch(
            &intent.local_instance_id,
            &dispatch,
        ));
    }

    Ok(CreateBundleResult {
        bundle_data: result.bundle_bytes,
        bundle_id: result.bundle_id,
        involved_sources_details: result
            .involved_sources_details
            .into_iter()
            .map(Into::into)
            .collect(),
        l2_dispatch,
    })
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
    send_card_override: Option<bool>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<CreateBundleResult, FrontendError> {
    create_transfer_bundle_impl(
        Some(&app),
        recipient_id,
        sources,
        notes,
        sender_profile_name,
        standard_definitions_toml,
        use_privacy_mode,
        send_card_override,
        password,
        &state,
    )
    .await
}

pub(crate) fn receive_bundle_impl(
    app: Option<&tauri::AppHandle>,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    state: &AppState,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    info!("Attempting to receive and process a transfer bundle...");
    let mut service = state.service.lock().unwrap();
    let archive = open_wallet_archive(app, &service);

    match service.receive_bundle(
        &bundle_data,
        &standard_definitions_toml,
        archive.as_ref(),
        password.as_deref(),
        force_accept_tolerance_bundle,
    ) {
        Ok(result) => {
            info!("Bundle processed successfully. Creating transaction record.");

            let conflict_count = result.check_result.verifiable_conflicts.len();
            let warning_count = result.check_result.unverifiable_warnings.len();

            if conflict_count > 0 || warning_count > 0 {
                log::warn!(
                    "Fraud detection during bundle import: confirmed_conflicts={}, gossip_warnings={}",
                    conflict_count,
                    warning_count
                );
            }

            let transfer_summary = result.transfer_summary.clone();
            let involved_vouchers_details = result.involved_vouchers_details.clone();
            let involved_vouchers_ids: Vec<String> = involved_vouchers_details
                .iter()
                .map(|d| d.local_instance_id.clone())
                .collect();
            let notes = result.header.notes.clone();
            let sender_profile_name = result.header.sender_profile_name.clone();
            let bundle_id = result.header.bundle_id.clone();
            let recipient_id = service
                .with_wallet(|w| w.get_user_id().to_string())
                .map_err(FrontendError::from)?;

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

            state
                .append_to_history(&mut service, record.clone(), password.as_deref())
                .map_err(FrontendError::from)?;

            if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
                error!("Failed to refresh events cache after receive_bundle: {}", e);
            }

            let wot_imported_lists = if let Some(ref wot_json) = result.wot_sync_json {
                state.try_import_piggybacked_wot(
                    &mut service,
                    wot_json,
                    &result.header.sender_id,
                    password.as_deref(),
                )
            } else {
                None
            };

            if let Some(ref card_json) = result.contact_card_json {
                state.try_import_piggybacked_contact_card(
                    &mut service,
                    Some(card_json),
                    password.as_deref(),
                );
            }

            Ok(ReceiveSuccessPayload {
                sender_id: result.header.sender_id,
                sender_profile_name,
                notes,
                transfer_summary: result.transfer_summary.into(),
                involved_vouchers: involved_vouchers_ids,
                involved_vouchers_details: involved_vouchers_details
                    .into_iter()
                    .map(|d| d.into())
                    .collect(),
                verifiable_conflicts: result.check_result.verifiable_conflicts,
                conflict_summaries: service
                    .list_conflicts()
                    .map_err(FrontendError::from)?
                    .into_iter()
                    .map(|s| s.into())
                    .collect(),
                checkout_verdict: None,
                purchase_id: None,
                newly_committed: None,
                is_replay: None,
                wot_imported_lists,
            })
        }
        Err(e) => {
            error!("Failed to process bundle: {}", e);
            Err(FrontendError::from(e))
        }
    }
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
    receive_bundle_impl(
        Some(&app),
        bundle_data,
        standard_definitions_toml,
        password,
        force_accept_tolerance_bundle,
        &state,
    )
}

#[tauri::command]
pub fn save_transaction_record(
    record: FrontendTransactionRecord,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    let record: TransactionRecord = record.into();
    log::debug!("Saving new transaction record");
    let mut service = state.service.lock().unwrap();
    state
        .append_to_history(&mut service, record, password.as_deref())
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn get_transaction_history(
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendTransactionRecord>, FrontendError> {
    info!("Loading transaction history...");
    let history = state.get_cached_history().map_err(FrontendError::from)?;
    Ok(history.iter().map(|r| r.into()).collect())
}

fn compute_wot_bonuses(
    state: &AppState,
    wallet: &human_money_core::wallet::Wallet,
    recipient_id: &str,
) -> (HashMap<String, i64>, HashMap<String, u8>, bool) {
    use human_money_core::models::conflict::TrustStatus;
    let mut bonuses = HashMap::new();
    let mut trust_percents = HashMap::new();

    // Always exclude local KnownOffender creators regardless of recipient or WoT graph state.
    for (local_id, instance) in &wallet.voucher_store.vouchers {
        if instance.status != human_money_core::wallet::instance::VoucherStatus::Active {
            continue;
        }
        let creator_id = instance.voucher.creator_profile.id.as_deref().unwrap_or("");
        if matches!(
            wallet.check_reputation(creator_id),
            TrustStatus::KnownOffender(_)
        ) {
            bonuses.insert(local_id.clone(), -1);
        }
    }

    let Ok(can_recipient) = crate::state::canonicalize_did(recipient_id) else {
        return (bonuses, trust_percents, false);
    };

    let wot_guard = state.wot.lock().unwrap();
    let reg = match wot_guard.as_ref() {
        Some(r) => r,
        None => return (bonuses, trust_percents, false),
    };

    let recipient_list_known =
        reg.raw_list_store.contains_key(&can_recipient) || can_recipient == reg.self_did;
    let now_secs = chrono::Utc::now().timestamp() as u64;

    for (local_id, instance) in &wallet.voucher_store.vouchers {
        if instance.status != human_money_core::wallet::instance::VoucherStatus::Active {
            continue;
        }
        if bonuses.get(local_id).is_some_and(|&v| v < 0) {
            continue;
        }

        let creator_id = instance.voucher.creator_profile.id.as_deref().unwrap_or("");
        let guarantor_refs: Vec<&str> = instance
            .voucher
            .signatures
            .iter()
            .filter(|s| {
                s.role.eq_ignore_ascii_case("guarantor")
                    && !matches!(
                        wallet.check_reputation(&s.signer_id),
                        TrustStatus::KnownOffender(_)
                    )
            })
            .map(|s| s.signer_id.as_str())
            .collect();

        if let Ok(eval) = reg.evaluate_voucher_for_observer(
            recipient_id,
            creator_id,
            &guarantor_refs,
            false,
            Some(now_secs),
        ) {
            if eval.blocked_by_offender {
                bonuses.insert(local_id.clone(), -1);
            } else if recipient_list_known || eval.score >= 1.0 {
                let bonus = (eval.score * 6000.0).round() as i64;
                let pct = (eval.score * 100.0).round().clamp(0.0, 100.0) as u8;
                bonuses.insert(local_id.clone(), bonus);
                trust_percents.insert(local_id.clone(), pct);
            }
        }
    }

    (bonuses, trust_percents, recipient_list_known)
}

pub(crate) fn prepare_transfer_plan_impl(
    target_amount: String,
    standard_uuid_filter: Option<String>,
    recipient_id: String,
    is_test_voucher: Option<bool>,
    state: &AppState,
) -> Result<FrontendTransferPlan, FrontendError> {
    let recipient_id = recipient_id.trim().to_string();
    let service = state.service.lock().unwrap();
    let (bonuses, trust_percents, recipient_list_known) = service
        .with_wallet(|w| compute_wot_bonuses(state, w, &recipient_id))
        .map_err(FrontendError::from)?;
    let plan = service
        .prepare_transfer_with_context(
            &target_amount,
            standard_uuid_filter.as_deref(),
            is_test_voucher,
            &recipient_id,
            Some(&bonuses),
        )
        .map_err(FrontendError::from)?;
    let min_recipient_trust_percent = if recipient_list_known && !plan.selected.is_empty() {
        plan.selected
            .iter()
            .filter_map(|rv| trust_percents.get(&rv.local_instance_id).copied())
            .min()
    } else {
        None
    };
    let mut frontend_plan: FrontendTransferPlan = plan.into();
    frontend_plan.min_recipient_trust_percent = min_recipient_trust_percent;
    Ok(frontend_plan)
}

#[tauri::command]
pub fn prepare_transfer_plan(
    target_amount: String,
    standard_uuid_filter: Option<String>,
    recipient_id: String,
    is_test_voucher: Option<bool>,
    state: tauri::State<AppState>,
) -> Result<FrontendTransferPlan, FrontendError> {
    prepare_transfer_plan_impl(
        target_amount,
        standard_uuid_filter,
        recipient_id,
        is_test_voucher,
        &state,
    )
}

#[tauri::command]
pub fn rank_vouchers_for_recipient(
    target_amount: String,
    standard_uuid_filter: Option<String>,
    recipient_id: String,
    is_test_voucher: Option<bool>,
    state: tauri::State<AppState>,
) -> Result<Vec<FrontendRankedVoucher>, FrontendError> {
    let recipient_id = recipient_id.trim().to_string();
    let service = state.service.lock().unwrap();
    let (bonuses, _, _) = service
        .with_wallet(|w| compute_wot_bonuses(&state, w, &recipient_id))
        .map_err(FrontendError::from)?;
    let ranked = service
        .rank_vouchers_with_context(
            &target_amount,
            standard_uuid_filter.as_deref(),
            is_test_voucher,
            &recipient_id,
            Some(&bonuses),
        )
        .map_err(FrontendError::from)?;
    Ok(ranked.into_iter().map(|r| r.into()).collect())
}

/// Evaluates an untrusted IPC checkout outcome string (Z01 fail-closed boundary):
/// caller-supplied strings can NEVER mint `Confirmed` or authorize automatic release.
#[tauri::command]
pub fn evaluate_checkout_release(
    outcome: String,
    explicit_merchant_approval: bool,
    _state: tauri::State<AppState>,
) -> Result<FrontendCheckoutReleaseVerdict, FrontendError> {
    let l2_outcome = match outcome.as_str() {
        "OfflineOnly" => L2ProcessingOutcome::OfflineOnly,
        s if s.starts_with("Quarantined:") => L2ProcessingOutcome::Quarantined {
            reason: s
                .strip_prefix("Quarantined:")
                .unwrap_or("Conflict")
                .to_string(),
        },
        s if s.starts_with("Yellow:") => L2ProcessingOutcome::Yellow {
            reason: s
                .strip_prefix("Yellow:")
                .unwrap_or("Unverified")
                .to_string(),
        },
        _ => L2ProcessingOutcome::Yellow {
            reason: "Untrusted IPC outcome observation".to_string(),
        },
    };
    let mut verdict = CheckoutReleaseVerdict::new(l2_outcome, explicit_merchant_approval);
    verdict.can_release_goods_automatically = false;
    Ok(verdict.into())
}

#[tauri::command]
pub async fn submit_l2_send_intent(
    local_instance_id: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<crate::gateway_resolver::IntentDispatchObservation, FrontendError> {
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let now_ms = u64::try_from(Utc::now().timestamp_millis())
        .map_err(|_| FrontendError::from("Invalid verification time".to_string()))?;

    let (intent, pow_base_bits) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let pow_bits = service
            .with_wallet(|w| {
                let mut bits = DEFAULT_LOCK_POW_BASE_BITS;
                if let Some(inst) = w.get_voucher_instance(&local_instance_id) {
                    if let Some(anchor) = inst.voucher.layer2_anchor {
                        let ahex = hex::encode(anchor);
                        if let Some(m) = w.profile.l2_anchor_manifests.get(&ahex) {
                            for s in &m.servers {
                                let norm = crate::checkout::normalize_sync_write_url(&s.write_url);
                                if let Some(p) = settings.l2_probe_cache.get(&norm) {
                                    bits = bits.max(p.pow_base_bits);
                                }
                            }
                        }
                    }
                }
                bits
            })
            .unwrap_or(DEFAULT_LOCK_POW_BASE_BITS);
        let intent = service
            .prepare_l2_send_intent(&local_instance_id, pow_bits, password.as_deref())
            .map_err(FrontendError::from)?;
        (intent, pow_bits)
    };

    if let Some(res) = &intent.terminal_resolution {
        let status = match res {
            human_money_core::app_service::IntentTerminalResolution::Confirmed { .. } => {
                "reconciled_confirmed"
            }
            human_money_core::app_service::IntentTerminalResolution::Conflicted { .. } => {
                "conflicted"
            }
            human_money_core::app_service::IntentTerminalResolution::Expired { .. } => "expired",
        };
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.cleanup_terminal_intents(60_000, now_ms, password.as_deref());
        return Ok(crate::gateway_resolver::IntentDispatchObservation {
            local_instance_id,
            status: status.into(),
            http_status: None,
        });
    }

    if now_ms >= intent.root_valid_until_ms {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.expire_intent_if_overdue(&local_instance_id, now_ms, password.as_deref());
        let _ = service.cleanup_terminal_intents(60_000, now_ms, password.as_deref());
        return Ok(crate::gateway_resolver::IntentDispatchObservation {
            local_instance_id,
            status: "expired".into(),
            http_status: None,
        });
    }

    let nonce = random_non_zero_nonce();
    let requests = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .build_intent_committee_requests(&local_instance_id, &nonce, &HashMap::new(), true)
            .map_err(FrontendError::from)?
    };

    if requests.is_empty() {
        return Ok(crate::gateway_resolver::IntentDispatchObservation {
            local_instance_id,
            status: "confirmed".into(),
            http_status: Some(200),
        });
    }

    let client = crate::sync_client::create_sync_http_client();
    let dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
        &client,
        &requests,
        Duration::from_secs(4),
    )
    .await;

    let target_slot = intent.entries.last().map(|e| e.slot);
    let target_receipts: Vec<_> = dispatch
        .receipts
        .iter()
        .filter(|r| target_slot == Some(r.slot))
        .cloned()
        .collect();

    if !target_receipts.is_empty() || !dispatch.competing_entries.is_empty() {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.record_intent_server_receipts(
            &local_instance_id,
            &nonce,
            &target_receipts,
            dispatch.competing_entries.first(),
            pow_base_bits,
            now_ms,
            password.as_deref(),
        );
    }

    Ok(crate::gateway_resolver::summarize_intent_dispatch(
        &local_instance_id,
        &dispatch,
    ))
}

/// Productive purchase path: Sync Lock committee fan-out + atomic wallet/history commit.
#[tauri::command]
pub async fn checkout_received_bundle(
    purchase_id: String,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<crate::checkout::PurchaseCompletion, FrontendError> {
    crate::checkout::checkout_bundle(
        &state,
        &purchase_id,
        &bundle_data,
        &standard_definitions_toml,
        password.as_deref(),
    )
    .await
}

#[cfg(test)]
mod offline_and_degraded_transfer_tests {
    use super::*;
    use crate::commands::sync::tests::{
        create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE,
    };
    use crate::settings::AppSettings;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::{
        decode_hex_fixed, encode_quarter,
        manifest::{AnchorAuthority, AnchorManifest, GenesisTicket, NominatedBy, ServerEntry},
        types::{LockReceipt, LockReceiptKind},
    };
    use human_money_core::models::profile::PublicProfile;
    use human_money_core::models::voucher::ValueDefinition;
    use human_money_core::services::l2_gateway;
    use human_money_core::wallet::instance::VoucherStatus;
    use human_money_core::{test_utils, NewVoucherData};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn std_map() -> (HashMap<String, String>, String) {
        let standard_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let mut map = HashMap::new();
        map.insert(
            "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
            standard_toml.clone(),
        );
        (map, standard_toml)
    }

    fn issue_100_voucher(state: &AppState, standard_toml: &str, password: &str) -> String {
        let mut s = state.service.lock().unwrap();
        if state.get_cached_settings().is_err() {
            state
                .save_settings(&mut s, AppSettings::default(), Some(password))
                .unwrap();
        }
        let user_id = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        s.create_new_voucher(
            standard_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(user_id),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "100".to_string(),
                    unit: "Taler".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(password),
        )
        .unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()
            .into_iter()
            .find(|v| v.status == VoucherStatus::Active)
            .unwrap()
            .local_instance_id
    }

    fn active_voucher_id(state: &AppState) -> String {
        let s = state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()
            .into_iter()
            .find(|v| v.status == VoucherStatus::Active)
            .unwrap()
            .local_instance_id
    }

    #[test]
    fn test_evaluate_checkout_release_ipc_fail_closed() {
        let confirmed_str = CheckoutReleaseVerdict::new(
            L2ProcessingOutcome::Yellow {
                reason: "Untrusted IPC outcome observation".into(),
            },
            true,
        );
        assert!(!confirmed_str.can_release_goods_automatically);
        assert!(confirmed_str.requires_merchant_approval);
    }

    #[tokio::test]
    async fn test_offline_multi_split_and_receive_without_l2_or_sync() {
        let base_dir = temp_test_dir("offline_multi_split");
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &base_dir.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &base_dir.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));

        let (std_map, standard_toml) = std_map();
        let v1_id = issue_100_voucher(&alice_state, &standard_toml, password);
        let bob_did = bob_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let res1 = create_transfer_bundle_impl(
            None,
            bob_did.clone(),
            vec![FrontendSourceTransfer {
                local_instance_id: v1_id,
                amount_to_send: "30".into(),
            }],
            None,
            None,
            std_map.clone(),
            Some(false),
            Some(false),
            Some(password.into()),
            &alice_state,
        )
        .await
        .expect("Split #1 must succeed offline");
        assert!(res1.l2_dispatch.is_empty());

        let v2_id = active_voucher_id(&alice_state);
        let res2 = create_transfer_bundle_impl(
            None,
            bob_did.clone(),
            vec![FrontendSourceTransfer {
                local_instance_id: v2_id,
                amount_to_send: "25".into(),
            }],
            None,
            None,
            std_map.clone(),
            Some(false),
            Some(false),
            Some(password.into()),
            &alice_state,
        )
        .await
        .expect("Split #2 on change remainder must succeed offline without blocking");

        let v3_id = active_voucher_id(&alice_state);
        let res3 = create_transfer_bundle_impl(
            None,
            bob_did,
            vec![FrontendSourceTransfer {
                local_instance_id: v3_id,
                amount_to_send: "45".into(),
            }],
            None,
            None,
            std_map.clone(),
            Some(false),
            Some(false),
            Some(password.into()),
            &alice_state,
        )
        .await
        .expect("Final spend #3 of remaining balance must succeed offline");

        {
            let s = alice_state.service.lock().unwrap();
            let active_left = s
                .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
                .unwrap()
                .into_iter()
                .filter(|v| v.status == VoucherStatus::Active)
                .count();
            assert_eq!(active_left, 0);
            assert!(s.unresolved_l2_send_intents().unwrap().is_empty());
        }

        for bundle in [res1.bundle_data, res2.bundle_data, res3.bundle_data] {
            receive_bundle_impl(
                None,
                bundle,
                std_map.clone(),
                Some(password.into()),
                false,
                &bob_state,
            )
            .expect("Offline receive_bundle_impl must succeed");
        }

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[tokio::test]
    async fn test_anchored_durable_split_lock_and_committee_dispatch() {
        let base_dir = temp_test_dir("anchored_split_lock");
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &base_dir.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &base_dir.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));

        let (std_map, standard_toml) = std_map();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let sk1 = SigningKey::from_bytes(&[71u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();

        let anchor_nonce = [88u8; 32];
        let anchor_id = {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_settings(&mut s, AppSettings::default(), Some(password))
                .unwrap();
            let mig_pub = s
                .with_wallet_and_identity(|_, id| {
                    l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                        .map(|k| k.verifying_key().to_bytes())
                })
                .unwrap()
                .unwrap();
            let manifest = AnchorManifest {
                revision: 1,
                threshold: 1,
                servers: vec![ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", p1),
                    read_url: format!("http://127.0.0.1:{}/d", p1),
                    admin_pub: sk1.verifying_key().to_bytes(),
                    lock_pub: sk1.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Creator,
                }],
                successors: vec![],
                authority: AnchorAuthority {
                    creator_mig_pub: mig_pub,
                    guarantor_mig_pubs: vec![],
                },
                migration_chain: vec![],
                issued_at_ms: 1_760_000_000_000,
                retain_until_quarter: retain_q,
            };
            let aid = manifest.rev1_anchor_id().unwrap();
            let mut ticket = GenesisTicket {
                anchor_id: aid,
                retain_until_quarter: retain_q,
                issued_at: 1_760_000_000,
                lock_pub: sk1.verifying_key().to_bytes(),
                sig: [0u8; 64],
            };
            ticket.sig = sk1.sign(&ticket.digest()).to_bytes();
            s.register_l2_anchor_template_with_nonce(
                manifest,
                vec![ticket],
                Some(&anchor_nonce),
                Some(password),
            )
            .unwrap();
            let uid = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
            s.create_new_voucher(
                &standard_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(uid),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "100".into(),
                        unit: "Taler".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(aid),
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();
            aid
        };
        let _ = anchor_id;

        let bob_did = bob_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let srv = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = l1.accept().await else {
                    break;
                };
                let mut buf = [0u8; 65_536];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                let req: human_money_core::models::layer2_api::SyncLockBatchRequest =
                    serde_json::from_str(body_str).unwrap();
                let receipts: Vec<LockReceipt> = req
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        let wire = decode_hex_fixed::<605>(h).unwrap();
                        let entry =
                            human_money_core::humoco_proto::causal::LockEntry::from_wire_bytes(
                                &wire,
                            )
                            .unwrap();
                        let mut r = LockReceipt {
                            kind: LockReceiptKind::FirstWrite,
                            lock_pub: sk1.verifying_key().to_bytes(),
                            bucket: req.bucket,
                            slot: entry.slot,
                            child_lock: entry.child_lock,
                            entry_hash: entry.entry_hash().unwrap(),
                            retain_until_quarter: retain_q,
                            write_seq: (i as u64) + 1,
                            bucket_head: [3u8; 32],
                            recipient_nonce: req.recipient_nonce,
                            sig: [0u8; 64],
                        };
                        r.sig = sk1.sign(&r.digest()).to_bytes();
                        r
                    })
                    .collect();
                let resp = serde_json::json!({
                    "status": "ok",
                    "bucket": hex::encode(req.bucket),
                    "retain_until_quarter": retain_q,
                    "receipts": receipts,
                    "conflict": null,
                })
                .to_string();
                let http = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    resp.len(),
                    resp
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        });

        let v1_id = active_voucher_id(&alice_state);
        let res1 = create_transfer_bundle_impl(
            None,
            bob_did,
            vec![FrontendSourceTransfer {
                local_instance_id: v1_id,
                amount_to_send: "30".into(),
            }],
            None,
            None,
            std_map,
            Some(false),
            Some(false),
            Some(password.into()),
            &alice_state,
        )
        .await
        .expect("Anchored split should succeed and confirm with server");

        assert_eq!(res1.l2_dispatch.len(), 1);
        assert_eq!(res1.l2_dispatch[0].status, "confirmed");
        let unresolved = alice_state
            .service
            .lock()
            .unwrap()
            .unresolved_l2_send_intents()
            .unwrap();
        assert!(
            unresolved.is_empty(),
            "Confirmed committee write resolves intent immediately"
        );

        srv.abort();
        let _ = std::fs::remove_dir_all(base_dir);
    }
}
