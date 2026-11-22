use super::execute::resolve_manifest_upgrade_after_dispatch;
use super::*;
use std::collections::{HashMap, HashSet};

use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS, manifest::AnchorManifest, types::LockPubKey,
};
use human_money_core::models::layer2_api::LockEntry;
use human_money_core::services::{bundle_processor, l2_gateway};

use crate::{models::FrontendError, sync_client, AppState};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PrecommitL2Verification {
    OfflineOnly,
    CleanQuorum {
        quorum_certificates: Vec<(
            String,
            Vec<human_money_core::humoco_proto::types::QuorumCertificate>,
        )>,
    },
    ConflictDetected {
        quorum_certificates: Vec<(
            String,
            Vec<human_money_core::humoco_proto::types::QuorumCertificate>,
        )>,
        competing_entries: Vec<(LockEntry, u32)>,
    },
    RetryLater {
        reason: String,
    },
}

enum VoucherPrecommitState {
    CleanQuorum,
    VerifiedConflict(Vec<(LockEntry, u32)>),
    Unsettled(String),
}

type PrecommitBatchPayload = (
    Vec<human_money_core::humoco_proto::types::QuorumCertificate>,
    Vec<LockEntry>,
    Vec<(
        human_money_core::humoco_proto::manifest::ServerEntry,
        human_money_core::models::layer2_api::SyncLockBatchRequest,
    )>,
    [u8; 32],
);

#[allow(clippy::too_many_arguments)]
fn collect_prefix_qcs_and_build_precommit_batch(
    state: &AppState,
    session: &crate::session::SessionToken,
    opened_bundle: &human_money_core::models::profile::TransactionBundle,
    voucher: &human_money_core::Voucher,
    anchor_id: &[u8; 32],
    manifest: &AnchorManifest,
    pow_base_bits: u32,
    password: Option<&str>,
) -> Result<PrecommitBatchPayload, FrontendError> {
    let first_unconfirmed = l2_gateway::first_unconfirmed_hop_index(
        voucher,
        manifest,
        &opened_bundle.quorum_certificates,
    )
    .map_err(FrontendError::from)?;

    let mut voucher_qcs = Vec::new();
    for tx in &voucher.transactions[..first_unconfirmed] {
        if let Ok(child) = human_money_core::crypto::decode_bs58_fixed::<32>(&tx.t_id, "t_id") {
            if let Some(qc) = opened_bundle
                .quorum_certificates
                .iter()
                .find(|q| q.child_lock == child)
            {
                voucher_qcs.push(qc.clone());
            }
        }
    }

    if first_unconfirmed >= voucher.transactions.len() {
        return Ok((voucher_qcs, Vec::new(), Vec::new(), [0u8; 32]));
    }

    let (entries, requests, nonce) = state.with_session_required(
        session,
        "Wallet session changed during pre-commit L2 verification",
        |service| {
            let known_heads =
                crate::state::load_known_head_receipts_for_anchor(service, anchor_id, password)?;
            let entries = l2_gateway::build_voucher_lock_batch(
                voucher,
                first_unconfirmed,
                &opened_bundle.causal_outputs,
                if first_unconfirmed == 0 {
                    voucher
                        .transactions
                        .first()
                        .and_then(|t| opened_bundle.root_mig_sig_for(&t.t_id))
                } else {
                    None
                },
                pow_base_bits,
            )
            .map_err(FrontendError::from)?;
            let nonce = random_non_zero_nonce();
            let requests = l2_gateway::build_committee_batch_requests(
                anchor_id,
                manifest,
                &opened_bundle.genesis_tickets,
                &entries,
                &nonce,
                &known_heads,
            )
            .map_err(FrontendError::from)?;
            Ok((entries, requests, nonce))
        },
    )?;

    Ok((voucher_qcs, entries, requests, nonce))
}

