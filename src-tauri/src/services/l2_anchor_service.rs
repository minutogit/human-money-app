use std::collections::HashSet;

use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    decode_hex_fixed,
    manifest::{AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry, MAX_GUARANTORS},
    parse_quarter,
};
use human_money_core::models::layer2_api::{
    LockEntry, ObservedSlotState, VoucherReconciliationResult,
};
use human_money_core::wallet::reconciliation::VoucherLockReadTarget;
use human_money_core::services::l2_gateway;
use rand::RngCore;

use crate::{
    gateway_resolver::{extract_registrable_domain, sybil_fold_committee, SybilServerCandidate},
    models::{
        AnchorServerInputDto, FrontendError, FrontendL2LockStatusDto, FrontendVoucherL2StatusDto,
    },
    services::common::{max_pow_base_bits_for_servers, normalize_sync_url},
    settings::{NominatedLockServerMeta, StoredAnchorServerMeta, StoredAnchorTemplateMeta},
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
        match sync_client::fetch_static_lock_entry(client, &srv.read_url, rel_path).await {
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
                res =
                    service.reconcile_voucher_state(local_id, observed, None, pow_bits, password);
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

fn parse_asn_u32(raw: Option<&str>) -> Option<u32> {
    raw.map(str::trim).filter(|s| !s.is_empty()).and_then(|s| {
        s.trim_start_matches("AS")
            .trim_start_matches("as")
            .parse::<u32>()
            .ok()
    })
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

pub(crate) async fn probe_lock_server_cmd_impl(
    write_url: String,
    expected_admin_pub: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendLockServerProbeResult, FrontendError> {
    let session = state.authorize_and_begin_session(crate::state::RequiredAccess::Unlocked, None)?;
    let client = sync_client::create_sync_probe_client();
    let mut result = sync_client::probe_lock_server(
        &client,
        &write_url,
        expected_admin_pub
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .await;

    let norm_url = normalize_sync_url(&result.write_url);
    if !norm_url.is_empty() {
        let offenders = state.with_session_required(
            &session,
            "Active wallet session changed before probe could be validated",
            |service| {
                crate::l2_lifecycle::load_server_offender_list(service, password.as_deref())
            },
        )?;
        if let Ok(lock_pk) = decode_hex_fixed::<32>(&result.lock_pub) {
            let admin_pk = decode_hex_fixed::<32>(&result.admin_pub).ok();
            if crate::l2_lifecycle::is_server_key_revoked(&offenders, &lock_pk, admin_pk.as_ref()) {
                result.passed = false;
                result.error = Some(
                    "Server key or operator key is permanently revoked on ServerOffenderList"
                        .to_string(),
                );
            }
        }
        state.mutate_internal_lifecycle_for_session(&session, password.as_deref(), |lc| {
            lc.l2_probe_cache.insert(norm_url.clone(), result.clone());
            true
        })?;
    }

    Ok(result)
}

pub(crate) fn set_own_nominated_lock_server_impl(
    write_url: Option<String>,
    asn: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<Option<NominatedLockServerMeta>, FrontendError> {
    let mut service =
        state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
    let lifecycle = state
        .get_or_load_internal_lifecycle(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let updated = match write_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(url) => {
            sync_client::validate_sync_url(url).map_err(FrontendError::from)?;
            let norm = normalize_sync_url(url);
            let probe = lifecycle
                .l2_probe_cache
                .get(&norm)
                .filter(|p| p.passed)
                .ok_or_else(|| {
                    FrontendError::from(
                        "Server muss zuerst erfolgreich extern geprüft werden (?info, Probe-POST, ?lock_selftest, Kein Key-Leak)."
                            .to_string(),
                    )
                })?;

            let lock_pk = decode_hex_fixed::<32>(&probe.lock_pub)
                .map_err(|e| FrontendError::from(e.to_string()))?;
            let admin_pk = decode_hex_fixed::<32>(&probe.admin_pub).ok();
            let offenders =
                crate::l2_lifecycle::load_server_offender_list(&mut service, password.as_deref())?;
            if crate::l2_lifecycle::is_server_key_revoked(&offenders, &lock_pk, admin_pk.as_ref()) {
                return Err(FrontendError::from(
                    "Server steht auf der lokalen Server-Sanktionsliste (ServerOffenderList) und kann nicht gewählt werden."
                        .to_string(),
                ));
            }

            Some(NominatedLockServerMeta {
                write_url: probe.write_url.clone(),
                read_url: probe.read_url.clone(),
                drop_pub: probe.drop_pub.clone(),
                admin_pub: probe.admin_pub.clone(),
                lock_pub: probe.lock_pub.clone(),
                asn: asn.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                probed_at_secs: probe.probed_at_secs,
            })
        }
    };

    state
        .mutate_security_policy(&mut service, password.as_deref(), |policy| {
            policy.own_nominated_lock_server = updated.clone();
        })
        .map_err(FrontendError::from)?;
    Ok(updated)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn assemble_anchor_template_impl(
    label: String,
    retain_until_quarter_str: String,
    servers: Vec<AnchorServerInputDto>,
    guarantor_mig_pubs_hex: Vec<String>,
    threshold: Option<u16>,
    password: Option<String>,
    state: &AppState,
) -> Result<StoredAnchorTemplateMeta, FrontendError> {
    let (session, lifecycle, offenders, creator_mig_pub, signing_key_clone, anchor_nonce) = {
        let mut service =
            state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
        if servers.is_empty() || servers.len() > human_money_core::humoco_proto::MAX_COMMITTEE_SIZE
        {
            return Err(FrontendError::from(
                "Anker-Vorlage benötigt 1 bis 5 geprüfte Lock-Server.".to_string(),
            ));
        }
        if guarantor_mig_pubs_hex.len() > MAX_GUARANTORS {
            return Err(FrontendError::from(
                "Too many guarantor authority keys".to_string(),
            ));
        }
        let session = crate::session::SessionToken {
            generation: state.current_session_generation(),
            owner_did: service
                .with_wallet(|w| w.get_user_id().to_string())
                .map_err(FrontendError::from)?,
        };
        let lifecycle = state
            .get_or_load_internal_lifecycle(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let offenders =
            crate::l2_lifecycle::load_server_offender_list(&mut service, password.as_deref())?;
        let mut anchor_nonce = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut anchor_nonce);
        let (creator_mig_pub, signing_key_clone) = service
            .with_wallet_and_identity(|_, id| {
                let mig_sk = l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)?;
                Ok::<_, human_money_core::Error>((
                    mig_sk.verifying_key().to_bytes(),
                    id.signing_key.clone(),
                ))
            })
            .map_err(FrontendError::from)?
            .map_err(FrontendError::from)?;
        (
            session,
            lifecycle,
            offenders,
            creator_mig_pub,
            signing_key_clone,
            anchor_nonce,
        )
    };

    let retain_until_quarter = parse_quarter(retain_until_quarter_str.trim())
        .map_err(|e| FrontendError::from(format!("Invalid retain_until_quarter: {}", e)))?;

    let mut server_entries = Vec::with_capacity(servers.len());
    let mut stored_metas = Vec::with_capacity(servers.len());
    let mut sybil_candidates = Vec::with_capacity(servers.len());
    let mut allow_pow_flags = Vec::with_capacity(servers.len());

    for input in &servers {
        let norm = normalize_sync_url(&input.write_url);
        let probe = lifecycle
            .l2_probe_cache
            .get(&norm)
            .filter(|p| p.passed)
            .ok_or_else(|| {
                FrontendError::from(format!(
                    "Lock-Server '{}' ist nicht erfolgreich geprüft und kann nicht gewählt werden.",
                    input.write_url
                ))
            })?;

        if retain_until_quarter > probe.max_retention_quarter {
            return Err(FrontendError::from(format!(
                "Server '{}' erlaubt maximal Aufbewahrung bis Quartal {}, gewählt wurde {}.",
                probe.write_url, probe.max_retention_quarter, retain_until_quarter_str
            )));
        }

        let admin_pub = decode_hex_fixed::<32>(&probe.admin_pub).map_err(|e| {
            FrontendError::from(format!("Invalid server admin_pub in probe cache: {}", e))
        })?;
        let lock_pub = decode_hex_fixed::<32>(&probe.lock_pub).map_err(|e| {
            FrontendError::from(format!("Invalid server lock_pub in probe cache: {}", e))
        })?;
        if crate::l2_lifecycle::is_server_key_revoked(&offenders, &lock_pub, Some(&admin_pub)) {
            return Err(FrontendError::from(format!(
                "Lock-Server '{}' steht auf der lokalen Sanktionsliste (ServerOffenderList).",
                probe.write_url
            )));
        }

        let nominated_by = match input.nominated_by.trim().to_ascii_lowercase().as_str() {
            "guarantor" => {
                let idx = input.guarantor_index.unwrap_or(0);
                NominatedBy::Guarantor(idx)
            }
            _ => NominatedBy::Creator,
        };

        let srv_entry = ServerEntry {
            write_url: probe.write_url.clone(),
            read_url: probe.read_url.clone(),
            admin_pub,
            lock_pub,
            nominated_by,
        };
        server_entries.push(srv_entry);
        allow_pow_flags.push(probe.allow_pow_tickets);

        let domain = extract_registrable_domain(&probe.write_url);
        sybil_candidates.push(SybilServerCandidate {
            write_url: probe.write_url.clone(),
            lock_pub,
            admin_pub: Some(admin_pub),
            asn: parse_asn_u32(input.asn.as_deref()),
        });

        stored_metas.push(StoredAnchorServerMeta {
            write_url: probe.write_url.clone(),
            read_url: probe.read_url.clone(),
            drop_pub: probe.drop_pub.clone(),
            admin_pub: probe.admin_pub.clone(),
            lock_pub: probe.lock_pub.clone(),
            nominated_by: match nominated_by {
                NominatedBy::Creator => "creator".to_string(),
                NominatedBy::Guarantor(i) => format!("guarantor:{}", i),
            },
            operator_domain: domain,
            asn: input.asn.clone().filter(|s| !s.trim().is_empty()),
            info_fetched_at_secs: probe.probed_at_secs,
            max_retention_quarter: probe.max_retention_quarter,
            pow_base_bits: probe.pow_base_bits,
        });
    }

    let folded = sybil_fold_committee(&sybil_candidates);
    if folded.len() < servers.len() {
        return Err(FrontendError::from(
            "Sybil-Warnung: Mindestens zwei gewählte Lock-Server teilen sich Lock-Schlüssel, Admin-Schlüssel, Domain oder ASN."
                .to_string(),
        ));
    }

    let mut guarantor_mig_pubs = Vec::with_capacity(guarantor_mig_pubs_hex.len());
    for g_hex in &guarantor_mig_pubs_hex {
        let g = decode_hex_fixed::<32>(g_hex.trim()).map_err(|e| {
            FrontendError::from(format!("Invalid guarantor migration pubkey: {}", e))
        })?;
        guarantor_mig_pubs.push(g);
    }

    let n = server_entries.len() as u16;
    let min_q = (n / 2) + 1;
    let q = threshold.unwrap_or(min_q).max(min_q).min(n);

    let now_secs = sync_client::current_unix_timestamp();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: q as u8,
        servers: server_entries,
        successors: Vec::new(),
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs,
        },
        migration_chain: Vec::new(),
        issued_at_ms: now_secs.saturating_mul(1000),
        retain_until_quarter,
    };

    let anchor_id = manifest
        .rev1_anchor_id()
        .map_err(|e| FrontendError::from(format!("Invalid AnchorManifest: {}", e)))?;

    let client = sync_client::create_sync_http_client();
    let mut ticket_set = tokio::task::JoinSet::new();
    for (srv, &allow_pow) in manifest.servers.iter().zip(allow_pow_flags.iter()) {
        let c = client.clone();
        let wurl = srv.write_url.clone();
        let sk_ref = if allow_pow {
            None
        } else {
            Some(signing_key_clone.clone())
        };
        ticket_set.spawn(async move {
            sync_client::request_genesis_ticket(
                &c,
                &wurl,
                &anchor_id,
                retain_until_quarter,
                None,
                sk_ref.as_ref(),
            )
            .await
        });
    }

    let mut tickets = Vec::with_capacity(manifest.servers.len());
    while let Some(joined) = ticket_set.join_next().await {
        let res = joined.map_err(|e| FrontendError::from(e.to_string()))?;
        tickets.push(res.map_err(FrontendError::from)?);
    }

    state.with_session_required(
        &session,
        "Active wallet changed before anchor template could be saved",
        |service| {
            service
                .register_l2_anchor_template_with_nonce(
                    manifest,
                    tickets,
                    Some(&anchor_nonce),
                    password.as_deref(),
                )
                .map_err(FrontendError::from)
        },
    )?;

    let anchor_hex = hex::encode(anchor_id);
    let tmpl_meta = StoredAnchorTemplateMeta {
        anchor_id: anchor_hex.clone(),
        label: if label.trim().is_empty() {
            format!("Sync-Anker ({}/{})", q, n)
        } else {
            label.trim().to_string()
        },
        threshold: q as u8,
        retain_until_quarter,
        created_at_secs: now_secs,
        servers: stored_metas,
    };

    state.mutate_internal_lifecycle_for_session(&session, password.as_deref(), |lc| {
        lc.l2_anchor_templates_meta
            .retain(|t| t.anchor_id != anchor_hex);
        lc.l2_anchor_templates_meta.push(tmpl_meta.clone());
        true
    })?;

    Ok(tmpl_meta)
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
    if let Some(ref own) = policy.own_nominated_lock_server {
        urls.push(own.write_url.clone());
    }
    for t in &lifecycle.l2_anchor_templates_meta {
        for srv in &t.servers {
            urls.push(srv.write_url.clone());
        }
    }
    let client = sync_client::create_sync_probe_client();
    crate::gateway_resolver::prewarm_lock_urls(&client, &urls).await;
    Ok(())
}

pub(crate) fn check_l2_lock_status_impl(
    state: &AppState,
) -> Result<FrontendL2LockStatusDto, FrontendError> {
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    let policy = state.get_or_load_security_policy_locked(None)?;
    let lifecycle = state.get_or_load_internal_lifecycle_locked(None)?;
    let unresolved_intents_count = {
        let service = state.lock_service()?;
        service
            .unresolved_l2_send_intents()
            .map(|v| v.len())
            .unwrap_or(0)
    };

    let now_secs = sync_client::current_unix_timestamp();
    let max_age_limit_secs = crate::settings::SYNC_BASE_MAX_AGE_SECS;
    let mut stale_servers_count = 0usize;
    let mut max_probe_age_secs: Option<u64> = None;

    for probe in lifecycle.l2_probe_cache.values() {
        let age = now_secs.saturating_sub(probe.probed_at_secs);
        max_probe_age_secs = Some(max_probe_age_secs.map(|m| m.max(age)).unwrap_or(age));
        if age > max_age_limit_secs || !probe.passed || probe.inode_warning {
            stale_servers_count += 1;
        }
    }

    let templates_count = lifecycle.l2_anchor_templates_meta.len();
    let has_own_nominated_server = policy.own_nominated_lock_server.is_some();
    let has_any_sync = policy
        .public_syncs
        .iter()
        .any(|s| s.enabled && s.status != "revoked");

    let status = if lifecycle.pending_restore_l2_reconciliation || unresolved_intents_count > 0 {
        "Syncing".to_string()
    } else if templates_count == 0 && !has_own_nominated_server && !has_any_sync {
        "NoServers".to_string()
    } else if stale_servers_count > 0 {
        "Degraded".to_string()
    } else {
        "Ok".to_string()
    };

    Ok(FrontendL2LockStatusDto {
        status,
        unresolved_intents_count,
        pending_restore_reconciliation: lifecycle.pending_restore_l2_reconciliation,
        templates_count,
        has_own_nominated_server,
        stale_servers_count,
        max_probe_age_secs,
    })
}

pub(crate) async fn reconcile_vouchers_via_static_locks(
    state: &AppState,
    filter_ids: Option<&[String]>,
    password: Option<&str>,
) -> Result<StaticReconcileSummary, FrontendError> {
    let filter_set: Option<HashSet<&str>> =
        filter_ids.map(|ids| ids.iter().map(|s| s.as_str()).collect());

    let (session, targets, missing_manifest_ids) = {
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
                                target
                                    .server_paths
                                    .iter()
                                    .map(|(srv, _)| srv.write_url.as_str()),
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
        (session, targets, missing_manifest_ids)
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
        let probe = probe_voucher_static_lock(&client, &target).await;
        if probe.any_server_answered {
            any_committee_reachable = true;
        }

        let outcome = state.with_session_required(
            &session,
            "Wallet session changed during static L2 reconciliation",
            |service| {
                reconcile_static_probe_on_service(
                    service,
                    &local_id,
                    &probe,
                    pow_bits,
                    true,
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
                    log::debug!("Static lock reconciliation inconclusive for {}: {}", local_id, reason);
                    inconclusive_count += 1;
                }
            },
            Ok(StaticVoucherReconcileOutcome::Inconclusive { reason, .. }) => {
                log::debug!("Static lock probe inconclusive for {}: {}", local_id, reason);
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

    let (target_opt, is_currently_archived, pow_bits, reduced_protection, manifest_rev) = state
        .with_session_required(
            &session,
            "Wallet session changed before checking voucher L2 status",
            |service| {
                let lifecycle = state
                    .get_or_load_internal_lifecycle(service, password.as_deref())
                    .map_err(FrontendError::from)?;
                service
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
                        let target = wallet.build_voucher_read_targets(
                            identity,
                            &local_instance_id,
                            None,
                        )?;
                        let bits = max_pow_base_bits_for_servers(
                            &lifecycle,
                            target
                                .server_paths
                                .iter()
                                .map(|(srv, _)| srv.write_url.as_str()),
                        );
                        Ok((Some(target), is_archived, bits, reduced, rev))
                    })
                    .map_err(FrontendError::from)?
                    .map_err(FrontendError::from)
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
    let probe = probe_voucher_static_lock(&client, &target).await;

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
}
