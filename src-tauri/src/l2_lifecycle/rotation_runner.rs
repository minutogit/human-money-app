use super::discovery::{
    apply_health_observations_and_collect_demotions, classify_server_probe_info, ServerProbeOutcome,
};
use super::rotation_policy::{
    apply_rotation_step_outcome, compute_proposal_phase, compute_quorum_creator_activity_secs,
    evaluate_imported_commit_transition, evaluate_polled_proposals_for_veto,
    evaluate_proposed_phase_transition, is_proposal_timelock_elapsed, record_guarantor_veto_sent,
    select_auto_growth_proposal, select_rotation_proposal, validate_in_place_delegation_refresh,
    verify_collected_import_acks, verify_single_import_ack, CommitPhaseDecision,
    ProposedPhaseDecision, RotationStepOutcome, VetoDecision,
};
use super::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use ed25519_dalek::SigningKey;
use human_money_core::humoco_proto::{
    crypto::{verify_ed25519_strict, DEFAULT_LOCK_POW_BASE_BITS},
    decode_hex_fixed,
    types::{LockPubKey, QuorumCertificate},
    AnchorManifest, CommitteeSignature, ImportAck, MigrationCert, MigrationProposal,
    SnapshotExport,
};
use human_money_core::services::{l2_gateway, l2_rotation};
use humoco_wot::ServerOffenderList;

use crate::models::FrontendError;
use crate::services::common::{
    normalize_sync_url, resolve_server_locator, upsert_verified_lock_probe,
};
use crate::settings::{InternalLifecycleState, SecurityPolicy};
use crate::state::AppState;
use crate::sync_client::{self, FrontendLockServerProbeResult};

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

enum ProposedStepResult {
    EarlyOutcome(RotationStepOutcome),
    ReadyToCommit(Box<MigrationProposal>, ImportedStatePair),
}

async fn resolve_rotation_precheck_outcome(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    proposal_from_rev: u64,
    proposal_to_rev: u64,
) -> Result<Option<RotationStepOutcome>, FrontendError> {
    let RotationEnv {
        state,
        session,
        client,
        password,
        ..
    } = env;
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
            return Ok(Some(RotationStepOutcome::Completed {
                new_revision: discovered.revision,
            }));
        }
        if discovered.revision != proposal_from_rev {
            return Ok(Some(RotationStepOutcome::Aborted {
                reason: format!(
                    "Discovered manifest revision {} superseded proposal from_rev {}",
                    discovered.revision, proposal_from_rev
                ),
            }));
        }
    }

    Ok(None)
}

