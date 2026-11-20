use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    decode_hex_fixed,
    manifest::{AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry},
    quarter_of_timestamp_ms, retention_covers,
};
use human_money_core::models::layer2_api::{
    LockEntry, ObservedSlotState, VoucherReconciliationResult,
};
use human_money_core::services::l2_gateway;
use human_money_core::wallet::reconciliation::VoucherLockReadTarget;
use rand::RngCore;

use crate::{
    gateway_resolver::{sybil_fold_committee, SybilServerCandidate},
    models::{
        FrontendError, FrontendLockProtectionOverviewDto, FrontendLockServerItemDto,
        FrontendVoucherL2StatusDto,
    },
    services::common::{
        get_wallet_master_seed, max_pow_base_bits_for_servers, normalize_sync_url,
        resolve_server_locator, upsert_verified_lock_probe,
    },
    settings::{is_lock_admin_trusted, InternalLifecycleState, SecurityPolicy},
    sync_client::{self, FrontendLockServerProbeResult, LockTicketAuth},
    AppState,
};

#[derive(Debug, Clone)]
pub(crate) struct StaticReconcileSummary {
    pub active_anchored_count: usize,
    pub reconciled_spent_count: usize,
    pub all_resolved: bool,
    pub status: String, // "no_servers" | "offline" | "partial" | "reconciled"
}

struct StaticSlotProbeResult {
    candidate_entries: Vec<LockEntry>,
    responded_404: usize,
    total_servers: usize,
    threshold: u16,
    any_server_answered: bool,
}

enum StaticVoucherReconcileOutcome {
    Resolved(Box<VoucherReconciliationResult>),
    Inconclusive { reason: String, any_reachable: bool },
}

