use std::collections::HashSet;

use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    decode_hex_fixed,
    manifest::{AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry, MAX_GUARANTORS},
    parse_quarter,
};
use human_money_core::models::layer2_api::VoucherReconciliationResult;
use human_money_core::services::l2_gateway;
use log::{error, info};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::{
    gateway_resolver::{extract_registrable_domain, sybil_fold_committee, SybilServerCandidate},
    models::{
        FrontendError, FrontendRollbackReconciliationDto, FrontendVoucherL2StatusDto,
        SealUploadData,
    },
    settings::{
        AppSettings, NominatedLockServerMeta, StoredAnchorServerMeta, StoredAnchorTemplateMeta,
    },
    sync_client::{self, FrontendLockServerProbeResult},
    AppState,
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReachPresetDto {
    pub preset: String,
    pub own_list_max_hops: u8,
    pub card_max_hops: u8,
    pub share_min_level: u8,
    pub media_auto_load_max_hops: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnchorServerInputDto {
    pub write_url: String,
    pub nominated_by: String, // "creator" | "guarantor"
    #[serde(default)]
    pub guarantor_index: Option<u8>,
    #[serde(default)]
    pub asn: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendL2LockStatusDto {
    pub status: String, // "Ok" | "Syncing" | "NoServers" | "Degraded"
    pub unresolved_intents_count: usize,
    pub pending_restore_reconciliation: bool,
    pub templates_count: usize,
    pub has_own_nominated_server: bool,
    pub stale_servers_count: usize,
    pub max_probe_age_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct StaticReconcileSummary {
    pub active_anchored_count: usize,
    pub reconciled_spent_count: usize,
    pub spent_voucher_ids: Vec<String>,
    pub all_responded: bool,
    pub status: String, // "no_servers" | "offline" | "reconciled"
}

fn parse_asn_u32(raw: Option<&str>) -> Option<u32> {
    raw.map(str::trim).filter(|s| !s.is_empty()).and_then(|s| {
        s.trim_start_matches("AS")
            .trim_start_matches("as")
            .parse::<u32>()
            .ok()
    })
}

pub fn preset_values(preset: &str) -> Result<(u8, u8, u8, u8), FrontendError> {
    match preset {
        "near" => Ok((1, 2, 1, 1)),
        "medium" => Ok((2, 1, 2, 2)),
        "far" => Ok((3, 1, 3, 2)),
        _ => Err(FrontendError::from(format!(
            "Invalid reach preset: {}",
            preset
        ))),
    }
}

pub fn detect_reach_preset(
    own_list_max_hops: u8,
    share_min_level: u8,
    card_max_hops: u8,
    media_auto_load_max_hops: u8,
) -> String {
    for preset in ["near", "medium", "far"] {
        if let Ok(expected) = preset_values(preset) {
            if expected
                == (
                    own_list_max_hops,
                    share_min_level,
                    card_max_hops,
                    media_auto_load_max_hops,
                )
            {
                return preset.to_string();
            }
        }
    }
    "custom".to_string()
}

pub fn get_reach_preset_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<ReachPresetDto, FrontendError> {
    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let media_auto_load_max_hops = settings.media_auto_load_max_hops.min(3);
    let share_min_level = settings.share_min_level.min(3);

    let (own_list_max_hops, card_max_hops) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let reg = state
            .get_or_load_wot_register(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let card_hops =
            crate::commands::market::get_own_card_max_hops_inner(&mut service, password.as_deref());
        (reg.policy.own_list_max_hops.clamp(1, 3), card_hops)
    };

    let preset = detect_reach_preset(
        own_list_max_hops,
        share_min_level,
        card_max_hops,
        media_auto_load_max_hops,
    );

    Ok(ReachPresetDto {
        preset,
        own_list_max_hops,
        card_max_hops,
        share_min_level,
        media_auto_load_max_hops,
    })
}

#[tauri::command]
pub fn get_reach_preset(
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<ReachPresetDto, FrontendError> {
    get_reach_preset_impl(password, &state)
}

pub fn apply_reach_preset_impl(
    preset: String,
    password: Option<String>,
    state: &AppState,
) -> Result<ReachPresetDto, FrontendError> {
    let (own_list_max_hops, share_min_level, card_max_hops, media_auto_load_max_hops) =
        preset_values(&preset)?;

    // Called before locking state.service because set_wot_max_hops_impl acquires it internally.
    crate::commands::wot::set_wot_max_hops_impl(own_list_max_hops, password.clone(), state)?;

    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        crate::commands::market::set_own_card_max_hops_inner(
            &mut service,
            card_max_hops,
            password.as_deref(),
        )?;

        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        settings.share_min_level = share_min_level;
        settings.media_auto_load_max_hops = media_auto_load_max_hops;
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    Ok(ReachPresetDto {
        preset,
        own_list_max_hops,
        card_max_hops,
        share_min_level,
        media_auto_load_max_hops,
    })
}

#[tauri::command]
pub fn apply_reach_preset(
    preset: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<ReachPresetDto, FrontendError> {
    apply_reach_preset_impl(preset, password, &state)
}

#[tauri::command]
pub fn get_app_settings(state: tauri::State<AppState>) -> Result<AppSettings, FrontendError> {
    info!("Getting app settings from cache...");
    state.get_cached_settings().map_err(FrontendError::from)
}

#[tauri::command]
pub async fn save_app_settings(
    settings: AppSettings,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), FrontendError> {
    let mut settings = settings;
    settings.media_auto_load_max_hops = settings.media_auto_load_max_hops.min(3);
    settings.share_min_level = settings.share_min_level.min(3);
    info!("Saving app settings to disk...");
    let mut service = state.service.lock().unwrap();
    state
        .save_settings(&mut service, settings, password.as_deref())
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn set_conflict_local_override(
    proof_id: String,
    value: bool,
    note: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    info!(
        "Setting local override for conflict {} to {} with note: {:?}",
        proof_id, value, note
    );
    let mut service = state.service.lock().unwrap();

    if let Some(ref pwd) = password {
        if let Err(e) = service.unlock_session(pwd, 300) {
            error!("Failed to unlock session for local override: {}", e);
        }
    }

    service
        .set_conflict_local_override(&proof_id, value, note, password.as_deref())
        .map_err(FrontendError::from)
}

// --- WalletSeal / Sync Commands ---

#[tauri::command]
pub fn get_seal_sync_status(state: tauri::State<AppState>) -> Result<String, FrontendError> {
    let service = state.service.lock().unwrap();
    let status = service
        .get_seal_sync_status()
        .map_err(FrontendError::from)?;
    let status_str = match status {
        human_money_core::models::seal::SyncStatus::PendingUpload => "PendingUpload",
        human_money_core::models::seal::SyncStatus::Synced => "Synced",
    };
    Ok(status_str.to_string())
}

#[tauri::command]
pub fn get_seal_for_upload(
    state: tauri::State<AppState>,
) -> Result<Option<SealUploadData>, FrontendError> {
    let service = state.service.lock().unwrap();

    let data = service.get_seal_for_upload().map_err(FrontendError::from)?;
    if let Some(bytes) = data {
        let seal: human_money_core::models::seal::WalletSeal = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse seal for hashing: {}", e)))?;

        let hash = seal.compute_hash().map_err(FrontendError::from)?;

        Ok(Some(SealUploadData {
            seal_bytes: bytes,
            seal_hash: hash,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn acknowledge_seal_sync(
    uploaded_seal_hash: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();
    service
        .acknowledge_seal_sync(&uploaded_seal_hash, password.as_deref())
        .map_err(FrontendError::from)
}

// --- Sync Lock Server Probing, Anchor Templates & Static Lock Reconciliation ---

#[tauri::command]
pub async fn probe_lock_server_cmd(
    write_url: String,
    expected_admin_pub: Option<String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendLockServerProbeResult, FrontendError> {
    let client = sync_client::create_sync_probe_client();
    let result = sync_client::probe_lock_server(
        &client,
        &write_url,
        expected_admin_pub
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
    )
    .await;

    let norm_url = crate::checkout::normalize_sync_write_url(&result.write_url);
    if !norm_url.is_empty() {
        let mut settings = state.get_cached_settings().unwrap_or_default();
        settings.l2_probe_cache.insert(norm_url, result.clone());
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = state.save_settings(&mut service, settings, password.as_deref());
    }

    Ok(result)
}

#[tauri::command]
pub fn set_own_nominated_lock_server(
    write_url: Option<String>,
    asn: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Option<NominatedLockServerMeta>, FrontendError> {
    let mut settings = state.get_cached_settings().unwrap_or_default();

    let updated = match write_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(url) => {
            sync_client::validate_sync_url(url).map_err(FrontendError::from)?;
            let norm = crate::checkout::normalize_sync_write_url(url);
            let probe = settings
                .l2_probe_cache
                .get(&norm)
                .filter(|p| p.passed)
                .ok_or_else(|| {
                    FrontendError::from(
                        "Server muss zuerst erfolgreich extern geprüft werden (?info, Probe-POST, ?lock_selftest, Kein Key-Leak)."
                            .to_string(),
                    )
                })?;

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

    settings.own_nominated_lock_server = updated.clone();
    let mut service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    state
        .save_settings(&mut service, settings, password.as_deref())
        .map_err(FrontendError::from)?;
    Ok(updated)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn assemble_anchor_template(
    label: String,
    retain_until_quarter_str: String,
    servers: Vec<AnchorServerInputDto>,
    guarantor_mig_pubs_hex: Vec<String>,
    threshold: Option<u16>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<StoredAnchorTemplateMeta, FrontendError> {
    if servers.is_empty() || servers.len() > human_money_core::humoco_proto::MAX_COMMITTEE_SIZE {
        return Err(FrontendError::from(
            "Anker-Vorlage benötigt 1 bis 5 geprüfte Lock-Server.".to_string(),
        ));
    }
    if guarantor_mig_pubs_hex.len() > MAX_GUARANTORS {
        return Err(FrontendError::from(
            "Too many guarantor authority keys".to_string(),
        ));
    }

    let retain_until_quarter = parse_quarter(retain_until_quarter_str.trim())
        .map_err(|e| FrontendError::from(format!("Invalid retain_until_quarter: {}", e)))?;

    let settings = state.get_cached_settings().unwrap_or_default();
    let mut server_entries = Vec::with_capacity(servers.len());
    let mut stored_metas = Vec::with_capacity(servers.len());
    let mut sybil_candidates = Vec::with_capacity(servers.len());
    let mut allow_pow_flags = Vec::with_capacity(servers.len());

    for input in &servers {
        let norm = crate::checkout::normalize_sync_write_url(&input.write_url);
        let probe = settings
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

    let mut anchor_nonce = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut anchor_nonce);

    let (creator_mig_pub, signing_key_clone) = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .with_wallet_and_identity(|_, id| {
                let mig_sk = l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)?;
                Ok::<_, human_money_core::Error>((
                    mig_sk.verifying_key().to_bytes(),
                    id.signing_key.clone(),
                ))
            })
            .map_err(FrontendError::from)?
            .map_err(FrontendError::from)?
    };

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

    {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .register_l2_anchor_template_with_nonce(
                manifest,
                tickets,
                Some(&anchor_nonce),
                password.as_deref(),
            )
            .map_err(FrontendError::from)?;
    }

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

    {
        let mut updated_settings = state.get_cached_settings().unwrap_or_default();
        updated_settings
            .l2_anchor_templates_meta
            .retain(|t| t.anchor_id != anchor_hex);
        updated_settings
            .l2_anchor_templates_meta
            .push(tmpl_meta.clone());
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        state
            .save_settings(&mut service, updated_settings, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    Ok(tmpl_meta)
}

#[tauri::command]
pub async fn prewarm_l2_checkout(state: tauri::State<'_, AppState>) -> Result<(), FrontendError> {
    let Ok(settings) = state.get_cached_settings() else {
        return Ok(());
    };
    let mut urls = Vec::new();
    for s in &settings.public_syncs {
        if s.enabled && s.status != "revoked" {
            urls.push(s.write_url.clone());
        }
    }
    if let Some(ref own) = settings.own_nominated_lock_server {
        urls.push(own.write_url.clone());
    }
    for t in &settings.l2_anchor_templates_meta {
        for srv in &t.servers {
            urls.push(srv.write_url.clone());
        }
    }
    let client = sync_client::create_sync_probe_client();
    crate::gateway_resolver::prewarm_lock_urls(&client, &urls).await;
    Ok(())
}

#[tauri::command]
pub fn check_l2_lock_status(
    state: tauri::State<AppState>,
) -> Result<FrontendL2LockStatusDto, FrontendError> {
    let settings = state.get_cached_settings().unwrap_or_default();
    let unresolved_intents_count = {
        let service = state.service.lock().unwrap();
        service
            .unresolved_l2_send_intents()
            .map(|v| v.len())
            .unwrap_or(0)
    };

    let now_secs = sync_client::current_unix_timestamp();
    let max_age_limit_secs = crate::settings::SYNC_BASE_MAX_AGE_SECS;
    let mut stale_servers_count = 0usize;
    let mut max_probe_age_secs: Option<u64> = None;

    for probe in settings.l2_probe_cache.values() {
        let age = now_secs.saturating_sub(probe.probed_at_secs);
        max_probe_age_secs = Some(max_probe_age_secs.map(|m| m.max(age)).unwrap_or(age));
        if age > max_age_limit_secs || !probe.passed || probe.inode_warning {
            stale_servers_count += 1;
        }
    }

    let templates_count = settings.l2_anchor_templates_meta.len();
    let has_own_nominated_server = settings.own_nominated_lock_server.is_some();
    let has_any_sync = settings
        .public_syncs
        .iter()
        .any(|s| s.enabled && s.status != "revoked");

    let status = if settings.pending_restore_l2_reconciliation || unresolved_intents_count > 0 {
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
        pending_restore_reconciliation: settings.pending_restore_l2_reconciliation,
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
    let settings = state.get_cached_settings().unwrap_or_default();
    let filter_set: Option<HashSet<&str>> =
        filter_ids.map(|ids| ids.iter().map(|s| s.as_str()).collect());

    let (targets, missing_manifest_ids) = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
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
                            let mut pow_bits = DEFAULT_LOCK_POW_BASE_BITS;
                            for (srv, _) in &target.server_paths {
                                let norm =
                                    crate::checkout::normalize_sync_write_url(&srv.write_url);
                                if let Some(p) = settings.l2_probe_cache.get(&norm) {
                                    pow_bits = pow_bits.max(p.pow_base_bits);
                                }
                            }
                            ready.push((id.clone(), target, pow_bits));
                        }
                        Err(_) => {
                            missing.push(id.clone());
                        }
                    }
                }
                (ready, missing)
            })
            .map_err(FrontendError::from)?
    };

    let active_anchored_count = targets.len().saturating_add(missing_manifest_ids.len());
    if active_anchored_count == 0 {
        return Ok(StaticReconcileSummary {
            active_anchored_count: 0,
            reconciled_spent_count: 0,
            spent_voucher_ids: Vec::new(),
            all_responded: true,
            status: "reconciled".to_string(),
        });
    }

    let client = sync_client::create_sync_probe_client();
    let mut spent_voucher_ids = Vec::new();
    let mut all_responded = missing_manifest_ids.is_empty();
    let mut any_committee_reachable = false;

    for (local_id, target, pow_bits) in targets {
        let mut responded = 0usize;
        let mut found_entry = None;

        for (srv, rel_path) in &target.server_paths {
            match sync_client::fetch_static_lock_entry(&client, &srv.read_url, rel_path).await {
                Ok(Some(entry)) => {
                    responded += 1;
                    any_committee_reachable = true;
                    found_entry = Some(entry);
                    break;
                }
                Ok(None) => {
                    responded += 1;
                    any_committee_reachable = true;
                }
                Err(_) => {}
            }
        }

        if let Some(entry) = found_entry {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            match service.reconcile_voucher_state(&local_id, Some(&entry), None, pow_bits, password)
            {
                Ok(VoucherReconciliationResult::AlreadySpent { .. })
                | Ok(VoucherReconciliationResult::ConflictDetected { .. }) => {
                    spent_voucher_ids.push(local_id);
                }
                Ok(VoucherReconciliationResult::UnspentSafe) | Err(_) => {}
            }
        } else if responded > 0 {
            let mut service = state
                .service
                .lock()
                .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
            let _ = service.reconcile_voucher_state(&local_id, None, None, pow_bits, password);
        } else {
            all_responded = false;
        }
    }

    let status = if !missing_manifest_ids.is_empty() && !any_committee_reachable {
        "no_servers".to_string()
    } else if any_committee_reachable {
        "reconciled".to_string()
    } else {
        "offline".to_string()
    };

    Ok(StaticReconcileSummary {
        active_anchored_count,
        reconciled_spent_count: spent_voucher_ids.len(),
        spent_voucher_ids,
        all_responded,
        status,
    })
}

pub(crate) async fn reconcile_restored_wallet_if_pending(
    state: &AppState,
    password: Option<&str>,
) -> Result<bool, FrontendError> {
    let settings = match state.get_cached_settings() {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    if !settings.pending_restore_l2_reconciliation {
        return Ok(false);
    }

    let summary = reconcile_vouchers_via_static_locks(state, None, password).await?;
    if summary.all_responded || summary.active_anchored_count == 0 {
        let mut updated = settings;
        updated.pending_restore_l2_reconciliation = false;
        if let Ok(mut service) = state.service.lock() {
            let _ = state.save_settings(&mut service, updated, password);
        }
        return Ok(true);
    }
    Ok(false)
}

#[tauri::command]
pub async fn reconcile_rollback(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendRollbackReconciliationDto, FrontendError> {
    info!("Reconciling rollback against static Sync Lock committee files...");
    if let Some(ref pwd) = password {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.unlock_session(pwd, 300);
    }

    let summary = reconcile_vouchers_via_static_locks(&state, None, password.as_deref()).await?;

    let report = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        service
            .reconcile_rollback_with_diagnostics(
                &summary.spent_voucher_ids,
                None,
                password.as_deref(),
            )
            .map_err(FrontendError::from)?
    };

    if summary.all_responded || summary.active_anchored_count == 0 {
        if let Ok(mut settings) = state.get_cached_settings() {
            if settings.pending_restore_l2_reconciliation {
                settings.pending_restore_l2_reconciliation = false;
                if let Ok(mut service) = state.service.lock() {
                    let _ = state.save_settings(&mut service, settings, password.as_deref());
                }
            }
        }
    }

    Ok(report.into())
}

#[tauri::command]
pub async fn check_voucher_l2_status(
    local_instance_id: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendVoucherL2StatusDto, FrontendError> {
    let settings = state.get_cached_settings().unwrap_or_default();

    let (target_opt, is_currently_archived, pow_bits) = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
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
                if instance.voucher.layer2_anchor.is_none() {
                    return Ok::<_, human_money_core::Error>((
                        None,
                        is_archived,
                        DEFAULT_LOCK_POW_BASE_BITS,
                    ));
                }
                let target =
                    wallet.build_voucher_read_targets(identity, &local_instance_id, None)?;
                let mut bits = DEFAULT_LOCK_POW_BASE_BITS;
                for (srv, _) in &target.server_paths {
                    let norm = crate::checkout::normalize_sync_write_url(&srv.write_url);
                    if let Some(p) = settings.l2_probe_cache.get(&norm) {
                        bits = bits.max(p.pow_base_bits);
                    }
                }
                Ok((Some(target), is_archived, bits))
            })
            .map_err(FrontendError::from)?
            .map_err(FrontendError::from)?
    };

    let Some(target) = target_opt else {
        return Ok(FrontendVoucherL2StatusDto {
            status: "offline_only".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        });
    };

    if target.server_paths.is_empty() {
        return Ok(FrontendVoucherL2StatusDto {
            status: "no_servers".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        });
    }

    let client = sync_client::create_sync_probe_client();
    let mut responded = 0usize;
    let mut found_entry = None;

    for (srv, rel_path) in &target.server_paths {
        match sync_client::fetch_static_lock_entry(&client, &srv.read_url, rel_path).await {
            Ok(Some(entry)) => {
                responded += 1;
                found_entry = Some(entry);
                break;
            }
            Ok(None) => {
                responded += 1;
            }
            Err(_) => {}
        }
    }

    if let Some(entry) = found_entry {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        match service.reconcile_voucher_state(
            &local_instance_id,
            Some(&entry),
            None,
            pow_bits,
            password.as_deref(),
        ) {
            Ok(VoucherReconciliationResult::AlreadySpent { successor_t_id }) => {
                return Ok(FrontendVoucherL2StatusDto {
                    status: "spent".to_string(),
                    archived: true,
                    successor_t_id: Some(successor_t_id),
                });
            }
            Ok(VoucherReconciliationResult::ConflictDetected { conflicting_entry }) => {
                let successor_t_id = bs58::encode(conflicting_entry.child_lock).into_string();
                return Ok(FrontendVoucherL2StatusDto {
                    status: "conflict".to_string(),
                    archived: true,
                    successor_t_id: Some(successor_t_id),
                });
            }
            Ok(VoucherReconciliationResult::UnspentSafe) => {
                return Ok(FrontendVoucherL2StatusDto {
                    status: "unspent".to_string(),
                    archived: is_currently_archived,
                    successor_t_id: None,
                });
            }
            Err(e) => {
                log::debug!("Voucher reconciliation rejected entry: {}", e);
            }
        }
    }

    if responded > 0 {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let _ = service.reconcile_voucher_state(
            &local_instance_id,
            None,
            None,
            pow_bits,
            password.as_deref(),
        );
        Ok(FrontendVoucherL2StatusDto {
            status: "unspent".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        })
    } else {
        Ok(FrontendVoucherL2StatusDto {
            status: "offline".to_string(),
            archived: is_currently_archived,
            successor_t_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontend_rollback_reconciliation_dto_serialization() {
        let report = human_money_core::models::seal::RollbackReconciliationReport {
            quarantined_voucher_ids: vec!["v1".to_string()],
            archived_voucher_ids: vec!["v2".to_string()],
            retained_voucher_ids: vec!["v3".to_string()],
            new_tx_nonce: 42,
            consolidated_state_hash: "hash123".to_string(),
            message: "Reconciled".to_string(),
        };
        let dto: FrontendRollbackReconciliationDto = report.into();
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"newTxNonce\":42"));
        assert!(json.contains("\"consolidatedStateHash\":\"hash123\""));
        assert!(json.contains("\"quarantinedVoucherIds\":[\"v1\"]"));
        assert!(json.contains("\"archivedVoucherIds\":[\"v2\"]"));
        assert!(json.contains("\"retainedVoucherIds\":[\"v3\"]"));
        assert!(json.contains("\"message\":\"Reconciled\""));
    }

    #[test]
    fn test_frontend_voucher_l2_status_dto_serialization() {
        let dto = FrontendVoucherL2StatusDto {
            status: "spent".to_string(),
            archived: true,
            successor_t_id: Some("succ123".to_string()),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"status\":\"spent\""));
        assert!(json.contains("\"archived\":true"));
        assert!(json.contains("\"successorTId\":\"succ123\""));

        let unspent_dto = FrontendVoucherL2StatusDto {
            status: "unspent".to_string(),
            archived: false,
            successor_t_id: None,
        };
        let unspent_json = serde_json::to_string(&unspent_dto).unwrap();
        assert!(unspent_json.contains("\"status\":\"unspent\""));
        assert!(unspent_json.contains("\"archived\":false"));
        assert!(!unspent_json.contains("successorTId"));
    }

    #[test]
    fn test_reach_preset_values_and_detection() {
        assert_eq!(preset_values("near").unwrap(), (1, 2, 1, 1));
        assert_eq!(preset_values("medium").unwrap(), (2, 1, 2, 2));
        assert_eq!(preset_values("far").unwrap(), (3, 1, 3, 2));
        assert!(preset_values("ultra").is_err());

        assert_eq!(detect_reach_preset(1, 2, 1, 1), "near");
        assert_eq!(detect_reach_preset(2, 1, 2, 2), "medium");
        assert_eq!(detect_reach_preset(3, 1, 3, 2), "far");

        assert_eq!(detect_reach_preset(1, 1, 1, 1), "custom");
        assert_eq!(detect_reach_preset(2, 1, 3, 2), "custom");
        assert_eq!(detect_reach_preset(2, 2, 2, 2), "custom");
        assert_eq!(detect_reach_preset(2, 1, 2, 0), "custom");
        assert_eq!(detect_reach_preset(3, 1, 3, 3), "custom");
    }
}
