use super::trusted_keys::{build_receive_payload_and_record, resolve_trusted_lock_keys};
use super::*;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS, manifest::AnchorManifest, types::LockPubKey,
};
use human_money_core::models::layer2_api::{CheckoutReleaseVerdict, L2ProcessingOutcome};
use human_money_core::services::{bundle_processor, l2_gateway};

use crate::{models::FrontendError, settings::InternalLifecycleState, sync_client, AppState};

/// Sync-Lock committee checkout orchestration.
///
/// - `N = 0` (`layer2_anchor == None`): Returns `OfflineOnly` verdict requiring manual trust approval.
/// - `N >= 1`: Prepares Leap-Lock `m..=k` `SyncLockBatchRequest` payloads, fans them out in parallel
///   across the voucher's `AnchorManifest` committee, records any returned `ConflictSeen` evidence,
///   and evaluates Green / Yellow / Red via `l2_gateway::evaluate_bundle_checkout`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn checkout_bundle(
    state: &AppState,
    session: &crate::session::SessionToken,
    purchase_id: &str,
    bundle: &[u8],
    standards: &HashMap<String, String>,
    force_accept_tolerance_bundle: bool,
    password: Option<&str>,
) -> Result<PurchaseCompletion, FrontendError> {
    // 1. Idempotent replay check before any network I/O
    {
        let mut service = state.lock_service()?;
        if !state.is_session_current(session, &service) {
            return Err(fail("Wallet session changed before checkout"));
        }
        if let Some(completion) = service
            .replay_checkout_purchase(purchase_id, bundle)
            .map_err(FrontendError::from)?
        {
            let mut verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                completion.verdict.into();
            if let Ok(Ok(b)) = service.with_wallet_and_identity(|_, id| {
                bundle_processor::open_and_verify_bundle(id, bundle)
            }) {
                if let Some(aid) = b.vouchers.first().and_then(|v| v.layer2_anchor) {
                    let ahex = hex::encode(aid);
                    let _ = service.with_wallet(|w| {
                        if let Some(m) = w.profile.l2_anchor_manifests.get(&ahex) {
                            verdict_fe.reduced_guarantor_protection =
                                m.has_reduced_guarantor_protection().unwrap_or(false);
                            verdict_fe.manifest_revision = Some(m.revision);
                        }
                    });
                }
            }

            let replay_l2_quorum = if verdict_fe.outcome == "Confirmed" {
                service
                    .with_wallet_and_identity(|w, id| {
                        let b = bundle_processor::open_and_verify_bundle(id, bundle).ok()?;
                        let aid = b.vouchers.first()?.layer2_anchor?;
                        let manifest = w.profile.l2_anchor_manifests.get(&hex::encode(aid))?;
                        Some(crate::models::FrontendL2QuorumDto {
                            status: "final".to_string(),
                            signer_count: Some(manifest.threshold as usize),
                            active_nodes_count: Some(manifest.servers.len()),
                            details: None,
                        })
                    })
                    .ok()
                    .flatten()
            } else {
                None
            };

            let (receive_payload, replay_record_opt) = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                false,
                true,
                verdict_fe.clone(),
                replay_l2_quorum,
                password,
            )
            .map(|(payload, rec)| (Some(payload), rec))
            .unwrap_or((None, None));
            if let Some(rec) = replay_record_opt {
                let history = state
                    .load_history_from_disk(&mut service, password)
                    .map_err(FrontendError::from)?;
                let already_recorded = !rec.bundle_id.is_empty()
                    && history
                        .iter()
                        .any(|r| r.bundle_id == rec.bundle_id && r.direction == rec.direction);
                if !already_recorded {
                    state
                        .append_to_history(&mut service, rec, password)
                        .map_err(FrontendError::from)?;
                }
            }

            return Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                is_replay: true,
                verdict: verdict_fe,
                receive_payload,
            });
        }
    }

    // 2. Decrypt & inspect bundle to check replay history and N = 0 offline-only vs anchored vouchers
    let (opened_bundle, merchant_id, already_in_history) = state.with_session_required(
        session,
        "Wallet session changed during checkout",
        |service| {
            service
                .with_wallet_and_identity(|wallet, identity| {
                    let parsed = bundle_processor::open_and_verify_bundle(identity, bundle)?;
                    let in_history = wallet
                        .bundle_meta_store
                        .history
                        .contains_key(&parsed.bundle_id);
                    Ok::<_, human_money_core::Error>((
                        parsed,
                        wallet.get_user_id().to_string(),
                        in_history,
                    ))
                })
                .map_err(FrontendError::from)?
                .map_err(FrontendError::from)
        },
    )?;

    if already_in_history {
        let (verdict_fe, receive_payload) = state.with_session_required(
            session,
            "Wallet session changed during checkout",
            |service| {
                let any_quarantined = service
                    .with_wallet(|wallet| {
                        opened_bundle.vouchers.iter().any(|v| {
                            human_money_core::wallet::Wallet::calculate_local_instance_id(
                                v,
                                &merchant_id,
                            )
                            .ok()
                            .and_then(|lid| wallet.voucher_store.vouchers.get(&lid))
                            .is_some_and(|inst| {
                                matches!(
                                    inst.status,
                                    human_money_core::VoucherStatus::Quarantined { .. }
                                )
                            })
                        })
                    })
                    .map_err(FrontendError::from)?;
                let (conflict_summaries, _) =
                    crate::services::transfer_service::filter_conflicts_for_received_vouchers(
                        service,
                        &opened_bundle.vouchers,
                    )?;
                let replay_outcome = if any_quarantined || !conflict_summaries.is_empty() {
                    L2ProcessingOutcome::Quarantined {
                        reason: "BundleAlreadyProcessedQuarantined".to_string(),
                    }
                } else if opened_bundle
                    .vouchers
                    .iter()
                    .all(|v| v.layer2_anchor.is_none())
                {
                    L2ProcessingOutcome::OfflineOnly
                } else {
                    L2ProcessingOutcome::Yellow {
                        reason: "BundleAlreadyProcessedReplay".to_string(),
                    }
                };
                let verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                    CheckoutReleaseVerdict::new(replay_outcome, false).into();
                let receive_payload = build_receive_payload_and_record(
                    state,
                    service,
                    purchase_id,
                    bundle,
                    standards,
                    false,
                    true,
                    verdict_fe.clone(),
                    None,
                    password,
                )
                .map(|(p, _)| p)
                .ok();
                Ok((verdict_fe, receive_payload))
            },
        )?;
        return Ok(PurchaseCompletion {
            purchase_id: purchase_id.to_string(),
            newly_committed: false,
            is_replay: true,
            verdict: verdict_fe,
            receive_payload,
        });
    }

    if opened_bundle.vouchers.is_empty() {
        return Err(fail("Transaction bundle contains no vouchers"));
    }
    if opened_bundle
        .vouchers
        .iter()
        .all(|v| v.layer2_anchor.is_none())
    {
        let verdict_core = CheckoutReleaseVerdict::new(L2ProcessingOutcome::OfflineOnly, false);
        let verdict_fe: crate::models::FrontendCheckoutReleaseVerdict = verdict_core.into();
        let receive_payload = state.with_session_required(
            session,
            "Wallet session changed during checkout",
            |service| {
                Ok(build_receive_payload_and_record(
                    state,
                    service,
                    purchase_id,
                    bundle,
                    standards,
                    false,
                    false,
                    verdict_fe.clone(),
                    None,
                    password,
                )
                .map(|(p, _)| p)
                .ok())
            },
        )?;
        return Ok(PurchaseCompletion {
            purchase_id: purchase_id.to_string(),
            newly_committed: false,
            is_replay: false,
            verdict: verdict_fe,
            receive_payload,
        });
    }

    if opened_bundle.vouchers.len() != 1 {
        let verdict_core = CheckoutReleaseVerdict::new(
            L2ProcessingOutcome::Yellow {
                reason: "MultiVoucherManualApproval".to_string(),
            },
            false,
        );
        let verdict_fe: crate::models::FrontendCheckoutReleaseVerdict = verdict_core.into();
        let receive_payload = state.with_session_required(
            session,
            "Wallet session changed during checkout",
            |service| {
                Ok(build_receive_payload_and_record(
                    state,
                    service,
                    purchase_id,
                    bundle,
                    standards,
                    false,
                    false,
                    verdict_fe.clone(),
                    None,
                    password,
                )
                .map(|(p, _)| p)
                .ok())
            },
        )?;
        return Ok(PurchaseCompletion {
            purchase_id: purchase_id.to_string(),
            newly_committed: false,
            is_replay: false,
            verdict: verdict_fe,
            receive_payload,
        });
    }
    let voucher = opened_bundle.vouchers[0].clone();
    let anchor_id = voucher
        .layer2_anchor
        .ok_or_else(|| fail("Anchored voucher missing layer2_anchor"))?;
    let guarantor_dids: Vec<String> = voucher
        .signatures
        .iter()
        .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
        .map(|s| s.signer_id.clone())
        .collect();
    let (security_policy, mut active_lifecycle) = state.with_session_required(
        session,
        "Wallet session changed during checkout",
        |service| {
            let sp = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            let lc = state
                .get_or_load_internal_lifecycle(service, password)
                .map_err(FrontendError::from)?;
            Ok((sp, lc))
        },
    )?;
    let client = sync_client::create_sync_http_client();
    let mut pow_base_bits = opened_bundle
        .anchor_manifest_for(&anchor_id)
        .map(|m| {
            crate::services::common::max_pow_base_bits_for_servers(
                &active_lifecycle,
                m.servers.iter().map(|s| s.lock_pub),
            )
        })
        .unwrap_or(DEFAULT_LOCK_POW_BASE_BITS);

    // 3. Prepare durable challenge context + per-server SyncLockBatchRequests (running Discovery if local manifest missing).
    // Fail closed immediately if the local anti-rollback sidecar is corrupted BEFORE any discovery fallback.
    let first_prepare = state.with_session_required(
        session,
        "Wallet session changed during checkout",
        |service| {
            let known_heads =
                crate::state::load_known_head_receipts_for_anchor(service, &anchor_id, password)?;
            Ok(service.prepare_checkout_requests(
                purchase_id,
                bundle,
                pow_base_bits,
                now()?,
                &known_heads,
                password,
            ))
        },
    )?;
    let (mut context, mut requests) = match first_prepare {
        Ok(pair) => pair,
        Err(_) => {
            let discovered = crate::l2_lifecycle::discover_anchor_manifest(
                state,
                session,
                &client,
                &anchor_id,
                voucher.creator_profile.id.as_deref(),
                &guarantor_dids,
                password,
            )
            .await?;
            if let Some(ref m) = discovered {
                pow_base_bits =
                    pow_base_bits.max(crate::services::common::max_pow_base_bits_for_servers(
                        &active_lifecycle,
                        m.servers.iter().map(|s| s.lock_pub),
                    ));
            }
            let anchor_hex = hex::encode(anchor_id);
            let retry_res = state.with_session_required(
                session,
                "Wallet changed during checkout",
                |service| {
                    let known_heads = crate::state::load_known_head_receipts_for_anchor(
                        service, &anchor_id, password,
                    )?;
                    let has_manifest = service
                        .with_wallet(|w| w.profile.l2_anchor_manifests.contains_key(&anchor_hex))
                        .unwrap_or(false)
                        || opened_bundle.anchor_manifest_for(&anchor_id).is_some();
                    match service.prepare_checkout_requests(
                        purchase_id,
                        bundle,
                        pow_base_bits,
                        now()?,
                        &known_heads,
                        password,
                    ) {
                        Ok(pair) => Ok(Ok(pair)),
                        Err(e) if !has_manifest => Ok(Err(e)),
                        Err(e) => Err(FrontendError::from(e)),
                    }
                },
            )?;
            match retry_res {
                Ok(pair) => pair,
                Err(prep_err) => {
                    log::warn!(
                        "Checkout manifest unavailable after discovery (offline/unreachable): {}",
                        prep_err
                    );
                    let verdict_core = CheckoutReleaseVerdict::new(
                        L2ProcessingOutcome::Yellow {
                            reason: "Protection committee manifest unavailable (offline/unreachable); manual trust approval required".to_string(),
                        },
                        false,
                    );
                    let verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                        verdict_core.into();
                    let receive_payload = state.with_session_required(
                        session,
                        "Wallet session changed during checkout",
                        |service| {
                            Ok(build_receive_payload_and_record(
                                state,
                                service,
                                purchase_id,
                                bundle,
                                standards,
                                false,
                                false,
                                verdict_fe.clone(),
                                None,
                                password,
                            )
                            .map(|(p, _)| p)
                            .ok())
                        },
                    )?;
                    return Ok(PurchaseCompletion {
                        purchase_id: purchase_id.to_string(),
                        newly_committed: false,
                        is_replay: false,
                        verdict: verdict_fe,
                        receive_payload,
                    });
                }
            }
        }
    };

    // 4. Resolve trusted LockPubKeys & fan out committee batches in parallel
    let (mut base_trusted_keys, mut independently_pinned_creator_keys) = resolve_trusted_lock_keys(
        state,
        session,
        &security_policy,
        &active_lifecycle,
        &client,
        voucher.creator_profile.id.as_deref(),
        Some(&context.manifest),
        password,
    )
    .await?;

    let mut dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
        &client,
        &requests,
        CHECKOUT_BATCH_TIMEOUT,
        &active_lifecycle.l2_probe_cache,
    )
    .await;
    let mut all_competing_entries = dispatch.competing_entries.clone();
    let mut all_receipts_for_equiv = dispatch.receipts.clone();
    let mut all_equiv_proofs = dispatch.equivocation_proofs.clone();
    for qc in &opened_bundle.quorum_certificates {
        all_receipts_for_equiv.extend(qc.receipts.iter().cloned());
    }

    // 4b. Handle `409 epoch_frozen`, `410 revoked`, or total NXDOMAIN committee failure via Discovery Cascade
    let upgrade = resolve_manifest_upgrade_after_dispatch(
        state,
        session,
        &client,
        &anchor_id,
        voucher.creator_profile.id.as_deref(),
        &guarantor_dids,
        &active_lifecycle,
        &context.manifest,
        &dispatch,
        password,
    )
    .await?;
    if upgrade.upgraded_manifest.is_some() || upgrade.relocated {
        let upgraded = upgrade.upgraded_manifest.is_some();
        let fresh_lifecycle =
            state.with_session_required(session, "Wallet changed during checkout", |service| {
                if let Some((newer_manifest, upgraded_pow_bits)) = upgrade.upgraded_manifest {
                    pow_base_bits = pow_base_bits.max(upgraded_pow_bits);
                    service
                        .update_l2_anchor_manifest(newer_manifest, password)
                        .map_err(FrontendError::from)?;
                    let known_heads = crate::state::load_known_head_receipts_for_anchor(
                        service, &anchor_id, password,
                    )?;
                    let pair = service
                        .prepare_checkout_requests(
                            purchase_id,
                            bundle,
                            pow_base_bits,
                            now()?,
                            &known_heads,
                            password,
                        )
                        .map_err(FrontendError::from)?;
                    context = pair.0;
                    requests = pair.1;
                }
                state
                    .get_or_load_internal_lifecycle(service, password)
                    .map_err(FrontendError::from)
            })?;
        active_lifecycle = fresh_lifecycle;
        let keys = resolve_trusted_lock_keys(
            state,
            session,
            &security_policy,
            &active_lifecycle,
            &client,
            voucher.creator_profile.id.as_deref(),
            Some(&context.manifest),
            password,
        )
        .await?;
        base_trusted_keys = keys.0;
        independently_pinned_creator_keys = keys.1;
        if upgraded {
            let dispatch2 = crate::gateway_resolver::dispatch_lock_committee_batches(
                &client,
                &requests,
                CHECKOUT_BATCH_TIMEOUT,
                &active_lifecycle.l2_probe_cache,
            )
            .await;
            all_competing_entries.extend(dispatch2.competing_entries.iter().cloned());
            all_receipts_for_equiv.extend(dispatch2.receipts.iter().cloned());
            all_equiv_proofs.extend(dispatch2.equivocation_proofs.iter().cloned());
            dispatch = dispatch2;
        } else {
            let pending_retry_pubs: HashSet<LockPubKey> = dispatch
                .pending_retries
                .iter()
                .map(|(s, _)| s.lock_pub)
                .collect();
            let retry_requests: Vec<_> = requests
                .iter()
                .filter(|(s, _)| {
                    upgrade.relocated_lock_pubs.contains(&s.lock_pub)
                        || pending_retry_pubs.contains(&s.lock_pub)
                })
                .cloned()
                .collect();
            if !retry_requests.is_empty() {
                let dispatch2 = crate::gateway_resolver::dispatch_lock_committee_batches(
                    &client,
                    &retry_requests,
                    CHECKOUT_BATCH_TIMEOUT,
                    &active_lifecycle.l2_probe_cache,
                )
                .await;
                all_competing_entries.extend(dispatch2.competing_entries.iter().cloned());
                all_receipts_for_equiv.extend(dispatch2.receipts.iter().cloned());
                all_equiv_proofs.extend(dispatch2.equivocation_proofs.iter().cloned());
                merge_receipts_dedup(&mut dispatch.receipts, dispatch2.receipts);
                dispatch.pending_retries = dispatch2.pending_retries;
            }
        }
    }

    if let Some(equiv_proof) = l2_gateway::detect_equivocation(&all_receipts_for_equiv) {
        all_equiv_proofs.push(equiv_proof);
    }
    if !all_equiv_proofs.is_empty() {
        let updated_offenders = state
            .with_session(session, |service| {
                let now_secs = sync_client::current_unix_timestamp().max(1);
                for equiv_proof in &all_equiv_proofs {
                    if let Err(e) = crate::l2_lifecycle::record_equivocation_and_refresh_cache(
                        state,
                        service,
                        &active_lifecycle.l2_probe_cache,
                        equiv_proof,
                        now_secs,
                        password,
                    ) {
                        log::warn!("Failed to record server equivocation proof: {}", e.message);
                    }
                }
                state.get_or_load_server_offender_list(service, password)
            })
            .ok()
            .flatten()
            .unwrap_or_default();
        let offending_locks: HashSet<LockPubKey> = all_equiv_proofs
            .iter()
            .map(|p| p.receipt_a.lock_pub)
            .collect();
        let admin_by_lock: HashMap<LockPubKey, [u8; 32]> = context
            .manifest
            .servers
            .iter()
            .map(|s| (s.lock_pub, s.admin_pub))
            .collect();
        let keep_key = |pk: &LockPubKey| {
            !offending_locks.contains(pk)
                && !crate::l2_lifecycle::is_server_key_revoked(
                    &updated_offenders,
                    pk,
                    admin_by_lock.get(pk),
                )
        };
        base_trusted_keys.retain(keep_key);
        independently_pinned_creator_keys.retain(keep_key);
    }

    // 5. Evaluate outcome under Green-Rule & LÜCKE-03 isolation
    let outcome = l2_gateway::evaluate_bundle_checkout(
        &voucher,
        &context.manifest,
        &context.target_slot,
        &context.target_child_lock,
        &context.recipient_nonce,
        &dispatch.receipts,
        &merchant_id,
        &base_trusted_keys,
        &independently_pinned_creator_keys,
    );

    let reduced_guarantor_protection = context
        .manifest
        .has_reduced_guarantor_protection()
        .unwrap_or(false);
    let manifest_revision = Some(context.manifest.revision);

    let mut service = state.lock_service()?;
    if !state.is_session_current(session, &service) {
        return Err(fail("Wallet changed during checkout"));
    }

    for competing_entry in &all_competing_entries {
        if let Err(err) = service.record_checkout_upstream_conflict(
            bundle,
            competing_entry,
            DEFAULT_LOCK_POW_BASE_BITS,
            password,
        ) {
            log::warn!("Recording upstream conflict entry failed: {}", err);
        }
    }

    crate::state::record_known_head_receipts(
        &mut service,
        &anchor_id,
        &all_receipts_for_equiv,
        password,
    )?;

    match outcome {
        L2ProcessingOutcome::Confirmed {
            ref quorum_certificates,
        } => {
            let signer_count = quorum_certificates
                .last()
                .map(|qc| qc.receipts.len())
                .or(Some(context.manifest.threshold as usize));
            let active_nodes_count = Some(context.manifest.servers.len());
            let l2_quorum = Some(crate::models::FrontendL2QuorumDto {
                status: "final".to_string(),
                signer_count,
                active_nodes_count,
                details: None,
            });

            let baseline_history = state
                .load_history_from_disk(&mut service, password)
                .map_err(FrontendError::from)?;
            let next_history_cell =
                std::cell::RefCell::new(None::<Vec<crate::models::TransactionRecord>>);
            let completion = service
                .complete_checkout_purchase_with_companions(
                    purchase_id,
                    bundle,
                    &dispatch.receipts,
                    &base_trusted_keys,
                    &independently_pinned_creator_keys,
                    standards,
                    now()?,
                    force_accept_tolerance_bundle,
                    password,
                    |res, wallet| {
                        let involved_vouchers_ids: Vec<String> = res
                            .involved_vouchers_details
                            .iter()
                            .map(|d| d.local_instance_id.clone())
                            .collect();
                        let record = crate::models::TransactionRecord {
                            id: uuid::Uuid::new_v4().to_string(),
                            direction: "received".to_string(),
                            recipient_id: wallet.get_user_id().to_string(),
                            sender_id: res.header.sender_id.clone(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            summable_amounts: res.transfer_summary.summable_amounts.clone(),
                            countable_items: res.transfer_summary.countable_items.clone(),
                            involved_vouchers: if involved_vouchers_ids.is_empty() {
                                res.involved_vouchers.clone()
                            } else {
                                involved_vouchers_ids
                            },
                            involved_sources_details: Some(
                                res.involved_vouchers_details
                                    .iter()
                                    .cloned()
                                    .map(Into::into)
                                    .collect(),
                            ),
                            bundle_data: Vec::new(),
                            bundle_id: res.header.bundle_id.clone(),
                            notes: res
                                .header
                                .notes
                                .as_deref()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_string),
                            sender_profile_name: res.header.sender_profile_name.clone(),
                        };
                        let (blobs, next_hist) = crate::state::prepare_receive_history_companion(
                            &baseline_history,
                            record,
                        )
                        .map_err(|e| {
                            human_money_core::Error::Storage(
                                human_money_core::StorageError::InvalidFormat(e.message),
                            )
                        })?;
                        *next_history_cell.borrow_mut() = Some(next_hist);
                        Ok(blobs)
                    },
                )
                .map_err(FrontendError::from)?;

            if let Some(next_hist) = next_history_cell.into_inner() {
                state.write_cache_slot_for_service(&service, |c| &mut c.history, Some(next_hist));
            }
            if let Err(error) = state.refresh_events_cache(&mut service, password) {
                log::warn!("Checkout committed; event cache refresh failed: {}", error);
            }

            let mut verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                completion.verdict.into();
            verdict_fe.reduced_guarantor_protection = reduced_guarantor_protection;
            verdict_fe.manifest_revision = manifest_revision;

            let is_replay = !completion.newly_committed;
            let (receive_payload, _) = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                completion.newly_committed,
                is_replay,
                verdict_fe.clone(),
                l2_quorum,
                password,
            )?;

            // Catch-up background completion for any committee servers that timed out during Green checkout
            if !dispatch.pending_retries.is_empty() {
                let retries = dispatch.pending_retries;
                let retry_client = client.clone();
                let retry_probe_cache = active_lifecycle.l2_probe_cache.clone();
                tokio::spawn(async move {
                    let _ = crate::gateway_resolver::dispatch_lock_committee_batches(
                        &retry_client,
                        &retries,
                        Duration::from_secs(4),
                        &retry_probe_cache,
                    )
                    .await;
                });
            }

            drop(service);

            Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                is_replay,
                verdict: verdict_fe,
                receive_payload: Some(receive_payload),
            })
        }
        L2ProcessingOutcome::Yellow { reason } => {
            let mut verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                CheckoutReleaseVerdict::new(L2ProcessingOutcome::Yellow { reason }, false).into();
            verdict_fe.reduced_guarantor_protection = reduced_guarantor_protection;
            verdict_fe.manifest_revision = manifest_revision;
            let receive_payload = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                false,
                false,
                verdict_fe.clone(),
                None,
                password,
            )
            .map(|(p, _)| p)
            .ok();
            Ok(PurchaseCompletion {
                purchase_id: purchase_id.to_string(),
                newly_committed: false,
                is_replay: false,
                verdict: verdict_fe,
                receive_payload,
            })
        }
        L2ProcessingOutcome::Quarantined { reason } => {
            let mut verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                CheckoutReleaseVerdict::new(L2ProcessingOutcome::Quarantined { reason }, false)
                    .into();
            verdict_fe.reduced_guarantor_protection = reduced_guarantor_protection;
            verdict_fe.manifest_revision = manifest_revision;
            let receive_payload = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                false,
                false,
                verdict_fe.clone(),
                None,
                password,
            )
            .map(|(p, _)| p)
            .ok();
            Ok(PurchaseCompletion {
                purchase_id: purchase_id.to_string(),
                newly_committed: false,
                is_replay: false,
                verdict: verdict_fe,
                receive_payload,
            })
        }
        L2ProcessingOutcome::OfflineOnly => unreachable!("handled prior to anchored prepare"),
    }
}

