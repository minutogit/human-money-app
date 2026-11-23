use super::committee::*;
use super::sources::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use human_money_core::humoco_proto::decode_hex_fixed;

use crate::{
    models::{FrontendError, FrontendLockProtectionOverviewDto, FrontendLockServerItemDto},
    services::common::{normalize_sync_url, upsert_verified_lock_probe},
    settings::{is_lock_admin_trusted, InternalLifecycleState, SecurityPolicy},
    sync_client::{self, FrontendLockServerProbeResult},
    AppState,
};

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

pub(super) async fn probe_known_sync_sources_into_cache(
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
        if !service.is_wallet_unlocked() || !service.is_session_active() {
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
