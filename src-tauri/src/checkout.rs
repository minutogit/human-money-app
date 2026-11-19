//! Recipient Sync-Lock checkout orchestration (`CORE-020` / `APP-070`).
//!
//! Computes `base_trusted_keys` and `independently_pinned_creator_keys` strictly from locally
//! trusted sources (own Syncs, Hop-1/Hop-2 non-offender contacts, ADR-08 provider filter seals)
//! and resolves authentic `LockPubKey`s without trusting untrusted manifest claims.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS, decode_hex_fixed, manifest::AnchorManifest,
    types::LockPubKey,
};
use human_money_core::models::conflict::TrustStatus;
use human_money_core::models::layer2_api::{CheckoutReleaseVerdict, L2ProcessingOutcome};
use human_money_core::services::{bundle_processor, l2_gateway};
use crate::state::canonicalize_did;

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
/// - Own active Sync servers (`security_policy.public_syncs`) and nominated lock server
/// - Locally assembled anchor templates (`lifecycle.l2_anchor_templates_meta`)
/// - Non-offender Hop-1/Hop-2 AddressBook contacts & WoT ADR-08 provider filter seals
///
/// # Security (`LÜCKE-03` & Untrusted Manifest Isolation)
/// - Never trusts `s.lock_pub` copied out of an incoming unverified `AnchorManifest`. Every
///   trusted `write_url` resolves its `lock_pub` strictly via locally cached verified probes,
///   locally stored template/nomination metadata, or live `?info` + `verify_lock_delegation`.
/// - Excludes sync URLs contributed solely by a foreign `voucher.creator_profile.id` so stranger
///   merchants do not trust creator-nominated servers via contact cards alone.
pub(crate) async fn resolve_trusted_lock_keys(
    state: &AppState,
    security_policy: &SecurityPolicy,
    lifecycle: &InternalLifecycleState,
    client: &reqwest::Client,
    creator_id: Option<&str>,
    manifest: Option<&AnchorManifest>,
    password: Option<&str>,
) -> Result<(Vec<LockPubKey>, Vec<LockPubKey>), FrontendError> {
    let mut known_url_to_key: HashMap<String, LockPubKey> = HashMap::new();

    for (url, probe) in &lifecycle.l2_probe_cache {
        if probe.passed {
            if let Ok(pk) = decode_hex_fixed::<32>(&probe.lock_pub) {
                known_url_to_key.insert(crate::services::common::normalize_sync_url(url), pk);
            }
        }
    }
    if let Some(ref own) = security_policy.own_nominated_lock_server {
        if let Ok(pk) = decode_hex_fixed::<32>(&own.lock_pub) {
            known_url_to_key.insert(
                crate::services::common::normalize_sync_url(&own.write_url),
                pk,
            );
        }
    }
    for tmpl in &lifecycle.l2_anchor_templates_meta {
        for srv in &tmpl.servers {
            if let Ok(pk) = decode_hex_fixed::<32>(&srv.lock_pub) {
                known_url_to_key.insert(
                    crate::services::common::normalize_sync_url(&srv.write_url),
                    pk,
                );
            }
        }
    }

    let mut trusted_urls: HashSet<String> = HashSet::new();
    let mut trusted_keys: HashSet<LockPubKey> = HashSet::new();

    for s in &security_policy.public_syncs {
        if s.enabled && s.status != "revoked" {
            let norm = crate::services::common::normalize_sync_url(&s.write_url);
            if !norm.is_empty() {
                trusted_urls.insert(norm);
            }
        }
    }
    if let Some(ref own) = security_policy.own_nominated_lock_server {
        let norm = crate::services::common::normalize_sync_url(&own.write_url);
        if !norm.is_empty() {
            trusted_urls.insert(norm);
        }
    }
    for tmpl in &lifecycle.l2_anchor_templates_meta {
        for srv in &tmpl.servers {
            let norm = crate::services::common::normalize_sync_url(&srv.write_url);
            if !norm.is_empty() {
                trusted_urls.insert(norm);
            }
        }
    }

    let canonical_creator = creator_id.and_then(|c| canonicalize_did(c).ok());
    let contact_candidates: Vec<String> = {
        let svc_guard = state.service.lock().ok();
        let book_guard = state.contacts.lock().ok();
        let wot_guard = state.wot.lock().ok();

        let self_did = wot_guard
            .as_ref()
            .and_then(|g| g.as_ref().map(|r| r.self_did.clone()))
            .or_else(|| {
                svc_guard.as_ref().and_then(|s| {
                    s.with_wallet(|w| w.get_user_id().to_string())
                        .ok()
                        .and_then(|d| canonicalize_did(&d).ok())
                })
            });

        let wot_ref = wot_guard.as_ref().and_then(|g| g.as_ref());
        let wallet_offenders: HashSet<String> = svc_guard
            .as_ref()
            .and_then(|s| {
                s.with_wallet(|w| {
                    book_guard
                        .as_ref()
                        .and_then(|b| b.as_ref())
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
                .ok()
            })
            .unwrap_or_default();

        let mut urls = Vec::new();
        if let Some(Some(book)) = book_guard.as_deref() {
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

                let mut contact_trusted = contact.trust_level.map(|lvl| lvl >= 1).unwrap_or(false);
                if !contact_trusted {
                    if let (Some(reg), Some(can_c)) = (wot_ref, can_contact.as_ref()) {
                        let hop1_trusted =
                            reg.own_contacts.get(can_c).is_some_and(|e| e.level >= 1);
                        let adr08_certified = reg.provider_filters.values().any(|pf| {
                            pf.local_trust_level >= 1 && pf.certified_level(can_c).is_some()
                        });
                        // Hop-2 reachability check via ego graph (depth <= 2 from node 0 with positive weight)
                        let graph = reg.get_or_build_graph_cache();
                        let hop2_trusted = graph.did_to_id.get(can_c).is_some_and(|&target_idx| {
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
                                    edges1.iter().any(|e| e.to == target_idx && e.level >= 1)
                                })
                            })
                        });
                        contact_trusted = hop1_trusted || hop2_trusted || adr08_certified;
                    }
                }

                if contact_trusted {
                    for sync in &contact.syncs {
                        if sync.status != "revoked" {
                            let norm = crate::services::common::normalize_sync_url(&sync.write_url);
                            if !norm.is_empty() {
                                urls.push(norm);
                            }
                        }
                    }
                }
            }
        }
        urls
    };

    for u in contact_candidates {
        trusted_urls.insert(u);
    }

    let offenders = {
        let mut svc_guard = state.lock_service()?;
        crate::l2_lifecycle::load_server_offender_list(&mut svc_guard, password)?
    };

    let mut urls_to_live_probe: Vec<String> = Vec::new();
    for url in &trusted_urls {
        if let Some(pk) = known_url_to_key.get(url) {
            if !crate::l2_lifecycle::is_server_key_revoked(&offenders, pk, None) {
                trusted_keys.insert(*pk);
            }
        } else if manifest.is_some_and(|m| {
            m.servers
                .iter()
                .any(|s| crate::services::common::normalize_sync_url(&s.write_url) == *url)
        }) {
            urls_to_live_probe.push(url.clone());
        }
    }

    if !urls_to_live_probe.is_empty() {
        let mut join_set = tokio::task::JoinSet::new();
        for url in urls_to_live_probe {
            let c = client.clone();
            join_set.spawn(async move {
                let res = tokio::time::timeout(
                    Duration::from_secs(2),
                    sync_client::fetch_sync_info(&c, &url),
                )
                .await;
                res.ok().and_then(|r| r.ok()).and_then(|info| {
                    let lock = info.lock?;
                    let del = lock.delegation.as_ref()?;
                    if !sync_client::verify_lock_delegation(&lock.lock_pub, del, None) {
                        return None;
                    }
                    let lock_pk = decode_hex_fixed::<32>(&lock.lock_pub).ok()?;
                    let admin_pk = decode_hex_fixed::<32>(&del.admin_pub).ok()?;
                    Some((lock_pk, admin_pk))
                })
            });
        }
        while let Some(res) = join_set.join_next().await {
            if let Ok(Some((pk, admin_pk))) = res {
                if !crate::l2_lifecycle::is_server_key_revoked(&offenders, &pk, Some(&admin_pk)) {
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

        if balances_are_summable {
            transfer_summary
                .summable_amounts
                .insert(display_currency.clone(), last_tx.amount.clone());
        } else {
            transfer_summary
                .countable_items
                .insert(display_currency.clone(), 1);
        }

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
            state.try_import_piggybacked_contact_card(service, Some(card_json), password);
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

            let receive_payload = build_receive_payload_and_record(
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
            .map(|(payload, _)| payload)
            .ok();

            return Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                is_replay: true,
                verdict: verdict_fe,
                receive_payload,
            });
        }
    }

    // 2. Decrypt & inspect bundle to check N = 0 offline-only vs anchored vouchers
    let (opened_bundle, merchant_id) = {
        let service = state.lock_service()?;
        service
            .with_wallet_and_identity(|wallet, identity| {
                let parsed = bundle_processor::open_and_verify_bundle(identity, bundle)?;
                Ok::<_, human_money_core::Error>((parsed, wallet.get_user_id().to_string()))
            })
            .map_err(FrontendError::from)?
            .map_err(FrontendError::from)?
    };

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
                m.servers.iter().map(|s| s.write_url.as_str()),
            )
        })
        .unwrap_or(DEFAULT_LOCK_POW_BASE_BITS);

    // 3. Prepare durable challenge context + per-server SyncLockBatchRequests (running Discovery if local manifest missing)
    let first_prepare = {
        let mut service = state.lock_service()?;
        service.prepare_checkout_requests(
            purchase_id,
            bundle,
            pow_base_bits,
            now()?,
            &HashMap::new(),
            password,
        )
    };
    let (mut context, mut requests) = match first_prepare {
        Ok(pair) => pair,
        Err(_) => {
            let discovered = crate::l2_lifecycle::discover_anchor_manifest(
                state,
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
                        m.servers.iter().map(|s| s.write_url.as_str()),
                    ));
            }
            let mut service = state.lock_service()?;
            service
                .prepare_checkout_requests(
                    purchase_id,
                    bundle,
                    pow_base_bits,
                    now()?,
                    &HashMap::new(),
                    password,
                )
                .map_err(FrontendError::from)?
        }
    };

    // 4. Resolve trusted LockPubKeys & fan out committee batches in parallel
    let (mut base_trusted_keys, mut independently_pinned_creator_keys) = resolve_trusted_lock_keys(
        state,
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
    )
    .await;

    // 4b. Handle `409 epoch_frozen`, `410 revoked`, or total NXDOMAIN committee failure via Discovery Cascade
    {
        let now_secs = sync_client::current_unix_timestamp();
        if !dispatch.revoked_notices.is_empty() {
            if let Ok(mut service) = state.service.lock() {
                let _ = crate::l2_lifecycle::record_revoked_servers(
                    &mut service,
                    &anchor_id,
                    &dispatch.revoked_notices,
                    &context.manifest.servers,
                    now_secs,
                    password,
                );
            }
        }

        let mut best_upgraded: Option<AnchorManifest> = None;
        for m in &dispatch.epoch_frozen_manifests {
            if m.revision > context.manifest.revision
                && m.verify_anchor_id(&anchor_id).is_ok()
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
            && (all_unreachable || !dispatch.epoch_frozen_manifests.is_empty())
        {
            if let Ok(Some(discovered)) = crate::l2_lifecycle::discover_anchor_manifest(
                state,
                &client,
                &anchor_id,
                voucher.creator_profile.id.as_deref(),
                &guarantor_dids,
                password,
            )
            .await
            {
                if discovered.revision > context.manifest.revision {
                    best_upgraded = Some(discovered);
                }
            }
        }

        if let Some(newer_manifest) = best_upgraded {
            pow_base_bits =
                pow_base_bits.max(crate::services::common::max_pow_base_bits_for_servers(
                    &lifecycle,
                    newer_manifest.servers.iter().map(|s| s.write_url.as_str()),
                ));
            let upgraded_pair = {
                let mut service = state.lock_service()?;
                service
                    .update_l2_anchor_manifest(newer_manifest, password)
                    .map_err(FrontendError::from)?;
                service
                    .prepare_checkout_requests(
                        purchase_id,
                        bundle,
                        pow_base_bits,
                        now()?,
                        &HashMap::new(),
                        password,
                    )
                    .map_err(FrontendError::from)?
            };
            context = upgraded_pair.0;
            requests = upgraded_pair.1;
            let keys = resolve_trusted_lock_keys(
                state,
                &security_policy,
                &lifecycle,
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
            )
            .await;
        }
    }

    if let Some(equiv_proof) = l2_gateway::detect_equivocation(&dispatch.receipts) {
        if let Ok(mut service) = state.service.lock() {
            let now_secs = sync_client::current_unix_timestamp().max(1);
            let _ = crate::l2_lifecycle::record_equivocation_proof(
                &mut service,
                None,
                &equiv_proof,
                now_secs,
                password,
            );
        }
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
    let current_owner = service
        .with_wallet(|wallet| wallet.get_user_id().to_string())
        .map_err(FrontendError::from)?;
    if current_owner != merchant_id {
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

            let completion = service
                .complete_checkout_purchase(
                    purchase_id,
                    bundle,
                    &dispatch.receipts,
                    &base_trusted_keys,
                    &independently_pinned_creator_keys,
                    standards,
                    now()?,
                    password,
                )
                .map_err(FrontendError::from)?;

            if let Err(error) = state.refresh_events_cache(&mut service, password) {
                log::warn!("Checkout committed; event cache refresh failed: {}", error);
            }

            let mut verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                completion.verdict.into();
            verdict_fe.reduced_guarantor_protection = reduced_guarantor_protection;
            verdict_fe.manifest_revision = manifest_revision;

            let (receive_payload, record_opt) = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                completion.newly_committed,
                false,
                verdict_fe.clone(),
                l2_quorum,
                password,
            )?;

            if completion.newly_committed {
                if let Some(record) = record_opt {
                    if let Err(e) = state.append_to_history(&mut service, record, password) {
                        log::warn!(
                            "Checkout committed; transaction history recording failed: {}",
                            e
                        );
                    }
                }
            }

            // Catch-up background completion for any committee servers that timed out during Green checkout
            if !dispatch.pending_retries.is_empty() {
                let retries = dispatch.pending_retries;
                let retry_client = client.clone();
                tokio::spawn(async move {
                    let _ = crate::gateway_resolver::dispatch_lock_committee_batches(
                        &retry_client,
                        &retries,
                        Duration::from_secs(4),
                    )
                    .await;
                });
            }

            drop(service);

            Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                is_replay: false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::{
        crypto::compute_ticket_pow_hash,
        encode_quarter,
        manifest::{AnchorAuthority, GenesisTicket, NominatedBy, ServerEntry},
        types::{LockReceipt, LockReceiptKind},
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
        *state_bob.security_policy.lock().unwrap() = Some(SecurityPolicy::default());
        *state_bob.internal_lifecycle.lock().unwrap() = Some(InternalLifecycleState::default());
        let completion = checkout_bundle(
            &state_bob,
            "pos-offline-1",
            &transfer.bundle_bytes,
            &standards,
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
        *state_bob.security_policy.lock().unwrap() = Some(SecurityPolicy::default());
        *state_bob.internal_lifecycle.lock().unwrap() = Some(InternalLifecycleState::default());
        let yellow = checkout_bundle(
            &state_bob,
            "pos-yellow",
            &transfer.bundle_bytes,
            &standards,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(!yellow.newly_committed);
        assert!(yellow.verdict.outcome.starts_with("Yellow:"));
        assert!(yellow.verdict.requires_merchant_approval);

        // Case B: Bob trusts both servers via own public_syncs + verified probe cache -> Green Confirmed!
        {
            let mut st_sec = state_bob.security_policy.lock().unwrap();
            let s = st_sec.as_mut().unwrap();
            let mut st_lc = state_bob.internal_lifecycle.lock().unwrap();
            let lc = st_lc.as_mut().unwrap();
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
                lc.l2_probe_cache.insert(
                    wurl.clone(),
                    sync_client::FrontendLockServerProbeResult {
                        write_url: wurl,
                        read_url: format!("http://127.0.0.1:{}/d", port),
                        drop_pub: hex::encode(sk.verifying_key().as_bytes()),
                        admin_pub: hex::encode(sk.verifying_key().as_bytes()),
                        lock_pub: hex::encode(sk.verifying_key().as_bytes()),
                        delegation_valid: true,
                        waf_probe_passed: true,
                        selftest_passed: true,
                        key_leak_safe: true,
                        passed: true,
                        latency_ms: 5,
                        max_retention_quarter: retain_q,
                        pow_base_bits: DEFAULT_LOCK_POW_BASE_BITS,
                        allow_pow_tickets: true,
                        inode_warning: false,
                        used_inodes: 0,
                        max_inodes: 50000,
                        probed_at_secs: 1,
                        error: None,
                    },
                );
            }
        }
        let green = checkout_bundle(
            &state_bob,
            "pos-green",
            &transfer.bundle_bytes,
            &standards,
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
}
