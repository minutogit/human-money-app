//! Recipient Sync-Lock checkout orchestration (`CORE-020` / `APP-070`).
//!
//! Computes `base_trusted_keys` and `independently_pinned_creator_keys` strictly from locally
//! trusted sources (own Syncs, Hop-1/Hop-2 non-offender contacts, ADR-08 provider filter seals)
//! and resolves authentic `LockPubKey`s without trusting untrusted manifest claims.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::state::canonicalize_did;
use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS, decode_hex_fixed, manifest::AnchorManifest,
    types::LockPubKey,
};
use human_money_core::models::conflict::TrustStatus;
use human_money_core::models::layer2_api::{
    CheckoutReleaseVerdict, L2ProcessingOutcome, LockEntry,
};
use human_money_core::services::{bundle_processor, l2_gateway};
use rand::RngCore;

use crate::{
    models::FrontendError,
    settings::{InternalLifecycleState, SecurityPolicy},
    sync_client, AppState,
};

const CHECKOUT_BATCH_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VerifiedServerBinding {
    lock_pub: LockPubKey,
    admin_pub: [u8; 32],
}

pub(crate) fn random_non_zero_nonce() -> [u8; 32] {
    loop {
        let mut nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        if nonce != [0u8; 32] {
            return nonce;
        }
    }
}

type ReceiptDedupKey = (LockPubKey, [u8; 32], [u8; 32], [u8; 32]);

pub(crate) fn merge_receipts_dedup(
    base: &mut Vec<human_money_core::humoco_proto::types::LockReceipt>,
    extra: impl IntoIterator<Item = human_money_core::humoco_proto::types::LockReceipt>,
) {
    let mut seen: HashSet<ReceiptDedupKey> = base
        .iter()
        .map(|r| (r.lock_pub, r.slot, r.child_lock, r.entry_hash))
        .collect();
    for r in extra {
        if seen.insert((r.lock_pub, r.slot, r.child_lock, r.entry_hash)) {
            base.push(r);
        }
    }
}

fn fail(message: &str) -> FrontendError {
    FrontendError::from(message.to_string())
}

