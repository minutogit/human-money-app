use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use human_money_core::humoco_proto::{
    crypto::{DEFAULT_LOCK_POW_BASE_BITS, MAX_LOCK_POW_BITS},
    manifest::ServerEntry,
};
use human_money_core::models::layer2_api::{
    LockEntry, ObservedSlotState, VoucherReconciliationResult,
};
use human_money_core::wallet::reconciliation::VoucherLockReadTarget;

use crate::{
    models::{FrontendError, FrontendVoucherL2StatusDto},
    services::common::{resolve_verified_server_locator, upsert_verified_locator_only},
    settings::InternalLifecycleState,
    sync_client::{self, FrontendLockServerProbeResult},
    AppState,
};

#[derive(Debug, Clone)]
pub(crate) struct StaticReconcileSummary {
    pub active_anchored_count: usize,
    pub reconciled_spent_count: usize,
    pub all_resolved: bool,
    pub status: String, // "no_servers" | "offline" | "partial" | "reconciled"
}

pub(super) struct StaticSlotProbeResult {
    pub(super) candidate_entries: Vec<LockEntry>,
    pub(super) responded_404: usize,
    pub(super) total_servers: usize,
    pub(super) threshold: u16,
    pub(super) any_server_answered: bool,
}

pub(super) enum StaticVoucherReconcileOutcome {
    Resolved(Box<VoucherReconciliationResult>),
    Inconclusive { reason: String, any_reachable: bool },
}

pub(super) async fn verify_and_cache_static_read_locator(
    client: &reqwest::Client,
    srv: &ServerEntry,
    probe_cache: &mut BTreeMap<String, FrontendLockServerProbeResult>,
    learned_probes: &mut Vec<FrontendLockServerProbeResult>,
) -> Option<String> {
    if let Some((_, verified_read_url)) = resolve_verified_server_locator(srv, probe_cache) {
        return Some(verified_read_url);
    }

    let lock_hex = hex::encode(srv.lock_pub);
    let admin_hex = hex::encode(srv.admin_pub);
    let info = tokio::time::timeout(
        Duration::from_secs(3),
        sync_client::fetch_sync_info(client, &srv.write_url),
    )
    .await
    .ok()?
    .ok()?;

    if let Some(ref rev) = info.revocation {
        if rev.replacement.is_some() {
            if let Some(probe) =
                crate::l2_lifecycle::try_follow_relocation_notice(client, probe_cache, srv, rev)
                    .await
            {
                learned_probes.push(probe);
                return resolve_verified_server_locator(srv, probe_cache).map(|(_, r)| r);
            }
        }
        let now_secs = sync_client::current_unix_timestamp();
        if sync_client::verify_revocation_notice(rev, &admin_hex).is_ok()
            && rev.effective_at <= now_secs
        {
            return None;
        }
    }

    let lock_info = info.lock?;
    if !lock_info.lock_pub.trim().eq_ignore_ascii_case(&lock_hex) {
        return None;
    }
    let del = lock_info.delegation?;
    let d_write = del.write_url.trim().to_string();
    let d_read = del.read_url.trim().to_string();
    if !sync_client::verify_lock_delegation(&lock_hex, &del, Some(&admin_hex))
        || sync_client::validate_sync_url(&d_write).is_err()
        || sync_client::validate_sync_read_url(&d_read).is_err()
    {
        return None;
    }

    let probe = FrontendLockServerProbeResult {
        write_url: d_write,
        read_url: d_read.clone(),
        drop_pub: info.drop_pub.trim().to_ascii_lowercase(),
        admin_pub: admin_hex,
        lock_pub: lock_hex,
        delegation_valid: true,
        waf_probe_passed: false,
        selftest_passed: false,
        key_leak_safe: false,
        passed: false,
        latency_ms: 0,
        max_retention_quarter: lock_info.max_retention_quarter,
        pow_base_bits: lock_info.pow_base_bits.min(MAX_LOCK_POW_BITS),
        allow_pow_tickets: lock_info.allow_pow_tickets,
        ticket_pow_bits: lock_info.ticket_pow_bits.min(MAX_LOCK_POW_BITS),
        inode_warning: lock_info.inode_warning,
        used_inodes: lock_info.used_inodes,
        max_inodes: lock_info.max_inodes,
        probed_at_secs: sync_client::current_unix_timestamp(),
        delegation: Some(del),
        error: None,
    };
    upsert_verified_locator_only(probe_cache, probe.clone());
    learned_probes.push(probe);
    Some(d_read)
}

