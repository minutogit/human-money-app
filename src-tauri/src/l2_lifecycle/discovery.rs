use super::offenders::resolve_current_did;
use super::*;
use std::collections::{BTreeMap, HashSet};

use human_money_core::humoco_proto::{
    decode_hex_fixed, types::LockPubKey, AnchorManifest, ServerEntry,
};
use humoco_market::CardStore;

use crate::models::{FrontendAddressBook, FrontendError};
use crate::services::common::{
    normalize_sync_url, resolve_server_locator, upsert_verified_lock_probe,
};
use crate::settings::{is_lock_admin_trusted, SecurityPolicy};
use crate::state::{AppState, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY};
use crate::sync_client::{self, FrontendLockServerProbeResult, SyncRevocationNotice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerProbeOutcome {
    Healthy,
    Unhealthy,
    Revoked,
}

/// Pure decision helper classifying a server's `/info` response into healthy, unhealthy, or revoked
/// strictly against the expected manifest operator/lock identity (`admin_pub` and `lock_pub`).
pub(crate) fn classify_server_probe_info(
    info: &sync_client::SyncServerInfo,
    expected_lock_hex: &str,
    expected_admin_hex: &str,
    now_secs: u64,
) -> ServerProbeOutcome {
    let revoked = info.revocation.as_ref().is_some_and(|rev| {
        sync_client::verify_revocation_notice(rev, expected_admin_hex).is_ok()
            && rev.effective_at <= now_secs
    });
    if revoked {
        return ServerProbeOutcome::Revoked;
    }
    let healthy = info.lock.as_ref().is_some_and(|l| {
        l.lock_pub.trim().eq_ignore_ascii_case(expected_lock_hex)
            && l.delegation.as_ref().is_some_and(|d| {
                sync_client::verify_lock_delegation(expected_lock_hex, d, Some(expected_admin_hex))
            })
    });
    if healthy {
        ServerProbeOutcome::Healthy
    } else {
        ServerProbeOutcome::Unhealthy
    }
}

/// Validates a signed same-`lock_pub` relocation notice and replacement probe, updating `probe_cache`
/// without revoking `expected_srv.lock_pub`, and returning the normalized verified probe on update.
pub(crate) fn try_apply_relocation_from_notice(
    probe_cache: &mut BTreeMap<String, FrontendLockServerProbeResult>,
    expected_srv: &ServerEntry,
    notice: &SyncRevocationNotice,
    replacement_probe: FrontendLockServerProbeResult,
) -> Option<FrontendLockServerProbeResult> {
    let admin_hex = hex::encode(expected_srv.admin_pub);
    let lock_hex = hex::encode(expected_srv.lock_pub);
    if sync_client::verify_revocation_notice(notice, &admin_hex).is_err() {
        return None;
    }
    let rep = notice.replacement.as_ref()?;
    if !rep.drop_pub.trim().eq_ignore_ascii_case(&admin_hex)
        || sync_client::validate_sync_url(rep.write_url.trim()).is_err()
        || sync_client::validate_sync_read_url(rep.read_url.trim()).is_err()
    {
        return None;
    }
    if !replacement_probe.delegation_valid || !replacement_probe.passed {
        return None;
    }
    if !replacement_probe
        .lock_pub
        .trim()
        .eq_ignore_ascii_case(&lock_hex)
        || !replacement_probe
            .admin_pub
            .trim()
            .eq_ignore_ascii_case(&admin_hex)
    {
        return None;
    }
    let rep_w = normalize_sync_url(&rep.write_url);
    let rep_r = normalize_sync_url(&rep.read_url);
    let (cur_w, cur_r) = resolve_server_locator(expected_srv, probe_cache);
    if rep_w.is_empty()
        || rep_r.is_empty()
        || (rep_w == normalize_sync_url(&cur_w) && rep_r == normalize_sync_url(&cur_r))
        || normalize_sync_url(&replacement_probe.write_url) != rep_w
        || normalize_sync_url(&replacement_probe.read_url) != rep_r
    {
        return None;
    }
    if !replacement_probe.delegation.as_ref().is_some_and(|d| {
        normalize_sync_url(&d.write_url) == rep_w
            && normalize_sync_url(&d.read_url) == rep_r
            && crate::services::common::is_valid_delegation_for_entry(expected_srv, d)
    }) {
        return None;
    }
    upsert_verified_lock_probe(probe_cache, replacement_probe);
    probe_cache
        .get(&lock_hex)
        .filter(|c| {
            c.passed
                && c.delegation_valid
                && normalize_sync_url(&c.write_url) == rep_w
                && normalize_sync_url(&c.read_url) == rep_r
        })
        .cloned()
}

pub(crate) async fn try_follow_relocation_notice(
    client: &reqwest::Client,
    probe_cache: &mut BTreeMap<String, FrontendLockServerProbeResult>,
    expected_srv: &ServerEntry,
    notice: &SyncRevocationNotice,
) -> Option<FrontendLockServerProbeResult> {
    let admin_hex = hex::encode(expected_srv.admin_pub);
    let lock_hex = hex::encode(expected_srv.lock_pub);
    if sync_client::verify_revocation_notice(notice, &admin_hex).is_err() {
        return None;
    }
    let rep = notice.replacement.as_ref()?;
    if !rep.drop_pub.trim().eq_ignore_ascii_case(&admin_hex)
        || sync_client::validate_sync_url(rep.write_url.trim()).is_err()
        || sync_client::validate_sync_read_url(rep.read_url.trim()).is_err()
    {
        return None;
    }
    let rep_w = normalize_sync_url(&rep.write_url);
    let rep_r = normalize_sync_url(&rep.read_url);
    let (cur_w, cur_r) = resolve_server_locator(expected_srv, probe_cache);
    if rep_w == normalize_sync_url(&cur_w) && rep_r == normalize_sync_url(&cur_r) {
        return None;
    }
    if let Some(cached) = probe_cache.get(&lock_hex) {
        if cached.passed
            && cached.delegation_valid
            && normalize_sync_url(&cached.write_url) == rep_w
            && normalize_sync_url(&cached.read_url) == rep_r
        {
            return Some(cached.clone());
        }
    }
    let Ok(rep_probe) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        sync_client::probe_lock_server(client, &rep.write_url, Some(&admin_hex)),
    )
    .await
    else {
        return None;
    };
    try_apply_relocation_from_notice(probe_cache, expected_srv, notice, rep_probe)
}

