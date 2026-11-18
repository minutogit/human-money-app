//! Sync-Lock committee lifecycle orchestration (`APP-071`, Concept §5.4–§5.7):
//! - Permanent `ServerOffenderList` persistence and equivocation recording (`WOT-013`).
//! - `KeySuccessorAttestation` storage and 4-stage long-term manifest discovery cascade (§5.6).
//! - Periodic server health checks with 5-day hysteresis (immediate on `SyncRevocationNotice` / `410`).
//! - Crash-resumable 3-phase committee rotation (`propose` -> `fetch_snapshot`/`import` -> `commit`/`finalize`).
//! - Automatic guarantor vetoing of unauthorized or protection-reducing proposals during timelock.
//! - Regular L1 `Tx_claim` at guarantors and L1 `DeadQuorumSwap` emergency exchange commands (§5.7).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ed25519_dalek::SigningKey;
use human_money_core::humoco_proto::{
    self, decode_hex_fixed, resolve_import_union,
    types::{EquivocationProof, LockPubKey, LockReceiptKind, QuorumCertificate},
    AnchorManifest, CommitteeSignature, ImportAck, MigrationProposal, NominatedBy, ServerEntry,
    SnapshotExport,
};
use human_money_core::services::{crypto::get_pubkey_from_user_id, l2_gateway, l2_rotation};
use humoco_market::CardStore;
use humoco_wot::{
    resolve_successor_chain, KeySuccessorAttestation, ServerOffenderList, ServerOffenderRecord,
    WotEquivocationProof, WotLockDelegation, WotLockReceipt, WotLockReceiptKind,
};
use serde::{Deserialize, Serialize};