pub(super) async fn probe_voucher_static_lock(
    client: &reqwest::Client,
    target: &VoucherLockReadTarget,
    probe_cache: &mut BTreeMap<String, FrontendLockServerProbeResult>,
    offenders: &humoco_wot::ServerOffenderList,
    learned_probes: &mut Vec<FrontendLockServerProbeResult>,
) -> StaticSlotProbeResult {
    let mut seen_lock_pubs: HashSet<[u8; 32]> = HashSet::new();
    let mut dedup_paths = Vec::with_capacity(target.server_paths.len());
    for (srv, rel_path) in &target.server_paths {
        if seen_lock_pubs.insert(srv.lock_pub) {
            dedup_paths.push((srv, rel_path));
        }
    }
    let total_servers = dedup_paths.len();
    let mut candidate_entries = Vec::new();
    let mut responded_404 = 0usize;
    let mut any_server_answered = false;

    for (srv, rel_path) in dedup_paths {
        if crate::l2_lifecycle::is_server_key_revoked(
            offenders,
            &srv.lock_pub,
            Some(&srv.admin_pub),
        ) {
            continue;
        }
        let Some(verified_read_url) =
            verify_and_cache_static_read_locator(client, srv, probe_cache, learned_probes).await
        else {
            continue;
        };
        match sync_client::fetch_static_lock_entry(client, &verified_read_url, rel_path).await {
            Ok(Some(entry)) => {
                any_server_answered = true;
                if entry.slot == target.slot {
                    candidate_entries.push(entry);
                }
            }
            Ok(None) => {
                any_server_answered = true;
                responded_404 += 1;
            }
            Err(_) => {}
        }
    }

    StaticSlotProbeResult {
        candidate_entries,
        responded_404,
        total_servers,
        threshold: target.threshold,
        any_server_answered,
    }
}

pub(super) fn reconcile_static_probe_on_service(
    service: &mut human_money_core::app_service::AppService,
    local_id: &str,
    probe: &StaticSlotProbeResult,
    pow_bits: u32,
    password: Option<&str>,
) -> Result<StaticVoucherReconcileOutcome, FrontendError> {
    let mut had_entry_error = false;
    let mut first_error_reason: Option<String> = None;

    for entry in &probe.candidate_entries {
        let res = service.reconcile_voucher_state(
            local_id,
            ObservedSlotState::Entry(entry),
            None,
            pow_bits,
            password,
        );
        match res {
            Ok(outcome) => return Ok(StaticVoucherReconcileOutcome::Resolved(Box::new(outcome))),
            Err(err) => {
                log::debug!(
                    "Static lock candidate entry rejected for voucher {}: {}",
                    local_id,
                    err
                );
                had_entry_error = true;
                if first_error_reason.is_none() {
                    first_error_reason = Some(err.to_string());
                }
            }
        }
    }

    match ObservedSlotState::verify_quorum_404_coverage(
        probe.responded_404,
        probe.total_servers,
        probe.threshold,
    ) {
        Ok(()) => {
            let observed = ObservedSlotState::ConfirmedEmpty {
                responded_404: probe.responded_404,
                total_servers: probe.total_servers,
                threshold: probe.threshold,
            };
            match service.reconcile_voucher_state(local_id, observed, None, pow_bits, password) {
                Ok(outcome) => Ok(StaticVoucherReconcileOutcome::Resolved(Box::new(outcome))),
                Err(err) => Err(FrontendError::from(err)),
            }
        }
        Err(coverage_reason) => {
            let reason = if had_entry_error {
                first_error_reason.unwrap_or(coverage_reason)
            } else {
                coverage_reason
            };
            Ok(StaticVoucherReconcileOutcome::Inconclusive {
                reason,
                any_reachable: probe.any_server_answered,
            })
        }
    }
}