async fn probe_voucher_static_lock(
    client: &reqwest::Client,
    target: &VoucherLockReadTarget,
    probe_cache: &BTreeMap<String, FrontendLockServerProbeResult>,
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
        let (_, resolved_read_url) = resolve_server_locator(srv, probe_cache);
        match sync_client::fetch_static_lock_entry(client, &resolved_read_url, rel_path).await {
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

fn reconcile_static_probe_on_service(
    service: &mut human_money_core::app_service::AppService,
    local_id: &str,
    probe: &StaticSlotProbeResult,
    pow_bits: u32,
    allow_fork_retry: bool,
    password: Option<&str>,
) -> Result<StaticVoucherReconcileOutcome, FrontendError> {
    let mut had_entry_error = false;
    let mut first_error_reason: Option<String> = None;

    for entry in &probe.candidate_entries {
        let mut res = service.reconcile_voucher_state(
            local_id,
            ObservedSlotState::Entry(entry),
            None,
            pow_bits,
            password,
        );
        if allow_fork_retry && matches!(res, Err(human_money_core::Error::WalletLockedDueToFork)) {
            let _ = service.reconcile_rollback_with_diagnostics(&[], None, password);
            res = service.reconcile_voucher_state(
                local_id,
                ObservedSlotState::Entry(entry),
                None,
                pow_bits,
                password,
            );
        }
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
            let mut res =
                service.reconcile_voucher_state(local_id, observed, None, pow_bits, password);
            if allow_fork_retry
                && matches!(res, Err(human_money_core::Error::WalletLockedDueToFork))
            {
                let _ = service.reconcile_rollback_with_diagnostics(&[], None, password);
                res = service.reconcile_voucher_state(local_id, observed, None, pow_bits, password);
            }
            match res {
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

pub(crate) fn get_seal_sync_status_impl(state: &AppState) -> Result<String, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let status = service
        .get_seal_sync_status()
        .map_err(FrontendError::from)?;
    let status_str = match status {
        human_money_core::models::seal::SyncStatus::PendingUpload => "PendingUpload",
        human_money_core::models::seal::SyncStatus::Synced => "Synced",
    };
    Ok(status_str.to_string())
}

#[derive(Clone, Debug)]
pub(crate) struct KnownSyncSource {
    pub write_url: String,
    pub read_url: String,
    pub admin_pub: String,
    pub source: &'static str, // "own" | "circle" | "public"
    pub priority: u8,         // 0 = own, 1 = circle, 2 = public
}

pub(crate) fn collect_known_sync_sources(
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    password: Option<&str>,
) -> Vec<KnownSyncSource> {
    let mut by_url: BTreeMap<String, KnownSyncSource> = BTreeMap::new();

    if let Ok(Some(admin)) =
        crate::services::sync_engine::admin::load_stored_sync_admin_opt(service, password)
    {
        let norm = normalize_sync_url(&admin.descriptor.write_url);
        if !norm.is_empty() {
            by_url.insert(
                norm,
                KnownSyncSource {
                    write_url: admin.descriptor.write_url.trim().to_string(),
                    read_url: admin.descriptor.read_url.trim().to_string(),
                    admin_pub: admin.descriptor.drop_pub.trim().to_ascii_lowercase(),
                    source: "own",
                    priority: 0,
                },
            );
        }
    }

    for d in &policy.market_selected_syncs {
        let norm = normalize_sync_url(&d.write_url);
        if !norm.is_empty() && !by_url.contains_key(&norm) {
            let (source, priority) = if d.slot_id.is_some() {
                ("public", 2)
            } else {
                ("circle", 1)
            };
            by_url.insert(
                norm,
                KnownSyncSource {
                    write_url: d.write_url.trim().to_string(),
                    read_url: d.read_url.trim().to_string(),
                    admin_pub: d.drop_pub.trim().to_ascii_lowercase(),
                    source,
                    priority,
                },
            );
        }
    }

    for s in &policy.public_syncs {
        if !s.enabled || s.status == "revoked" {
            continue;
        }
        let norm = normalize_sync_url(&s.write_url);
        if !norm.is_empty() && !by_url.contains_key(&norm) {
            by_url.insert(
                norm,
                KnownSyncSource {
                    write_url: s.write_url.trim().to_string(),
                    read_url: s.read_url.trim().to_string(),
                    admin_pub: s.drop_pub.trim().to_ascii_lowercase(),
                    source: "public",
                    priority: 2,
                },
            );
        }
    }

    let mut list: Vec<KnownSyncSource> = by_url.into_values().collect();
    list.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then(a.write_url.cmp(&b.write_url))
    });
    list
}

pub(crate) fn resolve_ticket_auth_for_server(
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    probe: &FrontendLockServerProbeResult,
    password: Option<&str>,
) -> Result<Option<LockTicketAuth>, FrontendError> {
    let probe_write_norm = normalize_sync_url(&probe.write_url);
    let probe_admin_lower = probe.admin_pub.trim().to_ascii_lowercase();
    let master_seed = get_wallet_master_seed(service)?;

    // 1. Own Sync Admin -> Direct signature with admin_sk
    if let Some(admin) =
        crate::services::sync_engine::admin::load_stored_sync_admin_opt(service, password)?
    {
        let admin_write_norm = normalize_sync_url(&admin.descriptor.write_url);
        let admin_pub_lower = admin.descriptor.drop_pub.trim().to_ascii_lowercase();
        let matches = (!admin_pub_lower.is_empty() && admin_pub_lower == probe_admin_lower)
            || (!admin_write_norm.is_empty() && admin_write_norm == probe_write_norm);
        if matches {
            let (admin_sk, _, _) =
                humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
            return Ok(Some(LockTicketAuth::Direct(admin_sk)));
        }
    }

    // 2. Circle / Peer Sync (`market_selected_syncs` without slot_id) -> Stealth key `P`
    for d in &policy.market_selected_syncs {
        if d.slot_id.is_some() {
            continue;
        }
        let d_write_norm = normalize_sync_url(&d.write_url);
        let d_drop_lower = d.drop_pub.trim().to_ascii_lowercase();
        let matches = (!d_drop_lower.is_empty() && d_drop_lower == probe_admin_lower)
            || (!d_write_norm.is_empty() && d_write_norm == probe_write_norm);
        if matches && humoco_market::parse_ed25519_pubkey_hex64(&d_drop_lower).is_ok() {
            let wallet_ed_sk = service
                .with_wallet_and_identity(|_, id| id.signing_key.clone())
                .map_err(FrontendError::from)?;
            return Ok(Some(LockTicketAuth::Stealth {
                wallet_ed_sk,
                drop_pub_hex: d_drop_lower,
            }));
        }
    }

    // 3. Registered Public Sync -> Direct signature with deterministic `R_sk`
    let public_match_url = policy
        .public_syncs
        .iter()
        .filter(|s| s.enabled && s.status != "revoked")
        .find(|s| {
            let s_write_norm = normalize_sync_url(&s.write_url);
            let s_drop_lower = s.drop_pub.trim().to_ascii_lowercase();
            (!s_drop_lower.is_empty() && s_drop_lower == probe_admin_lower)
                || (!s_write_norm.is_empty() && s_write_norm == probe_write_norm)
        })
        .map(|s| s.write_url.clone())
        .or_else(|| {
            policy
                .market_selected_syncs
                .iter()
                .filter(|d| d.slot_id.is_some())
                .find(|d| {
                    let d_write_norm = normalize_sync_url(&d.write_url);
                    let d_drop_lower = d.drop_pub.trim().to_ascii_lowercase();
                    (!d_drop_lower.is_empty() && d_drop_lower == probe_admin_lower)
                        || (!d_write_norm.is_empty() && d_write_norm == probe_write_norm)
                })
                .map(|d| d.write_url.clone())
        });

    if let Some(write_url) = public_match_url {
        let canonical = humoco_market::canonicalize_write_url(&write_url)
            .unwrap_or_else(|_| write_url.trim().to_string());
        let (r_sk, _, _) = humoco_market::derive_public_drop_keypair(&master_seed, &canonical)
            .map_err(|e| FrontendError::from(e.to_string()))?;
        return Ok(Some(LockTicketAuth::Direct(r_sk)));
    }

    // 4. Open PoW fallback ONLY when server advertises `allow_pow_tickets = true`
    if probe.allow_pow_tickets {
        return Ok(Some(LockTicketAuth::Pow { pow_bits: 16 }));
    }

    Ok(None)
}

fn required_retention_quarter(deletable_ms: u64) -> Result<u16, FrontendError> {
    let mut q = quarter_of_timestamp_ms(deletable_ms).ok_or_else(|| {
        FrontendError::from("Voucher deletable_at timestamp is out of quarter range".to_string())
    })?;
    while !retention_covers(deletable_ms, q) {
        q = q.checked_add(1).ok_or_else(|| {
            FrontendError::from("Required retention quarter overflow".to_string())
        })?;
    }
    Ok(q)
}

fn find_reusable_offline_anchor(
    service: &human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    offenders: &humoco_wot::ServerOffenderList,
    deletable_ms: u64,
) -> Option<[u8; 32]> {
    service
        .with_wallet(|w| {
            let mut best: Option<(u64, [u8; 32])> = None;
            for (aid_hex, manifest) in &w.profile.l2_anchor_manifests {
                if manifest.revision != 1
                    || !manifest.authority.guarantor_mig_pubs.is_empty()
                    || !w.profile.l2_anchor_nonces.contains_key(aid_hex)
                    || !retention_covers(deletable_ms, manifest.retain_until_quarter)
                {
                    continue;
                }
                let Ok(aid) = decode_hex_fixed::<32>(aid_hex) else {
                    continue;
                };
                if manifest.verify_anchor_id(&aid).is_err() {
                    continue;
                }
                let healthy_trusted = manifest
                    .servers
                    .iter()
                    .filter(|s| {
                        is_lock_admin_trusted(policy, &hex::encode(s.admin_pub))
                            && !crate::l2_lifecycle::is_server_key_revoked(
                                offenders,
                                &s.lock_pub,
                                Some(&s.admin_pub),
                            )
                    })
                    .count();
                if healthy_trusted >= usize::from(manifest.threshold)
                    && best
                        .as_ref()
                        .is_none_or(|(ts, _)| manifest.issued_at_ms > *ts)
                {
                    best = Some((manifest.issued_at_ms, aid));
                }
            }
            best.map(|(_, aid)| aid)
        })
        .ok()
        .flatten()
}

pub(crate) fn build_lock_protection_overview_from_service(
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    lifecycle: &InternalLifecycleState,
) -> FrontendLockProtectionOverviewDto {
    let sources = collect_known_sync_sources(service, policy, None);
    let mut probe_by_write_norm: BTreeMap<String, &FrontendLockServerProbeResult> = BTreeMap::new();
    let mut probe_by_admin: BTreeMap<String, &FrontendLockServerProbeResult> = BTreeMap::new();
    for probe in lifecycle.l2_probe_cache.values() {
        let wn = normalize_sync_url(&probe.write_url);
        if !wn.is_empty() {
            probe_by_write_norm.insert(wn, probe);
        }
        let ap = probe.admin_pub.trim().to_ascii_lowercase();
        if !ap.is_empty() {
            probe_by_admin.insert(ap, probe);
        }
    }

    let mut seen_admin_or_url: HashSet<String> = HashSet::new();
    let mut servers: Vec<FrontendLockServerItemDto> = Vec::new();
    let mut ready_trusted_count = 0usize;

    let item_status_from_probe = |p: &FrontendLockServerProbeResult| -> String {
        if p.passed && p.delegation_valid {
            "ok".to_string()
        } else if p
            .error
            .as_deref()
            .is_some_and(|e| e.to_ascii_lowercase().contains("not enabled"))
        {
            "no_lock".to_string()
        } else if p.lock_pub.trim().is_empty() {
            "unreachable".to_string()
        } else {
            "unverified".to_string()
        }
    };

    for src in &sources {
        let norm_w = normalize_sync_url(&src.write_url);
        let probe_opt = probe_by_write_norm.get(&norm_w).copied().or_else(|| {
            if !src.admin_pub.is_empty() {
                probe_by_admin.get(&src.admin_pub).copied()
            } else {
                None
            }
        });
        let admin_pub = probe_opt
            .map(|p| p.admin_pub.clone())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| src.admin_pub.clone());
        let lock_pub = probe_opt.map(|p| p.lock_pub.clone()).unwrap_or_default();
        if !lock_pub.is_empty() {
            seen_admin_or_url.insert(lock_pub.clone());
        }
        if !admin_pub.is_empty() {
            seen_admin_or_url.insert(admin_pub.clone());
        }
        seen_admin_or_url.insert(norm_w);

        let trusted = if admin_pub.is_empty() {
            true
        } else {
            is_lock_admin_trusted(policy, &admin_pub)
        };
        let status = match probe_opt {
            Some(p) => item_status_from_probe(p),
            None => "unreachable".to_string(),
        };
        if status == "ok" && trusted {
            ready_trusted_count += 1;
        }
        servers.push(FrontendLockServerItemDto {
            write_url: probe_opt
                .map(|p| p.write_url.clone())
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| src.write_url.clone()),
            read_url: probe_opt
                .map(|p| p.read_url.clone())
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| src.read_url.clone()),
            admin_pub,
            lock_pub,
            source: src.source.to_string(),
            status,
            trusted,
            allow_pow_tickets: probe_opt.is_some_and(|p| p.allow_pow_tickets),
            max_retention_quarter: probe_opt.map(|p| p.max_retention_quarter).unwrap_or(0),
            probed_at_secs: probe_opt.map(|p| p.probed_at_secs).unwrap_or(0),
            error: probe_opt.and_then(|p| p.error.clone()),
        });
    }

    for probe in lifecycle.l2_probe_cache.values() {
        let lp = probe.lock_pub.trim().to_ascii_lowercase();
        let ap = probe.admin_pub.trim().to_ascii_lowercase();
        let wn = normalize_sync_url(&probe.write_url);
        if seen_admin_or_url.contains(&lp)
            || (!ap.is_empty() && seen_admin_or_url.contains(&ap))
            || (!wn.is_empty() && seen_admin_or_url.contains(&wn))
        {
            continue;
        }
        let trusted = if ap.is_empty() {
            true
        } else {
            is_lock_admin_trusted(policy, &ap)
        };
        let status = item_status_from_probe(probe);
        if status == "ok" && trusted {
            ready_trusted_count += 1;
        }
        servers.push(FrontendLockServerItemDto {
            write_url: probe.write_url.clone(),
            read_url: probe.read_url.clone(),
            admin_pub: ap,
            lock_pub: lp,
            source: "cache".to_string(),
            status,
            trusted,
            allow_pow_tickets: probe.allow_pow_tickets,
            max_retention_quarter: probe.max_retention_quarter,
            probed_at_secs: probe.probed_at_secs,
            error: probe.error.clone(),
        });
    }

    let now_ms = sync_client::current_unix_timestamp().saturating_mul(1000);
    let offenders =
        crate::l2_lifecycle::load_server_offender_list(service, None).unwrap_or_default();
    let has_offline_cached_anchor =
        find_reusable_offline_anchor(service, policy, &offenders, now_ms).is_some();
    let unresolved_intents_count = service
        .unresolved_l2_send_intents()
        .map(|v| v.len())
        .unwrap_or(0);

    let status = if lifecycle.pending_restore_l2_reconciliation || unresolved_intents_count > 0 {
        "syncing"
    } else if ready_trusted_count > 0 {
        "ready"
    } else if !servers.is_empty() || has_offline_cached_anchor {
        "degraded"
    } else {
        "none"
    }
    .to_string();

    FrontendLockProtectionOverviewDto {
        status,
        ready_trusted_count,
        servers,
        has_offline_cached_anchor,
        unresolved_intents_count,
        pending_restore_reconciliation: lifecycle.pending_restore_l2_reconciliation,
    }
}

