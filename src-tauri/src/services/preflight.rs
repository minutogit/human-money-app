use std::collections::{HashMap, HashSet};
use std::time::Duration;

use human_money_core::app_service::l2_intents::L2SendIntent;
use human_money_core::app_service::AppService;
use human_money_core::humoco_proto::{
    causal::LockEntry,
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    manifest::AnchorManifest,
    types::{LockPubKey, LockReceiptKind},
};
use rand::RngCore;

use crate::gateway_resolver::{
    dispatch_lock_committee_batches, summarize_intent_dispatch, IntentDispatchObservation,
};
use crate::l2_lifecycle::{is_server_key_revoked, record_equivocation_and_refresh_cache};
use crate::models::FrontendError;
use crate::services::common::ensure_restore_l2_reconciled;
use crate::state::AppState;

/// Unified pre-flight checks enforced across value-outflow, claim, swap, issuance, and endorsement paths.
pub struct ValueOutflowPreflight<'a> {
    pub require_restore_reconciled: bool,
    pub check_known_offender_dids: &'a [&'a str],
    pub check_blocking_intent_local_ids: &'a [&'a str],
    pub check_anchor_id_quorum_health: Option<[u8; 32]>,
    pub preverified_anchor_manifest: Option<&'a AnchorManifest>,
}

/// Generates a fresh 32-byte non-zero Sync Lock request nonce.
pub(crate) fn random_non_zero_nonce() -> [u8; 32] {
    let mut nonce = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    if nonce == [0u8; 32] {
        nonce[0] = 1;
    }
    nonce
}

/// Runs configured pre-flight safety checks synchronously under the locked `AppService`.
pub fn run_value_outflow_preflight(
    state: &AppState,
    service: &mut AppService,
    password: Option<&str>,
    req: &ValueOutflowPreflight<'_>,
) -> Result<(), FrontendError> {
    if req.require_restore_reconciled {
        ensure_restore_l2_reconciled(state, service, password)?;
    }

    if req.require_restore_reconciled
        || !req.check_known_offender_dids.is_empty()
        || !req.check_blocking_intent_local_ids.is_empty()
    {
        service
            .with_wallet(|wallet| -> Result<(), FrontendError> {
                wallet
                    .assert_not_known_offender(wallet.get_user_id())
                    .map_err(FrontendError::from)?;
                for did in req.check_known_offender_dids {
                    wallet
                        .assert_not_known_offender(did)
                        .map_err(FrontendError::from)?;
                }
                for local_id in req.check_blocking_intent_local_ids {
                    wallet
                        .assert_no_blocking_l2_intent(local_id)
                        .map_err(FrontendError::from)?;
                }
                Ok(())
            })
            .map_err(FrontendError::from)??;
    }

    if let Some(anchor_id) = req.check_anchor_id_quorum_health {
        let anchor_hex = hex::encode(anchor_id);
        let manifest = if let Some(preverified) = req.preverified_anchor_manifest {
            preverified.clone()
        } else {
            service
                .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
                .map_err(FrontendError::from)?
                .ok_or_else(|| {
                    FrontendError::with_code(
                        "error.l2.anchorManifestMissing",
                        format!("Anchor manifest not found for {anchor_hex}"),
                    )
                })?
        };
        manifest
            .verify_anchor_id(&anchor_id)
            .map_err(|e| FrontendError::from(format!("Invalid anchor manifest: {e}")))?;
        let committee = manifest
            .committee()
            .map_err(|e| FrontendError::from(format!("Invalid anchor committee: {e}")))?;

        let offenders = state.get_or_load_server_offender_list(service, password)?;
        let healthy_servers = manifest
            .servers
            .iter()
            .filter(|s| !is_server_key_revoked(&offenders, &s.lock_pub, Some(&s.admin_pub)))
            .count();
        let threshold = usize::from(committee.threshold);
        if healthy_servers < threshold {
            return Err(FrontendError::with_code(
                "error.l2.anchorQuorumUnhealthy",
                format!(
                    "Anchor quorum has only {healthy_servers}/{} non-banned servers (threshold {threshold})",
                    manifest.servers.len()
                ),
            ));
        }
    }

    Ok(())
}