pub(super) struct VoucherStaticProbeDescriptor {
    local_id: String,
    anchor_id: [u8; 32],
    manifest_rev: Option<u64>,
    reduced_protection: bool,
    creator_did: Option<String>,
    guarantor_dids: Vec<String>,
    target: Option<VoucherLockReadTarget>,
}

pub(super) struct VoucherStaticProbeWithDiscoveryResult {
    probe: Option<StaticSlotProbeResult>,
    manifest_rev: Option<u64>,
    reduced_protection: bool,
}

pub(super) async fn rebuild_reconcile_read_target(
    state: &AppState,
    session: &crate::session::SessionToken,
    local_id: &str,
) -> Option<VoucherLockReadTarget> {
    state
        .with_session(session, |service| {
            service
                .with_wallet_and_identity(|wallet, identity| {
                    wallet
                        .build_voucher_read_targets(identity, local_id, None)
                        .ok()
                })
                .map_err(FrontendError::from)
        })
        .ok()
        .flatten()
        .flatten()
}

/// Shared static-lock probe helper used by BOTH `reconcile_vouchers_via_static_locks` and
/// `check_voucher_l2_status_impl`, handling both missing local manifests and empty/404-after-rotation
/// manifest discovery while returning the latest verified `manifest_rev` and `reduced_protection`.
#[allow(clippy::too_many_arguments)]
pub(super) async fn probe_voucher_static_lock_with_discovery(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    mut desc: VoucherStaticProbeDescriptor,
    probe_cache: &mut BTreeMap<String, FrontendLockServerProbeResult>,
    offenders: &humoco_wot::ServerOffenderList,
    learned_probes: &mut Vec<FrontendLockServerProbeResult>,
    password: Option<&str>,
) -> VoucherStaticProbeWithDiscoveryResult {
    let mut manifest_rev = desc.manifest_rev;
    let mut reduced_protection = desc.reduced_protection;

    if desc.target.is_none() {
        if let Ok(Some(discovered)) = crate::l2_lifecycle::discover_anchor_manifest(
            state,
            session,
            client,
            &desc.anchor_id,
            desc.creator_did.as_deref(),
            &desc.guarantor_dids,
            password,
        )
        .await
        {
            manifest_rev = Some(discovered.revision);
            reduced_protection = discovered
                .has_reduced_guarantor_protection()
                .unwrap_or(false);
            desc.target = rebuild_reconcile_read_target(state, session, &desc.local_id).await;
        }
    }

    let Some(mut target) = desc.target else {
        return VoucherStaticProbeWithDiscoveryResult {
            probe: None,
            manifest_rev,
            reduced_protection,
        };
    };

    let mut probe =
        probe_voucher_static_lock(client, &target, probe_cache, offenders, learned_probes).await;

    if probe.candidate_entries.is_empty() {
        let current_rev = manifest_rev.unwrap_or(0);
        if let Ok(Some(discovered)) = crate::l2_lifecycle::discover_anchor_manifest(
            state,
            session,
            client,
            &desc.anchor_id,
            desc.creator_did.as_deref(),
            &desc.guarantor_dids,
            password,
        )
        .await
        {
            if discovered.revision > current_rev {
                manifest_rev = Some(discovered.revision);
                reduced_protection = discovered
                    .has_reduced_guarantor_protection()
                    .unwrap_or(false);
                if let Some(new_target) =
                    rebuild_reconcile_read_target(state, session, &desc.local_id).await
                {
                    target = new_target;
                    probe = probe_voucher_static_lock(
                        client,
                        &target,
                        probe_cache,
                        offenders,
                        learned_probes,
                    )
                    .await;
                }
            }
        }
    }

    VoucherStaticProbeWithDiscoveryResult {
        probe: Some(probe),
        manifest_rev,
        reduced_protection,
    }
}

