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
use humoco_wot::canonicalize_did;

use crate::{models::FrontendError, settings::AppSettings, sync_client, AppState};

const CHECKOUT_BATCH_TIMEOUT: Duration = Duration::from_secs(4);

fn fail(message: &str) -> FrontendError {
    FrontendError::from(message.to_string())
}

fn now() -> Result<u64, FrontendError> {
    u64::try_from(chrono::Utc::now().timestamp_millis())
        .map_err(|_| fail("Invalid verification time"))
}

pub(crate) fn normalize_sync_write_url(raw: &str) -> String {
    raw.trim().trim_end_matches('/').to_string()
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseCompletion {
    pub purchase_id: String,
    pub newly_committed: bool,
    pub verdict: crate::models::FrontendCheckoutReleaseVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
/// - Own active Sync servers (`settings.public_syncs`) and nominated lock server
/// - Locally assembled anchor templates (`settings.l2_anchor_templates_meta`)
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
    settings: &AppSettings,
    client: &reqwest::Client,
    creator_id: Option<&str>,
    manifest: Option<&AnchorManifest>,
) -> (Vec<LockPubKey>, Vec<LockPubKey>) {
    let mut known_url_to_key: HashMap<String, LockPubKey> = HashMap::new();

    for (url, probe) in &settings.l2_probe_cache {
        if probe.passed {
            if let Ok(pk) = decode_hex_fixed::<32>(&probe.lock_pub) {
                known_url_to_key.insert(normalize_sync_write_url(url), pk);
            }
        }
    }
    if let Some(ref own) = settings.own_nominated_lock_server {
        if let Ok(pk) = decode_hex_fixed::<32>(&own.lock_pub) {
            known_url_to_key.insert(normalize_sync_write_url(&own.write_url), pk);
        }
    }
    for tmpl in &settings.l2_anchor_templates_meta {
        for srv in &tmpl.servers {
            if let Ok(pk) = decode_hex_fixed::<32>(&srv.lock_pub) {
                known_url_to_key.insert(normalize_sync_write_url(&srv.write_url), pk);
            }
        }
    }

    let mut trusted_urls: HashSet<String> = HashSet::new();
    let mut trusted_keys: HashSet<LockPubKey> = HashSet::new();

    for s in &settings.public_syncs {
        if s.enabled && s.status != "revoked" {
            let norm = normalize_sync_write_url(&s.write_url);
            if !norm.is_empty() {
                trusted_urls.insert(norm);
            }
        }
    }
    if let Some(ref own) = settings.own_nominated_lock_server {
        let norm = normalize_sync_write_url(&own.write_url);
        if !norm.is_empty() {
            trusted_urls.insert(norm);
        }
    }
    for tmpl in &settings.l2_anchor_templates_meta {
        for srv in &tmpl.servers {
            let norm = normalize_sync_write_url(&srv.write_url);
            if !norm.is_empty() {
                trusted_urls.insert(norm);
            }
        }
    }

    let canonical_creator = creator_id.and_then(|c| canonicalize_did(c).ok());
    let contact_candidates: Vec<String> = {
        let book_guard = state.contacts.lock().ok();
        let wot_guard = state.wot.lock().ok();
        let svc_guard = state.service.lock().ok();

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
                            let norm = normalize_sync_write_url(&sync.write_url);
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

    let mut urls_to_live_probe: Vec<String> = Vec::new();
    for url in &trusted_urls {
        if let Some(pk) = known_url_to_key.get(url) {
            trusted_keys.insert(*pk);
        } else if manifest.is_some_and(|m| {
            m.servers
                .iter()
                .any(|s| normalize_sync_write_url(&s.write_url) == *url)
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
                    decode_hex_fixed::<32>(&lock.lock_pub).ok()
                })
            });
        }
        while let Some(res) = join_set.join_next().await {
            if let Ok(Some(pk)) = res {
                trusted_keys.insert(pk);
            }
        }
    }

    let keys_vec: Vec<LockPubKey> = trusted_keys.into_iter().collect();
    (keys_vec.clone(), keys_vec)
}

#[allow(clippy::too_many_arguments)]
fn build_receive_payload_and_record(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    purchase_id: &str,
    bundle_data: &[u8],
    standards: &HashMap<String, String>,
    newly_committed: bool,
    verdict: crate::models::FrontendCheckoutReleaseVerdict,
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

    let conflict_summaries = service
        .list_conflicts()
        .map_err(FrontendError::from)?
        .into_iter()
        .map(|s| s.into())
        .collect();

    let record = crate::models::TransactionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        direction: "received".to_string(),
        recipient_id,
        sender_id: opened_bundle.sender_id.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        summable_amounts: transfer_summary.summable_amounts.clone(),
        countable_items: transfer_summary.countable_items.clone(),
        involved_vouchers: involved_vouchers_ids.clone(),
        involved_sources_details: Some(involved_vouchers_details.clone()),
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
        checkout_verdict: Some(verdict),
        purchase_id: Some(purchase_id.to_string()),
        newly_committed: Some(newly_committed),
        is_replay: Some(!newly_committed),
        wot_imported_lists,
    };

    Ok((payload, Some(record)))
}