/// Pure decision helper applying verified effective `SyncRevocationNotice` entries to anchor health,
/// skipping servers whose `lock_pub` relocated cleanly via a verified replacement probe.
pub(crate) fn apply_revocation_notices_to_health(
    health: &mut StoredAnchorHealth,
    notices: &[(LockPubKey, SyncRevocationNotice)],
    expected_servers: &[ServerEntry],
    relocated_lock_pubs: &HashSet<LockPubKey>,
    now_secs: u64,
) {
    for (lock_pub, notice) in notices {
        let lock_hex = hex::encode(lock_pub);
        if relocated_lock_pubs.contains(lock_pub) {
            health.unhealthy_since_by_lock_pub.remove(&lock_hex);
            continue;
        }
        let Some(srv) = expected_servers.iter().find(|s| s.lock_pub == *lock_pub) else {
            continue;
        };
        let expected_admin = hex::encode(srv.admin_pub);
        if sync_client::verify_revocation_notice(notice, &expected_admin).is_ok()
            && notice.effective_at <= now_secs
        {
            health.revoked_lock_pubs.insert(lock_hex);
        }
    }
}

/// Pure 5-day hysteresis and immediate-revocation state machine for server health demotions.
///
/// Clears servers that recovered in the current tick, preserves original `unhealthy_since`
/// timestamps for persistent failures, records newly revoked servers immediately, and returns
/// all `LockPubKey`s eligible for committee rotation.
pub(crate) fn apply_health_observations_and_collect_demotions(
    health: &mut StoredAnchorHealth,
    unhealthy_now: &HashSet<String>,
    revoked_now: &HashSet<String>,
    now_secs: u64,
) -> HashSet<LockPubKey> {
    health.last_checked_secs = now_secs;

    // Clear servers that came back healthy
    health
        .unhealthy_since_by_lock_pub
        .retain(|k, _| unhealthy_now.contains(k));
    for lk in unhealthy_now {
        health
            .unhealthy_since_by_lock_pub
            .entry(lk.clone())
            .or_insert(now_secs);
    }
    for lk in revoked_now {
        health.revoked_lock_pubs.insert(lk.clone());
    }

    let mut failed: HashSet<LockPubKey> = HashSet::new();
    for lk in &health.revoked_lock_pubs {
        if let Ok(pk) = decode_hex_fixed::<32>(lk) {
            failed.insert(pk);
        }
    }
    for (lk, &since) in &health.unhealthy_since_by_lock_pub {
        if now_secs.saturating_sub(since) >= UNHEALTHY_HYSTERESIS_SECS {
            if let Ok(pk) = decode_hex_fixed::<32>(lk) {
                failed.insert(pk);
            }
        }
    }
    failed
}