pub(crate) async fn reconcile_vouchers_via_static_locks(
    state: &AppState,
    session: &crate::session::SessionToken,
    filter_ids: Option<&[String]>,
    password: Option<&str>,
) -> Result<StaticReconcileSummary, FrontendError> {
    let filter_set: Option<HashSet<&str>> =
        filter_ids.map(|ids| ids.iter().map(|s| s.as_str()).collect());

    let Some((mut probe_cache, offenders, descriptors)) =
        state.with_session(session, |service| {
            let lifecycle = state
                .get_or_load_internal_lifecycle(service, password)
                .map_err(FrontendError::from)?;
            let offenders = state.get_or_load_server_offender_list(service, password)?;
            let descriptors = service
                .with_wallet_and_identity(|wallet, identity| {
                    let mut list = Vec::new();
                    for (id, instance) in &wallet.voucher_store.vouchers {
                        if instance.status
                            != human_money_core::wallet::instance::VoucherStatus::Active
                        {
                            continue;
                        }
                        if let Some(ref fset) = filter_set {
                            if !fset.contains(id.as_str()) {
                                continue;
                            }
                        }
                        let Some(anchor_id) = instance.voucher.layer2_anchor else {
                            continue;
                        };
                        let creator_did = instance.voucher.creator_profile.id.clone();
                        let guarantor_dids: Vec<String> = instance
                            .voucher
                            .signatures
                            .iter()
                            .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
                            .map(|s| s.signer_id.clone())
                            .collect();
                        let (reduced_protection, manifest_rev) = wallet
                            .profile
                            .l2_anchor_manifests
                            .get(&hex::encode(anchor_id))
                            .map(|m| {
                                (
                                    m.has_reduced_guarantor_protection().unwrap_or(false),
                                    Some(m.revision),
                                )
                            })
                            .unwrap_or((false, None));
                        let target = wallet.build_voucher_read_targets(identity, id, None).ok();
                        list.push(VoucherStaticProbeDescriptor {
                            local_id: id.clone(),
                            anchor_id,
                            manifest_rev,
                            reduced_protection,
                            creator_did,
                            guarantor_dids,
                            target,
                        });
                    }
                    list
                })
                .map_err(FrontendError::from)?;
            Ok((lifecycle.l2_probe_cache, offenders, descriptors))
        })?
    else {
        return Err(FrontendError::from(
            "Wallet session changed before static L2 reconciliation started".to_string(),
        ));
    };

    let active_anchored_count = descriptors.len();
    if active_anchored_count == 0 {
        return Ok(StaticReconcileSummary {
            active_anchored_count: 0,
            reconciled_spent_count: 0,
            all_resolved: true,
            status: "reconciled".to_string(),
        });
    }

    let client = sync_client::create_sync_probe_client();
    let mut unresolved_missing_count = 0usize;
    let mut reconciled_spent_count = 0usize;
    let mut inconclusive_count = 0usize;
    let mut any_committee_reachable = false;
    let mut learned_probes = Vec::new();

    for desc in descriptors {
        let local_id = desc.local_id.clone();
        let probe_res = probe_voucher_static_lock_with_discovery(
            state,
            session,
            &client,
            desc,
            &mut probe_cache,
            &offenders,
            &mut learned_probes,
            password,
        )
        .await;
        let Some(probe) = probe_res.probe else {
            unresolved_missing_count += 1;
            inconclusive_count += 1;
            continue;
        };
        if probe.any_server_answered {
            any_committee_reachable = true;
        }

        let outcome = state.with_session_required(
            session,
            "Wallet session changed during static L2 reconciliation",
            |service| {
                reconcile_static_probe_on_service(
                    service,
                    &local_id,
                    &probe,
                    DEFAULT_LOCK_POW_BASE_BITS,
                    password,
                )
            },
        );
        match outcome {
            Ok(StaticVoucherReconcileOutcome::Resolved(res)) => match *res {
                VoucherReconciliationResult::AlreadySpent { .. }
                | VoucherReconciliationResult::ConflictDetected { .. } => {
                    reconciled_spent_count += 1;
                }
                VoucherReconciliationResult::UnspentSafe => {}
                VoucherReconciliationResult::Inconclusive { reason } => {
                    log::debug!(
                        "Static lock reconciliation inconclusive for {}: {}",
                        local_id,
                        reason
                    );
                    inconclusive_count += 1;
                }
            },
            Ok(StaticVoucherReconcileOutcome::Inconclusive { reason, .. }) => {
                log::debug!(
                    "Static lock probe inconclusive for {}: {}",
                    local_id,
                    reason
                );
                inconclusive_count += 1;
            }
            Err(err) => {
                log::warn!(
                    "Failed persisting static lock reconciliation for {}: {}",
                    local_id,
                    err.message
                );
                inconclusive_count += 1;
            }
        }
    }

    if !learned_probes.is_empty() {
        let _ = state.mutate_internal_lifecycle_for_session(session, password, |lc| {
            let mut changed = false;
            for probe in learned_probes {
                if upsert_verified_locator_only(&mut lc.l2_probe_cache, probe) {
                    changed = true;
                }
            }
            changed
        });
    }

    let all_resolved = inconclusive_count == 0;
    let status = if unresolved_missing_count > 0 && !any_committee_reachable {
        "no_servers".to_string()
    } else if all_resolved {
        "reconciled".to_string()
    } else if any_committee_reachable {
        "partial".to_string()
    } else {
        "offline".to_string()
    };

    Ok(StaticReconcileSummary {
        active_anchored_count,
        reconciled_spent_count,
        all_resolved,
        status,
    })
}