async fn drive_proposed_rotation_phase(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    anchor_hex: &str,
    manifest: &AnchorManifest,
    proposal: MigrationProposal,
    effective_at_secs: u64,
) -> Result<ProposedStepResult, FrontendError> {
    let RotationEnv {
        state,
        session,
        lifecycle,
        client,
        now_secs,
        password,
        ..
    } = env;

    if !is_proposal_timelock_elapsed(effective_at_secs, now_secs) {
        return Ok(ProposedStepResult::EarlyOutcome(
            RotationStepOutcome::InProgress(Box::new(StoredRotationPhase::Proposed {
                proposal,
                effective_at_secs,
            })),
        ));
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
            sync_client::get_lock_freeze_status(client, &write_url, anchor_hex).await
        {
            if let Some(m) = st.active_manifest {
                if m.revision > manifest.revision && m.verify_anchor_id(&anchor_id).is_ok() {
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
                                .entry(anchor_hex.to_string())
                                .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.to_string()));
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
                    return Ok(ProposedStepResult::EarlyOutcome(
                        RotationStepOutcome::Completed {
                            new_revision: new_rev,
                        },
                    ));
                }
            }

            let mut export_matches = false;
            if let Some(exp) = st.snapshot_export {
                if exp.snapshot.lock_pub == srv.lock_pub {
                    export_matches =
                        exp.snapshot.permanent && exp.snapshot.proposal_hash == Some(prop_hash);
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
        return Ok(ProposedStepResult::EarlyOutcome(
            RotationStepOutcome::Aborted {
                reason: "Rotation proposal was vetoed or cleared on committee".to_string(),
            },
        ));
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
                                    && inst.voucher.transactions.iter().any(|t| &t.t_id == *t_id)
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
        manifest,
        proposal,
        effective_at_secs,
        candidate_exports,
        &qcs,
        DEFAULT_LOCK_POW_BASE_BITS,
        now_secs,
    )? {
        ProposedPhaseDecision::WaitTimelock(p) | ProposedPhaseDecision::AwaitSnapshots(p) => {
            return Ok(ProposedStepResult::EarlyOutcome(
                RotationStepOutcome::InProgress(Box::new(p)),
            ))
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
            client, &write_url, &anchor_id, manifest, &proposal, &exports, &qcs,
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
    verify_collected_import_acks(&import_acks, &anchor_id, &proposal, &expected_state_hash)?;

    Ok(ProposedStepResult::ReadyToCommit(
        Box::new(proposal),
        (expected_state_hash, import_acks),
    ))
}

async fn drive_imported_rotation_phase(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    anchor_hex: &str,
    manifest: AnchorManifest,
    proposal: MigrationProposal,
    imported_state_hash: [u8; 32],
    import_acks: Vec<ImportAck>,
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
                .entry(anchor_hex.to_string())
                .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.to_string()));
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

async fn drive_active_rotation(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    manifest: AnchorManifest,
    phase: StoredRotationPhase,
) -> Result<RotationStepOutcome, FrontendError> {
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

    if let Some(outcome) =
        resolve_rotation_precheck_outcome(env, anchor_id, proposal_from_rev, proposal_to_rev)
            .await?
    {
        return Ok(outcome);
    }

    let (proposal, (imported_state_hash, import_acks)) = match phase {
        StoredRotationPhase::Proposed {
            proposal,
            effective_at_secs,
        } => {
            match drive_proposed_rotation_phase(
                env,
                anchor_id,
                &anchor_hex,
                &manifest,
                proposal,
                effective_at_secs,
            )
            .await?
            {
                ProposedStepResult::EarlyOutcome(outcome) => return Ok(outcome),
                ProposedStepResult::ReadyToCommit(proposal, imported) => (*proposal, imported),
            }
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
            (proposal, (imported_state_hash, import_acks))
        }
    };

    drive_imported_rotation_phase(
        env,
        anchor_id,
        &anchor_hex,
        manifest,
        proposal,
        imported_state_hash,
        import_acks,
    )
    .await
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

struct TickScanContext {
    anchors_to_check: Vec<(String, AnchorManifest, Option<StoredGuarantorRole>, bool)>,
    offenders: ServerOffenderList,
    known_source_prio_by_url: BTreeMap<String, u8>,
    own_write_url_norm: Option<String>,
    outflow_allowed: bool,
}

enum TickCollectOutcome {
    Early,
    Ready(TickScanContext),
}

struct AnchorHealthProbeReport {
    unhealthy_now: HashSet<String>,
    revoked_now: HashSet<String>,
    last_creator_activity_secs: u64,
}

fn collect_lifecycle_tick_anchors(
    state: &AppState,
    session: &crate::session::SessionToken,
    security_policy: &SecurityPolicy,
    now_secs: u64,
    force: bool,
    password: Option<&str>,
) -> Result<Option<TickScanContext>, FrontendError> {
    match state.with_session_required(
        session,
        "Wallet session changed during L2 lifecycle tick",
        |service| {
            let mut lc = load_lifecycle_state(service, password)?;
            if !force && now_secs.saturating_sub(lc.last_tick_secs) < HEALTH_CHECK_INTERVAL_SECS {
                return Ok(TickCollectOutcome::Early);
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
                security_policy,
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
            Ok(TickCollectOutcome::Ready(TickScanContext {
                anchors_to_check: anchors,
                offenders,
                known_source_prio_by_url,
                own_write_url_norm,
                outflow_allowed,
            }))
        },
    )? {
        TickCollectOutcome::Early => Ok(None),
        TickCollectOutcome::Ready(ctx) => Ok(Some(ctx)),
    }
}

async fn resume_anchor_active_rotation(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    anchor_hex: &str,
    manifest: AnchorManifest,
    phase: StoredRotationPhase,
) -> Result<(), FrontendError> {
    let step_result = drive_active_rotation(env, anchor_id, manifest, phase).await;
    env.state.with_session_required(
        env.session,
        "Wallet session changed after driving active L2 rotation",
        |service| {
            let mut lc = load_lifecycle_state(service, env.password)?;
            let entry = lc
                .anchors
                .entry(anchor_hex.to_string())
                .or_insert_with(|| StoredAnchorHealth::new(anchor_hex.to_string()));
            match &step_result {
                Ok(outcome) => {
                    apply_rotation_step_outcome(entry, Ok(outcome), env.now_secs);
                }
                Err(err) => {
                    apply_rotation_step_outcome(entry, Err(err.message.as_str()), env.now_secs);
                }
            }
            save_lifecycle_state(service, &lc, env.password)
        },
    )
}

#[allow(clippy::too_many_arguments)]
async fn probe_anchor_committee_health(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    internal_lc: &mut InternalLifecycleState,
    offenders: &ServerOffenderList,
    manifest: &AnchorManifest,
    anchor_hex: &str,
    is_guarantor: bool,
    now_secs: u64,
    password: Option<&str>,
) -> AnchorHealthProbeReport {
    let mut unhealthy_now: HashSet<String> = HashSet::new();
    let mut revoked_now: HashSet<String> = HashSet::new();
    let mut probe_cache_deltas: Vec<FrontendLockServerProbeResult> = Vec::new();
    let mut reported_creator_activities: Vec<u64> = Vec::new();

    for srv in &manifest.servers {
        let lock_hex = hex::encode(srv.lock_pub);
        if is_server_key_revoked(offenders, &srv.lock_pub, Some(&srv.admin_pub)) {
            revoked_now.insert(lock_hex);
            continue;
        }
        let (mut active_write_url, _) = resolve_server_locator(srv, &internal_lc.l2_probe_cache);
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
                        match classify_server_probe_info(&info, &lock_hex, &admin_hex, now_secs) {
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
                                            && sync_client::validate_sync_url(target_url).is_ok()
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
                                                offenders,
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

        if is_guarantor && is_healthy_now {
            if let Ok(Some(st)) =
                sync_client::get_lock_freeze_status(client, &active_write_url, anchor_hex).await
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

    AnchorHealthProbeReport {
        unhealthy_now,
        revoked_now,
        last_creator_activity_secs,
    }
}

type AnchorAuthorityKeys = (Option<SigningKey>, Option<(u8, SigningKey)>);

fn derive_anchor_authority_keys(
    state: &AppState,
    session: &crate::session::SessionToken,
    anchor_hex: &str,
    has_creator: bool,
    g_role_opt: Option<&StoredGuarantorRole>,
    password: Option<&str>,
) -> Result<AnchorAuthorityKeys, FrontendError> {
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
                                .get(anchor_hex)
                                .and_then(|h| hex::decode(h).ok())
                                .and_then(|b| {
                                    l2_gateway::derive_anchor_mig_signing_key(id, &b).ok()
                                })
                        } else {
                            None
                        };
                        let g_sk = g_role_opt.and_then(|r| {
                            let nb = hex::decode(&r.context_nonce_hex).ok()?;
                            let sk = l2_rotation::derive_guarantor_mig_signing_key(id, &nb).ok()?;
                            Some((r.guarantor_index, sk))
                        });
                        (c_sk, g_sk)
                    },
                )
                .map_err(FrontendError::from)
        },
    )
}

#[allow(clippy::too_many_arguments)]
async fn evaluate_and_initiate_anchor_rotation(
    env: RotationEnv<'_>,
    anchor_id: [u8; 32],
    anchor_hex: &str,
    manifest: &AnchorManifest,
    failed_lock_pubs: &HashSet<LockPubKey>,
    creator_sk_opt: Option<SigningKey>,
    guarantor_sk_opt: Option<(u8, SigningKey)>,
    last_creator_activity_secs: u64,
    ctx: &TickScanContext,
) -> Result<(), FrontendError> {
    if failed_lock_pubs.is_empty() {
        if manifest.authority.guarantor_mig_pubs.is_empty() && manifest.servers.len() < 3 {
            if let Some(ref c_sk) = creator_sk_opt {
                if let Some(growth_prop) = select_auto_growth_proposal(
                    anchor_id,
                    manifest,
                    c_sk,
                    env.security_policy,
                    env.lifecycle,
                    &ctx.offenders,
                    &ctx.known_source_prio_by_url,
                    ctx.own_write_url_norm.as_deref(),
                    env.now_secs,
                ) {
                    initiate_rotation_proposal(env, anchor_id, anchor_hex, manifest, growth_prop)
                        .await?;
                }
            }
        }
        return Ok(());
    }

    let guarantor_tuple = guarantor_sk_opt
        .as_ref()
        .map(|(idx, sk)| (*idx, sk, last_creator_activity_secs));

    if let Some(proposal) = select_rotation_proposal(
        anchor_id,
        manifest,
        failed_lock_pubs,
        creator_sk_opt.as_ref(),
        guarantor_tuple,
        env.security_policy,
        env.lifecycle,
        &ctx.offenders,
        env.now_secs,
    ) {
        initiate_rotation_proposal(env, anchor_id, anchor_hex, manifest, proposal).await?;
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
    let Some(ctx) = collect_lifecycle_tick_anchors(
        state,
        session,
        &security_policy,
        now_secs,
        force,
        password,
    )?
    else {
        return Ok(());
    };

    for (anchor_hex, manifest, g_role_opt, has_creator) in &ctx.anchors_to_check {
        let Ok(anchor_id) = decode_hex_fixed::<32>(anchor_hex) else {
            continue;
        };

        let existing_phase = state.with_session_required(
            session,
            "Wallet session changed during L2 lifecycle tick",
            |service| {
                let lc = load_lifecycle_state(service, password)?;
                Ok(lc
                    .anchors
                    .get(anchor_hex)
                    .and_then(|h| h.active_rotation.clone()))
            },
        )?;

        // Resume active crash-safe rotation phase if any
        if let Some(phase) = existing_phase {
            resume_anchor_active_rotation(
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
                anchor_hex,
                manifest.clone(),
                phase,
            )
            .await?;
            continue;
        }

        // Probe health of every server in manifest.servers (relocation-aware)
        let probe_report = probe_anchor_committee_health(
            state,
            session,
            client,
            &mut internal_lc,
            &ctx.offenders,
            manifest,
            anchor_hex,
            g_role_opt.is_some(),
            now_secs,
            password,
        )
        .await;

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
                    &probe_report.unhealthy_now,
                    &probe_report.revoked_now,
                    now_secs,
                );
                save_lifecycle_state(service, &lc, password)?;
                Ok(failed)
            },
        )?;

        // Derive signing authority keys if present in wallet (strictly gated on DailyPay)
        let (creator_sk_opt, guarantor_sk_opt) = derive_anchor_authority_keys(
            state,
            session,
            anchor_hex,
            *has_creator,
            g_role_opt.as_ref(),
            password,
        )?;

        if !ctx.outflow_allowed {
            continue;
        }

        evaluate_and_initiate_anchor_rotation(
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
            anchor_hex,
            manifest,
            &failed_lock_pubs,
            creator_sk_opt,
            guarantor_sk_opt,
            probe_report.last_creator_activity_secs,
            &ctx,
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