use crate::gateway_resolver::{sybil_fold_committee, SybilServerCandidate};
use crate::models::{FrontendAddressBook, FrontendError};
use crate::services::common::max_pow_base_bits_for_servers;
use crate::settings::AppSettings;
use crate::state::{AppState, CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY};
use crate::sync_client::{self, SyncRevocationNotice};

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendApprovedSwapClaimDto {
    pub claimant_did: String,
    pub voucher_id: String,
    pub tip_t_id: String,
    pub approved_amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendRejectedSwapClaimDto {
    pub claimant_did: String,
    pub voucher_id: String,
    pub tip_t_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDeadQuorumAdjudicationDto {
    pub approved: Vec<FrontendApprovedSwapClaimDto>,
    pub rejected: Vec<FrontendRejectedSwapClaimDto>,
    pub double_spend_proofs_count: usize,
}

pub fn load_server_offender_list(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> ServerOffenderList {
    match service.load_encrypted_data(SERVER_OFFENDER_LIST_KEY, password) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => ServerOffenderList::new(),
    }
}

pub fn save_server_offender_list(
    service: &mut human_money_core::app_service::AppService,
    list: &ServerOffenderList,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(list).map_err(|e| lifecycle_err(e.to_string()))?;
    service
        .save_encrypted_data(SERVER_OFFENDER_LIST_KEY, &bytes, password)
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

pub fn record_equivocation_proof(
    service: &mut human_money_core::app_service::AppService,
    delegation_opt: Option<WotLockDelegation>,
    proof: &EquivocationProof,
    now_secs: u64,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    let wot_proof = WotEquivocationProof {
        receipt_a: to_wot_receipt(&proof.receipt_a),
        receipt_b: to_wot_receipt(&proof.receipt_b),
    };
    let offending_lock = proof.receipt_a.lock_pub;
    let mut list = load_server_offender_list(service, password);
    let already = list.is_lock_pub_revoked(&offending_lock);
    let record = ServerOffenderRecord {
        proof: wot_proof,
        delegation: delegation_opt,
        recorded_at: now_secs.max(1),
    };
    list.insert_verified(record, now_secs.max(1))
        .map_err(|e| lifecycle_err(format!("Invalid equivocation proof: {}", e)))?;
    save_server_offender_list(service, &list, password)?;
    Ok(!already)
}

pub fn load_key_successor_attestations(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Vec<KeySuccessorAttestation> {
    match service.load_encrypted_data(KEY_SUCCESSOR_STORE_KEY, password) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_key_successor_attestations(
    service: &mut human_money_core::app_service::AppService,
    list: &[KeySuccessorAttestation],
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(list).map_err(|e| lifecycle_err(e.to_string()))?;
    service
        .save_encrypted_data(KEY_SUCCESSOR_STORE_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

pub(crate) fn strip_user_id_prefix(id: &str) -> &str {
    let trimmed = id.trim();
    trimmed.split_once('@').map_or(trimmed, |(_, did)| did)
}

pub fn resolve_current_did(
    service: &mut human_money_core::app_service::AppService,
    did: &str,
    now_secs: u64,
    password: Option<&str>,
) -> String {
    let attestations = load_key_successor_attestations(service, password);
    let canonical = strip_user_id_prefix(did);
    resolve_successor_chain(canonical, &attestations, now_secs)
        .unwrap_or_else(|_| canonical.to_string())
}

pub fn load_lifecycle_state(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> StoredL2LifecycleState {
    match service.load_encrypted_data(L2_LIFECYCLE_STATE_KEY, password) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => StoredL2LifecycleState::default(),
    }
}

pub fn save_lifecycle_state(
    service: &mut human_money_core::app_service::AppService,
    lc_state: &StoredL2LifecycleState,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(lc_state).map_err(|e| lifecycle_err(e.to_string()))?;
    service
        .save_encrypted_data(L2_LIFECYCLE_STATE_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerProbeOutcome {
    Healthy,
    Unhealthy,
    Revoked,
}

/// Pure decision helper classifying a server's `/info` response into healthy, unhealthy, or revoked.
pub(crate) fn classify_server_probe_info(
    info: &sync_client::SyncServerInfo,
    expected_lock_hex: &str,
    now_secs: u64,
) -> ServerProbeOutcome {
    let revoked = info.revocation.as_ref().is_some_and(|rev| {
        sync_client::verify_revocation_notice(rev, &info.drop_pub).is_ok()
            && rev.effective_at <= now_secs
    });
    if revoked {
        return ServerProbeOutcome::Revoked;
    }
    let lock_matches = info
        .lock
        .as_ref()
        .is_some_and(|l| l.lock_pub.trim().eq_ignore_ascii_case(expected_lock_hex));
    if lock_matches {
        ServerProbeOutcome::Healthy
    } else {
        ServerProbeOutcome::Unhealthy
    }
}

/// Pure decision helper applying verified effective `SyncRevocationNotice` entries to anchor health.
pub(crate) fn apply_revocation_notices_to_health(
    health: &mut StoredAnchorHealth,
    notices: &[(LockPubKey, SyncRevocationNotice)],
    expected_servers: &[ServerEntry],
    now_secs: u64,
) {
    for (lock_pub, notice) in notices {
        let Some(srv) = expected_servers.iter().find(|s| s.lock_pub == *lock_pub) else {
            continue;
        };
        let expected_admin = hex::encode(srv.admin_pub);
        if sync_client::verify_revocation_notice(notice, &expected_admin).is_ok()
            && notice.effective_at <= now_secs
        {
            health.revoked_lock_pubs.insert(hex::encode(lock_pub));
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
    now_secs: u64,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    if notices.is_empty() {
        return Ok(());
    }
    let anchor_hex = hex::encode(anchor_id);
    let mut lc = load_lifecycle_state(service, password);
    let health = lc
        .anchors
        .entry(anchor_hex.clone())
        .or_insert_with(|| StoredAnchorHealth::new(anchor_hex));
    apply_revocation_notices_to_health(health, notices, expected_servers, now_secs);
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
    if let Ok(pk) = get_pubkey_from_user_id(did) {
        let pk_bytes: [u8; 32] = pk.to_bytes();
        if let Some(stored_card) = card_store.cards.get(&pk_bytes) {
            for drop_desc in &stored_card.card.payload.drops {
                push_unique_url(out, seen, &drop_desc.read_url);
                push_unique_url(out, seen, &drop_desc.write_url);
            }
        }
    }
    if let Some(book) = address_book {
        let can_target = humoco_wot::canonicalize_did(strip_user_id_prefix(did)).ok();
        for contact in book.contacts.values() {
            let matches_did = contact.did == did
                || strip_user_id_prefix(&contact.did) == strip_user_id_prefix(did)
                || (can_target.is_some()
                    && humoco_wot::canonicalize_did(strip_user_id_prefix(&contact.did)).ok()
                        == can_target);
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
    settings: &AppSettings,
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
    ) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| lifecycle_err("Wallet service unavailable"))?;

        let local_manifest = service
            .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
            .map_err(FrontendError::from)?;
        let local_manifest = local_manifest.filter(|m| m.verify_anchor_id(anchor_id).is_ok());
        let local_rev = local_manifest.as_ref().map(|m| m.revision).unwrap_or(0);

        let card_store: CardStore = service
            .load_encrypted_data(MARKET_CARD_STORE_KEY, password)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let disk_contacts: Option<FrontendAddressBook> = service
            .load_encrypted_data(CONTACTS_DATA_NAME, password)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());

        let resolved_creator = creator_did.map(|c| {
            let resolved = resolve_current_did(&mut service, c, now_secs, password);
            (c.to_string(), resolved)
        });
        let resolved_guarantors: Vec<(String, String)> = guarantor_dids
            .iter()
            .map(|g| {
                (
                    g.clone(),
                    resolve_current_did(&mut service, g, now_secs, password),
                )
            })
            .collect();

        (
            local_manifest,
            local_rev,
            card_store,
            disk_contacts,
            resolved_creator,
            resolved_guarantors,
        )
    };

    let cached_contacts = state.contacts.lock().ok().and_then(|g| g.clone());
    let address_book_ref = cached_contacts.as_ref().or(address_book_opt.as_ref());

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
    for s in &settings.public_syncs {
        if s.enabled && s.status != "revoked" {
            push_unique_url(&mut stage_urls, &mut seen, &s.read_url);
            push_unique_url(&mut stage_urls, &mut seen, &s.write_url);
        }
    }
    for (url, probe) in &settings.l2_probe_cache {
        if probe.passed {
            push_unique_url(&mut stage_urls, &mut seen, &probe.read_url);
            push_unique_url(&mut stage_urls, &mut seen, url);
        }
    }
    if let Some(ref own) = settings.own_nominated_lock_server {
        push_unique_url(&mut stage_urls, &mut seen, &own.read_url);
        push_unique_url(&mut stage_urls, &mut seen, &own.write_url);
    }
    if let Some(ref m) = best_manifest {
        for srv in &m.servers {
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
            let mut service = state
                .service
                .lock()
                .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
            let _ = service.update_l2_anchor_manifest(found.clone(), password);
        }
    }

    Ok(best_manifest)
}

/// Mirrors a verified `revision >= 2` `AnchorManifest` onto own Sync servers for Guarantor/Creator Anchor Mirroring (§5.6 Stage 3).
pub async fn mirror_manifest_to_own_syncs(
    client: &reqwest::Client,
    settings: &AppSettings,
    anchor_id: &[u8; 32],
    manifest: &AnchorManifest,
) {
    if manifest.revision < 2 {
        return;
    }
    let mut urls = HashSet::new();
    for s in &settings.public_syncs {
        if s.enabled && s.status != "revoked" {
            urls.insert(s.write_url.clone());
        }
    }
    if let Some(ref own) = settings.own_nominated_lock_server {
        urls.insert(own.write_url.clone());
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
    settings: &AppSettings,
    offenders: &ServerOffenderList,
    now_secs: u64,
) -> Option<MigrationProposal> {
    if failed_lock_pubs.is_empty() {
        return None;
    }

    // 1. Try authority-free HolderStandby promotion if standby pool covers all failed seats
    let failed_vec: Vec<LockPubKey> = failed_lock_pubs.iter().copied().collect();
    if let Ok(prop) =
        l2_rotation::build_holder_standby_proposal(anchor_id, manifest, &failed_vec, now_secs)
    {
        return Some(prop);
    }

    let mut remaining_successors = manifest.successors.clone();
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
        .map(|s| SybilServerCandidate {
            write_url: s.write_url.clone(),
            lock_pub: s.lock_pub,
            admin_pub: Some(s.admin_pub),
            asn: None,
        })
        .collect();

    for srv in &manifest.servers {
        if !failed_lock_pubs.contains(&srv.lock_pub) {
            new_servers.push(srv.clone());
            continue;
        }
        if let Some(pos) = remaining_successors
            .iter()
            .position(|c| c.nominated_by == srv.nominated_by)
        {
            let succ = remaining_successors.remove(pos);
            used_lock_pubs.insert(succ.lock_pub);
            new_servers.push(succ);
            continue;
        }

        let can_pick_probe = match srv.nominated_by {
            NominatedBy::Creator => creator_mig_sk.is_some(),
            NominatedBy::Guarantor(g_idx) => guarantor_auth
                .as_ref()
                .is_some_and(|(auth_idx, _, _)| *auth_idx == g_idx),
        };
        if can_pick_probe {
            let mut picked: Option<ServerEntry> = None;
            for (url, probe) in &settings.l2_probe_cache {
                if !probe.passed || probe.max_retention_quarter < manifest.retain_until_quarter {
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
                    write_url: url.clone(),
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

    if new_servers.is_empty() || new_servers == manifest.servers {
        return None;
    }

    let n = new_servers.len() as u8;
    let min_q = (n / 2) + 1;
    let new_threshold = manifest.threshold.min(n).max(min_q);

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

async fn check_guarantor_vetoes(
    state: &AppState,
    client: &reqwest::Client,
    now_secs: u64,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let roles_and_manifests: Vec<(String, AnchorManifest, u8, Vec<u8>)> = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
        let lc = load_lifecycle_state(&mut service, password);
        service
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
            .map_err(FrontendError::from)?
    };

    for (anchor_hex, manifest, g_idx, ctx_nonce) in roles_and_manifests {
        let Ok(anchor_id) = decode_hex_fixed::<32>(&anchor_hex) else {
            continue;
        };

        let mut polled_proposals: Vec<MigrationProposal> = Vec::new();
        for srv in &manifest.servers {
            if let Ok(Some(status)) =
                sync_client::get_lock_freeze_status(client, &srv.write_url, &anchor_hex).await
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
            let veto_res = {
                let service = state
                    .service
                    .lock()
                    .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
                service
                    .with_wallet_and_identity(|_, id| {
                        let g_sk = l2_rotation::derive_guarantor_mig_signing_key(id, &ctx_nonce)?;
                        l2_rotation::sign_migration_veto(
                            &g_sk,
                            anchor_id,
                            manifest.revision,
                            prop_hash,
                            now_secs.max(1),
                        )
                    })
                    .map_err(FrontendError::from)?
            };

            let Ok(veto) = veto_res else {
                continue;
            };
            let body = serde_json::json!({
                "action": "veto",
                "veto": veto,
            });
            let mut sent_any = false;
            for srv in &manifest.servers {
                if sync_client::post_lock_freeze_action(client, &srv.write_url, &body)
                    .await
                    .is_ok()
                {
                    sent_any = true;
                }
            }
            if sent_any {
                let mut service = state
                    .service
                    .lock()
                    .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
                let mut lc = load_lifecycle_state(&mut service, password);
                record_guarantor_veto_sent(&mut lc, reason);
                save_lifecycle_state(&mut service, &lc, password)?;
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

pub(crate) fn is_proposal_timelock_elapsed(effective_at_secs: u64, now_secs: u64) -> bool {
    now_secs >= effective_at_secs
}

/// Pure Phase 1 -> Phase 2 state transition: enters `Proposed` only if `>= threshold` servers
/// acknowledged the proposal, picking the earliest `effective_at` capped at the protocol timelock.
pub(crate) fn compute_proposal_phase(
    proposal: MigrationProposal,
    accepted_effective_ats: &[Option<u64>],
    threshold: u8,
    now_secs: u64,
) -> Option<StoredRotationPhase> {
    if accepted_effective_ats.len() < usize::from(threshold) {
        return None;
    }
    let mut effective_at_secs = now_secs.saturating_add(humoco_proto::PROPOSAL_TIMELOCK_SECS);
    for eff in accepted_effective_ats.iter().flatten() {
        effective_at_secs = effective_at_secs.min(*eff);
    }
    Some(StoredRotationPhase::Proposed {
        proposal,
        effective_at_secs,
    })
}

/// Pure `Proposed` phase evaluator: checks timelock expiry, deduplicates and verifies
/// `FreezeSnapshot`s, enforces `>= manifest.threshold`, and computes the deterministic import union.
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

    let mut verified_exports: Vec<SnapshotExport> = Vec::new();
    let mut seen_pubs: HashSet<LockPubKey> = HashSet::new();
    for exp in candidate_exports {
        if seen_pubs.insert(exp.snapshot.lock_pub) && exp.snapshot.verify() {
            verified_exports.push(exp);
        }
    }

    if verified_exports.len() < usize::from(manifest.threshold) {
        return Ok(ProposedPhaseDecision::AwaitSnapshots(
            StoredRotationPhase::Proposed {
                proposal,
                effective_at_secs,
            },
        ));
    }

    let resolved = resolve_import_union(
        &anchor_id,
        manifest,
        &proposal,
        &verified_exports,
        qcs,
        pow_base_bits,
    )
    .map_err(|e| lifecycle_err(format!("resolve_import_union failed: {}", e)))?;

    Ok(ProposedPhaseDecision::ReadyToImport {
        proposal,
        verified_exports,
        expected_state_hash: resolved.imported_state_hash,
    })
}

pub(crate) fn verify_collected_import_acks(
    acks: &[ImportAck],
    expected_state_hash: &[u8; 32],
) -> Result<(), FrontendError> {
    for ack in acks {
        if ack.imported_state_hash != *expected_state_hash {
            return Err(lifecycle_err("Target server ImportAck state hash mismatch"));
        }
    }
    Ok(())
}

/// Pure `Imported` -> `Finalized` evaluator: deduplicates `commit_sigs` by `lock_pub`, stays in
/// `StoredRotationPhase::Imported` until `>= manifest.threshold`, and assembles the next manifest.
pub(crate) fn evaluate_imported_commit_transition(
    manifest: &AnchorManifest,
    anchor_id: &[u8; 32],
    proposal: MigrationProposal,
    imported_state_hash: [u8; 32],
    import_acks: Vec<ImportAck>,
    commit_sigs: Vec<CommitteeSignature>,
) -> Result<CommitPhaseDecision, FrontendError> {
    let mut unique_sigs: Vec<CommitteeSignature> = Vec::new();
    let mut signed_pubs: HashSet<LockPubKey> = HashSet::new();
    for cs in commit_sigs {
        if signed_pubs.insert(cs.lock_pub) {
            unique_sigs.push(cs);
        }
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
pub(crate) fn apply_rotation_step_outcome(
    health: &mut StoredAnchorHealth,
    outcome: Result<&Option<StoredRotationPhase>, &str>,
    current_manifest_rev: u64,
    now_secs: u64,
) {
    match outcome {
        Ok(next_phase) => {
            health.last_checked_secs = now_secs;
            health.active_rotation = next_phase.clone();
            health.last_error = None;
            if next_phase.is_none() {
                health.unhealthy_since_by_lock_pub.clear();
                health.revoked_lock_pubs.clear();
                health.last_rotated_rev = current_manifest_rev.saturating_add(1);
            }
        }
        Err(err_msg) => {
            health.last_error = Some(err_msg.to_string());
        }
    }
}

#[derive(Clone, Copy)]
struct RotationEnv<'a> {
    state: &'a AppState,
    settings: &'a AppSettings,
    client: &'a reqwest::Client,
    now_secs: u64,
    password: Option<&'a str>,
}

async fn drive_active_rotation(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    manifest: AnchorManifest,
    phase: StoredRotationPhase,
) -> Result<Option<StoredRotationPhase>, FrontendError> {
    let RotationEnv {
        state,
        settings,
        client,
        now_secs,
        password,
    } = env;
    let anchor_hex = hex::encode(anchor_id);

    let (proposal, maybe_imported): (MigrationProposal, Option<ImportedStatePair>) = match phase {
        StoredRotationPhase::Proposed {
            proposal,
            effective_at_secs,
        } => {
            if !is_proposal_timelock_elapsed(effective_at_secs, now_secs) {
                return Ok(Some(StoredRotationPhase::Proposed {
                    proposal,
                    effective_at_secs,
                }));
            }

            // Ensure permanent freeze & collect >= Q_old FreezeSnapshots
            let mut candidate_exports: Vec<SnapshotExport> = Vec::new();
            for srv in &manifest.servers {
                if let Ok(Some(st)) =
                    sync_client::get_lock_freeze_status(client, &srv.write_url, &anchor_hex).await
                {
                    if let Some(exp) = st.snapshot_export {
                        candidate_exports.push(exp);
                    }
                } else {
                    let fetch_body = serde_json::json!({
                        "action": "fetch_snapshot",
                        "anchor_id": anchor_hex,
                    });
                    if let Ok(val) =
                        sync_client::post_lock_freeze_action(client, &srv.write_url, &fetch_body)
                            .await
                    {
                        if let Some(exp_val) = val.get("snapshot_export").cloned() {
                            if let Ok(exp) = serde_json::from_value::<SnapshotExport>(exp_val) {
                                candidate_exports.push(exp);
                            }
                        }
                    }
                }
            }

            let qcs: Vec<QuorumCertificate> = {
                let service = state
                    .service
                    .lock()
                    .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
                service
                    .with_wallet(|w| {
                        w.profile
                            .l2_quorum_certificates
                            .values()
                            .flatten()
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .map_err(FrontendError::from)?
            };
            let pow_bits = max_pow_base_bits_for_servers(
                settings,
                manifest.servers.iter().map(|srv| srv.write_url.as_str()),
            );

            let (proposal, exports, expected_state_hash) = match evaluate_proposed_phase_transition(
                anchor_id,
                &manifest,
                proposal,
                effective_at_secs,
                candidate_exports,
                &qcs,
                pow_bits,
                now_secs,
            )? {
                ProposedPhaseDecision::WaitTimelock(p)
                | ProposedPhaseDecision::AwaitSnapshots(p) => return Ok(Some(p)),
                ProposedPhaseDecision::ReadyToImport {
                    proposal,
                    verified_exports,
                    expected_state_hash,
                } => (proposal, verified_exports, expected_state_hash),
            };

            let mut import_acks: Vec<ImportAck> = Vec::with_capacity(proposal.new_servers.len());
            for new_srv in &proposal.new_servers {
                let ack = sync_client::post_lock_import(
                    client,
                    &new_srv.write_url,
                    &anchor_id,
                    &manifest,
                    &proposal,
                    &exports,
                    &qcs,
                )
                .await
                .map_err(lifecycle_err)?;
                verify_collected_import_acks(std::slice::from_ref(&ack), &expected_state_hash)?;
                import_acks.push(ack);
            }

            (proposal, Some((expected_state_hash, import_acks)))
        }
        StoredRotationPhase::Imported {
            proposal,
            imported_state_hash_hex,
            import_acks,
        } => {
            let imported_state_hash =
                decode_hex_fixed::<32>(&imported_state_hash_hex).map_err(lifecycle_err)?;
            (proposal, Some((imported_state_hash, import_acks)))
        }
    };

    let Some((imported_state_hash, import_acks)) = maybe_imported else {
        unreachable!("handled in Proposed guard");
    };

    // Phase 3: Collect >= Q_old commit signatures
    let mut commit_sigs: Vec<CommitteeSignature> = Vec::new();
    for old_srv in &manifest.servers {
        if let Ok(cs) = sync_client::post_lock_commit_sign(
            client,
            &old_srv.write_url,
            &anchor_id,
            &proposal,
            &imported_state_hash,
            &import_acks,
        )
        .await
        {
            if !commit_sigs
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
        CommitPhaseDecision::AwaitCommitSigs(phase) => return Ok(Some(phase)),
        CommitPhaseDecision::Finalized(m) => m,
    };

    let mut final_urls = HashSet::new();
    for srv in manifest.servers.iter().chain(next_manifest.servers.iter()) {
        final_urls.insert(srv.write_url.clone());
    }
    for url in final_urls {
        let _ =
            sync_client::post_lock_commit_finalize(client, &url, &anchor_id, &next_manifest).await;
    }

    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
        service
            .update_l2_anchor_manifest(next_manifest.clone(), password)
            .map_err(FrontendError::from)?;
    }

    mirror_manifest_to_own_syncs(client, settings, &anchor_id, &next_manifest).await;
    Ok(None)
}

/// Executes one health check, guarantor-veto scan, and crash-resumable rotation step using `now_secs`.
pub(crate) async fn run_lifecycle_tick_with_now(
    state: &AppState,
    client: &reqwest::Client,
    now_secs: u64,
    force: bool,
    password: Option<&str>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    let settings = state.get_cached_settings().unwrap_or_default();

    // 1. Online Guarantor Automatic Veto check (§5.4)
    check_guarantor_vetoes(state, client, now_secs, password).await?;

    // 2. Collect tracked anchors
    let (anchors_to_check, offenders) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
        let mut lc = load_lifecycle_state(&mut service, password);
        if !force && now_secs.saturating_sub(lc.last_tick_secs) < HEALTH_CHECK_INTERVAL_SECS {
            return build_status_dto(&mut service, &lc, password);
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
        save_lifecycle_state(&mut service, &lc, password)?;
        let offenders = load_server_offender_list(&mut service, password);
        (anchors, offenders)
    };

    for (anchor_hex, manifest, g_role_opt, has_creator) in anchors_to_check {
        let Ok(anchor_id) = decode_hex_fixed::<32>(&anchor_hex) else {
            continue;
        };

        let existing_phase = {
            let mut service = state
                .service
                .lock()
                .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
            let lc = load_lifecycle_state(&mut service, password);
            lc.anchors
                .get(&anchor_hex)
                .and_then(|h| h.active_rotation.clone())
        };

        // Resume active crash-safe rotation phase if any
        if let Some(phase) = existing_phase {
            let step_result = drive_active_rotation(
                RotationEnv {
                    state,
                    settings: &settings,
                    client,
                    now_secs,
                    password,
                },
                anchor_id,
                manifest.clone(),
                phase,
            )
            .await;
            let mut service = state
                .service
                .lock()
                .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
            let mut lc = load_lifecycle_state(&mut service, password);
            let entry = lc
                .anchors
                .entry(anchor_hex.clone())
                .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.clone()));
            match &step_result {
                Ok(next_phase) => {
                    apply_rotation_step_outcome(entry, Ok(next_phase), manifest.revision, now_secs);
                }
                Err(err) => {
                    apply_rotation_step_outcome(
                        entry,
                        Err(err.message.as_str()),
                        manifest.revision,
                        now_secs,
                    );
                }
            }
            save_lifecycle_state(&mut service, &lc, password)?;
            continue;
        }

        // Probe health of every server in manifest.servers
        let mut unhealthy_now: HashSet<String> = HashSet::new();
        let mut revoked_now: HashSet<String> = HashSet::new();
        let mut last_creator_activity_secs = manifest.issued_at_ms / 1_000;

        for srv in &manifest.servers {
            let lock_hex = hex::encode(srv.lock_pub);
            if is_server_key_revoked(&offenders, &srv.lock_pub, Some(&srv.admin_pub)) {
                revoked_now.insert(lock_hex);
                continue;
            }
            match sync_client::fetch_sync_info(client, &srv.write_url).await {
                Ok(info) => match classify_server_probe_info(&info, &lock_hex, now_secs) {
                    ServerProbeOutcome::Healthy => {}
                    ServerProbeOutcome::Unhealthy => {
                        unhealthy_now.insert(lock_hex);
                    }
                    ServerProbeOutcome::Revoked => {
                        revoked_now.insert(lock_hex);
                    }
                },
                Err(_) => {
                    unhealthy_now.insert(lock_hex);
                }
            }

            if g_role_opt.is_some() {
                if let Ok(Some(st)) =
                    sync_client::get_lock_freeze_status(client, &srv.write_url, &anchor_hex).await
                {
                    if let Some(act) = st.last_creator_activity_secs {
                        last_creator_activity_secs = last_creator_activity_secs.max(act);
                    }
                }
            }
        }

        // Update hysteresis state and collect servers triggering rotation
        let failed_lock_pubs: HashSet<LockPubKey> = {
            let mut service = state
                .service
                .lock()
                .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
            let mut lc = load_lifecycle_state(&mut service, password);
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
            save_lifecycle_state(&mut service, &lc, password)?;
            failed
        };

        if failed_lock_pubs.is_empty() {
            continue;
        }

        // Derive signing authority keys if present in wallet
        let (creator_sk_opt, guarantor_sk_opt): (Option<SigningKey>, Option<(u8, SigningKey)>) = {
            let service = state
                .service
                .lock()
                .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
            service
                .with_wallet_and_identity(|w, id| {
                    let c_sk = if has_creator {
                        w.profile
                            .l2_anchor_nonces
                            .get(&anchor_hex)
                            .and_then(|h| hex::decode(h).ok())
                            .and_then(|b| l2_gateway::derive_anchor_mig_signing_key(id, &b).ok())
                    } else {
                        None
                    };
                    let g_sk = g_role_opt.as_ref().and_then(|r| {
                        let nb = hex::decode(&r.context_nonce_hex).ok()?;
                        let sk = l2_rotation::derive_guarantor_mig_signing_key(id, &nb).ok()?;
                        Some((r.guarantor_index, sk))
                    });
                    (c_sk, g_sk)
                })
                .map_err(FrontendError::from)?
        };

        let guarantor_tuple = guarantor_sk_opt
            .as_ref()
            .map(|(idx, sk)| (*idx, sk, last_creator_activity_secs));

        let Some(proposal) = select_rotation_proposal(
            anchor_id,
            &manifest,
            &failed_lock_pubs,
            creator_sk_opt.as_ref(),
            guarantor_tuple,
            &settings,
            &offenders,
            now_secs,
        ) else {
            continue;
        };

        // Phase 1: Propose on reachable old committee servers
        let prop_body = serde_json::json!({
            "action": "propose",
            "proposal": proposal,
            "current_manifest": manifest,
        });
        let mut accepted_effective_ats: Vec<Option<u64>> = Vec::new();
        for srv in &manifest.servers {
            if let Ok(resp) =
                sync_client::post_lock_freeze_action(client, &srv.write_url, &prop_body).await
            {
                accepted_effective_ats.push(resp.get("effective_at").and_then(|v| v.as_u64()));
            }
        }

        if let Some(phase) = compute_proposal_phase(
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
            {
                let mut service = state
                    .service
                    .lock()
                    .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
                let mut lc = load_lifecycle_state(&mut service, password);
                let health = lc
                    .anchors
                    .entry(anchor_hex.clone())
                    .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.clone()));
                health.active_rotation = Some(phase.clone());
                health.last_error = None;
                save_lifecycle_state(&mut service, &lc, password)?;
            }

            // If timelock is already elapsed (e.g., short dev/test timelock = 0), advance immediately
            if is_proposal_timelock_elapsed(phase_effective_at, now_secs) {
                if let Ok(next_phase) = drive_active_rotation(
                    RotationEnv {
                        state,
                        settings: &settings,
                        client,
                        now_secs,
                        password,
                    },
                    anchor_id,
                    manifest.clone(),
                    phase,
                )
                .await
                {
                    let mut service = state
                        .service
                        .lock()
                        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
                    let mut lc = load_lifecycle_state(&mut service, password);
                    let health = lc
                        .anchors
                        .entry(anchor_hex.clone())
                        .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.clone()));
                    apply_rotation_step_outcome(
                        health,
                        Ok(&next_phase),
                        manifest.revision,
                        now_secs,
                    );
                    save_lifecycle_state(&mut service, &lc, password)?;
                }
            }
        }
    }

    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
    let lc = load_lifecycle_state(&mut service, password);
    build_status_dto(&mut service, &lc, password)
}

pub(crate) fn build_status_dto(
    service: &mut human_money_core::app_service::AppService,
    lc: &StoredL2LifecycleState,
    password: Option<&str>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    let offenders = load_server_offender_list(service, password);
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
    let should_run = {
        let Ok(mut service) = state.service.lock() else {
            return;
        };
        if !service.is_wallet_unlocked() {
            return;
        }
        let lc = load_lifecycle_state(&mut service, None);
        let now_secs = sync_client::current_unix_timestamp();
        now_secs.saturating_sub(lc.last_tick_secs) >= HEALTH_CHECK_INTERVAL_SECS
    };
    if !should_run {
        return;
    }
    let client = sync_client::create_sync_probe_client();
    let now_secs = sync_client::current_unix_timestamp();
    if let Err(err) = run_lifecycle_tick_with_now(state, &client, now_secs, false, None).await {
        log::debug!(
            "Background L2 lifecycle tick skipped/failed: {}",
            err.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::{
        humoco_proto::{
            encode_quarter, types::LockReceipt, AnchorAuthority, GenesisTicket, MigrationCert,
            ProposalInitiator, DEAD_QUORUM_CLAIM_WINDOW_SECS,
        },
        services::{
            dead_quorum_swap::{
                adjudicate_dead_quorum_claims, solve_swap_ticket_pow, DeadQuorumClaimSubmission,
                DeadQuorumSwapOffer,
            },
            l2_gateway, l2_rotation,
        },
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

        let loaded = load_server_offender_list(&mut svc, Some(pw));
        assert!(loaded.is_lock_pub_revoked(&bad_pub));
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
                did: strip_user_id_prefix(&old_creator_did).to_string(),
                signing_key: ACTORS.alice.identity.signing_key.clone(),
            },
            &crate::state::DidWotSigner {
                did: strip_user_id_prefix(&new_creator_did).to_string(),
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
        let settings = AppSettings::default();
        let client = sync_client::create_sync_probe_client();

        let discovered = discover_anchor_manifest(
            &state,
            &settings,
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
                                "allow_pow_tickets": true
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
        let client = sync_client::create_sync_probe_client();

        // Trigger immediate rotation on SyncRevocationNotice from S1
        s1_revoked.store(true, Ordering::Relaxed);
        let status1 = run_lifecycle_tick_with_now(&state, &client, 1_760_000_050, true, Some(pw))
            .await
            .unwrap();
        assert_eq!(
            status1.anchors[0].active_rotation_phase.as_deref(),
            Some("proposed")
        );

        // Advance past timelock -> finishes Rev 2 automatically!
        let status2 =
            run_lifecycle_tick_with_now(&state, &client, effective_ts + 10, true, Some(pw))
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
        let mut lc = load_lifecycle_state(&mut svc, Some(pw));
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
                                "pow_base_bits": 0
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
        let client = sync_client::create_sync_probe_client();

        let dto = run_lifecycle_tick_with_now(&state, &client, 1_760_000_050, true, Some(pw))
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
                let lock_hex = hex::encode(sk.verifying_key().to_bytes());
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
                            "read_url": "http://127.0.0.1:9/d",
                            "revocation": null,
                            "lock": {
                                "lock_pub": lock_hex,
                                "max_retention_quarter": 8159,
                                "pow_base_bits": 0
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
        let client = sync_client::create_sync_probe_client();

        // Day 0: S1 marked unhealthy, no rotation triggered yet
        let t0 = 1_760_000_000u64;
        let d0 = run_lifecycle_tick_with_now(&state, &client, t0, true, Some(pw))
            .await
            .unwrap();
        assert_eq!(d0.anchors[0].unhealthy_servers.len(), 1);
        assert!(d0.anchors[0].active_rotation_phase.is_none());

        // Day 4: still < 5 days hysteresis -> MUST NOT propose rotation yet
        let d4 =
            run_lifecycle_tick_with_now(&state, &client, t0 + 4 * 86_400 + 3_600, true, Some(pw))
                .await
                .unwrap();
        assert!(d4.anchors[0].active_rotation_phase.is_none());

        // Day 5: >= 5 days elapsed -> proposes Phase 1 rotation!
        let d5 = run_lifecycle_tick_with_now(&state, &client, t0 + 5 * 86_400, true, Some(pw))
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
    fn test_guarantor_claim_bundle_and_dead_quorum_swap_roundtrip() {
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

        let dead_anchor = [0xAAu8; 32];
        let repl_anchor = [0xBBu8; 32];

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
        v_data.layer2_anchor = Some(dead_anchor);
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
            )
            .unwrap();
        let opened_v = svc_guar
            .open_voucher_signing_request(&sig_req, Some(pw))
            .unwrap();
        let detached = svc_guar
            .create_detached_signature_response_bundle(
                &opened_v,
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
            .process_and_attach_signature(&detached, &ft_toml, None, Some(pw))
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

        // 1. Holder submits DeadQuorumSwapOffer/Claim BEFORE claiming -> verify offer + claim pow!
        let offer = svc_issuer
            .with_wallet_and_identity(|_, id| {
                DeadQuorumSwapOffer::create_and_sign(
                    id,
                    dead_anchor,
                    repl_anchor,
                    1_760_000_000,
                    DEAD_QUORUM_CLAIM_WINDOW_SECS,
                    4,
                )
            })
            .unwrap()
            .unwrap();
        let v_for_claim = svc_holder
            .with_wallet(|w| {
                w.get_voucher_instance(&holder_local_id)
                    .unwrap()
                    .voucher
                    .clone()
            })
            .unwrap();
        let tip_id = v_for_claim.transactions.last().unwrap().t_id.clone();
        let nonce = solve_swap_ticket_pow(
            &offer.verify().unwrap(),
            &v_for_claim.voucher_id,
            &tip_id,
            &holder_id,
            4,
            0,
            100_000,
        )
        .unwrap();
        let sub = DeadQuorumClaimSubmission {
            voucher: v_for_claim,
            claimant_did: holder_id.clone(),
            quorum_certificates: vec![],
            swap_pow_nonce: nonce,
        };
        let adj = svc_issuer
            .with_wallet_and_identity(|_, id| {
                adjudicate_dead_quorum_claims(
                    &offer,
                    None,
                    &ft_def,
                    &[sub],
                    id,
                    offer.claim_window_ends_at_secs + 1,
                )
            })
            .unwrap()
            .unwrap();
        assert_eq!(adj.approved.len(), 1);
        assert_eq!(adj.approved[0].approved_amount, "100.00");

        // 2. Partial Tx_claim (40.00 of 100.00) directly at Guarantor Bob!
        let claim_res = svc_holder
            .create_guarantor_claim_bundle(
                &holder_local_id,
                &guar_id,
                "40.00",
                &ft_toml,
                None,
                Some("partial claim".into()),
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
        revocation: Option<SyncRevocationNotice>,
    ) -> sync_client::SyncServerInfo {
        sync_client::SyncServerInfo {
            drop_pub: drop_pub_hex.to_string(),
            read_url: "https://node.example/d".to_string(),
            register_url: None,
            channels: vec![],
            revocation,
            lock: Some(sync_client::SyncLockInfoDto {
                lock_pub: lock_hex.to_string(),
                delegation: None,
                max_retention_quarter: 8159,
                max_inodes: 1000,
                used_inodes: 10,
                inode_warning: false,
                pow_base_bits: 0,
                allow_pow_tickets: true,
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

        // 1. Classify probe info: healthy, mismatched lock_pub, missing lock, future/invalid/active revocation
        let healthy_info = make_probe_info(&s1_hex.to_uppercase(), &s1_hex, None);
        assert_eq!(
            classify_server_probe_info(&healthy_info, &s1_hex, now0),
            ServerProbeOutcome::Healthy
        );

        let mismatched_info = make_probe_info(&wrong_hex, &s1_hex, None);
        assert_eq!(
            classify_server_probe_info(&mismatched_info, &s1_hex, now0),
            ServerProbeOutcome::Unhealthy
        );

        let mut no_lock_info = make_probe_info(&s1_hex, &s1_hex, None);
        no_lock_info.lock = None;
        assert_eq!(
            classify_server_probe_info(&no_lock_info, &s1_hex, now0),
            ServerProbeOutcome::Unhealthy
        );

        let future_rev =
            sync_client::sign_revocation_notice(&s1_sk, now0, now0 + 600, None, None).unwrap();
        let future_rev_info = make_probe_info(&s1_hex, &s1_hex, Some(future_rev));
        assert_eq!(
            classify_server_probe_info(&future_rev_info, &s1_hex, now0),
            ServerProbeOutcome::Healthy,
            "future revocation notice must not trigger early revocation"
        );

        let mut forged_rev =
            sync_client::sign_revocation_notice(&s2_sk, now0, now0, None, None).unwrap();
        forged_rev.drop_pub = s1_hex.clone();
        let forged_rev_info = make_probe_info(&s1_hex, &s1_hex, Some(forged_rev));
        assert_eq!(
            classify_server_probe_info(&forged_rev_info, &s1_hex, now0),
            ServerProbeOutcome::Healthy,
            "forged revocation notice must be ignored"
        );

        let active_rev =
            sync_client::sign_revocation_notice(&s1_sk, now0, now0, Some("eol".into()), None)
                .unwrap();
        let active_rev_info = make_probe_info(&s1_hex, &s1_hex, Some(active_rev.clone()));
        assert_eq!(
            classify_server_probe_info(&active_rev_info, &s1_hex, now0),
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
        let settings = AppSettings::default();
        let offenders = ServerOffenderList::new();
        assert!(select_rotation_proposal(
            anchor_id,
            &manifest,
            &HashSet::new(),
            Some(&creator_mig_sk),
            None,
            &settings,
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
            &settings,
            &offenders,
            now0,
        )
        .expect("standby successor must be promoted");
        assert_eq!(standby_prop.initiator, ProposalInitiator::HolderStandby);
        assert_eq!(standby_prop.new_servers[0], s3_standby);

        // 3. select_rotation_proposal without standby falls back to l2_probe_cache for Creator seat
        let mut manifest_no_standby = manifest.clone();
        manifest_no_standby.successors.clear();
        let mut settings_with_probe = AppSettings::default();
        settings_with_probe.l2_probe_cache.insert(
            "https://srv4.delta.example/sync.php".into(),
            sync_client::FrontendLockServerProbeResult {
                write_url: "https://srv4.delta.example/sync.php".into(),
                read_url: "https://srv4.delta.example/d".into(),
                drop_pub: hex::encode(s4_sk.verifying_key().as_bytes()),
                admin_pub: hex::encode(s4_sk.verifying_key().as_bytes()),
                lock_pub: hex::encode(s4_sk.verifying_key().as_bytes()),
                delegation_valid: true,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 12,
                max_retention_quarter: manifest.retain_until_quarter,
                pow_base_bits: 0,
                allow_pow_tickets: true,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 1000,
                probed_at_secs: now0,
                error: None,
            },
        );
        let creator_prop = select_rotation_proposal(
            anchor_id,
            &manifest_no_standby,
            &failed,
            Some(&creator_mig_sk),
            None,
            &settings_with_probe,
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
            standby_prop.clone(),
            &[Some(now0 + 100)],
            manifest.threshold,
            now0
        )
        .is_none());
        let proposed_phase = compute_proposal_phase(
            standby_prop.clone(),
            &[None, Some(now0 + 500)],
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
        verify_collected_import_acks(&[ack_s3.clone(), ack_s2.clone()], &expected_state_hash)
            .unwrap();

        let mut bad_ack = ack_s3.clone();
        bad_ack.imported_state_hash = [0xAAu8; 32];
        assert!(verify_collected_import_acks(&[bad_ack], &expected_state_hash).is_err());

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

        // 8. StoredAnchorHealth transition updates on error vs pause vs completion
        let mut health = StoredAnchorHealth::new(hex::encode(anchor_id));
        health
            .unhealthy_since_by_lock_pub
            .insert(hex::encode(s1_sk.verifying_key().as_bytes()), now0);
        apply_rotation_step_outcome(
            &mut health,
            Ok(&Some(imported_phase)),
            manifest.revision,
            now0 + 10,
        );
        assert!(matches!(
            health.active_rotation,
            Some(StoredRotationPhase::Imported { .. })
        ));
        assert_eq!(health.last_rotated_rev, 1);

        apply_rotation_step_outcome(
            &mut health,
            Err("temporary timeout"),
            manifest.revision,
            now0 + 20,
        );
        assert_eq!(health.last_error.as_deref(), Some("temporary timeout"));
        assert!(health.active_rotation.is_some());

        apply_rotation_step_outcome(&mut health, Ok(&None), manifest.revision, now0 + 30);
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
}