pub(crate) fn clear_fork_lock_for_service(
    service: &mut human_money_core::app_service::AppService,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    if matches!(
        service
            .get_seal_lock_reason(password)
            .map_err(FrontendError::from)?,
        Some(human_money_core::models::seal::SealLockReason::ForkDetected)
    ) {
        service
            .reconcile_rollback_with_diagnostics(&[], None, password)
            .map_err(FrontendError::from)?;
    }
    Ok(())
}

pub(crate) fn has_fork_or_stale_alert_and_offline_vouchers(
    service: &human_money_core::app_service::AppService,
    lifecycle: &InternalLifecycleState,
    password: Option<&str>,
) -> (bool, bool) {
    let fork_or_stale_alert = matches!(
        service.get_seal_lock_reason(password),
        Ok(Some(
            human_money_core::models::seal::SealLockReason::ForkDetected
        ))
    ) || lifecycle
        .wallet_sync_alert
        .as_ref()
        .is_some_and(|a| a.kind == "fork_detected" || a.kind == "stale_local_backup");
    let has_active_offline_vouchers = service
        .with_wallet(|wallet| {
            wallet.voucher_store.vouchers.values().any(|inst| {
                inst.status == human_money_core::wallet::instance::VoucherStatus::Active
                    && inst.voucher.layer2_anchor.is_none()
            })
        })
        .unwrap_or(true);
    (fork_or_stale_alert, has_active_offline_vouchers)
}

