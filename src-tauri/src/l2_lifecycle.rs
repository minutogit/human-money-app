//! Sync-Lock committee lifecycle orchestration (`APP-071`, Concept §5.4–§5.7):
//! - Permanent `ServerOffenderList` persistence and equivocation recording (`WOT-013`).
//! - `KeySuccessorAttestation` storage and 4-stage long-term manifest discovery cascade (§5.6).
//! - Periodic server health checks with 5-day hysteresis (immediate on `SyncRevocationNotice` / `410`).
//! - Crash-resumable 3-phase committee rotation (`propose` -> `fetch_snapshot`/`import` -> `commit`/`finalize`).
//! - Automatic guarantor vetoing of unauthorized or protection-reducing proposals during timelock.
//! - Regular L1 `Tx_claim` at guarantors (§5.7).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ed25519_dalek::SigningKey;
use human_money_core::humoco_proto::{
    self,
    crypto::{verify_ed25519_strict, DEFAULT_LOCK_POW_BASE_BITS},
    decode_hex_fixed, resolve_import_union,
    types::{EquivocationProof, LockPubKey, LockReceiptKind, QuorumCertificate},
    AnchorManifest, CommitteeSignature, ImportAck, LockEntry, MigrationCert, MigrationProposal,
    NominatedBy, ServerEntry, SnapshotExport,
};
use human_money_core::services::{l2_gateway, l2_rotation};
use humoco_market::CardStore;
use humoco_wot::{
    resolve_successor_chain, KeySuccessorAttestation, ServerOffenderList, ServerOffenderRecord,
    WotEquivocationProof, WotLockDelegation, WotLockReceipt, WotLockReceiptKind,
};
use serde::{Deserialize, Serialize};

use crate::gateway_resolver::{sybil_fold_committee, SybilServerCandidate};
use crate::models::{FrontendAddressBook, FrontendError};
use crate::services::common::{
    normalize_sync_url, resolve_server_locator, upsert_verified_lock_probe,
};
use crate::settings::{is_lock_admin_trusted, InternalLifecycleState, SecurityPolicy};
use crate::state::{AppState, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY};
use crate::sync_client::{self, FrontendLockServerProbeResult, SyncRevocationNotice};

pub const SERVER_OFFENDER_LIST_KEY: &str = "l2_server_offenders";
pub const KEY_SUCCESSOR_STORE_KEY: &str = "wot_key_successors";
pub const L2_LIFECYCLE_STATE_KEY: &str = "l2_lifecycle_state";
pub const HEALTH_CHECK_INTERVAL_SECS: u64 = 6 * 3600;
pub const UNHEALTHY_HYSTERESIS_SECS: u64 = 5 * 86_400;

