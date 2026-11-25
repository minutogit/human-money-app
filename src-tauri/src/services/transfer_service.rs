use crate::{
    models::{
        CreateBundleResult, FrontendError, FrontendSourceTransfer, FrontendTransferPlan,
        ReceiveCommitMode, ReceiveSuccessPayload, TransactionRecord,
    },
    AppState,
};
use chrono::Utc;
use human_money_core::{
    humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS,
    wallet::{MultiTransferRequest, SourceTransfer},
};
use log::{error, info};
use sha2::Digest;
use std::collections::HashMap;
use uuid::Uuid;

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
    deposit_to_post: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<CreateBundleResult, FrontendError> {
    let validated_recipient =
        human_money_core::ValidatedUserId::parse(&recipient_id).map_err(FrontendError::from)?;
    let recipient_pk_bytes = validated_recipient.public_key_bytes();
    let recipient_id = validated_recipient.into_string();
    log::debug!("Creating transfer bundle (sources={})", sources.len());

    let source_transfers: Vec<SourceTransfer> = sources
        .into_iter()
        .map(|s| SourceTransfer {
            local_instance_id: s.local_instance_id,
            amount_to_send: s.amount_to_send.trim().replace(',', "."),
        })
        .collect();
    let blocking_ids: Vec<&str> = source_transfers
        .iter()
        .map(|s| s.local_instance_id.as_str())
        .collect();

    let (session, resolved_pw, result, intents, pow_base_bits) = {
        let (mut service_guard, resolved_pw) = state.authorize_and_lock_secret(
            crate::state::RequiredAccess::DailyPay,
            password.as_deref(),
        )?;
        let service = &mut *service_guard;
        let pw = resolved_pw.as_deref().map(String::as_str);
        let session = state.session_token_for_service(service)?;
        let preflight = crate::services::preflight::ValueOutflowPreflight {
            require_restore_reconciled: true,
            check_known_offender_dids: &[recipient_id.as_str()],
            check_blocking_intent_local_ids: &blocking_ids,
            check_anchor_id_quorum_health: None,
            preverified_anchor_manifest: None,
        };
        crate::services::preflight::run_value_outflow_preflight(state, service, pw, &preflight)?;

        let mut wot_sync_json: Option<String> = None;
        let mut pending_receipt: Option<humoco_wot::SyncReceipt> = None;

        let mut contact_card_json: Option<String> = None;
        let mut pending_card_sent: Option<([u8; 32], u64)> = None;

        let loaded_ui_prefs = state
            .get_or_load_ui_preferences(service, pw)
            .map_err(FrontendError::from)?;
        let loaded_lifecycle = state
            .get_or_load_internal_lifecycle(service, pw)
            .map_err(FrontendError::from)?;

        let should_send_card =
            send_card_override.unwrap_or(loaded_ui_prefs.market_send_card_on_payment);
        if should_send_card {
            let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
            let own_profile_opt: Option<crate::models::StoredOwnMarketProfile> =
                crate::services::market_service::load_own_market_profile_state(state, service, pw)
                    .ok();

            if let Some(own_profile) = own_profile_opt {
                if own_profile.published {
                    if let Some(card) = own_profile.last_card {
                        if now_secs < card.payload.valid_until {
                            {
                                let peer_pk_bytes = recipient_pk_bytes;
                                let store_res = crate::state::load_encrypted_json_or_default::<
                                    humoco_market::CardStore,
                                >(
                                    service, crate::state::MARKET_CARD_STORE_KEY, pw
                                );
                                match store_res {
                                    Ok(store) => {
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
                                    Err(e) => {
                                        log::warn!(
                                            "Failed to load CardStore before attaching card: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Ok(can_recipient) = crate::state::canonicalize_did(&recipient_id) {
            if let Ok(mut reg) = state.get_or_load_wot_register(service, pw) {
                if reg
                    .own_contacts
                    .get(&can_recipient)
                    .is_some_and(|e| e.level >= 1)
                {
                    if let Ok(changed) = state.ensure_wot_list_signed(service, &mut reg) {
                        if changed {
                            let _ = state.save_wot_register(service, &reg, pw);
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

        let (pow_bits, all_anchored_have_manifest) = service
            .with_wallet(|wallet| {
                let mut lock_pubs: Vec<[u8; 32]> = Vec::new();
                let mut all_have_manifest = true;
                for src in &source_transfers {
                    if let Some(anchor_id) = wallet
                        .get_voucher_instance(&src.local_instance_id)
                        .and_then(|inst| inst.voucher.layer2_anchor)
                    {
                        let anchor_hex = hex::encode(anchor_id);
                        if let Some(manifest) = wallet.profile.l2_anchor_manifests.get(&anchor_hex)
                        {
                            lock_pubs.extend(manifest.servers.iter().map(|s| s.lock_pub));
                        } else {
                            all_have_manifest = false;
                        }
                    }
                }
                (
                    crate::services::common::max_pow_base_bits_for_servers(
                        &loaded_lifecycle,
                        lock_pubs,
                    ),
                    all_have_manifest,
                )
            })
            .unwrap_or((DEFAULT_LOCK_POW_BASE_BITS, true));
        let source_count = source_transfers.len();
        let record_recipient_id = recipient_id.clone();
        let record_notes = notes.clone();
        let record_sender_profile_name = sender_profile_name.clone();
        let mut summable_by_uuid: HashMap<String, bool> = HashMap::new();
        let mut summable_by_name: HashMap<String, bool> = HashMap::new();
        for toml in standard_definitions_toml.values() {
            if let Ok((def, _)) =
                human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(toml)
            {
                let summable = def.immutable.features.balances_are_summable;
                summable_by_uuid.insert(def.immutable.identity.uuid, summable);
                summable_by_name.insert(def.immutable.identity.name, summable);
            }
        }
        let baseline_history = state
            .load_history_from_disk(service, pw)
            .map_err(FrontendError::from)?;
        let baseline_outgoing = crate::state::load_outgoing_bundles(service, pw)?;
        let next_history_cell = std::cell::RefCell::new(None::<Vec<TransactionRecord>>);
        let next_outgoing_cell =
            std::cell::RefCell::new(None::<crate::models::StoredOutgoingBundles>);
        let request = MultiTransferRequest {
            recipient_id,
            sources: source_transfers,
            notes,
            sender_profile_name,
            use_privacy_mode,
            wot_sync_json,
            contact_card_json,
        };

        let archive = app.and_then(|a| crate::services::common::open_wallet_archive(a, service));
        let build_companions =
            |res: &human_money_core::wallet::CreateBundleResult,
             wallet: &human_money_core::wallet::Wallet| {
                let record = build_sent_transaction_record(
                    wallet,
                    wallet.get_user_id().to_string(),
                    record_recipient_id.clone(),
                    res,
                    &summable_by_uuid,
                    &summable_by_name,
                    record_notes.clone(),
                    record_sender_profile_name.clone(),
                );
                let batch = crate::state::prepare_outgoing_history_and_bundle_companions(
                    &baseline_history,
                    &baseline_outgoing,
                    vec![(record, res.bundle_bytes.clone())],
                )
                .map_err(|e| {
                    human_money_core::Error::Storage(human_money_core::StorageError::InvalidFormat(
                        e.message,
                    ))
                })?;
                *next_history_cell.borrow_mut() = Some(batch.history);
                *next_outgoing_cell.borrow_mut() = Some(batch.outgoing);
                Ok(batch.blobs)
            };
        let result = if all_anchored_have_manifest {
            service.create_transfer_bundle_with_l2_intents_and_companions(
                request,
                &standard_definitions_toml,
                archive.as_ref(),
                pw,
                pow_bits,
                build_companions,
            )
        } else {
            log::warn!(
                "Offline-first transfer fallback: local AnchorManifest missing for anchored source(s) (sources={}); committing offline without L2SendIntent",
                source_count
            );
            service.create_transfer_bundle_with_companions(
                request,
                &standard_definitions_toml,
                archive.as_ref(),
                pw,
                build_companions,
            )
        }
        .map_err(|e| {
            error!(
                "create_transfer_bundle failed (durable_sync_lock, sources={}): {}",
                source_count, e
            );
            FrontendError::from(e)
        })?;

        if let Some(next_hist) = next_history_cell.into_inner() {
            state.write_cache_slot_for_service(service, |c| &mut c.history, Some(next_hist));
        }
        if let Some(next_out) = next_outgoing_cell.into_inner() {
            state.write_cache_slot_for_service(
                service,
                |c| &mut c.outgoing_bundles,
                Some(next_out),
            );
        }

        let mut intents = Vec::with_capacity(result.involved_sources_details.len());
        for source in &result.involved_sources_details {
            if let Ok(intent) = service.pending_l2_send_intent(&source.local_instance_id) {
                intents.push(intent);
            }
        }

        if let Some((peer_pk_bytes, own_v)) = pending_card_sent {
            match state.load_card_store_or_cached(service, pw) {
                Ok(mut store) => {
                    humoco_market::mark_sent(&mut store, peer_pk_bytes, own_v);
                    if let Err(e) = state.save_card_store(service, &store, pw) {
                        log::warn!("Failed to save CardStore after sending card: {}", e);
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Skipping CardStore mark_sent after transfer because load failed: {}",
                        e
                    );
                }
            }
        }

        if let Some(receipt) = pending_receipt {
            if let Ok(mut reg) = state.get_or_load_wot_register(service, pw) {
                let now_secs = crate::sync_client::current_unix_timestamp();
                reg.commit_sync_receipt(receipt, now_secs);
                if let Err(e) = state.save_wot_register(service, &reg, pw) {
                    log::warn!(
                        "Failed to save WoT register after committing sync receipt: {}",
                        e
                    );
                }
            }
        }

        if let Err(error) = state.refresh_events_cache(service, pw) {
            error!("Failed to refresh events after transfer commit: {}", error);
        }

        (session, resolved_pw, result, intents, pow_bits)
    };
    let pw = resolved_pw.as_deref().map(String::as_str);

    // Service mutex released; L1 + durable L2 intents are committed before network fan-out.
    let l2_dispatch = match crate::services::preflight::dispatch_l2_send_intents(
        state,
        &session,
        &intents,
        pow_base_bits,
        pw,
    )
    .await
    {
        Ok(obs) => obs,
        Err(err) => {
            log::warn!(
                "Transfer L1 committed ({}); L2 intent dispatch deferred/pending: {}",
                result.bundle_id,
                err.message
            );
            intents
                .iter()
                .map(|i| crate::gateway_resolver::IntentDispatchObservation {
                    local_instance_id: i.local_instance_id.clone(),
                    status: "pending".to_string(),
                    http_status: None,
                })
                .collect()
        }
    };

    let (post_deposit, post_deposit_error) = if deposit_to_post.unwrap_or(false) {
        match crate::services::post_service::deposit_transfer_to_post_with_session(
            &session,
            result.bundle_id.clone(),
            pw,
            state,
        )
        .await
        {
            Ok(dep) => (Some(dep), None),
            Err(err) => (None, Some(err.message)),
        }
    } else {
        (None, None)
    };

    Ok(CreateBundleResult {
        bundle_data: result.bundle_bytes,
        bundle_id: result.bundle_id,
        involved_sources_details: result
            .involved_sources_details
            .into_iter()
            .map(Into::into)
            .collect(),
        l2_dispatch,
        post_deposit,
        post_deposit_error,
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
) -> Result<
    (
        Vec<crate::models::FrontendProofOfDoubleSpendSummary>,
        Option<usize>,
    ),
    FrontendError,
> {
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
        } else if !summary.local_override {
            unrelated_open += 1;
        }
    }

    Ok((matching, (unrelated_open > 0).then_some(unrelated_open)))
}

pub(crate) fn apply_receive_companion_side_effects(
    service: &mut human_money_core::app_service::AppService,
    state: &AppState,
    result: &human_money_core::wallet::types::ProcessBundleResult,
    password: Option<&str>,
) -> Option<usize> {
    if let Err(e) = state.refresh_events_cache(service, password) {
        error!("Failed to refresh events cache after receive_bundle: {}", e);
    }

    let wot_imported_lists = if let Some(ref wot_json) = result.wot_sync_json {
        state.try_import_piggybacked_wot(service, wot_json, &result.header.sender_id, password)
    } else {
        None
    };

    if let Some(ref card_json) = result.contact_card_json {
        state.try_import_piggybacked_contact_card(
            service,
            Some(card_json),
            Some(&result.header.sender_id),
            password,
        );
    }

    wot_imported_lists
}

pub(crate) fn build_receive_success_payload_from_result(
    service: &mut human_money_core::app_service::AppService,
    state: &AppState,
    bundle_data: &[u8],
    result: human_money_core::wallet::types::ProcessBundleResult,
    effective_purchase_id: String,
    password: Option<&str>,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    let conflict_count = result.check_result.verifiable_conflicts.len();
    let warning_count = result.check_result.unverifiable_warnings.len();

    if conflict_count > 0 || warning_count > 0 {
        log::warn!(
            "Fraud detection during bundle import: confirmed_conflicts={}, gossip_warnings={}",
            conflict_count,
            warning_count
        );
    }

    let involved_vouchers_details = result.involved_vouchers_details.clone();
    let involved_vouchers_ids: Vec<String> = involved_vouchers_details
        .iter()
        .map(|d| d.local_instance_id.clone())
        .collect();
    let notes = result.header.notes.clone();
    let sender_profile_name = result.header.sender_profile_name.clone();

    let wot_imported_lists =
        apply_receive_companion_side_effects(service, state, &result, password);

    let receive_kind = service
        .with_wallet(|w| {
            let mut types = involved_vouchers_ids
                .iter()
                .filter_map(|id| w.get_voucher_instance(id))
                .filter_map(|inst| inst.voucher.transactions.last().map(|t| t.t_type.clone()))
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

    let (opened_vouchers, reduced_guarantor_protection, manifest_revision) =
        if !involved_vouchers_ids.is_empty() {
            service
                .with_wallet(|w| -> Result<_, FrontendError> {
                    let mut vouchers = Vec::with_capacity(involved_vouchers_ids.len());
                    let mut reduced = false;
                    let mut rev: Option<u64> = None;
                    for id in &involved_vouchers_ids {
                        let inst = w.get_voucher_instance(id).ok_or_else(|| {
                            FrontendError::from(format!(
                                "Committed voucher '{}' missing from wallet after receive",
                                id
                            ))
                        })?;
                        if let Some(anchor_id) = inst.voucher.layer2_anchor {
                            let anchor_hex = hex::encode(anchor_id);
                            if let Some(manifest) = w.profile.l2_anchor_manifests.get(&anchor_hex) {
                                reduced |=
                                    manifest.has_reduced_guarantor_protection().unwrap_or(false);
                                rev =
                                    Some(rev.map_or(manifest.revision, |prev| {
                                        prev.max(manifest.revision)
                                    }));
                            }
                        }
                        vouchers.push(inst.voucher.clone());
                    }
                    Ok((vouchers, reduced, rev))
                })
                .map_err(FrontendError::from)??
        } else {
            let vouchers = service
                .with_wallet_and_identity(|_, id| {
                    human_money_core::services::bundle_processor::open_and_verify_bundle(
                        id,
                        bundle_data,
                    )
                    .map(|b| b.vouchers)
                })
                .map_err(FrontendError::from)?
                .map_err(FrontendError::from)?;
            let (reduced, rev) = service
                .with_wallet(|w| {
                    let mut reduced = false;
                    let mut rev: Option<u64> = None;
                    for v in &vouchers {
                        if let Some(anchor_id) = v.layer2_anchor {
                            let anchor_hex = hex::encode(anchor_id);
                            if let Some(manifest) = w.profile.l2_anchor_manifests.get(&anchor_hex) {
                                reduced |=
                                    manifest.has_reduced_guarantor_protection().unwrap_or(false);
                                rev =
                                    Some(rev.map_or(manifest.revision, |prev| {
                                        prev.max(manifest.revision)
                                    }));
                            }
                        }
                    }
                    (reduced, rev)
                })
                .map_err(FrontendError::from)?;
            (vouchers, reduced, rev)
        };
    let all_unanchored =
        !opened_vouchers.is_empty() && opened_vouchers.iter().all(|v| v.layer2_anchor.is_none());
    let (conflict_summaries, unrelated_open_conflicts_count) =
        filter_conflicts_for_received_vouchers(service, &opened_vouchers)?;
    let markers = collect_received_voucher_markers(&opened_vouchers);
    let verifiable_conflicts: HashMap<String, Vec<crate::models::FrontendTransactionFingerprint>> =
        result
            .check_result
            .verifiable_conflicts
            .into_iter()
            .filter(|(k, fps)| {
                markers.prev_hashes.contains(k)
                    || fps.iter().any(|fp| {
                        markers.tx_ids.contains(&fp.t_id) || markers.trap_tags.contains(&fp.ds_tag)
                    })
            })
            .map(|(k, fps)| {
                (
                    k,
                    fps.into_iter()
                        .map(crate::models::FrontendTransactionFingerprint::from)
                        .collect(),
                )
            })
            .collect();

    let has_victim_conflict = conflict_summaries
        .iter()
        .any(|c| c.conflict_role == "victim" && !c.local_override)
        || !verifiable_conflicts.is_empty();
    let checkout_verdict = Some(crate::models::FrontendCheckoutReleaseVerdict {
        outcome: if has_victim_conflict {
            "Quarantined:DoubleSpend".to_string()
        } else if all_unanchored {
            "OfflineOnly".to_string()
        } else {
            "Yellow:AcceptedOnTrust".to_string()
        },
        can_release_goods_automatically: false,
        requires_merchant_approval: false,
        is_goods_release_authorized: !has_victim_conflict,
        merchant_warning: None,
        reduced_guarantor_protection,
        manifest_revision,
    });

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
        checkout_verdict,
        purchase_id: Some(effective_purchase_id),
        newly_committed: Some(true),
        is_replay: Some(false),
        wot_imported_lists,
        unrelated_open_conflicts_count,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn receive_bundle_impl(
    app: Option<&tauri::AppHandle>,
    bundle_data: Vec<u8>,
    standard_definitions_toml: HashMap<String, String>,
    password: Option<String>,
    force_accept_tolerance_bundle: bool,
    commit_mode: Option<ReceiveCommitMode>,
    purchase_id: Option<String>,
    state: &AppState,
) -> Result<ReceiveSuccessPayload, FrontendError> {
    info!("Attempting to receive and process a transfer bundle...");
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let effective_purchase_id = purchase_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let digest = sha2::Sha256::digest(&bundle_data);
            format!("bundle-{}", hex::encode(&digest[..16]))
        });

    let mode = commit_mode.unwrap_or_default();
    if mode == ReceiveCommitMode::OnlineVerify {
        let completion = crate::checkout::checkout_bundle(
            state,
            &session,
            &effective_purchase_id,
            &bundle_data,
            &standard_definitions_toml,
            force_accept_tolerance_bundle,
            pw,
        )
        .await?;
        return completion.receive_payload.ok_or_else(|| {
            FrontendError::from("Failed to construct receive payload from checkout".to_string())
        });
    }

    let mut service = state.lock_service()?;
    if !state.is_session_current(&session, &service) {
        return Err(FrontendError::from(
            "Wallet session changed before receiving bundle".to_string(),
        ));
    }
    let archive = app.and_then(|a| crate::services::common::open_wallet_archive(a, &service));
    let baseline_history = state
        .load_history_from_disk(&mut service, pw)
        .map_err(FrontendError::from)?;
    let next_history_cell = std::cell::RefCell::new(None::<Vec<TransactionRecord>>);

    match service.receive_bundle_with_companions(
        &bundle_data,
        &standard_definitions_toml,
        archive.as_ref(),
        pw,
        force_accept_tolerance_bundle,
        None,
        |result, wallet| {
            let transfer_summary = &result.transfer_summary;
            let involved_vouchers_ids: Vec<String> = result
                .involved_vouchers_details
                .iter()
                .map(|d| d.local_instance_id.clone())
                .collect();
            let record = TransactionRecord {
                id: Uuid::new_v4().to_string(),
                direction: "received".to_string(),
                recipient_id: wallet.get_user_id().to_string(),
                sender_id: result.header.sender_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts: transfer_summary.summable_amounts.clone(),
                countable_items: transfer_summary.countable_items.clone(),
                involved_vouchers: involved_vouchers_ids,
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
                notes: result.header.notes.clone(),
                sender_profile_name: result.header.sender_profile_name.clone(),
            };
            let (blobs, next_hist) =
                crate::state::prepare_receive_history_companion(&baseline_history, record)
                    .map_err(|e| {
                        human_money_core::Error::Storage(
                            human_money_core::StorageError::InvalidFormat(e.message),
                        )
                    })?;
            *next_history_cell.borrow_mut() = Some(next_hist);
            Ok(blobs)
        },
    ) {
        Ok(result) => {
            info!("Bundle processed successfully with atomic history commit.");

            if let Some(next_hist) = next_history_cell.into_inner() {
                state.write_cache_slot_for_service(&service, |c| &mut c.history, Some(next_hist));
            }

            let payload = build_receive_success_payload_from_result(
                &mut service,
                state,
                &bundle_data,
                result,
                effective_purchase_id,
                pw,
            )?;
            drop(service);
            Ok(payload)
        }
        Err(e) => {
            error!("Failed to process bundle: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_sent_transaction_record(
    wallet: &human_money_core::wallet::Wallet,
    sender_id: String,
    recipient_id: String,
    result: &human_money_core::wallet::CreateBundleResult,
    summable_by_uuid: &HashMap<String, bool>,
    summable_by_name: &HashMap<String, bool>,
    notes: Option<String>,
    sender_profile_name: Option<String>,
) -> TransactionRecord {
    use crate::services::balance_service::{format_fixed_point, parse_fixed_point};
    let mut summable_sums: HashMap<String, i128> = HashMap::new();
    let mut countable_items: HashMap<String, u32> = HashMap::new();
    let involved_vouchers: Vec<String> = result
        .involved_sources_details
        .iter()
        .map(|d| d.local_instance_id.clone())
        .collect();
    for detail in &result.involved_sources_details {
        let summable = wallet
            .get_voucher_instance(&detail.local_instance_id)
            .and_then(|inst| summable_by_uuid.get(&inst.voucher.voucher_standard.uuid))
            .or_else(|| summable_by_name.get(&detail.standard_name))
            .copied()
            .unwrap_or(true);
        if summable {
            if let Some(amt) = parse_fixed_point(&detail.amount) {
                let sum = summable_sums
                    .entry(detail.display_currency.clone())
                    .or_default();
                *sum = sum.saturating_add(amt);
            }
        } else {
            let count = countable_items
                .entry(detail.display_currency.clone())
                .or_default();
            *count = count.saturating_add(1);
        }
    }
    let summable_amounts = summable_sums
        .into_iter()
        .map(|(k, v)| (k, format_fixed_point(v)))
        .collect();
    TransactionRecord {
        id: Uuid::new_v4().to_string(),
        direction: "sent".to_string(),
        recipient_id,
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
        bundle_data: Vec::new(),
        bundle_id: result.bundle_id.clone(),
        notes,
        sender_profile_name,
    }
}

pub fn get_transaction_history_impl(
    state: &AppState,
) -> Result<Vec<TransactionRecord>, FrontendError> {
    info!("Loading transaction history...");
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    state.get_hydrated_history().map_err(FrontendError::from)
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

    let wot_opt = state.cache_wot();
    let reg = match wot_opt.as_ref() {
        Some(r) => r,
        None => return (bonuses, trust_percents, false),
    };

    let recipient_list_known =
        reg.raw_list_store.contains_key(&can_recipient) || can_recipient == reg.self_did;
    let now_secs = crate::sync_client::current_unix_timestamp();

    for (local_id, instance) in &wallet.voucher_store.vouchers {
        if instance.status != human_money_core::wallet::instance::VoucherStatus::Active {
            continue;
        }
        if bonuses.get(local_id).is_some_and(|&v| v < 0) {
            continue;
        }

        let creator_id = instance.voucher.creator_profile.id.as_deref().unwrap_or("");
        let creator_canonical =
            crate::state::canonicalize_did(creator_id).unwrap_or_else(|_| creator_id.to_string());
        let guarantor_canonicals: Vec<String> = instance
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
            .map(|s| {
                crate::state::canonicalize_did(&s.signer_id).unwrap_or_else(|_| s.signer_id.clone())
            })
            .collect();
        let guarantor_refs: Vec<&str> = guarantor_canonicals.iter().map(String::as_str).collect();

        if let Ok(eval) = reg.evaluate_voucher_for_observer(
            &can_recipient,
            &creator_canonical,
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
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
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
            .map(|rv| {
                trust_percents
                    .get(&rv.local_instance_id)
                    .copied()
                    .unwrap_or(0)
            })
            .min()
    } else {
        None
    };
    let mut frontend_plan: FrontendTransferPlan = plan.into();
    frontend_plan.min_recipient_trust_percent = min_recipient_trust_percent;
    Ok(frontend_plan)
}

#[cfg(test)]
mod offline_and_degraded_transfer_tests {
    use super::*;
    use crate::services::fs_helpers::TempDirGuard;
    use crate::settings::{InternalLifecycleState, UiPreferences, UI_PREFERENCES_KEY};
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

    fn wallet_did(state: &AppState) -> String {
        state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap()
    }

    fn issue_100_voucher(state: &AppState, standard_toml: &str, password: &str) -> String {
        let mut s = state.service.lock().unwrap();
        if state.get_cached_ui_preferences().is_err() {
            state
                .save_ui_preferences(&mut s, UiPreferences::default(), Some(password))
                .unwrap();
        }
        if state.get_cached_internal_lifecycle().is_err() {
            state
                .save_internal_lifecycle(&mut s, InternalLifecycleState::default(), Some(password))
                .unwrap();
        }
        let before_ids: std::collections::HashSet<String> = s
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()
            .into_iter()
            .map(|v| v.local_instance_id)
            .collect();
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
            .find(|v| {
                v.status == VoucherStatus::Active && !before_ids.contains(&v.local_instance_id)
            })
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

    fn spawn_mock_l2_confirm_server(
        listener: tokio::net::TcpListener,
        sk: SigningKey,
        retain_q: u16,
        head_fill: u8,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 65_536];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    continue;
                }
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
                            lock_pub: sk.verifying_key().to_bytes(),
                            bucket: req.bucket,
                            slot: entry.slot,
                            child_lock: entry.child_lock,
                            entry_hash: entry.entry_hash().unwrap(),
                            retain_until_quarter: retain_q,
                            write_seq: (i as u64) + 1,
                            bucket_head: [head_fill; 32],
                            recipient_nonce: req.recipient_nonce,
                            sig: [0u8; 64],
                        };
                        r.sig = sk.sign(&r.digest()).to_bytes();
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
                stream.write_all(http.as_bytes()).await.unwrap();
            }
        })
    }

    fn register_single_server_anchor_and_voucher(
        state: &AppState,
        port: u16,
        sk: &SigningKey,
        anchor_nonce: &[u8; 32],
        amount: &str,
        password: &str,
    ) -> [u8; 32] {
        let (_, standard_toml) = std_map();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let mut s = state.service.lock().unwrap();
        state
            .save_ui_preferences(&mut s, UiPreferences::default(), Some(password))
            .unwrap();
        state
            .save_internal_lifecycle(&mut s, InternalLifecycleState::default(), Some(password))
            .unwrap();
        let mig_pub = s
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", port),
                read_url: format!("http://127.0.0.1:{}/d", port),
                admin_pub: sk.verifying_key().to_bytes(),
                lock_pub: sk.verifying_key().to_bytes(),
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
            lock_pub: sk.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        ticket.sig = sk.sign(&ticket.digest()).to_bytes();
        s.register_l2_anchor_template_with_nonce(
            manifest,
            vec![ticket],
            Some(anchor_nonce),
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
                    amount: amount.into(),
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
    }

    fn trust_l2_server_on_wallet(
        state: &AppState,
        port: u16,
        sk: &SigningKey,
        retain_q: u16,
        password: &str,
    ) {
        let write_url = format!("http://127.0.0.1:{}/sync.php", port);
        let read_url = format!("http://127.0.0.1:{}/d", port);
        let sk_hex = hex::encode(sk.verifying_key().as_bytes());
        let del_msg = humoco_market::format_lock_delegate_signature_message(
            &sk_hex,
            &sk_hex,
            &write_url,
            &read_url,
            1_760_000_000,
        );
        let del_sig = hex::encode(sk.sign(del_msg.as_bytes()).to_bytes());
        let mut s = state.service.lock().unwrap();
        state
            .save_ui_preferences(&mut s, UiPreferences::default(), Some(password))
            .unwrap();
        let mut sec = crate::settings::SecurityPolicy::default();
        sec.public_syncs.push(crate::settings::StoredPublicSync {
            write_url: write_url.clone(),
            read_url: read_url.clone(),
            drop_pub: sk_hex.clone(),
            register_url: None,
            access_pubkey: "aa".repeat(32),
            enabled: true,
            authorized_channels: vec!["lock".into()],
            status: "active".into(),
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
        });
        state
            .save_security_policy(&mut s, sec, Some(password))
            .unwrap();
        let mut lc = InternalLifecycleState::default();
        lc.l2_probe_cache.insert(
            sk_hex.clone(),
            crate::sync_client::FrontendLockServerProbeResult {
                write_url: write_url.clone(),
                read_url: read_url.clone(),
                drop_pub: sk_hex.clone(),
                admin_pub: sk_hex.clone(),
                lock_pub: sk_hex.clone(),
                delegation_valid: true,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 5,
                max_retention_quarter: retain_q,
                pow_base_bits: DEFAULT_LOCK_POW_BASE_BITS,
                allow_pow_tickets: true,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 50_000,
                probed_at_secs: 1,
                delegation: Some(crate::sync_client::SyncLockDelegationDto {
                    admin_pub: sk_hex.clone(),
                    lock_pub: sk_hex,
                    write_url,
                    read_url,
                    issued_at: 1_760_000_000,
                    sig: del_sig,
                }),
                error: None,
            },
        );
        state
            .save_internal_lifecycle(&mut s, lc, Some(password))
            .unwrap();
    }

    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir_recursive(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    fn build_rev2_two_server_manifest(
        sk1: &SigningKey,
        sk2: &SigningKey,
        mig_sk: &SigningKey,
        retain_q: u16,
    ) -> ([u8; 32], AnchorManifest) {
        let manifest_rev1 = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: "https://s1.example/sync.php".into(),
                read_url: "https://s1.example/d".into(),
                admin_pub: sk1.verifying_key().to_bytes(),
                lock_pub: sk1.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub: mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest_rev1.rev1_anchor_id().unwrap();
        let new_servers = vec![
            manifest_rev1.servers[0].clone(),
            ServerEntry {
                write_url: "https://s2.example/sync.php".into(),
                read_url: "https://s2.example/d".into(),
                admin_pub: sk2.verifying_key().to_bytes(),
                lock_pub: sk2.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            },
        ];
        let prop = human_money_core::services::l2_rotation::build_creator_rotation_proposal(
            anchor_id,
            &manifest_rev1,
            2,
            new_servers,
            Vec::new(),
            1_760_000_020,
            mig_sk,
        )
        .unwrap();
        let state_hash = human_money_core::humoco_proto::compute_import_union_state_hash(
            &anchor_id,
            2,
            &std::collections::BTreeMap::new(),
        )
        .unwrap();
        let prop_hash = prop.core_digest().unwrap();
        let acks: Vec<_> = [sk1, sk2]
            .iter()
            .map(|sk| {
                let mut ack = human_money_core::humoco_proto::ImportAck {
                    anchor_id,
                    proposal_hash: prop_hash,
                    to_rev: 2,
                    imported_state_hash: state_hash,
                    retain_until_quarter: retain_q,
                    lock_pub: sk.verifying_key().to_bytes(),
                    acked_at: 1_760_000_020,
                    sig: [0u8; 64],
                };
                ack.sig = sk.sign(&ack.digest()).to_bytes();
                ack
            })
            .collect();
        let commit_d = human_money_core::humoco_proto::MigrationCert::commit_digest(
            &anchor_id,
            &prop_hash,
            2,
            &state_hash,
        );
        let commit_sig = human_money_core::humoco_proto::CommitteeSignature {
            lock_pub: sk1.verifying_key().to_bytes(),
            committed_at: 1_760_000_020,
            sig: sk1.sign(&commit_d).to_bytes(),
        };
        let (rev2_manifest, _) =
            human_money_core::services::l2_rotation::assemble_migrated_manifest(
                &manifest_rev1,
                &anchor_id,
                prop,
                state_hash,
                acks,
                vec![commit_sig],
            )
            .unwrap();
        (anchor_id, rev2_manifest)
    }

    async fn send_simple_transfer(
        state: &AppState,
        recipient_did: &str,
        local_id: &str,
        amount: &str,
        std_map: &HashMap<String, String>,
        password: &str,
    ) -> CreateBundleResult {
        create_transfer_bundle_impl(
            None,
            recipient_did.to_string(),
            vec![FrontendSourceTransfer {
                local_instance_id: local_id.to_string(),
                amount_to_send: amount.to_string(),
            }],
            None,
            None,
            std_map.clone(),
            Some(false),
            Some(false),
            None,
            Some(password.to_string()),
            state,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_offline_multi_split_and_receive_without_l2_or_sync() {
        let dir_guard = TempDirGuard(temp_test_dir("offline_multi_split"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));

        let (std_map, standard_toml) = std_map();
        let v1_id = issue_100_voucher(&alice_state, &standard_toml, password);
        let bob_did = wallet_did(&bob_state);

        let res1 = send_simple_transfer(&alice_state, &bob_did, &v1_id, "30", &std_map, password).await;
        assert!(res1.l2_dispatch.is_empty());

        let v2_id = active_voucher_id(&alice_state);
        let res2 = send_simple_transfer(&alice_state, &bob_did, &v2_id, "25", &std_map, password).await;

        let v3_id = active_voucher_id(&alice_state);
        let res3 = send_simple_transfer(&alice_state, &bob_did, &v3_id, "45", &std_map, password).await;

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
                Some(ReceiveCommitMode::ForceTrustOffline),
                None,
                &bob_state,
            )
            .await
            .expect("Offline receive_bundle_impl must succeed");
        }
    }

    #[tokio::test]
    async fn test_anchored_durable_split_lock_and_committee_dispatch() {
        let dir_guard = TempDirGuard(temp_test_dir("anchored_split_lock"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));

        let (std_map, _) = std_map();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let sk1 = SigningKey::from_bytes(&[71u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();

        let anchor_id = register_single_server_anchor_and_voucher(
            &alice_state,
            p1,
            &sk1,
            &[88u8; 32],
            "100",
            password,
        );
        assert_ne!(anchor_id, [0u8; 32]);
        let bob_did = wallet_did(&bob_state);
        let srv = spawn_mock_l2_confirm_server(l1, sk1, retain_q, 3u8);

        let v1_id = active_voucher_id(&alice_state);
        let res1 = send_simple_transfer(&alice_state, &bob_did, &v1_id, "30", &std_map, password).await;

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
    }

    fn assert_deep_conflict_matches(
        service: &mut human_money_core::app_service::AppService,
        did: &str,
        clean_voucher: &human_money_core::Voucher,
    ) {
        let mut ct_tid_only = clean_voucher.transactions.last().unwrap().clone();
        ct_tid_only.prev_hash = "aa".repeat(32);
        ct_tid_only.trap_data = None;
        let mut ct_ds_only = clean_voucher
            .transactions
            .iter()
            .rev()
            .find(|t| t.trap_data.is_some())
            .expect("clean voucher has tx with trap_data")
            .clone();
        ct_ds_only.t_id.clear();
        ct_ds_only.prev_hash = "aa".repeat(32);
        let clean_ds_tag = clean_voucher
            .transactions
            .iter()
            .find_map(|t| t.trap_data.as_ref().map(|d| d.ds_tag.clone()))
            .expect("clean transfer transaction must carry trap_data.ds_tag");
        let deep_cases = [
            ("proof-deep-ct-tid", vec![ct_tid_only], Vec::new()),
            ("proof-deep-ct-ds", vec![ct_ds_only], Vec::new()),
            (
                "proof-deep-fp-ds",
                Vec::new(),
                vec![human_money_core::models::conflict::TransactionFingerprint {
                    ds_tag: clean_ds_tag,
                    ..Default::default()
                }],
            ),
        ];
        for (proof_id, conflicting_transactions, fingerprints) in deep_cases {
            service.get_unlocked_mut_for_test().0.proof_store.proofs.insert(
                proof_id.to_string(),
                human_money_core::models::conflict::ProofStoreEntry {
                    proof: human_money_core::models::conflict::ProofOfDoubleSpend {
                        proof_id: proof_id.to_string(),
                        offender_id: did.to_string(),
                        suspected_identity: None,
                        fork_point_prev_hash: "ff".repeat(32),
                        conflicting_transactions,
                        fingerprints,
                        victim_witnesses: Vec::new(),
                        deletable_at: "2099-01-01T00:00:00Z".to_string(),
                        reporter_id: did.to_string(),
                        report_timestamp: "2026-01-01T00:00:00Z".to_string(),
                        reporter_signature: "sig".to_string(),
                        affected_voucher_name: None,
                        voucher_standard_uuid: None,
                        resolutions: None,
                        non_redeemable_test_voucher: false,
                    },
                    local_override: false,
                    local_note: None,
                    conflict_role: human_money_core::models::conflict::ConflictRole::default(),
                },
            );
            let (matched, unrelated) =
                filter_conflicts_for_received_vouchers(service, std::slice::from_ref(clean_voucher))
                    .unwrap();
            assert_eq!(matched.len(), 1, "{proof_id} must match via deep inspection");
            assert_eq!(matched[0].proof_id, proof_id);
            assert_eq!(unrelated, None);
            service.get_unlocked_mut_for_test().0.proof_store.proofs.remove(proof_id);
        }
    }

    #[tokio::test]
    async fn test_unrelated_conflict_does_not_flag_clean_transfer() {
        let dir_guard = TempDirGuard(temp_test_dir("unrelated_conflict_clean_transfer"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let carol_dir = dir_guard.0.join("carol");
        let carol_clone_dir = dir_guard.0.join("carol_clone");
        let carol_state = create_test_state(setup_test_app(
            &carol_dir,
            "Carol",
            crate::test_support::CHARLIE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let (std_map, standard_toml) = std_map();

        let v1_id = issue_100_voucher(&carol_state, &standard_toml, password);
        let v2_id = issue_100_voucher(&alice_state, &standard_toml, password);

        copy_dir_recursive(&carol_dir, &carol_clone_dir);
        let mut clone_svc =
            human_money_core::app_service::AppService::new(&carol_clone_dir).unwrap();
        let clone_folder = clone_svc.list_profiles().unwrap()[0].folder_name.clone();
        clone_svc
            .login(&clone_folder, password, false, "device-drops-test".to_string())
            .unwrap();
        clone_svc.unlock_session(password, 300).unwrap();
        let carol_clone_state = create_test_state(clone_svc);
        {
            let mut s = carol_clone_state.service.lock().unwrap();
            carol_clone_state.load_ui_preferences(&mut s, Some(password)).unwrap();
            carol_clone_state.load_internal_lifecycle(&mut s, Some(password)).unwrap();
        }

        let bob_did = wallet_did(&bob_state);
        let b1_first = send_simple_transfer(&carol_state, &bob_did, &v1_id, "20", &std_map, password).await;
        let b1_double =
            send_simple_transfer(&carol_clone_state, &bob_did, &v1_id, "25", &std_map, password).await;

        receive_bundle_impl(
            None,
            b1_first.bundle_data,
            std_map.clone(),
            Some(password.into()),
            false,
            Some(ReceiveCommitMode::ForceTrustOffline),
            None,
            &bob_state,
        )
        .await
        .unwrap();
        let ds_recv = receive_bundle_impl(
            None,
            b1_double.bundle_data,
            std_map.clone(),
            Some(password.into()),
            false,
            Some(ReceiveCommitMode::ForceTrustOffline),
            None,
            &bob_state,
        )
        .await
        .unwrap();
        assert!(!ds_recv.conflict_summaries.is_empty());
        let unrelated_proof_id = ds_recv.conflict_summaries[0].proof_id.clone();

        let b2_clean = send_simple_transfer(&alice_state, &bob_did, &v2_id, "30", &std_map, password).await;
        let clean_recv = receive_bundle_impl(
            None,
            b2_clean.bundle_data,
            std_map,
            Some(password.into()),
            false,
            Some(ReceiveCommitMode::ForceTrustOffline),
            None,
            &bob_state,
        )
        .await
        .unwrap();
        assert!(clean_recv.conflict_summaries.is_empty());
        assert_eq!(clean_recv.unrelated_open_conflicts_count, Some(1));

        // Setting local_override on the unrelated proof suppresses unrelated_open_conflicts_count.
        let mut s_bob = bob_state.service.lock().unwrap();
        let clean_voucher = s_bob
            .with_wallet(|w| {
                w.get_voucher_instance(&clean_recv.involved_vouchers[0])
                    .unwrap()
                    .voucher
                    .clone()
            })
            .unwrap();
        s_bob
            .set_conflict_local_override(&unrelated_proof_id, true, Some("ignored".into()), Some(password))
            .unwrap();
        let (matching_after, unrelated_after) =
            filter_conflicts_for_received_vouchers(&s_bob, std::slice::from_ref(&clean_voucher))
                .unwrap();
        assert!(matching_after.is_empty());
        assert_eq!(unrelated_after, None);

        assert_deep_conflict_matches(&mut s_bob, &bob_did, &clean_voucher);
    }

    #[tokio::test]
    async fn test_corrupt_card_store_and_settings_fail_closed_without_overwrite() {
        let dir_guard = TempDirGuard(temp_test_dir("transfer_corrupt_stores"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let bob_did = wallet_did(&bob_state);

        let (std_map, standard_toml) = std_map();
        let v1_id = issue_100_voucher(&alice_state, &standard_toml, password);

        crate::services::market_service::save_my_market_profile_impl(
            crate::models::FrontendSaveMyMarketProfileRequest {
                profile: crate::models::FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                visibility: Default::default(),
                skills: Vec::new(),
                max_hops: 2,
                validity_years: 2,
                allow_inquiries: true,
                published: None,
            },
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        crate::services::market_service::set_card_published_impl(
            true,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        {
            let mut s = alice_state.service.lock().unwrap();
            crate::state::save_encrypted_bytes(
                &mut s,
                crate::state::MARKET_CARD_STORE_KEY,
                b"{corrupt-card-store",
                Some(password),
            )
            .unwrap();
        }
        create_transfer_bundle_impl(
            None,
            bob_did.clone(),
            vec![FrontendSourceTransfer {
                local_instance_id: v1_id,
                amount_to_send: "25".into(),
            }],
            None,
            None,
            std_map.clone(),
            Some(true),
            Some(false),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        let raw_card_store = {
            let mut s = alice_state.service.lock().unwrap();
            crate::state::load_encrypted_bytes_opt(
                &mut s,
                crate::state::MARKET_CARD_STORE_KEY,
                Some(password),
            )
            .unwrap()
            .unwrap()
        };
        assert_eq!(raw_card_store, b"{corrupt-card-store");

        {
            let mut s = alice_state.service.lock().unwrap();
            crate::state::save_encrypted_bytes(
                &mut s,
                UI_PREFERENCES_KEY,
                b"{corrupt-settings",
                Some(password),
            )
            .unwrap();
        }
        alice_state.set_cached_ui_preferences(None);

        let remaining_id = active_voucher_id(&alice_state);
        let settings_err = create_transfer_bundle_impl(
            None,
            bob_did,
            vec![FrontendSourceTransfer {
                local_instance_id: remaining_id,
                amount_to_send: "10".into(),
            }],
            None,
            None,
            std_map,
            Some(false),
            Some(false),
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .expect_err("Corrupt settings must abort transfer");
        assert!(
            settings_err.message.contains("Failed to parse")
                && settings_err.message.contains(UI_PREFERENCES_KEY),
            "unexpected corrupt settings error: {:?}",
            settings_err
        );

        let raw_settings = {
            let mut s = alice_state.service.lock().unwrap();
            crate::state::load_encrypted_bytes_opt(&mut s, UI_PREFERENCES_KEY, Some(password))
                .unwrap()
                .unwrap()
        };
        assert_eq!(raw_settings, b"{corrupt-settings");
    }

    #[tokio::test]
    async fn k3_transfer_persists_outgoing_bundles_atomically_with_stripped_disk_history_and_hydrates(
    ) {
        let dir_guard = TempDirGuard(temp_test_dir("k3_outgoing_bundles_atomic"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));

        let (std_map, standard_toml) = std_map();
        let v1_id = issue_100_voucher(&alice_state, &standard_toml, password);
        let bob_did = wallet_did(&bob_state);

        let res = create_transfer_bundle_impl(
            None,
            bob_did,
            vec![FrontendSourceTransfer {
                local_instance_id: v1_id,
                amount_to_send: "25".into(),
            }],
            Some("K3 note".into()),
            Some("Alice".into()),
            std_map,
            Some(false),
            Some(false),
            None,
            Some(password.into()),
            &alice_state,
        )
        .await
        .expect("Transfer commit must persist L1 + outgoing_bundles + tx_history");

        {
            let mut s = alice_state.service.lock().unwrap();
            let raw_hist = alice_state.load_history_from_disk(&mut s, Some(password)).unwrap();
            let rec = raw_hist.iter().find(|r| r.bundle_id == res.bundle_id).expect("TransactionRecord missing");
            assert!(rec.bundle_data.is_empty());
            let stored_bytes =
                crate::state::get_outgoing_bundle_bytes(&mut s, &res.bundle_id, Some(password))
                    .unwrap()
                    .expect("Canonical bytes must exist in outgoing_bundles");
            assert_eq!(stored_bytes, res.bundle_data);
        }

        let hydrated = get_transaction_history_impl(&alice_state).unwrap();
        let h_rec = hydrated.iter().find(|r| r.bundle_id == res.bundle_id).unwrap();
        assert_eq!(h_rec.bundle_data, res.bundle_data);

        let dep = crate::services::post_service::deposit_transfer_to_post_impl(
            res.bundle_id,
            Some(password.into()),
            &alice_state,
        )
        .await
        .expect("deposit_transfer_to_post_impl must succeed using outgoing_bundles");
        assert!(!dep.msg_id.is_empty());
    }

    #[tokio::test]
    async fn f2_receive_bundle_online_verifies_anchored_green_replay_and_stale_token() {
        let dir_guard = TempDirGuard(temp_test_dir("f2_anchored_receive"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let bob_did = wallet_did(&bob_state);

        let (std_map, _) = std_map();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let sk1 = SigningKey::from_bytes(&[91u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let srv = spawn_mock_l2_confirm_server(l1, sk1.clone(), retain_q, 5u8);

        let anchor_id = register_single_server_anchor_and_voucher(
            &alice_state,
            p1,
            &sk1,
            &[89u8; 32],
            "50",
            password,
        );
        assert_ne!(anchor_id, [0u8; 32]);
        trust_l2_server_on_wallet(&bob_state, p1, &sk1, retain_q, password);

        let v_anchored_id = active_voucher_id(&alice_state);
        let tx_anchored =
            send_simple_transfer(&alice_state, &bob_did, &v_anchored_id, "20", &std_map, password).await;

        let green = receive_bundle_impl(
            None,
            tx_anchored.bundle_data.clone(),
            std_map.clone(),
            Some(password.into()),
            false,
            None,
            None,
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(green.newly_committed, Some(true));
        assert_eq!(green.is_replay, Some(false));
        assert_eq!(green.checkout_verdict.as_ref().unwrap().outcome, "Confirmed");
        assert!(green.checkout_verdict.as_ref().unwrap().is_goods_release_authorized);
        assert_eq!(green.l2_quorum.as_ref().unwrap().status, "final");

        let replay = receive_bundle_impl(
            None,
            tx_anchored.bundle_data.clone(),
            std_map,
            Some(password.into()),
            false,
            None,
            None,
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(replay.newly_committed, Some(false));
        assert_eq!(replay.is_replay, Some(true));

        let stale_token = crate::session::SessionToken {
            generation: bob_state.current_session_generation().saturating_sub(1),
            owner_did: bob_did,
        };
        let stale_err = crate::checkout::verify_bundle_l2_precommit(
            &bob_state,
            &stale_token,
            &tx_anchored.bundle_data,
            Some(password),
        )
        .await
        .expect_err("Stale SessionToken must be rejected");
        assert!(
            stale_err
                .message
                .contains("Wallet session changed before pre-commit L2 verification"),
            "unexpected stale session error: {:?}",
            stale_err
        );

        srv.abort();
    }

    #[tokio::test]
    async fn f2_receive_bundle_online_unanchored_two_phase_trust_commit() {
        let dir_guard = TempDirGuard(temp_test_dir("f2_unanchored_two_phase"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let bob_did = wallet_did(&bob_state);

        let (std_map, standard_toml) = std_map();
        let v_unanchored_id = issue_100_voucher(&alice_state, &standard_toml, password);
        let tx_offline =
            send_simple_transfer(&alice_state, &bob_did, &v_unanchored_id, "15", &std_map, password)
                .await;

        let pending = receive_bundle_impl(
            None,
            tx_offline.bundle_data.clone(),
            std_map.clone(),
            Some(password.into()),
            false,
            Some(ReceiveCommitMode::OnlineVerify),
            None,
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(pending.newly_committed, Some(false));
        assert_eq!(pending.is_replay, Some(false));
        let pending_verdict = pending.checkout_verdict.as_ref().unwrap();
        assert_eq!(pending_verdict.outcome, "OfflineOnly");
        assert!(!pending_verdict.is_goods_release_authorized);

        let committed = receive_bundle_impl(
            None,
            tx_offline.bundle_data,
            std_map,
            Some(password.into()),
            false,
            Some(ReceiveCommitMode::ForceTrustOffline),
            pending.purchase_id,
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(committed.newly_committed, Some(true));
        assert_eq!(committed.is_replay, Some(false));
        assert!(committed.checkout_verdict.as_ref().unwrap().is_goods_release_authorized);
    }

    #[tokio::test]
    async fn h1_one_shot_pin_grant_completes_transfer_and_post_deposit_in_single_call() {
        let dir_guard = TempDirGuard(temp_test_dir("h1_single_call_grant"));
        let keystore_dir = dir_guard.0.join("keystore");
        let password = "MasterPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));
        let (std_map, standard_toml) = std_map();
        let v_id = issue_100_voucher(&alice_state, &standard_toml, password);
        let bob_did = wallet_did(&bob_state);
        let folder = alice_state.service.lock().unwrap().list_profiles().unwrap()[0]
            .folder_name
            .clone();

        crate::commands::auth::enable_profile_pin_impl(
            password,
            "246810",
            "dailyPay",
            &folder,
            &keystore_dir,
            "device-drops-test",
            &alice_state,
        )
        .unwrap();
        alice_state.service.lock().unwrap().logout();
        crate::commands::auth::login_with_pin_impl(
            &folder,
            "246810",
            false,
            "device-drops-test".into(),
            &keystore_dir,
            "device-drops-test",
            &alice_state,
            None,
        )
        .unwrap();
        alice_state.service.lock().unwrap().lock_session();
        alice_state.set_cached_security_policy(Some(crate::settings::SecurityPolicy {
            session_timeout_seconds: 0,
            ..Default::default()
        }));

        let grant = crate::commands::auth::unlock_session_with_pin_impl(
            &folder,
            "246810",
            &keystore_dir,
            "device-drops-test",
            &alice_state,
        )
        .unwrap()
        .one_shot_grant
        .expect("timeout=0 PIN unlock must issue a one-shot grant");

        let res = create_transfer_bundle_impl(
            None,
            bob_did,
            vec![FrontendSourceTransfer {
                local_instance_id: v_id,
                amount_to_send: "20".into(),
            }],
            Some("H1 atomic".into()),
            Some("Alice".into()),
            std_map,
            Some(false),
            Some(false),
            Some(true),
            Some(grant.clone()),
            &alice_state,
        )
        .await
        .expect("H1: Single-call transfer + PoST outbox deposit must succeed with one-shot PIN grant");

        let dep = res
            .post_deposit
            .expect("post_deposit should be returned (outbox queued)");
        assert!(!dep.msg_id.is_empty());
        assert!(res.post_deposit_error.is_none());

        let reuse_err = alice_state
            .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, Some(&grant))
            .map(|_| ())
            .expect_err("Consumed one-shot PIN grant must reject any second command");
        assert!(
            reuse_err
                .message
                .contains("One-shot PIN grant missing or already used"),
            "unexpected grant reuse error: {:?}",
            reuse_err
        );
    }

    #[test]
    fn s06_f13_receive_payload_fails_closed_on_missing_voucher_and_sets_manifest_meta() {
        let dir_guard = TempDirGuard(temp_test_dir("s06_f13_receive_payload"));
        let pw = "pw-s06-f13";
        let (mut svc, _) =
            test_utils::setup_service_with_profile(&dir_guard.0, &test_utils::ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();
        let state = create_test_state(svc);

        let fake_result = human_money_core::wallet::types::ProcessBundleResult {
            involved_vouchers_details: vec![human_money_core::wallet::types::InvolvedVoucherInfo {
                local_instance_id: "missing-local-id".into(),
                voucher_id: "vid-missing".into(),
                standard_name: "FreeTaler".into(),
                unit: "FT".into(),
                amount: "10".into(),
                allow_partial_transfers: true,
                is_test_voucher: false,
                display_currency: "FT".into(),
                display_standard_name: "FreeTaler".into(),
                counterparty_id: None,
                counterparty_name: None,
            }],
            ..Default::default()
        };
        let mut service = state.lock_service().unwrap();
        let missing_err = build_receive_success_payload_from_result(
            &mut service,
            &state,
            b"{not-a-valid-bundle",
            fake_result,
            "p-1".into(),
            Some(pw),
        )
        .expect_err("S06-F13: missing committed vouchers must fail closed");
        assert!(
            missing_err
                .message
                .contains("Committed voucher 'missing-local-id' missing from wallet after receive"),
            "unexpected missing voucher error: {:?}",
            missing_err
        );

        let sk1 = SigningKey::from_bytes(&[81u8; 32]);
        let sk2 = SigningKey::from_bytes(&[82u8; 32]);
        let mig_sk = SigningKey::from_bytes(&[83u8; 32]);
        let (anchor_id, rev2_manifest) =
            build_rev2_two_server_manifest(&sk1, &sk2, &mig_sk, encode_quarter(2035, 4).unwrap());
        service.update_l2_anchor_manifest(rev2_manifest, Some(pw)).unwrap();

        let user_id = service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let (_, ft_toml) = std_map();
        let voucher = service
            .create_new_voucher(
                &ft_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(user_id.clone()),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "20".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let local_id =
            human_money_core::wallet::Wallet::calculate_local_instance_id(&voucher, &user_id).unwrap();

        let real_result = human_money_core::wallet::types::ProcessBundleResult {
            involved_vouchers_details: vec![human_money_core::wallet::types::InvolvedVoucherInfo {
                local_instance_id: local_id,
                voucher_id: voucher.voucher_id,
                standard_name: "FreeTaler".into(),
                unit: "FT".into(),
                amount: "20".into(),
                allow_partial_transfers: true,
                is_test_voucher: false,
                display_currency: "FT".into(),
                display_standard_name: "FreeTaler".into(),
                counterparty_id: None,
                counterparty_name: None,
            }],
            ..Default::default()
        };
        let payload = build_receive_success_payload_from_result(
            &mut service,
            &state,
            b"unused-when-committed-in-wallet",
            real_result,
            "p-2".into(),
            Some(pw),
        )
        .unwrap();
        let verdict = payload.checkout_verdict.expect("verdict must be present");
        assert_eq!(verdict.manifest_revision, Some(2));
    }

    #[tokio::test]
    async fn s04_q2_create_transfer_bundle_falls_back_offline_when_manifest_missing() {
        let dir_guard = TempDirGuard(temp_test_dir("s04_q2_send_missing_manifest"));
        let password = "TestPassword123!";
        let alice_state = create_test_state(setup_test_app(
            &dir_guard.0.join("alice"),
            "Alice",
            ALICE_PHRASE,
            password,
        ));
        let bob_state = create_test_state(setup_test_app(
            &dir_guard.0.join("bob"),
            "Bob",
            BOB_PHRASE,
            password,
        ));

        let (std_map, standard_toml) = std_map();
        let anchor_id = [0xD5u8; 32];
        let local_instance_id = {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_ui_preferences(&mut s, UiPreferences::default(), Some(password)).unwrap();
            alice_state.save_internal_lifecycle(&mut s, InternalLifecycleState::default(), Some(password)).unwrap();
            let user_id = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
            s.create_new_voucher(
                &standard_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(user_id),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "50".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
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
        };

        let bob_did = wallet_did(&bob_state);
        let res =
            send_simple_transfer(&alice_state, &bob_did, &local_instance_id, "20", &std_map, password)
                .await;
        assert!(res.l2_dispatch.is_empty());

        let recv = receive_bundle_impl(
            None,
            res.bundle_data,
            std_map,
            Some(password.into()),
            false,
            Some(ReceiveCommitMode::ForceTrustOffline),
            None,
            &bob_state,
        )
        .await
        .expect("Bob must accept offline-fallback bundle on trust basis");
        assert_eq!(recv.newly_committed, Some(true));
    }

    #[test]
    fn test_build_sent_transaction_record_aggregates_summable_and_countable_items() {
        let dir_guard = TempDirGuard(temp_test_dir("build_sent_record_units"));
        let pw = "pw-sent-record";
        let state = create_test_state(setup_test_app(
            &dir_guard.0,
            "Alice",
            ALICE_PHRASE,
            pw,
        ));
        let s = state.lock_service().unwrap();
        let mut summable_by_uuid = HashMap::new();
        let mut summable_by_name = HashMap::new();
        summable_by_name.insert("FreeTaler".to_string(), true);
        summable_by_name.insert("ArtworkVoucher".to_string(), false);

        let bundle_res = human_money_core::wallet::CreateBundleResult {
            bundle_bytes: vec![1, 2, 3],
            bundle_id: "bundle-rec-1".to_string(),
            involved_sources_details: vec![
                human_money_core::wallet::types::InvolvedVoucherInfo {
                    local_instance_id: "loc-1".into(),
                    voucher_id: "v-1".into(),
                    standard_name: "FreeTaler".into(),
                    unit: "Taler".into(),
                    amount: "10.50".into(),
                    allow_partial_transfers: true,
                    is_test_voucher: false,
                    display_currency: "Taler".into(),
                    display_standard_name: "FreeTaler".into(),
                    counterparty_id: None,
                    counterparty_name: None,
                },
                human_money_core::wallet::types::InvolvedVoucherInfo {
                    local_instance_id: "loc-2".into(),
                    voucher_id: "v-2".into(),
                    standard_name: "FreeTaler".into(),
                    unit: "Taler".into(),
                    amount: "4.25".into(),
                    allow_partial_transfers: true,
                    is_test_voucher: false,
                    display_currency: "Taler".into(),
                    display_standard_name: "FreeTaler".into(),
                    counterparty_id: None,
                    counterparty_name: None,
                },
                human_money_core::wallet::types::InvolvedVoucherInfo {
                    local_instance_id: "loc-3".into(),
                    voucher_id: "v-3".into(),
                    standard_name: "ArtworkVoucher".into(),
                    unit: "ItemUnit".into(),
                    amount: "1".into(),
                    allow_partial_transfers: false,
                    is_test_voucher: false,
                    display_currency: "ItemUnit".into(),
                    display_standard_name: "ArtworkVoucher".into(),
                    counterparty_id: None,
                    counterparty_name: None,
                },
            ],
            forensic_archive_incomplete: Vec::new(),
        };

        let sender_did = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let record = s
            .with_wallet(|wallet| {
                build_sent_transaction_record(
                    wallet,
                    sender_did.clone(),
                    test_utils::ACTORS.bob.user_id.clone(),
                    &bundle_res,
                    &summable_by_uuid,
                    &summable_by_name,
                    Some("Note 1".into()),
                    Some("Alice".into()),
                )
            })
            .unwrap();

        assert_eq!(record.direction, "sent");
        assert_eq!(record.sender_id, sender_did);
        assert_eq!(record.recipient_id, test_utils::ACTORS.bob.user_id);
        assert_eq!(record.bundle_id, "bundle-rec-1");
        assert_eq!(record.summable_amounts.get("Taler").map(String::as_str), Some("14.75"));
        assert_eq!(record.countable_items.get("ItemUnit").copied(), Some(1));
        assert_eq!(record.involved_vouchers, vec!["loc-1", "loc-2", "loc-3"]);
        assert_eq!(record.notes.as_deref(), Some("Note 1"));
        assert_eq!(record.sender_profile_name.as_deref(), Some("Alice"));
        summable_by_uuid.clear();
    }
}