/// Verifies an incoming `TransactionBundle` against its Layer-2 Sync-Lock committee(s) BEFORE
/// committing any voucher into `VoucherStore` (K1 fail-closed per-voucher pre-commit contract).
pub(crate) async fn verify_bundle_l2_precommit(
    state: &AppState,
    session: &crate::session::SessionToken,
    bundle: &[u8],
    password: Option<&str>,
) -> Result<PrecommitL2Verification, FrontendError> {
    let opened_bundle = state.with_session_required(
        session,
        "Wallet session changed before pre-commit L2 verification",
        |service| {
            service
                .with_wallet_and_identity(|_, identity| {
                    bundle_processor::open_and_verify_bundle(identity, bundle)
                })
                .map_err(FrontendError::from)?
                .map_err(FrontendError::from)
        },
    )?;

    let anchored_vouchers: Vec<human_money_core::Voucher> = opened_bundle
        .vouchers
        .iter()
        .filter(|v| v.layer2_anchor.is_some())
        .cloned()
        .collect();
    if anchored_vouchers.is_empty() {
        return Ok(PrecommitL2Verification::OfflineOnly);
    }

    let mut active_lifecycle = state.with_session_required(
        session,
        "Wallet session changed before pre-commit L2 verification",
        |service| {
            state
                .get_or_load_internal_lifecycle(service, password)
                .map_err(FrontendError::from)
        },
    )?;
    let client = sync_client::create_sync_http_client();

    let mut quorum_certificates: Vec<(
        String,
        Vec<human_money_core::humoco_proto::types::QuorumCertificate>,
    )> = Vec::new();
    let mut competing_entries: Vec<(LockEntry, u32)> = Vec::new();
    let mut retry_reason: Option<String> = None;

    for voucher in &anchored_vouchers {
        let Some(anchor_id) = voucher.layer2_anchor else {
            continue;
        };
        let anchor_hex = hex::encode(anchor_id);
        let init_t_id = voucher
            .transactions
            .first()
            .map(|t| t.t_id.clone())
            .unwrap_or_default();
        let guarantor_dids: Vec<String> = voucher
            .signatures
            .iter()
            .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
            .map(|s| s.signer_id.clone())
            .collect();

        let local_manifest = state.with_session_required(
            session,
            "Wallet session changed during pre-commit L2 verification",
            |service| {
                let bundled = opened_bundle.anchor_manifest_for(&anchor_id).cloned();
                let stored = service
                    .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
                    .map_err(FrontendError::from)?;
                Ok(match (bundled, stored) {
                    (Some(b), Some(s)) if s.revision > b.revision => Some(s),
                    (Some(b), _) => Some(b),
                    (None, s) => s,
                })
            },
        )?;

        let mut manifest = match local_manifest {
            Some(m) => m,
            None => match crate::l2_lifecycle::discover_anchor_manifest(
                state,
                session,
                &client,
                &anchor_id,
                voucher.creator_profile.id.as_deref(),
                &guarantor_dids,
                password,
            )
            .await?
            {
                Some(m) => m,
                None => {
                    if retry_reason.is_none() {
                        retry_reason = Some(format!(
                            "Anchor manifest unavailable for voucher {}",
                            voucher.voucher_id
                        ));
                    }
                    continue;
                }
            },
        };

        let mut pow_base_bits = crate::services::common::max_pow_base_bits_for_servers(
            &active_lifecycle,
            manifest.servers.iter().map(|s| s.lock_pub),
        );

        let (mut voucher_qcs, mut entries, mut requests, mut expected_nonce) =
            collect_prefix_qcs_and_build_precommit_batch(
                state,
                session,
                &opened_bundle,
                voucher,
                &anchor_id,
                &manifest,
                pow_base_bits,
                password,
            )?;

        let voucher_state = if entries.is_empty() {
            VoucherPrecommitState::CleanQuorum
        } else {
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

            // H7/H10: Bounded single retry after `409 epoch_frozen` / `410 revoked` relocation / unreachable manifest upgrade
            let upgrade = resolve_manifest_upgrade_after_dispatch(
                state,
                session,
                &client,
                &anchor_id,
                voucher.creator_profile.id.as_deref(),
                &guarantor_dids,
                &active_lifecycle,
                &manifest,
                &dispatch,
                password,
            )
            .await?;
            if upgrade.upgraded_manifest.is_some() || upgrade.relocated {
                let upgraded = upgrade.upgraded_manifest.is_some();
                let fresh_lifecycle = state.with_session_required(
                    session,
                    "Wallet session changed during pre-commit manifest upgrade",
                    |service| {
                        if let Some((newer_manifest, upgraded_pow_bits)) = upgrade.upgraded_manifest
                        {
                            manifest = newer_manifest.clone();
                            pow_base_bits = pow_base_bits.max(upgraded_pow_bits);
                            service
                                .update_l2_anchor_manifest(newer_manifest, password)
                                .map_err(FrontendError::from)?;
                        }
                        state
                            .get_or_load_internal_lifecycle(service, password)
                            .map_err(FrontendError::from)
                    },
                )?;
                active_lifecycle = fresh_lifecycle;
                if upgraded {
                    let rebuilt = collect_prefix_qcs_and_build_precommit_batch(
                        state,
                        session,
                        &opened_bundle,
                        voucher,
                        &anchor_id,
                        &manifest,
                        pow_base_bits,
                        password,
                    )?;
                    voucher_qcs = rebuilt.0;
                    entries = rebuilt.1;
                    requests = rebuilt.2;
                    expected_nonce = rebuilt.3;
                    if !requests.is_empty() {
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
                    }
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
                    }
                }
            }

            if entries.is_empty() {
                VoucherPrecommitState::CleanQuorum
            } else {
                if let Some(eq) = l2_gateway::detect_equivocation(&all_receipts_for_equiv) {
                    all_equiv_proofs.push(eq);
                }
                let has_equivocation = !all_equiv_proofs.is_empty();
                let offenders = state.with_session_required(
                    session,
                    "Wallet session changed before recording PoST L2 check receipts",
                    |service| {
                        if has_equivocation {
                            let now_secs = sync_client::current_unix_timestamp().max(1);
                            for eq in &all_equiv_proofs {
                                if let Err(e) =
                                    crate::l2_lifecycle::record_equivocation_and_refresh_cache(
                                        state,
                                        service,
                                        &active_lifecycle.l2_probe_cache,
                                        eq,
                                        now_secs,
                                        password,
                                    )
                                {
                                    log::warn!(
                                        "Failed to record server equivocation proof: {}",
                                        e.message
                                    );
                                }
                            }
                        }
                        crate::state::record_known_head_receipts(
                            service,
                            &anchor_id,
                            &all_receipts_for_equiv,
                            password,
                        )?;
                        state.get_or_load_server_offender_list(service, password)
                    },
                )?;

                // K1: Verify each competing entry cryptographically against this voucher's hops
                // and creator_mig_pub before counting it as VerifiedConflict (always using
                // historical DEFAULT_LOCK_POW_BASE_BITS, never live server pow_base_bits).
                let mut verified_conflicts: Vec<(LockEntry, u32)> = Vec::new();
                for ce in &all_competing_entries {
                    let is_valid_conflict = voucher.transactions.iter().any(|hop_tx| {
                        let Ok(ds_tag) = l2_gateway::derive_challenge_tag_bytes(hop_tx) else {
                            return false;
                        };
                        if human_money_core::humoco_proto::derive_slot(&anchor_id, &ds_tag)
                            != ce.slot
                        {
                            return false;
                        }
                        let Ok(local_t_id) = human_money_core::crypto::decode_bs58_fixed::<32>(
                            &hop_tx.t_id,
                            "hop.t_id",
                        ) else {
                            return false;
                        };
                        if ce.child_lock == local_t_id {
                            return false;
                        }
                        l2_gateway::verify_competing_entry_and_build_fingerprint(
                            &anchor_id,
                            &ds_tag,
                            ce,
                            voucher,
                            Some(&manifest.authority.creator_mig_pub),
                            DEFAULT_LOCK_POW_BASE_BITS,
                        )
                        .is_ok()
                    });
                    if is_valid_conflict
                        && !verified_conflicts
                            .iter()
                            .any(|(existing, _)| existing == ce)
                    {
                        verified_conflicts.push((ce.clone(), DEFAULT_LOCK_POW_BASE_BITS));
                    }
                }

                if !verified_conflicts.is_empty() {
                    VoucherPrecommitState::VerifiedConflict(verified_conflicts)
                } else if has_equivocation {
                    VoucherPrecommitState::Unsettled(format!(
                        "PoST L2 committee equivocation detected for voucher {}",
                        voucher.voucher_id
                    ))
                } else {
                    let init_deletable_at_checked = match voucher
                        .transactions
                        .first()
                        .and_then(|t| t.deletable_at.as_deref())
                    {
                        Some(raw) => human_money_core::services::utils::parse_timestamp_ms(raw)
                            .ok()
                            .map(Some),
                        None => Some(None),
                    };
                    match (init_deletable_at_checked, manifest.committee()) {
                        (None, _) => VoucherPrecommitState::Unsettled(format!(
                            "PoST L2 voucher {} has invalid init.deletable_at timestamp",
                            voucher.voucher_id
                        )),
                        (Some(Some(del_ms)), _)
                            if !human_money_core::humoco_proto::retention_covers(
                                del_ms,
                                manifest.retain_until_quarter,
                            ) =>
                        {
                            VoucherPrecommitState::Unsettled(format!(
                                "PoST L2 manifest retention does not cover init.deletable_at for voucher {}",
                                voucher.voucher_id
                            ))
                        }
                        (_, Err(e)) => VoucherPrecommitState::Unsettled(format!(
                            "PoST L2 committee manifest invalid for voucher {}: {}",
                            voucher.voucher_id, e
                        )),
                        (Some(init_deletable_at_ms), Ok(committee)) => {
                            let admin_by_lock: HashMap<LockPubKey, [u8; 32]> = manifest
                                .servers
                                .iter()
                                .map(|s| (s.lock_pub, s.admin_pub))
                                .collect();
                            let mut all_entries_ok = true;
                            for entry_req in &entries {
                                let Ok(expected_entry_hash) = entry_req.entry_hash() else {
                                    all_entries_ok = false;
                                    break;
                                };
                                let mut valid_receipts = Vec::new();
                                let mut seen_pubs = HashSet::new();
                                for r in &dispatch.receipts {
                                    if matches!(
                                        r.kind,
                                        human_money_core::humoco_proto::types::LockReceiptKind::FirstWrite
                                    ) && r.slot == entry_req.slot
                                        && r.child_lock == entry_req.child_lock
                                        && r.entry_hash == expected_entry_hash
                                        && r.recipient_nonce == expected_nonce
                                        && r.bucket
                                            == human_money_core::humoco_proto::derive_bucket(
                                                &anchor_id,
                                                &r.lock_pub,
                                            )
                                        && committee.contains(&r.lock_pub)
                                        && !crate::l2_lifecycle::is_server_key_revoked(
                                            &offenders,
                                            &r.lock_pub,
                                            admin_by_lock.get(&r.lock_pub),
                                        )
                                        && init_deletable_at_ms.is_none_or(|del_ms| {
                                            human_money_core::humoco_proto::retention_covers(
                                                del_ms,
                                                r.retain_until_quarter,
                                            )
                                        })
                                        && r.verify_signature()
                                        && seen_pubs.insert(r.lock_pub)
                                    {
                                        valid_receipts.push(r.clone());
                                    }
                                }
                                let qc = human_money_core::humoco_proto::types::QuorumCertificate {
                                    slot: entry_req.slot,
                                    child_lock: entry_req.child_lock,
                                    receipts: valid_receipts,
                                };
                                if qc
                                    .verify_with_anchor(
                                        &committee,
                                        &anchor_id,
                                        manifest.retain_until_quarter,
                                    )
                                    .is_ok()
                                {
                                    voucher_qcs.push(qc);
                                } else {
                                    all_entries_ok = false;
                                    break;
                                }
                            }
                            if all_entries_ok {
                                VoucherPrecommitState::CleanQuorum
                            } else {
                                VoucherPrecommitState::Unsettled(format!(
                                    "PoST L2 committee verification incomplete for voucher {} (threshold Q={})",
                                    voucher.voucher_id, manifest.threshold
                                ))
                            }
                        }
                    }
                }
            }
        };

        match voucher_state {
            VoucherPrecommitState::CleanQuorum => {}
            VoucherPrecommitState::VerifiedConflict(conflicts) => {
                for entry_with_pow in conflicts {
                    if !competing_entries
                        .iter()
                        .any(|(existing, _)| existing == &entry_with_pow.0)
                    {
                        competing_entries.push(entry_with_pow);
                    }
                }
            }
            VoucherPrecommitState::Unsettled(reason) => {
                if retry_reason.is_none() {
                    retry_reason = Some(reason);
                }
            }
        }

        if !init_t_id.is_empty() && !voucher_qcs.is_empty() {
            quorum_certificates.push((init_t_id, voucher_qcs));
        }
    }

    // K1 fail-closed: EVERY anchored voucher must be settled (CleanQuorum or VerifiedConflict).
    // Even one Unsettled voucher prevents committing the bundle.
    if let Some(reason) = retry_reason {
        return Ok(PrecommitL2Verification::RetryLater { reason });
    }
    if !competing_entries.is_empty() {
        return Ok(PrecommitL2Verification::ConflictDetected {
            quorum_certificates,
            competing_entries,
        });
    }
    Ok(PrecommitL2Verification::CleanQuorum {
        quorum_certificates,
    })
}