fn determine_pow_base_bits(settings: &AppSettings, manifest: &AnchorManifest) -> u32 {
    let mut bits = DEFAULT_LOCK_POW_BASE_BITS;
    for srv in &manifest.servers {
        let norm = normalize_sync_write_url(&srv.write_url);
        if let Some(probe) = settings.l2_probe_cache.get(&norm) {
            bits = bits.max(probe.pow_base_bits);
        }
    }
    bits
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
        let mut service = state
            .service
            .lock()
            .map_err(|_| fail("Wallet service unavailable"))?;
        if let Some(completion) = service
            .replay_checkout_purchase(purchase_id, bundle)
            .map_err(FrontendError::from)?
        {
            let receive_payload = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                false,
                completion.verdict.clone().into(),
                password,
            )
            .map(|(payload, _)| payload)
            .ok();

            return Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                verdict: completion.verdict.into(),
                receive_payload,
            });
        }
    }

    // 2. Decrypt & inspect bundle to check N = 0 offline-only vs anchored vouchers
    let (opened_bundle, merchant_id) = {
        let service = state
            .service
            .lock()
            .map_err(|_| fail("Wallet service unavailable"))?;
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
            let mut service = state
                .service
                .lock()
                .map_err(|_| fail("Wallet service unavailable"))?;
            build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                false,
                verdict_fe.clone(),
                password,
            )
            .map(|(p, _)| p)
            .ok()
        };
        return Ok(PurchaseCompletion {
            purchase_id: purchase_id.to_string(),
            newly_committed: false,
            verdict: verdict_fe,
            receive_payload,
        });
    }

    if opened_bundle.vouchers.len() != 1 {
        return Err(fail(
            "Automatic Sync-Lock checkout requires a single anchored voucher",
        ));
    }
    let voucher = opened_bundle.vouchers[0].clone();
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let client = sync_client::create_sync_http_client();
    let pow_base_bits = opened_bundle
        .anchor_manifest
        .as_ref()
        .map(|m| determine_pow_base_bits(&settings, m))
        .unwrap_or(DEFAULT_LOCK_POW_BASE_BITS);

    // 3. Prepare durable challenge context + per-server SyncLockBatchRequests
    let (context, requests) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| fail("Wallet service unavailable"))?;
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

    // 4. Resolve trusted LockPubKeys & fan out committee batches in parallel
    let (base_trusted_keys, independently_pinned_creator_keys) = resolve_trusted_lock_keys(
        state,
        &settings,
        &client,
        voucher.creator_profile.id.as_deref(),
        Some(&context.manifest),
    )
    .await;

    let dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
        &client,
        &requests,
        CHECKOUT_BATCH_TIMEOUT,
    )
    .await;

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

    let mut service = state
        .service
        .lock()
        .map_err(|_| fail("Wallet service unavailable"))?;
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
        L2ProcessingOutcome::Confirmed { .. } => {
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

            let (receive_payload, record_opt) = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                completion.newly_committed,
                completion.verdict.clone().into(),
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

            Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                verdict: completion.verdict.into(),
                receive_payload: Some(receive_payload),
            })
        }
        L2ProcessingOutcome::Yellow { reason } => {
            let verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                CheckoutReleaseVerdict::new(L2ProcessingOutcome::Yellow { reason }, false).into();
            let receive_payload = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                false,
                verdict_fe.clone(),
                password,
            )
            .map(|(p, _)| p)
            .ok();
            Ok(PurchaseCompletion {
                purchase_id: purchase_id.to_string(),
                newly_committed: false,
                verdict: verdict_fe,
                receive_payload,
            })
        }
        L2ProcessingOutcome::Quarantined { reason } => {
            let verdict_fe: crate::models::FrontendCheckoutReleaseVerdict =
                CheckoutReleaseVerdict::new(L2ProcessingOutcome::Quarantined { reason }, false)
                    .into();
            Ok(PurchaseCompletion {
                purchase_id: purchase_id.to_string(),
                newly_committed: false,
                verdict: verdict_fe,
                receive_payload: None,
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
        *state_bob.settings.lock().unwrap() = Some(AppSettings::default());
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
        *state_bob.settings.lock().unwrap() = Some(AppSettings::default());
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
            let mut st = state_bob.settings.lock().unwrap();
            let s = st.as_mut().unwrap();
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
                s.l2_probe_cache.insert(
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
        assert_eq!(green.verdict.outcome, "Confirmed");
        assert!(green.verdict.can_release_goods_automatically);
        assert!(green.verdict.is_goods_release_authorized);

        h1.abort();
        h2.abort();
        drop(state_bob);
        drop(svc_alice);
        let _ = std::fs::remove_dir_all(dir_alice);
        let _ = std::fs::remove_dir_all(dir_bob);
    }
}
