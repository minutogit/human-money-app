use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use human_money_core::humoco_proto::{
    crypto::{DEFAULT_LOCK_POW_BASE_BITS, MAX_LOCK_POW_BITS},
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
        get_wallet_master_seed, normalize_sync_url, resolve_verified_server_locator,
        upsert_verified_locator_only, upsert_verified_lock_probe,
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

async fn verify_and_cache_static_read_locator(
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

async fn probe_voucher_static_lock(
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

fn reconcile_static_probe_on_service(
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
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    password: Option<&str>,
) -> Vec<KnownSyncSource> {
    let mut by_url: BTreeMap<String, KnownSyncSource> = BTreeMap::new();

    if let Ok(Some(desc)) = state.get_or_load_own_sync_descriptor(service, password) {
        let norm = normalize_sync_url(&desc.write_url);
        if !norm.is_empty() {
            by_url.insert(
                norm,
                KnownSyncSource {
                    write_url: desc.write_url.trim().to_string(),
                    read_url: desc.read_url.trim().to_string(),
                    admin_pub: desc.drop_pub.trim().to_ascii_lowercase(),
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

const MAX_AUTO_POW_TICKET_BITS: u32 = 20;

pub(crate) struct AssembledAutoCommittee {
    pub anchor_id: [u8; 32],
    pub lock_pubs: Vec<String>,
    pub threshold: u8,
    pub source: &'static str, // "live" | "cached"
}

/// Matches a stored sync endpoint against a probed server.
/// When both public keys are non-empty, requires exact case-insensitive pubkey equality;
/// URL fallback is permitted ONLY when at least one public key is empty.
pub(crate) fn matches_sync_endpoint(
    stored_pub: &str,
    stored_url: &str,
    probe_admin: &str,
    probe_url: &str,
) -> bool {
    let sp = stored_pub.trim();
    let pa = probe_admin.trim();
    if !sp.is_empty() && !pa.is_empty() {
        return sp.eq_ignore_ascii_case(pa);
    }
    let su = normalize_sync_url(stored_url);
    let pu = normalize_sync_url(probe_url);
    !su.is_empty() && su == pu
}

fn has_usable_ticket_auth(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    probe: &FrontendLockServerProbeResult,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    if let Some(desc) = state.get_or_load_own_sync_descriptor(service, password)? {
        if matches_sync_endpoint(
            &desc.drop_pub,
            &desc.write_url,
            &probe.admin_pub,
            &probe.write_url,
        ) {
            return Ok(true);
        }
    }

    for d in &policy.market_selected_syncs {
        if d.slot_id.is_some() {
            continue;
        }
        let d_drop_lower = d.drop_pub.trim().to_ascii_lowercase();
        if matches_sync_endpoint(
            &d_drop_lower,
            &d.write_url,
            &probe.admin_pub,
            &probe.write_url,
        ) && humoco_market::parse_ed25519_pubkey_hex64(&d_drop_lower).is_ok()
        {
            return Ok(true);
        }
    }

    let has_public = policy
        .public_syncs
        .iter()
        .filter(|s| s.enabled && s.status != "revoked")
        .any(|s| {
            matches_sync_endpoint(
                &s.drop_pub,
                &s.write_url,
                &probe.admin_pub,
                &probe.write_url,
            )
        })
        || policy
            .market_selected_syncs
            .iter()
            .filter(|d| d.slot_id.is_some())
            .any(|d| {
                matches_sync_endpoint(
                    &d.drop_pub,
                    &d.write_url,
                    &probe.admin_pub,
                    &probe.write_url,
                )
            });
    if has_public {
        return Ok(true);
    }

    Ok(probe.allow_pow_tickets && probe.ticket_pow_bits <= MAX_AUTO_POW_TICKET_BITS)
}

pub(crate) fn resolve_ticket_auth_for_server(
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    probe: &FrontendLockServerProbeResult,
    password: Option<&str>,
) -> Result<Option<LockTicketAuth>, FrontendError> {
    let master_seed = get_wallet_master_seed(service, password)?;

    // 1. Own Sync Admin -> Direct signature with admin_sk
    if let Some(admin) =
        crate::services::sync_engine::admin::load_stored_sync_admin_opt(service, password)?
    {
        if matches_sync_endpoint(
            &admin.descriptor.drop_pub,
            &admin.descriptor.write_url,
            &probe.admin_pub,
            &probe.write_url,
        ) {
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
        let d_drop_lower = d.drop_pub.trim().to_ascii_lowercase();
        if matches_sync_endpoint(
            &d_drop_lower,
            &d.write_url,
            &probe.admin_pub,
            &probe.write_url,
        ) && humoco_market::parse_ed25519_pubkey_hex64(&d_drop_lower).is_ok()
        {
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
            matches_sync_endpoint(
                &s.drop_pub,
                &s.write_url,
                &probe.admin_pub,
                &probe.write_url,
            )
        })
        .map(|s| s.write_url.clone())
        .or_else(|| {
            policy
                .market_selected_syncs
                .iter()
                .filter(|d| d.slot_id.is_some())
                .find(|d| {
                    matches_sync_endpoint(
                        &d.drop_pub,
                        &d.write_url,
                        &probe.admin_pub,
                        &probe.write_url,
                    )
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

    // 4. Open PoW fallback ONLY when server advertises `allow_pow_tickets = true` within auto-PoW budget
    if probe.allow_pow_tickets && probe.ticket_pow_bits <= MAX_AUTO_POW_TICKET_BITS {
        return Ok(Some(LockTicketAuth::Pow {
            pow_bits: probe.ticket_pow_bits,
        }));
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

fn estimate_deletable_ms(
    now_secs: u64,
    standard_toml: Option<&str>,
    validity_duration: Option<&str>,
) -> u64 {
    let now_dt =
        chrono::DateTime::from_timestamp(now_secs as i64, 0).unwrap_or_else(chrono::Utc::now);
    let fallback_ms = now_secs.saturating_add(365 * 86_400).saturating_mul(1000);
    let Some(toml) = standard_toml.map(str::trim).filter(|s| !s.is_empty()) else {
        return fallback_ms;
    };
    let Ok((std_def, _)) =
        human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(
            toml,
        )
    else {
        return fallback_ms;
    };
    let dur_str = validity_duration
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std_def
                .mutable
                .app_config
                .default_validity_duration
                .as_deref()
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            let min_dur = std_def
                .immutable
                .issuance
                .issuance_minimum_validity_duration
                .trim();
            (!min_dur.is_empty()).then_some(min_dur)
        })
        .or_else(|| {
            std_def
                .immutable
                .issuance
                .validity_duration_range
                .get(1)
                .or_else(|| std_def.immutable.issuance.validity_duration_range.first())
                .map(String::as_str)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("P1Y");
    let valid_until_dt = human_money_core::services::utils::add_iso8601_duration(now_dt, dur_str)
        .unwrap_or_else(|_| now_dt + chrono::Duration::days(365));
    let deletable_dt = std_def
        .mutable
        .app_config
        .server_history_retention
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|ret| {
            human_money_core::services::utils::add_iso8601_duration(valid_until_dt, ret).ok()
        })
        .unwrap_or(valid_until_dt);
    u64::try_from(deletable_dt.timestamp_millis().max(0)).unwrap_or(fallback_ms)
}

#[allow(clippy::too_many_arguments)]
fn select_expected_candidates_from_lifecycle(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    lifecycle: &InternalLifecycleState,
    offenders: &humoco_wot::ServerOffenderList,
    sources: &[KnownSyncSource],
    req_quarter: u16,
    password: Option<&str>,
) -> Result<Vec<(ServerEntry, FrontendLockServerProbeResult)>, FrontendError> {
    let source_prio_by_url: HashMap<String, u8> = sources
        .iter()
        .map(|s| (normalize_sync_url(&s.write_url), s.priority))
        .collect();
    let source_prio_by_admin: HashMap<String, u8> = sources
        .iter()
        .filter(|s| !s.admin_pub.is_empty())
        .map(|s| (s.admin_pub.clone(), s.priority))
        .collect();

    let mut entries: Vec<(
        u8,
        u64,
        SybilServerCandidate,
        ServerEntry,
        FrontendLockServerProbeResult,
    )> = Vec::new();
    for probe in lifecycle.l2_probe_cache.values() {
        if !probe.passed
            || !probe.delegation_valid
            || probe.max_retention_quarter < req_quarter
            || !is_lock_admin_trusted(policy, &probe.admin_pub)
        {
            continue;
        }
        let Ok(lock_pub) = decode_hex_fixed::<32>(&probe.lock_pub) else {
            continue;
        };
        let Ok(admin_pub) = decode_hex_fixed::<32>(&probe.admin_pub) else {
            continue;
        };
        if crate::l2_lifecycle::is_server_key_revoked(offenders, &lock_pub, Some(&admin_pub)) {
            continue;
        }
        if !has_usable_ticket_auth(state, service, policy, probe, password)? {
            continue;
        }
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
        entries.push((prio, probe.latency_ms, cand, srv, probe.clone()));
    }

    entries.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then_with(|| a.3.lock_pub.cmp(&b.3.lock_pub))
    });
    let cands: Vec<SybilServerCandidate> = entries.iter().map(|e| e.2.clone()).collect();
    let folded = sybil_fold_committee(&cands);
    let allowed_keys: HashSet<[u8; 32]> = folded.into_iter().map(|c| c.lock_pub).collect();
    let selected: Vec<(ServerEntry, FrontendLockServerProbeResult)> = entries
        .into_iter()
        .filter(|(_, _, c, _, _)| allowed_keys.contains(&c.lock_pub))
        .map(|(_, _, _, srv, probe)| (srv, probe))
        .take(3)
        .collect();
    Ok(selected)
}

fn find_reusable_offline_anchor(
    service: &human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    offenders: &humoco_wot::ServerOffenderList,
    deletable_ms: u64,
) -> Option<([u8; 32], AnchorManifest)> {
    service
        .with_wallet(|w| {
            let mut best: Option<(u64, [u8; 32], AnchorManifest)> = None;
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
                        .is_none_or(|(ts, _, _)| manifest.issued_at_ms > *ts)
                {
                    best = Some((manifest.issued_at_ms, aid, manifest.clone()));
                }
            }
            best.map(|(_, aid, m)| (aid, m))
        })
        .ok()
        .flatten()
}

pub(crate) fn build_lock_protection_overview_from_service(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    policy: &SecurityPolicy,
    lifecycle: &InternalLifecycleState,
    standard_toml: Option<&str>,
    validity_duration: Option<&str>,
    password: Option<&str>,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    let offenders = state.get_or_load_server_offender_list(service, password)?;
    let is_offender_hex = |lock_hex: &str, admin_hex: &str| -> bool {
        let lock_pk = decode_hex_fixed::<32>(lock_hex).ok();
        let admin_pk = decode_hex_fixed::<32>(admin_hex).ok();
        match (lock_pk, admin_pk) {
            (Some(lpk), apk) => {
                crate::l2_lifecycle::is_server_key_revoked(&offenders, &lpk, apk.as_ref())
            }
            (None, Some(apk)) => offenders.is_admin_pub_revoked(&apk),
            (None, None) => false,
        }
    };
    let sources = collect_known_sync_sources(state, service, policy, password);
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

    let item_status_from_probe = |p: &FrontendLockServerProbeResult, revoked: bool| -> String {
        if revoked {
            "unverified".to_string()
        } else if p.passed && p.delegation_valid {
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
        let probe_opt = probe_by_write_norm
            .get(&norm_w)
            .copied()
            .filter(|p| {
                matches_sync_endpoint(&src.admin_pub, &src.write_url, &p.admin_pub, &p.write_url)
            })
            .or_else(|| {
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

        let revoked = is_offender_hex(&lock_pub, &admin_pub);
        let trusted = if admin_pub.is_empty() {
            true
        } else {
            is_lock_admin_trusted(policy, &admin_pub)
        };
        let status = match probe_opt {
            Some(p) => item_status_from_probe(p, revoked),
            None => "unreachable".to_string(),
        };
        if status == "ok" && trusted && !revoked {
            ready_trusted_count += 1;
        }
        let error = probe_opt.and_then(|p| p.error.clone()).or_else(|| {
            revoked.then(|| {
                "Server key or operator key is permanently revoked on ServerOffenderList"
                    .to_string()
            })
        });
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
            error,
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
        let revoked = is_offender_hex(&lp, &ap);
        let trusted = if ap.is_empty() {
            true
        } else {
            is_lock_admin_trusted(policy, &ap)
        };
        let status = item_status_from_probe(probe, revoked);
        if status == "ok" && trusted && !revoked {
            ready_trusted_count += 1;
        }
        let error = probe.error.clone().or_else(|| {
            revoked.then(|| {
                "Server key or operator key is permanently revoked on ServerOffenderList"
                    .to_string()
            })
        });
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
            error,
        });
    }

    let now_secs = sync_client::current_unix_timestamp();
    let preview_deletable_ms = estimate_deletable_ms(now_secs, standard_toml, validity_duration);
    let reusable_offline =
        find_reusable_offline_anchor(service, policy, &offenders, preview_deletable_ms);
    let has_offline_cached_anchor = reusable_offline.is_some();
    let preview_quarter = required_retention_quarter(preview_deletable_ms).ok();
    let live_expected = if let Some(req_q) = preview_quarter {
        select_expected_candidates_from_lifecycle(
            state, service, policy, lifecycle, &offenders, &sources, req_q, password,
        )?
    } else {
        Vec::new()
    };
    let (expected_lock_pubs, expected_threshold, expected_source) = if !live_expected.is_empty() {
        let mut pks: Vec<String> = live_expected
            .iter()
            .map(|(s, _)| hex::encode(s.lock_pub))
            .collect();
        pks.sort();
        let thr = ((pks.len() / 2) + 1) as u8;
        (pks, thr, "live".to_string())
    } else if let Some((_, manifest)) = reusable_offline {
        let mut pks: Vec<String> = manifest
            .servers
            .iter()
            .map(|s| hex::encode(s.lock_pub))
            .collect();
        pks.sort();
        (pks, manifest.threshold, "cached".to_string())
    } else {
        (Vec::new(), 0, "none".to_string())
    };
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

    Ok(FrontendLockProtectionOverviewDto {
        status,
        ready_trusted_count,
        servers,
        has_offline_cached_anchor,
        unresolved_intents_count,
        pending_restore_reconciliation: lifecycle.pending_restore_l2_reconciliation,
        expected_lock_pubs,
        expected_threshold,
        expected_source,
    })
}

pub(crate) fn get_lock_protection_overview_impl(
    standard_toml: Option<&str>,
    validity_duration: Option<&str>,
    state: &AppState,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let policy = state
        .get_or_load_security_policy(&mut service, None)
        .map_err(FrontendError::from)?;
    let lifecycle = state
        .get_or_load_internal_lifecycle(&mut service, None)
        .map_err(FrontendError::from)?;
    build_lock_protection_overview_from_service(
        state,
        &mut service,
        &policy,
        &lifecycle,
        standard_toml,
        validity_duration,
        None,
    )
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
    build_lock_protection_overview_from_service(
        state,
        &mut service,
        &policy,
        &lifecycle,
        None,
        None,
        password.as_deref(),
    )
}

async fn probe_known_sync_sources_into_cache(
    state: &AppState,
    session: &crate::session::SessionToken,
    client: &reqwest::Client,
    sources: &[KnownSyncSource],
    password: Option<&str>,
) -> Result<(), FrontendError> {
    if sources.is_empty() {
        return Ok(());
    }

    let offenders = state.with_session_required(
        session,
        "Wallet session changed before loading offender list for sync probe",
        |service| state.get_or_load_server_offender_list(service, password),
    )?;

    let src_admin_by_write_norm: HashMap<String, String> = sources
        .iter()
        .filter(|s| !s.admin_pub.trim().is_empty())
        .map(|s| {
            (
                normalize_sync_url(&s.write_url),
                s.admin_pub.trim().to_ascii_lowercase(),
            )
        })
        .collect();

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
        if let Ok(Some(res)) = joined {
            probes.push(res);
        }
    }

    if !probes.is_empty() {
        let _ = state.mutate_internal_lifecycle_for_session(session, password, |lc| {
            let mut changed = false;
            for mut probe in probes {
                let norm_w = normalize_sync_url(&probe.write_url);
                let hint_admin = (!probe.admin_pub.trim().is_empty())
                    .then(|| probe.admin_pub.trim().to_ascii_lowercase())
                    .or_else(|| src_admin_by_write_norm.get(&norm_w).cloned());
                if probe.lock_pub.trim().is_empty() {
                    let matched = lc
                        .l2_probe_cache
                        .values()
                        .find(|e| !norm_w.is_empty() && normalize_sync_url(&e.write_url) == norm_w)
                        .or_else(|| {
                            let admin_hint = hint_admin.as_ref()?;
                            let mut admin_matches = lc.l2_probe_cache.values().filter(|e| {
                                !e.admin_pub.trim().is_empty()
                                    && e.admin_pub.trim().eq_ignore_ascii_case(admin_hint)
                            });
                            let first = admin_matches.next()?;
                            admin_matches.next().is_none().then_some(first)
                        });
                    if let Some(existing) = matched {
                        probe.lock_pub = existing.lock_pub.clone();
                        if probe.admin_pub.trim().is_empty() {
                            probe.admin_pub = existing.admin_pub.clone();
                        }
                    }
                }
                if let Ok(lock_pk) = decode_hex_fixed::<32>(&probe.lock_pub) {
                    let admin_pk = decode_hex_fixed::<32>(&probe.admin_pub).ok();
                    if crate::l2_lifecycle::is_server_key_revoked(
                        &offenders,
                        &lock_pk,
                        admin_pk.as_ref(),
                    ) {
                        probe.passed = false;
                        probe.error = Some(
                            "Server key or operator key is permanently revoked on ServerOffenderList"
                                .to_string(),
                        );
                    }
                }
                if upsert_verified_lock_probe(&mut lc.l2_probe_cache, probe) {
                    changed = true;
                }
            }
            changed
        });
    }
    Ok(())
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
    candidates: Vec<(ServerEntry, LockTicketAuth, bool, u32)>,
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
    let try_batch = |subset: &[(ServerEntry, LockTicketAuth, bool, u32)]| {
        let n = subset.len();
        let threshold = ((n / 2) + 1) as u8;
        let servers: Vec<ServerEntry> = subset.iter().map(|(s, _, _, _)| s.clone()).collect();
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

    let fetch_tickets =
        |batch_anchor_id: [u8; 32], subset: Vec<(ServerEntry, LockTicketAuth, bool, u32)>| async move {
            let mut set = tokio::task::JoinSet::new();
            for (idx, (srv, auth, allow_pow, pow_bits)) in subset.into_iter().enumerate() {
                let c = client.clone();
                let wurl = srv.write_url.clone();
                let expected_lp = srv.lock_pub;
                set.spawn(async move {
                    let mut active_auth = auth;
                    let mut res = tokio::time::timeout(
                        Duration::from_secs(3),
                        sync_client::request_genesis_ticket(
                            &c,
                            &wurl,
                            expected_lp,
                            &batch_anchor_id,
                            retain_until_quarter,
                            active_auth.clone(),
                        ),
                    )
                    .await
                    .map_err(|_| "GenesisTicket request timeout".to_string())
                    .and_then(|r| r);

                    if let Err(ref err_msg) = res {
                        let is_auth_reject =
                            err_msg.contains("HTTP 401") || err_msg.contains("HTTP 403");
                        if is_auth_reject
                            && allow_pow
                            && pow_bits <= MAX_AUTO_POW_TICKET_BITS
                            && !matches!(active_auth, LockTicketAuth::Pow { .. })
                        {
                            let fallback_auth = LockTicketAuth::Pow { pow_bits };
                            let fallback_res = tokio::time::timeout(
                                Duration::from_secs(4),
                                sync_client::request_genesis_ticket(
                                    &c,
                                    &wurl,
                                    expected_lp,
                                    &batch_anchor_id,
                                    retain_until_quarter,
                                    fallback_auth.clone(),
                                ),
                            )
                            .await
                            .map_err(|_| "GenesisTicket PoW fallback timeout".to_string())
                            .and_then(|r| r);
                            if fallback_res.is_ok() {
                                active_auth = fallback_auth;
                            }
                            res = fallback_res;
                        }
                    }
                    (idx, srv, active_auth, allow_pow, pow_bits, res)
                });
            }
            let mut succeeded_indexed = Vec::new();
            let mut surviving_indexed = Vec::new();
            let mut failed_servers = Vec::new();
            let mut had_failure = false;
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok((idx, srv, auth, allow_pow, pow_bits, Ok(ticket))) => {
                        succeeded_indexed.push((idx, ticket));
                        surviving_indexed.push((idx, (srv, auth, allow_pow, pow_bits)));
                    }
                    Ok((_, srv, _, _, _, Err(err_msg))) => {
                        had_failure = true;
                        failed_servers.push((hex::encode(srv.lock_pub), err_msg));
                    }
                    Err(_) => {
                        had_failure = true;
                    }
                }
            }
            succeeded_indexed.sort_by_key(|(idx, _)| *idx);
            surviving_indexed.sort_by_key(|(idx, _)| *idx);
            let succeeded = succeeded_indexed
                .into_iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>();
            let surviving = surviving_indexed
                .into_iter()
                .map(|(_, item)| item)
                .collect::<Vec<_>>();
            (succeeded, surviving, failed_servers, had_failure)
        };

    let (tickets1, surviving1, mut all_failed, had_failure1) =
        fetch_tickets(anchor_id, candidates.clone()).await;

    let final_pair = if !had_failure1 && tickets1.len() == manifest.servers.len() {
        Some((manifest, tickets1))
    } else if !surviving1.is_empty() && surviving1.len() < candidates.len() {
        if let Some((retry_manifest, retry_aid)) = try_batch(&surviving1) {
            let (tickets2, _surv2, failed2, had_failure2) =
                fetch_tickets(retry_aid, surviving1).await;
            all_failed.extend(failed2);
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

    if !all_failed.is_empty() {
        let _ = state.mutate_internal_lifecycle_for_session(session, password, |lc| {
            let mut changed = false;
            for (lock_hex, err_msg) in &all_failed {
                if let Some(cached) = lc.l2_probe_cache.get_mut(lock_hex) {
                    cached.passed = false;
                    cached.probed_at_secs = now_secs;
                    cached.error = Some(err_msg.clone());
                    changed = true;
                }
            }
            changed
        });
    }

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
    force_reprobe: bool,
    password: Option<&str>,
) -> Result<
    (
        Vec<(ServerEntry, LockTicketAuth, bool, u32)>,
        [u8; 32],
        [u8; 32],
    ),
    FrontendError,
> {
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
            let offenders = state.get_or_load_server_offender_list(service, password)?;
            let sources = collect_known_sync_sources(state, service, &policy, password);
            Ok((sources, policy, lifecycle, offenders))
        },
    )?;

    let now_secs = sync_client::current_unix_timestamp();
    let cached_urls: HashMap<String, &FrontendLockServerProbeResult> = lifecycle
        .l2_probe_cache
        .values()
        .map(|p| (normalize_sync_url(&p.write_url), p))
        .collect();
    let need_probe: Vec<KnownSyncSource> = if force_reprobe {
        sources.clone()
    } else {
        sources
            .iter()
            .filter(|src| {
                let norm = normalize_sync_url(&src.write_url);
                match cached_urls.get(&norm) {
                    None => true,
                    Some(p) => !p.passed && now_secs.saturating_sub(p.probed_at_secs) >= 60,
                }
            })
            .cloned()
            .collect()
    };
    if !need_probe.is_empty() {
        probe_known_sync_sources_into_cache(state, session, client, &need_probe, password).await?;
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
            let expected = select_expected_candidates_from_lifecycle(
                state,
                service,
                &policy,
                &fresh_lc,
                &offenders,
                &sources,
                req_quarter,
                password,
            )?;
            let mut out = Vec::with_capacity(expected.len());
            for (srv, probe) in expected {
                if let Some(auth) =
                    resolve_ticket_auth_for_server(service, &policy, &probe, password)?
                {
                    out.push((srv, auth, probe.allow_pow_tickets, probe.ticket_pow_bits));
                }
            }
            Ok((creator_mig_pub, out))
        },
    )?;

    Ok((ranked_pairs, creator_mig_pub, anchor_nonce))
}

pub(crate) async fn assemble_auto_committee_for_voucher(
    state: &AppState,
    session: &crate::session::SessionToken,
    deletable_ms: u64,
    force_reprobe: bool,
    password: Option<&str>,
) -> Result<Option<AssembledAutoCommittee>, FrontendError> {
    let req_quarter = required_retention_quarter(deletable_ms)?;
    let client = sync_client::create_sync_probe_client();

    let (candidates, creator_mig_pub, anchor_nonce) = gather_committee_candidates(
        state,
        session,
        &client,
        req_quarter,
        force_reprobe,
        password,
    )
    .await?;

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
            let meta = state.with_session_required(
                session,
                "Wallet session changed after registering live L2 committee",
                |service| {
                    let aid_hex = hex::encode(aid);
                    service
                        .with_wallet(|w| {
                            w.profile.l2_anchor_manifests.get(&aid_hex).map(|m| {
                                let mut pks: Vec<String> =
                                    m.servers.iter().map(|s| hex::encode(s.lock_pub)).collect();
                                pks.sort();
                                (pks, m.threshold)
                            })
                        })
                        .map_err(FrontendError::from)
                },
            )?;
            if let Some((lock_pubs, threshold)) = meta {
                return Ok(Some(AssembledAutoCommittee {
                    anchor_id: aid,
                    lock_pubs,
                    threshold,
                    source: "live",
                }));
            }
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
            let offenders = state.get_or_load_server_offender_list(service, password)?;
            Ok(
                find_reusable_offline_anchor(service, &policy, &offenders, deletable_ms).map(
                    |(aid, m)| {
                        let mut pks: Vec<String> =
                            m.servers.iter().map(|s| hex::encode(s.lock_pub)).collect();
                        pks.sort();
                        AssembledAutoCommittee {
                            anchor_id: aid,
                            lock_pubs: pks,
                            threshold: m.threshold,
                            source: "cached",
                        }
                    },
                ),
            )
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
            let offenders = state.get_or_load_server_offender_list(service, password)?;
            Ok(find_reusable_offline_anchor(service, &policy, &offenders, horizon_ms).is_some())
        })
        .ok()
        .flatten()
        .unwrap_or(true);
    if already_cached {
        return;
    }

    if let Ok((candidates, creator_mig_pub, anchor_nonce)) =
        gather_committee_candidates(state, session, client, req_quarter, false, password).await
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
            Ok(collect_known_sync_sources(
                state, service, &policy, password,
            ))
        },
    )?;

    let client = sync_client::create_sync_probe_client();
    probe_known_sync_sources_into_cache(state, session, &client, &sources, password).await?;
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
            build_lock_protection_overview_from_service(
                state, service, &policy, &lifecycle, None, None, password,
            )
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

pub(crate) async fn refresh_lock_servers_for_coordinator(state: &AppState) -> bool {
    let session = {
        let Ok(service) = state.service.lock() else {
            return false;
        };
        if !service.is_wallet_unlocked() {
            return false;
        }
        let Ok(s) = state.session_token_for_service(&service) else {
            return false;
        };
        s
    };
    refresh_lock_servers_with_session(state, &session, None)
        .await
        .is_ok()
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

struct VoucherStaticProbeDescriptor {
    local_id: String,
    anchor_id: [u8; 32],
    manifest_rev: Option<u64>,
    reduced_protection: bool,
    creator_did: Option<String>,
    guarantor_dids: Vec<String>,
    target: Option<VoucherLockReadTarget>,
}

struct VoucherStaticProbeWithDiscoveryResult {
    probe: Option<StaticSlotProbeResult>,
    manifest_rev: Option<u64>,
    reduced_protection: bool,
}

async fn rebuild_reconcile_read_target(
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
async fn probe_voucher_static_lock_with_discovery(
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::NominatedBy;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

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
            assemble_auto_committee_for_voucher(&state, &session, deletable_ms, false, Some(pw))
                .await
                .unwrap()
                .expect("retry with surviving server should register N=1 committee")
                .anchor_id;
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
            assemble_auto_committee_for_voucher(&state, &session, deletable_ms, false, Some(pw))
                .await
                .unwrap()
                .expect("offline fallback must reuse cached rev1 creator anchor")
                .anchor_id;
        assert_eq!(aid_offline_cached, aid_retry);

        // 3. Offline with no matching cache (future quarter 2045 beyond 2035) -> clean N = 0 (Ok(None))
        let far_future_ms = 2_380_000_000_000u64;
        let aid_none =
            assemble_auto_committee_for_voucher(&state, &session, far_future_ms, false, Some(pw))
                .await
                .unwrap();
        assert!(aid_none.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_k1_and_h9_unverified_404_rejected_and_negative_health_overwrites_stale() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let dir = crate::test_support::temp_test_dir("k1_h9_locator_health");
        let pw = "StrongPassword123!";
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let admin_sk = SigningKey::from_bytes(&[61u8; 32]);
        let lock_sk = SigningKey::from_bytes(&[62u8; 32]);
        let admin_pub: [u8; 32] = admin_sk.verifying_key().to_bytes();
        let lock_pub: [u8; 32] = lock_sk.verifying_key().to_bytes();
        let admin_hex = hex::encode(admin_pub);
        let lock_hex = hex::encode(lock_pub);

        // Unverified fake read server returning 404 (not in probe_cache, ?info fails)
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = Arc::clone(&stop);
        let srv = tokio::spawn(async move {
            while !stop_bg.load(Ordering::Relaxed) {
                if let Ok(Ok((mut stream, _))) =
                    tokio::time::timeout(Duration::from_millis(50), listener.accept()).await
                {
                    let mut buf = [0u8; 2048];
                    let _ = stream.read(&mut buf).await;
                    let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                    let _ = stream.write_all(resp.as_bytes()).await;
                }
            }
        });

        let evil_read_url = format!("http://127.0.0.1:{}/d", port);
        let dead_write_url = format!("http://127.0.0.1:{}/sync.php", port);
        let srv_entry = ServerEntry {
            admin_pub,
            lock_pub,
            write_url: dead_write_url.clone(),
            read_url: evil_read_url.clone(),
            nominated_by: NominatedBy::Creator,
        };

        let mut cache = BTreeMap::new();
        let mut learned = Vec::new();
        // K1: without a verified 5-field delegation in cache or from ?info, resolve_verified_server_locator MUST be None
        assert!(resolve_verified_server_locator(&srv_entry, &cache).is_none());
        let client = sync_client::create_sync_probe_client();
        assert!(
            verify_and_cache_static_read_locator(&client, &srv_entry, &mut cache, &mut learned)
                .await
                .is_none(),
            "K1: 404 on unverified ?info/read_url must never populate verified locator"
        );
        assert!(learned.is_empty());

        // H9 part A: newer negative health probe overwrites stale healthy passed=true while preserving delegation & capability metadata
        let del_msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex,
            &lock_hex,
            &dead_write_url,
            &evil_read_url,
            1_760_000_000,
        );
        let del = sync_client::SyncLockDelegationDto {
            admin_pub: admin_hex.clone(),
            lock_pub: lock_hex.clone(),
            write_url: dead_write_url.clone(),
            read_url: evil_read_url.clone(),
            issued_at: 1_760_000_000,
            sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
        };
        let healthy = FrontendLockServerProbeResult {
            write_url: dead_write_url.clone(),
            read_url: evil_read_url.clone(),
            drop_pub: admin_hex.clone(),
            admin_pub: admin_hex.clone(),
            lock_pub: lock_hex.clone(),
            delegation_valid: true,
            waf_probe_passed: true,
            selftest_passed: true,
            key_leak_safe: true,
            passed: true,
            latency_ms: 10,
            max_retention_quarter: 8200,
            pow_base_bits: 12,
            allow_pow_tickets: true,
            ticket_pow_bits: 16,
            inode_warning: false,
            used_inodes: 1,
            max_inodes: 1000,
            probed_at_secs: 1_760_000_010,
            delegation: Some(del),
            error: None,
        };
        assert!(upsert_verified_lock_probe(&mut cache, healthy));

        let failed_later = FrontendLockServerProbeResult {
            write_url: dead_write_url.clone(),
            read_url: String::new(),
            drop_pub: String::new(),
            admin_pub: String::new(),
            lock_pub: lock_hex.clone(),
            delegation_valid: false,
            waf_probe_passed: false,
            selftest_passed: false,
            key_leak_safe: false,
            passed: false,
            latency_ms: 500,
            max_retention_quarter: 0,
            pow_base_bits: 0,
            allow_pow_tickets: false,
            ticket_pow_bits: 0,
            inode_warning: false,
            used_inodes: 0,
            max_inodes: 0,
            probed_at_secs: 1_760_000_100,
            delegation: None,
            error: Some("connection refused".into()),
        };
        assert!(upsert_verified_lock_probe(&mut cache, failed_later));
        let cached = cache.get(&lock_hex).unwrap();
        assert!(
            !cached.passed,
            "H9: newer failed probe must mark passed=false"
        );
        assert!(
            cached.delegation_valid,
            "H9: verified delegation must remain intact"
        );
        assert_eq!(cached.pow_base_bits, 12);
        assert_eq!(cached.error.as_deref(), Some("connection refused"));

        // H9 part B: build_lock_protection_overview_from_service excludes ServerOffenderList servers
        let mut lc = InternalLifecycleState {
            l2_probe_cache: BTreeMap::from([(
                lock_hex.clone(),
                FrontendLockServerProbeResult {
                    passed: true,
                    delegation_valid: true,
                    error: None,
                    ..cached.clone()
                },
            )]),
            ..Default::default()
        };
        let mut offenders = humoco_wot::ServerOffenderList::new();
        offenders.revoked_admin_pubs.insert(admin_pub);
        crate::l2_lifecycle::save_server_offender_list(&mut svc, &offenders, Some(pw)).unwrap();
        let state = AppState::new(svc);
        {
            let mut s = state.lock_service().unwrap();
            let policy = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            lc.pending_restore_l2_reconciliation = false;
            let overview = build_lock_protection_overview_from_service(
                &state,
                &mut s,
                &policy,
                &lc,
                None,
                None,
                Some(pw),
            )
            .unwrap();
            assert_eq!(
                overview.ready_trusted_count, 0,
                "H9: revoked server must not count as ready"
            );
        }

        stop.store(true, Ordering::Relaxed);
        let _ = srv.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_s05_f8_get_lock_protection_overview_succeeds_with_zero_timeout() {
        let dir = crate::test_support::temp_test_dir("s05_f8_zero_timeout");
        let pw = "pw-s05-f8";
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();
        let state = AppState::new(svc);
        {
            let mut s = state.lock_service().unwrap();
            let mut sp = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            sp.session_timeout_seconds = 0;
            state.save_security_policy(&mut s, sp, Some(pw)).unwrap();
            s.lock_session();
            state.initialize_profile_session(&mut s, pw).unwrap();
        }
        let overview = get_lock_protection_overview_impl(None, None, &state)
            .expect("S05-F8: get_lock_protection_overview_impl must succeed when timeout == 0 using ActiveProfileCaches");
        assert_eq!(overview.status, "none");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_s05_f12_ticket_auth_rejects_mismatched_admin_pub_on_same_url() {
        let dir = crate::test_support::temp_test_dir("s05_f12_endpoint_match");
        let pw = "pw-s05-f12";
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();
        let mut policy = SecurityPolicy::default();
        let expected_drop_pub = hex::encode([0xAAu8; 32]);
        let hijacked_admin_pub = hex::encode([0xBBu8; 32]);
        let hijacked_lock_pub = hex::encode([0xCCu8; 32]);
        policy.market_selected_syncs.push(
            humoco_market::DropDescriptor {
                write_url: "https://sync.example/sync.php".to_string(),
                read_url: "https://sync.example/d".to_string(),
                slot_id: None,
                drop_pub: expected_drop_pub,
            }
            .into(),
        );
        let probe = FrontendLockServerProbeResult {
            write_url: "https://sync.example/sync.php".to_string(),
            read_url: "https://sync.example/d".to_string(),
            drop_pub: hijacked_admin_pub.clone(),
            admin_pub: hijacked_admin_pub,
            lock_pub: hijacked_lock_pub,
            delegation_valid: true,
            waf_probe_passed: true,
            selftest_passed: true,
            key_leak_safe: true,
            passed: true,
            latency_ms: 10,
            max_retention_quarter: 8159,
            pow_base_bits: 8,
            allow_pow_tickets: false,
            ticket_pow_bits: 16,
            inode_warning: false,
            used_inodes: 0,
            max_inodes: 1000,
            probed_at_secs: 1_760_000_000,
            delegation: None,
            error: None,
        };
        let state = AppState::new(svc);
        let mut s = state.lock_service().unwrap();
        assert!(
            !has_usable_ticket_auth(&state, &mut s, &policy, &probe, Some(pw)).unwrap(),
            "S05-F12: mismatched non-empty admin_pub on same URL must NOT match circle sync"
        );
        assert!(
            resolve_ticket_auth_for_server(&mut s, &policy, &probe, Some(pw))
                .unwrap()
                .is_none(),
            "S05-F12: resolve_ticket_auth_for_server must NOT sign stealth auth for mismatched admin_pub"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_s05_f14_has_offline_cached_anchor_requires_preview_deletable_coverage() {
        use ed25519_dalek::{Signer, SigningKey};
        use human_money_core::humoco_proto::manifest::GenesisTicket;

        let dir = crate::test_support::temp_test_dir("s05_f14_preview_del");
        let pw = "pw-s05-f14";
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();

        let now_secs = sync_client::current_unix_timestamp();
        let short_ms = now_secs.saturating_add(30 * 86_400).saturating_mul(1000);
        let short_quarter = required_retention_quarter(short_ms).unwrap();

        let sk1 = SigningKey::from_bytes(&[131u8; 32]);
        let anchor_nonce = [132u8; 32];
        let creator_mig_pub = svc
            .with_wallet_and_identity(|_, id| {
                l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                    .map(|k| k.verifying_key().to_bytes())
            })
            .unwrap()
            .unwrap();
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: "https://short.example/sync.php".into(),
                read_url: "https://short.example/d".into(),
                admin_pub: sk1.verifying_key().to_bytes(),
                lock_pub: sk1.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: now_secs.saturating_mul(1000),
            retain_until_quarter: short_quarter,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let mut ticket = GenesisTicket {
            anchor_id,
            retain_until_quarter: short_quarter,
            issued_at: now_secs,
            lock_pub: sk1.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        ticket.sig = sk1.sign(&ticket.digest()).to_bytes();
        svc.register_l2_anchor_template_with_nonce(
            manifest,
            vec![ticket],
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();

        let state = AppState::new(svc);
        let mut s = state.lock_service().unwrap();
        let policy = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
        let lc = InternalLifecycleState::default();
        let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let overview = build_lock_protection_overview_from_service(
            &state,
            &mut s,
            &policy,
            &lc,
            Some(&std_toml),
            Some("P2Y"),
            Some(pw),
        )
        .unwrap();
        assert!(
            !overview.has_offline_cached_anchor,
            "S05-F14: anchor covering only 30 days must NOT report has_offline_cached_anchor for P2Y voucher"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_s05_f4_reconcile_restored_wallet_preserves_fork_or_stale_alert() {
        let dir = crate::test_support::temp_test_dir("s05_f4_atomic_restore");
        let pw = "pw-s05-f4";
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();
        let state = AppState::new(svc);
        let session = {
            let mut s = state.lock_service().unwrap();
            let mut lc = state
                .get_or_load_internal_lifecycle(&mut s, Some(pw))
                .unwrap();
            lc.pending_restore_l2_reconciliation = true;
            lc.wallet_sync_alert = Some(crate::settings::WalletSyncAlert {
                kind: "stale_local_backup".to_string(),
                mirror_write_url: "https://mirror.example/sync.php".to_string(),
                remote_epoch: Some(5),
                remote_tx_nonce: Some(12),
                local_epoch: 5,
                local_tx_nonce: 8,
                detected_at: "2026-11-21T12:00:00Z".to_string(),
                has_signed_ack_proof: true,
            });
            state.save_internal_lifecycle(&mut s, lc, Some(pw)).unwrap();
            state.session_token_for_service(&s).unwrap()
        };

        let cleared = reconcile_restored_wallet_if_pending(&state, &session, Some(pw))
            .await
            .unwrap();
        assert!(
            !cleared,
            "S05-F4: must NOT auto-clear pending_restore_l2_reconciliation while stale_local_backup alert is active"
        );
        let lc_after = {
            let mut s = state.lock_service().unwrap();
            state
                .get_or_load_internal_lifecycle(&mut s, Some(pw))
                .unwrap()
        };
        assert!(lc_after.pending_restore_l2_reconciliation);
        assert_eq!(
            lc_after.wallet_sync_alert.as_ref().map(|a| a.kind.as_str()),
            Some("stale_local_backup"),
            "S05-F4: must preserve wallet_sync_alert"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_s05_f5_check_voucher_l2_status_discovers_missing_manifest() {
        use ed25519_dalek::{Signer, SigningKey};
        use human_money_core::humoco_proto::encode_quarter;
        use human_money_core::models::profile::PublicProfile;
        use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = crate::test_support::temp_test_dir("s05_f5_discover_missing");
        let pw = "pw-s05-f5";
        let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let mut svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        svc.unlock_session(pw, 600).unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://127.0.0.1:{}", addr.port());
        let write_url = format!("{base_url}/sync.php");
        let read_url = format!("{base_url}/d");

        let admin_sk = SigningKey::from_bytes(&[171u8; 32]);
        let lock_sk = SigningKey::from_bytes(&[172u8; 32]);
        let admin_pub = admin_sk.verifying_key().to_bytes();
        let lock_pub = lock_sk.verifying_key().to_bytes();
        let anchor_nonce = [173u8; 32];
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
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: write_url.clone(),
                read_url: read_url.clone(),
                admin_pub,
                lock_pub,
                nominated_by: NominatedBy::Creator,
            }],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub,
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let anchor_hex = hex::encode(anchor_id);

        let my_id = svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        // Intentionally do NOT register the manifest locally so disk & memory both lack it
        svc.create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(my_id),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "25".into(),
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

        let admin_hex = hex::encode(admin_pub);
        let lock_hex = hex::encode(lock_pub);
        let del_msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex,
            &lock_hex,
            &write_url,
            &read_url,
            1_760_000_000,
        );
        let del_sig = hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes());
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let info_body = serde_json::json!({
            "version": 1,
            "drop_pub": admin_hex,
            "read_url": read_url,
            "channels": ["card", "wot", "backup", "post", "lock"],
            "lock": {
                "lock_pub": lock_hex,
                "max_retention_quarter": retain_q,
                "pow_base_bits": 8,
                "allow_pow_tickets": true,
                "ticket_pow_bits": 16,
                "delegation": {
                    "admin_pub": admin_hex,
                    "lock_pub": lock_hex,
                    "write_url": write_url,
                    "read_url": read_url,
                    "issued_at": 1_760_000_000u64,
                    "sig": del_sig
                }
            }
        })
        .to_string();

        let stop = Arc::new(AtomicBool::new(false));
        let stop_c = stop.clone();
        let manifest_path = format!("/anchors/{anchor_hex}.bin");
        let srv = tokio::spawn(async move {
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
                if first_line.contains(&manifest_path) {
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        manifest_bytes.len()
                    );
                    let _ = stream.write_all(hdr.as_bytes()).await;
                    let _ = stream.write_all(&manifest_bytes).await;
                } else if first_line.contains("?info") {
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        info_body.len(),
                        info_body
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                } else {
                    let _ = stream
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                }
            }
        });

        let state = AppState::new(svc);
        {
            let mut s = state.lock_service().unwrap();
            let mut sp = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
            sp.market_selected_syncs.push(
                humoco_market::DropDescriptor {
                    write_url: write_url.clone(),
                    read_url: read_url.clone(),
                    slot_id: None,
                    drop_pub: admin_hex,
                }
                .into(),
            );
            state.save_security_policy(&mut s, sp, Some(pw)).unwrap();
        }

        let dto = check_voucher_l2_status_impl(local_id, Some(pw.to_string()), &state)
            .await
            .expect("S05-F5: check_voucher_l2_status_impl must discover missing manifest instead of failing");
        assert_eq!(dto.status, "unspent");
        assert_eq!(dto.manifest_revision, Some(1));
        assert!(!dto.reduced_guarantor_protection);

        stop.store(true, Ordering::Relaxed);
        let _ = srv.await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