pub(crate) struct DispatchUpgradeResult {
    pub upgraded_manifest: Option<(AnchorManifest, u32)>,
    pub relocated: bool,
    pub relocated_lock_pubs: HashSet<LockPubKey>,
}

/// Records `410 revoked` notices and resolves any verified newer `AnchorManifest` from
/// `409 epoch_frozen` responses or manifest discovery (H7/H10 shared path for checkout, PoST & intents).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_manifest_upgrade_after_dispatch(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    anchor_id: &[u8; 32],
    creator_id: Option<&str>,
    guarantor_dids: &[String],
    lifecycle: &InternalLifecycleState,
    current_manifest: &AnchorManifest,
    dispatch: &crate::gateway_resolver::CommitteeDispatchResult,
    password: Option<&str>,
) -> Result<DispatchUpgradeResult, FrontendError> {
    let now_secs = sync_client::current_unix_timestamp();
    let mut relocated = false;
    let mut relocated_lock_pubs: HashSet<LockPubKey> = HashSet::new();
    if !dispatch.revoked_notices.is_empty() {
        let mut scratch_cache = lifecycle.l2_probe_cache.clone();
        let mut probe_cache_deltas = Vec::new();
        for (lock_pub, notice) in &dispatch.revoked_notices {
            if notice.replacement.is_some() {
                if let Some(expected_srv) = current_manifest
                    .servers
                    .iter()
                    .find(|s| s.lock_pub == *lock_pub)
                {
                    if let Some(probe) = crate::l2_lifecycle::try_follow_relocation_notice(
                        client,
                        &mut scratch_cache,
                        expected_srv,
                        notice,
                    )
                    .await
                    {
                        relocated_lock_pubs.insert(*lock_pub);
                        probe_cache_deltas.push(probe);
                    }
                }
            }
        }
        if !probe_cache_deltas.is_empty() {
            relocated = true;
            let _ = state.mutate_internal_lifecycle_for_session(session, password, |lc| {
                let mut changed = false;
                for probe in probe_cache_deltas {
                    changed |= crate::services::common::upsert_verified_lock_probe(
                        &mut lc.l2_probe_cache,
                        probe,
                    );
                }
                changed
            });
        }
        let _ = state.with_session(session, |service| {
            let _ = crate::l2_lifecycle::record_revoked_servers(
                service,
                anchor_id,
                &dispatch.revoked_notices,
                &current_manifest.servers,
                &relocated_lock_pubs,
                now_secs,
                password,
            );
            Ok(())
        });
    }

    let mut best_upgraded: Option<AnchorManifest> = None;
    for m in &dispatch.epoch_frozen_manifests {
        if m.revision > current_manifest.revision
            && m.verify_anchor_id(anchor_id).is_ok()
            && best_upgraded
                .as_ref()
                .map(|b| m.revision > b.revision)
                .unwrap_or(true)
        {
            best_upgraded = Some(m.clone());
        }
    }

    let all_unreachable = dispatch.receipts.is_empty()
        && dispatch.competing_entries.is_empty()
        && (!dispatch.transport_errors.is_empty() || !dispatch.rejected_errors.is_empty());
    if best_upgraded.is_none()
        && (all_unreachable
            || dispatch.had_epoch_frozen
            || !dispatch.epoch_frozen_manifests.is_empty())
    {
        if let Ok(Some(discovered)) = crate::l2_lifecycle::discover_anchor_manifest(
            state,
            session,
            client,
            anchor_id,
            creator_id,
            guarantor_dids,
            password,
        )
        .await
        {
            if discovered.revision > current_manifest.revision
                && discovered.verify_anchor_id(anchor_id).is_ok()
            {
                best_upgraded = Some(discovered);
            }
        }
    }

    Ok(DispatchUpgradeResult {
        upgraded_manifest: best_upgraded.map(|m| {
            let pow_bits = crate::services::common::max_pow_base_bits_for_servers(
                lifecycle,
                m.servers.iter().map(|s| s.lock_pub),
            );
            (m, pow_bits)
        }),
        relocated,
        relocated_lock_pubs,
    })
}
