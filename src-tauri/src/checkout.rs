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

use crate::{
    models::FrontendError,
    settings::{InternalLifecycleState, SecurityPolicy},
    sync_client, AppState,
};

const CHECKOUT_BATCH_TIMEOUT: Duration = Duration::from_secs(4);

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
    let mut known_url_to_key: HashMap<String, LockPubKey> = HashMap::new();

    for probe in lifecycle.l2_probe_cache.values() {
        if probe.passed && crate::settings::is_lock_admin_trusted(security_policy, &probe.admin_pub)
        {
            if let Ok(pk) = decode_hex_fixed::<32>(&probe.lock_pub) {
                let norm = crate::services::common::normalize_sync_url(&probe.write_url);
                if !norm.is_empty() {
                    known_url_to_key.insert(norm, pk);
                }
                if let Some(ref del) = probe.delegation {
                    let norm_del = crate::services::common::normalize_sync_url(&del.write_url);
                    if !norm_del.is_empty() {
                        known_url_to_key.insert(norm_del, pk);
                    }
                }
            }
        }
    }

    let mut trusted_urls: HashSet<String> = HashSet::new();
    let mut trusted_keys: HashSet<LockPubKey> = HashSet::new();

    if let Ok(Some(Some(admin))) = state.with_session(session, |svc| {
        crate::services::sync_engine::admin::load_stored_sync_admin_opt(svc, password)
    }) {
        if crate::settings::is_lock_admin_trusted(security_policy, &admin.descriptor.drop_pub) {
            let norm = crate::services::common::normalize_sync_url(&admin.descriptor.write_url);
            if !norm.is_empty() {
                trusted_urls.insert(norm);
            }
        }
    }
    for d in &security_policy.market_selected_syncs {
        if crate::settings::is_lock_admin_trusted(security_policy, &d.drop_pub) {
            let norm = crate::services::common::normalize_sync_url(&d.write_url);
            if !norm.is_empty() {
                trusted_urls.insert(norm);
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
                trusted_urls.insert(norm);
            }
        }
    }

    let canonical_creator = creator_id.and_then(|c| canonicalize_did(c).ok());
    let contact_candidates: Vec<String> = state.with_session_required(
        session,
        "Wallet session changed while resolving contact trust for checkout",
        |s| {
            let caches_guard = state.profile_caches.lock().ok();
            let caches_ref = caches_guard.as_ref().and_then(|g| g.as_ref());
            let book_opt = caches_ref.and_then(|c| c.contacts.as_ref());
            let wot_ref = caches_ref.and_then(|c| c.wot.as_ref());

            let self_did = wot_ref.map(|r| r.self_did.clone()).or_else(|| {
                s.with_wallet(|w| w.get_user_id().to_string())
                    .ok()
                    .and_then(|d| canonicalize_did(&d).ok())
            });

            let wallet_offenders: HashSet<String> = s
                .with_wallet(|w| {
                    book_opt
                        .map(|book| {
                            book.contacts
                                .keys()
                                .filter(|did| {
                                    matches!(w.check_reputation(did), TrustStatus::KnownOffender(_))
                                })
                                .cloned()
                                .collect::<HashSet<String>>()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            let mut urls = Vec::new();
            if let Some(book) = book_opt {
                for contact in book.contacts.values() {
                    if wallet_offenders.contains(&contact.did) {
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
                            let adr08_certified = reg.provider_filters.values().any(|pf| {
                                pf.local_trust_level >= 1 && pf.certified_level(can_c).is_some()
                            });
                            // Hop-2 reachability check via ego graph (depth <= 2 from node 0 with positive weight)
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
                                                depth1_nodes.insert(e.to);
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
                                let norm =
                                    crate::services::common::normalize_sync_url(&sync.write_url);
                                if !norm.is_empty() {
                                    urls.push(norm);
                                }
                            }
                        }
                    }
                }
            }
            Ok(urls)
        },
    )?;

    for u in contact_candidates {
        trusted_urls.insert(u);
    }

    let offenders = state.with_session_required(
        session,
        "Wallet session changed while loading server offenders for checkout",
        |svc| crate::l2_lifecycle::load_server_offender_list(svc, password),
    )?;

    let mut urls_to_live_probe: HashMap<String, Option<String>> = HashMap::new();
    if let Some(m) = manifest {
        for s in &m.servers {
            let expected_admin_hex = hex::encode(s.admin_pub);
            if !crate::settings::is_lock_admin_trusted(security_policy, &expected_admin_hex) {
                continue;
            }
            let (resolved_url, _) =
                crate::services::common::resolve_server_locator(s, &lifecycle.l2_probe_cache);
            let norm_emb = crate::services::common::normalize_sync_url(&s.write_url);
            let norm_res = crate::services::common::normalize_sync_url(&resolved_url);
            let url_trusted = trusted_urls.contains(&norm_emb) || trusted_urls.contains(&norm_res);
            if !url_trusted {
                continue;
            }
            if let Some(pk) = known_url_to_key
                .get(&norm_res)
                .or_else(|| known_url_to_key.get(&norm_emb))
            {
                if !crate::l2_lifecycle::is_server_key_revoked(&offenders, pk, Some(&s.admin_pub)) {
                    trusted_keys.insert(*pk);
                }
            } else if !norm_res.is_empty() {
                urls_to_live_probe.insert(norm_res, Some(expected_admin_hex));
            }
        }
    }

    if !urls_to_live_probe.is_empty() {
        let now_secs = sync_client::current_unix_timestamp();
        let mut join_set = tokio::task::JoinSet::new();
        for (url, exp_admin_opt) in urls_to_live_probe {
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
                    if let Some(ref exp_admin) = exp_admin_opt {
                        if !del.admin_pub.trim().eq_ignore_ascii_case(exp_admin) {
                            return None;
                        }
                    }
                    if crate::services::common::normalize_sync_url(&del.write_url) != norm_target {
                        return None;
                    }
                    if let Some(ref rev) = info.revocation {
                        let rev_admin = exp_admin_opt.as_deref().unwrap_or(del.admin_pub.trim());
                        if sync_client::verify_revocation_notice(rev, rev_admin).is_ok()
                            && rev.effective_at <= now_secs
                        {
                            return None;
                        }
                    }
                    if !sync_client::verify_lock_delegation(
                        &lock.lock_pub,
                        del,
                        exp_admin_opt.as_deref(),
                    ) {
                        return None;
                    }
                    let lock_pk = decode_hex_fixed::<32>(&lock.lock_pub).ok()?;
                    let admin_pk = decode_hex_fixed::<32>(&del.admin_pub).ok()?;
                    Some((lock_pk, admin_pk, del.admin_pub.clone()))
                })
            });
        }
        while let Some(res) = join_set.join_next().await {
            if let Ok(Some((pk, admin_pk, admin_hex))) = res {
                if crate::settings::is_lock_admin_trusted(security_policy, &admin_hex)
                    && !crate::l2_lifecycle::is_server_key_revoked(&offenders, &pk, Some(&admin_pk))
                {
                    trusted_keys.insert(pk);
                }
            }
        }
    }

    let keys_vec: Vec<LockPubKey> = trusted_keys.into_iter().collect();
    Ok((keys_vec.clone(), keys_vec))
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
pub(crate) async fn checkout_bundle(
    state: &AppState,
    purchase_id: &str,
    bundle: &[u8],
    standards: &HashMap<String, String>,
    force_accept_tolerance_bundle: bool,
    password: Option<&str>,
) -> Result<PurchaseCompletion, FrontendError> {
    // 1. Idempotent replay check before any network I/O
    {
        let mut service = state.lock_service()?;
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
    let (opened_bundle, merchant_id, session, already_in_history) = {
        let service = state.lock_service()?;
        let (parsed, mid, in_history) = service
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
            .map_err(FrontendError::from)?;
        let session = crate::session::SessionToken {
            generation: state.current_session_generation(),
            owner_did: mid.clone(),
        };
        (parsed, mid, session, in_history)
    };

    if already_in_history {
        let verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
            CheckoutReleaseVerdict::new(
                L2ProcessingOutcome::Confirmed {
                    quorum_certificates: Vec::new(),
                },
                true,
            )
            .into();
        let receive_payload = {
            let mut service = state.lock_service()?;
            build_receive_payload_and_record(
                state,
                &mut service,
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
            .ok()
        };
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
        let receive_payload = {
            let mut service = state.lock_service()?;
            build_receive_payload_and_record(
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
            .ok()
        };
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
        let receive_payload = {
            let mut service = state.lock_service()?;
            build_receive_payload_and_record(
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
            .ok()
        };
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
    let security_policy = state
        .get_cached_security_policy()
        .map_err(FrontendError::from)?;
    let lifecycle = state
        .get_cached_internal_lifecycle()
        .map_err(FrontendError::from)?;
    let client = sync_client::create_sync_http_client();
    let mut pow_base_bits = opened_bundle
        .anchor_manifest
        .as_ref()
        .map(|m| {
            crate::services::common::max_pow_base_bits_for_servers(
                &lifecycle,
                m.servers.iter().map(|s| s.lock_pub),
            )
        })
        .unwrap_or(DEFAULT_LOCK_POW_BASE_BITS);

    // 3. Prepare durable challenge context + per-server SyncLockBatchRequests (running Discovery if local manifest missing).
    // Fail closed immediately if the local anti-rollback sidecar is corrupted BEFORE any discovery fallback.
    let known_heads = {
        let mut service = state.lock_service()?;
        crate::state::load_known_head_receipts_for_anchor(&mut service, &anchor_id, password)?
    };
    let first_prepare = {
        let mut service = state.lock_service()?;
        service.prepare_checkout_requests(
            purchase_id,
            bundle,
            pow_base_bits,
            now()?,
            &known_heads,
            password,
        )
    };
    let (mut context, mut requests) = match first_prepare {
        Ok(pair) => pair,
        Err(_) => {
            let discovered = crate::l2_lifecycle::discover_anchor_manifest(
                state,
                &session,
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
                        &lifecycle,
                        m.servers.iter().map(|s| s.lock_pub),
                    ));
            }
            state.with_session_required(&session, "Wallet changed during checkout", |service| {
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
        &session,
        &security_policy,
        &lifecycle,
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
        &lifecycle.l2_probe_cache,
    )
    .await;

    // 4b. Handle `409 epoch_frozen`, `410 revoked`, or total NXDOMAIN committee failure via Discovery Cascade
    let upgrade = resolve_manifest_upgrade_after_dispatch(
        state,
        &session,
        &client,
        &anchor_id,
        voucher.creator_profile.id.as_deref(),
        &guarantor_dids,
        &lifecycle,
        &context.manifest,
        &dispatch,
        password,
    )
    .await?;
    if upgrade.upgraded_manifest.is_some() || upgrade.relocated {
        let fresh_lifecycle =
            state.with_session_required(&session, "Wallet changed during checkout", |service| {
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
        let keys = resolve_trusted_lock_keys(
            state,
            &session,
            &security_policy,
            &fresh_lifecycle,
            &client,
            voucher.creator_profile.id.as_deref(),
            Some(&context.manifest),
            password,
        )
        .await?;
        base_trusted_keys = keys.0;
        independently_pinned_creator_keys = keys.1;
        dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
            &client,
            &requests,
            CHECKOUT_BATCH_TIMEOUT,
            &fresh_lifecycle.l2_probe_cache,
        )
        .await;
    }

    if let Some(equiv_proof) = l2_gateway::detect_equivocation(&dispatch.receipts) {
        let _ = state.with_session(&session, |service| {
            let now_secs = sync_client::current_unix_timestamp().max(1);
            let _ = crate::l2_lifecycle::record_equivocation_proof(
                service,
                None,
                &equiv_proof,
                now_secs,
                password,
            );
            Ok(())
        });
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
    if !state.is_session_current(&session, &service) {
        return Err(fail("Wallet changed during checkout"));
    }

    for competing_entry in &dispatch.competing_entries {
        if let Err(err) = service.record_checkout_upstream_conflict(
            bundle,
            competing_entry,
            pow_base_bits,
            password,
        ) {
            log::warn!("Recording upstream conflict entry failed: {}", err);
        }
    }

    crate::state::record_known_head_receipts(
        &mut service,
        &anchor_id,
        &dispatch.receipts,
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
                let retry_probe_cache = lifecycle.l2_probe_cache.clone();
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

struct DispatchUpgradeResult {
    upgraded_manifest: Option<(AnchorManifest, u32)>,
    relocated: bool,
}

/// Records `410 revoked` notices and resolves any verified newer `AnchorManifest` from
/// `409 epoch_frozen` responses or manifest discovery (H7/H10 shared path for checkout & PoST).
#[allow(clippy::too_many_arguments)]
async fn resolve_manifest_upgrade_after_dispatch(
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
    if !dispatch.revoked_notices.is_empty() {
        let mut relocated_lock_pubs: HashSet<LockPubKey> = HashSet::new();
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
    if best_upgraded.is_none() && (all_unreachable || !dispatch.epoch_frozen_manifests.is_empty()) {
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
        return Ok((voucher_qcs, Vec::new(), Vec::new()));
    }

    let (entries, requests) = state.with_session_required(
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
            let mut nonce = [0u8; 32];
            nonce[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
            nonce[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
            let requests = l2_gateway::build_committee_batch_requests(
                anchor_id,
                manifest,
                &opened_bundle.genesis_tickets,
                &entries,
                &nonce,
                &known_heads,
            )
            .map_err(FrontendError::from)?;
            Ok((entries, requests))
        },
    )?;

    Ok((voucher_qcs, entries, requests))
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

    let lifecycle = state
        .get_cached_internal_lifecycle()
        .map_err(FrontendError::from)?;
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
            &lifecycle,
            manifest.servers.iter().map(|s| s.lock_pub),
        );

        let (mut voucher_qcs, mut entries, mut requests) =
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
                &lifecycle.l2_probe_cache,
            )
            .await;

            // H7/H10: Bounded single retry after `409 epoch_frozen` / `410 revoked` relocation / unreachable manifest upgrade
            let upgrade = resolve_manifest_upgrade_after_dispatch(
                state,
                session,
                &client,
                &anchor_id,
                voucher.creator_profile.id.as_deref(),
                &guarantor_dids,
                &lifecycle,
                &manifest,
                &dispatch,
                password,
            )
            .await?;
            if upgrade.upgraded_manifest.is_some() || upgrade.relocated {
                let fresh_probe_cache = state.with_session_required(
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
                            .map(|lc| lc.l2_probe_cache)
                            .map_err(FrontendError::from)
                    },
                )?;
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
                if !requests.is_empty() {
                    dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
                        &client,
                        &requests,
                        CHECKOUT_BATCH_TIMEOUT,
                        &fresh_probe_cache,
                    )
                    .await;
                }
            }

            if entries.is_empty() {
                VoucherPrecommitState::CleanQuorum
            } else {
                let equiv_proof = l2_gateway::detect_equivocation(&dispatch.receipts);
                state.with_session_required(
                    session,
                    "Wallet session changed before recording PoST L2 check receipts",
                    |service| {
                        if let Some(ref eq) = equiv_proof {
                            let now_secs = sync_client::current_unix_timestamp().max(1);
                            let _ = crate::l2_lifecycle::record_equivocation_proof(
                                service, None, eq, now_secs, password,
                            );
                        }
                        crate::state::record_known_head_receipts(
                            service,
                            &anchor_id,
                            &dispatch.receipts,
                            password,
                        )
                    },
                )?;

                // K1: Verify each competing entry cryptographically against this voucher's hops
                // and creator_mig_pub before counting it as VerifiedConflict.
                let mut verified_conflicts: Vec<(LockEntry, u32)> = Vec::new();
                for ce in &dispatch.competing_entries {
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
                            pow_base_bits,
                        )
                        .is_ok()
                    });
                    if is_valid_conflict
                        && !verified_conflicts
                            .iter()
                            .any(|(existing, _)| existing == ce)
                    {
                        verified_conflicts.push((ce.clone(), pow_base_bits));
                    }
                }

                if !verified_conflicts.is_empty() {
                    VoucherPrecommitState::VerifiedConflict(verified_conflicts)
                } else if equiv_proof.is_some() {
                    VoucherPrecommitState::Unsettled(format!(
                        "PoST L2 committee equivocation detected for voucher {}",
                        voucher.voucher_id
                    ))
                } else {
                    match manifest.committee() {
                        Err(e) => VoucherPrecommitState::Unsettled(format!(
                            "PoST L2 committee manifest invalid for voucher {}: {}",
                            voucher.voucher_id, e
                        )),
                        Ok(committee) => {
                            let mut all_entries_ok = true;
                            for entry_req in &entries {
                                let mut valid_receipts = Vec::new();
                                let mut seen_pubs = HashSet::new();
                                for r in &dispatch.receipts {
                                    if matches!(
                                        r.kind,
                                        human_money_core::humoco_proto::types::LockReceiptKind::FirstWrite
                                    ) && r.slot == entry_req.slot
                                        && r.child_lock == entry_req.child_lock
                                        && committee.contains(&r.lock_pub)
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
        let completion = checkout_bundle(
            &state_bob,
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
        let yellow = checkout_bundle(
            &state_bob,
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
                s.public_syncs.push(crate::settings::StoredPublicSync {
                    write_url: wurl.clone(),
                    read_url: format!("http://127.0.0.1:{}/d", port),
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
                let lock_hex = hex::encode(sk.verifying_key().as_bytes());
                lc.l2_probe_cache.insert(
                    lock_hex.clone(),
                    sync_client::FrontendLockServerProbeResult {
                        write_url: wurl,
                        read_url: format!("http://127.0.0.1:{}/d", port),
                        drop_pub: lock_hex.clone(),
                        admin_pub: lock_hex.clone(),
                        lock_pub: lock_hex,
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
                        probed_at_secs: 1,
                        delegation: None,
                        error: None,
                    },
                );
            }
            state_bob.set_cached_security_policy(Some(s));
            state_bob.set_cached_internal_lifecycle(Some(lc));
        }
        let green = checkout_bundle(
            &state_bob,
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
}