pub(crate) async fn reconcile_restored_wallet_if_pending(
    state: &AppState,
    session: &crate::session::SessionToken,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    let Some(is_pending) = state.with_session(session, |service| {
        let lifecycle = state
            .get_or_load_internal_lifecycle(service, password)
            .map_err(FrontendError::from)?;
        Ok(lifecycle.pending_restore_l2_reconciliation)
    })?
    else {
        return Ok(false);
    };
    if !is_pending {
        return Ok(false);
    }

    let summary = reconcile_vouchers_via_static_locks(state, session, None, password).await?;
    if !(summary.all_resolved || summary.active_anchored_count == 0) {
        return Ok(false);
    }

    // Re-load and verify atomically under ONE session-bound lock so a concurrent fork/stale alert
    // or seal lock is never silently wiped or bypassed.
    let updated = state.with_session(session, |service| {
        let mut lc = state
            .get_or_load_internal_lifecycle(service, password)
            .map_err(FrontendError::from)?;
        if !lc.pending_restore_l2_reconciliation {
            return Ok(false);
        }
        let (fork_or_stale_alert, _) =
            has_fork_or_stale_alert_and_offline_vouchers(service, &lc, password);
        if fork_or_stale_alert {
            return Ok(false);
        }
        lc.pending_restore_l2_reconciliation = false;
        state
            .save_internal_lifecycle(service, lc, password)
            .map_err(FrontendError::from)?;
        Ok(true)
    })?;
    Ok(updated.unwrap_or(false))
}

