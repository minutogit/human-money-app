use crate::{
    models::{
        CreateBundleResult, FrontendError, FrontendSourceTransfer, FrontendTransferPlan,
        ReceiveSuccessPayload, TransactionRecord,
    },
    AppState,
};
use chrono::Utc;
use human_money_core::{
    humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS,
    wallet::{MultiTransferRequest, SourceTransfer},
};
use log::{error, info};
use rand::RngCore;
use std::collections::HashMap;
use std::time::Duration;
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

#[allow(clippy::too_many_arguments)]
pub async fn create_transfer_bundle_impl(
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

    {
        let mut service = state.lock_service()?;
        crate::services::common::ensure_restore_l2_reconciled(
            state,
            &mut service,
            password.as_deref(),
        )?;
    }

    let source_transfers: Vec<SourceTransfer> = sources
        .into_iter()
        .map(|s| SourceTransfer {
            local_instance_id: s.local_instance_id,
            amount_to_send: s.amount_to_send.trim().replace(',', "."),
        })
        .collect();

    let (result, intents, is_durable, pow_base_bits) = {
        let mut service = state.lock_service()?;

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
            let own_profile_opt: Option<crate::models::StoredOwnMarketProfile> =
                crate::services::market_service::load_own_market_profile_state(
                    state,
                    &mut service,
                    password.as_deref(),
                )
                .ok();

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

        let (all_anchored, pow_bits) = service
            .with_wallet(|wallet| {
                let mut all_anchored = !source_transfers.is_empty();
                let mut write_urls: Vec<&str> = Vec::new();
                for src in &source_transfers {
                    match wallet
                        .get_voucher_instance(&src.local_instance_id)
                        .and_then(|inst| inst.voucher.layer2_anchor)
                    {
                        Some(anchor_id) => {
                            let anchor_hex = hex::encode(anchor_id);
                            if let Some(manifest) =
                                wallet.profile.l2_anchor_manifests.get(&anchor_hex)
                            {
                                write_urls
                                    .extend(manifest.servers.iter().map(|s| s.write_url.as_str()));
                            }
                        }
                        None => {
                            all_anchored = false;
                        }
                    }
                }
                let bits = crate::services::common::max_pow_base_bits_for_servers(
                    &loaded_settings,
                    write_urls,
                );
                (all_anchored, bits)
            })
            .unwrap_or((!source_transfers.is_empty(), DEFAULT_LOCK_POW_BASE_BITS));
        let source_count = source_transfers.len();
        let record_recipient_id = recipient_id.clone();
        let record_notes = notes.clone();
        let record_sender_profile_name = sender_profile_name.clone();
        let request = MultiTransferRequest {
            recipient_id,
            sources: source_transfers,
            notes,
            sender_profile_name,
            use_privacy_mode,
            wot_sync_json,
            contact_card_json,
        };

        let archive = app.and_then(|a| crate::services::common::open_wallet_archive(a, &service));
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

        // Persist the outgoing TransactionRecord with full bundle_data immediately after L1 commit
        // and BEFORE the L2 network fan-out so closing/interruption never loses the receipt or `.transfer`.
        if let Ok(sender_id) = service.with_wallet(|w| w.get_user_id().to_string()) {
            use crate::services::balance_service::{format_fixed_point, parse_fixed_point};
            let summable_by_name: HashMap<String, bool> = standard_definitions_toml
                .values()
                .filter_map(|toml| {
                    human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(toml).ok()
                })
                .map(|(def, _)| (def.immutable.identity.name, def.immutable.features.balances_are_summable))
                .collect();
            let mut summable_sums: HashMap<String, i128> = HashMap::new();
            let mut countable_items: HashMap<String, u32> = HashMap::new();
            let involved_vouchers: Vec<String> = result
                .involved_sources_details
                .iter()
                .map(|d| d.local_instance_id.clone())
                .collect();
            for detail in &result.involved_sources_details {
                let summable = summable_by_name
                    .get(&detail.standard_name)
                    .copied()
                    .unwrap_or(true);
                if summable {
                    if let Some(amt) = parse_fixed_point(&detail.amount) {
                        *summable_sums
                            .entry(detail.display_currency.clone())
                            .or_default() += amt;
                    }
                } else {
                    *countable_items
                        .entry(detail.display_currency.clone())
                        .or_default() += 1;
                }
            }
            let summable_amounts = summable_sums
                .into_iter()
                .map(|(k, v)| (k, format_fixed_point(v)))
                .collect();
            let record = TransactionRecord {
                id: Uuid::new_v4().to_string(),
                direction: "sent".to_string(),
                recipient_id: record_recipient_id,
                sender_id,
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts,
                countable_items,
                involved_vouchers,
                involved_sources_details: Some(
                    result
                        .involved_sources_details
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect(),
                ),
                bundle_data: result.bundle_bytes.clone(),
                bundle_id: result.bundle_id.clone(),
                notes: record_notes,
                sender_profile_name: record_sender_profile_name,
            };
            if let Err(err) = state.append_to_history(&mut service, record, password.as_deref()) {
                log::warn!("Persisting sent TransactionRecord failed: {}", err);
            }
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
            let service = state.lock_service()?;
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
            let mut service = state.lock_service()?;
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

struct ReceivedVoucherMarkers {
    prev_hashes: std::collections::HashSet<String>,
    tx_ids: std::collections::HashSet<String>,
    trap_tags: std::collections::HashSet<String>,
}

fn collect_received_voucher_markers(
    received_vouchers: &[human_money_core::Voucher],
) -> ReceivedVoucherMarkers {
    let mut prev_hashes = std::collections::HashSet::<String>::new();
    let mut tx_ids = std::collections::HashSet::<String>::new();
    let mut trap_tags = std::collections::HashSet::<String>::new();
    for voucher in received_vouchers {
        for tx in &voucher.transactions {
            if !tx.prev_hash.is_empty() {
                prev_hashes.insert(tx.prev_hash.clone());
            }
            if !tx.t_id.is_empty() {
                tx_ids.insert(tx.t_id.clone());
            }
            if let Some(ref trap) = tx.trap_data {
                if !trap.ds_tag.is_empty() {
                    trap_tags.insert(trap.ds_tag.clone());
                }
            }
        }
    }
    ReceivedVoucherMarkers {
        prev_hashes,
        tx_ids,
        trap_tags,
    }
}

/// Filters wallet-wide double-spend conflicts down to conflicts belonging to the received
/// vouchers, while counting unrelated unresolved conflicts separately so clean payments
/// are never falsely flagged.
pub(crate) fn filter_conflicts_for_received_vouchers(
    service: &human_money_core::app_service::AppService,
    received_vouchers: &[human_money_core::Voucher],
) -> Result<(Vec<crate::models::FrontendProofOfDoubleSpendSummary>, Option<usize>), FrontendError> {
    let all_conflicts = service.list_conflicts().map_err(FrontendError::from)?;
    if all_conflicts.is_empty() {
        return Ok((Vec::new(), None));
    }

    let markers = collect_received_voucher_markers(received_vouchers);
    let mut matching = Vec::new();
    let mut unrelated_open = 0usize;

    for summary in all_conflicts {
        let mut is_match = markers.prev_hashes.contains(&summary.fork_point_prev_hash);

        if !is_match {
            if let Ok(full) = service.get_proof_of_double_spend(&summary.proof_id) {
                is_match = full.conflicting_transactions.iter().any(|ct| {
                    markers.tx_ids.contains(&ct.t_id)
                        || markers.prev_hashes.contains(&ct.prev_hash)
                        || ct
                            .trap_data
                            .as_ref()
                            .is_some_and(|t| markers.trap_tags.contains(&t.ds_tag))
                }) || full.fingerprints.iter().any(|fp| {
                    markers.tx_ids.contains(&fp.t_id) || markers.trap_tags.contains(&fp.ds_tag)
                });
            }
        }

        if is_match {
            matching.push(summary.into());
        } else if !summary.is_resolved && !summary.local_override {
            unrelated_open += 1;
        }
    }

    Ok((matching, (unrelated_open > 0).then_some(unrelated_open)))
}

pub fn receive_bundle_impl(
    app: Option<&tauri::AppHandle>,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    state: &AppState,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    info!("Attempting to receive and process a transfer bundle...");
    let mut service = state.lock_service()?;
    let archive = app.and_then(|a| crate::services::common::open_wallet_archive(a, &service));

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
                involved_sources_details: Some(
                    involved_vouchers_details
                        .iter()
                        .cloned()
                        .map(Into::into)
                        .collect(),
                ),
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

            let receive_kind = service
                .with_wallet(|w| {
                    let mut types = involved_vouchers_ids
                        .iter()
                        .filter_map(|id| w.get_voucher_instance(id))
                        .filter_map(|inst| {
                            inst.voucher.transactions.last().map(|t| t.t_type.clone())
                        })
                        .collect::<Vec<_>>();
                    if types.is_empty() {
                        return Some("transfer".to_string());
                    }
                    types.sort();
                    types.dedup();
                    if types.len() == 1 && (types[0] == "claim" || types[0] == "redeem") {
                        Some(types[0].clone())
                    } else {
                        Some("transfer".to_string())
                    }
                })
                .ok()
                .flatten();

            let opened_vouchers: Vec<human_money_core::Voucher> = service
                .with_wallet_and_identity(|_, id| {
                    human_money_core::services::bundle_processor::open_and_verify_bundle(
                        id,
                        &bundle_data,
                    )
                    .map(|b| b.vouchers)
                    .unwrap_or_default()
                })
                .unwrap_or_default();
            let (conflict_summaries, unrelated_open_conflicts_count) =
                filter_conflicts_for_received_vouchers(&service, &opened_vouchers)?;
            let markers = collect_received_voucher_markers(&opened_vouchers);
            let verifiable_conflicts: HashMap<String, Vec<human_money_core::models::conflict::TransactionFingerprint>> =
                result
                    .check_result
                    .verifiable_conflicts
                    .into_iter()
                    .filter(|(k, fps)| {
                        markers.prev_hashes.contains(k)
                            || fps.iter().any(|fp| {
                                markers.tx_ids.contains(&fp.t_id)
                                    || markers.trap_tags.contains(&fp.ds_tag)
                            })
                    })
                    .collect();

            drop(service);
            crate::services::post_service::remove_pending_pos_payment_by_bytes(
                state,
                &bundle_data,
                password.as_deref(),
            );

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
                verifiable_conflicts,
                conflict_summaries,
                l2_quorum: None,
                receive_kind,
                checkout_verdict: None,
                purchase_id: None,
                newly_committed: None,
                is_replay: None,
                wot_imported_lists,
                unrelated_open_conflicts_count,
            })
        }
        Err(e) => {
            error!("Failed to process bundle: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

pub fn save_transaction_record_impl(
    record: TransactionRecord,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    log::debug!("Saving new transaction record");
    let mut service = state.service.lock().unwrap();
    state
        .append_to_history(&mut service, record, password.as_deref())
        .map_err(FrontendError::from)
}

pub fn get_transaction_history_impl(
    state: &AppState,
) -> Result<Vec<TransactionRecord>, FrontendError> {
    info!("Loading transaction history...");
    state.get_cached_history().map_err(FrontendError::from)
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

pub fn prepare_transfer_plan_impl(
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

pub async fn checkout_received_bundle_impl(
    purchase_id: String,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    state: &AppState,
) -> Result<crate::checkout::PurchaseCompletion, FrontendError> {
    crate::checkout::checkout_bundle(
        state,
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
    use crate::settings::AppSettings;
    use crate::test_support::{
        create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE,
    };
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

    #[tokio::test]
    async fn test_unrelated_conflict_does_not_flag_clean_transfer() {
        let base_dir = temp_test_dir("unrelated_conflict_clean_transfer");
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &base_dir.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let carol_dir = base_dir.join("carol");
        let carol_clone_dir = base_dir.join("carol_clone");
        let carol_state = create_test_state(setup_test_app(
            &carol_dir,
            "Carol",
            crate::test_support::CHARLIE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &base_dir.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let (std_map, standard_toml) = std_map();

        // Innocent Alice issues a clean voucher v2; unrelated Carol issues v1 and double-spends it.
        let v1_id = issue_100_voucher(&carol_state, &standard_toml, password);
        let v2_id = issue_100_voucher(&alice_state, &standard_toml, password);

        // Snapshot Carol's wallet directory BEFORE spending v1 so clone spends the exact same v1 state
        fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
            std::fs::create_dir_all(dst).unwrap();
            for entry in std::fs::read_dir(src).unwrap() {
                let entry = entry.unwrap();
                let ty = entry.file_type().unwrap();
                let target = dst.join(entry.file_name());
                if ty.is_dir() {
                    copy_dir(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), &target).unwrap();
                }
            }
        }
        copy_dir(&carol_dir, &carol_clone_dir);
        let mut clone_svc = human_money_core::app_service::AppService::new(&carol_clone_dir).unwrap();
        let clone_folder = clone_svc.list_profiles().unwrap()[0].folder_name.clone();
        clone_svc
            .login(&clone_folder, password, false, "device-drops-test".to_string())
            .unwrap();
        clone_svc.unlock_session(password, 300).unwrap();
        let carol_clone_state = create_test_state(clone_svc);
        {
            let mut s = carol_clone_state.service.lock().unwrap();
            carol_clone_state
                .load_settings(&mut s, Some(password))
                .unwrap();
        }

        let bob_did = bob_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let b1_first = create_transfer_bundle_impl(
            None,
            bob_did.clone(),
            vec![FrontendSourceTransfer {
                local_instance_id: v1_id.clone(),
                amount_to_send: "20".into(),
            }],
            None,
            None,
            std_map.clone(),
            Some(false),
            Some(false),
            Some(password.into()),
            &carol_state,
        )
        .await
        .unwrap();

        let b1_double = create_transfer_bundle_impl(
            None,
            bob_did.clone(),
            vec![FrontendSourceTransfer {
                local_instance_id: v1_id,
                amount_to_send: "25".into(),
            }],
            None,
            None,
            std_map.clone(),
            Some(false),
            Some(false),
            Some(password.into()),
            &carol_clone_state,
        )
        .await
        .unwrap();

        receive_bundle_impl(
            None,
            b1_first.bundle_data,
            std_map.clone(),
            Some(password.into()),
            false,
            &bob_state,
        )
        .unwrap();
        let ds_recv = receive_bundle_impl(
            None,
            b1_double.bundle_data,
            std_map.clone(),
            Some(password.into()),
            false,
            &bob_state,
        )
        .unwrap();
        assert!(
            !ds_recv.conflict_summaries.is_empty(),
            "Double-spend on v1 must populate conflict_summaries on the conflicting receive"
        );

        // Now innocent Alice sends a completely unrelated, clean voucher v2 to Bob
        let b2_clean = create_transfer_bundle_impl(
            None,
            bob_did,
            vec![FrontendSourceTransfer {
                local_instance_id: v2_id,
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
        .unwrap();

        let clean_recv = receive_bundle_impl(
            None,
            b2_clean.bundle_data,
            std_map,
            Some(password.into()),
            false,
            &bob_state,
        )
        .unwrap();
        assert!(
            clean_recv.conflict_summaries.is_empty(),
            "Clean payment on v2 must NOT inherit unrelated v1 conflicts"
        );
        assert_eq!(
            clean_recv.unrelated_open_conflicts_count,
            Some(1),
            "Clean payment must report unrelated unresolved conflict separately"
        );

        let _ = std::fs::remove_dir_all(base_dir);
    }
}