pub(crate) fn lifecycle_err(msg: impl Into<String>) -> FrontendError {
    FrontendError::from(msg.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredGuarantorRole {
    pub anchor_id: String,
    pub guarantor_index: u8,
    pub context_nonce_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum StoredRotationPhase {
    Proposed {
        proposal: MigrationProposal,
        effective_at_secs: u64,
    },
    Imported {
        proposal: MigrationProposal,
        imported_state_hash_hex: String,
        import_acks: Vec<ImportAck>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredAnchorHealth {
    pub anchor_id: String,
    pub last_checked_secs: u64,
    pub unhealthy_since_by_lock_pub: BTreeMap<String, u64>,
    pub revoked_lock_pubs: BTreeSet<String>,
    pub active_rotation: Option<StoredRotationPhase>,
    pub last_rotated_rev: u64,
    pub last_error: Option<String>,
}

impl StoredAnchorHealth {
    fn new(anchor_id: String) -> Self {
        Self {
            anchor_id,
            last_checked_secs: 0,
            unhealthy_since_by_lock_pub: BTreeMap::new(),
            revoked_lock_pubs: BTreeSet::new(),
            active_rotation: None,
            last_rotated_rev: 1,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredL2LifecycleState {
    pub last_tick_secs: u64,
    pub anchors: BTreeMap<String, StoredAnchorHealth>,
    pub guarantor_roles: BTreeMap<String, StoredGuarantorRole>,
    pub auto_vetoes_sent: u64,
    pub last_veto_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAnchorLifecycleDto {
    pub anchor_id: String,
    pub revision: u64,
    pub reduced_guarantor_protection: bool,
    pub unhealthy_servers: Vec<String>,
    pub revoked_servers: Vec<String>,
    pub active_rotation_phase: Option<String>,
    pub effective_at_secs: Option<u64>,
    pub last_checked_secs: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendL2LifecycleStatusDto {
    pub last_tick_secs: u64,
    pub tracked_anchors_count: usize,
    pub guarantor_roles_count: usize,
    pub server_offenders_count: usize,
    pub auto_vetoes_sent: u64,
    pub last_veto_reason: Option<String>,
    pub anchors: Vec<FrontendAnchorLifecycleDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAnchorDiscoveryDto {
    pub anchor_id: String,
    pub revision: u64,
    pub threshold: u8,
    pub server_count: usize,
    pub reduced_guarantor_protection: bool,
    pub manifest_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendClaimBundleDto {
    pub bundle_data: Vec<u8>,
    pub bundle_id: String,
    pub guarantor_id: String,
    pub claimed_amount: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_deposit: Option<crate::models::FrontendPostDepositResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_deposit_error: Option<String>,
}

pub fn load_server_offender_list(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<ServerOffenderList, FrontendError> {
    crate::state::load_encrypted_json_opt::<ServerOffenderList>(
        service,
        SERVER_OFFENDER_LIST_KEY,
        password,
    )
    .map(|opt| opt.unwrap_or_else(ServerOffenderList::new))
    .map_err(FrontendError::from)
}

pub fn save_server_offender_list(
    service: &mut human_money_core::app_service::AppService,
    list: &ServerOffenderList,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, SERVER_OFFENDER_LIST_KEY, list, password)
        .map_err(FrontendError::from)
}

fn to_wot_receipt(r: &human_money_core::humoco_proto::types::LockReceipt) -> WotLockReceipt {
    WotLockReceipt {
        kind: match r.kind {
            LockReceiptKind::FirstWrite => WotLockReceiptKind::FirstWrite,
            LockReceiptKind::ConflictSeen => WotLockReceiptKind::ConflictSeen,
        },
        lock_pub: r.lock_pub,
        bucket: r.bucket,
        slot: r.slot,
        child_lock: r.child_lock,
        entry_hash: r.entry_hash,
        retain_until_quarter: r.retain_until_quarter,
        write_seq: r.write_seq,
        bucket_head: r.bucket_head,
        recipient_nonce: r.recipient_nonce,
        sig: r.sig,
    }
}

pub fn is_server_key_revoked(
    offenders: &ServerOffenderList,
    lock_pub: &LockPubKey,
    admin_pub: Option<&[u8; 32]>,
) -> bool {
    offenders.is_lock_pub_revoked(lock_pub)
        || admin_pub.is_some_and(|a| offenders.is_admin_pub_revoked(a))
}

fn record_equivocation_proof_inner(
    service: &mut human_money_core::app_service::AppService,
    delegation_opt: Option<WotLockDelegation>,
    proof: &EquivocationProof,
    now_secs: u64,
    password: Option<&str>,
) -> Result<(bool, ServerOffenderList), FrontendError> {
    let wot_proof = WotEquivocationProof {
        receipt_a: to_wot_receipt(&proof.receipt_a),
        receipt_b: to_wot_receipt(&proof.receipt_b),
    };
    let offending_lock = proof.receipt_a.lock_pub;
    let mut list = load_server_offender_list(service, password)?;
    let already = list.is_lock_pub_revoked(&offending_lock);
    let record = ServerOffenderRecord {
        proof: wot_proof,
        delegation: delegation_opt,
        recorded_at: now_secs.max(1),
    };
    list.insert_verified(record, now_secs.max(1))
        .map_err(|e| lifecycle_err(format!("Invalid equivocation proof: {}", e)))?;
    save_server_offender_list(service, &list, password)?;
    Ok((!already, list))
}

pub fn record_equivocation_proof(
    service: &mut human_money_core::app_service::AppService,
    delegation_opt: Option<WotLockDelegation>,
    proof: &EquivocationProof,
    now_secs: u64,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    record_equivocation_proof_inner(service, delegation_opt, proof, now_secs, password)
        .map(|(newly, _)| newly)
}

pub(crate) fn sync_delegation_to_wot(
    del: &sync_client::SyncLockDelegationDto,
) -> Option<WotLockDelegation> {
    let admin_pub = decode_hex_fixed::<32>(del.admin_pub.trim()).ok()?;
    let lock_pub = decode_hex_fixed::<32>(del.lock_pub.trim()).ok()?;
    let sig = decode_hex_fixed::<64>(del.sig.trim()).ok()?;
    Some(WotLockDelegation {
        admin_pub,
        lock_pub,
        write_url: del.write_url.trim().to_string(),
        read_url: del.read_url.trim().to_string(),
        issued_at: del.issued_at,
        sig,
    })
}

/// Persists a verified `EquivocationProof`, attaching the verified `WotLockDelegation` from
/// `probe_cache` when available (enabling ADR-10 `revoked_admin_pubs` operator bans), and updates
/// `state`'s `server_offenders` cache slot atomically while `service` is held.
pub fn record_equivocation_and_refresh_cache(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    probe_cache: &BTreeMap<String, FrontendLockServerProbeResult>,
    proof: &EquivocationProof,
    now_secs: u64,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    let lock_hex = hex::encode(proof.receipt_a.lock_pub);
    let wot_delegation = probe_cache
        .get(&lock_hex)
        .filter(|probe| probe.delegation_valid)
        .and_then(|probe| probe.delegation.as_ref())
        .filter(|del| sync_client::verify_lock_delegation(&lock_hex, del, None))
        .and_then(sync_delegation_to_wot);
    let (newly_recorded, updated_list) =
        record_equivocation_proof_inner(service, wot_delegation, proof, now_secs, password)?;
    state.write_cache_slot_for_service(service, |c| &mut c.server_offenders, Some(updated_list));
    Ok(newly_recorded)
}

pub fn load_key_successor_attestations(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<Vec<KeySuccessorAttestation>, FrontendError> {
    crate::state::load_encrypted_json_or_default(service, KEY_SUCCESSOR_STORE_KEY, password)
        .map_err(FrontendError::from)
}

pub fn save_key_successor_attestations(
    service: &mut human_money_core::app_service::AppService,
    list: &[KeySuccessorAttestation],
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, KEY_SUCCESSOR_STORE_KEY, &list, password)
        .map_err(FrontendError::from)
}

pub fn resolve_current_did(
    service: &mut human_money_core::app_service::AppService,
    did: &str,
    now_secs: u64,
    password: Option<&str>,
) -> Result<String, FrontendError> {
    let attestations = load_key_successor_attestations(service, password)?;
    let canonical = crate::state::canonicalize_did(did)
        .map_err(|e| FrontendError::from(format!("Invalid DID '{}': {}", did.trim(), e)))?;
    Ok(resolve_successor_chain(&canonical, &attestations, now_secs).unwrap_or(canonical))
}

pub fn load_lifecycle_state(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<StoredL2LifecycleState, FrontendError> {
    crate::state::load_encrypted_json_or_default(service, L2_LIFECYCLE_STATE_KEY, password)
        .map_err(FrontendError::from)
}

pub fn save_lifecycle_state(
    service: &mut human_money_core::app_service::AppService,
    lc_state: &StoredL2LifecycleState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, L2_LIFECYCLE_STATE_KEY, lc_state, password)
        .map_err(FrontendError::from)
}

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

#[allow(clippy::too_many_arguments)]
pub(crate) fn select_rotation_proposal(
    anchor_id: [u8; 32],
    manifest: &AnchorManifest,
    failed_lock_pubs: &HashSet<LockPubKey>,
    creator_mig_sk: Option<&SigningKey>,
    guarantor_auth: Option<(u8, &SigningKey, u64)>,
    security_policy: &SecurityPolicy,
    lifecycle: &InternalLifecycleState,
    offenders: &ServerOffenderList,
    now_secs: u64,
) -> Option<MigrationProposal> {
    if failed_lock_pubs.is_empty() {
        return None;
    }

    // 1. Try authority-free HolderStandby promotion if standby pool covers all failed seats
    // and every promoted standby successor is non-revoked and locally trusted.
    let existing_pubs: HashSet<LockPubKey> = manifest.servers.iter().map(|s| s.lock_pub).collect();
    let failed_vec: Vec<LockPubKey> = failed_lock_pubs.iter().copied().collect();
    if let Ok(prop) =
        l2_rotation::build_holder_standby_proposal(anchor_id, manifest, &failed_vec, now_secs)
    {
        let promoted_ok = prop.new_servers.iter().all(|s| {
            existing_pubs.contains(&s.lock_pub)
                || (!is_server_key_revoked(offenders, &s.lock_pub, Some(&s.admin_pub))
                    && is_lock_admin_trusted(security_policy, &hex::encode(s.admin_pub)))
        });
        if promoted_ok {
            return Some(prop);
        }
    }

    // Preserve all declared successors initially so non-signing nominators' FIFO standby lists
    // remain intact for `humoco-proto::verify_nomination_binding`.
    let mut remaining_successors: Vec<ServerEntry> = manifest.successors.clone();
    let mut new_servers: Vec<ServerEntry> = Vec::with_capacity(manifest.servers.len());
    let mut used_lock_pubs: HashSet<LockPubKey> = manifest
        .servers
        .iter()
        .filter(|s| !failed_lock_pubs.contains(&s.lock_pub))
        .map(|s| s.lock_pub)
        .collect();

    let mut sybil_existing: Vec<SybilServerCandidate> = manifest
        .servers
        .iter()
        .filter(|s| !failed_lock_pubs.contains(&s.lock_pub))
        .map(|s| {
            let (resolved_w, _) = resolve_server_locator(s, &lifecycle.l2_probe_cache);
            SybilServerCandidate {
                write_url: resolved_w,
                lock_pub: s.lock_pub,
                admin_pub: Some(s.admin_pub),
                asn: None,
            }
        })
        .collect();

    let is_role_signed = |role: NominatedBy| match role {
        NominatedBy::Creator => creator_mig_sk.is_some(),
        NominatedBy::Guarantor(g_idx) => guarantor_auth
            .as_ref()
            .is_some_and(|(auth_idx, _, _)| *auth_idx == g_idx),
    };
    let is_successor_eligible = |s: &ServerEntry, used: &HashSet<LockPubKey>| {
        !used.contains(&s.lock_pub)
            && !failed_lock_pubs.contains(&s.lock_pub)
            && !is_server_key_revoked(offenders, &s.lock_pub, Some(&s.admin_pub))
            && is_lock_admin_trusted(security_policy, &hex::encode(s.admin_pub))
    };

    for srv in &manifest.servers {
        if !failed_lock_pubs.contains(&srv.lock_pub) {
            new_servers.push(srv.clone());
            continue;
        }
        let can_pick_probe = is_role_signed(srv.nominated_by);
        let promoted_pos = if can_pick_probe {
            remaining_successors.iter().position(|c| {
                c.nominated_by == srv.nominated_by && is_successor_eligible(c, &used_lock_pubs)
            })
        } else {
            // Non-signing nominator: strictly respect FIFO head without dropping/skipping successors.
            remaining_successors
                .iter()
                .position(|c| c.nominated_by == srv.nominated_by)
                .filter(|&pos| is_successor_eligible(&remaining_successors[pos], &used_lock_pubs))
        };
        if let Some(pos) = promoted_pos {
            let succ = remaining_successors.remove(pos);
            let (resolved_w, _) = resolve_server_locator(&succ, &lifecycle.l2_probe_cache);
            used_lock_pubs.insert(succ.lock_pub);
            sybil_existing.push(SybilServerCandidate {
                write_url: resolved_w,
                lock_pub: succ.lock_pub,
                admin_pub: Some(succ.admin_pub),
                asn: None,
            });
            new_servers.push(succ);
            continue;
        }

        if can_pick_probe {
            let mut picked: Option<ServerEntry> = None;
            for probe in lifecycle.l2_probe_cache.values() {
                if !probe.passed
                    || !probe.delegation_valid
                    || probe.max_retention_quarter < manifest.retain_until_quarter
                    || !is_lock_admin_trusted(security_policy, &probe.admin_pub)
                {
                    continue;
                }
                let Ok(lock_pub) = decode_hex_fixed::<32>(&probe.lock_pub) else {
                    continue;
                };
                let Ok(admin_pub) = decode_hex_fixed::<32>(&probe.admin_pub) else {
                    continue;
                };
                if used_lock_pubs.contains(&lock_pub)
                    || is_server_key_revoked(offenders, &lock_pub, Some(&admin_pub))
                {
                    continue;
                }
                let cand = SybilServerCandidate {
                    write_url: probe.write_url.clone(),
                    lock_pub,
                    admin_pub: Some(admin_pub),
                    asn: None,
                };
                let mut trial = sybil_existing.clone();
                trial.push(cand.clone());
                if sybil_fold_committee(&trial).len() == trial.len() {
                    used_lock_pubs.insert(lock_pub);
                    sybil_existing.push(cand);
                    picked = Some(ServerEntry {
                        write_url: probe.write_url.clone(),
                        read_url: probe.read_url.clone(),
                        admin_pub,
                        lock_pub,
                        nominated_by: srv.nominated_by,
                    });
                    break;
                }
            }
            if let Some(rep) = picked {
                new_servers.push(rep);
                continue;
            }
        }
        // Otherwise shrink dead slot cleanly (Concept §5.4: never fill a guarantor seat with creator server).
    }

    let same_committee = new_servers.len() == manifest.servers.len()
        && new_servers
            .iter()
            .zip(&manifest.servers)
            .all(|(a, b)| a.same_identity(b));
    if new_servers.is_empty() || same_committee {
        return None;
    }

    let n = new_servers.len() as u8;
    let min_q = (n / 2) + 1;
    let new_threshold = manifest.threshold.min(n).max(min_q);
    if manifest.authority.guarantor_mig_pubs.is_empty()
        && manifest.threshold >= 2
        && new_threshold < 2
    {
        return None;
    }

    if let Some(c_sk) = creator_mig_sk {
        if let Ok(prop) = l2_rotation::build_creator_rotation_proposal(
            anchor_id,
            manifest,
            new_threshold,
            new_servers.clone(),
            remaining_successors.clone(),
            now_secs,
            c_sk,
        ) {
            return Some(prop);
        }
    }

    if let Some((g_idx, g_sk, last_creator_secs)) = guarantor_auth {
        if let Ok(prop) = l2_rotation::build_guarantor_takeover_proposal(
            anchor_id,
            manifest,
            new_threshold,
            new_servers,
            remaining_successors,
            now_secs,
            g_idx,
            g_sk,
            last_creator_secs,
            now_secs,
        ) {
            return Some(prop);
        }
    }

    None
}

/// Phase 5 Safe Auto-Growth: safely expands healthy `creator`-only committees with empty
/// `guarantor_mig_pubs` (`N < 3`) up to `N = 3` (`threshold = 2`), or `1 -> 2` (`threshold = 2`)
/// when a trusted friend/circle server (`market_selected_syncs` with `slot_id == None`, not own)
/// is added as the 2nd server. Preserved across ALL vouchers (`Incomplete` + `Active`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn select_auto_growth_proposal(
    anchor_id: [u8; 32],
    manifest: &AnchorManifest,
    creator_mig_sk: &SigningKey,
    security_policy: &SecurityPolicy,
    lifecycle: &InternalLifecycleState,
    offenders: &ServerOffenderList,
    known_source_prio_by_url: &BTreeMap<String, u8>,
    own_write_url_norm: Option<&str>,
    now_secs: u64,
) -> Option<MigrationProposal> {
    if !manifest.authority.guarantor_mig_pubs.is_empty()
        || manifest.servers.is_empty()
        || manifest.servers.len() >= 3
    {
        return None;
    }

    let circle_urls: HashSet<String> = security_policy
        .market_selected_syncs
        .iter()
        .filter(|d| d.slot_id.is_none())
        .map(|d| normalize_sync_url(&d.write_url))
        .filter(|u| !u.is_empty() && own_write_url_norm != Some(u.as_str()))
        .collect();

    let mut used_lock_pubs: HashSet<LockPubKey> =
        manifest.servers.iter().map(|s| s.lock_pub).collect();
    let mut sybil_existing: Vec<SybilServerCandidate> = manifest
        .servers
        .iter()
        .map(|s| {
            let (resolved_w, _) = resolve_server_locator(s, &lifecycle.l2_probe_cache);
            SybilServerCandidate {
                write_url: resolved_w,
                lock_pub: s.lock_pub,
                admin_pub: Some(s.admin_pub),
                asn: None,
            }
        })
        .collect();

    type RankedAutoGrowthProbe<'a> = (
        u8,
        u64,
        &'a FrontendLockServerProbeResult,
        [u8; 32],
        [u8; 32],
    );
    let mut ranked_probes: Vec<RankedAutoGrowthProbe<'_>> = Vec::new();
    for probe in lifecycle.l2_probe_cache.values() {
        if !probe.passed
            || !probe.delegation_valid
            || probe.max_retention_quarter < manifest.retain_until_quarter
            || !is_lock_admin_trusted(security_policy, &probe.admin_pub)
        {
            continue;
        }
        let norm_w = normalize_sync_url(&probe.write_url);
        let Some(&prio) = known_source_prio_by_url.get(&norm_w) else {
            continue;
        };
        let Ok(lock_pub) = decode_hex_fixed::<32>(&probe.lock_pub) else {
            continue;
        };
        let Ok(admin_pub) = decode_hex_fixed::<32>(&probe.admin_pub) else {
            continue;
        };
        if used_lock_pubs.contains(&lock_pub)
            || is_server_key_revoked(offenders, &lock_pub, Some(&admin_pub))
        {
            continue;
        }
        ranked_probes.push((prio, probe.latency_ms, probe, lock_pub, admin_pub));
    }
    ranked_probes.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then_with(|| a.3.cmp(&b.3))
    });

    let mut available_extra: Vec<ServerEntry> = Vec::new();
    for (_, _, probe, lock_pub, admin_pub) in ranked_probes {
        let cand = SybilServerCandidate {
            write_url: probe.write_url.clone(),
            lock_pub,
            admin_pub: Some(admin_pub),
            asn: None,
        };
        let mut trial = sybil_existing.clone();
        trial.push(cand.clone());
        if sybil_fold_committee(&trial).len() == trial.len() {
            used_lock_pubs.insert(lock_pub);
            sybil_existing.push(cand);
            available_extra.push(ServerEntry {
                write_url: probe.write_url.clone(),
                read_url: probe.read_url.clone(),
                admin_pub,
                lock_pub,
                nominated_by: NominatedBy::Creator,
            });
        }
    }

    let add_count = match (manifest.servers.len(), available_extra.len()) {
        (1, n) if n >= 2 => 2,
        (1, 1) if circle_urls.contains(&normalize_sync_url(&available_extra[0].write_url)) => 1,
        (2, n) if n >= 1 => 1,
        _ => return None,
    };

    let mut new_servers = manifest.servers.clone();
    new_servers.extend(available_extra.into_iter().take(add_count));

    l2_rotation::build_creator_rotation_proposal(
        anchor_id,
        manifest,
        2,
        new_servers,
        manifest.successors.clone(),
        now_secs,
        creator_mig_sk,
    )
    .ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VetoDecision {
    pub proposal_hash: [u8; 32],
    pub reason: &'static str,
}

/// Pure decision helper deduplicating polled migration proposals and returning those requiring an
/// automatic guarantor veto for `guarantor_index`.
pub(crate) fn evaluate_polled_proposals_for_veto(
    manifest: &AnchorManifest,
    polled_proposals: &[MigrationProposal],
    guarantor_index: u8,
) -> Vec<VetoDecision> {
    let mut seen_hashes: HashSet<[u8; 32]> = HashSet::new();
    let mut decisions = Vec::new();
    for prop in polled_proposals {
        let Ok(prop_hash) = prop.core_digest() else {
            continue;
        };
        if !seen_hashes.insert(prop_hash) {
            continue;
        }
        if let Some(reason) =
            l2_rotation::evaluate_guarantor_veto_reason(manifest, prop, guarantor_index)
        {
            decisions.push(VetoDecision {
                proposal_hash: prop_hash,
                reason,
            });
        }
    }
    decisions
}

/// Pure state transition updating lifecycle veto counters after dispatching a veto.
pub(crate) fn record_guarantor_veto_sent(lc: &mut StoredL2LifecycleState, reason: &str) {
    lc.auto_vetoes_sent = lc.auto_vetoes_sent.saturating_add(1);
    lc.last_veto_reason = Some(reason.to_string());
}

type GuarantorRoleEntry = (String, AnchorManifest, u8, Vec<u8>);
type GuarantorVetoScanData = (
    Vec<GuarantorRoleEntry>,
    BTreeMap<String, FrontendLockServerProbeResult>,
);

async fn check_guarantor_vetoes(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    now_secs: u64,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let (roles_and_manifests, probe_cache): GuarantorVetoScanData = state.with_session_required(
        session,
        "Wallet session changed during guarantor veto scan",
        |service| {
            let lc = load_lifecycle_state(service, password)?;
            let probe_cache = state
                .get_or_load_internal_lifecycle(service, password)
                .map(|ilc| ilc.l2_probe_cache)
                .unwrap_or_default();
            let roles = service
                .with_wallet(|w| {
                    lc.guarantor_roles
                        .values()
                        .filter_map(|r| {
                            let m = w.profile.l2_anchor_manifests.get(&r.anchor_id)?.clone();
                            let nonce = hex::decode(&r.context_nonce_hex).ok()?;
                            Some((r.anchor_id.clone(), m, r.guarantor_index, nonce))
                        })
                        .collect::<Vec<_>>()
                })
                .map_err(FrontendError::from)?;
            Ok((roles, probe_cache))
        },
    )?;

    for (anchor_hex, manifest, g_idx, ctx_nonce) in roles_and_manifests {
        let Ok(anchor_id) = decode_hex_fixed::<32>(&anchor_hex) else {
            continue;
        };

        let mut polled_proposals: Vec<MigrationProposal> = Vec::new();
        for srv in &manifest.servers {
            let (write_url, _) = resolve_server_locator(srv, &probe_cache);
            if let Ok(Some(status)) =
                sync_client::get_lock_freeze_status(client, &write_url, &anchor_hex).await
            {
                if let Some(prop) = status.voted_proposal {
                    polled_proposals.push(prop);
                }
            }
        }

        for VetoDecision {
            proposal_hash: prop_hash,
            reason,
        } in evaluate_polled_proposals_for_veto(&manifest, &polled_proposals, g_idx)
        {
            let veto_res = state.with_session_required(
                session,
                "Wallet session changed while signing guarantor veto",
                |service| {
                    service
                        .with_authorized_identity(
                            human_money_core::AccessMode::DailyPay,
                            password,
                            |_, id| {
                                let g_sk =
                                    l2_rotation::derive_guarantor_mig_signing_key(id, &ctx_nonce)?;
                                l2_rotation::sign_migration_veto(
                                    &g_sk,
                                    anchor_id,
                                    manifest.revision,
                                    prop_hash,
                                    now_secs.max(1),
                                )
                            },
                        )
                        .map_err(FrontendError::from)
                },
            )?;

            let Ok(veto) = veto_res else {
                continue;
            };
            let body = serde_json::json!({
                "action": "veto",
                "veto": veto,
            });
            let mut sent_any = false;
            for srv in &manifest.servers {
                let (write_url, _) = resolve_server_locator(srv, &probe_cache);
                if sync_client::post_lock_freeze_action(client, &write_url, &body)
                    .await
                    .is_ok()
                {
                    sent_any = true;
                }
            }
            if sent_any {
                state.with_session_required(
                    session,
                    "Wallet session changed after dispatching guarantor veto",
                    |service| {
                        let mut lc = load_lifecycle_state(service, password)?;
                        record_guarantor_veto_sent(&mut lc, reason);
                        save_lifecycle_state(service, &lc, password)
                    },
                )?;
            }
        }
    }

    Ok(())
}

type ImportedStatePair = ([u8; 32], Vec<ImportAck>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProposedPhaseDecision {
    WaitTimelock(StoredRotationPhase),
    AwaitSnapshots(StoredRotationPhase),
    ReadyToImport {
        proposal: MigrationProposal,
        verified_exports: Vec<SnapshotExport>,
        expected_state_hash: [u8; 32],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommitPhaseDecision {
    AwaitCommitSigs(StoredRotationPhase),
    Finalized(AnchorManifest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RotationStepOutcome {
    InProgress(Box<StoredRotationPhase>),
    Completed { new_revision: u64 },
    Aborted { reason: String },
}

pub(crate) fn is_proposal_timelock_elapsed(effective_at_secs: u64, now_secs: u64) -> bool {
    now_secs >= effective_at_secs
}

/// Computes panic-free quorum order statistic for `last_creator_activity_secs` from healthy,
/// non-revoked servers, clamping every reported timestamp to `<= now_secs`.
pub(crate) fn compute_quorum_creator_activity_secs(
    baseline: u64,
    reported_activities: &[u64],
    threshold: u8,
    now_secs: u64,
) -> u64 {
    let mut activities: Vec<u64> = reported_activities
        .iter()
        .map(|&a| a.min(now_secs))
        .collect();
    let q = usize::from(threshold).max(1);
    if activities.len() >= q {
        activities.sort_unstable();
        activities[q - 1].max(baseline)
    } else {
        activities
            .into_iter()
            .max()
            .unwrap_or(baseline)
            .max(baseline)
    }
}

/// Strictly validates an in-place delegation refresh probe before persisting into `l2_probe_cache`.
pub(crate) fn validate_in_place_delegation_refresh(
    srv: &ServerEntry,
    del: &sync_client::SyncLockDelegationDto,
    updated_probe: &FrontendLockServerProbeResult,
    offenders: &ServerOffenderList,
) -> bool {
    if is_server_key_revoked(offenders, &srv.lock_pub, Some(&srv.admin_pub)) {
        return false;
    }
    let del_write_raw = del.write_url.trim();
    let del_read_raw = del.read_url.trim();
    if sync_client::validate_sync_url(del_write_raw).is_err()
        || sync_client::validate_sync_read_url(del_read_raw).is_err()
    {
        return false;
    }
    if !updated_probe.passed || !updated_probe.delegation_valid {
        return false;
    }
    let lock_hex = hex::encode(srv.lock_pub);
    let admin_hex = hex::encode(srv.admin_pub);
    if !updated_probe
        .lock_pub
        .trim()
        .eq_ignore_ascii_case(&lock_hex)
        || !updated_probe
            .admin_pub
            .trim()
            .eq_ignore_ascii_case(&admin_hex)
    {
        return false;
    }
    let del_w = normalize_sync_url(del_write_raw);
    let del_r = normalize_sync_url(del_read_raw);
    if del_w.is_empty()
        || del_r.is_empty()
        || normalize_sync_url(&updated_probe.write_url) != del_w
        || normalize_sync_url(&updated_probe.read_url) != del_r
    {
        return false;
    }
    updated_probe.delegation.as_ref().is_some_and(|ud| {
        normalize_sync_url(&ud.write_url) == del_w
            && normalize_sync_url(&ud.read_url) == del_r
            && crate::services::common::is_valid_delegation_for_entry(srv, ud)
    })
}

/// Pure Phase 1 -> Phase 2 state transition: enters `Proposed` only if `>= threshold` servers
/// acknowledged the proposal, computing the quorum order statistic over clamped `effective_at`s.
pub(crate) fn compute_proposal_phase(
    manifest: &AnchorManifest,
    proposal: MigrationProposal,
    accepted_effective_ats: &[Option<u64>],
    threshold: u8,
    now_secs: u64,
) -> Option<StoredRotationPhase> {
    let q = usize::from(threshold).max(1);
    if accepted_effective_ats.len() < q {
        return None;
    }
    let default_timelock = if matches!(proposal.initiator, humoco_proto::ProposalInitiator::Creator)
        && manifest.authority.guarantor_mig_pubs.is_empty()
        && human_money_core::humoco_proto::is_pure_committee_growth(manifest, &proposal)
    {
        0
    } else {
        humoco_proto::PROPOSAL_TIMELOCK_SECS
    };
    let max_eff = now_secs.saturating_add(default_timelock);
    let mut effs: Vec<u64> = accepted_effective_ats
        .iter()
        .flatten()
        .map(|&e| e.min(max_eff))
        .collect();
    effs.sort_unstable();
    let effective_at_secs = if effs.len() >= q {
        effs[q - 1]
    } else {
        max_eff
    };
    Some(StoredRotationPhase::Proposed {
        proposal,
        effective_at_secs,
    })
}

fn decode_and_verify_snapshot_export(
    exp: &SnapshotExport,
    anchor_id: &[u8; 32],
    manifest: &AnchorManifest,
    committee: &humoco_proto::LockCommittee,
    prop_hash: &[u8; 32],
) -> Option<(Vec<LockEntry>, Vec<Vec<LockEntry>>)> {
    let snap = &exp.snapshot;
    if !committee.contains(&snap.lock_pub)
        || snap.anchor_id != *anchor_id
        || snap.from_rev != manifest.revision
        || snap.retain_until_quarter != manifest.retain_until_quarter
        || !snap.permanent
        || snap.proposal_hash != Some(*prop_hash)
    {
        return None;
    }
    let mut decoded_entries = Vec::with_capacity(exp.entries.len());
    for hex_s in &exp.entries {
        decoded_entries.push(LockEntry::from_wire_hex(hex_s).ok()?);
    }
    let mut decoded_conflicts = Vec::with_capacity(exp.conflicts.len());
    for group in &exp.conflicts {
        let mut decoded_group = Vec::with_capacity(group.len());
        for hex_s in group {
            decoded_group.push(LockEntry::from_wire_hex(hex_s).ok()?);
        }
        decoded_conflicts.push(decoded_group);
    }
    snap.verify_with_entries(&decoded_entries, &decoded_conflicts)
        .ok()?;
    Some((decoded_entries, decoded_conflicts))
}

fn is_decoded_export_causally_valid(
    anchor_id: &[u8; 32],
    creator_mig_pub: &[u8; 32],
    entries: &[LockEntry],
    conflicts: &[Vec<LockEntry>],
    pow_base_bits: u32,
) -> bool {
    let mut by_slot: BTreeMap<[u8; 32], BTreeMap<[u8; 32], LockEntry>> = BTreeMap::new();
    for entry in entries
        .iter()
        .chain(conflicts.iter().flat_map(|g| g.iter()))
    {
        let Ok(eh) = entry.entry_hash() else {
            return false;
        };
        by_slot
            .entry(entry.slot)
            .or_default()
            .insert(eh, entry.clone());
    }
    let mut ordered: Vec<(u32, [u8; 32], Vec<LockEntry>)> = by_slot
        .into_iter()
        .map(|(slot, map)| {
            let mut list: Vec<LockEntry> = map.into_values().collect();
            list.sort_by_key(|e| e.depth);
            let min_depth = list.first().map(|e| e.depth).unwrap_or(0);
            (min_depth, slot, list)
        })
        .collect();
    ordered.sort_by_key(|(d, s, _)| (*d, *s));

    let mut verified_slots: BTreeMap<[u8; 32], Vec<LockEntry>> = BTreeMap::new();
    let mut output_claims: BTreeMap<([u8; 32], u8), [u8; 32]> = BTreeMap::new();
    for (_d, slot, list) in ordered {
        let mut valid_list = Vec::with_capacity(list.len());
        for entry in list {
            if entry.depth == 0 {
                if entry
                    .verify_entry(anchor_id, creator_mig_pub, pow_base_bits, None)
                    .is_err()
                {
                    return false;
                }
            } else {
                let Some(parents) = verified_slots.get(&entry.parent_slot) else {
                    return false;
                };
                let ok_parent = parents.iter().any(|p| {
                    entry
                        .verify_entry(anchor_id, creator_mig_pub, pow_base_bits, Some(p))
                        .is_ok()
                });
                if !ok_parent {
                    return false;
                }
                if let Some(existing) = output_claims.get(&(entry.parent_slot, entry.output_branch))
                {
                    if *existing != entry.slot {
                        return false;
                    }
                } else {
                    output_claims.insert((entry.parent_slot, entry.output_branch), entry.slot);
                }
            }
            valid_list.push(entry);
        }
        verified_slots.insert(slot, valid_list);
    }
    true
}

/// Pure `Proposed` phase evaluator: checks timelock expiry, strictly validates `SnapshotExport`
/// committee membership, metadata, and `verify_with_entries` before deduplicating and counting
/// `>= manifest.threshold`, and computes the deterministic import union (filtering any Byzantine
/// export whose entries fail causal/PoW verification when `> threshold` exports are present).
#[allow(clippy::too_many_arguments)]
pub(crate) fn evaluate_proposed_phase_transition(
    anchor_id: [u8; 32],
    manifest: &AnchorManifest,
    proposal: MigrationProposal,
    effective_at_secs: u64,
    candidate_exports: Vec<SnapshotExport>,
    qcs: &[QuorumCertificate],
    pow_base_bits: u32,
    now_secs: u64,
) -> Result<ProposedPhaseDecision, FrontendError> {
    if !is_proposal_timelock_elapsed(effective_at_secs, now_secs) {
        return Ok(ProposedPhaseDecision::WaitTimelock(
            StoredRotationPhase::Proposed {
                proposal,
                effective_at_secs,
            },
        ));
    }

    let prop_hash = proposal
        .core_digest()
        .map_err(|e| lifecycle_err(format!("Invalid migration proposal: {}", e)))?;
    let committee = manifest
        .committee()
        .map_err(|e| lifecycle_err(format!("Invalid manifest committee: {}", e)))?;

    let mut verified_with_decoded: Vec<(SnapshotExport, Vec<LockEntry>, Vec<Vec<LockEntry>>)> =
        Vec::new();
    let mut seen_pubs: HashSet<LockPubKey> = HashSet::new();
    for exp in candidate_exports {
        if seen_pubs.contains(&exp.snapshot.lock_pub) {
            continue;
        }
        if let Some((decoded_entries, decoded_conflicts)) =
            decode_and_verify_snapshot_export(&exp, &anchor_id, manifest, &committee, &prop_hash)
        {
            seen_pubs.insert(exp.snapshot.lock_pub);
            verified_with_decoded.push((exp, decoded_entries, decoded_conflicts));
        }
    }

    let threshold = usize::from(manifest.threshold);
    if verified_with_decoded.len() < threshold {
        return Ok(ProposedPhaseDecision::AwaitSnapshots(
            StoredRotationPhase::Proposed {
                proposal,
                effective_at_secs,
            },
        ));
    }

    let mut verified_exports: Vec<SnapshotExport> = verified_with_decoded
        .iter()
        .map(|(exp, _, _)| exp.clone())
        .collect();

    let resolved = match resolve_import_union(
        &anchor_id,
        manifest,
        &proposal,
        &verified_exports,
        qcs,
        pow_base_bits,
    ) {
        Ok(res) => res,
        Err(first_err) if verified_with_decoded.len() > threshold => {
            let filtered: Vec<SnapshotExport> = verified_with_decoded
                .into_iter()
                .filter(|(_, entries, conflicts)| {
                    is_decoded_export_causally_valid(
                        &anchor_id,
                        &manifest.authority.creator_mig_pub,
                        entries,
                        conflicts,
                        pow_base_bits,
                    )
                })
                .map(|(exp, _, _)| exp)
                .collect();
            if filtered.len() < threshold {
                return Err(lifecycle_err(format!(
                    "resolve_import_union failed: {}",
                    first_err
                )));
            }
            verified_exports = filtered;
            resolve_import_union(
                &anchor_id,
                manifest,
                &proposal,
                &verified_exports,
                qcs,
                pow_base_bits,
            )
            .map_err(|e| lifecycle_err(format!("resolve_import_union failed: {}", e)))?
        }
        Err(e) => {
            return Err(lifecycle_err(format!("resolve_import_union failed: {}", e)));
        }
    };

    Ok(ProposedPhaseDecision::ReadyToImport {
        proposal,
        verified_exports,
        expected_state_hash: resolved.imported_state_hash,
    })
}

pub(crate) fn verify_single_import_ack(
    ack: &ImportAck,
    expected_lock_pub: &LockPubKey,
    anchor_id: &[u8; 32],
    proposal: &MigrationProposal,
    expected_state_hash: &[u8; 32],
) -> Result<(), FrontendError> {
    let prop_hash = proposal
        .core_digest()
        .map_err(|e| lifecycle_err(format!("Invalid migration proposal: {}", e)))?;
    if ack.lock_pub != *expected_lock_pub {
        return Err(lifecycle_err("Target server ImportAck lock_pub mismatch"));
    }
    if ack.anchor_id != *anchor_id
        || ack.proposal_hash != prop_hash
        || ack.to_rev != proposal.to_rev
        || ack.retain_until_quarter != proposal.retain_until_quarter
    {
        return Err(lifecycle_err("Target server ImportAck metadata mismatch"));
    }
    if ack.imported_state_hash != *expected_state_hash {
        return Err(lifecycle_err("Target server ImportAck state hash mismatch"));
    }
    if !ack.verify() {
        return Err(lifecycle_err(
            "Target server ImportAck failed Ed25519 signature verification",
        ));
    }
    Ok(())
}

pub(crate) fn verify_collected_import_acks(
    acks: &[ImportAck],
    anchor_id: &[u8; 32],
    proposal: &MigrationProposal,
    expected_state_hash: &[u8; 32],
) -> Result<(), FrontendError> {
    let expected_targets: HashSet<LockPubKey> =
        proposal.new_servers.iter().map(|s| s.lock_pub).collect();
    if acks.len() != expected_targets.len() {
        return Err(lifecycle_err(
            "ImportAck set does not match proposal.new_servers count",
        ));
    }
    let mut seen: HashSet<LockPubKey> = HashSet::with_capacity(acks.len());
    for ack in acks {
        if !expected_targets.contains(&ack.lock_pub) {
            return Err(lifecycle_err(
                "ImportAck signer is not in proposal.new_servers",
            ));
        }
        verify_single_import_ack(ack, &ack.lock_pub, anchor_id, proposal, expected_state_hash)?;
        if !seen.insert(ack.lock_pub) {
            return Err(lifecycle_err("Duplicate ImportAck from same target server"));
        }
    }
    Ok(())
}

/// Pure `Imported` -> `Finalized` evaluator: validates old-committee membership, `committed_at > 0`,
/// and strict Ed25519 signature over `MigrationCert::commit_digest` before deduplicating and
/// counting `>= manifest.threshold`, then assembles the next manifest.
pub(crate) fn evaluate_imported_commit_transition(
    manifest: &AnchorManifest,
    anchor_id: &[u8; 32],
    proposal: MigrationProposal,
    imported_state_hash: [u8; 32],
    import_acks: Vec<ImportAck>,
    commit_sigs: Vec<CommitteeSignature>,
) -> Result<CommitPhaseDecision, FrontendError> {
    let committee = manifest
        .committee()
        .map_err(|e| lifecycle_err(format!("Invalid manifest committee: {}", e)))?;
    let prop_hash = proposal
        .core_digest()
        .map_err(|e| lifecycle_err(format!("Invalid migration proposal: {}", e)))?;
    let commit_digest =
        MigrationCert::commit_digest(anchor_id, &prop_hash, proposal.to_rev, &imported_state_hash);

    let mut unique_sigs: Vec<CommitteeSignature> = Vec::new();
    let mut signed_pubs: HashSet<LockPubKey> = HashSet::new();
    for cs in commit_sigs {
        if cs.committed_at == 0
            || !committee.contains(&cs.lock_pub)
            || signed_pubs.contains(&cs.lock_pub)
            || !verify_ed25519_strict(&cs.lock_pub, &commit_digest, &cs.sig)
        {
            continue;
        }
        signed_pubs.insert(cs.lock_pub);
        unique_sigs.push(cs);
    }

    if unique_sigs.len() < usize::from(manifest.threshold) {
        return Ok(CommitPhaseDecision::AwaitCommitSigs(
            StoredRotationPhase::Imported {
                proposal,
                imported_state_hash_hex: hex::encode(imported_state_hash),
                import_acks,
            },
        ));
    }

    let (next_manifest, _reduced) = l2_rotation::assemble_migrated_manifest(
        manifest,
        anchor_id,
        proposal,
        imported_state_hash,
        import_acks,
        unique_sigs,
    )
    .map_err(FrontendError::from)?;

    Ok(CommitPhaseDecision::Finalized(next_manifest))
}

/// Pure state transition updating `StoredAnchorHealth` from the result of a rotation step.
///
/// Only `RotationStepOutcome::Completed` clears `unhealthy_since_by_lock_pub` and
/// `revoked_lock_pubs` and advances `last_rotated_rev`. `Aborted` clears `active_rotation`,
/// records `last_error`, and preserves health/revocation state.
pub(crate) fn apply_rotation_step_outcome(
    health: &mut StoredAnchorHealth,
    outcome: Result<&RotationStepOutcome, &str>,
    now_secs: u64,
) {
    match outcome {
        Ok(RotationStepOutcome::InProgress(next_phase)) => {
            health.last_checked_secs = now_secs;
            health.active_rotation = Some((**next_phase).clone());
            health.last_error = None;
        }
        Ok(RotationStepOutcome::Completed { new_revision }) => {
            health.last_checked_secs = now_secs;
            health.active_rotation = None;
            health.last_error = None;
            health.unhealthy_since_by_lock_pub.clear();
            health.revoked_lock_pubs.clear();
            health.last_rotated_rev = health.last_rotated_rev.max(*new_revision);
        }
        Ok(RotationStepOutcome::Aborted { reason }) => {
            health.last_checked_secs = now_secs;
            health.active_rotation = None;
            health.last_error = Some(reason.clone());
        }
        Err(err_msg) => {
            health.last_error = Some(err_msg.to_string());
        }
    }
}

#[derive(Clone, Copy)]
struct RotationEnv<'a> {
    state: &'a AppState,
    session: &'a crate::session::SessionToken,
    security_policy: &'a SecurityPolicy,
    lifecycle: &'a InternalLifecycleState,
    client: &'a reqwest::Client,
    now_secs: u64,
    password: Option<&'a str>,
}

fn phase_proposal_revs(phase: &StoredRotationPhase) -> (u64, u64) {
    match phase {
        StoredRotationPhase::Proposed { proposal, .. }
        | StoredRotationPhase::Imported { proposal, .. } => (proposal.from_rev, proposal.to_rev),
    }
}

async fn drive_active_rotation(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    manifest: AnchorManifest,
    phase: StoredRotationPhase,
) -> Result<RotationStepOutcome, FrontendError> {
    let RotationEnv {
        state,
        session,
        security_policy,
        lifecycle,
        client,
        now_secs,
        password,
    } = env;
    let anchor_hex = hex::encode(anchor_id);
    let (proposal_from_rev, proposal_to_rev) = phase_proposal_revs(&phase);

    if manifest.revision >= proposal_to_rev {
        return Ok(RotationStepOutcome::Completed {
            new_revision: manifest.revision,
        });
    }
    if manifest.revision != proposal_from_rev {
        return Ok(RotationStepOutcome::Aborted {
            reason: format!(
                "Stored rotation proposal from_rev {} mismatches active manifest revision {}",
                proposal_from_rev, manifest.revision
            ),
        });
    }

    let (creator_did, guarantor_dids): (Option<String>, Vec<String>) = state
        .with_session_required(
            session,
            "Wallet session changed before checking anchor metadata",
            |service| {
                service
                    .with_wallet(|w| {
                        let mut c_did = None;
                        let mut g_set = BTreeSet::new();
                        for inst in w.voucher_store.vouchers.values() {
                            if inst.voucher.layer2_anchor == Some(anchor_id) {
                                if c_did.is_none() {
                                    c_did = inst.voucher.creator_profile.id.clone();
                                }
                                for sig in &inst.voucher.signatures {
                                    if sig.role.eq_ignore_ascii_case("guarantor")
                                        && !sig.signer_id.trim().is_empty()
                                    {
                                        g_set.insert(sig.signer_id.clone());
                                    }
                                }
                            }
                        }
                        (c_did, g_set.into_iter().collect())
                    })
                    .map_err(FrontendError::from)
            },
        )?;

    if let Ok(Some(discovered)) = discover_anchor_manifest(
        state,
        session,
        client,
        &anchor_id,
        creator_did.as_deref(),
        &guarantor_dids,
        password,
    )
    .await
    {
        if discovered.revision >= proposal_to_rev {
            return Ok(RotationStepOutcome::Completed {
                new_revision: discovered.revision,
            });
        }
        if discovered.revision != proposal_from_rev {
            return Ok(RotationStepOutcome::Aborted {
                reason: format!(
                    "Discovered manifest revision {} superseded proposal from_rev {}",
                    discovered.revision, proposal_from_rev
                ),
            });
        }
    }

    let (proposal, maybe_imported): (MigrationProposal, Option<ImportedStatePair>) = match phase {
        StoredRotationPhase::Proposed {
            proposal,
            effective_at_secs,
        } => {
            if !is_proposal_timelock_elapsed(effective_at_secs, now_secs) {
                return Ok(RotationStepOutcome::InProgress(Box::new(
                    StoredRotationPhase::Proposed {
                        proposal,
                        effective_at_secs,
                    },
                )));
            }

            let prop_hash = proposal
                .core_digest()
                .map_err(|e| lifecycle_err(format!("Invalid migration proposal: {}", e)))?;
            let abort_threshold = manifest
                .servers
                .len()
                .saturating_sub(usize::from(manifest.threshold))
                .saturating_add(1);

            // Ensure permanent freeze & collect >= Q_old FreezeSnapshots
            let mut candidate_exports: Vec<SnapshotExport> = Vec::new();
            let mut aborted_votes: HashSet<LockPubKey> = HashSet::new();
            for srv in &manifest.servers {
                let (write_url, _) = resolve_server_locator(srv, &lifecycle.l2_probe_cache);
                let mut got_export = false;
                if let Ok(Some(st)) =
                    sync_client::get_lock_freeze_status(client, &write_url, &anchor_hex).await
                {
                    if let Some(m) = st.active_manifest {
                        if m.revision > manifest.revision && m.verify_anchor_id(&anchor_id).is_ok()
                        {
                            let new_rev = m.revision;
                            state.with_session_required(
                                session,
                                "Wallet session changed before committing discovered active manifest",
                                |service| {
                                    service
                                        .update_l2_anchor_manifest(m.clone(), password)
                                        .map_err(FrontendError::from)?;
                                    let mut lc = load_lifecycle_state(service, password)?;
                                    let entry = lc
                                        .anchors
                                        .entry(anchor_hex.clone())
                                        .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.clone()));
                                    apply_rotation_step_outcome(
                                        entry,
                                        Ok(&RotationStepOutcome::Completed {
                                            new_revision: new_rev,
                                        }),
                                        now_secs,
                                    );
                                    save_lifecycle_state(service, &lc, password)
                                },
                            )?;
                            return Ok(RotationStepOutcome::Completed {
                                new_revision: new_rev,
                            });
                        }
                    }

                    let mut export_matches = false;
                    if let Some(exp) = st.snapshot_export {
                        if exp.snapshot.lock_pub == srv.lock_pub {
                            export_matches = exp.snapshot.permanent
                                && exp.snapshot.proposal_hash == Some(prop_hash);
                            candidate_exports.push(exp);
                            got_export = true;
                        }
                    }

                    let voted_matches = st
                        .voted_proposal
                        .as_ref()
                        .and_then(|p| p.core_digest().ok())
                        .is_some_and(|h| h == prop_hash);
                    let voted_different = st
                        .voted_proposal
                        .as_ref()
                        .and_then(|p| p.core_digest().ok())
                        .is_some_and(|h| h != prop_hash);
                    let reported_active_without_proposal = st
                        .bucket_freeze_state
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case("active"))
                        && !voted_matches
                        && !export_matches;
                    if voted_different || reported_active_without_proposal {
                        aborted_votes.insert(srv.lock_pub);
                    }
                }
                if !got_export && !aborted_votes.contains(&srv.lock_pub) {
                    let fetch_body = serde_json::json!({
                        "action": "fetch_snapshot",
                        "anchor_id": anchor_hex,
                    });
                    if let Ok(val) =
                        sync_client::post_lock_freeze_action(client, &write_url, &fetch_body).await
                    {
                        if let Some(exp_val) = val.get("snapshot_export").cloned() {
                            if let Ok(exp) = serde_json::from_value::<SnapshotExport>(exp_val) {
                                if exp.snapshot.lock_pub == srv.lock_pub {
                                    candidate_exports.push(exp);
                                }
                            }
                        }
                    }
                }
            }

            if aborted_votes.len() >= abort_threshold {
                return Ok(RotationStepOutcome::Aborted {
                    reason: "Rotation proposal was vetoed or cleared on committee".to_string(),
                });
            }

            let qcs: Vec<QuorumCertificate> = state.with_session_required(
                session,
                "Wallet session changed while driving L2 rotation",
                |service| {
                    service
                        .with_wallet(|w| {
                            w.profile
                                .l2_quorum_certificates
                                .iter()
                                .filter(|(t_id, _)| {
                                    w.voucher_store.vouchers.values().any(|inst| {
                                        inst.voucher.layer2_anchor == Some(anchor_id)
                                            && inst
                                                .voucher
                                                .transactions
                                                .iter()
                                                .any(|t| &t.t_id == *t_id)
                                    })
                                })
                                .flat_map(|(_, qcs)| qcs.iter().cloned())
                                .collect::<Vec<_>>()
                        })
                        .map_err(FrontendError::from)
                },
            )?;

            let (proposal, exports, expected_state_hash) = match evaluate_proposed_phase_transition(
                anchor_id,
                &manifest,
                proposal,
                effective_at_secs,
                candidate_exports,
                &qcs,
                DEFAULT_LOCK_POW_BASE_BITS,
                now_secs,
            )? {
                ProposedPhaseDecision::WaitTimelock(p)
                | ProposedPhaseDecision::AwaitSnapshots(p) => {
                    return Ok(RotationStepOutcome::InProgress(Box::new(p)))
                }
                ProposedPhaseDecision::ReadyToImport {
                    proposal,
                    verified_exports,
                    expected_state_hash,
                } => (proposal, verified_exports, expected_state_hash),
            };

            let mut import_acks: Vec<ImportAck> = Vec::with_capacity(proposal.new_servers.len());
            for new_srv in &proposal.new_servers {
                let (write_url, _) = resolve_server_locator(new_srv, &lifecycle.l2_probe_cache);
                let ack = sync_client::post_lock_import(
                    client, &write_url, &anchor_id, &manifest, &proposal, &exports, &qcs,
                )
                .await
                .map_err(|e| lifecycle_err(e.to_string()))?;
                verify_single_import_ack(
                    &ack,
                    &new_srv.lock_pub,
                    &anchor_id,
                    &proposal,
                    &expected_state_hash,
                )?;
                import_acks.push(ack);
            }
            verify_collected_import_acks(
                &import_acks,
                &anchor_id,
                &proposal,
                &expected_state_hash,
            )?;

            (proposal, Some((expected_state_hash, import_acks)))
        }
        StoredRotationPhase::Imported {
            proposal,
            imported_state_hash_hex,
            import_acks,
        } => {
            let imported_state_hash =
                decode_hex_fixed::<32>(&imported_state_hash_hex).map_err(lifecycle_err)?;
            verify_collected_import_acks(
                &import_acks,
                &anchor_id,
                &proposal,
                &imported_state_hash,
            )?;
            (proposal, Some((imported_state_hash, import_acks)))
        }
    };

    let Some((imported_state_hash, import_acks)) = maybe_imported else {
        unreachable!("handled in Proposed guard");
    };

    // Phase 3: Collect >= Q_old verified commit signatures
    let prop_hash = proposal
        .core_digest()
        .map_err(|e| lifecycle_err(format!("Invalid migration proposal: {}", e)))?;
    let commit_digest = MigrationCert::commit_digest(
        &anchor_id,
        &prop_hash,
        proposal.to_rev,
        &imported_state_hash,
    );
    let mut commit_sigs: Vec<CommitteeSignature> = Vec::new();
    for old_srv in &manifest.servers {
        let (write_url, _) = resolve_server_locator(old_srv, &lifecycle.l2_probe_cache);
        if let Ok(cs) = sync_client::post_lock_commit_sign(
            client,
            &write_url,
            &anchor_id,
            &proposal,
            &imported_state_hash,
            &import_acks,
        )
        .await
        {
            if cs.lock_pub == old_srv.lock_pub
                && cs.committed_at > 0
                && verify_ed25519_strict(&cs.lock_pub, &commit_digest, &cs.sig)
                && !commit_sigs
                    .iter()
                    .any(|existing| existing.lock_pub == cs.lock_pub)
            {
                commit_sigs.push(cs);
            }
        }
        if commit_sigs.len() >= usize::from(manifest.threshold) {
            break;
        }
    }

    let next_manifest = match evaluate_imported_commit_transition(
        &manifest,
        &anchor_id,
        proposal,
        imported_state_hash,
        import_acks,
        commit_sigs,
    )? {
        CommitPhaseDecision::AwaitCommitSigs(phase) => {
            return Ok(RotationStepOutcome::InProgress(Box::new(phase)))
        }
        CommitPhaseDecision::Finalized(m) => m,
    };

    let mut final_urls = HashSet::new();
    for srv in manifest.servers.iter().chain(next_manifest.servers.iter()) {
        let (write_url, _) = resolve_server_locator(srv, &lifecycle.l2_probe_cache);
        final_urls.insert(write_url);
    }
    for url in final_urls {
        let _ =
            sync_client::post_lock_commit_finalize(client, &url, &anchor_id, &next_manifest).await;
    }

    let new_rev = next_manifest.revision;
    state.with_session_required(
        session,
        "Wallet session changed before committing rotated anchor manifest",
        |service| {
            service
                .update_l2_anchor_manifest(next_manifest.clone(), password)
                .map_err(FrontendError::from)?;
            let mut lc = load_lifecycle_state(service, password)?;
            let entry = lc
                .anchors
                .entry(anchor_hex.clone())
                .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.clone()));
            apply_rotation_step_outcome(
                entry,
                Ok(&RotationStepOutcome::Completed {
                    new_revision: new_rev,
                }),
                now_secs,
            );
            save_lifecycle_state(service, &lc, password)
        },
    )?;

    mirror_manifest_to_own_syncs(client, security_policy, &anchor_id, &next_manifest).await;
    Ok(RotationStepOutcome::Completed {
        new_revision: new_rev,
    })
}

async fn initiate_rotation_proposal(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    anchor_hex: &str,
    manifest: &AnchorManifest,
    proposal: MigrationProposal,
) -> Result<(), FrontendError> {
    let RotationEnv {
        state,
        session,
        lifecycle,
        client,
        now_secs,
        password,
        ..
    } = env;

    let prop_body = serde_json::json!({
        "action": "propose",
        "proposal": proposal,
        "current_manifest": manifest,
    });
    let mut accepted_effective_ats: Vec<Option<u64>> = Vec::new();
    for srv in &manifest.servers {
        let (write_url, _) = resolve_server_locator(srv, &lifecycle.l2_probe_cache);
        if let Ok(resp) = sync_client::post_lock_freeze_action(client, &write_url, &prop_body).await
        {
            accepted_effective_ats.push(resp.get("effective_at").and_then(|v| v.as_u64()));
        }
    }

    if let Some(phase) = compute_proposal_phase(
        manifest,
        proposal,
        &accepted_effective_ats,
        manifest.threshold,
        now_secs,
    ) {
        let phase_effective_at = match &phase {
            StoredRotationPhase::Proposed {
                effective_at_secs, ..
            } => *effective_at_secs,
            StoredRotationPhase::Imported { .. } => now_secs,
        };
        state.with_session_required(
            session,
            "Wallet session changed after proposing L2 rotation",
            |service| {
                let mut lc = load_lifecycle_state(service, password)?;
                let health = lc
                    .anchors
                    .entry(anchor_hex.to_string())
                    .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.to_string()));
                health.active_rotation = Some(phase.clone());
                health.last_error = None;
                save_lifecycle_state(service, &lc, password)
            },
        )?;

        if is_proposal_timelock_elapsed(phase_effective_at, now_secs) {
            let step_result = drive_active_rotation(env, anchor_id, manifest.clone(), phase).await;
            state.with_session_required(
                session,
                "Wallet session changed after driving L2 rotation step",
                |service| {
                    let mut lc = load_lifecycle_state(service, password)?;
                    let health = lc
                        .anchors
                        .entry(anchor_hex.to_string())
                        .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.to_string()));
                    match &step_result {
                        Ok(outcome) => apply_rotation_step_outcome(health, Ok(outcome), now_secs),
                        Err(err) => {
                            apply_rotation_step_outcome(health, Err(err.message.as_str()), now_secs)
                        }
                    }
                    save_lifecycle_state(service, &lc, password)
                },
            )?;
        }
    }

    Ok(())
}

/// Executes one health check, guarantor-veto scan, and crash-resumable rotation step using `now_secs`
/// without building a frontend status DTO (used by background ticks).
pub(crate) async fn run_lifecycle_tick_core(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    now_secs: u64,
    force: bool,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let (security_policy, mut internal_lc) = state.with_session_required(
        session,
        "Wallet session changed before starting L2 lifecycle tick",
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

    // 1. Online Guarantor Automatic Veto check (§5.4)
    check_guarantor_vetoes(state, session, client, now_secs, password).await?;

    // 2. Collect tracked anchors
    enum CollectOutcome {
        Early,
        Ready(
            Vec<(String, AnchorManifest, Option<StoredGuarantorRole>, bool)>,
            ServerOffenderList,
            BTreeMap<String, u8>,
            Option<String>,
            bool,
        ),
    }
    let (
        anchors_to_check,
        offenders,
        known_source_prio_by_url,
        own_write_url_norm,
        outflow_allowed,
    ) = match state.with_session_required(
        session,
        "Wallet session changed during L2 lifecycle tick",
        |service| {
            let mut lc = load_lifecycle_state(service, password)?;
            if !force && now_secs.saturating_sub(lc.last_tick_secs) < HEALTH_CHECK_INTERVAL_SECS {
                return Ok(CollectOutcome::Early);
            }
            lc.last_tick_secs = now_secs;
            let anchors: Vec<(String, AnchorManifest, Option<StoredGuarantorRole>, bool)> = service
                .with_wallet(|w| {
                    w.profile
                        .l2_anchor_manifests
                        .iter()
                        .map(|(aid, m)| {
                            let has_creator = w.profile.l2_anchor_nonces.contains_key(aid);
                            let g_role = lc.guarantor_roles.get(aid).cloned();
                            (aid.clone(), m.clone(), g_role, has_creator)
                        })
                        .collect()
                })
                .map_err(FrontendError::from)?;
            save_lifecycle_state(service, &lc, password)?;
            let offenders = state.get_or_load_server_offender_list(service, password)?;
            let known_sources = crate::services::l2_anchor_service::collect_known_sync_sources(
                state,
                service,
                &security_policy,
                password,
            );
            let mut known_source_prio_by_url: BTreeMap<String, u8> = BTreeMap::new();
            for s in known_sources {
                let norm = normalize_sync_url(&s.write_url);
                if !norm.is_empty() {
                    known_source_prio_by_url
                        .entry(norm)
                        .and_modify(|p| *p = (*p).min(s.priority))
                        .or_insert(s.priority);
                }
            }
            if let Ok(contacts) = state.load_contacts(service, password) {
                for c in contacts {
                    for d in c.syncs {
                        let norm = normalize_sync_url(&d.write_url);
                        if !norm.is_empty() {
                            known_source_prio_by_url.entry(norm).or_insert(2);
                        }
                    }
                }
            }
            let own_write_url_norm = state
                .get_or_load_own_sync_descriptor(service, password)
                .ok()
                .flatten()
                .map(|d| normalize_sync_url(&d.write_url))
                .filter(|u| !u.is_empty());
            let preflight = crate::services::preflight::ValueOutflowPreflight {
                require_restore_reconciled: true,
                check_known_offender_dids: &[],
                check_blocking_intent_local_ids: &[],
                check_anchor_id_quorum_health: None,
                preverified_anchor_manifest: None,
            };
            let outflow_allowed = crate::services::preflight::run_value_outflow_preflight(
                state, service, password, &preflight,
            )
            .is_ok()
                && matches!(service.get_seal_lock_reason(password), Ok(None));
            Ok(CollectOutcome::Ready(
                anchors,
                offenders,
                known_source_prio_by_url,
                own_write_url_norm,
                outflow_allowed,
            ))
        },
    )? {
        CollectOutcome::Early => return Ok(()),
        CollectOutcome::Ready(anchors, offenders, known_prio, own_w, outflow_ok) => {
            (anchors, offenders, known_prio, own_w, outflow_ok)
        }
    };

    for (anchor_hex, manifest, g_role_opt, has_creator) in anchors_to_check {
        let Ok(anchor_id) = decode_hex_fixed::<32>(&anchor_hex) else {
            continue;
        };

        let existing_phase = state.with_session_required(
            session,
            "Wallet session changed during L2 lifecycle tick",
            |service| {
                let lc = load_lifecycle_state(service, password)?;
                Ok(lc
                    .anchors
                    .get(&anchor_hex)
                    .and_then(|h| h.active_rotation.clone()))
            },
        )?;

        // Resume active crash-safe rotation phase if any
        if let Some(phase) = existing_phase {
            let step_result = drive_active_rotation(
                RotationEnv {
                    state,
                    session,
                    security_policy: &security_policy,
                    lifecycle: &internal_lc,
                    client,
                    now_secs,
                    password,
                },
                anchor_id,
                manifest.clone(),
                phase,
            )
            .await;
            state.with_session_required(
                session,
                "Wallet session changed after driving active L2 rotation",
                |service| {
                    let mut lc = load_lifecycle_state(service, password)?;
                    let entry = lc
                        .anchors
                        .entry(anchor_hex.clone())
                        .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.clone()));
                    match &step_result {
                        Ok(outcome) => {
                            apply_rotation_step_outcome(entry, Ok(outcome), now_secs);
                        }
                        Err(err) => {
                            apply_rotation_step_outcome(entry, Err(err.message.as_str()), now_secs);
                        }
                    }
                    save_lifecycle_state(service, &lc, password)
                },
            )?;
            continue;
        }

        // Probe health of every server in manifest.servers (relocation-aware)
        let mut unhealthy_now: HashSet<String> = HashSet::new();
        let mut revoked_now: HashSet<String> = HashSet::new();
        let mut probe_cache_deltas: Vec<FrontendLockServerProbeResult> = Vec::new();
        let mut reported_creator_activities: Vec<u64> = Vec::new();

        for srv in &manifest.servers {
            let lock_hex = hex::encode(srv.lock_pub);
            if is_server_key_revoked(&offenders, &srv.lock_pub, Some(&srv.admin_pub)) {
                revoked_now.insert(lock_hex);
                continue;
            }
            let (mut active_write_url, _) =
                resolve_server_locator(srv, &internal_lc.l2_probe_cache);
            let admin_hex = hex::encode(srv.admin_pub);
            let mut is_healthy_now = false;
            match sync_client::fetch_sync_info(client, &active_write_url).await {
                Ok(info) => {
                    if let Some(ref rev) = info.revocation {
                        if let Some(relocated_probe) = try_follow_relocation_notice(
                            client,
                            &mut internal_lc.l2_probe_cache,
                            srv,
                            rev,
                        )
                        .await
                        {
                            probe_cache_deltas.push(relocated_probe);
                            active_write_url =
                                resolve_server_locator(srv, &internal_lc.l2_probe_cache).0;
                            is_healthy_now = true;
                        } else {
                            match classify_server_probe_info(&info, &lock_hex, &admin_hex, now_secs)
                            {
                                ServerProbeOutcome::Healthy => {
                                    is_healthy_now = true;
                                }
                                ServerProbeOutcome::Unhealthy => {
                                    unhealthy_now.insert(lock_hex.clone());
                                }
                                ServerProbeOutcome::Revoked => {
                                    revoked_now.insert(lock_hex.clone());
                                }
                            }
                        }
                    } else {
                        match classify_server_probe_info(&info, &lock_hex, &admin_hex, now_secs) {
                            ServerProbeOutcome::Healthy => {
                                is_healthy_now = true;
                                if let Some(ref lock_info) = info.lock {
                                    if let Some(ref del) = lock_info.delegation {
                                        if sync_client::verify_lock_delegation(
                                            &lock_hex,
                                            del,
                                            Some(&admin_hex),
                                        ) {
                                            let target_url = del.write_url.trim();
                                            let cur_cached = internal_lc
                                                .l2_probe_cache
                                                .get(&lock_hex)
                                                .and_then(|p| p.delegation.as_ref())
                                                .map(|d| d.issued_at)
                                                .unwrap_or(0);
                                            if del.issued_at > cur_cached
                                                && sync_client::validate_sync_url(target_url)
                                                    .is_ok()
                                                && sync_client::validate_sync_read_url(
                                                    del.read_url.trim(),
                                                )
                                                .is_ok()
                                            {
                                                let updated_probe = sync_client::probe_lock_server(
                                                    client,
                                                    target_url,
                                                    Some(&admin_hex),
                                                )
                                                .await;
                                                if validate_in_place_delegation_refresh(
                                                    srv,
                                                    del,
                                                    &updated_probe,
                                                    &offenders,
                                                ) && upsert_verified_lock_probe(
                                                    &mut internal_lc.l2_probe_cache,
                                                    updated_probe.clone(),
                                                ) {
                                                    probe_cache_deltas.push(updated_probe);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            ServerProbeOutcome::Unhealthy => {
                                unhealthy_now.insert(lock_hex.clone());
                            }
                            ServerProbeOutcome::Revoked => {
                                revoked_now.insert(lock_hex.clone());
                            }
                        }
                    }
                }
                Err(_) => {
                    unhealthy_now.insert(lock_hex);
                }
            }

            if g_role_opt.is_some() && is_healthy_now {
                if let Ok(Some(st)) =
                    sync_client::get_lock_freeze_status(client, &active_write_url, &anchor_hex)
                        .await
                {
                    if let Some(act) = st.last_creator_activity_secs {
                        reported_creator_activities.push(act);
                    }
                }
            }
        }

        let last_creator_activity_secs = compute_quorum_creator_activity_secs(
            manifest.issued_at_ms / 1_000,
            &reported_creator_activities,
            manifest.threshold,
            now_secs,
        );

        if !probe_cache_deltas.is_empty() {
            let _ = state.mutate_internal_lifecycle_for_session(session, password, |lc| {
                let mut changed = false;
                for probe in probe_cache_deltas {
                    changed |= upsert_verified_lock_probe(&mut lc.l2_probe_cache, probe);
                }
                changed
            });
        }

        // Update hysteresis state and collect servers triggering rotation
        let failed_lock_pubs: HashSet<LockPubKey> = state.with_session_required(
            session,
            "Wallet session changed after probing anchor servers",
            |service| {
                let mut lc = load_lifecycle_state(service, password)?;
                let health = lc
                    .anchors
                    .entry(anchor_hex.clone())
                    .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.clone()));
                let failed = apply_health_observations_and_collect_demotions(
                    health,
                    &unhealthy_now,
                    &revoked_now,
                    now_secs,
                );
                save_lifecycle_state(service, &lc, password)?;
                Ok(failed)
            },
        )?;

        // Derive signing authority keys if present in wallet (strictly gated on DailyPay)
        let (creator_sk_opt, guarantor_sk_opt): (Option<SigningKey>, Option<(u8, SigningKey)>) =
            state.with_session_required(
                session,
                "Wallet session changed before deriving L2 rotation authority keys",
                |service| {
                    service
                        .with_authorized_identity(
                            human_money_core::AccessMode::DailyPay,
                            password,
                            |w, id| {
                                let c_sk = if has_creator {
                                    w.profile
                                        .l2_anchor_nonces
                                        .get(&anchor_hex)
                                        .and_then(|h| hex::decode(h).ok())
                                        .and_then(|b| {
                                            l2_gateway::derive_anchor_mig_signing_key(id, &b).ok()
                                        })
                                } else {
                                    None
                                };
                                let g_sk = g_role_opt.as_ref().and_then(|r| {
                                    let nb = hex::decode(&r.context_nonce_hex).ok()?;
                                    let sk = l2_rotation::derive_guarantor_mig_signing_key(id, &nb)
                                        .ok()?;
                                    Some((r.guarantor_index, sk))
                                });
                                (c_sk, g_sk)
                            },
                        )
                        .map_err(FrontendError::from)
                },
            )?;

        if !outflow_allowed {
            continue;
        }

        if failed_lock_pubs.is_empty() {
            if manifest.authority.guarantor_mig_pubs.is_empty() && manifest.servers.len() < 3 {
                if let Some(ref c_sk) = creator_sk_opt {
                    if let Some(growth_prop) = select_auto_growth_proposal(
                        anchor_id,
                        &manifest,
                        c_sk,
                        &security_policy,
                        &internal_lc,
                        &offenders,
                        &known_source_prio_by_url,
                        own_write_url_norm.as_deref(),
                        now_secs,
                    ) {
                        initiate_rotation_proposal(
                            RotationEnv {
                                state,
                                session,
                                security_policy: &security_policy,
                                lifecycle: &internal_lc,
                                client,
                                now_secs,
                                password,
                            },
                            anchor_id,
                            &anchor_hex,
                            &manifest,
                            growth_prop,
                        )
                        .await?;
                    }
                }
            }
            continue;
        }

        let guarantor_tuple = guarantor_sk_opt
            .as_ref()
            .map(|(idx, sk)| (*idx, sk, last_creator_activity_secs));

        let Some(proposal) = select_rotation_proposal(
            anchor_id,
            &manifest,
            &failed_lock_pubs,
            creator_sk_opt.as_ref(),
            guarantor_tuple,
            &security_policy,
            &internal_lc,
            &offenders,
            now_secs,
        ) else {
            continue;
        };

        initiate_rotation_proposal(
            RotationEnv {
                state,
                session,
                security_policy: &security_policy,
                lifecycle: &internal_lc,
                client,
                now_secs,
                password,
            },
            anchor_id,
            &anchor_hex,
            &manifest,
            proposal,
        )
        .await?;
    }

    Ok(())
}

/// Executes one health check, guarantor-veto scan, and crash-resumable rotation step using `now_secs`,
/// returning the refreshed frontend status DTO.
#[cfg(test)]
pub(crate) async fn run_lifecycle_tick_with_now(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    now_secs: u64,
    force: bool,
    password: Option<&str>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    run_lifecycle_tick_core(state, session, client, now_secs, force, password).await?;
    state.with_session_required(
        session,
        "Wallet session changed before completing L2 lifecycle status",
        |service| {
            let lc = load_lifecycle_state(service, password)?;
            build_status_dto(state, service, &lc, password)
        },
    )
}

#[cfg(test)]
pub(crate) fn build_status_dto(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    lc: &StoredL2LifecycleState,
    password: Option<&str>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    let offenders = state.get_or_load_server_offender_list(service, password)?;
    let anchors: Vec<FrontendAnchorLifecycleDto> = service
        .with_wallet(|w| {
            w.profile
                .l2_anchor_manifests
                .iter()
                .map(|(aid, m)| {
                    let health = lc.anchors.get(aid);
                    let (phase_label, eff) = match health.and_then(|h| h.active_rotation.as_ref()) {
                        Some(StoredRotationPhase::Proposed {
                            effective_at_secs, ..
                        }) => (Some("proposed".to_string()), Some(*effective_at_secs)),
                        Some(StoredRotationPhase::Imported { .. }) => {
                            (Some("imported".to_string()), None)
                        }
                        None => (None, None),
                    };
                    FrontendAnchorLifecycleDto {
                        anchor_id: aid.clone(),
                        revision: m.revision,
                        reduced_guarantor_protection: m
                            .has_reduced_guarantor_protection()
                            .unwrap_or(false),
                        unhealthy_servers: health
                            .map(|h| h.unhealthy_since_by_lock_pub.keys().cloned().collect())
                            .unwrap_or_default(),
                        revoked_servers: health
                            .map(|h| h.revoked_lock_pubs.iter().cloned().collect())
                            .unwrap_or_default(),
                        active_rotation_phase: phase_label,
                        effective_at_secs: eff,
                        last_checked_secs: health.map(|h| h.last_checked_secs).unwrap_or(0),
                        last_error: health.and_then(|h| h.last_error.clone()),
                    }
                })
                .collect()
        })
        .map_err(FrontendError::from)?;

    Ok(FrontendL2LifecycleStatusDto {
        last_tick_secs: lc.last_tick_secs,
        tracked_anchors_count: anchors.len(),
        guarantor_roles_count: lc.guarantor_roles.len(),
        server_offenders_count: offenders.records.len(),
        auto_vetoes_sent: lc.auto_vetoes_sent,
        last_veto_reason: lc.last_veto_reason.clone(),
        anchors,
    })
}

pub(crate) async fn maybe_run_background_lifecycle_tick(state: &AppState) {
    let session = {
        let Ok(mut service) = state.service.lock() else {
            return;
        };
        if !service.is_wallet_unlocked()
            || !service.is_session_active()
            || service
                .require_access_mode(human_money_core::AccessMode::DailyPay, None)
                .is_err()
        {
            return;
        }
        let Ok(session) = state.session_token_for_service(&service) else {
            return;
        };
        let lc = match load_lifecycle_state(&mut service, None) {
            Ok(lc) => lc,
            Err(e) => {
                log::debug!(
                    "Background L2 lifecycle tick skipped (lifecycle state load error): {}",
                    e.message
                );
                return;
            }
        };
        let now_secs = sync_client::current_unix_timestamp();
        if now_secs.saturating_sub(lc.last_tick_secs) < HEALTH_CHECK_INTERVAL_SECS {
            return;
        }
        session
    };
    let client = sync_client::create_sync_probe_client();
    let now_secs = sync_client::current_unix_timestamp();
    if let Err(err) = run_lifecycle_tick_core(state, &session, &client, now_secs, false, None).await
    {
        log::debug!(
            "Background L2 lifecycle tick skipped/failed: {}",
            err.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::common::max_pow_base_bits_for_servers;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::{
        humoco_proto::{
            encode_quarter, types::LockReceipt, AnchorAuthority, GenesisTicket, MigrationCert,
            ProposalInitiator,
        },
        services::{l2_gateway, l2_rotation},
        test_utils::{self, ACTORS},
    };
    use humoco_market::{own_envelope, render_wire, sign_sd_card, DropDescriptor, WireFilter};
    use humoco_wot::KeyRotationReason;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use uuid::Uuid;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("l2-lc-{}-{}", tag, Uuid::new_v4()))
    }

    fn make_test_manifest(
        s1_sk: &SigningKey,
        s2_sk: &SigningKey,
        s1_url: &str,
        s2_url: &str,
        successor: Option<ServerEntry>,
        creator_mig_pub: [u8; 32],
        guarantor_mig_pubs: Vec<[u8; 32]>,
    ) -> AnchorManifest {
        AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![
                ServerEntry {
                    write_url: format!("{}/sync.php", s1_url),
                    read_url: format!("{}/d", s1_url),
                    admin_pub: s1_sk.verifying_key().to_bytes(),
                    lock_pub: s1_sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Creator,
                },
                ServerEntry {
                    write_url: format!("{}/sync.php", s2_url),
                    read_url: format!("{}/d", s2_url),
                    admin_pub: s2_sk.verifying_key().to_bytes(),
                    lock_pub: s2_sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
            ],
            successors: successor.into_iter().collect(),
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs,
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: encode_quarter(2035, 4).unwrap(),
        }
    }

    #[test]
    fn test_server_offender_list_blocks_equivocating_lock_key() {
        let dir = temp_dir("offender");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();

        let bad_sk = SigningKey::from_bytes(&[77u8; 32]);
        let bad_pub = bad_sk.verifying_key().to_bytes();
        let make_rcpt = |child: [u8; 32], eh: [u8; 32]| {
            let mut r = LockReceipt {
                kind: LockReceiptKind::FirstWrite,
                lock_pub: bad_pub,
                bucket: [1u8; 32],
                slot: [2u8; 32],
                child_lock: child,
                entry_hash: eh,
                retain_until_quarter: 8159,
                write_seq: 1,
                bucket_head: [3u8; 32],
                recipient_nonce: [4u8; 32],
                sig: [0u8; 64],
            };
            r.sig = bad_sk.sign(&r.digest()).to_bytes();
            r
        };
        let eq = EquivocationProof {
            receipt_a: make_rcpt([10u8; 32], [20u8; 32]),
            receipt_b: make_rcpt([11u8; 32], [21u8; 32]),
        };

        let newly = record_equivocation_proof(&mut svc, None, &eq, 1_760_000_000, Some(pw))
            .expect("record equivocation");
        assert!(newly);

        let loaded = load_server_offender_list(&mut svc, Some(pw)).unwrap();
        assert!(loaded.is_lock_pub_revoked(&bad_pub));

        // Corrupt ServerOffenderList or L2LifecycleState must fail closed without overwriting
        svc.save_encrypted_data(SERVER_OFFENDER_LIST_KEY, b"{corrupt-offenders", Some(pw))
            .unwrap();
        assert!(load_server_offender_list(&mut svc, Some(pw)).is_err());
        assert!(record_equivocation_proof(&mut svc, None, &eq, 1_760_000_000, Some(pw)).is_err());
        assert_eq!(
            svc.load_encrypted_data(SERVER_OFFENDER_LIST_KEY, Some(pw))
                .unwrap(),
            b"{corrupt-offenders"
        );

        svc.save_encrypted_data(L2_LIFECYCLE_STATE_KEY, b"{corrupt-lifecycle", Some(pw))
            .unwrap();
        assert!(load_lifecycle_state(&mut svc, Some(pw)).is_err());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_discovery_cascade_resolves_successor_chain_and_card_store() {
        let dir = temp_dir("discovery");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.bob, "Bob", pw);
        svc.unlock_session(pw, 600).unwrap();

        // Build Rev 1 -> Rev 2 migrated manifest
        let s1_sk = SigningKey::from_bytes(&[11u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[12u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[13u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[14u8; 32]);

        let rev1 = make_test_manifest(
            &s1_sk,
            &s2_sk,
            "http://127.0.0.1:9",
            "http://127.0.0.1:9",
            Some(ServerEntry {
                write_url: "http://127.0.0.1:9/sync.php".into(),
                read_url: "http://127.0.0.1:9/d".into(),
                admin_pub: s3_sk.verifying_key().to_bytes(),
                lock_pub: s3_sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![SigningKey::from_bytes(&[15u8; 32])
                .verifying_key()
                .to_bytes()],
        );
        let anchor_id = rev1.rev1_anchor_id().unwrap();
        let proposal = l2_rotation::build_holder_standby_proposal(
            anchor_id,
            &rev1,
            &[s1_sk.verifying_key().to_bytes()],
            1_760_000_100,
        )
        .unwrap();
        let exports = vec![
            {
                let mut exp = SnapshotExport {
                    snapshot: humoco_proto::FreezeSnapshot {
                        anchor_id,
                        bucket: humoco_proto::derive_bucket(
                            &anchor_id,
                            &s1_sk.verifying_key().to_bytes(),
                        ),
                        from_rev: 1,
                        retain_until_quarter: rev1.retain_until_quarter,
                        write_seq: 0,
                        bucket_head: humoco_proto::crypto::compute_bucket_init_head(
                            &humoco_proto::derive_bucket(
                                &anchor_id,
                                &s1_sk.verifying_key().to_bytes(),
                            ),
                            rev1.retain_until_quarter,
                        ),
                        state_hash: humoco_proto::compute_freeze_state_hash(
                            &anchor_id,
                            1,
                            &[],
                            &[],
                        )
                        .unwrap(),
                        permanent: true,
                        proposal_hash: Some(proposal.core_digest().unwrap()),
                        frozen_at: 1_760_000_100,
                        expires_at: None,
                        lock_pub: s1_sk.verifying_key().to_bytes(),
                        sig: [0u8; 64],
                    },
                    entries: vec![],
                    conflicts: vec![],
                };
                exp.snapshot.sig = s1_sk.sign(&exp.snapshot.digest()).to_bytes();
                exp
            },
            {
                let mut exp = SnapshotExport {
                    snapshot: humoco_proto::FreezeSnapshot {
                        anchor_id,
                        bucket: humoco_proto::derive_bucket(
                            &anchor_id,
                            &s2_sk.verifying_key().to_bytes(),
                        ),
                        from_rev: 1,
                        retain_until_quarter: rev1.retain_until_quarter,
                        write_seq: 0,
                        bucket_head: humoco_proto::crypto::compute_bucket_init_head(
                            &humoco_proto::derive_bucket(
                                &anchor_id,
                                &s2_sk.verifying_key().to_bytes(),
                            ),
                            rev1.retain_until_quarter,
                        ),
                        state_hash: humoco_proto::compute_freeze_state_hash(
                            &anchor_id,
                            1,
                            &[],
                            &[],
                        )
                        .unwrap(),
                        permanent: true,
                        proposal_hash: Some(proposal.core_digest().unwrap()),
                        frozen_at: 1_760_000_100,
                        expires_at: None,
                        lock_pub: s2_sk.verifying_key().to_bytes(),
                        sig: [0u8; 64],
                    },
                    entries: vec![],
                    conflicts: vec![],
                };
                exp.snapshot.sig = s2_sk.sign(&exp.snapshot.digest()).to_bytes();
                exp
            },
        ];
        let resolved =
            resolve_import_union(&anchor_id, &rev1, &proposal, &exports, &[], 0).unwrap();
        let mut ack_s3 = ImportAck {
            anchor_id,
            proposal_hash: proposal.core_digest().unwrap(),
            to_rev: 2,
            imported_state_hash: resolved.imported_state_hash,
            retain_until_quarter: rev1.retain_until_quarter,
            lock_pub: s3_sk.verifying_key().to_bytes(),
            acked_at: 1_760_000_200,
            sig: [0u8; 64],
        };
        ack_s3.sig = s3_sk.sign(&ack_s3.digest()).to_bytes();
        let mut ack_s2 = ImportAck {
            anchor_id,
            proposal_hash: proposal.core_digest().unwrap(),
            to_rev: 2,
            imported_state_hash: resolved.imported_state_hash,
            retain_until_quarter: rev1.retain_until_quarter,
            lock_pub: s2_sk.verifying_key().to_bytes(),
            acked_at: 1_760_000_200,
            sig: [0u8; 64],
        };
        ack_s2.sig = s2_sk.sign(&ack_s2.digest()).to_bytes();
        let c_digest = MigrationCert::commit_digest(
            &anchor_id,
            &proposal.core_digest().unwrap(),
            2,
            &resolved.imported_state_hash,
        );
        let commit_sigs = vec![
            CommitteeSignature {
                lock_pub: s1_sk.verifying_key().to_bytes(),
                committed_at: 1_760_000_300,
                sig: s1_sk.sign(&c_digest).to_bytes(),
            },
            CommitteeSignature {
                lock_pub: s2_sk.verifying_key().to_bytes(),
                committed_at: 1_760_000_300,
                sig: s2_sk.sign(&c_digest).to_bytes(),
            },
        ];
        let (rev2, _) = l2_rotation::assemble_migrated_manifest(
            &rev1,
            &anchor_id,
            proposal,
            resolved.imported_state_hash,
            vec![ack_s3, ack_s2],
            commit_sigs,
        )
        .unwrap();

        // Spawn HTTP server serving /anchors/<anchor_id>.bin
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let rev2_json = serde_json::to_vec(&rev2).unwrap();
        let expected_path = format!("/anchors/{}.bin", hex::encode(anchor_id));
        let server_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .unwrap_or("")
                .split_whitespace()
                .nth(1)
                .unwrap_or("");
            if path.ends_with(&expected_path) {
                let mut out = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                    rev2_json.len()
                )
                .into_bytes();
                out.extend_from_slice(&rev2_json);
                stream.write_all(&out).await.unwrap();
            } else {
                stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await
                    .unwrap();
            }
        });

        // Old creator rotated key to new creator DID, and stored new drop in CardStore!
        let old_creator_did = ACTORS.alice.identity.user_id.clone();
        let new_creator_did = ACTORS.charlie.identity.user_id.clone();
        let att = KeySuccessorAttestation::sign_dual(
            &crate::state::DidWotSigner {
                did: crate::state::canonicalize_did(&old_creator_did).unwrap(),
                signing_key: ACTORS.alice.identity.signing_key.clone(),
            },
            &crate::state::DidWotSigner {
                did: crate::state::canonicalize_did(&new_creator_did).unwrap(),
                signing_key: ACTORS.charlie.identity.signing_key.clone(),
            },
            KeyRotationReason::Routine,
            None,
            sync_client::current_unix_timestamp().saturating_sub(10),
        )
        .unwrap();
        save_key_successor_attestations(&mut svc, &[att], Some(pw)).unwrap();

        let card = sign_sd_card(
            humoco_market::CardPayloadInput {
                did: new_creator_did.clone(),
                v: 1,
                valid_until: sync_client::current_unix_timestamp() + 86_400,
                max_hops: 2,
                ad_count: 0,
                allow_inquiries: false,
                drops: vec![DropDescriptor {
                    write_url: format!("http://127.0.0.1:{}/sync.php", port),
                    read_url: format!("http://127.0.0.1:{}/d-alice", port),
                    slot_id: None,
                    drop_pub: "aa".repeat(32),
                }],
            },
            vec![],
            &ACTORS.charlie.identity.signing_key,
        )
        .unwrap();
        let card_wire = render_wire(&card, 1, WireFilter::Full).unwrap();
        let mut store = CardStore::default();
        humoco_market::accept_envelope(
            &mut store,
            &own_envelope(card_wire),
            sync_client::current_unix_timestamp(),
            100,
            None,
            &|_| false,
        );
        svc.save_encrypted_data(
            MARKET_CARD_STORE_KEY,
            &serde_json::to_vec(&store).unwrap(),
            Some(pw),
        )
        .unwrap();

        let state = AppState::new(svc);
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let client = sync_client::create_sync_probe_client();

        let discovered = discover_anchor_manifest(
            &state,
            &session,
            &client,
            &anchor_id,
            Some(&old_creator_did),
            &[],
            Some(pw),
        )
        .await
        .unwrap()
        .expect("must discover migrated rev 2 manifest");

        assert_eq!(discovered.revision, 2);
        let cached = state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&hex::encode(anchor_id))
                    .cloned()
            })
            .unwrap()
            .unwrap();
        assert_eq!(cached.revision, 2);

        server_task.await.unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_hysteresis_5d_vs_immediate_revocation_and_guarantor_auto_veto() {
        let dir = temp_dir("hysteresis");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();

        // Keys for 2/3-like 1-server or 2-server? Wait: 2/2 with S1(Creator, standby S3), S2(Guarantor)
        let s1_sk = SigningKey::from_bytes(&[21u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[22u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[23u8; 32]);
        let anchor_nonce = [88u8; 32];
        let creator_mig_sk = svc
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
            })
            .unwrap()
            .unwrap();
        let g_mig_sk = SigningKey::from_bytes(&[24u8; 32]);

        // Mock S1 (goes down when down_flag is set) & S2 (stays up and participates in rotation!)
        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p3 = l3.local_addr().unwrap().port();

        let s3_entry = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p3),
            read_url: format!("http://127.0.0.1:{}/d", p3),
            admin_pub: s3_sk.verifying_key().to_bytes(),
            lock_pub: s3_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            &format!("http://127.0.0.1:{}", p1),
            &format!("http://127.0.0.1:{}", p2),
            Some(s3_entry),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_sk.verifying_key().to_bytes()],
        );
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let mut tickets = Vec::new();
        for sk in [&s1_sk, &s2_sk] {
            let mut t = GenesisTicket {
                anchor_id,
                retain_until_quarter: manifest.retain_until_quarter,
                issued_at: 1_760_000_000,
                lock_pub: sk.verifying_key().to_bytes(),
                sig: [0u8; 64],
            };
            t.sig = sk.sign(&t.digest()).to_bytes();
            tickets.push(t);
        }
        svc.register_l2_anchor_template_with_nonce(
            manifest.clone(),
            tickets,
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();

        let s1_revoked = Arc::new(AtomicBool::new(false));
        let stop_servers = Arc::new(AtomicBool::new(false));
        let voted_prop: Arc<Mutex<Option<MigrationProposal>>> = Arc::new(Mutex::new(None));
        let effective_ts = 1_760_500_000u64;

        let spawn_mock = |listener: tokio::net::TcpListener,
                          sk: SigningKey,
                          revoked_flag: Arc<AtomicBool>,
                          vp: Arc<Mutex<Option<MigrationProposal>>>,
                          stop: Arc<AtomicBool>| {
            tokio::spawn(async move {
                let lock_pub = sk.verifying_key().to_bytes();
                let lock_hex = hex::encode(lock_pub);
                let bucket = humoco_proto::derive_bucket(&anchor_id, &lock_pub);
                let retain_q = encode_quarter(2035, 4).unwrap();
                let mut current_rev = 1u64;
                loop {
                    let accept = tokio::time::timeout(
                        std::time::Duration::from_millis(50),
                        listener.accept(),
                    )
                    .await;
                    let Ok(Ok((mut stream, _))) = accept else {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        continue;
                    };
                    let mut buf = [0u8; 32_768];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let mut parts = raw.splitn(2, "\r\n\r\n");
                    let head = parts.next().unwrap_or("");
                    let body = parts.next().unwrap_or("");
                    let path = head
                        .lines()
                        .next()
                        .unwrap_or("")
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("");

                    let resp_json: serde_json::Value = if path.contains("info") {
                        let rev_val = if revoked_flag.load(Ordering::Relaxed) {
                            let n = sync_client::sign_revocation_notice(
                                &sk,
                                1_760_000_000,
                                1_760_000_010,
                                Some("planned".into()),
                                None,
                            )
                            .unwrap();
                            serde_json::to_value(n).unwrap()
                        } else {
                            serde_json::Value::Null
                        };
                        serde_json::json!({
                            "drop_pub": lock_hex,
                            "read_url": "http://127.0.0.1:9/d",
                            "revocation": rev_val,
                            "lock": {
                                "lock_pub": lock_hex,
                                "max_retention_quarter": 8159,
                                "pow_base_bits": 0,
                                "allow_pow_tickets": true,
                                "ticket_pow_bits": 16
                            }
                        })
                    } else if path.contains("lock_freeze") {
                        if head.starts_with("GET ") {
                            let p = vp.lock().unwrap().clone();
                            let snap = p.as_ref().map(|prop| {
                                let mut s = humoco_proto::FreezeSnapshot {
                                    anchor_id,
                                    bucket,
                                    from_rev: 1,
                                    retain_until_quarter: retain_q,
                                    write_seq: 0,
                                    bucket_head: humoco_proto::crypto::compute_bucket_init_head(
                                        &bucket, retain_q,
                                    ),
                                    state_hash: humoco_proto::compute_freeze_state_hash(
                                        &anchor_id,
                                        1,
                                        &[],
                                        &[],
                                    )
                                    .unwrap(),
                                    permanent: true,
                                    proposal_hash: Some(prop.core_digest().unwrap()),
                                    frozen_at: 1_760_499_900,
                                    expires_at: None,
                                    lock_pub,
                                    sig: [0u8; 64],
                                };
                                s.sig = sk.sign(&s.digest()).to_bytes();
                                SnapshotExport {
                                    snapshot: s,
                                    entries: vec![],
                                    conflicts: vec![],
                                }
                            });
                            serde_json::json!({
                                "anchor_id": hex::encode(anchor_id),
                                "bucket": hex::encode(bucket),
                                "active_manifest": serde_json::Value::Null,
                                "last_creator_activity_secs": 1_760_000_000u64,
                                "bucket_freeze_state": {},
                                "voted_proposal": p,
                                "snapshot_export": snap
                            })
                        } else {
                            let val: serde_json::Value = serde_json::from_str(body).unwrap();
                            if val.get("action").and_then(|v| v.as_str()) == Some("propose") {
                                let prop: MigrationProposal =
                                    serde_json::from_value(val["proposal"].clone()).unwrap();
                                *vp.lock().unwrap() = Some(prop);
                                serde_json::json!({
                                    "status": "proposed",
                                    "effective_at": effective_ts
                                })
                            } else {
                                serde_json::json!({ "status": "ok" })
                            }
                        }
                    } else if path.contains("lock_import") {
                        let req: serde_json::Value = serde_json::from_str(body).unwrap();
                        let prop: MigrationProposal =
                            serde_json::from_value(req["proposal"].clone()).unwrap();
                        let mut ack = ImportAck {
                            anchor_id,
                            proposal_hash: prop.core_digest().unwrap(),
                            to_rev: prop.to_rev,
                            imported_state_hash: humoco_proto::compute_import_union_state_hash(
                                &anchor_id,
                                prop.to_rev,
                                &BTreeMap::new(),
                            )
                            .unwrap(),
                            retain_until_quarter: retain_q,
                            lock_pub,
                            acked_at: effective_ts,
                            sig: [0u8; 64],
                        };
                        ack.sig = sk.sign(&ack.digest()).to_bytes();
                        serde_json::json!({ "import_ack": ack })
                    } else if path.contains("lock_commit") {
                        let req: serde_json::Value = serde_json::from_str(body).unwrap();
                        if req.get("action").and_then(|v| v.as_str()) == Some("sign_commit") {
                            let prop: MigrationProposal =
                                serde_json::from_value(req["proposal"].clone()).unwrap();
                            let ish = decode_hex_fixed::<32>(
                                req["imported_state_hash"].as_str().unwrap(),
                            )
                            .unwrap();
                            let d = MigrationCert::commit_digest(
                                &anchor_id,
                                &prop.core_digest().unwrap(),
                                prop.to_rev,
                                &ish,
                            );
                            let cs = CommitteeSignature {
                                lock_pub,
                                committed_at: effective_ts,
                                sig: sk.sign(&d).to_bytes(),
                            };
                            serde_json::json!({ "committee_signature": cs })
                        } else {
                            current_rev += 1;
                            serde_json::json!({ "status": "finalized", "revision": current_rev })
                        }
                    } else {
                        serde_json::json!({})
                    };

                    let json_str = resp_json.to_string();
                    let http = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        json_str.len(),
                        json_str
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };

        let h1 = spawn_mock(
            l1,
            s1_sk.clone(),
            Arc::clone(&s1_revoked),
            Arc::clone(&voted_prop),
            Arc::clone(&stop_servers),
        );
        let h2 = spawn_mock(
            l2,
            s2_sk.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::clone(&voted_prop),
            Arc::clone(&stop_servers),
        );
        let h3 = spawn_mock(
            l3,
            s3_sk.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(Mutex::new(None)),
            Arc::clone(&stop_servers),
        );

        let state = AppState::new(svc);
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let client = sync_client::create_sync_probe_client();

        // Trigger immediate rotation on SyncRevocationNotice from S1
        s1_revoked.store(true, Ordering::Relaxed);
        let status1 =
            run_lifecycle_tick_with_now(&state, &session, &client, 1_760_000_050, true, Some(pw))
                .await
                .unwrap();
        assert_eq!(
            status1.anchors[0].active_rotation_phase.as_deref(),
            Some("proposed")
        );

        // Advance past timelock -> finishes Rev 2 automatically!
        let status2 = run_lifecycle_tick_with_now(
            &state,
            &session,
            &client,
            effective_ts + 10,
            true,
            Some(pw),
        )
        .await
        .unwrap();
        assert_eq!(status2.anchors[0].revision, 2);
        assert!(status2.anchors[0].active_rotation_phase.is_none());

        stop_servers.store(true, Ordering::Relaxed);
        let _ = h1.await;
        let _ = h2.await;
        let _ = h3.await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_guarantor_auto_veto_and_5d_hysteresis_timing() {
        let dir = temp_dir("veto");
        let pw = "pw";
        let (mut svc, _) =
            test_utils::setup_service_with_profile(&dir, &ACTORS.bob, "BobGuarantor", pw);
        svc.unlock_session(pw, 600).unwrap();

        let s1_sk = SigningKey::from_bytes(&[31u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[32u8; 32]);
        let s_evil_sk = SigningKey::from_bytes(&[33u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[34u8; 32]);
        let ctx_nonce = [44u8; 32];
        let g_mig_pub = svc
            .with_wallet_and_identity(|_, id| {
                l2_rotation::derive_guarantor_mig_signing_key(id, &ctx_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();

        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            &format!("http://127.0.0.1:{}", p1),
            &format!("http://127.0.0.1:{}", p2),
            None,
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_pub],
        );
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let anchor_hex = hex::encode(anchor_id);

        // Unauthorized proposal reducing guarantor protection without guarantor co-signature or removal
        let bad_proposal = {
            let mut prop = MigrationProposal {
                anchor_id,
                from_rev: 1,
                to_rev: 2,
                new_threshold: 2,
                new_servers: vec![
                    manifest.servers[0].clone(),
                    ServerEntry {
                        write_url: "https://evil.example/sync.php".into(),
                        read_url: "https://evil.example/d".into(),
                        admin_pub: s_evil_sk.verifying_key().to_bytes(),
                        lock_pub: s_evil_sk.verifying_key().to_bytes(),
                        nominated_by: NominatedBy::Creator,
                    },
                ],
                new_successors: vec![],
                retain_until_quarter: manifest.retain_until_quarter,
                proposed_at: 1_760_000_000,
                initiator: ProposalInitiator::Creator,
                proposer_sig: None,
                guarantor_cosigs: vec![],
            };
            let digest = prop.core_digest().unwrap();
            prop.proposer_sig = Some(creator_mig_sk.sign(&digest).to_bytes());
            prop
        };

        svc.update_l2_anchor_manifest(manifest.clone(), Some(pw))
            .unwrap();
        let mut lc = load_lifecycle_state(&mut svc, Some(pw)).unwrap();
        lc.guarantor_roles.insert(
            anchor_hex.clone(),
            StoredGuarantorRole {
                anchor_id: anchor_hex.clone(),
                guarantor_index: 0,
                context_nonce_hex: hex::encode(ctx_nonce),
            },
        );
        save_lifecycle_state(&mut svc, &lc, Some(pw)).unwrap();

        let veto_received = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        let spawn_srv = |listener: tokio::net::TcpListener,
                         sk: SigningKey,
                         prop: MigrationProposal,
                         veto_flag: Arc<AtomicBool>,
                         stop_flag: Arc<AtomicBool>| {
            tokio::spawn(async move {
                let lock_hex = hex::encode(sk.verifying_key().to_bytes());
                loop {
                    let accept = tokio::time::timeout(
                        std::time::Duration::from_millis(50),
                        listener.accept(),
                    )
                    .await;
                    let Ok(Ok((mut stream, _))) = accept else {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                        continue;
                    };
                    let mut buf = [0u8; 16384];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let mut parts = raw.splitn(2, "\r\n\r\n");
                    let head = parts.next().unwrap_or("");
                    let body = parts.next().unwrap_or("");
                    let path = head
                        .lines()
                        .next()
                        .unwrap_or("")
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("");

                    let resp = if path.contains("info") {
                        serde_json::json!({
                            "drop_pub": lock_hex,
                            "read_url": "http://127.0.0.1:9/d",
                            "revocation": null,
                            "lock": {
                                "lock_pub": lock_hex,
                                "max_retention_quarter": 8159,
                                "pow_base_bits": 0,
                                "ticket_pow_bits": 16
                            }
                        })
                    } else if path.contains("lock_freeze") && head.starts_with("GET ") {
                        serde_json::json!({
                            "anchor_id": hex::encode(anchor_id),
                            "bucket": "00".repeat(32),
                            "active_manifest": serde_json::Value::Null,
                            "last_creator_activity_secs": 1_760_000_000u64,
                            "bucket_freeze_state": {},
                            "voted_proposal": prop,
                            "snapshot_export": serde_json::Value::Null
                        })
                    } else if path.contains("lock_freeze") {
                        let val: serde_json::Value = serde_json::from_str(body).unwrap();
                        if val.get("action").and_then(|v| v.as_str()) == Some("veto") {
                            veto_flag.store(true, Ordering::Relaxed);
                        }
                        serde_json::json!({ "status": "vetoed" })
                    } else {
                        serde_json::json!({})
                    };
                    let txt = resp.to_string();
                    let http = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        txt.len(),
                        txt
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };

        let h1 = spawn_srv(
            l1,
            s1_sk.clone(),
            bad_proposal.clone(),
            Arc::clone(&veto_received),
            Arc::clone(&stop),
        );
        let h2 = spawn_srv(
            l2,
            s2_sk.clone(),
            bad_proposal,
            Arc::clone(&veto_received),
            Arc::clone(&stop),
        );

        let state = AppState::new(svc);
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let client = sync_client::create_sync_probe_client();

        let dto =
            run_lifecycle_tick_with_now(&state, &session, &client, 1_760_000_050, true, Some(pw))
                .await
                .unwrap();
        assert!(veto_received.load(Ordering::Relaxed));
        assert!(dto.auto_vetoes_sent >= 1);
        assert!(dto.last_veto_reason.is_some());

        stop.store(true, Ordering::Relaxed);
        let _ = h1.await;
        let _ = h2.await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_5day_unhealthy_hysteresis_boundary_blocks_day4_rotates_day5() {
        let dir = temp_dir("hyst5d");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();

        let s1_sk = SigningKey::from_bytes(&[51u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[52u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[53u8; 32]);
        let anchor_nonce = [54u8; 32];
        let creator_mig_sk = svc
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
            })
            .unwrap()
            .unwrap();
        let g_mig_sk = SigningKey::from_bytes(&[55u8; 32]);

        // Allocate closed port for S1 (simulates server offline / unreachable)
        let dead_l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_p = dead_l.local_addr().unwrap().port();
        drop(dead_l);

        // Live mock S2 (2/2 committee threshold=2? Wait: threshold=1 of 2? No, make 2/3 or S2+S_extra so quorum >= 2 is reachable!)
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p3 = l3.local_addr().unwrap().port();
        let l4 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p4 = l4.local_addr().unwrap().port();
        let s4_sk = SigningKey::from_bytes(&[56u8; 32]);

        let s1 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", dead_p),
            read_url: format!("http://127.0.0.1:{}/d", dead_p),
            admin_pub: s1_sk.verifying_key().to_bytes(),
            lock_pub: s1_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let s2 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p2),
            read_url: format!("http://127.0.0.1:{}/d", p2),
            admin_pub: s2_sk.verifying_key().to_bytes(),
            lock_pub: s2_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let s3 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p3),
            read_url: format!("http://127.0.0.1:{}/d", p3),
            admin_pub: s3_sk.verifying_key().to_bytes(),
            lock_pub: s3_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Guarantor(0),
        };
        let s4 = ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", p4),
            read_url: format!("http://127.0.0.1:{}/d", p4),
            admin_pub: s4_sk.verifying_key().to_bytes(),
            lock_pub: s4_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };

        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![s1.clone(), s2.clone(), s3.clone()],
            successors: vec![s4],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![g_mig_sk.verifying_key().to_bytes()],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        svc.register_l2_anchor_template_with_nonce(manifest, vec![], Some(&anchor_nonce), Some(pw))
            .unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let spawn_alive = |listener: tokio::net::TcpListener,
                           sk: SigningKey,
                           stop_flag: Arc<AtomicBool>| {
            tokio::spawn(async move {
                let port = listener.local_addr().unwrap().port();
                let write_url = format!("http://127.0.0.1:{}/sync.php", port);
                let read_url = format!("http://127.0.0.1:{}/d", port);
                let lock_hex = hex::encode(sk.verifying_key().to_bytes());
                let del_msg = humoco_market::format_lock_delegate_signature_message(
                    &lock_hex,
                    &lock_hex,
                    &write_url,
                    &read_url,
                    1_760_000_000,
                );
                let del_sig = hex::encode(sk.sign(del_msg.as_bytes()).to_bytes());
                loop {
                    let accept = tokio::time::timeout(
                        std::time::Duration::from_millis(40),
                        listener.accept(),
                    )
                    .await;
                    let Ok(Ok((mut stream, _))) = accept else {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                        continue;
                    };
                    let mut buf = [0u8; 16384];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let path = raw
                        .lines()
                        .next()
                        .unwrap_or("")
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("");
                    let resp = if path.contains("info") {
                        serde_json::json!({
                            "drop_pub": lock_hex,
                            "read_url": read_url,
                            "revocation": null,
                            "lock": {
                                "lock_pub": lock_hex,
                                "max_retention_quarter": 8159,
                                "pow_base_bits": 0,
                                "ticket_pow_bits": 16,
                                "delegation": {
                                    "admin_pub": lock_hex,
                                    "lock_pub": lock_hex,
                                    "write_url": write_url,
                                    "read_url": read_url,
                                    "issued_at": 1_760_000_000u64,
                                    "sig": del_sig
                                }
                            }
                        })
                    } else if path.contains("lock_freeze") {
                        serde_json::json!({
                            "status": "proposed",
                            "effective_at": 1_761_000_000u64
                        })
                    } else {
                        serde_json::json!({})
                    };
                    let txt = resp.to_string();
                    let http = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        txt.len(),
                        txt
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };

        let h2 = spawn_alive(l2, s2_sk.clone(), Arc::clone(&stop));
        let h3 = spawn_alive(l3, s3_sk.clone(), Arc::clone(&stop));
        let h4 = spawn_alive(l4, s4_sk.clone(), Arc::clone(&stop));

        let state = AppState::new(svc);
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let client = sync_client::create_sync_probe_client();

        // Day 0: S1 marked unhealthy, no rotation triggered yet
        let t0 = 1_760_000_000u64;
        let d0 = run_lifecycle_tick_with_now(&state, &session, &client, t0, true, Some(pw))
            .await
            .unwrap();
        assert_eq!(d0.anchors[0].unhealthy_servers.len(), 1);
        assert!(d0.anchors[0].active_rotation_phase.is_none());

        // Day 4: still < 5 days hysteresis -> MUST NOT propose rotation yet
        let d4 = run_lifecycle_tick_with_now(
            &state,
            &session,
            &client,
            t0 + 4 * 86_400 + 3_600,
            true,
            Some(pw),
        )
        .await
        .unwrap();
        assert!(d4.anchors[0].active_rotation_phase.is_none());

        // Day 5: >= 5 days elapsed -> proposes Phase 1 rotation!
        let d5 =
            run_lifecycle_tick_with_now(&state, &session, &client, t0 + 5 * 86_400, true, Some(pw))
                .await
                .unwrap();
        assert_eq!(
            d5.anchors[0].active_rotation_phase.as_deref(),
            Some("proposed")
        );

        let _ = anchor_id;
        stop.store(true, Ordering::Relaxed);
        let _ = h2.await;
        let _ = h3.await;
        let _ = h4.await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_guarantor_claim_bundle_roundtrip() {
        let dir_issuer = temp_dir("claim-issuer");
        let dir_guar = temp_dir("claim-guarantor");
        let dir_holder = temp_dir("claim-holder");
        let pw = "pw";

        let (mut svc_issuer, _) =
            test_utils::setup_service_with_profile(&dir_issuer, &ACTORS.alice, "Alice", pw);
        svc_issuer.unlock_session(pw, 600).unwrap();
        let (mut svc_guar, _) =
            test_utils::setup_service_with_profile(&dir_guar, &ACTORS.bob, "BobGuarantor", pw);
        svc_guar.unlock_session(pw, 600).unwrap();
        let (mut svc_holder, _) = test_utils::setup_service_with_profile(
            &dir_holder,
            &ACTORS.charlie,
            "CharlieHolder",
            pw,
        );
        svc_holder.unlock_session(pw, 600).unwrap();

        let issuer_id = svc_issuer
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let guar_id = svc_guar
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let holder_id = svc_holder
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let (std_def, _) = (*test_utils::MINUTO_STANDARD).clone();
        let minuto_toml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../voucher_standards/minuto_v1/standard.toml"
        ))
        .unwrap();
        let mut standards = std::collections::HashMap::new();
        standards.insert(std_def.immutable.identity.uuid.clone(), minuto_toml.clone());

        let anchor_id = [0xAAu8; 32];

        // Create and sign a guaranteed voucher (Minuto requires additional signatures -> let's use Freetaler with optional guarantor signature so 1 guarantor suffices cleanly!)
        let (ft_def, _) = (*test_utils::FREETALER_STANDARD).clone();
        let ft_toml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../voucher_standards/freetaler_v1/standard.toml"
        ))
        .unwrap();
        standards.insert(ft_def.immutable.identity.uuid.clone(), ft_toml.clone());

        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(issuer_id.clone()),
                ..Default::default()
            },
        );
        v_data.creator_profile.id = Some(issuer_id.clone());
        v_data.nominal_value.amount = "100.00".into();
        v_data.nominal_value.unit = ft_def.immutable.blueprint.unit.clone();
        v_data.layer2_anchor = Some(anchor_id);
        let voucher = svc_issuer
            .create_new_voucher(&ft_toml, v_data, Some(pw))
            .unwrap();
        let issuer_local_id =
            human_money_core::wallet::Wallet::calculate_local_instance_id(&voucher, &issuer_id)
                .unwrap();

        // Guarantor Bob signs
        let sig_req = svc_issuer
            .create_signing_request_bundle(
                &issuer_local_id,
                human_money_core::models::secure_container::ContainerConfig::TargetDid(
                    guar_id.clone(),
                    human_money_core::models::secure_container::PrivacyMode::TrialDecryption,
                ),
                None,
                Some(pw),
            )
            .unwrap();
        let opened_req = svc_guar
            .open_voucher_signing_request(&sig_req, Some(pw))
            .unwrap();
        let detached = svc_guar
            .create_detached_signature_response_bundle(
                &opened_req.voucher,
                "guarantor",
                false,
                human_money_core::models::secure_container::ContainerConfig::TargetDid(
                    issuer_id.clone(),
                    human_money_core::models::secure_container::PrivacyMode::TrialDecryption,
                ),
                Some(pw),
            )
            .unwrap();
        svc_issuer
            .process_and_attach_signature(&detached, &standards, None, Some(pw))
            .unwrap();

        // Issuer sends voucher to Holder
        let send_res = svc_issuer
            .create_transfer_bundle(
                human_money_core::wallet::types::MultiTransferRequest {
                    recipient_id: holder_id.clone(),
                    sources: vec![human_money_core::wallet::types::SourceTransfer {
                        local_instance_id: issuer_local_id,
                        amount_to_send: "100.00".into(),
                    }],
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: Some(false),
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &standards,
                None,
                Some(pw),
            )
            .unwrap();
        let recv_res = svc_holder
            .receive_bundle(&send_res.bundle_bytes, &standards, None, Some(pw), false)
            .unwrap();
        let holder_local_id = recv_res.involved_vouchers[0].clone();

        // Partial Tx_claim (40.00 of 100.00) directly at Guarantor Bob!
        let (claim_res, _) = svc_holder
            .create_guarantor_claim_bundle(
                &holder_local_id,
                &guar_id,
                "40.00",
                &ft_toml,
                None,
                Some("partial claim".into()),
                human_money_core::L2TransferMode::Offline,
                Some(pw),
            )
            .unwrap();
        let guar_recv = svc_guar
            .receive_bundle(&claim_res.bundle_bytes, &standards, None, Some(pw), false)
            .unwrap();
        assert_eq!(guar_recv.involved_vouchers_details[0].amount, "40.00");

        let _ = std::fs::remove_dir_all(dir_issuer);
        let _ = std::fs::remove_dir_all(dir_guar);
        let _ = std::fs::remove_dir_all(dir_holder);
    }

    fn make_probe_info(
        lock_hex: &str,
        drop_pub_hex: &str,
        admin_sk: &SigningKey,
        revocation: Option<SyncRevocationNotice>,
    ) -> sync_client::SyncServerInfo {
        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_lower = lock_hex.trim().to_ascii_lowercase();
        let write_u = "https://node.example/sync.php".to_string();
        let read_u = "https://node.example/d".to_string();
        let issued_at = 1_759_000_000u64;
        let del_msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex,
            &lock_lower,
            &write_u,
            &read_u,
            issued_at,
        );
        sync_client::SyncServerInfo {
            drop_pub: drop_pub_hex.to_string(),
            read_url: read_u.clone(),
            register_url: None,
            channels: vec![],
            revocation,
            lock: Some(sync_client::SyncLockInfoDto {
                lock_pub: lock_hex.to_string(),
                delegation: Some(sync_client::SyncLockDelegationDto {
                    admin_pub: admin_hex,
                    lock_pub: lock_lower,
                    write_url: write_u,
                    read_url: read_u,
                    issued_at,
                    sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
                }),
                max_retention_quarter: 8159,
                max_inodes: 1000,
                used_inodes: 10,
                inode_warning: false,
                pow_base_bits: 0,
                allow_pow_tickets: true,
                ticket_pow_bits: 16,
                selftest: None,
            }),
        }
    }

    fn make_freeze_export(
        anchor_id: [u8; 32],
        sk: &SigningKey,
        retain_q: u16,
        proposal_hash: [u8; 32],
        frozen_at: u64,
    ) -> SnapshotExport {
        let lock_pub = sk.verifying_key().to_bytes();
        let bucket = humoco_proto::derive_bucket(&anchor_id, &lock_pub);
        let mut snap = humoco_proto::FreezeSnapshot {
            anchor_id,
            bucket,
            from_rev: 1,
            retain_until_quarter: retain_q,
            write_seq: 0,
            bucket_head: humoco_proto::crypto::compute_bucket_init_head(&bucket, retain_q),
            state_hash: humoco_proto::compute_freeze_state_hash(&anchor_id, 1, &[], &[]).unwrap(),
            permanent: true,
            proposal_hash: Some(proposal_hash),
            frozen_at,
            expires_at: None,
            lock_pub,
            sig: [0u8; 64],
        };
        snap.sig = sk.sign(&snap.digest()).to_bytes();
        SnapshotExport {
            snapshot: snap,
            entries: vec![],
            conflicts: vec![],
        }
    }

    #[test]
    fn test_server_probe_classification_and_5d_hysteresis_state_machine() {
        let s1_sk = SigningKey::from_bytes(&[61u8; 32]);
        let s1_hex = hex::encode(s1_sk.verifying_key().as_bytes());
        let s2_sk = SigningKey::from_bytes(&[62u8; 32]);
        let _s2_hex = hex::encode(s2_sk.verifying_key().as_bytes());
        let wrong_hex = "ff".repeat(32);

        let now0 = 1_760_000_000u64;

        // 1. Classify probe info: healthy, mismatched lock_pub, missing lock, future/invalid/forged/active revocation
        let healthy_info = make_probe_info(&s1_hex.to_uppercase(), &s1_hex, &s1_sk, None);
        assert_eq!(
            classify_server_probe_info(&healthy_info, &s1_hex, &s1_hex, now0),
            ServerProbeOutcome::Healthy
        );

        let mismatched_info = make_probe_info(&wrong_hex, &s1_hex, &s1_sk, None);
        assert_eq!(
            classify_server_probe_info(&mismatched_info, &s1_hex, &s1_hex, now0),
            ServerProbeOutcome::Unhealthy
        );

        let mut no_lock_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, None);
        no_lock_info.lock = None;
        assert_eq!(
            classify_server_probe_info(&no_lock_info, &s1_hex, &s1_hex, now0),
            ServerProbeOutcome::Unhealthy
        );

        let future_rev =
            sync_client::sign_revocation_notice(&s1_sk, now0, now0 + 600, None, None).unwrap();
        let future_rev_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, Some(future_rev));
        assert_eq!(
            classify_server_probe_info(&future_rev_info, &s1_hex, &s1_hex, now0),
            ServerProbeOutcome::Healthy,
            "future revocation notice must not trigger early revocation"
        );

        let mut forged_rev =
            sync_client::sign_revocation_notice(&s2_sk, now0, now0, None, None).unwrap();
        forged_rev.drop_pub = s1_hex.clone();
        let forged_rev_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, Some(forged_rev));
        assert_eq!(
            classify_server_probe_info(&forged_rev_info, &s1_hex, &s1_hex, now0),
            ServerProbeOutcome::Healthy,
            "forged revocation notice must be ignored"
        );

        // H8 regression guard: attacker hijacks domain, advertises own drop_pub & self-signed revocation
        let s2_hex = hex::encode(s2_sk.verifying_key().as_bytes());
        let attacker_self_rev =
            sync_client::sign_revocation_notice(&s2_sk, now0, now0, Some("hijack".into()), None)
                .unwrap();
        let hijacked_info = make_probe_info(&s2_hex, &s2_hex, &s2_sk, Some(attacker_self_rev));
        assert_eq!(
            classify_server_probe_info(&hijacked_info, &s1_hex, &s1_hex, now0),
            ServerProbeOutcome::Unhealthy,
            "self-signed revocation under foreign drop_pub must never mark expected server as Revoked"
        );

        let active_rev =
            sync_client::sign_revocation_notice(&s1_sk, now0, now0, Some("eol".into()), None)
                .unwrap();
        let active_rev_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, Some(active_rev.clone()));
        assert_eq!(
            classify_server_probe_info(&active_rev_info, &s1_hex, &s1_hex, now0),
            ServerProbeOutcome::Revoked
        );

        // 2. Hysteresis state machine: day 0 -> day 5 - 1s -> recovery resets timer -> day 5 demotes
        let mut health = StoredAnchorHealth::new("aa".repeat(32));
        let mut unhealthy = HashSet::from([s1_hex.clone()]);
        let empty_revoked = HashSet::new();

        let demoted_t0 = apply_health_observations_and_collect_demotions(
            &mut health,
            &unhealthy,
            &empty_revoked,
            now0,
        );
        assert!(demoted_t0.is_empty());
        assert_eq!(health.unhealthy_since_by_lock_pub.get(&s1_hex), Some(&now0));

        // 4d 23h 59m 59s later -> still under 5d threshold, original `since` preserved
        let demoted_before_5d = apply_health_observations_and_collect_demotions(
            &mut health,
            &unhealthy,
            &empty_revoked,
            now0 + UNHEALTHY_HYSTERESIS_SECS - 1,
        );
        assert!(demoted_before_5d.is_empty());
        assert_eq!(health.unhealthy_since_by_lock_pub.get(&s1_hex), Some(&now0));

        // Server recovers for one tick -> unhealthy timestamp must clear!
        unhealthy.clear();
        let demoted_recovered = apply_health_observations_and_collect_demotions(
            &mut health,
            &unhealthy,
            &empty_revoked,
            now0 + UNHEALTHY_HYSTERESIS_SECS - 1,
        );
        assert!(demoted_recovered.is_empty());
        assert!(!health.unhealthy_since_by_lock_pub.contains_key(&s1_hex));

        // Fails again at t_fail2 -> new 5-day clock starts
        let t_fail2 = now0 + UNHEALTHY_HYSTERESIS_SECS;
        unhealthy.insert(s1_hex.clone());
        assert!(apply_health_observations_and_collect_demotions(
            &mut health,
            &unhealthy,
            &empty_revoked,
            t_fail2,
        )
        .is_empty());
        assert_eq!(
            health.unhealthy_since_by_lock_pub.get(&s1_hex),
            Some(&t_fail2)
        );

        // Exactly 5 days after t_fail2 -> demoted!
        let demoted_day5 = apply_health_observations_and_collect_demotions(
            &mut health,
            &unhealthy,
            &empty_revoked,
            t_fail2 + UNHEALTHY_HYSTERESIS_SECS,
        );
        assert!(demoted_day5.contains(&s1_sk.verifying_key().to_bytes()));

        // 3. Immediate revocation bypasses 5-day hysteresis via apply_revocation_notices_to_health
        let mut rev_health = StoredAnchorHealth::new("bb".repeat(32));
        let expected_servers = vec![ServerEntry {
            write_url: "https://s1.example/sync.php".into(),
            read_url: "https://s1.example/d".into(),
            admin_pub: s1_sk.verifying_key().to_bytes(),
            lock_pub: s1_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        }];
        let s2_wrong_notice =
            sync_client::sign_revocation_notice(&s2_sk, now0, now0, None, None).unwrap();
        apply_revocation_notices_to_health(
            &mut rev_health,
            &[
                (s1_sk.verifying_key().to_bytes(), s2_wrong_notice),
                (s1_sk.verifying_key().to_bytes(), active_rev),
            ],
            &expected_servers,
            &HashSet::new(),
            now0,
        );
        assert!(rev_health.revoked_lock_pubs.contains(&s1_hex));
        let demoted_rev = apply_health_observations_and_collect_demotions(
            &mut rev_health,
            &HashSet::new(),
            &HashSet::new(),
            now0,
        );
        assert!(demoted_rev.contains(&s1_sk.verifying_key().to_bytes()));
    }

    #[test]
    fn test_rotation_phase_state_machine_transitions_without_network() {
        let s1_sk = SigningKey::from_bytes(&[71u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[72u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[73u8; 32]);
        let s4_sk = SigningKey::from_bytes(&[74u8; 32]);
        let bad_sk = SigningKey::from_bytes(&[75u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[76u8; 32]);
        let g_mig_sk = SigningKey::from_bytes(&[77u8; 32]);

        let s3_standby = ServerEntry {
            write_url: "https://standby.gamma.example/sync.php".into(),
            read_url: "https://standby.gamma.example/d".into(),
            admin_pub: s3_sk.verifying_key().to_bytes(),
            lock_pub: s3_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            "https://srv1.alpha.example",
            "https://srv2.beta.example",
            Some(s3_standby.clone()),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_sk.verifying_key().to_bytes()],
        );
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let now0 = 1_760_000_100u64;

        // 1. select_rotation_proposal: empty failed set returns None
        let lifecycle = InternalLifecycleState::default();
        let offenders = ServerOffenderList::new();
        let empty_policy = SecurityPolicy::default();
        assert!(select_rotation_proposal(
            anchor_id,
            &manifest,
            &HashSet::new(),
            Some(&creator_mig_sk),
            None,
            &empty_policy,
            &lifecycle,
            &offenders,
            now0,
        )
        .is_none());

        // 2. select_rotation_proposal promotes HolderStandby when standby covers failed Creator seat
        let failed = HashSet::from([s1_sk.verifying_key().to_bytes()]);
        let standby_prop = select_rotation_proposal(
            anchor_id,
            &manifest,
            &failed,
            Some(&creator_mig_sk),
            None,
            &empty_policy,
            &lifecycle,
            &offenders,
            now0,
        )
        .expect("standby successor must be promoted");
        assert_eq!(standby_prop.initiator, ProposalInitiator::HolderStandby);
        assert_eq!(standby_prop.new_servers[0], s3_standby);

        // 3. select_rotation_proposal without standby falls back to l2_probe_cache for Creator seat
        let mut manifest_no_standby = manifest.clone();
        manifest_no_standby.successors.clear();
        let mut lifecycle_with_probe = InternalLifecycleState::default();
        let s4_hex = hex::encode(s4_sk.verifying_key().as_bytes());
        lifecycle_with_probe.l2_probe_cache.insert(
            s4_hex.clone(),
            sync_client::FrontendLockServerProbeResult {
                write_url: "https://srv4.delta.example/sync.php".into(),
                read_url: "https://srv4.delta.example/d".into(),
                drop_pub: s4_hex.clone(),
                admin_pub: s4_hex.clone(),
                lock_pub: s4_hex,
                delegation_valid: true,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 12,
                max_retention_quarter: manifest.retain_until_quarter,
                pow_base_bits: 0,
                allow_pow_tickets: true,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 1000,
                probed_at_secs: now0,
                delegation: None,
                error: None,
            },
        );
        let creator_prop = select_rotation_proposal(
            anchor_id,
            &manifest_no_standby,
            &failed,
            Some(&creator_mig_sk),
            None,
            &empty_policy,
            &lifecycle_with_probe,
            &offenders,
            now0,
        )
        .expect("creator probe cache fallback must select non-sybil replacement");
        assert_eq!(creator_prop.initiator, ProposalInitiator::Creator);
        assert_eq!(
            creator_prop.new_servers[0].lock_pub,
            s4_sk.verifying_key().to_bytes()
        );

        // 4. Phase 1 -> Proposed: requires >= threshold acknowledgements
        assert!(compute_proposal_phase(
            &manifest,
            standby_prop.clone(),
            &[Some(now0 + 100)],
            manifest.threshold,
            now0,
        )
        .is_none());
        let proposed_phase = compute_proposal_phase(
            &manifest,
            standby_prop.clone(),
            &[Some(now0 + 400), Some(now0 + 500)],
            manifest.threshold,
            now0,
        )
        .expect("threshold acks must enter Proposed phase");
        let StoredRotationPhase::Proposed {
            proposal: prop,
            effective_at_secs,
        } = proposed_phase
        else {
            panic!("expected Proposed phase");
        };
        assert_eq!(effective_at_secs, now0 + 500);

        // 5. Phase 2: Before timelock -> WaitTimelock
        let d_wait = evaluate_proposed_phase_transition(
            anchor_id,
            &manifest,
            prop.clone(),
            effective_at_secs,
            vec![],
            &[],
            0,
            effective_at_secs - 1,
        )
        .unwrap();
        assert!(matches!(d_wait, ProposedPhaseDecision::WaitTimelock(_)));

        // After timelock with only 1 valid export (plus duplicate & forged snapshot) -> AwaitSnapshots
        let prop_hash = prop.core_digest().unwrap();
        let exp1 = make_freeze_export(
            anchor_id,
            &s1_sk,
            manifest.retain_until_quarter,
            prop_hash,
            effective_at_secs,
        );
        let mut bad_exp = make_freeze_export(
            anchor_id,
            &bad_sk,
            manifest.retain_until_quarter,
            prop_hash,
            effective_at_secs,
        );
        bad_exp.snapshot.sig = [9u8; 64]; // invalid signature
        let d_await_snaps = evaluate_proposed_phase_transition(
            anchor_id,
            &manifest,
            prop.clone(),
            effective_at_secs,
            vec![exp1.clone(), exp1.clone(), bad_exp],
            &[],
            0,
            effective_at_secs,
        )
        .unwrap();
        assert!(matches!(
            d_await_snaps,
            ProposedPhaseDecision::AwaitSnapshots(_)
        ));

        // With 2 distinct valid exports -> ReadyToImport
        let exp2 = make_freeze_export(
            anchor_id,
            &s2_sk,
            manifest.retain_until_quarter,
            prop_hash,
            effective_at_secs,
        );
        let d_ready = evaluate_proposed_phase_transition(
            anchor_id,
            &manifest,
            prop.clone(),
            effective_at_secs,
            vec![exp1, exp2],
            &[],
            0,
            effective_at_secs,
        )
        .unwrap();
        let ProposedPhaseDecision::ReadyToImport {
            expected_state_hash,
            ..
        } = d_ready
        else {
            panic!("expected ReadyToImport");
        };

        // 6. Verify ImportAck state hash matching
        let mut ack_s3 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash: expected_state_hash,
            retain_until_quarter: manifest.retain_until_quarter,
            lock_pub: s3_sk.verifying_key().to_bytes(),
            acked_at: effective_at_secs + 1,
            sig: [0u8; 64],
        };
        ack_s3.sig = s3_sk.sign(&ack_s3.digest()).to_bytes();
        let mut ack_s2 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash: expected_state_hash,
            retain_until_quarter: manifest.retain_until_quarter,
            lock_pub: s2_sk.verifying_key().to_bytes(),
            acked_at: effective_at_secs + 1,
            sig: [0u8; 64],
        };
        ack_s2.sig = s2_sk.sign(&ack_s2.digest()).to_bytes();
        verify_collected_import_acks(
            &[ack_s3.clone(), ack_s2.clone()],
            &anchor_id,
            &prop,
            &expected_state_hash,
        )
        .unwrap();

        let mut bad_ack = ack_s3.clone();
        bad_ack.imported_state_hash = [0xAAu8; 32];
        assert!(verify_collected_import_acks(
            &[bad_ack, ack_s2.clone()],
            &anchor_id,
            &prop,
            &expected_state_hash,
        )
        .is_err());

        // 7. Phase 3: Commit sigs < threshold stays Imported; >= threshold finalizes Rev 2
        let c_digest =
            MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &expected_state_hash);
        let cs1 = CommitteeSignature {
            lock_pub: s1_sk.verifying_key().to_bytes(),
            committed_at: effective_at_secs + 2,
            sig: s1_sk.sign(&c_digest).to_bytes(),
        };
        let cs2 = CommitteeSignature {
            lock_pub: s2_sk.verifying_key().to_bytes(),
            committed_at: effective_at_secs + 2,
            sig: s2_sk.sign(&c_digest).to_bytes(),
        };

        let c_wait = evaluate_imported_commit_transition(
            &manifest,
            &anchor_id,
            prop.clone(),
            expected_state_hash,
            vec![ack_s3.clone(), ack_s2.clone()],
            vec![cs1.clone(), cs1.clone()], // duplicate sig counts once -> < threshold(2)
        )
        .unwrap();
        let CommitPhaseDecision::AwaitCommitSigs(imported_phase) = c_wait else {
            panic!("expected AwaitCommitSigs");
        };

        let c_done = evaluate_imported_commit_transition(
            &manifest,
            &anchor_id,
            prop,
            expected_state_hash,
            vec![ack_s3, ack_s2],
            vec![cs1, cs2],
        )
        .unwrap();
        let CommitPhaseDecision::Finalized(rev2_manifest) = c_done else {
            panic!("expected Finalized");
        };
        assert_eq!(rev2_manifest.revision, 2);

        // 8. StoredAnchorHealth transition updates on error vs pause vs abort vs completion
        let mut health = StoredAnchorHealth::new(hex::encode(anchor_id));
        health
            .unhealthy_since_by_lock_pub
            .insert(hex::encode(s1_sk.verifying_key().as_bytes()), now0);
        health
            .revoked_lock_pubs
            .insert(hex::encode(s2_sk.verifying_key().as_bytes()));
        apply_rotation_step_outcome(
            &mut health,
            Ok(&RotationStepOutcome::InProgress(Box::new(imported_phase))),
            now0 + 10,
        );
        assert!(matches!(
            health.active_rotation,
            Some(StoredRotationPhase::Imported { .. })
        ));
        assert_eq!(health.last_rotated_rev, 1);

        apply_rotation_step_outcome(&mut health, Err("temporary timeout"), now0 + 20);
        assert_eq!(health.last_error.as_deref(), Some("temporary timeout"));
        assert!(health.active_rotation.is_some());

        apply_rotation_step_outcome(
            &mut health,
            Ok(&RotationStepOutcome::Aborted {
                reason: "Rotation proposal was vetoed or cleared on committee".into(),
            }),
            now0 + 25,
        );
        assert!(health.active_rotation.is_none());
        assert_eq!(
            health.last_error.as_deref(),
            Some("Rotation proposal was vetoed or cleared on committee")
        );
        assert!(
            !health.unhealthy_since_by_lock_pub.is_empty(),
            "Aborted must preserve unhealthy_since_by_lock_pub"
        );
        assert!(
            !health.revoked_lock_pubs.is_empty(),
            "Aborted must preserve revoked_lock_pubs"
        );
        assert_eq!(
            health.last_rotated_rev, 1,
            "Aborted must preserve last_rotated_rev"
        );

        apply_rotation_step_outcome(
            &mut health,
            Ok(&RotationStepOutcome::Completed { new_revision: 2 }),
            now0 + 30,
        );
        assert!(health.active_rotation.is_none());
        assert!(health.last_error.is_none());
        assert!(health.unhealthy_since_by_lock_pub.is_empty());
        assert!(health.revoked_lock_pubs.is_empty());
        assert_eq!(health.last_rotated_rev, 2);
    }

    #[test]
    fn test_conflict_veto_checks_and_deduplication() {
        let s1_sk = SigningKey::from_bytes(&[81u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[82u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[83u8; 32]);
        let s_evil_sk = SigningKey::from_bytes(&[84u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[85u8; 32]);
        let g_mig_sk = SigningKey::from_bytes(&[86u8; 32]);

        let s3_standby_g0 = ServerEntry {
            write_url: "https://gstandby.gamma.example/sync.php".into(),
            read_url: "https://gstandby.gamma.example/d".into(),
            admin_pub: s3_sk.verifying_key().to_bytes(),
            lock_pub: s3_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Guarantor(0),
        };
        let manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            "https://srv1.alpha.example",
            "https://srv2.beta.example",
            Some(s3_standby_g0),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_sk.verifying_key().to_bytes()],
        );
        let anchor_id = manifest.rev1_anchor_id().unwrap();

        // 1. Authorized HolderStandby promotion of guarantor's own standby seat -> NO veto
        let ok_standby_prop = l2_rotation::build_holder_standby_proposal(
            anchor_id,
            &manifest,
            &[s2_sk.verifying_key().to_bytes()],
            1_760_000_100,
        )
        .unwrap();
        assert!(
            evaluate_polled_proposals_for_veto(&manifest, &[ok_standby_prop], 0).is_empty(),
            "authorized standby promotion of guarantor seat must not trigger veto"
        );

        // 2. Unauthorized proposal replacing guarantor seat with creator server (and reducing protection)
        let mut bad_prop = MigrationProposal {
            anchor_id,
            from_rev: 1,
            to_rev: 2,
            new_threshold: 2,
            new_servers: vec![
                manifest.servers[0].clone(),
                ServerEntry {
                    write_url: "https://evil.example/sync.php".into(),
                    read_url: "https://evil.example/d".into(),
                    admin_pub: s_evil_sk.verifying_key().to_bytes(),
                    lock_pub: s_evil_sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Creator,
                },
            ],
            new_successors: vec![],
            retain_until_quarter: manifest.retain_until_quarter,
            proposed_at: 1_760_000_200,
            initiator: ProposalInitiator::Creator,
            proposer_sig: None,
            guarantor_cosigs: vec![],
        };
        let digest = bad_prop.core_digest().unwrap();
        bad_prop.proposer_sig = Some(creator_mig_sk.sign(&digest).to_bytes());

        // Duplicate identical proposals polled across multiple servers deduplicate to 1 VetoDecision
        let decisions =
            evaluate_polled_proposals_for_veto(&manifest, &[bad_prop.clone(), bad_prop], 0);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].proposal_hash, digest);

        let mut lc = StoredL2LifecycleState::default();
        record_guarantor_veto_sent(&mut lc, decisions[0].reason);
        assert_eq!(lc.auto_vetoes_sent, 1);
        assert_eq!(lc.last_veto_reason.as_deref(), Some(decisions[0].reason));
    }

    fn make_signed_probe_result(
        admin_sk: &SigningKey,
        lock_sk: &SigningKey,
        write_url: &str,
        read_url: &str,
        issued_at: u64,
        max_retention_quarter: u16,
    ) -> FrontendLockServerProbeResult {
        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
        let del_msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex, &lock_hex, write_url, read_url, issued_at,
        );
        let del = sync_client::SyncLockDelegationDto {
            admin_pub: admin_hex.clone(),
            lock_pub: lock_hex.clone(),
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            issued_at,
            sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
        };
        FrontendLockServerProbeResult {
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            drop_pub: admin_hex.clone(),
            admin_pub: admin_hex,
            lock_pub: lock_hex,
            delegation_valid: true,
            waf_probe_passed: true,
            selftest_passed: true,
            key_leak_safe: true,
            passed: true,
            latency_ms: 5,
            max_retention_quarter,
            pow_base_bits: 16,
            allow_pow_tickets: false,
            ticket_pow_bits: 16,
            inode_warning: false,
            used_inodes: 0,
            max_inodes: 50_000,
            probed_at_secs: issued_at,
            delegation: Some(del),
            error: None,
        }
    }

    #[test]
    fn test_relocation_by_delegation_and_revocation_notice() {
        let admin_sk = SigningKey::from_bytes(&[71u8; 32]);
        let lock_sk = SigningKey::from_bytes(&[72u8; 32]);
        let retain_q = encode_quarter(2035, 4).unwrap();
        let entry = ServerEntry {
            write_url: "https://old.host.example/sync.php".into(),
            read_url: "https://old.host.example/d".into(),
            admin_pub: admin_sk.verifying_key().to_bytes(),
            lock_pub: lock_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let lock_hex = hex::encode(entry.lock_pub);
        let mut cache = BTreeMap::new();

        // 1. Initial probe at old URL, then relocate via newer signed 5-field delegation
        let p_old = make_signed_probe_result(
            &admin_sk,
            &lock_sk,
            "https://old.host.example/sync.php",
            "https://old.host.example/d",
            100,
            retain_q,
        );
        assert!(upsert_verified_lock_probe(&mut cache, p_old));

        let p_moved1 = make_signed_probe_result(
            &admin_sk,
            &lock_sk,
            "https://moved1.host.example/sync.php",
            "https://moved1.host.example/d",
            200,
            retain_q,
        );
        assert!(upsert_verified_lock_probe(&mut cache, p_moved1));
        assert_eq!(
            resolve_server_locator(&entry, &cache),
            (
                "https://moved1.host.example/sync.php".to_string(),
                "https://moved1.host.example/d".to_string()
            )
        );

        // Stale issued_at cannot overwrite newer delegation
        let p_stale = make_signed_probe_result(
            &admin_sk,
            &lock_sk,
            "https://stale.host.example/sync.php",
            "https://stale.host.example/d",
            150,
            retain_q,
        );
        assert!(!upsert_verified_lock_probe(&mut cache, p_stale));

        // 2. Relocate via signed SyncRevocationNotice pointing to same lock_pub at new URL
        let notice = sync_client::sign_revocation_notice(
            &admin_sk,
            250,
            300,
            Some("relocated".into()),
            Some(sync_client::SyncReplacementTarget {
                write_url: "https://moved2.host.example/sync.php".into(),
                read_url: "https://moved2.host.example/d".into(),
                drop_pub: hex::encode(admin_sk.verifying_key().as_bytes()),
            }),
        )
        .unwrap();
        let p_moved2 = make_signed_probe_result(
            &admin_sk,
            &lock_sk,
            "https://moved2.host.example/sync.php",
            "https://moved2.host.example/d",
            300,
            retain_q,
        );
        assert!(try_apply_relocation_from_notice(&mut cache, &entry, &notice, p_moved2).is_some());
        assert_eq!(
            resolve_server_locator(&entry, &cache),
            (
                "https://moved2.host.example/sync.php".to_string(),
                "https://moved2.host.example/d".to_string()
            )
        );

        let mut health = StoredAnchorHealth::new("aa".repeat(32));
        let demotions = apply_health_observations_and_collect_demotions(
            &mut health,
            &HashSet::new(),
            &HashSet::new(),
            350,
        );
        assert!(demotions.is_empty());
        assert!(!health.revoked_lock_pubs.contains(&lock_hex));
    }

    #[tokio::test]
    async fn test_auto_growth_n1_to_n3_and_stops_at_n3() {
        let dir = temp_dir("auto_growth_n1_n3");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();

        let s1_sk = SigningKey::from_bytes(&[101u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[102u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[103u8; 32]);
        let s4_sk = SigningKey::from_bytes(&[104u8; 32]);
        let anchor_nonce = [105u8; 32];
        let creator_mig_pub = svc
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();

        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p3 = l3.local_addr().unwrap().port();
        let l4 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p4 = l4.local_addr().unwrap().port();

        let retain_q = encode_quarter(2035, 4).unwrap();
        let now0 = 1_760_000_100u64;
        let w1 = format!("http://127.0.0.1:{}/sync.php", p1);
        let r1 = format!("http://127.0.0.1:{}/d", p1);
        let w2 = format!("http://127.0.0.1:{}/sync.php", p2);
        let r2 = format!("http://127.0.0.1:{}/d", p2);
        let w3 = format!("http://127.0.0.1:{}/sync.php", p3);
        let r3 = format!("http://127.0.0.1:{}/d", p3);
        let w4 = format!("http://127.0.0.1:{}/sync.php", p4);
        let r4 = format!("http://127.0.0.1:{}/d", p4);

        // Create initial N=1 healthy creator-only anchor with empty guarantor_mig_pubs
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: w1.clone(),
                read_url: r1.clone(),
                admin_pub: s1_sk.verifying_key().to_bytes(),
                lock_pub: s1_sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: now0 * 1000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let mut t1 = GenesisTicket {
            anchor_id,
            retain_until_quarter: retain_q,
            issued_at: now0,
            lock_pub: s1_sk.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        t1.sig = s1_sk.sign(&t1.digest()).to_bytes();
        svc.register_l2_anchor_template_with_nonce(
            manifest.clone(),
            vec![t1],
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let spawn_growth_mock = |listener: tokio::net::TcpListener,
                                 sk: SigningKey,
                                 stop_flag: Arc<AtomicBool>| {
            let lock_pub = sk.verifying_key().to_bytes();
            let lock_hex = hex::encode(lock_pub);
            let bucket = humoco_proto::derive_bucket(&anchor_id, &lock_pub);
            let vp: Arc<Mutex<Option<MigrationProposal>>> = Arc::new(Mutex::new(None));
            tokio::spawn(async move {
                loop {
                    let accept = tokio::time::timeout(
                        std::time::Duration::from_millis(40),
                        listener.accept(),
                    )
                    .await;
                    let Ok(Ok((mut stream, _))) = accept else {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                        continue;
                    };
                    let mut buf = [0u8; 32_768];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
                    let path = head
                        .lines()
                        .next()
                        .unwrap_or("")
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("");

                    let make_export = |prop_hash_opt: Option<[u8; 32]>| {
                        let mut s = humoco_proto::FreezeSnapshot {
                            anchor_id,
                            bucket,
                            from_rev: 1,
                            retain_until_quarter: retain_q,
                            write_seq: 0,
                            bucket_head: humoco_proto::crypto::compute_bucket_init_head(
                                &bucket, retain_q,
                            ),
                            state_hash: humoco_proto::compute_freeze_state_hash(
                                &anchor_id,
                                1,
                                &[],
                                &[],
                            )
                            .unwrap(),
                            permanent: true,
                            proposal_hash: prop_hash_opt,
                            frozen_at: now0,
                            expires_at: None,
                            lock_pub,
                            sig: [0u8; 64],
                        };
                        s.sig = sk.sign(&s.digest()).to_bytes();
                        SnapshotExport {
                            snapshot: s,
                            entries: vec![],
                            conflicts: vec![],
                        }
                    };

                    let resp_json: serde_json::Value = if path.contains("info") {
                        serde_json::json!({
                            "drop_pub": lock_hex,
                            "read_url": "http://127.0.0.1:9/d",
                            "lock": {
                                "lock_pub": lock_hex,
                                "max_retention_quarter": retain_q,
                                "pow_base_bits": 0,
                                "allow_pow_tickets": true,
                                "ticket_pow_bits": 16
                            }
                        })
                    } else if path.contains("lock_freeze") {
                        if head.starts_with("GET ") {
                            let p = vp.lock().unwrap().clone();
                            serde_json::json!({
                                "anchor_id": hex::encode(anchor_id),
                                "bucket": hex::encode(bucket),
                                "active_manifest": serde_json::Value::Null,
                                "last_creator_activity_secs": now0,
                                "bucket_freeze_state": {},
                                "voted_proposal": p,
                                "snapshot_export": serde_json::Value::Null
                            })
                        } else {
                            let val: serde_json::Value = serde_json::from_str(body).unwrap();
                            match val.get("action").and_then(|v| v.as_str()) {
                                Some("propose") => {
                                    let prop: MigrationProposal =
                                        serde_json::from_value(val["proposal"].clone()).unwrap();
                                    *vp.lock().unwrap() = Some(prop);
                                    // 0s timelock for Creator + empty guarantors
                                    serde_json::json!({
                                        "status": "proposed",
                                        "effective_at": now0
                                    })
                                }
                                Some("fetch_snapshot") => {
                                    let ph = vp
                                        .lock()
                                        .unwrap()
                                        .as_ref()
                                        .and_then(|p| p.core_digest().ok());
                                    let exp = make_export(ph);
                                    serde_json::json!({ "snapshot_export": exp })
                                }
                                _ => serde_json::json!({ "status": "ok" }),
                            }
                        }
                    } else if path.contains("lock_import") {
                        let req: serde_json::Value = serde_json::from_str(body).unwrap();
                        let prop: MigrationProposal =
                            serde_json::from_value(req["proposal"].clone()).unwrap();
                        let mut ack = ImportAck {
                            anchor_id,
                            proposal_hash: prop.core_digest().unwrap(),
                            to_rev: prop.to_rev,
                            imported_state_hash: humoco_proto::compute_import_union_state_hash(
                                &anchor_id,
                                prop.to_rev,
                                &BTreeMap::new(),
                            )
                            .unwrap(),
                            retain_until_quarter: retain_q,
                            lock_pub,
                            acked_at: now0,
                            sig: [0u8; 64],
                        };
                        ack.sig = sk.sign(&ack.digest()).to_bytes();
                        serde_json::json!({ "import_ack": ack })
                    } else if path.contains("lock_commit") {
                        let req: serde_json::Value = serde_json::from_str(body).unwrap();
                        if req.get("action").and_then(|v| v.as_str()) == Some("sign_commit") {
                            let prop: MigrationProposal =
                                serde_json::from_value(req["proposal"].clone()).unwrap();
                            let ish = decode_hex_fixed::<32>(
                                req["imported_state_hash"].as_str().unwrap(),
                            )
                            .unwrap();
                            let d = MigrationCert::commit_digest(
                                &anchor_id,
                                &prop.core_digest().unwrap(),
                                prop.to_rev,
                                &ish,
                            );
                            let cs = CommitteeSignature {
                                lock_pub,
                                committed_at: now0,
                                sig: sk.sign(&d).to_bytes(),
                            };
                            serde_json::json!({ "committee_signature": cs })
                        } else {
                            serde_json::json!({ "status": "finalized", "revision": 2 })
                        }
                    } else {
                        serde_json::json!({})
                    };

                    let json_str = resp_json.to_string();
                    let http = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            json_str.len(),
                            json_str
                        );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };

        let h1 = spawn_growth_mock(l1, s1_sk.clone(), Arc::clone(&stop));
        let h2 = spawn_growth_mock(l2, s2_sk.clone(), Arc::clone(&stop));
        let h3 = spawn_growth_mock(l3, s3_sk.clone(), Arc::clone(&stop));
        let h4 = spawn_growth_mock(l4, s4_sk.clone(), Arc::clone(&stop));

        let state = AppState::new(svc);
        {
            let mut s = state.lock_service().unwrap();
            let make_pub_sync =
                |sk: &SigningKey, w: &str, r: &str| crate::settings::StoredPublicSync {
                    write_url: w.to_string(),
                    read_url: r.to_string(),
                    drop_pub: hex::encode(sk.verifying_key().to_bytes()),
                    register_url: None,
                    access_pubkey: "11".repeat(32),
                    enabled: true,
                    authorized_channels: vec!["lock".to_string()],
                    status: "active".to_string(),
                    sunset_at: None,
                    revocation_reason: None,
                    replacement: None,
                };
            state
                .mutate_security_policy(&mut s, Some(pw), |pol| {
                    pol.public_syncs.push(make_pub_sync(&s1_sk, &w1, &r1));
                    pol.public_syncs.push(make_pub_sync(&s2_sk, &w2, &r2));
                    pol.public_syncs.push(make_pub_sync(&s3_sk, &w3, &r3));
                })
                .unwrap();
            let p1_res = make_signed_probe_result(&s1_sk, &s1_sk, &w1, &r1, now0, retain_q);
            let p2_res = make_signed_probe_result(&s2_sk, &s2_sk, &w2, &r2, now0, retain_q);
            let p3_res = make_signed_probe_result(&s3_sk, &s3_sk, &w3, &r3, now0, retain_q);
            // Unknown cache-only probe (not in public_syncs / market_selected_syncs / contacts / own)
            let unk_sk = SigningKey::from_bytes(&[99u8; 32]);
            let p_unk = make_signed_probe_result(
                &unk_sk,
                &unk_sk,
                "http://127.0.0.1:49999/sync.php",
                "http://127.0.0.1:49999/d",
                now0,
                retain_q,
            );
            state
                .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                    upsert_verified_lock_probe(&mut lc.l2_probe_cache, p1_res);
                    upsert_verified_lock_probe(&mut lc.l2_probe_cache, p2_res);
                    upsert_verified_lock_probe(&mut lc.l2_probe_cache, p3_res);
                    upsert_verified_lock_probe(&mut lc.l2_probe_cache, p_unk);
                })
                .unwrap();
        }
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };
        let client = sync_client::create_sync_probe_client();

        // Single tick rotates healthy N=1 creator-only anchor directly to N=3 (threshold=2, rev=2)
        let status1 = run_lifecycle_tick_with_now(&state, &session, &client, now0, true, Some(pw))
            .await
            .unwrap();
        assert_eq!(status1.anchors[0].revision, 2);
        assert!(status1.anchors[0].active_rotation_phase.is_none());
        let rev2_manifest = {
            let s = state.lock_service().unwrap();
            s.with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&hex::encode(anchor_id))
                    .cloned()
            })
            .unwrap()
            .unwrap()
        };
        assert_eq!(rev2_manifest.servers.len(), 3);
        assert_eq!(rev2_manifest.threshold, 2);

        // Add S4 and run another tick -> stops at N >= 3 without further rotation
        {
            let mut s = state.lock_service().unwrap();
            state
                .mutate_security_policy(&mut s, Some(pw), |pol| {
                    pol.public_syncs.push(crate::settings::StoredPublicSync {
                        write_url: w4.clone(),
                        read_url: r4.clone(),
                        drop_pub: hex::encode(s4_sk.verifying_key().to_bytes()),
                        register_url: None,
                        access_pubkey: "11".repeat(32),
                        enabled: true,
                        authorized_channels: vec!["lock".to_string()],
                        status: "active".to_string(),
                        sunset_at: None,
                        revocation_reason: None,
                        replacement: None,
                    });
                })
                .unwrap();
            let p4_res = make_signed_probe_result(&s4_sk, &s4_sk, &w4, &r4, now0 + 10, retain_q);
            state
                .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                    upsert_verified_lock_probe(&mut lc.l2_probe_cache, p4_res);
                })
                .unwrap();
        }
        let status2 =
            run_lifecycle_tick_with_now(&state, &session, &client, now0 + 100, true, Some(pw))
                .await
                .unwrap();
        assert_eq!(status2.anchors[0].revision, 2);
        assert!(status2.anchors[0].active_rotation_phase.is_none());
        let after_manifest = {
            let s = state.lock_service().unwrap();
            s.with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&hex::encode(anchor_id))
                    .cloned()
            })
            .unwrap()
            .unwrap()
        };
        assert_eq!(after_manifest.servers.len(), 3);

        stop.store(true, Ordering::Relaxed);
        let _ = h1.await;
        let _ = h2.await;
        let _ = h3.await;
        let _ = h4.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_h7_delta_probe_merge_preserves_concurrent_cache_entries() {
        let dir = temp_dir("h7_delta_merge");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();
        let state = AppState::new(svc);
        let session = {
            let s = state.service.lock().unwrap();
            state.session_token_for_service(&s).unwrap()
        };

        let s1_admin = SigningKey::from_bytes(&[31u8; 32]);
        let s1_lock = SigningKey::from_bytes(&[32u8; 32]);
        let s2_admin = SigningKey::from_bytes(&[33u8; 32]);
        let s2_lock = SigningKey::from_bytes(&[34u8; 32]);

        let p1 = make_signed_probe_result(
            &s1_admin,
            &s1_lock,
            "https://s1.example/sync.php",
            "https://s1.example/d",
            100,
            8159,
        );
        let p2 = make_signed_probe_result(
            &s2_admin,
            &s2_lock,
            "https://s2.example/sync.php",
            "https://s2.example/d",
            200,
            8159,
        );
        let p2_key = p2.lock_pub.clone();

        state
            .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
                upsert_verified_lock_probe(&mut lc.l2_probe_cache, p1.clone())
            })
            .unwrap();
        state
            .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
                upsert_verified_lock_probe(&mut lc.l2_probe_cache, p2.clone())
            })
            .unwrap();

        let p1_newer = make_signed_probe_result(
            &s1_admin,
            &s1_lock,
            "https://s1-new.example/sync.php",
            "https://s1-new.example/d",
            300,
            8159,
        );
        state
            .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
                upsert_verified_lock_probe(&mut lc.l2_probe_cache, p1_newer.clone())
            })
            .unwrap();

        let lc = {
            let mut s = state.service.lock().unwrap();
            state.load_internal_lifecycle(&mut s, Some(pw)).unwrap()
        };
        assert!(
            lc.l2_probe_cache.contains_key(&p2_key),
            "H7: concurrent cache entry S2 preserved"
        );
        assert_eq!(
            lc.l2_probe_cache.get(&p1.lock_pub).unwrap().write_url,
            "https://s1-new.example/sync.php"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_s05_f1_rotation_rejects_unverified_snapshot_and_bad_commit_sig() {
        let s1_sk = SigningKey::from_bytes(&[101u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[102u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[103u8; 32]);
        let non_member_sk = SigningKey::from_bytes(&[104u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[105u8; 32]);
        let g_mig_sk = SigningKey::from_bytes(&[106u8; 32]);

        let s3_standby = ServerEntry {
            write_url: "https://s3.gamma.example/sync.php".into(),
            read_url: "https://s3.gamma.example/d".into(),
            admin_pub: s3_sk.verifying_key().to_bytes(),
            lock_pub: s3_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            "https://s1.alpha.example",
            "https://s2.beta.example",
            Some(s3_standby),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_sk.verifying_key().to_bytes()],
        );
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let now0 = 1_760_100_000u64;
        let prop = l2_rotation::build_holder_standby_proposal(
            anchor_id,
            &manifest,
            &[s1_sk.verifying_key().to_bytes()],
            now0,
        )
        .unwrap();
        let prop_hash = prop.core_digest().unwrap();

        let exp1 = make_freeze_export(
            anchor_id,
            &s1_sk,
            manifest.retain_until_quarter,
            prop_hash,
            now0,
        );
        let exp2 = make_freeze_export(
            anchor_id,
            &s2_sk,
            manifest.retain_until_quarter,
            prop_hash,
            now0,
        );
        let exp_non_member = make_freeze_export(
            anchor_id,
            &non_member_sk,
            manifest.retain_until_quarter,
            prop_hash,
            now0,
        );

        // Non-member snapshot must not count toward threshold -> AwaitSnapshots (not error)
        let d_non_member = evaluate_proposed_phase_transition(
            anchor_id,
            &manifest,
            prop.clone(),
            now0,
            vec![exp1.clone(), exp_non_member],
            &[],
            0,
            now0,
        )
        .expect("non-member snapshot must be ignored and yield AwaitSnapshots");
        assert!(matches!(
            d_non_member,
            ProposedPhaseDecision::AwaitSnapshots(_)
        ));

        // Tampered entries (state_hash mismatch) before a valid export from the same server must be ignored
        let mut exp1_tampered = exp1.clone();
        exp1_tampered.entries.push("deadbeef".to_string());
        let d_recovered = evaluate_proposed_phase_transition(
            anchor_id,
            &manifest,
            prop.clone(),
            now0,
            vec![exp1_tampered, exp1, exp2],
            &[],
            0,
            now0,
        )
        .expect("tampered export must not shadow valid export for same lock_pub");
        let ProposedPhaseDecision::ReadyToImport {
            expected_state_hash,
            ..
        } = d_recovered
        else {
            panic!("expected ReadyToImport");
        };

        // Phase 3: bad commit_sig must be rejected before threshold counting & must not shadow valid sig
        let mut ack_s3 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash: expected_state_hash,
            retain_until_quarter: manifest.retain_until_quarter,
            lock_pub: s3_sk.verifying_key().to_bytes(),
            acked_at: now0 + 1,
            sig: [0u8; 64],
        };
        ack_s3.sig = s3_sk.sign(&ack_s3.digest()).to_bytes();
        let mut ack_s2 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash: expected_state_hash,
            retain_until_quarter: manifest.retain_until_quarter,
            lock_pub: s2_sk.verifying_key().to_bytes(),
            acked_at: now0 + 1,
            sig: [0u8; 64],
        };
        ack_s2.sig = s2_sk.sign(&ack_s2.digest()).to_bytes();

        let c_digest =
            MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &expected_state_hash);
        let cs1_bad = CommitteeSignature {
            lock_pub: s1_sk.verifying_key().to_bytes(),
            committed_at: now0 + 2,
            sig: [0xEEu8; 64],
        };
        let cs1_good = CommitteeSignature {
            lock_pub: s1_sk.verifying_key().to_bytes(),
            committed_at: now0 + 2,
            sig: s1_sk.sign(&c_digest).to_bytes(),
        };
        let cs2_good = CommitteeSignature {
            lock_pub: s2_sk.verifying_key().to_bytes(),
            committed_at: now0 + 2,
            sig: s2_sk.sign(&c_digest).to_bytes(),
        };

        // Only 1 valid sig + 1 forged sig -> AwaitCommitSigs (not Err)
        let c_wait = evaluate_imported_commit_transition(
            &manifest,
            &anchor_id,
            prop.clone(),
            expected_state_hash,
            vec![ack_s3.clone(), ack_s2.clone()],
            vec![cs1_bad.clone(), cs2_good.clone()],
        )
        .expect("forged commit_sig must be filtered -> AwaitCommitSigs");
        assert!(matches!(c_wait, CommitPhaseDecision::AwaitCommitSigs(_)));

        // Forged sig before valid sig for same lock_pub must not shadow valid sig
        let c_done = evaluate_imported_commit_transition(
            &manifest,
            &anchor_id,
            prop,
            expected_state_hash,
            vec![ack_s3, ack_s2],
            vec![cs1_bad, cs1_good, cs2_good],
        )
        .expect("forged sig must not block subsequent valid sig for same lock_pub");
        assert!(matches!(c_done, CommitPhaseDecision::Finalized(_)));
    }

    #[test]
    fn test_s05_f2_max_pow_ignores_unverified_probes_and_clamps_to_max() {
        let mut lc = InternalLifecycleState::default();
        let lp_unverified = [0x11u8; 32];
        let lp_verified = [0x22u8; 32];
        let lp_unver_hex = hex::encode(lp_unverified);
        let lp_ver_hex = hex::encode(lp_verified);

        lc.l2_probe_cache.insert(
            lp_unver_hex.clone(),
            sync_client::FrontendLockServerProbeResult {
                write_url: "https://evil.example/sync.php".into(),
                read_url: "https://evil.example/d".into(),
                drop_pub: lp_unver_hex.clone(),
                admin_pub: lp_unver_hex.clone(),
                lock_pub: lp_unver_hex,
                delegation_valid: false,
                waf_probe_passed: false,
                selftest_passed: false,
                key_leak_safe: false,
                passed: false,
                latency_ms: 5,
                max_retention_quarter: 8159,
                pow_base_bits: 24,
                allow_pow_tickets: false,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 1000,
                probed_at_secs: 100,
                delegation: None,
                error: None,
            },
        );
        assert_eq!(
            max_pow_base_bits_for_servers(&lc, [lp_unverified]),
            humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS,
            "unverified probe must not inflate pow_base_bits"
        );

        lc.l2_probe_cache.insert(
            lp_ver_hex.clone(),
            sync_client::FrontendLockServerProbeResult {
                write_url: "https://good.example/sync.php".into(),
                read_url: "https://good.example/d".into(),
                drop_pub: lp_ver_hex.clone(),
                admin_pub: lp_ver_hex.clone(),
                lock_pub: lp_ver_hex,
                delegation_valid: true,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 5,
                max_retention_quarter: 8159,
                pow_base_bits: 99,
                allow_pow_tickets: false,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 1000,
                probed_at_secs: 100,
                delegation: None,
                error: None,
            },
        );
        assert_eq!(
            max_pow_base_bits_for_servers(&lc, [lp_verified]),
            humoco_proto::crypto::MAX_LOCK_POW_BITS,
            "verified probe pow_base_bits must clamp to MAX_LOCK_POW_BITS"
        );
    }

    #[test]
    fn test_s05_f6_standby_and_probe_filtering_preserves_unsigned_successors() {
        let s1_sk = SigningKey::from_bytes(&[111u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[112u8; 32]);
        let s3_standby_sk = SigningKey::from_bytes(&[113u8; 32]);
        let g_succ_sk = SigningKey::from_bytes(&[114u8; 32]);
        let bad_probe_sk = SigningKey::from_bytes(&[115u8; 32]);
        let good_probe_sk = SigningKey::from_bytes(&[116u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[117u8; 32]);
        let g_mig_sk = SigningKey::from_bytes(&[118u8; 32]);

        let s3_standby = ServerEntry {
            write_url: "https://standby.gamma.example/sync.php".into(),
            read_url: "https://standby.gamma.example/d".into(),
            admin_pub: s3_standby_sk.verifying_key().to_bytes(),
            lock_pub: s3_standby_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let g_succ = ServerEntry {
            write_url: "https://gsucc.epsilon.example/sync.php".into(),
            read_url: "https://gsucc.epsilon.example/d".into(),
            admin_pub: g_succ_sk.verifying_key().to_bytes(),
            lock_pub: g_succ_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Guarantor(0),
        };
        let mut manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            "https://s1.alpha.example",
            "https://s2.beta.example",
            Some(s3_standby.clone()),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_sk.verifying_key().to_bytes()],
        );
        manifest.successors.push(g_succ.clone());
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let now0 = 1_760_200_000u64;
        let failed = HashSet::from([s1_sk.verifying_key().to_bytes()]);

        // (a) Revoked Creator standby MUST NOT be promoted by HolderStandby
        let mut offenders = ServerOffenderList::new();
        offenders.revoked_lock_pubs.insert(s3_standby.lock_pub);
        // Mark Guarantor's successor admin untrusted locally -> Creator rotation MUST STILL preserve g_succ in new_successors!
        let mut policy = SecurityPolicy::default();
        policy
            .lock_trust_by_admin_pub
            .insert(hex::encode(g_succ.admin_pub), false);

        // Add one probe with delegation_valid=false (must be ignored) and one valid probe
        let mut lc = InternalLifecycleState::default();
        let bad_hex = hex::encode(bad_probe_sk.verifying_key().as_bytes());
        let good_hex = hex::encode(good_probe_sk.verifying_key().as_bytes());
        lc.l2_probe_cache.insert(
            bad_hex.clone(),
            sync_client::FrontendLockServerProbeResult {
                write_url: "https://badprobe.zeta.example/sync.php".into(),
                read_url: "https://badprobe.zeta.example/d".into(),
                drop_pub: bad_hex.clone(),
                admin_pub: bad_hex.clone(),
                lock_pub: bad_hex,
                delegation_valid: false,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 1,
                max_retention_quarter: manifest.retain_until_quarter,
                pow_base_bits: 8,
                allow_pow_tickets: true,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 1000,
                probed_at_secs: now0,
                delegation: None,
                error: None,
            },
        );
        lc.l2_probe_cache.insert(
            good_hex.clone(),
            sync_client::FrontendLockServerProbeResult {
                write_url: "https://goodprobe.eta.example/sync.php".into(),
                read_url: "https://goodprobe.eta.example/d".into(),
                drop_pub: good_hex.clone(),
                admin_pub: good_hex.clone(),
                lock_pub: good_hex,
                delegation_valid: true,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 50,
                max_retention_quarter: manifest.retain_until_quarter,
                pow_base_bits: 8,
                allow_pow_tickets: true,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 1000,
                probed_at_secs: now0,
                delegation: None,
                error: None,
            },
        );

        let prop = select_rotation_proposal(
            anchor_id,
            &manifest,
            &failed,
            Some(&creator_mig_sk),
            None,
            &policy,
            &lc,
            &offenders,
            now0,
        )
        .expect("must fall back to Creator proposal while preserving guarantor's successor");
        assert_eq!(prop.initiator, ProposalInitiator::Creator);
        assert_eq!(
            prop.new_servers[0].lock_pub,
            good_probe_sk.verifying_key().to_bytes(),
            "must skip delegation_valid=false probe and banned standby"
        );
        assert!(
            prop.new_successors.iter().any(|s| s.same_identity(&g_succ)),
            "must preserve non-signing Guarantor successor in new_successors"
        );
        assert!(
            prop.verify_against_manifest(&anchor_id, &manifest).is_ok(),
            "resulting proposal must satisfy humoco-proto verify_against_manifest"
        );
    }

    #[test]
    fn test_s05_f13_rogue_server_cannot_shorten_proposal_timelock() {
        let s1_sk = SigningKey::from_bytes(&[121u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[122u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[123u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[124u8; 32]);
        let g_mig_sk = SigningKey::from_bytes(&[125u8; 32]);
        let s3_standby = ServerEntry {
            write_url: "https://s3.gamma.example/sync.php".into(),
            read_url: "https://s3.gamma.example/d".into(),
            admin_pub: s3_sk.verifying_key().to_bytes(),
            lock_pub: s3_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            "https://s1.alpha.example",
            "https://s2.beta.example",
            Some(s3_standby),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_sk.verifying_key().to_bytes()],
        );
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let now0 = 1_760_300_000u64;
        let prop = l2_rotation::build_holder_standby_proposal(
            anchor_id,
            &manifest,
            &[s1_sk.verifying_key().to_bytes()],
            now0,
        )
        .unwrap();

        // 1 rogue server claiming effective_at=now0+1 + 1 honest server claiming now0+500 -> must use 2nd order statistic (now0+500)
        let phase_quorum = compute_proposal_phase(
            &manifest,
            prop.clone(),
            &[Some(now0 + 1), Some(now0 + 500)],
            2,
            now0,
        )
        .unwrap();
        let StoredRotationPhase::Proposed {
            effective_at_secs, ..
        } = phase_quorum
        else {
            panic!("expected Proposed");
        };
        assert_eq!(effective_at_secs, now0 + 500);

        // Only 1 server returned effective_at (< threshold=2) -> must fall back to full timelock
        let phase_partial =
            compute_proposal_phase(&manifest, prop, &[None, Some(now0 + 1)], 2, now0).unwrap();
        let StoredRotationPhase::Proposed {
            effective_at_secs: eff_partial,
            ..
        } = phase_partial
        else {
            panic!("expected Proposed");
        };
        assert_eq!(
            eff_partial,
            now0 + humoco_proto::PROPOSAL_TIMELOCK_SECS,
            "single server effective_at must not shorten timelock below threshold"
        );
    }

    #[test]
    fn test_s05_f1_byzantine_export_filtered_out_and_import_ack_verified() {
        let s1_sk = SigningKey::from_bytes(&[131u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[132u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[133u8; 32]);
        let s4_sk = SigningKey::from_bytes(&[134u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[135u8; 32]);

        let retain_q = encode_quarter(2035, 4).unwrap();
        let make_srv = |sk: &SigningKey, host: &str| ServerEntry {
            write_url: format!("https://{host}.example/sync.php"),
            read_url: format!("https://{host}.example/d"),
            admin_pub: sk.verifying_key().to_bytes(),
            lock_pub: sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![
                make_srv(&s1_sk, "s1"),
                make_srv(&s2_sk, "s2"),
                make_srv(&s3_sk, "s3"),
            ],
            successors: vec![make_srv(&s4_sk, "s4")],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let now0 = 1_760_400_000u64;
        // Evicting Byzantine s3 via standby s4
        let prop = l2_rotation::build_holder_standby_proposal(
            anchor_id,
            &manifest,
            &[s3_sk.verifying_key().to_bytes()],
            now0,
        )
        .unwrap();
        let prop_hash = prop.core_digest().unwrap();

        let exp1 = make_freeze_export(anchor_id, &s1_sk, retain_q, prop_hash, now0);
        let exp2 = make_freeze_export(anchor_id, &s2_sk, retain_q, prop_hash, now0);

        // s3 validly signs its FreezeSnapshot, but injects a causally invalid depth-0 LockEntry
        let bogus_entry = LockEntry {
            slot: [0x99u8; 32],
            parent_slot: [0u8; 32],
            output_branch: 0,
            depth: 0,
            child_lock: [0x88u8; 32],
            sender_pubkey: [0x77u8; 32],
            receiver_pubkey_hash: [0x66u8; 32],
            change_pubkey_hash: None,
            causal_sig: [0x55u8; 64],
            root_mig_sig: Some([0x44u8; 64]),
            encrypted_payload: [0x33u8; 265],
            pow_nonce: 0,
        };
        let s3_pub = s3_sk.verifying_key().to_bytes();
        let s3_bucket = humoco_proto::derive_bucket(&anchor_id, &s3_pub);
        let mut s3_snap = humoco_proto::FreezeSnapshot {
            anchor_id,
            bucket: s3_bucket,
            from_rev: 1,
            retain_until_quarter: retain_q,
            write_seq: 1,
            bucket_head: [0x22u8; 32],
            state_hash: humoco_proto::compute_freeze_state_hash(
                &anchor_id,
                1,
                std::slice::from_ref(&bogus_entry),
                &[],
            )
            .unwrap(),
            permanent: true,
            proposal_hash: Some(prop_hash),
            frozen_at: now0,
            expires_at: None,
            lock_pub: s3_pub,
            sig: [0u8; 64],
        };
        s3_snap.sig = s3_sk.sign(&s3_snap.digest()).to_bytes();
        let exp3_byzantine = SnapshotExport {
            snapshot: s3_snap,
            entries: vec![bogus_entry.to_wire_hex().unwrap()],
            conflicts: vec![],
        };

        // 1 Byzantine server among 3 (threshold=2) MUST be filtered out so its own eviction proceeds
        let dec = evaluate_proposed_phase_transition(
            anchor_id,
            &manifest,
            prop.clone(),
            now0,
            vec![exp1, exp2, exp3_byzantine],
            &[],
            DEFAULT_LOCK_POW_BASE_BITS,
            now0,
        )
        .expect("Byzantine export must be filtered out when >= threshold honest exports remain");
        let ProposedPhaseDecision::ReadyToImport {
            verified_exports,
            expected_state_hash,
            ..
        } = dec
        else {
            panic!("expected ReadyToImport");
        };
        assert_eq!(verified_exports.len(), 2);
        assert!(verified_exports
            .iter()
            .all(|e| e.snapshot.lock_pub != s3_pub));

        // verify_single_import_ack / verify_collected_import_acks reject forged metadata & signatures
        let sign_ack = |sk: &SigningKey| {
            let mut a = ImportAck {
                anchor_id,
                proposal_hash: prop_hash,
                to_rev: prop.to_rev,
                imported_state_hash: expected_state_hash,
                retain_until_quarter: prop.retain_until_quarter,
                lock_pub: sk.verifying_key().to_bytes(),
                acked_at: now0 + 1,
                sig: [0u8; 64],
            };
            a.sig = sk.sign(&a.digest()).to_bytes();
            a
        };
        let a1 = sign_ack(&s1_sk);
        let a2 = sign_ack(&s2_sk);
        let a4 = sign_ack(&s4_sk);
        verify_collected_import_acks(
            &[a1.clone(), a2.clone(), a4.clone()],
            &anchor_id,
            &prop,
            &expected_state_hash,
        )
        .unwrap();

        // Tampered signature must fail
        let mut a4_bad_sig = a4.clone();
        a4_bad_sig.sig[0] ^= 0xFF;
        assert!(verify_single_import_ack(
            &a4_bad_sig,
            &s4_sk.verifying_key().to_bytes(),
            &anchor_id,
            &prop,
            &expected_state_hash,
        )
        .is_err());

        // Wrong lock_pub for expected server must fail
        assert!(verify_single_import_ack(
            &a1,
            &s4_sk.verifying_key().to_bytes(),
            &anchor_id,
            &prop,
            &expected_state_hash,
        )
        .is_err());

        // Missing server / duplicate server in collected set must fail
        assert!(verify_collected_import_acks(
            &[a1.clone(), a1, a2],
            &anchor_id,
            &prop,
            &expected_state_hash,
        )
        .is_err());
    }

    #[tokio::test]
    async fn test_s05_f3_vetoed_committee_aborts_preserving_health_and_active_manifest_completes() {
        let dir = temp_dir("s05_f3_veto_abort");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();

        let s1_sk = SigningKey::from_bytes(&[141u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[142u8; 32]);
        let s3_sk = SigningKey::from_bytes(&[143u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[144u8; 32]);
        let g_mig_sk = SigningKey::from_bytes(&[145u8; 32]);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://127.0.0.1:{}", addr.port());

        let s3_standby = ServerEntry {
            write_url: format!("{base_url}/s3/sync.php"),
            read_url: format!("{base_url}/s3/d"),
            admin_pub: s3_sk.verifying_key().to_bytes(),
            lock_pub: s3_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let manifest = make_test_manifest(
            &s1_sk,
            &s2_sk,
            &format!("{base_url}/s1"),
            &format!("{base_url}/s2"),
            Some(s3_standby),
            creator_mig_sk.verifying_key().to_bytes(),
            vec![g_mig_sk.verifying_key().to_bytes()],
        );
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let anchor_hex = hex::encode(anchor_id);
        let now0 = 1_760_500_000u64;
        let prop = l2_rotation::build_holder_standby_proposal(
            anchor_id,
            &manifest,
            &[s1_sk.verifying_key().to_bytes()],
            now0,
        )
        .unwrap();

        // Build a valid Rev 2 manifest for Phase B (active_manifest upgrade discovery)
        let prop_hash = prop.core_digest().unwrap();
        let exp1 = make_freeze_export(
            anchor_id,
            &s1_sk,
            manifest.retain_until_quarter,
            prop_hash,
            now0,
        );
        let exp2 = make_freeze_export(
            anchor_id,
            &s2_sk,
            manifest.retain_until_quarter,
            prop_hash,
            now0,
        );
        let resolved = resolve_import_union(
            &anchor_id,
            &manifest,
            &prop,
            &[exp1, exp2],
            &[],
            DEFAULT_LOCK_POW_BASE_BITS,
        )
        .unwrap();
        let mut ack_s3 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash: resolved.imported_state_hash,
            retain_until_quarter: manifest.retain_until_quarter,
            lock_pub: s3_sk.verifying_key().to_bytes(),
            acked_at: now0 + 1,
            sig: [0u8; 64],
        };
        ack_s3.sig = s3_sk.sign(&ack_s3.digest()).to_bytes();
        let mut ack_s2 = ImportAck {
            anchor_id,
            proposal_hash: prop_hash,
            to_rev: 2,
            imported_state_hash: resolved.imported_state_hash,
            retain_until_quarter: manifest.retain_until_quarter,
            lock_pub: s2_sk.verifying_key().to_bytes(),
            acked_at: now0 + 1,
            sig: [0u8; 64],
        };
        ack_s2.sig = s2_sk.sign(&ack_s2.digest()).to_bytes();
        let c_digest =
            MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &resolved.imported_state_hash);
        let cs1 = CommitteeSignature {
            lock_pub: s1_sk.verifying_key().to_bytes(),
            committed_at: now0 + 2,
            sig: s1_sk.sign(&c_digest).to_bytes(),
        };
        let cs2 = CommitteeSignature {
            lock_pub: s2_sk.verifying_key().to_bytes(),
            committed_at: now0 + 2,
            sig: s2_sk.sign(&c_digest).to_bytes(),
        };
        let (rev2_manifest, _) = l2_rotation::assemble_migrated_manifest(
            &manifest,
            &anchor_id,
            prop.clone(),
            resolved.imported_state_hash,
            vec![ack_s3, ack_s2],
            vec![cs1, cs2],
        )
        .unwrap();

        let tickets: Vec<GenesisTicket> = [&s1_sk, &s2_sk]
            .into_iter()
            .map(|sk| {
                let mut t = GenesisTicket {
                    anchor_id,
                    retain_until_quarter: manifest.retain_until_quarter,
                    issued_at: now0,
                    lock_pub: sk.verifying_key().to_bytes(),
                    sig: [0u8; 64],
                };
                t.sig = sk.sign(&t.digest()).to_bytes();
                t
            })
            .collect();
        svc.register_l2_anchor_template(manifest.clone(), tickets, Some(pw))
            .unwrap();

        let serve_rev2 = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let serve_rev2_c = serve_rev2.clone();
        let stop_c = stop.clone();
        let rev2_c = rev2_manifest.clone();
        let anchor_hex_c = anchor_hex.clone();

        let server = tokio::spawn(async move {
            while !stop_c.load(Ordering::Relaxed) {
                let Ok(Ok((mut stream, _))) =
                    tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                        .await
                else {
                    continue;
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("");
                let resp = if first_line.contains("lock_freeze") {
                    let body = if serve_rev2_c.load(Ordering::Relaxed) {
                        serde_json::json!({
                            "anchor_id": anchor_hex_c,
                            "bucket": "00".repeat(32),
                            "active_manifest": rev2_c,
                            "last_creator_activity_secs": now0,
                            "bucket_freeze_state": "active",
                            "voted_proposal": serde_json::Value::Null,
                            "snapshot_export": serde_json::Value::Null,
                        })
                    } else {
                        serde_json::json!({
                            "anchor_id": anchor_hex_c,
                            "bucket": "00".repeat(32),
                            "active_manifest": serde_json::Value::Null,
                            "last_creator_activity_secs": now0,
                            "bucket_freeze_state": "active",
                            "voted_proposal": serde_json::Value::Null,
                            "snapshot_export": serde_json::Value::Null,
                        })
                    };
                    let s = body.to_string();
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        s.len(),
                        s
                    )
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_string()
                };
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });

        let state = AppState::new(svc);
        let session = {
            let mut s = state.service.lock().unwrap();
            let mut lc = StoredL2LifecycleState::default();
            let mut health = StoredAnchorHealth::new(anchor_hex.clone());
            health
                .unhealthy_since_by_lock_pub
                .insert(hex::encode(s1_sk.verifying_key().as_bytes()), now0 - 10);
            health
                .revoked_lock_pubs
                .insert(hex::encode(s1_sk.verifying_key().as_bytes()));
            health.active_rotation = Some(StoredRotationPhase::Proposed {
                proposal: prop.clone(),
                effective_at_secs: now0,
            });
            lc.anchors.insert(anchor_hex.clone(), health);
            save_lifecycle_state(&mut s, &lc, Some(pw)).unwrap();
            state.session_token_for_service(&s).unwrap()
        };

        let client = sync_client::create_sync_probe_client();
        // Phase A: Committee reports "active" without proposal -> Aborted, preserving health/revocations
        let status_abort =
            run_lifecycle_tick_with_now(&state, &session, &client, now0 + 5, true, Some(pw))
                .await
                .unwrap();
        let a_abort = status_abort
            .anchors
            .iter()
            .find(|a| a.anchor_id == anchor_hex)
            .unwrap();
        assert!(a_abort.active_rotation_phase.is_none());
        assert!(a_abort
            .last_error
            .as_deref()
            .unwrap_or("")
            .contains("vetoed or cleared"));
        assert_eq!(a_abort.unhealthy_servers.len(), 1);
        assert_eq!(a_abort.revoked_servers.len(), 1);
        assert_eq!(a_abort.revision, 1);

        // Phase B: Re-arm Proposed, server returns verified Rev 2 active_manifest -> Completed atomically
        serve_rev2.store(true, Ordering::Relaxed);
        {
            let mut s = state.service.lock().unwrap();
            let mut lc = load_lifecycle_state(&mut s, Some(pw)).unwrap();
            lc.anchors.get_mut(&anchor_hex).unwrap().active_rotation =
                Some(StoredRotationPhase::Proposed {
                    proposal: prop,
                    effective_at_secs: now0,
                });
            save_lifecycle_state(&mut s, &lc, Some(pw)).unwrap();
        }
        let status_done =
            run_lifecycle_tick_with_now(&state, &session, &client, now0 + 10, true, Some(pw))
                .await
                .unwrap();
        let a_done = status_done
            .anchors
            .iter()
            .find(|a| a.anchor_id == anchor_hex)
            .unwrap();
        assert_eq!(a_done.revision, 2);
        assert!(a_done.active_rotation_phase.is_none());
        assert!(a_done.last_error.is_none());
        assert!(a_done.unhealthy_servers.is_empty());
        assert!(a_done.revoked_servers.is_empty());

        stop.store(true, Ordering::Relaxed);
        let _ = server.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_s05_f7_cashier_receive_only_blocked_from_background_tick() {
        let dir = temp_dir("s05_f7_receive_only");
        let pw = "pw-full";
        let cashier_pin = "pin-cashier-1234";
        let (mut svc, profile_info) =
            test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();
        svc.enable_secondary_auth(
            pw,
            cashier_pin,
            human_money_core::SecondaryScope::ReceiveOnly,
        )
        .unwrap();
        svc.logout();
        let granted = svc
            .login_with_secondary(
                &profile_info.folder_name,
                cashier_pin,
                true,
                "test-id".into(),
            )
            .unwrap();
        assert_eq!(granted, human_money_core::AccessMode::ReceiveOnly);
        svc.unlock_secondary_session(cashier_pin, 600).unwrap();

        let state = AppState::new(svc);
        // Background tick MUST skip ReceiveOnly session without advancing last_tick_secs
        maybe_run_background_lifecycle_tick(&state).await;
        let last_tick = {
            let mut s = state.service.lock().unwrap();
            load_lifecycle_state(&mut s, None).unwrap().last_tick_secs
        };
        assert_eq!(
            last_tick, 0,
            "S05-F7: ReceiveOnly session must not execute background lifecycle tick"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_s05_f9_in_place_delegation_refresh_validation() {
        let admin_sk = SigningKey::from_bytes(&[151u8; 32]);
        let lock_sk = SigningKey::from_bytes(&[152u8; 32]);
        let other_sk = SigningKey::from_bytes(&[153u8; 32]);
        let srv = ServerEntry {
            write_url: "https://old.example/sync.php".into(),
            read_url: "https://old.example/d".into(),
            admin_pub: admin_sk.verifying_key().to_bytes(),
            lock_pub: lock_sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        };
        let valid_probe = make_signed_probe_result(
            &admin_sk,
            &lock_sk,
            "https://new.example/sync.php",
            "https://new.example/d",
            200,
            8159,
        );
        let valid_del = valid_probe.delegation.clone().unwrap();
        let mut offenders = ServerOffenderList::new();

        assert!(validate_in_place_delegation_refresh(
            &srv,
            &valid_del,
            &valid_probe,
            &offenders
        ));

        // Failed probe rejected
        let mut failed_probe = valid_probe.clone();
        failed_probe.passed = false;
        assert!(!validate_in_place_delegation_refresh(
            &srv,
            &valid_del,
            &failed_probe,
            &offenders
        ));

        // Mismatched lock_pub rejected
        let mut wrong_lock_probe = valid_probe.clone();
        wrong_lock_probe.lock_pub = hex::encode(other_sk.verifying_key().as_bytes());
        assert!(!validate_in_place_delegation_refresh(
            &srv,
            &valid_del,
            &wrong_lock_probe,
            &offenders
        ));

        // Mismatched write_url rejected
        let mut wrong_url_probe = valid_probe.clone();
        wrong_url_probe.write_url = "https://attacker.example/sync.php".into();
        assert!(!validate_in_place_delegation_refresh(
            &srv,
            &valid_del,
            &wrong_url_probe,
            &offenders
        ));

        // Private/loopback read_url rejected
        let mut bad_read_del = valid_del.clone();
        bad_read_del.read_url = "http://169.254.169.254/latest".into();
        assert!(!validate_in_place_delegation_refresh(
            &srv,
            &bad_read_del,
            &valid_probe,
            &offenders
        ));

        // Revoked server on ServerOffenderList rejected
        offenders.revoked_lock_pubs.insert(srv.lock_pub);
        assert!(!validate_in_place_delegation_refresh(
            &srv,
            &valid_del,
            &valid_probe,
            &offenders
        ));
    }

    #[test]
    fn test_s05_f11_record_equivocation_and_refresh_cache_revokes_admin_pub_and_updates_cache() {
        let dir = temp_dir("s05_f11_equiv_delegation");
        let pw = "pw";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();

        let admin_sk = SigningKey::from_bytes(&[161u8; 32]);
        let lock_sk = SigningKey::from_bytes(&[162u8; 32]);
        let admin_pub = admin_sk.verifying_key().to_bytes();
        let lock_pub = lock_sk.verifying_key().to_bytes();

        let probe = make_signed_probe_result(
            &admin_sk,
            &lock_sk,
            "https://equiv.example/sync.php",
            "https://equiv.example/d",
            100,
            8159,
        );
        let mut probe_cache = BTreeMap::new();
        probe_cache.insert(probe.lock_pub.clone(), probe);

        let make_rcpt = |child: [u8; 32], eh: [u8; 32]| {
            let mut r = LockReceipt {
                kind: LockReceiptKind::FirstWrite,
                lock_pub,
                bucket: [1u8; 32],
                slot: [2u8; 32],
                child_lock: child,
                entry_hash: eh,
                retain_until_quarter: 8159,
                write_seq: 1,
                bucket_head: [3u8; 32],
                recipient_nonce: [4u8; 32],
                sig: [0u8; 64],
            };
            r.sig = lock_sk.sign(&r.digest()).to_bytes();
            r
        };
        let eq = EquivocationProof {
            receipt_a: make_rcpt([0xAAu8; 32], [0x11u8; 32]),
            receipt_b: make_rcpt([0xBBu8; 32], [0x22u8; 32]),
        };

        let state = AppState::new(svc);
        {
            let mut s = state.lock_service().unwrap();
            let newly = record_equivocation_and_refresh_cache(
                &state,
                &mut s,
                &probe_cache,
                &eq,
                1_760_000_000,
                Some(pw),
            )
            .unwrap();
            assert!(newly);

            // Lock session to verify ActiveProfileCaches holds the refreshed ServerOffenderList!
            s.lock_session();
            let cached = state
                .get_or_load_server_offender_list(&mut s, None)
                .unwrap();
            assert!(
                cached.is_lock_pub_revoked(&lock_pub),
                "S05-F11: lock_pub must be revoked in refreshed cache"
            );
            assert!(
                cached.is_admin_pub_revoked(&admin_pub),
                "S05-F11: ADR-10 admin_pub must be revoked via probe_cache delegation"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_s05_f13_quorum_creator_activity_resists_byzantine_outlier() {
        let now = 1_800_000_000u64;
        let baseline = 1_700_000_000u64;
        let t_old = 1_750_000_000u64;

        // 1 Byzantine server reporting u64::MAX among 3 (threshold=2) cannot mask inactivity
        assert_eq!(
            compute_quorum_creator_activity_secs(baseline, &[t_old, t_old, u64::MAX], 2, now),
            t_old
        );
        // 1 stale server reporting 0 among 3 (threshold=2) cannot forge premature inactivity
        assert_eq!(
            compute_quorum_creator_activity_secs(baseline, &[0, now - 10, now - 10], 2, now),
            now - 10
        );
        // Future timestamp clamped to <= now even when < threshold responses arrive
        assert_eq!(
            compute_quorum_creator_activity_secs(baseline, &[u64::MAX], 2, now),
            now
        );
        // Zero threshold must be panic-free
        assert_eq!(
            compute_quorum_creator_activity_secs(baseline, &[t_old], 0, now),
            t_old
        );
    }
}