fn now() -> Result<u64, FrontendError> {
    u64::try_from(chrono::Utc::now().timestamp_millis())
        .map_err(|_| fail("Invalid verification time"))
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseCompletion {
    pub purchase_id: String,
    pub newly_committed: bool,
    pub is_replay: bool,
    pub verdict: crate::models::FrontendCheckoutReleaseVerdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receive_payload: Option<crate::models::ReceiveSuccessPayload>,
}

fn format_bff_name(raw_name: &str, is_test: bool) -> String {
    if is_test {
        if raw_name.starts_with("TEST-") {
            raw_name.to_string()
        } else {
            format!("TEST-{}", raw_name)
        }
    } else {
        raw_name.to_string()
    }
}

/// Resolves trusted `LockPubKey`s (`base_trusted_keys`, `independently_pinned_creator_keys`) from:
/// - Own Sync admin drop & trusted Sync circle (`market_selected_syncs`, `public_syncs`) filtered by `lock_trust_by_admin_pub`
/// - Verified probes in `lifecycle.l2_probe_cache`
/// - Non-offender Hop-1/Hop-2 AddressBook contacts & WoT ADR-08 provider filter seals
///
/// # Security (`LÜCKE-03` & Untrusted Manifest Isolation)
/// - Never trusts `s.lock_pub` copied out of an incoming unverified `AnchorManifest`. Every
///   trusted `write_url` resolves its `lock_pub` strictly via locally cached verified probes
///   or live `?info` + `verify_lock_delegation`.
/// - Excludes sync URLs contributed solely by a foreign `voucher.creator_profile.id` so stranger
///   merchants do not trust creator-nominated servers via contact cards alone.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_trusted_lock_keys(
    state: &AppState,
    session: &crate::session::SessionToken,
    security_policy: &SecurityPolicy,
    lifecycle: &InternalLifecycleState,
    client: &reqwest::Client,
    creator_id: Option<&str>,
    manifest: Option<&AnchorManifest>,
    password: Option<&str>,
) -> Result<(Vec<LockPubKey>, Vec<LockPubKey>), FrontendError> {
    let mut known_url_to_binding: HashMap<String, VerifiedServerBinding> = HashMap::new();

    for probe in lifecycle.l2_probe_cache.values() {
        if !probe.passed
            || !probe.delegation_valid
            || !crate::settings::is_lock_admin_trusted(security_policy, &probe.admin_pub)
        {
            continue;
        }
        let Some(ref del) = probe.delegation else {
            continue;
        };
        if !sync_client::verify_lock_delegation(&probe.lock_pub, del, Some(&probe.admin_pub)) {
            continue;
        }
        let norm_del = crate::services::common::normalize_sync_url(&del.write_url);
        let norm_probe = crate::services::common::normalize_sync_url(&probe.write_url);
        if norm_del.is_empty() || norm_del != norm_probe {
            continue;
        }
        if let (Ok(lock_pub), Ok(admin_pub)) = (
            decode_hex_fixed::<32>(&probe.lock_pub),
            decode_hex_fixed::<32>(&probe.admin_pub),
        ) {
            known_url_to_binding.insert(
                norm_del,
                VerifiedServerBinding {
                    lock_pub,
                    admin_pub,
                },
            );
        }
    }

    let mut own_pinned_urls: HashSet<String> = HashSet::new();
    let mut contact_trusted_urls: HashSet<String> = HashSet::new();
    let mut base_trusted_keys: HashSet<LockPubKey> = HashSet::new();
    let mut independently_pinned_creator_keys: HashSet<LockPubKey> = HashSet::new();

    if let Ok(Some(Some(desc))) = state.with_session(session, |svc| {
        state.get_or_load_own_sync_descriptor(svc, password)
    }) {
        if crate::settings::is_lock_admin_trusted(security_policy, &desc.drop_pub) {
            let norm = crate::services::common::normalize_sync_url(&desc.write_url);
            if !norm.is_empty() {
                own_pinned_urls.insert(norm);
            }
        }
    }
    for d in &security_policy.market_selected_syncs {
        if crate::settings::is_lock_admin_trusted(security_policy, &d.drop_pub) {
            let norm = crate::services::common::normalize_sync_url(&d.write_url);
            if !norm.is_empty() {
                own_pinned_urls.insert(norm);
            }
        }
    }
    for s in &security_policy.public_syncs {
        if s.enabled
            && s.status != "revoked"
            && crate::settings::is_lock_admin_trusted(security_policy, &s.drop_pub)
        {
            let norm = crate::services::common::normalize_sync_url(&s.write_url);
            if !norm.is_empty() {
                own_pinned_urls.insert(norm);
            }
        }
    }

    let canonical_creator = creator_id.and_then(|c| canonicalize_did(c).ok());
    let contact_candidates: Vec<String> = state.with_session_required(
        session,
        "Wallet session changed while resolving contact trust for checkout",
        |s| {
            s.with_wallet(|w| {
                // Lock order strictly `service -> profile_caches`
                let caches_guard = state.profile_caches.lock().ok();
                let caches_ref = caches_guard.as_ref().and_then(|g| g.as_ref());
                let book_opt = caches_ref.and_then(|c| c.contacts.as_ref());
                let wot_ref = caches_ref.and_then(|c| c.wot.as_ref());

                let self_did = wot_ref
                    .map(|r| r.self_did.clone())
                    .or_else(|| canonicalize_did(w.get_user_id()).ok());

                let is_known_offender =
                    |did: &str| matches!(w.check_reputation(did), TrustStatus::KnownOffender(_));

                let mut urls = Vec::new();
                if let Some(book) = book_opt {
                    for contact in book.contacts.values() {
                        if is_known_offender(&contact.did) {
                            continue;
                        }
                        let can_contact = canonicalize_did(&contact.did).ok();
                        // LÜCKE-03: Never accept creator's own card syncs for stranger merchants.
                        if let (Some(can_c), Some(can_cr)) = (&can_contact, &canonical_creator) {
                            if can_c == can_cr && self_did.as_ref() != Some(can_cr) {
                                continue;
                            }
                        }

                        let mut contact_trusted =
                            contact.trust_level.map(|lvl| lvl >= 1).unwrap_or(false);
                        if !contact_trusted {
                            if let (Some(reg), Some(can_c)) = (wot_ref, can_contact.as_ref()) {
                                let hop1_trusted =
                                    reg.own_contacts.get(can_c).is_some_and(|e| e.level >= 1);
                                let adr08_certified =
                                    reg.provider_filters.iter().any(|(iss_did, pf)| {
                                        pf.local_trust_level >= 1
                                            && !is_known_offender(iss_did.as_str())
                                            && !is_known_offender(pf.payload.iss.as_str())
                                            && pf.certified_level(can_c).is_some()
                                    });
                                // Hop-2 reachability check via ego graph (depth <= 2 from node 0 with positive weight,
                                // strictly excluding KnownOffender depth-1 bridges)
                                let graph = reg.get_or_build_graph_cache();
                                let hop2_trusted =
                                    graph.did_to_id.get(can_c).is_some_and(|&target_idx| {
                                        if target_idx == 0 {
                                            return true;
                                        }
                                        let mut depth1_nodes = HashSet::new();
                                        if let Some(edges0) = graph.outgoing.first() {
                                            for e in edges0 {
                                                if e.level >= 1 {
                                                    if e.to == target_idx {
                                                        return true;
                                                    }
                                                    let bridge_is_offender = graph
                                                        .id_to_did
                                                        .get(e.to as usize)
                                                        .is_some_and(|bridge_did| {
                                                            is_known_offender(bridge_did.as_str())
                                                        });
                                                    if !bridge_is_offender {
                                                        depth1_nodes.insert(e.to);
                                                    }
                                                }
                                            }
                                        }
                                        depth1_nodes.into_iter().any(|n1| {
                                            graph.outgoing.get(n1 as usize).is_some_and(|edges1| {
                                                edges1
                                                    .iter()
                                                    .any(|e| e.to == target_idx && e.level >= 1)
                                            })
                                        })
                                    });
                                contact_trusted = hop1_trusted || hop2_trusted || adr08_certified;
                            }
                        }

                        if contact_trusted {
                            for sync in &contact.syncs {
                                if sync.status != "revoked" {
                                    let norm = crate::services::common::normalize_sync_url(
                                        &sync.write_url,
                                    );
                                    if !norm.is_empty() {
                                        urls.push(norm);
                                    }
                                }
                            }
                        }
                    }
                }
                urls
            })
            .map_err(FrontendError::from)
        },
    )?;

    for u in contact_candidates {
        contact_trusted_urls.insert(u);
    }

    let offenders = state.with_session_required(
        session,
        "Wallet session changed while loading server offenders for checkout",
        |svc| state.get_or_load_server_offender_list(svc, password),
    )?;

    let mut urls_to_live_probe: HashMap<(String, LockPubKey, [u8; 32]), (String, bool)> =
        HashMap::new();
    if let Some(m) = manifest {
        for s in &m.servers {
            let expected_admin_hex = hex::encode(s.admin_pub);
            if !crate::settings::is_lock_admin_trusted(security_policy, &expected_admin_hex)
                || crate::l2_lifecycle::is_server_key_revoked(
                    &offenders,
                    &s.lock_pub,
                    Some(&s.admin_pub),
                )
            {
                continue;
            }
            let (resolved_url, _) =
                crate::services::common::resolve_server_locator(s, &lifecycle.l2_probe_cache);
            let norm_emb = crate::services::common::normalize_sync_url(&s.write_url);
            let norm_res = crate::services::common::normalize_sync_url(&resolved_url);
            let is_own_pinned =
                own_pinned_urls.contains(&norm_emb) || own_pinned_urls.contains(&norm_res);
            let is_contact_trusted = contact_trusted_urls.contains(&norm_emb)
                || contact_trusted_urls.contains(&norm_res);
            if !is_own_pinned && !is_contact_trusted {
                continue;
            }
            if let Some(binding) = known_url_to_binding
                .get(&norm_res)
                .or_else(|| known_url_to_binding.get(&norm_emb))
            {
                if binding.lock_pub == s.lock_pub
                    && binding.admin_pub == s.admin_pub
                    && !crate::l2_lifecycle::is_server_key_revoked(
                        &offenders,
                        &binding.lock_pub,
                        Some(&binding.admin_pub),
                    )
                {
                    base_trusted_keys.insert(binding.lock_pub);
                    if is_own_pinned {
                        independently_pinned_creator_keys.insert(binding.lock_pub);
                    }
                }
            } else if !norm_res.is_empty() {
                urls_to_live_probe
                    .entry((norm_res, s.lock_pub, s.admin_pub))
                    .and_modify(|(_, pinned)| *pinned |= is_own_pinned)
                    .or_insert((expected_admin_hex, is_own_pinned));
            }
        }
    }

    if !urls_to_live_probe.is_empty() {
        let now_secs = sync_client::current_unix_timestamp();
        let mut join_set = tokio::task::JoinSet::new();
        for ((url, exp_lock_pub, exp_admin_pub), (exp_admin_hex, is_own_pinned)) in
            urls_to_live_probe
        {
            let c = client.clone();
            join_set.spawn(async move {
                let res = tokio::time::timeout(
                    Duration::from_secs(2),
                    sync_client::fetch_sync_info(&c, &url),
                )
                .await;
                let norm_target = crate::services::common::normalize_sync_url(&url);
                res.ok().and_then(|r| r.ok()).and_then(|info| {
                    let lock = info.lock?;
                    let del = lock.delegation.as_ref()?;
                    if !del.admin_pub.trim().eq_ignore_ascii_case(&exp_admin_hex) {
                        return None;
                    }
                    if crate::services::common::normalize_sync_url(&del.write_url) != norm_target {
                        return None;
                    }
                    if let Some(ref rev) = info.revocation {
                        if sync_client::verify_revocation_notice(rev, &exp_admin_hex).is_ok()
                            && rev.effective_at <= now_secs
                        {
                            return None;
                        }
                    }
                    if !sync_client::verify_lock_delegation(
                        &lock.lock_pub,
                        del,
                        Some(&exp_admin_hex),
                    ) {
                        return None;
                    }
                    let lock_pk = decode_hex_fixed::<32>(&lock.lock_pub).ok()?;
                    let admin_pk = decode_hex_fixed::<32>(&del.admin_pub).ok()?;
                    if lock_pk != exp_lock_pub || admin_pk != exp_admin_pub {
                        return None;
                    }
                    Some((lock_pk, admin_pk, del.admin_pub.clone(), is_own_pinned))
                })
            });
        }
        while let Some(res) = join_set.join_next().await {
            if let Ok(Some((pk, admin_pk, admin_hex, is_own_pinned))) = res {
                if crate::settings::is_lock_admin_trusted(security_policy, &admin_hex)
                    && !crate::l2_lifecycle::is_server_key_revoked(&offenders, &pk, Some(&admin_pk))
                {
                    base_trusted_keys.insert(pk);
                    if is_own_pinned {
                        independently_pinned_creator_keys.insert(pk);
                    }
                }
            }
        }
    }

    Ok((
        base_trusted_keys.into_iter().collect(),
        independently_pinned_creator_keys.into_iter().collect(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_receive_payload_and_record(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    purchase_id: &str,
    bundle_data: &[u8],
    standards: &HashMap<String, String>,
    newly_committed: bool,
    is_replay: bool,
    verdict: crate::models::FrontendCheckoutReleaseVerdict,
    l2_quorum: Option<crate::models::FrontendL2QuorumDto>,
    password: Option<&str>,
) -> Result<
    (
        crate::models::ReceiveSuccessPayload,
        Option<crate::models::TransactionRecord>,
    ),
    FrontendError,
> {
    let (opened_bundle, recipient_id) = service
        .with_wallet_and_identity(|wallet, identity| {
            let bundle = bundle_processor::open_and_verify_bundle(identity, bundle_data)?;
            Ok::<_, human_money_core::Error>((bundle, wallet.get_user_id().to_string()))
        })
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;

    let registry =
        human_money_core::services::voucher_validation::StandardRegistry::from_toml_map(standards)
            .map_err(FrontendError::from)?;

    let mut transfer_summary = human_money_core::wallet::types::TransferSummary::default();
    let mut involved_vouchers_ids = Vec::new();
    let mut involved_vouchers_details = Vec::new();

    for voucher in &opened_bundle.vouchers {
        let local_id =
            human_money_core::wallet::Wallet::calculate_local_instance_id(voucher, &recipient_id)
                .map_err(FrontendError::from)?;
        involved_vouchers_ids.push(local_id.clone());

        let last_tx = voucher
            .transactions
            .last()
            .ok_or_else(|| fail("purchase transaction missing"))?;

        let display_currency = format_bff_name(
            voucher
                .nominal_value
                .abbreviation
                .as_deref()
                .unwrap_or(&voucher.nominal_value.unit),
            voucher.non_redeemable_test_voucher,
        );

        let standard_opt = registry.get(&voucher.voucher_standard.uuid);
        let balances_are_summable = standard_opt
            .map(|s| s.immutable.features.balances_are_summable)
            .unwrap_or(true);
        let standard_name = standard_opt
            .map(|s| s.immutable.identity.name.clone())
            .unwrap_or_else(|| voucher.voucher_standard.name.clone());
        let allow_partial = standard_opt
            .map(|s| s.immutable.features.allow_partial_transfers)
            .unwrap_or(false);

        transfer_summary
            .add_voucher_entry(
                display_currency.clone(),
                &last_tx.amount,
                balances_are_summable,
            )
            .map_err(FrontendError::from)?;

        involved_vouchers_details.push(human_money_core::wallet::types::InvolvedVoucherInfo {
            local_instance_id: local_id,
            voucher_id: voucher.voucher_id.clone(),
            standard_name,
            unit: voucher.nominal_value.unit.clone(),
            amount: last_tx.amount.clone(),
            allow_partial_transfers: allow_partial,
            is_test_voucher: voucher.non_redeemable_test_voucher,
            display_currency,
            display_standard_name: format_bff_name(
                &voucher.voucher_standard.name,
                voucher.non_redeemable_test_voucher,
            ),
            counterparty_id: Some(opened_bundle.sender_id.clone()),
            counterparty_name: opened_bundle.sender_profile_name.clone(),
        });
    }

    let (conflict_summaries, unrelated_open_conflicts_count) =
        crate::services::transfer_service::filter_conflicts_for_received_vouchers(
            service,
            &opened_bundle.vouchers,
        )?;

    let record = crate::models::TransactionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        direction: "received".to_string(),
        recipient_id,
        sender_id: opened_bundle.sender_id.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
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
        bundle_id: opened_bundle.bundle_id.clone(),
        notes: opened_bundle.notes.clone(),
        sender_profile_name: opened_bundle.sender_profile_name.clone(),
    };

    let mut wot_imported_lists = None;
    if newly_committed {
        if let Some(ref wot_json) = opened_bundle.wot_sync_json {
            wot_imported_lists = state.try_import_piggybacked_wot(
                service,
                wot_json,
                &opened_bundle.sender_id,
                password,
            );
        }
        if let Some(ref card_json) = opened_bundle.contact_card_json {
            state.try_import_piggybacked_contact_card(
                service,
                Some(card_json),
                Some(&opened_bundle.sender_id),
                password,
            );
        }
    }

    let receive_kind = if !opened_bundle.vouchers.is_empty()
        && opened_bundle
            .vouchers
            .iter()
            .all(|v| v.transactions.last().is_some_and(|t| t.t_type == "claim"))
    {
        Some("claim".to_string())
    } else if !opened_bundle.vouchers.is_empty()
        && opened_bundle
            .vouchers
            .iter()
            .all(|v| v.transactions.last().is_some_and(|t| t.t_type == "redeem"))
    {
        Some("redeem".to_string())
    } else {
        Some("transfer".to_string())
    };

    let payload = crate::models::ReceiveSuccessPayload {
        sender_id: opened_bundle.sender_id,
        sender_profile_name: opened_bundle.sender_profile_name,
        notes: opened_bundle.notes,
        transfer_summary: transfer_summary.into(),
        involved_vouchers: involved_vouchers_ids,
        involved_vouchers_details: involved_vouchers_details
            .into_iter()
            .map(|d| d.into())
            .collect(),
        verifiable_conflicts: Default::default(),
        conflict_summaries,
        l2_quorum,
        receive_kind,
        checkout_verdict: Some(verdict),
        purchase_id: Some(purchase_id.to_string()),
        newly_committed: Some(newly_committed),
        is_replay: Some(is_replay),
        wot_imported_lists,
        unrelated_open_conflicts_count,
    };

    Ok((payload, Some(record)))
}

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
        .anchor_manifest
        .as_ref()
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
            state.with_session_required(session, "Wallet changed during checkout", |service| {
                let known_heads = crate::state::load_known_head_receipts_for_anchor(
                    service, &anchor_id, password,
                )?;
                service
                    .prepare_checkout_requests(
                        purchase_id,
                        bundle,
                        pow_base_bits,
                        now()?,
                        &known_heads,
                        password,
                    )
                    .map_err(FrontendError::from)
            })?
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
                    let _ = crate::l2_lifecycle::record_equivocation_and_refresh_cache(
                        state,
                        service,
                        &active_lifecycle.l2_probe_cache,
                        equiv_proof,
                        now_secs,
                        password,
                    );
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
                    opened_bundle.root_mig_sig
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
                let bundled = opened_bundle
                    .anchor_manifest
                    .as_ref()
                    .filter(|m| m.verify_anchor_id(&anchor_id).is_ok())
                    .cloned();
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
                                let _ = crate::l2_lifecycle::record_equivocation_and_refresh_cache(
                                    state,
                                    service,
                                    &active_lifecycle.l2_probe_cache,
                                    eq,
                                    now_secs,
                                    password,
                                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::{
        crypto::{compute_ticket_pow_hash, DEFAULT_LOCK_POW_BASE_BITS},
        decode_hex_fixed, encode_quarter,
        manifest::{AnchorAuthority, GenesisTicket, NominatedBy, ServerEntry},
        types::{LockReceipt, LockReceiptKind},
    };
    use human_money_core::models::layer2_api::{
        SyncLockBatchRequest, SyncLockBatchResponse, SyncLockConflict,
    };
    use human_money_core::models::profile::PublicProfile;
    use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
    use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
    use human_money_core::test_utils::{self, ACTORS};
    use human_money_core::wallet::types::{MultiTransferRequest, SourceTransfer};
    use std::path::PathBuf;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{}-{}", prefix, uuid::Uuid::new_v4()))
    }

    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir_recursive(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn checkout_bundle_returns_offline_only_for_n0_voucher() {
        let dir_alice = temp_dir("app-chk-alice-n0");
        let dir_bob = temp_dir("app-chk-bob-n0");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let mut standards = HashMap::new();
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let std_uuid = std_def.immutable.identity.uuid;
        standards.insert(std_uuid, std_toml.clone());

        let _ = svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "50".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();

        let instances = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap();
        let local_id = instances[0].local_instance_id.clone();

        let transfer = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: local_id,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        let state_bob = AppState::new(svc_bob);
        state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };
        let completion = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-offline-1",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();

        assert!(!completion.newly_committed);
        assert_eq!(completion.verdict.outcome, "OfflineOnly");
        assert!(completion.verdict.requires_merchant_approval);
        assert!(!completion.verdict.can_release_goods_automatically);
        assert!(completion.receive_payload.is_some());

        drop(state_bob);
        drop(svc_alice);
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_bob);
    }

    #[tokio::test]
    async fn checkout_bundle_green_and_yellow_for_anchored_voucher() {
        let dir_alice = temp_dir("app-chk-alice-l2");
        let dir_bob = temp_dir("app-chk-bob-l2");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        svc_alice.unlock_session(pw, 600).unwrap();
        svc_bob.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let std_uuid = std_def.immutable.identity.uuid;
        let mut standards = HashMap::new();
        standards.insert(std_uuid, std_toml.clone());

        let sk1 = SigningKey::from_bytes(&[31u8; 32]);
        let sk2 = SigningKey::from_bytes(&[32u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let anchor_nonce = [44u8; 32];
        let creator_mig_pub = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();

        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![
                ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", p1),
                    read_url: format!("http://127.0.0.1:{}/d", p1),
                    admin_pub: sk1.verifying_key().to_bytes(),
                    lock_pub: sk1.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
                ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", p2),
                    read_url: format!("http://127.0.0.1:{}/d", p2),
                    admin_pub: sk2.verifying_key().to_bytes(),
                    lock_pub: sk2.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
            ],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![SigningKey::from_bytes(&[99u8; 32])
                    .verifying_key()
                    .to_bytes()],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let tickets: Vec<GenesisTicket> = [&sk1, &sk2]
            .into_iter()
            .map(|sk| {
                let mut t = GenesisTicket {
                    anchor_id,
                    retain_until_quarter: retain_q,
                    issued_at: 1_760_000_000,
                    lock_pub: sk.verifying_key().to_bytes(),
                    sig: [0u8; 64],
                };
                t.sig = sk.sign(&t.digest()).to_bytes();
                t
            })
            .collect();

        svc_alice
            .register_l2_anchor_template_with_nonce(
                manifest.clone(),
                tickets,
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();

        let _ = svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
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

        let local_id = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();

        let transfer = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: local_id,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        let spawn_lock_srv = |listener: tokio::net::TcpListener, sk: SigningKey| {
            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
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
                        .map(|(i, hex_str)| {
                            let wire = decode_hex_fixed::<605>(hex_str).unwrap();
                            let entry =
                                human_money_core::humoco_proto::causal::LockEntry::from_wire_bytes(
                                    &wire,
                                )
                                .unwrap();
                            let _ = compute_ticket_pow_hash(&req.anchor_id, retain_q, 1, 0);
                            let mut r = LockReceipt {
                                kind: LockReceiptKind::FirstWrite,
                                lock_pub: sk.verifying_key().to_bytes(),
                                bucket: req.bucket,
                                slot: entry.slot,
                                child_lock: entry.child_lock,
                                entry_hash: entry.entry_hash().unwrap(),
                                retain_until_quarter: retain_q,
                                write_seq: (i as u64) + 1,
                                bucket_head: [7u8; 32],
                                recipient_nonce: req.recipient_nonce,
                                sig: [0u8; 64],
                            };
                            r.sig = sk.sign(&r.digest()).to_bytes();
                            r
                        })
                        .collect();
                    let resp_json = serde_json::json!({
                        "status": "ok",
                        "bucket": hex::encode(req.bucket),
                        "retain_until_quarter": retain_q,
                        "receipts": receipts,
                        "conflict": null,
                    })
                    .to_string();
                    let http = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        resp_json.len(),
                        resp_json
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };

        let h1 = spawn_lock_srv(l1, sk1.clone());
        let h2 = spawn_lock_srv(l2, sk2.clone());

        // Case A: Bob has NO trusted servers -> Yellow verdict, NOT auto-committed
        let state_bob = AppState::new(svc_bob);
        state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };
        let yellow = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-yellow",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(!yellow.newly_committed);
        assert!(yellow.verdict.outcome.starts_with("Yellow:"));
        assert!(yellow.verdict.requires_merchant_approval);

        // Case B: Bob trusts both servers via own public_syncs + verified probe cache -> Green Confirmed!
        {
            let mut s = state_bob.cache_security_policy().unwrap();
            let mut lc = state_bob.cache_internal_lifecycle().unwrap();
            for (port, sk) in [(p1, &sk1), (p2, &sk2)] {
                let wurl = format!("http://127.0.0.1:{}/sync.php", port);
                let rurl = format!("http://127.0.0.1:{}/d", port);
                s.public_syncs.push(crate::settings::StoredPublicSync {
                    write_url: wurl.clone(),
                    read_url: rurl.clone(),
                    drop_pub: hex::encode(sk.verifying_key().as_bytes()),
                    register_url: None,
                    access_pubkey: "aa".repeat(32),
                    enabled: true,
                    authorized_channels: vec!["lock".into()],
                    status: "active".into(),
                    sunset_at: None,
                    revocation_reason: None,
                    replacement: None,
                });
                let probe = make_signed_probe_for_test(sk, sk, &wurl, &rurl, retain_q);
                lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
            }
            state_bob.set_cached_security_policy(Some(s));
            state_bob.set_cached_internal_lifecycle(Some(lc));
        }
        let green = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-green",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(green.newly_committed);
        assert!(!green.is_replay);
        assert_eq!(green.verdict.outcome, "Confirmed");
        assert!(green.verdict.can_release_goods_automatically);
        assert!(green.verdict.is_goods_release_authorized);

        let replay = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-green",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(replay.is_replay);
        assert!(!replay.newly_committed);
        assert_eq!(replay.verdict.outcome, "Confirmed");
        assert_eq!(
            replay.receive_payload.as_ref().and_then(|p| p.is_replay),
            Some(true)
        );

        // Case C: Multi-voucher bundle (2 vouchers) must yield Yellow:MultiVoucherManualApproval
        let _ = svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(
                            svc_alice
                                .with_wallet(|w| w.get_user_id().to_string())
                                .unwrap(),
                        ),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "40".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let all_ids: Vec<String> = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()
            .into_iter()
            .map(|v| v.local_instance_id)
            .collect();
        assert_eq!(all_ids.len(), 2);
        let multi_transfer = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: state_bob
                        .lock_service()
                        .unwrap()
                        .with_wallet(|w| w.get_user_id().to_string())
                        .unwrap(),
                    sources: all_ids
                        .into_iter()
                        .map(|lid| SourceTransfer {
                            local_instance_id: lid,
                            amount_to_send: "10".into(),
                        })
                        .collect(),
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();
        let multi_res = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-multi",
            &multi_transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(!multi_res.newly_committed);
        assert!(!multi_res.is_replay);
        assert_eq!(
            multi_res.verdict.outcome,
            "Yellow:MultiVoucherManualApproval"
        );
        assert!(multi_res.verdict.requires_merchant_approval);
        assert!(multi_res.verdict.merchant_warning.is_some());

        h1.abort();
        h2.abort();
        drop(state_bob);
        drop(svc_alice);
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_bob);
    }

    #[tokio::test]
    async fn k2_precommit_l2_verification_retries_without_quorum_and_detects_conflict() {
        use human_money_core::VoucherStatus;

        let dir_alice = temp_dir("app-k2-alice");
        let dir_bob = temp_dir("app-k2-bob");
        let dir_eve = temp_dir("app-k2-eve");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        let (mut svc_eve, _) =
            test_utils::setup_service_with_profile(&dir_eve, &ACTORS.charlie, "Eve", pw);
        svc_alice.unlock_session(pw, 600).unwrap();
        svc_bob.unlock_session(pw, 600).unwrap();
        svc_eve.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let eve_id_str = svc_eve
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let eve_identity = svc_eve
            .with_wallet_and_identity(|_, id| id.clone())
            .unwrap();

        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let std_uuid = std_def.immutable.identity.uuid;
        let mut standards = HashMap::new();
        standards.insert(std_uuid, std_toml.clone());

        let sk1 = SigningKey::from_bytes(&[61u8; 32]);
        let sk2 = SigningKey::from_bytes(&[62u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let anchor_nonce = [55u8; 32];
        let creator_mig_pub = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![(&sk1, p1), (&sk2, p2)]
                .into_iter()
                .map(|(sk, port)| ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", port),
                    read_url: format!("http://127.0.0.1:{}/d", port),
                    admin_pub: sk.verifying_key().to_bytes(),
                    lock_pub: sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                })
                .collect(),
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![SigningKey::from_bytes(&[98u8; 32])
                    .verifying_key()
                    .to_bytes()],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let tickets: Vec<GenesisTicket> = [&sk1, &sk2]
            .into_iter()
            .map(|sk| {
                let mut t = GenesisTicket {
                    anchor_id,
                    retain_until_quarter: retain_q,
                    issued_at: 1_760_000_000,
                    lock_pub: sk.verifying_key().to_bytes(),
                    sig: [0u8; 64],
                };
                t.sig = sk.sign(&t.digest()).to_bytes();
                t
            })
            .collect();
        svc_alice
            .register_l2_anchor_template_with_nonce(
                manifest.clone(),
                tickets,
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();
        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
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
        let local_id = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();

        // Snapshot Alice's folder before first transfer to forge a genuine conflicting transfer to Eve
        let dir_alice_clone = temp_dir("app-k2-alice-clone");
        copy_dir_recursive(&dir_alice, &dir_alice_clone);
        let mut svc_alice_clone =
            human_money_core::app_service::AppService::new(&dir_alice_clone).unwrap();
        let profile_folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
        svc_alice_clone
            .login(&profile_folder, pw, false, "test-id".into())
            .unwrap();

        let transfer_bob = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: local_id.clone(),
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        let transfer_eve = svc_alice_clone
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: eve_id_str,
                    sources: vec![SourceTransfer {
                        local_instance_id: local_id,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        let opened_eve =
            bundle_processor::open_and_verify_bundle(&eve_identity, &transfer_eve.bundle_bytes)
                .unwrap();
        let competing_entry = l2_gateway::build_voucher_lock_batch(
            &opened_eve.vouchers[0],
            0,
            &opened_eve.causal_outputs,
            opened_eve.root_mig_sig,
            DEFAULT_LOCK_POW_BASE_BITS,
        )
        .unwrap()
        .into_iter()
        .last()
        .unwrap();
        let competing_hex = hex::encode(competing_entry.to_wire_bytes().unwrap());

        // 1. Offline / unreachable committee MUST return RetryLater before touching Bob's store
        let state_bob = AppState::new(svc_bob);
        state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };
        let offline_pre = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &transfer_bob.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(matches!(
            offline_pre,
            PrecommitL2Verification::RetryLater { .. }
        ));

        // 2. Committee reporting a conflicting entry MUST return ConflictDetected and atomically Quarantine on commit
        let spawn_conflict_srv = |listener: tokio::net::TcpListener,
                                  sk: SigningKey,
                                  comp_hex: String,
                                  comp_entry: LockEntry| {
            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        break;
                    };
                    let mut buf = [0u8; 65_536];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                    let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
                    let mut receipt = LockReceipt {
                        kind: LockReceiptKind::ConflictSeen,
                        lock_pub: sk.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: comp_entry.slot,
                        child_lock: comp_entry.child_lock,
                        entry_hash: comp_entry.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: 1,
                        bucket_head: [9u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    receipt.sig = sk.sign(&receipt.digest()).to_bytes();
                    let resp_json = serde_json::to_string(&SyncLockBatchResponse {
                        status: "conflict".to_string(),
                        bucket: req.bucket,
                        retain_until_quarter: retain_q,
                        receipts: vec![receipt.clone()],
                        conflict: Some(SyncLockConflict {
                            entry_index: 0,
                            slot: comp_entry.slot,
                            receipt,
                            competing_entry: comp_hex.clone(),
                        }),
                    })
                    .unwrap();
                    let http = format!(
                        "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        resp_json.len(),
                        resp_json
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };

        let h1 = spawn_conflict_srv(l1, sk1, competing_hex.clone(), competing_entry.clone());
        let h2 = spawn_conflict_srv(l2, sk2, competing_hex, competing_entry);

        let conflict_pre = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &transfer_bob.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        let (qcs, comp_entries) = match conflict_pre {
            PrecommitL2Verification::ConflictDetected {
                quorum_certificates,
                competing_entries,
            } => (quorum_certificates, competing_entries),
            _ => panic!("Expected ConflictDetected"),
        };
        assert!(!comp_entries.is_empty());

        let evidence = human_money_core::ReceiveL2Evidence {
            quorum_certificates: &qcs,
            competing_entries: &comp_entries,
        };
        {
            let mut s = state_bob.service.lock().unwrap();
            s.receive_bundle_with_companions(
                &transfer_bob.bundle_bytes,
                &standards,
                None,
                Some(pw),
                false,
                Some(&evidence),
                |_, _| Ok(vec![]),
            )
            .unwrap();
            let vouchers = s
                .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
                .unwrap();
            assert_eq!(vouchers.len(), 1);
            assert!(
                matches!(vouchers[0].status, VoucherStatus::Quarantined { .. }),
                "K2: Conflicting anchored PoST voucher MUST commit atomically as Quarantined, never Active (got {:?})",
                vouchers[0].status
            );
        }

        h1.abort();
        h2.abort();
        drop(state_bob);
        drop(svc_alice);
        drop(svc_alice_clone);
        drop(svc_eve);
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_alice_clone);
        let _ = std::fs::remove_dir_all(dir_bob);
        let _ = std::fs::remove_dir_all(dir_eve);
    }

    #[tokio::test]
    async fn k1_precommit_rejects_invalid_competing_entry_and_unsettled_second_voucher() {
        let dir_alice = temp_dir("app-k1-alice");
        let dir_bob = temp_dir("app-k1-bob");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let sk1 = SigningKey::from_bytes(&[71u8; 32]);
        let sk2 = SigningKey::from_bytes(&[72u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let anchor_nonce = [73u8; 32];
        let creator_mig_pub = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![(&sk1, p1), (&sk2, p2)]
                .into_iter()
                .map(|(sk, port)| ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", port),
                    read_url: format!("http://127.0.0.1:{}/d", port),
                    admin_pub: sk.verifying_key().to_bytes(),
                    lock_pub: sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                })
                .collect(),
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![SigningKey::from_bytes(&[95u8; 32])
                    .verifying_key()
                    .to_bytes()],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let tickets: Vec<GenesisTicket> = [&sk1, &sk2]
            .into_iter()
            .map(|sk| {
                let mut t = GenesisTicket {
                    anchor_id,
                    retain_until_quarter: retain_q,
                    issued_at: 1_760_000_000,
                    lock_pub: sk.verifying_key().to_bytes(),
                    sig: [0u8; 64],
                };
                t.sig = sk.sign(&t.digest()).to_bytes();
                t
            })
            .collect();
        svc_alice
            .register_l2_anchor_template_with_nonce(
                manifest.clone(),
                tickets,
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();

        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id.clone()),
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
        let lid1 = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();

        // Clone before spend so we have a valid competing LockEntry later for the multi-voucher check
        let dir_clone = temp_dir("app-k1-alice-clone");
        copy_dir_recursive(&dir_alice, &dir_clone);
        let mut svc_clone = human_money_core::app_service::AppService::new(&dir_clone).unwrap();
        let folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
        svc_clone
            .login(&folder, pw, false, "test-id".into())
            .unwrap();

        // Issue a SECOND voucher on Alice ONLY (so voucher 2 goes unsettled when server 2 drops)
        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "30".into(),
                        ..Default::default()
                    },
                    layer2_anchor: Some(anchor_id),
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let lid2 = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()
            .into_iter()
            .find(|v| v.local_instance_id != lid1)
            .unwrap()
            .local_instance_id;

        let dir_alice_pre_single = temp_dir("app-k1-alice-single");
        copy_dir_recursive(&dir_alice, &dir_alice_pre_single);
        let mut svc_alice_single =
            human_money_core::app_service::AppService::new(&dir_alice_pre_single).unwrap();
        svc_alice_single
            .login(&folder, pw, false, "test-id".into())
            .unwrap();

        let single_bundle = svc_alice_single
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id.clone(),
                    sources: vec![SourceTransfer {
                        local_instance_id: lid1.clone(),
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &HashMap::from([(
                    "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
                    std_toml.clone(),
                )]),
                None,
                Some(pw),
            )
            .unwrap();

        let two_voucher_bundle = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id.clone(),
                    sources: vec![
                        SourceTransfer {
                            local_instance_id: lid1.clone(),
                            amount_to_send: "20".into(),
                        },
                        SourceTransfer {
                            local_instance_id: lid2.clone(),
                            amount_to_send: "15".into(),
                        },
                    ],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &HashMap::from([(
                    "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
                    std_toml.clone(),
                )]),
                None,
                Some(pw),
            )
            .unwrap();

        // Build a REAL competing LockEntry for voucher 1, plus a CORRUPTED signature copy
        let alt_bundle = svc_clone
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: ACTORS.charlie.identity.user_id.clone(),
                    sources: vec![SourceTransfer {
                        local_instance_id: lid1.clone(),
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &HashMap::from([("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), std_toml)]),
                None,
                Some(pw),
            )
            .unwrap();
        let opened_alt = bundle_processor::open_and_verify_bundle(
            &ACTORS.charlie.identity,
            &alt_bundle.bundle_bytes,
        )
        .unwrap();
        let valid_comp = l2_gateway::build_voucher_lock_batch(
            &opened_alt.vouchers[0],
            0,
            &opened_alt.causal_outputs,
            opened_alt.root_mig_sig,
            DEFAULT_LOCK_POW_BASE_BITS,
        )
        .unwrap()
        .into_iter()
        .last()
        .unwrap();
        let mut bogus_comp = valid_comp.clone();
        bogus_comp.causal_sig = [0x55; 64];

        let state_bob = AppState::new(svc_bob);
        state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };

        // Case A: Server 1 returns valid ConflictSeen receipt over `bogus_comp` (invalid causal_sig),
        // Server 2 returns HTTP 500 -> NO quorum -> MUST return RetryLater!
        let srv1_mode = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let srv1_mode_ref = srv1_mode.clone();
        let valid_comp_for_srv = valid_comp.clone();
        let bogus_comp_for_srv = bogus_comp.clone();
        let h1 = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = l1.accept().await else {
                    break;
                };
                let mut buf = [0u8; 65_536];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
                let mode = srv1_mode_ref.load(std::sync::atomic::Ordering::SeqCst);
                let chosen_entry = if mode == 0 {
                    &bogus_comp_for_srv
                } else {
                    &valid_comp_for_srv
                };
                let resp_json = if mode == 0
                    || req.entries.iter().any(|h| {
                        decode_hex_fixed::<
                            { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                        >(h)
                        .ok()
                        .and_then(|b| LockEntry::from_wire_bytes(&b).ok())
                        .is_some_and(|e| e.slot == chosen_entry.slot)
                    }) {
                    let mut receipt = LockReceipt {
                        kind: LockReceiptKind::ConflictSeen,
                        lock_pub: sk1.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: chosen_entry.slot,
                        child_lock: chosen_entry.child_lock,
                        entry_hash: chosen_entry.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: 1,
                        bucket_head: [9u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    receipt.sig = sk1.sign(&receipt.digest()).to_bytes();
                    serde_json::to_string(&SyncLockBatchResponse {
                        status: "conflict".to_string(),
                        bucket: req.bucket,
                        retain_until_quarter: retain_q,
                        receipts: vec![receipt.clone()],
                        conflict: Some(SyncLockConflict {
                            entry_index: 0,
                            slot: chosen_entry.slot,
                            receipt,
                            competing_entry: chosen_entry.to_wire_hex().unwrap(),
                        }),
                    })
                    .unwrap()
                } else {
                    // Voucher 2 on server 1 returns FirstWrite, while server 2 fails -> Voucher 2 stays Unsettled!
                    let b0 = decode_hex_fixed::<
                        { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                    >(&req.entries[0])
                    .unwrap();
                    let e0 = LockEntry::from_wire_bytes(&b0).unwrap();
                    let mut receipt = LockReceipt {
                        kind: LockReceiptKind::FirstWrite,
                        lock_pub: sk1.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: e0.slot,
                        child_lock: e0.child_lock,
                        entry_hash: e0.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: 1,
                        bucket_head: [8u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    receipt.sig = sk1.sign(&receipt.digest()).to_bytes();
                    serde_json::to_string(&SyncLockBatchResponse {
                        status: "ok".to_string(),
                        bucket: req.bucket,
                        retain_until_quarter: retain_q,
                        receipts: vec![receipt],
                        conflict: None,
                    })
                    .unwrap()
                };
                let status_line = if resp_json.contains("\"conflict\"") {
                    "HTTP/1.1 409 Conflict"
                } else {
                    "HTTP/1.1 200 OK"
                };
                let http = format!(
                    "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    status_line,
                    resp_json.len(),
                    resp_json
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        });
        let h2 = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = l2.accept().await else {
                    break;
                };
                let mut buf = [0u8; 65_536];
                let _ = stream.read(&mut buf).await;
                let http = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 5\r\n\r\nerror";
                let _ = stream.write_all(http.as_bytes()).await;
            }
        });

        let res_a = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &single_bundle.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(
            matches!(res_a, PrecommitL2Verification::RetryLater { .. }),
            "K1: Cryptographically invalid competing LockEntry without quorum must return RetryLater, got {:?}",
            res_a
        );

        // Case B: Voucher 1 has VALID conflict, but Voucher 2 lacks Q=2 quorum -> MUST STILL return RetryLater!
        srv1_mode.store(1, std::sync::atomic::Ordering::SeqCst);
        let res_b = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &two_voucher_bundle.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(
            matches!(res_b, PrecommitL2Verification::RetryLater { .. }),
            "K1: Mixed bundle with VerifiedConflict on voucher A and Unsettled voucher B must return RetryLater, got {:?}",
            res_b
        );

        h1.abort();
        h2.abort();
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_alice_pre_single);
        let _ = std::fs::remove_dir_all(dir_clone);
        let _ = std::fs::remove_dir_all(dir_bob);
    }

    #[tokio::test]
    async fn h7_precommit_epoch_frozen_409_upgrades_manifest_and_retries() {
        use human_money_core::humoco_proto::rotation::{
            CommitteeSignature, ImportAck, MigrationCert, MigrationProposal, ProposalInitiator,
        };

        let dir_alice = temp_dir("app-h7-alice");
        let dir_bob = temp_dir("app-h7-bob");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );

        // Old server s1 (will return 409 epoch_frozen with rev2 manifest) + successor server s2
        let sk1 = SigningKey::from_bytes(&[81u8; 32]);
        let sk2 = SigningKey::from_bytes(&[82u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let anchor_nonce = [83u8; 32];
        let creator_mig_sk = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
            })
            .unwrap()
            .unwrap();
        let creator_mig_pub = creator_mig_sk.verifying_key().to_bytes();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let srv1_entry = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p1),
            read_url: format!("http://127.0.0.1:{}/d", p1),
            admin_pub: sk1.verifying_key().to_bytes(),
            lock_pub: sk1.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let srv2_entry = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p2),
            read_url: format!("http://127.0.0.1:{}/d", p2),
            admin_pub: sk2.verifying_key().to_bytes(),
            lock_pub: sk2.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let rev1 = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![srv1_entry],
            successors: vec![srv2_entry.clone()],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = rev1.rev1_anchor_id().unwrap();
        let mut t1 = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: 1_760_000_000,
            lock_pub: sk1.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        t1.sig = sk1.sign(&t1.digest()).to_bytes();

        // Build cryptographically valid Rev-2 manifest replacing s1 with s2
        let mut proposal = MigrationProposal {
            anchor_id,
            from_rev: 1,
            to_rev: 2,
            new_threshold: 1,
            new_servers: vec![srv2_entry],
            new_successors: vec![],
            retain_until_quarter: retain_q,
            proposed_at: 1_760_050_000,
            initiator: ProposalInitiator::Creator,
            proposer_sig: None,
            guarantor_cosigs: vec![],
        };
        proposal.proposer_sig = Some(
            creator_mig_sk
                .sign(&proposal.core_digest().unwrap())
                .to_bytes(),
        );
        let imported_state_hash = [0x33u8; 32];
        let prop_hash = proposal.core_digest().unwrap();
        let mut ack2 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash,
            retain_until_quarter: retain_q,
            lock_pub: sk2.verifying_key().to_bytes(),
            acked_at: 1_760_050_100,
            sig: [0u8; 64],
        };
        ack2.sig = sk2.sign(&ack2.digest()).to_bytes();
        let c_digest =
            MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &imported_state_hash);
        let commit_sig = CommitteeSignature {
            lock_pub: sk1.verifying_key().to_bytes(),
            committed_at: 1_760_050_200,
            sig: sk1.sign(&c_digest).to_bytes(),
        };
        let cert = MigrationCert {
            anchor_id,
            genesis_manifest: Some(Box::new(rev1.clone())),
            proposal,
            imported_state_hash,
            import_acks: vec![ack2],
            commit_sigs: vec![commit_sig],
        };
        let (rev2, _) = cert.verify_step(&anchor_id, &rev1).unwrap();

        svc_alice
            .register_l2_anchor_template_with_nonce(
                rev1.clone(),
                vec![t1],
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();
        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
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
        let lid = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        let bundle_res = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: lid,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &HashMap::from([("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), std_toml)]),
                None,
                Some(pw),
            )
            .unwrap();

        let rev2_for_s1 = rev2.clone();
        let h1 = tokio::spawn(async move {
            let Ok((mut stream, _)) = l1.accept().await else {
                return;
            };
            let mut buf = [0u8; 65_536];
            let _ = stream.read(&mut buf).await;
            let body = serde_json::json!({
                "error": "epoch_frozen",
                "latest_manifest": rev2_for_s1,
            })
            .to_string();
            let http = format!(
                "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(http.as_bytes()).await;
        });

        let h2 = tokio::spawn(async move {
            let Ok((mut stream, _)) = l2.accept().await else {
                return;
            };
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let receipts: Vec<LockReceipt> = req
                .entries
                .iter()
                .enumerate()
                .map(|(i, hex_str)| {
                    let b = decode_hex_fixed::<
                        { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                    >(hex_str)
                    .unwrap();
                    let entry = LockEntry::from_wire_bytes(&b).unwrap();
                    let mut r = LockReceipt {
                        kind: LockReceiptKind::FirstWrite,
                        lock_pub: sk2.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: entry.slot,
                        child_lock: entry.child_lock,
                        entry_hash: entry.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: (i as u64) + 1,
                        bucket_head: [7u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    r.sig = sk2.sign(&r.digest()).to_bytes();
                    r
                })
                .collect();
            let resp = serde_json::to_string(&SyncLockBatchResponse {
                status: "ok".to_string(),
                bucket: req.bucket,
                retain_until_quarter: retain_q,
                receipts,
                conflict: None,
            })
            .unwrap();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                resp.len(),
                resp
            );
            let _ = stream.write_all(http.as_bytes()).await;
        });

        let state_bob = AppState::new(svc_bob);
        state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };

        let upgraded = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &bundle_res.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(
            matches!(upgraded, PrecommitL2Verification::CleanQuorum { .. }),
            "H7: Precommit must upgrade rev-1 -> rev-2 on 409 epoch_frozen and succeed on s2, got {:?}",
            upgraded
        );

        let stored_rev = state_bob
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&hex::encode(anchor_id))
                    .map(|m| m.revision)
            })
            .unwrap();
        assert_eq!(
            stored_rev,
            Some(2),
            "Upgraded rev-2 manifest must be persisted"
        );

        h1.abort();
        h2.abort();
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_bob);
    }

    fn make_signed_probe_for_test(
        admin_sk: &SigningKey,
        lock_sk: &SigningKey,
        write_url: &str,
        read_url: &str,
        retain_q: u16,
    ) -> sync_client::FrontendLockServerProbeResult {
        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
        let msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex,
            &lock_hex,
            write_url,
            read_url,
            1_760_000_000,
        );
        let sig = hex::encode(admin_sk.sign(msg.as_bytes()).to_bytes());
        sync_client::FrontendLockServerProbeResult {
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            drop_pub: admin_hex.clone(),
            admin_pub: admin_hex.clone(),
            lock_pub: lock_hex.clone(),
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
            max_inodes: 50000,
            probed_at_secs: 1_760_000_000,
            delegation: Some(sync_client::SyncLockDelegationDto {
                admin_pub: admin_hex,
                lock_pub: lock_hex,
                write_url: write_url.to_string(),
                read_url: read_url.to_string(),
                issued_at: 1_760_000_000,
                sig,
            }),
            error: None,
        }
    }

    #[tokio::test]
    async fn test_s04_f2_already_in_history_replay_never_fabricates_confirmed() {
        let dir_alice = temp_dir("s04-f2-alice");
        let dir_bob = temp_dir("s04-f2-bob");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        svc_alice.unlock_session(pw, 600).unwrap();
        svc_bob.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "50".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let lid = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        let transfer = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: lid,
                        amount_to_send: "10".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        // Bob receives the N=0 bundle directly into wallet history (NOT via checkout_purchase)
        svc_bob
            .receive_bundle_with_companions(
                &transfer.bundle_bytes,
                &standards,
                None,
                Some(pw),
                false,
                None,
                |_, _| Ok(vec![]),
            )
            .unwrap();

        let state_bob = AppState::new(svc_bob);
        state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };
        let replay = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-replay-hist",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(replay.is_replay);
        assert_ne!(
            replay.verdict.outcome, "Confirmed",
            "S04-F2: already_in_history N=0 replay must NEVER fabricate Confirmed"
        );
        assert_eq!(replay.verdict.outcome, "OfflineOnly");
        assert!(!replay.verdict.is_goods_release_authorized);

        // S04-F2: if the already-received voucher is Quarantined, replay MUST report Quarantined
        {
            let mut s = state_bob.service.lock().unwrap();
            let (wallet, _) = s.get_unlocked_mut_for_test();
            for inst in wallet.voucher_store.vouchers.values_mut() {
                inst.status = human_money_core::VoucherStatus::Quarantined {
                    reason: "Upstream conflict".into(),
                };
            }
        }
        let replay_q = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-replay-quarantined",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(replay_q.is_replay);
        assert_eq!(
            replay_q.verdict.outcome,
            "Quarantined:BundleAlreadyProcessedQuarantined"
        );
        assert!(!replay_q.verdict.is_goods_release_authorized);

        // S04-F7: stale SessionToken generation MUST be rejected immediately
        let stale_session = crate::session::SessionToken {
            generation: bob_session.generation.saturating_add(99),
            owner_did: bob_session.owner_did.clone(),
        };
        let stale_err = checkout_bundle(
            &state_bob,
            &stale_session,
            "pos-stale",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap_err();
        assert!(
            stale_err.message.contains("session changed"),
            "S04-F7: stale session token must be rejected, got: {:?}",
            stale_err
        );

        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_bob);
    }

    #[tokio::test]
    async fn test_s04_f3_f8_f9_trusted_lock_keys_delegation_binding_and_lucke03_isolation() {
        let dir_bob = temp_dir("s04-f3-f8-f9-bob");
        let pw = "test-password";
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        svc_bob.unlock_session(pw, 600).unwrap();

        let retain_q = encode_quarter(2035, 4).unwrap();
        let sk_unsigned = SigningKey::from_bytes(&[181u8; 32]);
        let sk_contact = SigningKey::from_bytes(&[182u8; 32]);
        let sk_offender_bridge = SigningKey::from_bytes(&[183u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[184u8; 32]);

        let url_unsigned = "https://unsigned.example/sync.php";
        let url_contact = "https://hop1-contact.example/sync.php";
        let url_bridge = "https://hop2-via-offender.example/sync.php";

        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![
                ServerEntry {
                    write_url: url_unsigned.to_string(),
                    read_url: "https://unsigned.example/d".to_string(),
                    admin_pub: sk_unsigned.verifying_key().to_bytes(),
                    lock_pub: sk_unsigned.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Creator,
                },
                ServerEntry {
                    write_url: url_contact.to_string(),
                    read_url: "https://hop1-contact.example/d".to_string(),
                    admin_pub: sk_contact.verifying_key().to_bytes(),
                    lock_pub: sk_contact.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Creator,
                },
                ServerEntry {
                    write_url: url_bridge.to_string(),
                    read_url: "https://hop2-via-offender.example/d".to_string(),
                    admin_pub: sk_offender_bridge.verifying_key().to_bytes(),
                    lock_pub: sk_offender_bridge.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
            ],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![[9u8; 32]],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };

        // Mark Alice as KnownOffender in Bob's wallet proof_store (S04-F8 bridge test)
        let alice_did = ACTORS.alice.identity.user_id.clone();
        let charlie_did = ACTORS.charlie.identity.user_id.clone();
        let dave_did = ACTORS.david.identity.user_id.clone();
        {
            use human_money_core::models::conflict::{
                ConflictRole, ProofOfDoubleSpend, ProofStoreEntry,
            };
            let bob_uid = svc_bob
                .with_wallet(|w| w.get_user_id().to_string())
                .unwrap();
            let (wallet, _) = svc_bob.get_unlocked_mut_for_test();
            wallet.proof_store.proofs.insert(
                "proof-s04-f8".to_string(),
                ProofStoreEntry {
                    proof: ProofOfDoubleSpend {
                        proof_id: "proof-s04-f8".to_string(),
                        offender_id: alice_did.clone(),
                        suspected_identity: None,
                        fork_point_prev_hash: "00".repeat(32),
                        conflicting_transactions: Vec::new(),
                        fingerprints: Vec::new(),
                        victim_witnesses: Vec::new(),
                        deletable_at: "2099-01-01T00:00:00Z".to_string(),
                        reporter_id: bob_uid,
                        report_timestamp: "2026-01-01T00:00:00Z".to_string(),
                        reporter_signature: "sig".to_string(),
                        affected_voucher_name: None,
                        voucher_standard_uuid: None,
                        resolutions: None,
                        non_redeemable_test_voucher: false,
                    },
                    local_override: false,
                    local_note: None,
                    conflict_role: ConflictRole::default(),
                },
            );
        }

        let state_bob = AppState::new(svc_bob);
        let session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };

        // Security policy pins ONLY url_unsigned (whose probe lacks a signed delegation!)
        let mut policy = SecurityPolicy::default();
        policy.market_selected_syncs.push(
            humoco_market::DropDescriptor {
                write_url: url_unsigned.to_string(),
                read_url: "https://unsigned.example/d".to_string(),
                slot_id: None,
                drop_pub: hex::encode(sk_unsigned.verifying_key().as_bytes()),
            }
            .into(),
        );

        let mut lc = InternalLifecycleState::default();
        // S04-F3: probe with delegation: None must NOT enter VerifiedServerBinding
        let mut unsigned_probe = make_signed_probe_for_test(
            &sk_unsigned,
            &sk_unsigned,
            url_unsigned,
            "https://unsigned.example/d",
            retain_q,
        );
        unsigned_probe.delegation = None;
        lc.l2_probe_cache
            .insert(unsigned_probe.lock_pub.clone(), unsigned_probe);
        // S04-F9: valid signed probe for Hop-1 contact Charlie
        let p_contact = make_signed_probe_for_test(
            &sk_contact,
            &sk_contact,
            url_contact,
            "https://hop1-contact.example/d",
            retain_q,
        );
        lc.l2_probe_cache
            .insert(p_contact.lock_pub.clone(), p_contact);
        // S04-F8: valid signed probe for Hop-2 contact Dave (reachable ONLY through KnownOffender Alice!)
        let p_bridge = make_signed_probe_for_test(
            &sk_offender_bridge,
            &sk_offender_bridge,
            url_bridge,
            "https://hop2-via-offender.example/d",
            retain_q,
        );
        lc.l2_probe_cache
            .insert(p_bridge.lock_pub.clone(), p_bridge);

        // Populate AddressBook (Charlie trusted at Hop-1; Dave untrusted directly, reachable only via Alice)
        let mut book = crate::models::FrontendAddressBook::default();
        book.contacts.insert(
            charlie_did.clone(),
            crate::models::FrontendContact {
                did: charlie_did.clone(),
                profile: crate::models::FrontendUserProfile::default(),
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![humoco_market::DropDescriptor {
                    write_url: url_contact.to_string(),
                    read_url: "https://hop1-contact.example/d".to_string(),
                    slot_id: None,
                    drop_pub: hex::encode(sk_contact.verifying_key().as_bytes()),
                }
                .into()],
            },
        );
        book.contacts.insert(
            dave_did.clone(),
            crate::models::FrontendContact {
                did: dave_did.clone(),
                profile: crate::models::FrontendUserProfile::default(),
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: None,
                desired_hops: None,
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![humoco_market::DropDescriptor {
                    write_url: url_bridge.to_string(),
                    read_url: "https://hop2-via-offender.example/d".to_string(),
                    slot_id: None,
                    drop_pub: hex::encode(sk_offender_bridge.verifying_key().as_bytes()),
                }
                .into()],
            },
        );

        // Populate WoT graph: Bob -> Alice (KnownOffender, level 2) -> Dave (level 2)
        let bob_can = canonicalize_did(&ACTORS.bob.identity.user_id).unwrap();
        let alice_can = canonicalize_did(&alice_did).unwrap();
        let dave_can = canonicalize_did(&dave_did).unwrap();
        let mut reg =
            humoco_wot::TrustRegister::new(&bob_can, humoco_wot::WotPolicy::default()).unwrap();
        reg.set_contact_trust(&alice_can, 2, 2, 1_760_000_000)
            .unwrap();
        let mut alice_reg =
            humoco_wot::TrustRegister::new(&alice_can, humoco_wot::WotPolicy::default()).unwrap();
        alice_reg
            .set_contact_trust(&dave_can, 2, 2, 1_760_000_000)
            .unwrap();
        let alice_signer = crate::state::DidWotSigner::from_identity(&ACTORS.alice.identity);
        alice_reg
            .ensure_own_list_signed(&alice_signer, 1_760_000_000)
            .unwrap();
        if let Some(ref signed) = alice_reg.own_signed_list {
            reg.raw_list_store.insert(alice_can, signed.clone());
            reg.invalidate_graph_cache();
        }
        {
            let s = state_bob.service.lock().unwrap();
            state_bob.write_cache_slot_for_service(&s, |c| &mut c.contacts, Some(book));
            state_bob.write_cache_slot_for_service(&s, |c| &mut c.wot, Some(reg));
        }

        let client = sync_client::create_sync_http_client();
        let (base_trusted, independently_pinned) = resolve_trusted_lock_keys(
            &state_bob,
            &session,
            &policy,
            &lc,
            &client,
            Some(&alice_did),
            Some(&manifest),
            Some(pw),
        )
        .await
        .unwrap();

        assert!(
            !base_trusted.contains(&sk_unsigned.verifying_key().to_bytes()),
            "S04-F3: unsigned cached probe (delegation: None) must NOT be trusted"
        );
        assert!(
            base_trusted.contains(&sk_contact.verifying_key().to_bytes()),
            "S04-F9: Hop-1 contact sync must be in base_trusted_keys"
        );
        assert!(
            !independently_pinned.contains(&sk_contact.verifying_key().to_bytes()),
            "S04-F9: transitive Hop-1 contact sync must NOT enter independently_pinned_creator_keys"
        );
        assert!(
            !base_trusted.contains(&sk_offender_bridge.verifying_key().to_bytes()),
            "S04-F8: Hop-2 contact reachable only via KnownOffender bridge must NOT be trusted"
        );

        let _ = std::fs::remove_dir_all(dir_bob);
    }

    #[tokio::test]
    async fn test_s04_f1_precommit_rejects_wrong_nonce_and_revoked_server_and_uses_default_pow() {
        let dir_alice = temp_dir("s04-f1-pre-alice");
        let dir_alice_clone = temp_dir("s04-f1-pre-clone");
        let dir_bob = temp_dir("s04-f1-pre-bob");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        svc_alice.unlock_session(pw, 600).unwrap();
        svc_bob.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

        let sk1 = SigningKey::from_bytes(&[191u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let anchor_nonce = [192u8; 32];
        let creator_mig_pub = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let retain_q = encode_quarter(2035, 4).unwrap();
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
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let mut t1 = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: 1_760_000_000,
            lock_pub: sk1.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        t1.sig = sk1.sign(&t1.digest()).to_bytes();
        svc_alice
            .register_l2_anchor_template_with_nonce(
                manifest.clone(),
                vec![t1],
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();
        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
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
        let lid = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();

        copy_dir_recursive(&dir_alice, &dir_alice_clone);
        let mut svc_clone =
            human_money_core::app_service::AppService::new(&dir_alice_clone).unwrap();
        let folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
        svc_clone
            .login(&folder, pw, false, "test-id".into())
            .unwrap();
        let alt_bundle = svc_clone
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: ACTORS.charlie.identity.user_id.clone(),
                    sources: vec![SourceTransfer {
                        local_instance_id: lid.clone(),
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();
        let opened_alt = bundle_processor::open_and_verify_bundle(
            &ACTORS.charlie.identity,
            &alt_bundle.bundle_bytes,
        )
        .unwrap();
        let valid_comp = l2_gateway::build_voucher_lock_batch(
            &opened_alt.vouchers[0],
            0,
            &opened_alt.causal_outputs,
            opened_alt.root_mig_sig,
            DEFAULT_LOCK_POW_BASE_BITS,
        )
        .unwrap()
        .into_iter()
        .last()
        .unwrap();

        let transfer_bob = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: lid,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        // Mode 0: wrong recipient_nonce; Mode 1: valid FirstWrite (tested against revoked offender); Mode 2: ConflictSeen with 8-bit competing entry
        let mode = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let mode_ref = mode.clone();
        let comp_for_srv = valid_comp.clone();
        let sk1_for_srv = sk1.clone();
        let h1 = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = l1.accept().await else {
                    break;
                };
                let mut buf = [0u8; 65_536];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
                let m = mode_ref.load(std::sync::atomic::Ordering::SeqCst);
                let (status_line, resp_json) = if m == 2 {
                    let mut r = LockReceipt {
                        kind: LockReceiptKind::ConflictSeen,
                        lock_pub: sk1_for_srv.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: comp_for_srv.slot,
                        child_lock: comp_for_srv.child_lock,
                        entry_hash: comp_for_srv.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: 1,
                        bucket_head: [9u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    r.sig = sk1_for_srv.sign(&r.digest()).to_bytes();
                    let body = serde_json::to_string(&SyncLockBatchResponse {
                        status: "conflict".into(),
                        bucket: req.bucket,
                        retain_until_quarter: retain_q,
                        receipts: vec![],
                        conflict: Some(SyncLockConflict {
                            entry_index: 0,
                            slot: comp_for_srv.slot,
                            receipt: r,
                            competing_entry: comp_for_srv.to_wire_hex().unwrap(),
                        }),
                    })
                    .unwrap();
                    ("HTTP/1.1 409 Conflict", body)
                } else {
                    let receipts: Vec<LockReceipt> = req
                        .entries
                        .iter()
                        .enumerate()
                        .map(|(i, h)| {
                            let b = decode_hex_fixed::<
                                { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                            >(h)
                            .unwrap();
                            let e = LockEntry::from_wire_bytes(&b).unwrap();
                            let mut r = LockReceipt {
                                kind: LockReceiptKind::FirstWrite,
                                lock_pub: sk1_for_srv.verifying_key().to_bytes(),
                                bucket: req.bucket,
                                slot: e.slot,
                                child_lock: e.child_lock,
                                entry_hash: e.entry_hash().unwrap(),
                                retain_until_quarter: retain_q,
                                write_seq: (i as u64) + 1,
                                bucket_head: [8u8; 32],
                                recipient_nonce: if m == 0 {
                                    [0xee; 32]
                                } else {
                                    req.recipient_nonce
                                },
                                sig: [0u8; 64],
                            };
                            r.sig = sk1_for_srv.sign(&r.digest()).to_bytes();
                            r
                        })
                        .collect();
                    let body = serde_json::to_string(&SyncLockBatchResponse {
                        status: "ok".into(),
                        bucket: req.bucket,
                        retain_until_quarter: retain_q,
                        receipts,
                        conflict: None,
                    })
                    .unwrap();
                    ("HTTP/1.1 200 OK", body)
                };
                let http = format!(
                    "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    status_line,
                    resp_json.len(),
                    resp_json
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        });

        let state_bob = AppState::new(svc_bob);
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };
        let mut lc = InternalLifecycleState::default();
        let mut probe = make_signed_probe_for_test(
            &sk1,
            &sk1,
            &format!("http://127.0.0.1:{}/sync.php", p1),
            &format!("http://127.0.0.1:{}/d", p1),
            retain_q,
        );
        probe.pow_base_bits = 18;
        lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
        state_bob.set_cached_internal_lifecycle(Some(lc));

        // 1. Wrong nonce MUST NOT yield CleanQuorum
        let res_wrong_nonce = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &transfer_bob.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(
            matches!(res_wrong_nonce, PrecommitL2Verification::RetryLater { .. }),
            "S04-F1: wrong recipient_nonce must return RetryLater, got {:?}",
            res_wrong_nonce
        );

        // 2. Valid FirstWrite from revoked server MUST NOT yield CleanQuorum
        mode.store(1, std::sync::atomic::Ordering::SeqCst);
        {
            let mut offenders = humoco_wot::ServerOffenderList::new();
            offenders
                .revoked_lock_pubs
                .insert(sk1.verifying_key().to_bytes());
            let s = state_bob.service.lock().unwrap();
            state_bob.write_cache_slot_for_service(
                &s,
                |c| &mut c.server_offenders,
                Some(offenders),
            );
        }
        let res_revoked = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &transfer_bob.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(
            matches!(res_revoked, PrecommitL2Verification::RetryLater { .. }),
            "S04-F1: revoked server receipt must return RetryLater, got {:?}",
            res_revoked
        );

        // 3. Historical 8-bit competing entry MUST verify despite live probe pow_base_bits = 18
        mode.store(2, std::sync::atomic::Ordering::SeqCst);
        let res_conflict = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &transfer_bob.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        match res_conflict {
            PrecommitL2Verification::ConflictDetected {
                competing_entries, ..
            } => {
                assert_eq!(competing_entries.len(), 1);
                assert_eq!(competing_entries[0].1, DEFAULT_LOCK_POW_BASE_BITS);
            }
            other => panic!(
                "S04-F1: expected ConflictDetected with DEFAULT_LOCK_POW_BASE_BITS despite live PoW=18, got {:?}",
                other
            ),
        }

        h1.abort();
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_alice_clone);
        let _ = std::fs::remove_dir_all(dir_bob);
    }

    #[tokio::test]
    async fn test_s04_f3_live_equivocation_revokes_admin_sibling_before_checkout_evaluation() {
        let dir_alice = temp_dir("s04-f3-equiv-alice");
        let dir_bob = temp_dir("s04-f3-equiv-bob");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        svc_alice.unlock_session(pw, 600).unwrap();
        svc_bob.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

        // N=3, Q=2: Server 1 & sibling Server 2 share admin_pub A; Server 3 has independent admin_pub B.
        // Even though Server 2 + Server 3 return clean FirstWrite (meeting Q=2), Server 1's live
        // equivocation MUST revoke admin_pub A and prune sibling Server 2 before evaluate_bundle_checkout
        // so only 1 trusted server (< N - Q + 1 = 2) remains and verdict is NOT Confirmed!
        let admin_sk = SigningKey::from_bytes(&[201u8; 32]);
        let sk_lock1 = SigningKey::from_bytes(&[202u8; 32]);
        let sk_lock2 = SigningKey::from_bytes(&[203u8; 32]);
        let sk_lock3 = SigningKey::from_bytes(&[205u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p3 = l3.local_addr().unwrap().port();

        let anchor_nonce = [204u8; 32];
        let creator_mig_pub = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![
                ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", p1),
                    read_url: format!("http://127.0.0.1:{}/d", p1),
                    admin_pub: admin_sk.verifying_key().to_bytes(),
                    lock_pub: sk_lock1.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
                ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", p2),
                    read_url: format!("http://127.0.0.1:{}/d", p2),
                    admin_pub: admin_sk.verifying_key().to_bytes(),
                    lock_pub: sk_lock2.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
                ServerEntry {
                    write_url: format!("http://127.0.0.1:{}/sync.php", p3),
                    read_url: format!("http://127.0.0.1:{}/d", p3),
                    admin_pub: sk_lock3.verifying_key().to_bytes(),
                    lock_pub: sk_lock3.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
            ],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![[9u8; 32]],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let tickets: Vec<GenesisTicket> = [&sk_lock1, &sk_lock2, &sk_lock3]
            .into_iter()
            .map(|sk| {
                let mut t = GenesisTicket {
                    anchor_id,
                    retain_until_quarter: retain_q,
                    issued_at: 1_760_000_000,
                    lock_pub: sk.verifying_key().to_bytes(),
                    sig: [0u8; 64],
                };
                t.sig = sk.sign(&t.digest()).to_bytes();
                t
            })
            .collect();
        svc_alice
            .register_l2_anchor_template_with_nonce(
                manifest.clone(),
                tickets,
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();
        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
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
        let lid = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        let transfer = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: lid,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        // Server 1 equivocates: signs TWO conflicting FirstWrite receipts for the same slot!
        let sk1_srv = sk_lock1.clone();
        let h1 = tokio::spawn(async move {
            if let Ok((mut stream, _)) = l1.accept().await {
                let mut buf = [0u8; 65_536];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
                let b = decode_hex_fixed::<
                    { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                >(req.entries.last().unwrap())
                .unwrap();
                let entry = LockEntry::from_wire_bytes(&b).unwrap();
                let mut fw_a = LockReceipt {
                    kind: LockReceiptKind::FirstWrite,
                    lock_pub: sk1_srv.verifying_key().to_bytes(),
                    bucket: req.bucket,
                    slot: entry.slot,
                    child_lock: entry.child_lock,
                    entry_hash: entry.entry_hash().unwrap(),
                    retain_until_quarter: retain_q,
                    write_seq: 1,
                    bucket_head: [1u8; 32],
                    recipient_nonce: req.recipient_nonce,
                    sig: [0u8; 64],
                };
                fw_a.sig = sk1_srv.sign(&fw_a.digest()).to_bytes();

                let mut fw_b = fw_a.clone();
                fw_b.child_lock = [0x77; 32];
                fw_b.entry_hash = [0x88; 32];
                fw_b.write_seq = 2;
                fw_b.sig = sk1_srv.sign(&fw_b.digest()).to_bytes();
                let body = serde_json::to_string(&SyncLockBatchResponse {
                    status: "ok".into(),
                    bucket: req.bucket,
                    retain_until_quarter: retain_q,
                    receipts: vec![fw_a, fw_b],
                    conflict: None,
                })
                .unwrap();
                let http = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        });
        let spawn_clean_fw = |listener: tokio::net::TcpListener, sk: SigningKey| {
            tokio::spawn(async move {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let mut buf = [0u8; 65_536];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                    let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
                    let receipts: Vec<LockReceipt> = req
                        .entries
                        .iter()
                        .enumerate()
                        .map(|(i, h)| {
                            let b = decode_hex_fixed::<
                                { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                            >(h)
                            .unwrap();
                            let e = LockEntry::from_wire_bytes(&b).unwrap();
                            let mut r = LockReceipt {
                                kind: LockReceiptKind::FirstWrite,
                                lock_pub: sk.verifying_key().to_bytes(),
                                bucket: req.bucket,
                                slot: e.slot,
                                child_lock: e.child_lock,
                                entry_hash: e.entry_hash().unwrap(),
                                retain_until_quarter: retain_q,
                                write_seq: (i as u64) + 1,
                                bucket_head: [3u8; 32],
                                recipient_nonce: req.recipient_nonce,
                                sig: [0u8; 64],
                            };
                            r.sig = sk.sign(&r.digest()).to_bytes();
                            r
                        })
                        .collect();
                    let body = serde_json::to_string(&SyncLockBatchResponse {
                        status: "ok".into(),
                        bucket: req.bucket,
                        retain_until_quarter: retain_q,
                        receipts,
                        conflict: None,
                    })
                    .unwrap();
                    let http = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };
        let h2 = spawn_clean_fw(l2, sk_lock2.clone());
        let h3 = spawn_clean_fw(l3, sk_lock3.clone());

        let state_bob = AppState::new(svc_bob);
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };
        let mut policy = SecurityPolicy::default();
        let mut lc = InternalLifecycleState::default();
        for (port, adm_sk, lock_sk) in [
            (p1, &admin_sk, &sk_lock1),
            (p2, &admin_sk, &sk_lock2),
            (p3, &sk_lock3, &sk_lock3),
        ] {
            let wurl = format!("http://127.0.0.1:{}/sync.php", port);
            let rurl = format!("http://127.0.0.1:{}/d", port);
            policy.public_syncs.push(crate::settings::StoredPublicSync {
                write_url: wurl.clone(),
                read_url: rurl.clone(),
                drop_pub: hex::encode(adm_sk.verifying_key().as_bytes()),
                register_url: None,
                access_pubkey: "aa".repeat(32),
                enabled: true,
                authorized_channels: vec!["lock".into()],
                status: "active".into(),
                sunset_at: None,
                revocation_reason: None,
                replacement: None,
            });
            let probe = make_signed_probe_for_test(adm_sk, lock_sk, &wurl, &rurl, retain_q);
            lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
        }
        state_bob.set_cached_security_policy(Some(policy));
        state_bob.set_cached_internal_lifecycle(Some(lc));

        let res = checkout_bundle(
            &state_bob,
            &bob_session,
            "pos-equiv-sibling",
            &transfer.bundle_bytes,
            &standards,
            false,
            Some(pw),
        )
        .await
        .unwrap();
        assert_ne!(
            res.verdict.outcome, "Confirmed",
            "S04-F3: pruning equivocating admin_pub sibling must drop trusted count below N-Q+1=2 and prevent Confirmed"
        );

        // Verify sibling lock_pub2 is now revoked via admin_pub A in ServerOffenderList!
        let offenders = {
            let mut s = state_bob.service.lock().unwrap();
            state_bob
                .get_or_load_server_offender_list(&mut s, Some(pw))
                .unwrap()
        };
        assert!(
            crate::l2_lifecycle::is_server_key_revoked(
                &offenders,
                &sk_lock2.verifying_key().to_bytes(),
                Some(&admin_sk.verifying_key().to_bytes()),
            ),
            "S04-F3: ADR-10 equivocation must revoke shared admin_pub so sibling lock_pub2 is banned"
        );

        h1.abort();
        h2.abort();
        h3.abort();
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_bob);
    }

    #[tokio::test]
    async fn test_s04_f6_inline_less_409_epoch_frozen_triggers_discovery() {
        use human_money_core::humoco_proto::rotation::{
            CommitteeSignature, ImportAck, MigrationCert, MigrationProposal, ProposalInitiator,
        };

        let dir_alice = temp_dir("s04-f6-disc-alice");
        let dir_bob = temp_dir("s04-f6-disc-bob");
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
        svc_alice.unlock_session(pw, 600).unwrap();
        svc_bob.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

        let sk1 = SigningKey::from_bytes(&[211u8; 32]);
        let sk2 = SigningKey::from_bytes(&[212u8; 32]);
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let l_disc = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p_disc = l_disc.local_addr().unwrap().port();

        let anchor_nonce = [213u8; 32];
        let creator_mig_sk = svc_alice
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
            })
            .unwrap()
            .unwrap();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let srv1 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p1),
            read_url: format!("http://127.0.0.1:{}/d", p1),
            admin_pub: sk1.verifying_key().to_bytes(),
            lock_pub: sk1.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let srv2 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p2),
            read_url: format!("http://127.0.0.1:{}/d", p2),
            admin_pub: sk2.verifying_key().to_bytes(),
            lock_pub: sk2.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let rev1 = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![srv1],
            successors: vec![srv2.clone()],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = rev1.rev1_anchor_id().unwrap();
        let mut t1 = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: 1_760_000_000,
            lock_pub: sk1.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        t1.sig = sk1.sign(&t1.digest()).to_bytes();

        let mut proposal = MigrationProposal {
            anchor_id,
            from_rev: 1,
            to_rev: 2,
            new_threshold: 1,
            new_servers: vec![srv2],
            new_successors: vec![],
            retain_until_quarter: retain_q,
            proposed_at: 1_760_050_000,
            initiator: ProposalInitiator::Creator,
            proposer_sig: None,
            guarantor_cosigs: vec![],
        };
        proposal.proposer_sig = Some(
            creator_mig_sk
                .sign(&proposal.core_digest().unwrap())
                .to_bytes(),
        );
        let imported_state_hash = [0x44u8; 32];
        let prop_hash = proposal.core_digest().unwrap();
        let mut ack2 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash,
            retain_until_quarter: retain_q,
            lock_pub: sk2.verifying_key().to_bytes(),
            acked_at: 1_760_050_100,
            sig: [0u8; 64],
        };
        ack2.sig = sk2.sign(&ack2.digest()).to_bytes();
        let c_digest =
            MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &imported_state_hash);
        let commit_sig = CommitteeSignature {
            lock_pub: sk1.verifying_key().to_bytes(),
            committed_at: 1_760_050_200,
            sig: sk1.sign(&c_digest).to_bytes(),
        };
        let cert = MigrationCert {
            anchor_id,
            genesis_manifest: Some(Box::new(rev1.clone())),
            proposal,
            imported_state_hash,
            import_acks: vec![ack2],
            commit_sigs: vec![commit_sig],
        };
        let (rev2, _) = cert.verify_step(&anchor_id, &rev1).unwrap();

        svc_alice
            .register_l2_anchor_template_with_nonce(
                rev1.clone(),
                vec![t1],
                Some(&anchor_nonce),
                Some(pw),
            )
            .unwrap();
        svc_alice
            .create_new_voucher(
                &std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id),
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
        let lid = svc_alice
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();
        let bundle_res = svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: bob_id,
                    sources: vec![SourceTransfer {
                        local_instance_id: lid,
                        amount_to_send: "20".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();

        // Server 1 returns inline-less `409 epoch_frozen` (NO latest_manifest!)
        let h1 = tokio::spawn(async move {
            if let Ok((mut stream, _)) = l1.accept().await {
                let mut buf = [0u8; 65_536];
                let _ = stream.read(&mut buf).await;
                let body = r#"{"error":"epoch_frozen"}"#;
                let http = format!(
                    "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        });
        // Discovery server hosts Rev-2 manifest at `/anchors/{anchor_hex}.bin`
        let rev2_bytes = serde_json::to_vec(&rev2).unwrap();
        let expected_path = format!("/d/anchors/{}.bin", hex::encode(anchor_id));
        let h_disc = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = l_disc.accept().await else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                if raw.starts_with(&format!("GET {} ", expected_path)) {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                        rev2_bytes.len()
                    );
                    let _ = stream.write_all(hdr.as_bytes()).await;
                    let _ = stream.write_all(&rev2_bytes).await;
                } else {
                    let _ = stream
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                        .await;
                }
            }
        });
        // Server 2 handles the retried Round-2 batch on Rev 2
        let h2 = tokio::spawn(async move {
            if let Ok((mut stream, _)) = l2.accept().await {
                let mut buf = [0u8; 65_536];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
                let receipts: Vec<LockReceipt> = req
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, h)| {
                        let b = decode_hex_fixed::<
                            { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                        >(h)
                        .unwrap();
                        let e = LockEntry::from_wire_bytes(&b).unwrap();
                        let mut r = LockReceipt {
                            kind: LockReceiptKind::FirstWrite,
                            lock_pub: sk2.verifying_key().to_bytes(),
                            bucket: req.bucket,
                            slot: e.slot,
                            child_lock: e.child_lock,
                            entry_hash: e.entry_hash().unwrap(),
                            retain_until_quarter: retain_q,
                            write_seq: (i as u64) + 1,
                            bucket_head: [7u8; 32],
                            recipient_nonce: req.recipient_nonce,
                            sig: [0u8; 64],
                        };
                        r.sig = sk2.sign(&r.digest()).to_bytes();
                        r
                    })
                    .collect();
                let resp = serde_json::to_string(&SyncLockBatchResponse {
                    status: "ok".into(),
                    bucket: req.bucket,
                    retain_until_quarter: retain_q,
                    receipts,
                    conflict: None,
                })
                .unwrap();
                let http = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    resp.len(),
                    resp
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        });

        let state_bob = AppState::new(svc_bob);
        let mut policy = SecurityPolicy::default();
        policy.public_syncs.push(crate::settings::StoredPublicSync {
            write_url: format!("http://127.0.0.1:{}/sync.php", p_disc),
            read_url: format!("http://127.0.0.1:{}/d", p_disc),
            drop_pub: "cc".repeat(32),
            register_url: None,
            access_pubkey: "dd".repeat(32),
            enabled: true,
            authorized_channels: vec!["lock".into()],
            status: "active".into(),
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
        });
        state_bob.set_cached_security_policy(Some(policy));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };

        let upgraded = verify_bundle_l2_precommit(
            &state_bob,
            &bob_session,
            &bundle_res.bundle_bytes,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(
            matches!(upgraded, PrecommitL2Verification::CleanQuorum { .. }),
            "S04-F6: inline-less 409 epoch_frozen must trigger discovery and succeed on Rev 2, got {:?}",
            upgraded
        );

        h1.abort();
        h2.abort();
        h_disc.abort();
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_bob);
    }
}