pub(crate) fn get_lock_protection_overview_impl(
    state: &AppState,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let policy = state
        .get_or_load_security_policy(&mut service, None)
        .map_err(FrontendError::from)?;
    let lifecycle = state
        .get_or_load_internal_lifecycle(&mut service, None)
        .map_err(FrontendError::from)?;
    Ok(build_lock_protection_overview_from_service(
        &mut service,
        &policy,
        &lifecycle,
    ))
}

pub(crate) fn set_lock_server_trust_impl(
    admin_pub: String,
    trusted: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    let admin_lower = admin_pub.trim().to_ascii_lowercase();
    if humoco_market::parse_ed25519_pubkey_hex64(&admin_lower).is_err() {
        return Err(FrontendError::from(
            "Invalid admin_pub (expected 64-char hex Ed25519 key)".to_string(),
        ));
    }

    let mut service =
        state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
    let policy = state
        .mutate_security_policy(&mut service, password.as_deref(), |policy| {
            if trusted {
                policy.lock_trust_by_admin_pub.remove(&admin_lower);
            } else {
                policy.lock_trust_by_admin_pub.insert(admin_lower, false);
            }
        })
        .map_err(FrontendError::from)?;
    let lifecycle = state
        .get_or_load_internal_lifecycle(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;
    Ok(build_lock_protection_overview_from_service(
        &mut service,
        &policy,
        &lifecycle,
    ))
}

async fn probe_known_sync_sources_into_cache(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    sources: &[KnownSyncSource],
    password: Option<&str>,
) {
    if sources.is_empty() {
        return;
    }

    let offenders = state
        .with_session(session, |service| {
            crate::l2_lifecycle::load_server_offender_list(service, password)
        })
        .ok()
        .flatten()
        .unwrap_or_default();

    let mut join_set = tokio::task::JoinSet::new();
    for src in sources {
        let c = client.clone();
        let wurl = src.write_url.clone();
        let exp_admin = (!src.admin_pub.is_empty()).then(|| src.admin_pub.clone());
        join_set.spawn(async move {
            tokio::time::timeout(
                Duration::from_secs(3),
                sync_client::probe_lock_server(&c, &wurl, exp_admin.as_deref()),
            )
            .await
            .ok()
        });
    }

    let mut probes = Vec::new();
    while let Some(joined) = join_set.join_next().await {
        if let Ok(Some(mut res)) = joined {
            if let Ok(lock_pk) = decode_hex_fixed::<32>(&res.lock_pub) {
                let admin_pk = decode_hex_fixed::<32>(&res.admin_pub).ok();
                if crate::l2_lifecycle::is_server_key_revoked(
                    &offenders,
                    &lock_pk,
                    admin_pk.as_ref(),
                ) {
                    res.passed = false;
                    res.error = Some(
                        "Server key or operator key is permanently revoked on ServerOffenderList"
                            .to_string(),
                    );
                }
            }
            probes.push(res);
        }
    }

    if !probes.is_empty() {
        let _ = state.mutate_internal_lifecycle_for_session(session, password, |lc| {
            let mut changed = false;
            for probe in probes {
                if upsert_verified_lock_probe(&mut lc.l2_probe_cache, probe) {
                    changed = true;
                }
            }
            changed
        });
    }
}

struct AutoCommitteeSpec {
    creator_mig_pub: [u8; 32],
    anchor_nonce: [u8; 32],
    retain_until_quarter: u16,
}

async fn mint_and_register_committee(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    spec: AutoCommitteeSpec,
    candidates: Vec<(ServerEntry, LockTicketAuth)>,
    password: Option<&str>,
) -> Result<Option<[u8; 32]>, FrontendError> {
    let AutoCommitteeSpec {
        creator_mig_pub,
        anchor_nonce,
        retain_until_quarter,
    } = spec;
    if candidates.is_empty() {
        return Ok(None);
    }

    let now_secs = sync_client::current_unix_timestamp();
    let try_batch = |subset: &[(ServerEntry, LockTicketAuth)]| {
        let n = subset.len();
        let threshold = ((n / 2) + 1) as u8;
        let servers: Vec<ServerEntry> = subset.iter().map(|(s, _)| s.clone()).collect();
        let manifest = AnchorManifest {
            revision: 1,
            threshold,
            servers,
            successors: Vec::new(),
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: Vec::new(),
            },
            migration_chain: Vec::new(),
            issued_at_ms: now_secs.saturating_mul(1000),
            retain_until_quarter,
        };
        let aid = manifest.rev1_anchor_id().ok()?;
        Some((manifest, aid))
    };

    let Some((manifest, anchor_id)) = try_batch(&candidates) else {
        return Ok(None);
    };

    let fetch_tickets = |batch_anchor_id: [u8; 32], subset: Vec<(ServerEntry, LockTicketAuth)>| async move {
        let mut set = tokio::task::JoinSet::new();
        for (idx, (srv, auth)) in subset.into_iter().enumerate() {
            let c = client.clone();
            let wurl = srv.write_url.clone();
            set.spawn(async move {
                let res = tokio::time::timeout(
                    Duration::from_secs(3),
                    sync_client::request_genesis_ticket(
                        &c,
                        &wurl,
                        &batch_anchor_id,
                        retain_until_quarter,
                        auth.clone(),
                    ),
                )
                .await
                .map_err(|_| "GenesisTicket request timeout".to_string())
                .and_then(|r| r);
                (idx, srv, auth, res)
            });
        }
        let mut succeeded = Vec::new();
        let mut surviving = Vec::new();
        let mut had_failure = false;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok((_idx, srv, auth, Ok(ticket))) => {
                    succeeded.push(ticket);
                    surviving.push((srv, auth));
                }
                Ok((_, _, _, Err(_))) | Err(_) => {
                    had_failure = true;
                }
            }
        }
        (succeeded, surviving, had_failure)
    };

    let (tickets1, surviving1, had_failure1) = fetch_tickets(anchor_id, candidates.clone()).await;

    let final_pair = if !had_failure1 && tickets1.len() == manifest.servers.len() {
        Some((manifest, tickets1))
    } else if !surviving1.is_empty() && surviving1.len() < candidates.len() {
        if let Some((retry_manifest, retry_aid)) = try_batch(&surviving1) {
            let (tickets2, _surv2, had_failure2) = fetch_tickets(retry_aid, surviving1).await;
            if !had_failure2 && tickets2.len() == retry_manifest.servers.len() {
                Some((retry_manifest, tickets2))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let Some((chosen_manifest, chosen_tickets)) = final_pair else {
        return Ok(None);
    };

    let saved_id = state
        .with_session(session, |service| {
            service
                .register_l2_anchor_template_with_nonce(
                    chosen_manifest,
                    chosen_tickets,
                    Some(&anchor_nonce),
                    password,
                )
                .map_err(FrontendError::from)
        })?
        .ok_or_else(|| {
            FrontendError::from("Active wallet session changed before anchor save".to_string())
        })?;

    Ok(Some(saved_id))
}

async fn gather_committee_candidates(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    req_quarter: u16,
    password: Option<&str>,
) -> Result<(Vec<(ServerEntry, LockTicketAuth)>, [u8; 32], [u8; 32]), FrontendError> {
    let (sources, policy, lifecycle, offenders) = state.with_session_required(
        session,
        "Wallet session changed before preparing lock committee candidates",
        |service| {
            let policy = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            let lifecycle = state
                .get_or_load_internal_lifecycle(service, password)
                .map_err(FrontendError::from)?;
            let offenders = crate::l2_lifecycle::load_server_offender_list(service, password)?;
            let sources = collect_known_sync_sources(service, &policy, password);
            Ok((sources, policy, lifecycle, offenders))
        },
    )?;

    let now_secs = sync_client::current_unix_timestamp();
    let cached_urls: HashMap<String, &FrontendLockServerProbeResult> = lifecycle
        .l2_probe_cache
        .values()
        .map(|p| (normalize_sync_url(&p.write_url), p))
        .collect();
    let need_probe: Vec<KnownSyncSource> = sources
        .iter()
        .filter(|src| {
            let norm = normalize_sync_url(&src.write_url);
            match cached_urls.get(&norm) {
                None => true,
                Some(p) => !p.passed && now_secs.saturating_sub(p.probed_at_secs) >= 60,
            }
        })
        .cloned()
        .collect();
    if !need_probe.is_empty() {
        probe_known_sync_sources_into_cache(state, session, client, &need_probe, password).await;
    }

    let mut anchor_nonce = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut anchor_nonce);

    let (creator_mig_pub, ranked_pairs) = state.with_session_required(
        session,
        "Wallet session changed while assembling lock committee",
        |service| {
            let creator_mig_pub = service
                .with_wallet_and_identity(|_, id| {
                    l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                        .map(|k| k.verifying_key().to_bytes())
                })
                .map_err(FrontendError::from)?
                .map_err(FrontendError::from)?;

            let fresh_lc = state
                .get_or_load_internal_lifecycle(service, password)
                .map_err(FrontendError::from)?;

            let source_prio_by_url: HashMap<String, u8> = sources
                .iter()
                .map(|s| (normalize_sync_url(&s.write_url), s.priority))
                .collect();
            let source_prio_by_admin: HashMap<String, u8> = sources
                .iter()
                .filter(|s| !s.admin_pub.is_empty())
                .map(|s| (s.admin_pub.clone(), s.priority))
                .collect();

            let mut entries: Vec<(u8, u64, SybilServerCandidate, ServerEntry, LockTicketAuth)> =
                Vec::new();
            for probe in fresh_lc.l2_probe_cache.values() {
                if !probe.passed
                    || !probe.delegation_valid
                    || probe.max_retention_quarter < req_quarter
                    || !is_lock_admin_trusted(&policy, &probe.admin_pub)
                {
                    continue;
                }
                let Ok(lock_pub) = decode_hex_fixed::<32>(&probe.lock_pub) else {
                    continue;
                };
                let Ok(admin_pub) = decode_hex_fixed::<32>(&probe.admin_pub) else {
                    continue;
                };
                if crate::l2_lifecycle::is_server_key_revoked(
                    &offenders,
                    &lock_pub,
                    Some(&admin_pub),
                ) {
                    continue;
                }
                let Some(auth) = resolve_ticket_auth_for_server(service, &policy, probe, password)?
                else {
                    continue;
                };
                let prio = source_prio_by_url
                    .get(&normalize_sync_url(&probe.write_url))
                    .copied()
                    .or_else(|| {
                        source_prio_by_admin
                            .get(&probe.admin_pub.trim().to_ascii_lowercase())
                            .copied()
                    })
                    .unwrap_or(3);
                let cand = SybilServerCandidate {
                    write_url: probe.write_url.clone(),
                    lock_pub,
                    admin_pub: Some(admin_pub),
                    asn: None,
                };
                let srv = ServerEntry {
                    write_url: probe.write_url.clone(),
                    read_url: probe.read_url.clone(),
                    admin_pub,
                    lock_pub,
                    nominated_by: NominatedBy::Creator,
                };
                entries.push((prio, probe.latency_ms, cand, srv, auth));
            }

            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            let cands: Vec<SybilServerCandidate> = entries.iter().map(|e| e.2.clone()).collect();
            let folded = sybil_fold_committee(&cands);
            let allowed_keys: HashSet<[u8; 32]> = folded.into_iter().map(|c| c.lock_pub).collect();
            let selected: Vec<(ServerEntry, LockTicketAuth)> = entries
                .into_iter()
                .filter(|(_, _, c, _, _)| allowed_keys.contains(&c.lock_pub))
                .map(|(_, _, _, srv, auth)| (srv, auth))
                .take(3)
                .collect();

            Ok((creator_mig_pub, selected))
        },
    )?;

    Ok((ranked_pairs, creator_mig_pub, anchor_nonce))
}

pub(crate) async fn assemble_auto_committee_for_voucher(
    state: &AppState,
    session: &crate::session::SessionToken,
    deletable_ms: u64,
    password: Option<&str>,
) -> Result<Option<[u8; 32]>, FrontendError> {
    let req_quarter = required_retention_quarter(deletable_ms)?;
    let client = sync_client::create_sync_probe_client();

    let (candidates, creator_mig_pub, anchor_nonce) =
        gather_committee_candidates(state, session, &client, req_quarter, password).await?;

    if !candidates.is_empty() {
        if let Some(aid) = mint_and_register_committee(
            state,
            session,
            &client,
            AutoCommitteeSpec {
                creator_mig_pub,
                anchor_nonce,
                retain_until_quarter: req_quarter,
            },
            candidates,
            password,
        )
        .await?
        {
            return Ok(Some(aid));
        }
    }

    // Offline / unreachable fallback: reuse any cached rev1 creator-owned anchor covering `deletable_ms`
    let fallback = state.with_session_required(
        session,
        "Wallet session changed during offline anchor fallback lookup",
        |service| {
            let policy = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            let offenders = crate::l2_lifecycle::load_server_offender_list(service, password)?;
            Ok(find_reusable_offline_anchor(
                service,
                &policy,
                &offenders,
                deletable_ms,
            ))
        },
    )?;
    Ok(fallback)
}

pub(crate) async fn prewarm_offline_anchor_if_missing(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    password: Option<&str>,
) {
    let horizon_ms = sync_client::current_unix_timestamp()
        .saturating_add(5 * 365 * 86_400)
        .saturating_mul(1000);
    let Ok(req_quarter) = required_retention_quarter(horizon_ms) else {
        return;
    };

    let already_cached = state
        .with_session(session, |service| {
            let policy = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            let offenders = crate::l2_lifecycle::load_server_offender_list(service, password)?;
            Ok(find_reusable_offline_anchor(service, &policy, &offenders, horizon_ms).is_some())
        })
        .ok()
        .flatten()
        .unwrap_or(true);
    if already_cached {
        return;
    }

    if let Ok((candidates, creator_mig_pub, anchor_nonce)) =
        gather_committee_candidates(state, session, client, req_quarter, password).await
    {
        if !candidates.is_empty() {
            let _ = mint_and_register_committee(
                state,
                session,
                client,
                AutoCommitteeSpec {
                    creator_mig_pub,
                    anchor_nonce,
                    retain_until_quarter: req_quarter,
                },
                candidates,
                password,
            )
            .await;
        }
    }
}

pub(crate) async fn refresh_lock_servers_with_session(
    state: &AppState,
    session: &crate::session::SessionToken,
    password: Option<&str>,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    let sources = state.with_session_required(
        session,
        "Wallet session changed before lock server refresh",
        |service| {
            let policy = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            Ok(collect_known_sync_sources(service, &policy, password))
        },
    )?;

    let client = sync_client::create_sync_probe_client();
    probe_known_sync_sources_into_cache(state, session, &client, &sources, password).await;
    prewarm_offline_anchor_if_missing(state, session, &client, password).await;

    state.with_session_required(
        session,
        "Wallet session changed while building lock protection overview",
        |service| {
            let policy = state
                .get_or_load_security_policy(service, password)
                .map_err(FrontendError::from)?;
            let lifecycle = state
                .get_or_load_internal_lifecycle(service, password)
                .map_err(FrontendError::from)?;
            Ok(build_lock_protection_overview_from_service(
                service, &policy, &lifecycle,
            ))
        },
    )
}

pub(crate) async fn refresh_lock_servers_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::Unlocked,
        password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    refresh_lock_servers_with_session(state, &session, pw).await
}