pub fn record_revoked_servers(
    service: &mut human_money_core::app_service::AppService,
    anchor_id: &[u8; 32],
    notices: &[(LockPubKey, SyncRevocationNotice)],
    expected_servers: &[ServerEntry],
    relocated_lock_pubs: &HashSet<LockPubKey>,
    now_secs: u64,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    if notices.is_empty() {
        return Ok(());
    }
    let anchor_hex = hex::encode(anchor_id);
    let mut lc = load_lifecycle_state(service, password)?;
    let health = lc
        .anchors
        .entry(anchor_hex.clone())
        .or_insert_with(|| StoredAnchorHealth::new(anchor_hex));
    apply_revocation_notices_to_health(
        health,
        notices,
        expected_servers,
        relocated_lock_pubs,
        now_secs,
    );
    save_lifecycle_state(service, &lc, password)
}

fn push_unique_url(out: &mut Vec<String>, seen: &mut HashSet<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() || sync_client::validate_sync_url(trimmed).is_err() {
        return;
    }
    let key = trimmed.trim_end_matches('/').to_string();
    if seen.insert(key) {
        out.push(trimmed.to_string());
    }
}

fn collect_did_drop_urls(
    did: &str,
    card_store: &CardStore,
    address_book: Option<&FrontendAddressBook>,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if let Ok(validated) = human_money_core::ValidatedUserId::parse(did) {
        let pk_bytes: [u8; 32] = validated.public_key_bytes();
        for stored_card in card_store.cards.values() {
            if human_money_core::crypto::get_pubkey_from_user_id(&stored_card.card.payload.did)
                .ok()
                .map(|k| k.to_bytes())
                == Some(pk_bytes)
            {
                for drop_desc in &stored_card.card.payload.drops {
                    push_unique_url(out, seen, &drop_desc.read_url);
                    push_unique_url(out, seen, &drop_desc.write_url);
                }
            }
        }
    }
    if let Some(book) = address_book {
        let can_target = crate::state::canonicalize_did(did).ok();
        for contact in book.contacts.values() {
            let matches_did = contact.did.trim() == did.trim()
                || (can_target.is_some()
                    && crate::state::canonicalize_did(&contact.did).ok() == can_target);
            if matches_did {
                for sync in &contact.syncs {
                    if sync.status != "revoked" {
                        push_unique_url(out, seen, &sync.read_url);
                        push_unique_url(out, seen, &sync.write_url);
                    }
                }
            }
        }
    }
}

