use super::overview::probe_known_sync_sources_into_cache;
use super::sources::*;
use super::*;
use std::collections::HashMap;
use std::time::Duration;

use human_money_core::humoco_proto::manifest::{AnchorAuthority, AnchorManifest, ServerEntry};
use human_money_core::services::l2_gateway;
use rand::RngCore;

use crate::{
    models::FrontendError,
    services::common::normalize_sync_url,
    sync_client::{self, FrontendLockServerProbeResult, LockTicketAuth},
    AppState,
};

pub(crate) struct AssembledAutoCommittee {
    pub anchor_id: [u8; 32],
    pub lock_pubs: Vec<String>,
    pub threshold: u8,
    pub source: &'static str, // "live" | "cached"
}

pub(super) struct AutoCommitteeSpec {
    creator_mig_pub: [u8; 32],
    anchor_nonce: [u8; 32],
    retain_until_quarter: u16,
}

pub(super) async fn mint_and_register_committee(
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
                    .map_err(|_| {
                        sync_client::SyncClientError::Transport(
                            "GenesisTicket request timeout".to_string(),
                        )
                    })
                    .and_then(|r| r);

                    if let Err(ref sync_err) = res {
                        let is_auth_reject = matches!(sync_err.http_status(), Some(401 | 403));
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
                            .map_err(|_| {
                                sync_client::SyncClientError::Transport(
                                    "GenesisTicket PoW fallback timeout".to_string(),
                                )
                            })
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
                    Ok((_, srv, _, _, _, Err(sync_err))) => {
                        had_failure = true;
                        failed_servers.push((hex::encode(srv.lock_pub), sync_err.to_string()));
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

pub(super) async fn gather_committee_candidates(
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