pub(crate) async fn check_voucher_l2_status_impl(
    local_instance_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendVoucherL2StatusDto, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::Unlocked,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let (desc_opt, mut probe_cache, offenders, is_currently_archived) = state
        .with_session_required(
            &session,
            "Wallet session changed before checking voucher L2 status",
            |service| {
                let lifecycle = state
                    .get_or_load_internal_lifecycle(service, pw)
                    .map_err(FrontendError::from)?;
                let probe_cache = lifecycle.l2_probe_cache.clone();
                let offenders = state.get_or_load_server_offender_list(service, pw)?;
                let (desc_opt, is_archived) = service
                    .with_wallet_and_identity(|wallet, identity| {
                        let instance = wallet
                            .voucher_store
                            .vouchers
                            .get(&local_instance_id)
                            .ok_or_else(|| {
                                human_money_core::Error::App(
                                    human_money_core::error::AppError::InvariantViolation {
                                        message: "Voucher not found".into(),
                                    },
                                )
                            })?;
                        let is_archived = matches!(
                            instance.status,
                            human_money_core::wallet::instance::VoucherStatus::Archived
                        );
                        let Some(anchor_id) = instance.voucher.layer2_anchor else {
                            return Ok::<_, human_money_core::Error>((None, is_archived));
                        };
                        let anchor_hex = hex::encode(anchor_id);
                        let (reduced_protection, manifest_rev) = wallet
                            .profile
                            .l2_anchor_manifests
                            .get(&anchor_hex)
                            .map(|m| {
                                (
                                    m.has_reduced_guarantor_protection().unwrap_or(false),
                                    Some(m.revision),
                                )
                            })
                            .unwrap_or((false, None));
                        let creator_did = instance.voucher.creator_profile.id.clone();
                        let guarantor_dids: Vec<String> = instance
                            .voucher
                            .signatures
                            .iter()
                            .filter(|s| s.role.eq_ignore_ascii_case("guarantor"))
                            .map(|s| s.signer_id.clone())
                            .collect();
                        let target = wallet
                            .build_voucher_read_targets(identity, &local_instance_id, None)
                            .ok();
                        Ok((
                            Some(VoucherStaticProbeDescriptor {
                                local_id: local_instance_id.clone(),
                                anchor_id,
                                manifest_rev,
                                reduced_protection,
                                creator_did,
                                guarantor_dids,
                                target,
                            }),
                            is_archived,
                        ))
                    })
                    .map_err(FrontendError::from)?
                    .map_err(FrontendError::from)?;
                Ok((desc_opt, probe_cache, offenders, is_archived))
            },
        )?;

    let Some(desc) = desc_opt else {
        return Ok(FrontendVoucherL2StatusDto {
            status: "offline_only".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
            reduced_guarantor_protection: false,
            manifest_revision: None,
        });
    };

    let client = sync_client::create_sync_probe_client();
    let mut learned_probes = Vec::new();
    let probe_res = probe_voucher_static_lock_with_discovery(
        state,
        &session,
        &client,
        desc,
        &mut probe_cache,
        &offenders,
        &mut learned_probes,
        pw,
    )
    .await;
    if !learned_probes.is_empty() {
        let _ = state.mutate_internal_lifecycle_for_session(&session, pw, |lc| {
            let mut changed = false;
            for p in learned_probes {
                if upsert_verified_locator_only(&mut lc.l2_probe_cache, p) {
                    changed = true;
                }
            }
            changed
        });
    }

    let manifest_rev = probe_res.manifest_rev;
    let reduced_protection = probe_res.reduced_protection;
    let Some(probe) = probe_res.probe else {
        return Ok(FrontendVoucherL2StatusDto {
            status: "no_servers".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
            reduced_guarantor_protection: reduced_protection,
            manifest_revision: manifest_rev,
        });
    };

    if probe.total_servers == 0 {
        return Ok(FrontendVoucherL2StatusDto {
            status: "no_servers".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
            reduced_guarantor_protection: reduced_protection,
            manifest_revision: manifest_rev,
        });
    }

    let outcome = state.with_session_required(
        &session,
        "Wallet session changed while verifying voucher L2 status",
        |service| {
            reconcile_static_probe_on_service(
                service,
                &local_instance_id,
                &probe,
                DEFAULT_LOCK_POW_BASE_BITS,
                pw,
            )
        },
    )?;

    match outcome {
        StaticVoucherReconcileOutcome::Resolved(res) => match *res {
            VoucherReconciliationResult::AlreadySpent { successor_t_id } => {
                Ok(FrontendVoucherL2StatusDto {
                    status: "spent".to_string(),
                    archived: true,
                    successor_t_id: Some(successor_t_id),
                    reduced_guarantor_protection: reduced_protection,
                    manifest_revision: manifest_rev,
                })
            }
            VoucherReconciliationResult::ConflictDetected { conflicting_entry } => {
                let successor_t_id = bs58::encode(conflicting_entry.child_lock).into_string();
                Ok(FrontendVoucherL2StatusDto {
                    status: "conflict".to_string(),
                    archived: true,
                    successor_t_id: Some(successor_t_id),
                    reduced_guarantor_protection: reduced_protection,
                    manifest_revision: manifest_rev,
                })
            }
            VoucherReconciliationResult::UnspentSafe => Ok(FrontendVoucherL2StatusDto {
                status: "unspent".to_string(),
                archived: is_currently_archived,
                successor_t_id: None,
                reduced_guarantor_protection: reduced_protection,
                manifest_revision: manifest_rev,
            }),
            VoucherReconciliationResult::Inconclusive { .. } => Ok(FrontendVoucherL2StatusDto {
                status: "inconclusive".to_string(),
                archived: is_currently_archived,
                successor_t_id: None,
                reduced_guarantor_protection: reduced_protection,
                manifest_revision: manifest_rev,
            }),
        },
        StaticVoucherReconcileOutcome::Inconclusive {
            any_reachable: true,
            ..
        } => Ok(FrontendVoucherL2StatusDto {
            status: "inconclusive".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
            reduced_guarantor_protection: reduced_protection,
            manifest_revision: manifest_rev,
        }),
        StaticVoucherReconcileOutcome::Inconclusive {
            any_reachable: false,
            ..
        } => Ok(FrontendVoucherL2StatusDto {
            status: "offline".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
            reduced_guarantor_protection: reduced_protection,
            manifest_revision: manifest_rev,
        }),
    }
}
