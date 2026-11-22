use super::*;
use std::collections::{BTreeMap, HashSet};

use ed25519_dalek::SigningKey;
use human_money_core::humoco_proto::{
    self,
    crypto::verify_ed25519_strict,
    decode_hex_fixed, resolve_import_union,
    types::{LockPubKey, QuorumCertificate},
    AnchorManifest, CommitteeSignature, ImportAck, LockEntry, MigrationCert, MigrationProposal,
    NominatedBy, ServerEntry, SnapshotExport,
};
use human_money_core::services::l2_rotation;
use humoco_wot::ServerOffenderList;

use crate::gateway_resolver::{sybil_fold_committee, SybilServerCandidate};
use crate::models::FrontendError;
use crate::services::common::{normalize_sync_url, resolve_server_locator};
use crate::settings::{is_lock_admin_trusted, InternalLifecycleState, SecurityPolicy};
use crate::sync_client::{self, FrontendLockServerProbeResult};

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
