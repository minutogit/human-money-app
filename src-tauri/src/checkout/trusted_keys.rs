use super::*;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::state::canonicalize_did;
use human_money_core::humoco_proto::{
    decode_hex_fixed, manifest::AnchorManifest, types::LockPubKey,
};
use human_money_core::models::conflict::TrustStatus;
use human_money_core::services::bundle_processor;

use crate::{
    models::FrontendError,
    settings::{InternalLifecycleState, SecurityPolicy},
    sync_client, AppState,
};

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
pub(super) async fn resolve_trusted_lock_keys(
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
pub(super) fn build_receive_payload_and_record(
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
