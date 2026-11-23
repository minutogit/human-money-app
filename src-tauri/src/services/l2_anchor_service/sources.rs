use super::*;
use std::collections::{BTreeMap, HashMap, HashSet};

use human_money_core::humoco_proto::{
    decode_hex_fixed,
    manifest::{AnchorManifest, NominatedBy, ServerEntry},
    quarter_of_timestamp_ms, retention_covers,
};

use crate::{
    gateway_resolver::{sybil_fold_committee, SybilServerCandidate},
    models::FrontendError,
    services::common::{get_wallet_master_seed, normalize_sync_url},
    settings::{is_lock_admin_trusted, InternalLifecycleState, SecurityPolicy},
    sync_client::{FrontendLockServerProbeResult, LockTicketAuth},
    AppState,
};

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

pub(super) fn has_usable_ticket_auth(
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

pub(super) fn required_retention_quarter(deletable_ms: u64) -> Result<u16, FrontendError> {
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

pub(super) fn estimate_deletable_ms(
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
pub(super) fn select_expected_candidates_from_lifecycle(
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

pub(super) fn find_reusable_offline_anchor(
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