pub(crate) async fn refresh_lock_servers_for_coordinator(state: &AppState) {
    let session = {
        let Ok(service) = state.service.lock() else {
            return;
        };
        if !service.is_wallet_unlocked() {
            return;
        }
        let Ok(s) = state.session_token_for_service(&service) else {
            return;
        };
        s
    };
    let _ = refresh_lock_servers_with_session(state, &session, None).await;
}

pub(crate) async fn prewarm_l2_checkout_impl(state: &AppState) -> Result<(), FrontendError> {
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    let policy = state.get_or_load_security_policy_locked(None)?;
    let lifecycle = state.get_or_load_internal_lifecycle_locked(None)?;
    let mut urls = Vec::new();
    for s in &policy.public_syncs {
        if s.enabled && s.status != "revoked" {
            urls.push(s.write_url.clone());
        }
    }
    for d in &policy.market_selected_syncs {
        urls.push(d.write_url.clone());
    }
    for probe in lifecycle.l2_probe_cache.values() {
        if probe.passed {
            urls.push(probe.write_url.clone());
        }
    }
    let client = sync_client::create_sync_probe_client();
    crate::gateway_resolver::prewarm_lock_urls(&client, &urls).await;
    Ok(())
}

pub(crate) async fn reconcile_vouchers_via_static_locks(
    state: &AppState,
    filter_ids: Option<&[String]>,
    password: Option<&str>,
) -> Result<StaticReconcileSummary, FrontendError> {
    let filter_set: Option<HashSet<&str>> =
        filter_ids.map(|ids| ids.iter().map(|s| s.as_str()).collect());

    let (session, probe_cache, targets, missing_manifest_ids) = {
        let mut service = state.lock_service()?;
        let session = state.session_token_for_service(&service)?;
        let lifecycle = state
            .get_or_load_internal_lifecycle(&mut service, password)
            .map_err(FrontendError::from)?;
        let (targets, missing_manifest_ids) = service
            .with_wallet_and_identity(|wallet, identity| {
                let mut ready = Vec::new();
                let mut missing = Vec::new();
                for (id, instance) in &wallet.voucher_store.vouchers {
                    if instance.status != human_money_core::wallet::instance::VoucherStatus::Active
                    {
                        continue;
                    }
                    if let Some(ref fset) = filter_set {
                        if !fset.contains(id.as_str()) {
                            continue;
                        }
                    }
                    if instance.voucher.layer2_anchor.is_none() {
                        continue;
                    }
                    match wallet.build_voucher_read_targets(identity, id, None) {
                        Ok(target) => {
                            let pow_bits = max_pow_base_bits_for_servers(
                                &lifecycle,
                                target.server_paths.iter().map(|(srv, _)| srv.lock_pub),
                            );
                            ready.push((id.clone(), target, pow_bits));
                        }
                        Err(_) => {
                            missing.push(id.clone());
                        }
                    }
                }
                (ready, missing)
            })
            .map_err(FrontendError::from)?;
        (
            session,
            lifecycle.l2_probe_cache,
            targets,
            missing_manifest_ids,
        )
    };

    let active_anchored_count = targets.len().saturating_add(missing_manifest_ids.len());
    if active_anchored_count == 0 {
        return Ok(StaticReconcileSummary {
            active_anchored_count: 0,
            reconciled_spent_count: 0,
            all_resolved: true,
            status: "reconciled".to_string(),
        });
    }

    let client = sync_client::create_sync_probe_client();
    let mut reconciled_spent_count = 0usize;
    let mut inconclusive_count = missing_manifest_ids.len();
    let mut any_committee_reachable = false;

    for (local_id, target, pow_bits) in targets {
        let probe = probe_voucher_static_lock(&client, &target, &probe_cache).await;
        if probe.any_server_answered {
            any_committee_reachable = true;
        }

        let outcome = state.with_session_required(
            &session,
            "Wallet session changed during static L2 reconciliation",
            |service| {
                reconcile_static_probe_on_service(
                    service, &local_id, &probe, pow_bits, true, password,
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

    let all_resolved = inconclusive_count == 0;
    let status = if !missing_manifest_ids.is_empty() && !any_committee_reachable {
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
) {
    if matches!(
        service.get_seal_lock_reason(password),
        Ok(Some(
            human_money_core::models::seal::SealLockReason::ForkDetected
        ))
    ) {
        let _ = service.reconcile_rollback_with_diagnostics(&[], None, password);
    }
}

pub(crate) async fn reconcile_restored_wallet_if_pending(
    state: &AppState,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    let session = {
        let mut service = state.lock_service()?;
        let lifecycle = state
            .get_or_load_internal_lifecycle(&mut service, password)
            .map_err(FrontendError::from)?;
        if !lifecycle.pending_restore_l2_reconciliation {
            return Ok(false);
        }
        state.session_token_for_service(&service)?
    };

    let summary = reconcile_vouchers_via_static_locks(state, None, password).await?;
    if summary.all_resolved || summary.active_anchored_count == 0 {
        let cleared = state.with_session(&session, |service| {
            clear_fork_lock_for_service(service, password);
            Ok(())
        })?;
        if cleared.is_none() {
            return Ok(false);
        }
        let updated = state.mutate_internal_lifecycle_for_session(&session, password, |lc| {
            lc.pending_restore_l2_reconciliation = false;
            if lc
                .wallet_sync_alert
                .as_ref()
                .is_some_and(|a| a.kind == "stale_local_backup" || a.kind == "fork_detected")
            {
                lc.wallet_sync_alert = None;
            }
            true
        })?;
        return Ok(updated.is_some());
    }
    Ok(false)
}

pub(crate) async fn check_voucher_l2_status_impl(
    local_instance_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendVoucherL2StatusDto, FrontendError> {
    let session =
        state.authorize_and_begin_session(crate::state::RequiredAccess::Unlocked, None)?;

    let (
        target_opt,
        probe_cache,
        is_currently_archived,
        pow_bits,
        reduced_protection,
        manifest_rev,
    ) = state.with_session_required(
        &session,
        "Wallet session changed before checking voucher L2 status",
        |service| {
            let lifecycle = state
                .get_or_load_internal_lifecycle(service, password.as_deref())
                .map_err(FrontendError::from)?;
            let probe_cache = lifecycle.l2_probe_cache.clone();
            let tup = service
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
                        return Ok::<_, human_money_core::Error>((
                            None,
                            is_archived,
                            DEFAULT_LOCK_POW_BASE_BITS,
                            false,
                            None,
                        ));
                    };
                    let anchor_hex = hex::encode(anchor_id);
                    let (reduced, rev) = wallet
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
                    let target =
                        wallet.build_voucher_read_targets(identity, &local_instance_id, None)?;
                    let bits = max_pow_base_bits_for_servers(
                        &lifecycle,
                        target.server_paths.iter().map(|(srv, _)| srv.lock_pub),
                    );
                    Ok((Some(target), is_archived, bits, reduced, rev))
                })
                .map_err(FrontendError::from)?
                .map_err(FrontendError::from)?;
            Ok((tup.0, probe_cache, tup.1, tup.2, tup.3, tup.4))
        },
    )?;

    let Some(target) = target_opt else {
        return Ok(FrontendVoucherL2StatusDto {
            status: "offline_only".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
            reduced_guarantor_protection: reduced_protection,
            manifest_revision: manifest_rev,
        });
    };

    if target.server_paths.is_empty() {
        return Ok(FrontendVoucherL2StatusDto {
            status: "no_servers".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
            reduced_guarantor_protection: reduced_protection,
            manifest_revision: manifest_rev,
        });
    }

    let client = sync_client::create_sync_probe_client();
    let probe = probe_voucher_static_lock(&client, &target, &probe_cache).await;

    let outcome = state.with_session_required(
        &session,
        "Wallet session changed while verifying voucher L2 status",
        |service| {
            reconcile_static_probe_on_service(
                service,
                &local_instance_id,
                &probe,
                pow_bits,
                false,
                password.as_deref(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontend_voucher_l2_status_dto_serialization() {
        let dto = FrontendVoucherL2StatusDto {
            status: "spent".to_string(),
            archived: true,
            successor_t_id: Some("succ123".to_string()),
            reduced_guarantor_protection: true,
            manifest_revision: Some(2),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"status\":\"spent\""));
        assert!(json.contains("\"archived\":true"));
        assert!(json.contains("\"successorTId\":\"succ123\""));
        assert!(json.contains("\"reducedGuarantorProtection\":true"));
        assert!(json.contains("\"manifestRevision\":2"));

        let unspent_dto = FrontendVoucherL2StatusDto {
            status: "unspent".to_string(),
            archived: false,
            successor_t_id: None,
            reduced_guarantor_protection: false,
            manifest_revision: None,
        };
        let unspent_json = serde_json::to_string(&unspent_dto).unwrap();
        assert!(unspent_json.contains("\"status\":\"unspent\""));
        assert!(unspent_json.contains("\"archived\":false"));
        assert!(!unspent_json.contains("successorTId"));
    }

    #[test]
    fn k1_reconcile_requires_quorum_404_and_survives_corrupt_candidate_entry() {
        use ed25519_dalek::{Signer, SigningKey};
        use human_money_core::humoco_proto::encode_quarter;
        use human_money_core::humoco_proto::manifest::GenesisTicket;
        use human_money_core::models::profile::PublicProfile;
        use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
        use human_money_core::services::l2_gateway;
        use human_money_core::test_utils;

        let dir = crate::test_support::temp_test_dir("k1_quorum_404");
        let pw = "pw-k1-test";
        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();

        let sk1 = SigningKey::from_bytes(&[11u8; 32]);
        let sk2 = SigningKey::from_bytes(&[12u8; 32]);
        let sk3 = SigningKey::from_bytes(&[13u8; 32]);
        let anchor_nonce = [41u8; 32];
        let creator_mig_pub = svc
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let retain_q = encode_quarter(2035, 4).unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2, // N=3, Q=2 -> quorum 404 requires N - Q + 1 = 2 servers returning 404
            servers: vec![&sk1, &sk2, &sk3]
                .into_iter()
                .enumerate()
                .map(|(idx, sk)| ServerEntry {
                    write_url: format!("http://127.0.0.1:1808{}/sync.php", idx + 1),
                    read_url: format!("http://127.0.0.1:1808{}/d", idx + 1),
                    admin_pub: sk.verifying_key().to_bytes(),
                    lock_pub: sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                })
                .collect(),
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
        let tickets: Vec<GenesisTicket> = [&sk1, &sk2, &sk3]
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
        svc.register_l2_anchor_template_with_nonce(
            manifest,
            tickets,
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();

        let my_id = svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        svc.create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(my_id),
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
        let local_id = svc
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone();

        // 1. Only 1 of 3 servers answered 404 (2 unreachable) -> MUST stay Inconclusive
        let insufficient_404 = StaticSlotProbeResult {
            candidate_entries: vec![],
            responded_404: 1,
            total_servers: 3,
            threshold: 2,
            any_server_answered: true,
        };
        let out1 = reconcile_static_probe_on_service(
            &mut svc,
            &local_id,
            &insufficient_404,
            DEFAULT_LOCK_POW_BASE_BITS,
            true,
            Some(pw),
        )
        .unwrap();
        assert!(matches!(
            out1,
            StaticVoucherReconcileOutcome::Inconclusive {
                any_reachable: true,
                ..
            }
        ));

        // 2. Corrupt candidate entry + 1 of 3 404 -> ignores corrupt entry and stays Inconclusive
        let bogus_entry = LockEntry {
            slot: [9u8; 32],
            parent_slot: [0u8; 32],
            output_branch: 0,
            depth: 0,
            child_lock: [8u8; 32],
            sender_pubkey: [7u8; 32],
            receiver_pubkey_hash: [6u8; 32],
            change_pubkey_hash: None,
            causal_sig: [5u8; 64],
            root_mig_sig: Some([4u8; 64]),
            encrypted_payload: [3u8; 265],
            pow_nonce: 0,
        };
        let corrupt_plus_one_404 = StaticSlotProbeResult {
            candidate_entries: vec![bogus_entry.clone()],
            responded_404: 1,
            total_servers: 3,
            threshold: 2,
            any_server_answered: true,
        };
        let out2 = reconcile_static_probe_on_service(
            &mut svc,
            &local_id,
            &corrupt_plus_one_404,
            DEFAULT_LOCK_POW_BASE_BITS,
            true,
            Some(pw),
        )
        .unwrap();
        assert!(matches!(
            out2,
            StaticVoucherReconcileOutcome::Inconclusive {
                any_reachable: true,
                ..
            }
        ));

        // 3. Quorum-intersecting 404 (2 of 3 == N - Q + 1) resolves UnspentSafe even past corrupt entry
        let quorum_404 = StaticSlotProbeResult {
            candidate_entries: vec![bogus_entry],
            responded_404: 2,
            total_servers: 3,
            threshold: 2,
            any_server_answered: true,
        };
        let out3 = reconcile_static_probe_on_service(
            &mut svc,
            &local_id,
            &quorum_404,
            DEFAULT_LOCK_POW_BASE_BITS,
            true,
            Some(pw),
        )
        .unwrap();
        assert!(matches!(
            out3,
            StaticVoucherReconcileOutcome::Resolved(res)
                if matches!(*res, VoucherReconciliationResult::UnspentSafe)
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn build_signed_probe_info_parts(
        admin_sk: &ed25519_dalek::SigningKey,
        lock_sk: &ed25519_dalek::SigningKey,
        write_url: &str,
        read_url: &str,
        max_retention_quarter: u16,
    ) -> (
        sync_client::SyncLockDelegationDto,
        sync_client::SyncLockSelftestReportDto,
    ) {
        use ed25519_dalek::Signer;
        use sha2::Digest;

        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
        let issued_at = 1_760_000_000u64;
        let del_msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex, &lock_hex, write_url, read_url, issued_at,
        );
        let del = sync_client::SyncLockDelegationDto {
            admin_pub: admin_hex,
            lock_pub: lock_hex.clone(),
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            issued_at,
            sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
        };

        let checks = sync_client::SyncLockSelftestChecksDto {
            ext_sodium: "pass".into(),
            sha3_256: "pass".into(),
            atomic_link_eexist: "pass".into(),
            fsync: "pass".into(),
            ignore_user_abort: "pass".into(),
            storage_writable: "pass".into(),
            inode_budget: "pass".into(),
            fastcgi_finish_request: "pass".into(),
        };
        let line = format!(
            "{},{},{},{},{},{},{},{},{}",
            checks.ext_sodium,
            checks.sha3_256,
            checks.atomic_link_eexist,
            checks.fsync,
            checks.ignore_user_abort,
            checks.storage_writable,
            checks.inode_budget,
            checks.fastcgi_finish_request,
            50_000u64
        );
        let digest = hex::encode(sha2::Sha256::digest(line.as_bytes()));
        let st_msg = format!(
            "HUMOCO-LOCK-SELFTEST-V1\nrust\n{}\n{}\n1\n{}",
            issued_at, lock_hex, digest
        );
        let selftest = sync_client::SyncLockSelftestReportDto {
            runtime: "rust".into(),
            checked_at: issued_at,
            lock_pub: lock_hex,
            php_version: None,
            checks,
            free_inodes: 50_000,
            all_passed: true,
            sig: hex::encode(lock_sk.sign(st_msg.as_bytes()).to_bytes()),
        };
        let _ = max_retention_quarter;
        (del, selftest)
    }

    #[tokio::test]
    async fn test_auto_committee_retry_offline_cache_and_fallback_n0() {
        use ed25519_dalek::{Signer, SigningKey};
        use human_money_core::humoco_proto::encode_quarter;
        use human_money_core::humoco_proto::manifest::GenesisTicket;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = crate::test_support::temp_test_dir("auto_committee_fallback");
        let pw = "pw-auto-committee";
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();

        let admin1_sk = SigningKey::from_bytes(&[91u8; 32]);
        let lock1_sk = SigningKey::from_bytes(&[92u8; 32]);
        let admin2_sk = SigningKey::from_bytes(&[93u8; 32]);
        let lock2_sk = SigningKey::from_bytes(&[94u8; 32]);

        let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p1 = l1.local_addr().unwrap().port();
        let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();

        let w1 = format!("http://127.0.0.1:{}/sync.php", p1);
        let r1 = format!("http://127.0.0.1:{}/d", p1);
        let w2 = format!("http://127.0.0.1:{}/sync.php", p2);
        let r2 = format!("http://127.0.0.1:{}/d", p2);

        let retain_q = encode_quarter(2035, 4).unwrap();
        let stop = Arc::new(AtomicBool::new(false));

        let spawn_srv = |listener: tokio::net::TcpListener,
                         admin_sk: SigningKey,
                         lock_sk: SigningKey,
                         wurl: String,
                         rurl: String,
                         fail_ticket: bool,
                         stop_flag: Arc<AtomicBool>| {
            let (del, selftest) =
                build_signed_probe_info_parts(&admin_sk, &lock_sk, &wurl, &rurl, retain_q);
            let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
            let lock_pub = lock_sk.verifying_key().to_bytes();
            let lock_hex = hex::encode(lock_pub);
            tokio::spawn(async move {
                loop {
                    let accept =
                        tokio::time::timeout(Duration::from_millis(40), listener.accept()).await;
                    let Ok(Ok((mut stream, _))) = accept else {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                        continue;
                    };
                    let mut buf = [0u8; 16_384];
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

                    let (status, resp_body) = if path.contains(".lock_key.php") {
                        ("404 Not Found", String::new())
                    } else if path.contains("info") {
                        (
                            "200 OK",
                            serde_json::json!({
                                "drop_pub": admin_hex,
                                "read_url": rurl,
                                "lock": {
                                    "lock_pub": lock_hex,
                                    "max_retention_quarter": retain_q,
                                    "pow_base_bits": 16,
                                    "allow_pow_tickets": false,
                                    "used_inodes": 10,
                                    "max_inodes": 50_000,
                                    "inode_warning": false,
                                    "delegation": del,
                                    "selftest": selftest,
                                }
                            })
                            .to_string(),
                        )
                    } else if path.contains("lock_selftest") {
                        ("200 OK", serde_json::to_string(&selftest).unwrap())
                    } else if path.contains("lock_ticket") {
                        if fail_ticket {
                            (
                                "500 Internal Server Error",
                                r#"{"error":"ticket_boom"}"#.to_string(),
                            )
                        } else {
                            let req: serde_json::Value = serde_json::from_str(body).unwrap();
                            let req_aid =
                                decode_hex_fixed::<32>(req["anchor_id"].as_str().unwrap()).unwrap();
                            let req_q = req["retain_until_quarter"].as_u64().unwrap() as u16;
                            let mut ticket = GenesisTicket {
                                anchor_id: req_aid,
                                retain_until_quarter: req_q,
                                issued_at: 1_760_000_000,
                                lock_pub,
                                sig: [0u8; 64],
                            };
                            ticket.sig = lock_sk.sign(&ticket.digest()).to_bytes();
                            (
                                "200 OK",
                                serde_json::json!({ "ticket": ticket }).to_string(),
                            )
                        }
                    } else if path.contains("?lock") {
                        let req: serde_json::Value = serde_json::from_str(body).unwrap();
                        let nonce = req["client_nonce"]
                            .as_str()
                            .unwrap()
                            .trim()
                            .to_ascii_lowercase();
                        let server_time = 1_760_000_050u64;
                        let msg = format!(
                            "HUMOCO-LOCK-PROBE-V1\n{}\n{}\n{}",
                            lock_hex, nonce, server_time
                        );
                        let resp = sync_client::SyncLockProbeResponseDto {
                            probe_ok: true,
                            lock_pub: lock_hex.clone(),
                            client_nonce: nonce,
                            server_time,
                            sig: hex::encode(lock_sk.sign(msg.as_bytes()).to_bytes()),
                        };
                        ("200 OK", serde_json::to_string(&resp).unwrap())
                    } else {
                        ("404 Not Found", String::new())
                    };
                    let http = format!(
                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        status,
                        resp_body.len(),
                        resp_body
                    );
                    let _ = stream.write_all(http.as_bytes()).await;
                }
            })
        };

        let h1 = spawn_srv(
            l1,
            admin1_sk.clone(),
            lock1_sk.clone(),
            w1.clone(),
            r1.clone(),
            true,
            Arc::clone(&stop),
        );
        let h2 = spawn_srv(
            l2,
            admin2_sk.clone(),
            lock2_sk.clone(),
            w2.clone(),
            r2.clone(),
            false,
            Arc::clone(&stop),
        );

        let state = AppState::new(svc);
        {
            let mut s = state.lock_service().unwrap();
            let make_pub =
                |w: &str, r: &str, admin_sk: &SigningKey| crate::settings::StoredPublicSync {
                    write_url: w.into(),
                    read_url: r.into(),
                    drop_pub: hex::encode(admin_sk.verifying_key().as_bytes()),
                    register_url: None,
                    access_pubkey: "00".repeat(32),
                    enabled: true,
                    authorized_channels: vec!["lock".into()],
                    status: "active".into(),
                    sunset_at: None,
                    revocation_reason: None,
                    replacement: None,
                };
            state
                .mutate_security_policy(&mut s, Some(pw), |p| {
                    p.public_syncs = vec![
                        make_pub(&w1, &r1, &admin1_sk),
                        make_pub(&w2, &r2, &admin2_sk),
                    ];
                })
                .unwrap();
        }
        let session = {
            let s = state.lock_service().unwrap();
            state.session_token_for_service(&s).unwrap()
        };

        // 1. S1 fails ticket with 500 -> single retry succeeds cleanly with S2 (N = 1)
        let deletable_ms = 1_760_000_000_000u64;
        let aid_retry =
            assemble_auto_committee_for_voucher(&state, &session, deletable_ms, Some(pw))
                .await
                .unwrap()
                .expect("retry with surviving server should register N=1 committee");
        let saved_manifest = {
            let s = state.lock_service().unwrap();
            s.with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&hex::encode(aid_retry))
                    .cloned()
            })
            .unwrap()
            .unwrap()
        };
        assert_eq!(saved_manifest.servers.len(), 1);
        assert_eq!(saved_manifest.threshold, 1);
        assert_eq!(
            saved_manifest.servers[0].lock_pub,
            lock2_sk.verifying_key().to_bytes()
        );

        // 2. Both servers unreachable + cache invalidated -> reuses cached offline anchor!
        stop.store(true, Ordering::Relaxed);
        let _ = h1.await;
        let _ = h2.await;
        {
            let mut s = state.lock_service().unwrap();
            state
                .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                    lc.l2_probe_cache.clear();
                })
                .unwrap();
        }
        let aid_offline_cached =
            assemble_auto_committee_for_voucher(&state, &session, deletable_ms, Some(pw))
                .await
                .unwrap()
                .expect("offline fallback must reuse cached rev1 creator anchor");
        assert_eq!(aid_offline_cached, aid_retry);

        // 3. Offline with no matching cache (future quarter 2045 beyond 2035) -> clean N = 0 (Ok(None))
        let far_future_ms = 2_380_000_000_000u64;
        let aid_none =
            assemble_auto_committee_for_voucher(&state, &session, far_future_ms, Some(pw))
                .await
                .unwrap();
        assert!(aid_none.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