/// Dispatches staged durable `L2SendIntent`s to their committee servers, handles `409 epoch_frozen`
/// manifest upgrades and `410 revoked` relocations via the shared upgrade path, verifies competing
/// entries resiliently, persists server equivocation proofs with delegation cache lookups, and
/// returns honest dispatch observations.
pub async fn dispatch_l2_send_intents(
    state: &AppState,
    session: &crate::session::SessionToken,
    intents: &[L2SendIntent],
    pow_base_bits: u32,
    password: Option<&str>,
) -> Result<Vec<IntentDispatchObservation>, FrontendError> {
    if intents.is_empty() {
        return Ok(Vec::new());
    }

    let client = crate::sync_client::create_sync_http_client();
    let now_ms = u64::try_from(chrono::Utc::now().timestamp_millis())
        .map_err(|_| FrontendError::from("Invalid verification time".to_string()))?;
    let now_secs = now_ms / 1000;
    let mut l2_dispatch = Vec::with_capacity(intents.len());

    for intent in intents {
        let local_id = &intent.local_instance_id;
        let (mut active_nonce, reqs_res, mut lifecycle, creator_id, guarantor_dids) = state
            .with_session_required(
                session,
                "Wallet session changed during L2 dispatch",
                |service| {
                    let nonce = random_non_zero_nonce();
                    let known_heads = crate::state::load_known_head_receipts_for_anchor(
                        service,
                        &intent.anchor_id,
                        password,
                    )?;
                    let lifecycle = match state.get_or_load_internal_lifecycle(service, password) {
                        Ok(lc) => lc,
                        Err(err) => {
                            log::warn!(
                                "Failed to load internal lifecycle probe cache before L2 intent dispatch ({}): {}",
                                local_id,
                                err
                            );
                            Default::default()
                        }
                    };
                    let reqs_res = service
                        .build_intent_committee_requests(local_id, &nonce, &known_heads, true)
                        .map_err(FrontendError::from);
                    let (creator_id, guarantor_dids) = service
                        .with_wallet(|w| {
                            w.get_voucher_instance(local_id)
                                .map(|inst| {
                                    let creator = inst.voucher.creator_profile.id.clone();
                                    let guarantors = inst
                                        .voucher
                                        .signatures
                                        .iter()
                                        .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
                                        .map(|s| s.signer_id.clone())
                                        .collect::<Vec<_>>();
                                    (creator, guarantors)
                                })
                                .unwrap_or((None, Vec::new()))
                        })
                        .map_err(FrontendError::from)?;
                    Ok((nonce, reqs_res, lifecycle, creator_id, guarantor_dids))
                },
            )?;

        let requests = match reqs_res {
            Ok(reqs) => reqs,
            Err(err) => {
                log::warn!(
                    "Failed to build L2 intent committee requests for {}: {}",
                    local_id,
                    err.message
                );
                l2_dispatch.push(IntentDispatchObservation {
                    local_instance_id: local_id.clone(),
                    status: "pending".into(),
                    http_status: None,
                });
                continue;
            }
        };

        let mut dispatch = dispatch_lock_committee_batches(
            &client,
            &requests,
            Duration::from_secs(4),
            &lifecycle.l2_probe_cache,
        )
        .await;
        let mut all_competing_entries = dispatch.competing_entries.clone();
        let mut all_receipts_for_equiv = dispatch.receipts.clone();
        let mut all_equiv_proofs = dispatch.equivocation_proofs.clone();

        // Handle `409 epoch_frozen` manifest upgrades and `410 revoked` relocations via shared path.
        let upgrade = crate::checkout::resolve_manifest_upgrade_after_dispatch(
            state,
            session,
            &client,
            &intent.anchor_id,
            creator_id.as_deref(),
            &guarantor_dids,
            &lifecycle,
            &intent.manifest,
            &dispatch,
            password,
        )
        .await?;

        if upgrade.upgraded_manifest.is_some() || upgrade.relocated {
            if let Some((newer_manifest, _)) = upgrade.upgraded_manifest {
                let next_nonce = random_non_zero_nonce();
                let (fresh_lc, next_reqs_opt) = state.with_session_required(
                    session,
                    "Wallet session changed during L2 manifest upgrade retry",
                    |service| {
                        let updated_reqs = match service
                            .update_l2_anchor_manifest(newer_manifest.clone(), password)
                        {
                            Ok(_) => {
                                let known_heads =
                                    crate::state::load_known_head_receipts_for_anchor(
                                        service,
                                        &intent.anchor_id,
                                        password,
                                    )?;
                                match service.build_intent_committee_requests(
                                    local_id,
                                    &next_nonce,
                                    &known_heads,
                                    true,
                                ) {
                                    Ok(reqs) => Some(reqs),
                                    Err(err) => {
                                        log::warn!(
                                            "Rebuilding L2 intent requests after manifest upgrade failed for {}: {}",
                                            local_id,
                                            err
                                        );
                                        None
                                    }
                                }
                            }
                            Err(err) => {
                                log::warn!(
                                    "Persisting upgraded L2 anchor manifest failed for {}: {}",
                                    local_id,
                                    err
                                );
                                None
                            }
                        };
                        let fresh_lc = state
                            .get_or_load_internal_lifecycle(service, password)
                            .map_err(FrontendError::from)?;
                        Ok((fresh_lc, updated_reqs))
                    },
                )?;
                lifecycle = fresh_lc;
                if let Some(next_reqs) = next_reqs_opt {
                    active_nonce = next_nonce;
                    if !next_reqs.is_empty() {
                        let dispatch2 = dispatch_lock_committee_batches(
                            &client,
                            &next_reqs,
                            Duration::from_secs(4),
                            &lifecycle.l2_probe_cache,
                        )
                        .await;
                        all_competing_entries.extend(dispatch2.competing_entries.iter().cloned());
                        all_receipts_for_equiv.extend(dispatch2.receipts.iter().cloned());
                        all_equiv_proofs.extend(dispatch2.equivocation_proofs.iter().cloned());
                        dispatch = dispatch2;
                    } else {
                        dispatch = crate::gateway_resolver::CommitteeDispatchResult::default();
                    }
                }
            } else {
                lifecycle = state.with_session_required(
                    session,
                    "Wallet session changed during L2 relocation retry",
                    |service| {
                        state
                            .get_or_load_internal_lifecycle(service, password)
                            .map_err(FrontendError::from)
                    },
                )?;
                let pending_retry_pubs: HashSet<LockPubKey> = dispatch
                    .pending_retries
                    .iter()
                    .map(|(s, _)| s.lock_pub)
                    .collect();
                let retry_reqs: Vec<_> = requests
                    .iter()
                    .filter(|(s, _)| {
                        upgrade.relocated_lock_pubs.contains(&s.lock_pub)
                            || pending_retry_pubs.contains(&s.lock_pub)
                    })
                    .cloned()
                    .collect();
                if !retry_reqs.is_empty() {
                    let dispatch2 = dispatch_lock_committee_batches(
                        &client,
                        &retry_reqs,
                        Duration::from_secs(4),
                        &lifecycle.l2_probe_cache,
                    )
                    .await;
                    all_competing_entries.extend(dispatch2.competing_entries.iter().cloned());
                    all_receipts_for_equiv.extend(dispatch2.receipts.iter().cloned());
                    all_equiv_proofs.extend(dispatch2.equivocation_proofs.iter().cloned());
                    crate::checkout::merge_receipts_dedup(
                        &mut dispatch.receipts,
                        dispatch2.receipts,
                    );
                }
            }
        }

        let (receipts_recorded_ok, has_verified_ok, has_verified_conflict) = if !dispatch
            .receipts
            .is_empty()
            || !all_competing_entries.is_empty()
            || !all_receipts_for_equiv.is_empty()
            || !all_equiv_proofs.is_empty()
        {
            state.with_session_required(
                session,
                "Wallet session changed before L2 intent receipts could be persisted",
                |service| {
                    crate::state::record_known_head_receipts(
                        service,
                        &intent.anchor_id,
                        &all_receipts_for_equiv,
                        password,
                    )?;

                    if let Some(proof) =
                        human_money_core::services::l2_gateway::detect_equivocation(
                            &all_receipts_for_equiv,
                        )
                    {
                        all_equiv_proofs.push(proof);
                    }
                    for proof in &all_equiv_proofs {
                        if let Err(err) = record_equivocation_and_refresh_cache(
                            state,
                            service,
                            &lifecycle.l2_probe_cache,
                            proof,
                            now_secs.max(1),
                            password,
                        ) {
                            log::warn!(
                                "Failed to persist server equivocation proof: {}",
                                err.message
                            );
                        }
                    }

                    let offenders = state.get_or_load_server_offender_list(service, password)?;
                    let current_intent = match service.pending_l2_send_intent(local_id) {
                        Ok(ci) => ci,
                        Err(err) => {
                            log::warn!(
                                "Pending L2 send intent {} missing when recording receipts: {}",
                                local_id,
                                err
                            );
                            return Ok((false, false, false));
                        }
                    };

                    let valid_slots: HashSet<[u8; 32]> =
                        current_intent.entries.iter().map(|e| e.slot).collect();
                    let voucher_opt = service
                        .with_wallet(|w| {
                            let mut v_opt = current_intent
                                .created_change_local_id
                                .as_deref()
                                .and_then(|cid| w.get_voucher_instance(cid))
                                .or_else(|| w.get_voucher_instance(local_id))
                                .map(|i| i.voucher.clone());
                            if let (Some(ref mut v), Ok(intent_tx)) = (
                                &mut v_opt,
                                serde_json::from_slice::<
                                    human_money_core::models::voucher::Transaction,
                                >(
                                    &current_intent.transaction_bytes
                                ),
                            ) {
                                if !v.transactions.iter().any(|t| t.t_id == intent_tx.t_id) {
                                    v.transactions.push(intent_tx);
                                }
                            }
                            v_opt
                        })
                        .map_err(FrontendError::from)?;

                    type CompetingEntryKey = ([u8; 32], [u8; 32], [u8; 32]);
                    let mut verified_competing_by_key: HashMap<CompetingEntryKey, LockEntry> =
                        HashMap::new();
                    if let Some(ref voucher) = voucher_opt {
                        for ce in all_competing_entries
                            .iter()
                            .filter(|ce| valid_slots.contains(&ce.slot))
                        {
                            let Ok(ce_hash) = ce.entry_hash() else {
                                continue;
                            };
                            let valid = voucher.transactions.iter().rev().any(|hop_tx| {
                                let Ok(ds_tag) =
                                    human_money_core::services::l2_gateway::derive_challenge_tag_bytes(
                                        hop_tx,
                                    )
                                else {
                                    return false;
                                };
                                if human_money_core::humoco_proto::derive_slot(
                                    &intent.anchor_id,
                                    &ds_tag,
                                ) != ce.slot
                                {
                                    return false;
                                }
                                let Ok(local_t_id) =
                                    human_money_core::crypto::decode_bs58_fixed::<32>(
                                        &hop_tx.t_id,
                                        "hop.t_id",
                                    )
                                else {
                                    return false;
                                };
                                if ce.child_lock == local_t_id {
                                    return false;
                                }
                                human_money_core::services::l2_gateway::verify_competing_entry_and_build_fingerprint(
                                    &intent.anchor_id,
                                    &ds_tag,
                                    ce,
                                    voucher,
                                    Some(&current_intent.manifest.authority.creator_mig_pub),
                                    DEFAULT_LOCK_POW_BASE_BITS,
                                )
                                .is_ok()
                            });
                            if valid {
                                verified_competing_by_key
                                    .entry((ce.slot, ce.child_lock, ce_hash))
                                    .or_insert_with(|| ce.clone());
                            }
                        }
                    }

                    let Ok(committee) = current_intent.manifest.committee() else {
                        return Ok((false, false, false));
                    };
                    let admin_by_lock: HashMap<LockPubKey, [u8; 32]> = current_intent
                        .manifest
                        .servers
                        .iter()
                        .map(|s| (s.lock_pub, s.admin_pub))
                        .collect();

                    let mut filtered_receipts = Vec::new();
                    let mut chosen_competing: Option<LockEntry> = None;
                    for r in &dispatch.receipts {
                        let Some(matched_entry) =
                            current_intent.entries.iter().find(|e| e.slot == r.slot)
                        else {
                            continue;
                        };
                        let Ok(matched_entry_hash) = matched_entry.entry_hash() else {
                            continue;
                        };
                        if !committee.contains(&r.lock_pub)
                            || is_server_key_revoked(
                                &offenders,
                                &r.lock_pub,
                                admin_by_lock.get(&r.lock_pub),
                            )
                            || r.bucket
                                != human_money_core::humoco_proto::derive_bucket(
                                    &intent.anchor_id,
                                    &r.lock_pub,
                                )
                            || r.recipient_nonce != active_nonce
                            || r.write_seq == 0
                            || r.retain_until_quarter
                                != current_intent.manifest.retain_until_quarter
                            || !r.verify_signature()
                        {
                            continue;
                        }
                        let is_conflict = r.kind == LockReceiptKind::ConflictSeen
                            || r.child_lock != matched_entry.child_lock
                            || (r.kind == LockReceiptKind::FirstWrite
                                && r.entry_hash != matched_entry_hash);
                        if is_conflict {
                            let Some(verified_ce) =
                                verified_competing_by_key.get(&(r.slot, r.child_lock, r.entry_hash))
                            else {
                                continue;
                            };
                            if chosen_competing.is_none() {
                                chosen_competing = Some(verified_ce.clone());
                            }
                        }
                        filtered_receipts.push(r.clone());
                    }

                    if filtered_receipts.is_empty() {
                        let nothing_expected =
                            dispatch.receipts.is_empty() && all_competing_entries.is_empty();
                        return Ok((nothing_expected, false, false));
                    }

                    let verified_conflict = chosen_competing.is_some();
                    match service.record_intent_server_receipts(
                        local_id,
                        &active_nonce,
                        &filtered_receipts,
                        chosen_competing.as_ref(),
                        pow_base_bits,
                        now_ms,
                        password,
                    ) {
                        Ok((_, Some(equiv_proof))) => {
                            if let Err(err) = record_equivocation_and_refresh_cache(
                                state,
                                service,
                                &lifecycle.l2_probe_cache,
                                &equiv_proof,
                                now_secs.max(1),
                                password,
                            ) {
                                log::warn!(
                                    "Failed to persist server equivocation proof: {}",
                                    err.message
                                );
                            }
                            Ok((true, true, verified_conflict))
                        }
                        Ok((_, None)) => Ok((true, true, verified_conflict)),
                        Err(err) => {
                            log::warn!("Recording L2 send intent receipts failed: {}", err);
                            Ok((false, false, false))
                        }
                    }
                },
            )?
        } else {
            (true, false, false)
        };

        let mut obs = summarize_intent_dispatch(local_id, &dispatch);
        if !receipts_recorded_ok {
            obs.status = "pending".to_string();
        } else if has_verified_conflict {
            obs.status = "conflict".to_string();
        } else if obs.status == "conflict" {
            obs.status = if has_verified_ok {
                "confirmed".to_string()
            } else {
                "pending".to_string()
            };
        }
        l2_dispatch.push(obs);
    }

    Ok(l2_dispatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::{
        humoco_proto::{
            crypto::DEFAULT_LOCK_POW_BASE_BITS,
            encode_quarter,
            manifest::{AnchorAuthority, AnchorManifest, GenesisTicket, NominatedBy, ServerEntry},
        },
        models::{profile::PublicProfile, voucher::ValueDefinition},
        services::l2_gateway,
        test_utils::{self, ACTORS},
        NewVoucherData,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn k2_corrupt_known_heads_blocks_intent_dispatch_without_http() {
        let dir = crate::test_support::temp_test_dir("k2_corrupt_sidecar_preflight");
        let pw = "test-pw";
        let (mut service, _) =
            test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        service.unlock_session(pw, 600).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_bg = Arc::clone(&hits);
        let server_task = tokio::spawn(async move {
            while let Ok((_sock, _)) = listener.accept().await {
                hits_bg.fetch_add(1, Ordering::SeqCst);
            }
        });

        let anchor_nonce = [42u8; 32];
        let creator_mig_pub = service
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let sk1 = SigningKey::from_bytes(&[73u8; 32]);
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: format!("http://{addr}/sync.php"),
                read_url: format!("http://{addr}/d"),
                admin_pub: sk1.verifying_key().to_bytes(),
                lock_pub: sk1.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let mut t = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: 1_760_000_000,
            lock_pub: sk1.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        t.sig = sk1.sign(&t.digest()).to_bytes();
        service
            .register_l2_anchor_template_with_nonce(
                manifest,
                vec![t],
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();

        let user_id = service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        service
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(user_id),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "50".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let local_id = service
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        service
            .prepare_l2_send_intent(&local_id, DEFAULT_LOCK_POW_BASE_BITS, Some(pw))
            .unwrap();
        let intent = service.pending_l2_send_intent(&local_id).unwrap();

        // Corrupt the anti-rollback sidecar before dispatch.
        service
            .save_local_sidecar_data(
                crate::state::KNOWN_LOCK_HEADS_KEY,
                b"{corrupt-lock-heads",
                Some(pw),
            )
            .unwrap();

        let state = crate::test_support::create_test_state(service);
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let res = dispatch_l2_send_intents(
            &state,
            &session,
            &[intent],
            DEFAULT_LOCK_POW_BASE_BITS,
            Some(pw),
        )
        .await;

        server_task.abort();
        assert!(
            res.is_err(),
            "Corrupt anti-rollback sidecar must fail closed before sending L2 requests"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "Zero HTTP requests may be dispatched when known heads fail to load"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn s06_f4_f9_dispatch_intent_upgrades_on_409_and_demotes_rejected_receipts_to_pending() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = crate::test_support::temp_test_dir("s06_f4_f9_dispatch");
        let pw = "pw-s06-f4";
        let (mut service, _) =
            test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        service.unlock_session(pw, 600).unwrap();

        let anchor_nonce = [55u8; 32];
        let creator_mig_sk = service
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
            })
            .unwrap()
            .unwrap();
        let creator_mig_pub = creator_mig_sk.verifying_key().to_bytes();

        let sk1 = SigningKey::from_bytes(&[91u8; 32]);
        let sk2 = SigningKey::from_bytes(&[92u8; 32]);
        let retain_q = encode_quarter(2035, 4).unwrap();

        // Bind server 1 (will respond 409 epoch_frozen with upgraded rev=2 manifest pointing to server 2)
        // and server 2 (will return a 200 OK with an INVALID signature receipt so recording fails!).
        let listener1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr1 = listener1.local_addr().unwrap();
        let listener2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr2 = listener2.local_addr().unwrap();

        let manifest_rev1 = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: format!("http://{addr1}/sync.php"),
                read_url: format!("http://{addr1}/d"),
                admin_pub: sk1.verifying_key().to_bytes(),
                lock_pub: sk1.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest_rev1.rev1_anchor_id().unwrap();
        let anchor_hex = hex::encode(anchor_id);

        // Build genuine migrated rev=2 manifest switching committee to server 2
        let new_servers = vec![ServerEntry {
            write_url: format!("http://{addr2}/sync.php"),
            read_url: format!("http://{addr2}/d"),
            admin_pub: sk2.verifying_key().to_bytes(),
            lock_pub: sk2.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        }];
        let prop = human_money_core::services::l2_rotation::build_creator_rotation_proposal(
            anchor_id,
            &manifest_rev1,
            1,
            new_servers,
            Vec::new(),
            1_760_000_050,
            &creator_mig_sk,
        )
        .unwrap();
        let state_hash = human_money_core::humoco_proto::compute_import_union_state_hash(
            &anchor_id,
            2,
            &std::collections::BTreeMap::new(),
        )
        .unwrap();
        let prop_hash = prop.core_digest().unwrap();
        let mut ack2 = human_money_core::humoco_proto::ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash: state_hash,
            retain_until_quarter: retain_q,
            lock_pub: sk2.verifying_key().to_bytes(),
            acked_at: 1_760_000_050,
            sig: [0u8; 64],
        };
        ack2.sig = sk2.sign(&ack2.digest()).to_bytes();
        let commit_d = human_money_core::humoco_proto::MigrationCert::commit_digest(
            &anchor_id,
            &prop_hash,
            2,
            &state_hash,
        );
        let commit_sig = human_money_core::humoco_proto::CommitteeSignature {
            lock_pub: sk1.verifying_key().to_bytes(),
            committed_at: 1_760_000_050,
            sig: sk1.sign(&commit_d).to_bytes(),
        };
        let (manifest_rev2, _) =
            human_money_core::services::l2_rotation::assemble_migrated_manifest(
                &manifest_rev1,
                &anchor_id,
                prop,
                state_hash,
                vec![ack2],
                vec![commit_sig],
            )
            .unwrap();

        let rev2_for_s1 = manifest_rev2.clone();
        let s1_hits = Arc::new(AtomicUsize::new(0));
        let s1_hits_bg = Arc::clone(&s1_hits);
        let s1_task = tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener1.accept().await {
                s1_hits_bg.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0u8; 8192];
                let _ = sock.read(&mut buf).await;
                let body = serde_json::json!({
                    "error": "epoch_frozen",
                    "latest_manifest": rev2_for_s1,
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        let s2_hits = Arc::new(AtomicUsize::new(0));
        let s2_hits_bg = Arc::clone(&s2_hits);
        let sk2_pub = sk2.verifying_key().to_bytes();
        let s2_task = tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener2.accept().await {
                s2_hits_bg.fetch_add(1, Ordering::SeqCst);
                let mut buf = vec![0u8; 16384];
                let _ = sock.read(&mut buf).await;
                // Return a 200 OK with a forged-signature receipt (write_seq = 0, invalid sig)
                let bad_receipt = human_money_core::humoco_proto::types::LockReceipt {
                    kind: human_money_core::humoco_proto::types::LockReceiptKind::FirstWrite,
                    lock_pub: sk2_pub,
                    bucket: human_money_core::humoco_proto::derive_bucket(&anchor_id, &sk2_pub),
                    slot: [1u8; 32],
                    child_lock: [2u8; 32],
                    entry_hash: [3u8; 32],
                    recipient_nonce: [4u8; 32],
                    write_seq: 0,
                    bucket_head: [0u8; 32],
                    retain_until_quarter: retain_q,
                    sig: [0u8; 64],
                };
                let body = serde_json::json!({
                    "results": [{
                        "status": "ok",
                        "receipt": bad_receipt,
                    }]
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });

        let mut t1 = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: 1_760_000_000,
            lock_pub: sk1.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        t1.sig = sk1.sign(&t1.digest()).to_bytes();
        service
            .register_l2_anchor_template_with_nonce(
                manifest_rev1,
                vec![t1],
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();

        let user_id = service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        service
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(user_id),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "50".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let local_id = service
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        service
            .prepare_l2_send_intent(&local_id, DEFAULT_LOCK_POW_BASE_BITS, Some(pw))
            .unwrap();
        let intent = service.pending_l2_send_intent(&local_id).unwrap();

        let state = crate::test_support::create_test_state(service);
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let obs = dispatch_l2_send_intents(
            &state,
            &session,
            &[intent],
            DEFAULT_LOCK_POW_BASE_BITS,
            Some(pw),
        )
        .await
        .unwrap();

        s1_task.abort();
        s2_task.abort();

        assert_eq!(
            s1_hits.load(Ordering::SeqCst),
            1,
            "Server 1 must be hit first"
        );
        assert_eq!(
            s2_hits.load(Ordering::SeqCst),
            1,
            "S06-F4: 409 epoch_frozen must upgrade manifest to rev=2 and retry against Server 2"
        );
        let stored_rev = state
            .lock_service()
            .unwrap()
            .with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&anchor_hex)
                    .map(|m| m.revision)
            })
            .unwrap();
        assert_eq!(
            stored_rev,
            Some(2),
            "Local manifest must be upgraded to rev=2"
        );
        assert_eq!(
            obs[0].status, "pending",
            "S06-F9: rejected/unrecorded 200 OK receipts must NOT report confirmed (got {:?})",
            obs[0]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