/// Executes the 4-stage AnchorManifest Discovery Cascade (`Concept §5.6`):
/// 1. Local `wallet.profile.l2_anchor_manifests` sidecar/cache (0 ms).
/// 2. Creator's CardStore + AddressBook drops (resolved across verified `KeySuccessorAttestation`s).
/// 3. Guarantor Anchor Mirroring drops (`guarantor_dids` resolved across `KeySuccessorAttestation`s).
/// 4. Regional / community Sync servers (`public_syncs`, `l2_probe_cache`, known manifest servers).
///
/// Only accepts manifests cryptographically verifiable from Genesis `anchor_id`, monotonically
/// keeps the highest `revision`, and updates wallet storage via `update_l2_anchor_manifest`.
pub async fn discover_anchor_manifest(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    anchor_id: &[u8; 32],
    creator_did: Option<&str>,
    guarantor_dids: &[String],
    password: Option<&str>,
) -> Result<Option<AnchorManifest>, FrontendError> {
    let anchor_hex = hex::encode(anchor_id);
    let now_secs = sync_client::current_unix_timestamp();

    let (
        mut best_manifest,
        local_rev,
        card_store,
        address_book_opt,
        resolved_creator,
        resolved_guarantors,
        security_policy,
        lifecycle,
    ) = state.with_session_required(
        session,
        "Wallet session changed during anchor discovery",
        |service| {
            let security_policy = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            let lifecycle = state
                .get_or_load_internal_lifecycle(service, password)
                .map_err(FrontendError::from)?;

            let local_manifest = service
                .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
                .map_err(FrontendError::from)?;
            let local_manifest = local_manifest.filter(|m| m.verify_anchor_id(anchor_id).is_ok());
            let local_rev = local_manifest.as_ref().map(|m| m.revision).unwrap_or(0);

            let card_store: CardStore = crate::state::load_encrypted_json_or_default(
                service,
                MARKET_CARD_STORE_KEY,
                password,
            )
            .map_err(FrontendError::from)?;
            let address_book_opt: Option<FrontendAddressBook> =
                match state.read_cache_slot_for_service(service, |c| &c.contacts) {
                    Some(cached) => Some(cached),
                    None => {
                        crate::state::load_encrypted_json_opt(service, CONTACTS_DATA_NAME, password)
                            .map_err(FrontendError::from)?
                    }
                };

            let resolved_creator = match creator_did {
                Some(c) => {
                    let resolved = resolve_current_did(service, c, now_secs, password)?;
                    Some((c.to_string(), resolved))
                }
                None => None,
            };
            let mut resolved_guarantors: Vec<(String, String)> =
                Vec::with_capacity(guarantor_dids.len());
            for g in guarantor_dids {
                let resolved = resolve_current_did(service, g, now_secs, password)?;
                resolved_guarantors.push((g.clone(), resolved));
            }

            Ok((
                local_manifest,
                local_rev,
                card_store,
                address_book_opt,
                resolved_creator,
                resolved_guarantors,
                security_policy,
                lifecycle,
            ))
        },
    )?;

    let address_book_ref = address_book_opt.as_ref();

    let mut stage_urls: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Stage 2: Creator CardStore + AddressBook (resolved via KeySuccessorAttestation chain)
    if let Some((orig_creator, curr_creator)) = resolved_creator {
        collect_did_drop_urls(
            &curr_creator,
            &card_store,
            address_book_ref,
            &mut stage_urls,
            &mut seen,
        );
        if orig_creator != curr_creator {
            collect_did_drop_urls(
                &orig_creator,
                &card_store,
                address_book_ref,
                &mut stage_urls,
                &mut seen,
            );
        }
    }

    // Stage 3: Guarantor Anchor Mirroring
    for (orig_g, curr_g) in resolved_guarantors {
        collect_did_drop_urls(
            &curr_g,
            &card_store,
            address_book_ref,
            &mut stage_urls,
            &mut seen,
        );
        if orig_g != curr_g {
            collect_did_drop_urls(
                &orig_g,
                &card_store,
                address_book_ref,
                &mut stage_urls,
                &mut seen,
            );
        }
    }

    // Stage 4: Regional / Community servers & locally known manifest servers
    for s in &security_policy.public_syncs {
        if s.enabled && s.status != "revoked" {
            push_unique_url(&mut stage_urls, &mut seen, &s.read_url);
            push_unique_url(&mut stage_urls, &mut seen, &s.write_url);
        }
    }
    for d in &security_policy.market_selected_syncs {
        push_unique_url(&mut stage_urls, &mut seen, &d.read_url);
        push_unique_url(&mut stage_urls, &mut seen, &d.write_url);
    }
    for probe in lifecycle.l2_probe_cache.values() {
        if probe.passed {
            push_unique_url(&mut stage_urls, &mut seen, &probe.read_url);
            push_unique_url(&mut stage_urls, &mut seen, &probe.write_url);
        }
    }
    if let Some(ref m) = best_manifest {
        for srv in &m.servers {
            let (resolved_w, resolved_r) = resolve_server_locator(srv, &lifecycle.l2_probe_cache);
            push_unique_url(&mut stage_urls, &mut seen, &resolved_r);
            push_unique_url(&mut stage_urls, &mut seen, &resolved_w);
            push_unique_url(&mut stage_urls, &mut seen, &srv.read_url);
            push_unique_url(&mut stage_urls, &mut seen, &srv.write_url);
        }
    }

    for base_url in stage_urls {
        if let Ok(Some(candidate)) =
            sync_client::fetch_anchor_manifest_bin(client, &base_url, anchor_id).await
        {
            if candidate.verify_anchor_id(anchor_id).is_ok()
                && best_manifest
                    .as_ref()
                    .map(|b| candidate.revision > b.revision)
                    .unwrap_or(true)
            {
                best_manifest = Some(candidate);
            }
        }
    }

    if let Some(ref found) = best_manifest {
        if found.revision > local_rev {
            state.with_session_required(
                session,
                "Wallet session changed before saving discovered anchor manifest",
                |service| {
                    service
                        .update_l2_anchor_manifest(found.clone(), password)
                        .map_err(FrontendError::from)?;
                    Ok(())
                },
            )?;
        }
    }

    Ok(best_manifest)
}

/// Mirrors a verified `revision >= 2` `AnchorManifest` onto own Sync servers for Guarantor/Creator Anchor Mirroring (§5.6 Stage 3).
pub async fn mirror_manifest_to_own_syncs(
    client: &reqwest::Client,
    security_policy: &SecurityPolicy,
    anchor_id: &[u8; 32],
    manifest: &AnchorManifest,
) {
    if manifest.revision < 2 {
        return;
    }
    let mut urls = HashSet::new();
    for s in &security_policy.public_syncs {
        if s.enabled && s.status != "revoked" && is_lock_admin_trusted(security_policy, &s.drop_pub)
        {
            urls.insert(s.write_url.clone());
        }
    }
    for d in &security_policy.market_selected_syncs {
        if is_lock_admin_trusted(security_policy, &d.drop_pub) {
            urls.insert(d.write_url.clone());
        }
    }
    for write_url in urls {
        let _ =
            sync_client::post_lock_commit_finalize(client, &write_url, anchor_id, manifest).await;
    }
}
