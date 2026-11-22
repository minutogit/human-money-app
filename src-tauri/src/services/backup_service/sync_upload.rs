use super::*;
use crate::{
    models::{BackupDeltaContainer, FrontendAutoBackupStatusDto, FrontendBackupSyncResult},
    services::{
        common::{collect_backup_sync_targets, normalize_sync_url, DropChannelSigner},
        fs_helpers::collect_custom_standards,
    },
    AppState,
};
use chrono::Utc;

pub(crate) fn is_older_than_14_days(last_backup_at: &Option<String>) -> bool {
    let Some(ts_str) = last_backup_at else {
        return true;
    };
    let Ok(ts) = chrono::DateTime::parse_from_rfc3339(ts_str) else {
        return true;
    };
    let now = Utc::now();
    now.signed_duration_since(ts.with_timezone(&Utc)).num_days() >= 14
}

pub fn compute_backup_status_dto(
    security_policy: &crate::settings::SecurityPolicy,
    lifecycle: &crate::settings::InternalLifecycleState,
    has_own_sync: bool,
) -> FrontendAutoBackupStatusDto {
    let sync_backup_enabled = security_policy.sync_backup_enabled;
    let last_sync_backup_at = lifecycle.last_sync_backup_at.clone();

    let has_friend_syncs = security_policy.market_selected_syncs.iter().any(|d| {
        !d.write_url.trim().is_empty()
            && !crate::services::sync_endpoints::is_endpoint_revoked(&d.status)
    });
    let has_public_syncs = security_policy.public_syncs.iter().any(|d| {
        d.enabled
            && !d.write_url.trim().is_empty()
            && !crate::services::sync_endpoints::is_endpoint_revoked(&d.status)
    });
    let has_authorized_sync_backup_target =
        has_friend_syncs || has_public_syncs || has_own_sync || last_sync_backup_at.is_some();

    let folder_str_opt = security_policy
        .backup_sync_folder
        .as_ref()
        .filter(|f| !f.trim().is_empty());
    let local_configured = folder_str_opt.is_some();
    let folder_str = folder_str_opt.cloned();

    let local_connected = if let Some(ref path_str) = folder_str {
        Path::new(path_str).is_dir()
    } else {
        false
    };

    let local_current = local_configured
        && local_connected
        && !lifecycle.backup_dirty
        && !is_older_than_14_days(&lifecycle.last_backup_at);
    let sync_current = sync_backup_enabled
        && has_authorized_sync_backup_target
        && !lifecycle.backup_dirty
        && last_sync_backup_at.is_some()
        && !is_older_than_14_days(&last_sync_backup_at);

    let has_sync_target = sync_backup_enabled && has_authorized_sync_backup_target;

    let status = if !local_configured && !has_sync_target {
        "notConfigured"
    } else if local_current || sync_current {
        "ok"
    } else if local_configured && !local_connected && !sync_current {
        "disconnected"
    } else if lifecycle.backup_dirty {
        "pending"
    } else {
        "warning"
    };

    FrontendAutoBackupStatusDto {
        status: status.to_string(),
        last_backup_at: lifecycle.last_backup_at.clone(),
        backup_dirty: lifecycle.backup_dirty,
        sync_folder: folder_str,
        changed_blobs: None,
        sync_backup_enabled,
        last_sync_backup_at,
        has_authorized_sync_backup_target,
    }
}

pub(crate) fn get_backup_status_impl(
    state: &AppState,
) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let security_policy = state
        .get_or_load_security_policy(&mut service, None)
        .map_err(FrontendError::from)?;
    let lifecycle = state
        .get_or_load_internal_lifecycle(&mut service, None)
        .map_err(FrontendError::from)?;
    let has_own_sync = state
        .get_or_load_own_sync_descriptor(&mut service, None)?
        .is_some_and(|d| !d.write_url.trim().is_empty());
    Ok(compute_backup_status_dto(
        &security_policy,
        &lifecycle,
        has_own_sync,
    ))
}

pub(crate) fn run_auto_backup_sync_impl(
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendAutoBackupStatusDto, FrontendError> {
    let (session, resolved_pw) =
        state.authorize_and_begin_session_secret(crate::state::RequiredAccess::DailyPay, None)?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let (security_policy, mut lifecycle, has_own_sync) = state.with_session_required(
        &session,
        "Active wallet session changed before auto-backup sync",
        |service| {
            let sp = state
                .get_or_load_security_policy(service, pw)
                .map_err(FrontendError::from)?;
            let lc = state
                .get_or_load_internal_lifecycle(service, pw)
                .map_err(FrontendError::from)?;
            let has_own = state
                .get_or_load_own_sync_descriptor(service, pw)?
                .is_some_and(|d| !d.write_url.trim().is_empty());
            Ok((sp, lc, has_own))
        },
    )?;

    let folder_str = match &security_policy.backup_sync_folder {
        Some(f) if !f.trim().is_empty() => f.clone(),
        _ => {
            return Ok(compute_backup_status_dto(
                &security_policy,
                &lifecycle,
                has_own_sync,
            ));
        }
    };

    if !security_policy.auto_backup_enabled
        || security_policy.backup_protection_mode.as_deref() == Some("custom_password")
    {
        return Ok(compute_backup_status_dto(
            &security_policy,
            &lifecycle,
            has_own_sync,
        ));
    }

    let folder_path = Path::new(&folder_str);
    // Crucial USB safety check: verify is_dir(), NEVER call fs::create_dir_all if the folder is missing.
    if !folder_path.is_dir() {
        if !lifecycle.backup_dirty {
            if let Ok(Some(updated)) =
                state.mutate_internal_lifecycle_for_session(&session, pw, |lc| {
                    lc.backup_dirty = true;
                    true
                })
            {
                lifecycle = updated;
            }
        }
        return Ok(compute_backup_status_dto(
            &security_policy,
            &lifecycle,
            has_own_sync,
        ));
    }

    let custom_standards = collect_custom_standards(app);

    let sync_res = state.with_session(&session, |service| {
        if let Err(e) = service.require_access_mode(human_money_core::AccessMode::DailyPay, pw) {
            return Ok(Err(FrontendError::from(e)));
        }
        let outcome = sync_backup_folder_impl(
            folder_path,
            "mnemonic",
            BackupProtection::MnemonicOnly,
            None,
            false,
            service,
            custom_standards,
        );
        match outcome {
            Ok(summary) => {
                let updated_lc = state
                    .mutate_internal_lifecycle(service, pw, |lc| {
                        lc.last_backup_at = Some(summary.timestamp.clone());
                        lc.backup_dirty = false;
                    })
                    .ok();
                Ok(Ok((summary, updated_lc)))
            }
            Err(e) => {
                let is_locked = e.code == "error.wallet.locked"
                    || e.code == "error.auth.sessionTimeout"
                    || e.message.to_lowercase().contains("locked")
                    || e.message.to_lowercase().contains("session");
                if !is_locked && !lifecycle.backup_dirty {
                    let _ = state.mutate_internal_lifecycle(service, pw, |lc| {
                        lc.backup_dirty = true;
                    });
                }
                Ok(Err(e))
            }
        }
    })?;

    let Some(inner_res) = sync_res else {
        let mut dto = compute_backup_status_dto(&security_policy, &lifecycle, has_own_sync);
        dto.status = "locked".to_string();
        return Ok(dto);
    };

    match inner_res {
        Ok((summary, updated_lc)) => {
            if let Some(updated) = updated_lc {
                lifecycle = updated;
            }
            let mut dto = compute_backup_status_dto(&security_policy, &lifecycle, has_own_sync);
            dto.changed_blobs = Some(summary.changed_blobs);
            Ok(dto)
        }
        Err(e) => {
            let is_locked = e.code == "error.wallet.locked"
                || e.code == "error.auth.sessionTimeout"
                || e.message.to_lowercase().contains("locked")
                || e.message.to_lowercase().contains("session");
            if is_locked {
                let mut dto = compute_backup_status_dto(&security_policy, &lifecycle, has_own_sync);
                dto.status = "locked".to_string();
                Ok(dto)
            } else {
                Err(e)
            }
        }
    }
}

pub(crate) fn merge_backup_delta_into_base(
    base: &mut PortableBackupContainer,
    delta: &BackupDeltaContainer,
) -> Result<(), String> {
    if base.container_format != BACKUP_CONTAINER_FORMAT_V1 {
        return Err(format!(
            "Unsupported base backup container format: {}",
            base.container_format
        ));
    }
    if delta.container_format != BACKUP_DELTA_FORMAT_V1 {
        return Err(format!(
            "Unsupported backup delta container format: {}",
            delta.container_format
        ));
    }
    let base_manifest_bytes = human_money_core::hex::decode(&base.manifest_hex)
        .map_err(|e| format!("Invalid base manifest hex: {}", e))?;
    let actual_sha256 = sha256_hex(&base_manifest_bytes);
    if actual_sha256 != delta.base_manifest_sha256 {
        return Err(format!(
            "Backup delta base mismatch (expected {}, got {})",
            delta.base_manifest_sha256, actual_sha256
        ));
    }

    base.manifest_hex = delta.manifest_hex.clone();
    base.created_at = delta.created_at.clone();
    for del_name in &delta.delete_blobs {
        base.blobs_hex.remove(del_name);
    }
    for (name, hex_data) in &delta.put_blobs_hex {
        base.blobs_hex.insert(name.clone(), hex_data.clone());
    }
    base.custom_standards = delta.custom_standards.clone();
    Ok(())
}

pub(crate) async fn fetch_and_merge_backup_slot(
    client: &reqwest::Client,
    read_url: &str,
    pubkey_hex: &str,
    base_prefix: Option<&str>,
) -> Option<Vec<u8>> {
    let delta_prefix = humoco_market::derive_delta_prefix(base_prefix);
    let (base_res, delta_res) = tokio::join!(
        crate::sync_client::get_slot(client, read_url, pubkey_hex, base_prefix, "backup", None),
        crate::sync_client::get_slot(
            client,
            read_url,
            pubkey_hex,
            Some(delta_prefix.as_str()),
            "backup",
            None
        ),
    );

    let base_slot_bytes = match base_res {
        Ok(crate::sync_client::SlotFetchResult::Modified { bytes, .. }) => bytes,
        _ => return None,
    };

    let unpadded_base = humoco_market::unpad_backup_slot_bytes(&base_slot_bytes).ok()?;
    let mut base_container = parse_portable_backup_container(&unpadded_base).ok()?;

    if let Ok(crate::sync_client::SlotFetchResult::Modified {
        bytes: delta_slot_bytes,
        ..
    }) = delta_res
    {
        if let Ok(unpadded_delta) = humoco_market::unpad_backup_slot_bytes(&delta_slot_bytes) {
            if let Ok(delta_container) =
                serde_json::from_slice::<BackupDeltaContainer>(&unpadded_delta)
            {
                if let Err(e) = merge_backup_delta_into_base(&mut base_container, &delta_container)
                {
                    log::warn!(
                        "Ignoring stale/mismatched backup delta from {}: {}",
                        read_url,
                        e
                    );
                }
            }
        }
    }

    serde_json::to_vec(&base_container).ok()
}

pub(super) struct AuthorizedBackupTarget {
    desc: humoco_market::DropDescriptor,
    norm_write_url: String,
    cap_pubkey: String,
    is_public: bool,
    base_prefix: Option<String>,
    remote_base_etag: Option<String>,
    remote_delta_etag: Option<String>,
    ttl_days: Option<u32>,
    effective_max_bytes: usize,
}

pub(super) fn target_can_use_delta(
    target: &AuthorizedBackupTarget,
    state: &crate::state::StoredBackupSyncState,
    delta_len: usize,
    now_secs: u64,
    force_base_urls: &std::collections::HashSet<String>,
) -> bool {
    if force_base_urls.contains(&target.norm_write_url) {
        return false;
    }
    let Some(cur_sha) = state.base_manifest_sha256.as_deref() else {
        return false;
    };
    let Some(m) = state.mirrors_by_write_url.get(&target.norm_write_url) else {
        return false;
    };
    if m.base_manifest_sha256 != cur_sha {
        return false;
    }
    if target.remote_base_etag.is_none() {
        return false;
    }
    if m.base_slot_etag
        .as_ref()
        .is_some_and(|exp| target.remote_base_etag.as_deref() != Some(exp.as_str()))
    {
        return false;
    }
    if m.delta_slot_etag != target.remote_delta_etag {
        return false;
    }
    if now_secs.saturating_sub(m.base_uploaded_at) >= BACKUP_BASE_TTL_SECS {
        return false;
    }
    if delta_len > m.base_padded_bytes / 2 {
        return false;
    }
    m.base_padded_bytes.saturating_add(delta_len) <= target.effective_max_bytes
}

pub async fn sync_backup_impl(
    password: Option<String>,
    is_wifi: Option<bool>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<FrontendBackupSyncResult, FrontendError> {
    let custom_standards = collect_custom_standards(app);
    sync_backup_with_standards_impl(password, is_wifi, custom_standards, state).await
}

pub(crate) async fn sync_backup_with_standards_impl(
    password: Option<String>,
    is_wifi: Option<bool>,
    custom_standards: HashMap<String, String>,
    state: &AppState,
) -> Result<FrontendBackupSyncResult, FrontendError> {
    let (session, resolved_pw) = state.authorize_and_begin_session_secret(
        crate::state::RequiredAccess::DailyPay,
        password.as_deref(),
    )?;
    let _backup_guard = state.backup_op_lock.lock().await;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let (security_policy, my_ed_sk, master_seed, my_prefix, own_admin_opt) = {
        struct BackupSyncReady {
            security_policy: crate::settings::SecurityPolicy,
            my_ed_sk: ed25519_dalek::SigningKey,
            master_seed: zeroize::Zeroizing<[u8; 32]>,
            my_prefix: Option<String>,
            own_admin_opt: Option<crate::state::StoredSyncAdmin>,
        }
        enum BackupSyncPrep {
            Disabled,
            SkipWifi,
            Ready(Box<BackupSyncReady>),
        }
        let prep = state.with_session_required(
            &session,
            "Active wallet session changed before starting backup sync",
            |service| {
                let security_policy = state
                    .get_or_load_security_policy(service, pw)
                    .map_err(FrontendError::from)?;
                if !security_policy.sync_backup_enabled {
                    return Ok(BackupSyncPrep::Disabled);
                }
                let ui_prefs = state
                    .get_or_load_ui_preferences(service, pw)
                    .map_err(FrontendError::from)?;
                if ui_prefs.market_poll_wifi_only && is_wifi == Some(false) {
                    return Ok(BackupSyncPrep::SkipWifi);
                }
                let (my_ed_sk, my_prefix) = service
                    .with_wallet_and_identity(|_w, id| {
                        let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                            .map(|s| s.to_string());
                        (id.signing_key.clone(), pfx)
                    })
                    .map_err(FrontendError::from)?;
                let master_seed = zeroize::Zeroizing::new(my_ed_sk.to_bytes());
                let own_admin_opt =
                    crate::services::sync_engine::admin::load_stored_sync_admin_opt(service, pw)?;
                Ok(BackupSyncPrep::Ready(Box::new(BackupSyncReady {
                    security_policy,
                    my_ed_sk,
                    master_seed,
                    my_prefix,
                    own_admin_opt,
                })))
            },
        )?;
        match prep {
            BackupSyncPrep::Disabled => {
                return Ok(FrontendBackupSyncResult {
                    uploaded_to: 0,
                    authorized_targets: 0,
                    padded_bytes: 0,
                    timestamp: None,
                    errors: Vec::new(),
                    retry_after_secs: None,
                    skipped_wifi_only: false,
                });
            }
            BackupSyncPrep::SkipWifi => {
                return Ok(FrontendBackupSyncResult {
                    uploaded_to: 0,
                    authorized_targets: 0,
                    padded_bytes: 0,
                    timestamp: None,
                    errors: Vec::new(),
                    retry_after_secs: None,
                    skipped_wifi_only: true,
                });
            }
            BackupSyncPrep::Ready(boxed) => (
                boxed.security_policy,
                boxed.my_ed_sk,
                boxed.master_seed,
                boxed.my_prefix,
                boxed.own_admin_opt,
            ),
        }
    };

    // Gather active non-revoked backup sync targets via canonical helper (own -> friend -> public).
    let own_desc_opt = own_admin_opt
        .as_ref()
        .filter(|a| a.revocation.is_none())
        .map(|a| &a.descriptor);
    let candidate_targets =
        collect_backup_sync_targets(&security_policy, own_desc_opt, my_prefix.as_deref(), None);

    if candidate_targets.is_empty() {
        return Ok(FrontendBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
            retry_after_secs: None,
            skipped_wifi_only: false,
        });
    }

    // Probe which candidate drops authorize channel "backup"
    let probe_client = crate::sync_client::create_sync_probe_client();
    let mut authorized_targets = Vec::new();

    for sync_desc in candidate_targets {
        let (cap_pubkey, is_public, cap_prefix) = if let Some(ref slot_id) = sync_desc.slot_id {
            match humoco_market::parse_slot_id(slot_id) {
                Ok((base_pk, sid_pfx)) => (base_pk, true, sid_pfx.or_else(|| my_prefix.clone())),
                Err(_) => (slot_id.clone(), true, my_prefix.clone()),
            }
        } else {
            match humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &sync_desc.drop_pub) {
                Ok(pk) => (pk, false, my_prefix.clone()),
                Err(_) => continue,
            }
        };

        if let Ok(cap) = crate::sync_client::head_slot_capability(
            &probe_client,
            &sync_desc.write_url,
            &cap_pubkey,
            cap_prefix.as_deref(),
            "backup",
        )
        .await
        {
            if cap.authorized {
                let delta_pfx = humoco_market::derive_delta_prefix(cap_prefix.as_deref());
                let remote_delta_etag = crate::sync_client::head_slot_capability(
                    &probe_client,
                    &sync_desc.write_url,
                    &cap_pubkey,
                    Some(delta_pfx.as_str()),
                    "backup",
                )
                .await
                .ok()
                .and_then(|dcap| dcap.slot_etag);
                let norm_write_url = normalize_sync_url(&sync_desc.write_url);
                authorized_targets.push(AuthorizedBackupTarget {
                    desc: sync_desc,
                    norm_write_url,
                    cap_pubkey,
                    is_public,
                    base_prefix: cap_prefix,
                    remote_base_etag: cap.slot_etag,
                    remote_delta_etag,
                    ttl_days: cap.ttl_days,
                    effective_max_bytes: cap
                        .max_bytes
                        .filter(|&v| v > 0)
                        .unwrap_or(humoco_market::DROP_BACKUP_MAX_BYTES),
                });
            }
        }
    }

    let authorized_count = authorized_targets.len();
    if authorized_targets.is_empty() {
        return Ok(FrontendBackupSyncResult {
            uploaded_to: 0,
            authorized_targets: 0,
            padded_bytes: 0,
            timestamp: None,
            errors: Vec::new(),
            retry_after_secs: None,
            skipped_wifi_only: false,
        });
    }

    let max_target_bytes = authorized_targets
        .iter()
        .map(|t| t.effective_max_bytes)
        .max()
        .unwrap_or(humoco_market::DROP_BACKUP_MAX_BYTES);
    let preflight_now_secs = crate::sync_client::current_unix_timestamp();

    let (mut backup_sync_state, preflight_epoch, preflight_tx_nonce, probe_lock_pubs) = state
        .with_session_required(
            &session,
            "Active wallet session changed before preparing backup upload",
            |service| {
                let current_seal = service.get_current_seal().map_err(FrontendError::from)?;
                let (local_epoch, local_tx_nonce) = match current_seal {
                    Some(s) => (s.payload.epoch, s.payload.tx_nonce),
                    None => (0, 0),
                };
                let mut stored_state: crate::state::StoredBackupSyncState =
                    crate::state::load_local_sidecar_json_or_default(
                        service,
                        crate::state::BACKUP_SYNC_STATE_KEY,
                        pw,
                    )
                    .map_err(FrontendError::from)?;
                // Normalize mirror keys consistently to canonical normalized write_url.
                let mut normalized_mirrors = std::collections::BTreeMap::new();
                for (raw_url, mirror) in std::mem::take(&mut stored_state.mirrors_by_write_url) {
                    normalized_mirrors.insert(normalize_sync_url(&raw_url), mirror);
                }
                stored_state.mirrors_by_write_url = normalized_mirrors;

                let lc = state
                    .get_or_load_internal_lifecycle(service, pw)
                    .map_err(FrontendError::from)?;
                let mut lock_pubs = std::collections::HashMap::new();
                for probe in lc.l2_probe_cache.values() {
                    if !probe.delegation_valid {
                        continue;
                    }
                    let norm_write = normalize_sync_url(&probe.write_url);
                    let lp = probe.lock_pub.trim().to_ascii_lowercase();
                    if !norm_write.is_empty() && !lp.is_empty() {
                        lock_pubs.insert(norm_write, lp);
                    }
                }
                Ok((stored_state, local_epoch, local_tx_nonce, lock_pubs))
            },
        )?;

    // Pre-Upload Remote Seal Inspection & Rollback / Unexpected-Loss Detection
    let mut errors = Vec::new();
    let mut skip_urls = std::collections::HashSet::new();
    let mut force_base_urls = std::collections::HashSet::new();
    let mut mirror_anomalies: std::collections::HashMap<String, crate::settings::WalletSyncAlert> =
        std::collections::HashMap::new();
    let mut pending_sync_alert: Option<crate::settings::WalletSyncAlert> = None;
    let mut state_dirty = false;

    for target in &authorized_targets {
        let mirror_opt = backup_sync_state
            .mirrors_by_write_url
            .get(&target.norm_write_url)
            .cloned();
        let fast_path_match = target.remote_base_etag.is_some()
            && mirror_opt.as_ref().is_some_and(|m| {
                m.base_slot_etag == target.remote_base_etag
                    && m.delta_slot_etag == target.remote_delta_etag
            });
        if fast_path_match {
            continue;
        }

        if target.remote_base_etag.is_some() {
            let Some(merged_bytes) = fetch_and_merge_backup_slot(
                &probe_client,
                &target.desc.read_url,
                &target.cap_pubkey,
                target.base_prefix.as_deref(),
            )
            .await
            else {
                skip_urls.insert(target.norm_write_url.clone());
                errors.push(format!(
                    "Inspection skipped for {}: unable to fetch remote backup manifest",
                    target.desc.write_url
                ));
                continue;
            };

            let Ok(container) = parse_portable_backup_container(&merged_bytes) else {
                skip_urls.insert(target.norm_write_url.clone());
                errors.push(format!(
                    "Inspection skipped for {}: unreadable remote backup container",
                    target.desc.write_url
                ));
                continue;
            };
            let Ok(manifest_bytes) = human_money_core::hex::decode(&container.manifest_hex) else {
                skip_urls.insert(target.norm_write_url.clone());
                errors.push(format!(
                    "Inspection skipped for {}: invalid remote manifest hex",
                    target.desc.write_url
                ));
                continue;
            };

            let inspect_res = state.with_session_required(
                &session,
                "Active wallet session changed during backup inspection",
                |service| {
                    Ok(service.inspect_mnemonic_backup_with_unlocked_identity(&manifest_bytes))
                },
            )?;
            let report = match inspect_res {
                Ok(r) => r,
                Err(err) => {
                    skip_urls.insert(target.norm_write_url.clone());
                    errors.push(format!(
                        "Inspection failed for {}: {}",
                        target.desc.write_url, err
                    ));
                    continue;
                }
            };

            match report.sync_state {
                Some(human_money_core::SealSyncState::RemoteIsNewer)
                | Some(human_money_core::SealSyncState::ForkDetected) => {
                    let is_fork = matches!(
                        report.sync_state,
                        Some(human_money_core::SealSyncState::ForkDetected)
                    );
                    let alert = crate::settings::WalletSyncAlert {
                        kind: if is_fork {
                            "fork_detected".to_string()
                        } else {
                            "stale_local_backup".to_string()
                        },
                        mirror_write_url: target.desc.write_url.clone(),
                        remote_epoch: Some(report.manifest.seal.payload.epoch),
                        remote_tx_nonce: Some(report.manifest.seal.payload.tx_nonce),
                        local_epoch: preflight_epoch,
                        local_tx_nonce: preflight_tx_nonce,
                        detected_at: Utc::now().to_rfc3339(),
                        has_signed_ack_proof: mirror_opt
                            .as_ref()
                            .and_then(|m| m.last_slot_ack.as_ref())
                            .is_some(),
                    };
                    if is_fork {
                        state.with_session_required(
                            &session,
                            "Active wallet session changed while persisting fork lock",
                            |service| {
                                service
                                    .persist_seal_lock_reason(
                                        human_money_core::models::seal::SealLockReason::ForkDetected,
                                        pw,
                                    )
                                    .map_err(|err| {
                                        log::error!(
                                            "Failed to persist ForkDetected seal lock reason during backup sync: {}",
                                            err
                                        );
                                        FrontendError::from(err)
                                    })
                            },
                        )?;
                    }
                    let alert_clone = alert.clone();
                    if let Err(err) =
                        state.mutate_internal_lifecycle_for_session(&session, pw, |lc| {
                            lc.pending_restore_l2_reconciliation = true;
                            lc.wallet_sync_alert = Some(alert_clone);
                            true
                        })
                    {
                        log::error!(
                            "Failed to persist wallet sync fork/stale alert: {}",
                            err.message
                        );
                        return Err(err);
                    }
                    return Ok(FrontendBackupSyncResult {
                        uploaded_to: 0,
                        authorized_targets: authorized_count,
                        padded_bytes: 0,
                        timestamp: None,
                        errors: vec![if is_fork {
                            format!(
                                "FORK_DETECTED: Conflicting wallet state detected on {}",
                                target.desc.write_url
                            )
                        } else {
                            format!(
                                "STALE_LOCAL_BACKUP: Newer backup state (epoch {}, nonce {}) exists on {}",
                                report.manifest.seal.payload.epoch,
                                report.manifest.seal.payload.tx_nonce,
                                target.desc.write_url
                            )
                        }],
                        retry_after_secs: None,
                        skipped_wifi_only: false,
                    });
                }
                _ => {}
            }

            if let Some(ref m) = mirror_opt {
                if (
                    report.manifest.seal.payload.epoch,
                    report.manifest.seal.payload.tx_nonce,
                ) < (m.last_synced_epoch, m.last_synced_tx_nonce)
                {
                    let alert = crate::settings::WalletSyncAlert {
                        kind: "server_rollback".to_string(),
                        mirror_write_url: target.desc.write_url.clone(),
                        remote_epoch: Some(report.manifest.seal.payload.epoch),
                        remote_tx_nonce: Some(report.manifest.seal.payload.tx_nonce),
                        local_epoch: m.last_synced_epoch,
                        local_tx_nonce: m.last_synced_tx_nonce,
                        detected_at: Utc::now().to_rfc3339(),
                        has_signed_ack_proof: m.last_slot_ack.is_some(),
                    };
                    force_base_urls.insert(target.norm_write_url.clone());
                    mirror_anomalies.insert(target.norm_write_url.clone(), alert.clone());
                    pending_sync_alert = Some(alert);
                }
            }
        } else if let Some(ref m) = mirror_opt {
            if m.base_uploaded_at > 0 {
                let ttl_days_eff = target.ttl_days.map(u64::from).or(m.last_known_ttl_days);
                let ttl_secs = ttl_days_eff
                    .filter(|&d| d > 0)
                    .map(|d| d.saturating_mul(86_400));
                let within_ttl = match ttl_secs {
                    None => true,
                    Some(secs) => {
                        preflight_now_secs
                            .saturating_sub(m.base_uploaded_at)
                            .saturating_add(3600)
                            < secs
                    }
                };
                if within_ttl {
                    let alert = crate::settings::WalletSyncAlert {
                        kind: "unexpected_data_loss".to_string(),
                        mirror_write_url: target.desc.write_url.clone(),
                        remote_epoch: None,
                        remote_tx_nonce: None,
                        local_epoch: m.last_synced_epoch,
                        local_tx_nonce: m.last_synced_tx_nonce,
                        detected_at: Utc::now().to_rfc3339(),
                        has_signed_ack_proof: m.last_slot_ack.is_some(),
                    };
                    mirror_anomalies.insert(target.norm_write_url.clone(), alert.clone());
                    pending_sync_alert = Some(alert);
                }
            }
        }
    }

    let (
        base_manifest_bytes,
        base_manifest_sha256,
        base_padded_bytes,
        delta_padded_opt,
        local_epoch,
        local_tx_nonce,
        local_seal_hash,
        pending_seal_hash,
    ) = state.with_session_required(
        &session,
        "Active wallet session changed before creating backup payload",
        |service| {
            let current_seal = service.get_current_seal().map_err(FrontendError::from)?;
            let (local_epoch, local_tx_nonce, local_seal_hash, pending_seal_hash) =
                match current_seal {
                    Some(seal) => {
                        let hash_opt = seal.compute_hash().ok();
                        let hash_str = hash_opt.clone().unwrap_or_default();
                        (
                            seal.payload.epoch,
                            seal.payload.tx_nonce,
                            hash_str,
                            hash_opt,
                        )
                    }
                    None => (0, 0, String::new(), None),
                };

            // Try building a delta against the stored Base manifest (if one exists)
            let delta_attempt = if let (Some(prev_manifest), Some(prev_sha256)) = (
                backup_sync_state.base_manifest_bytes.as_deref(),
                backup_sync_state.base_manifest_sha256.as_deref(),
            ) {
                match service.create_differential_backup(
                    Some(prev_manifest),
                    BackupProtection::MnemonicOnly,
                    pw,
                ) {
                    Ok(diff) => {
                        let put_blobs_hex = encode_blobs_hex(diff.put_blobs);
                        let delta_container = BackupDeltaContainer {
                            container_format: BACKUP_DELTA_FORMAT_V1.to_string(),
                            created_at: Utc::now().to_rfc3339(),
                            base_manifest_sha256: prev_sha256.to_string(),
                            manifest_hex: human_money_core::hex::encode(
                                &diff.encrypted_manifest_bytes,
                            ),
                            put_blobs_hex,
                            delete_blobs: diff.delete_blobs,
                            custom_standards: custom_standards.clone(),
                        };
                        serde_json::to_vec(&delta_container).ok().and_then(|bytes| {
                            humoco_market::pad_backup_slot_bytes(&bytes, max_target_bytes).ok()
                        })
                    }
                    Err(_) => None,
                }
            } else {
                None
            };

            // Determine if any mirror needs a fresh Base (or if Delta grew too large)
            let any_mirror_needs_base = authorized_targets
                .iter()
                .filter(|t| !skip_urls.contains(&t.norm_write_url))
                .any(|target| {
                    !delta_attempt.as_ref().is_some_and(|d| {
                        target_can_use_delta(
                            target,
                            &backup_sync_state,
                            d.len(),
                            preflight_now_secs,
                            &force_base_urls,
                        )
                    })
                });

            let (base_m_bytes, base_m_sha, base_padded) = if any_mirror_needs_base {
                let diff = service
                    .create_differential_backup(None, BackupProtection::MnemonicOnly, pw)
                    .map_err(FrontendError::from)?;
                let blobs_hex = encode_blobs_hex(diff.put_blobs);
                let container = PortableBackupContainer {
                    container_format: BACKUP_CONTAINER_FORMAT_V1.to_string(),
                    created_at: Utc::now().to_rfc3339(),
                    manifest_hex: human_money_core::hex::encode(&diff.encrypted_manifest_bytes),
                    blobs_hex,
                    custom_standards,
                };
                let container_bytes = serde_json::to_vec(&container).map_err(|e| {
                    FrontendError::from(format!("Failed to serialize backup container: {}", e))
                })?;
                let padded =
                    humoco_market::pad_backup_slot_bytes(&container_bytes, max_target_bytes)
                        .map_err(|e| FrontendError::from(e.to_string()))?;
                let sha = sha256_hex(&diff.encrypted_manifest_bytes);
                (Some(diff.encrypted_manifest_bytes), Some(sha), Some(padded))
            } else {
                (None, None, None)
            };

            Ok((
                base_m_bytes,
                base_m_sha,
                base_padded,
                delta_attempt,
                local_epoch,
                local_tx_nonce,
                local_seal_hash,
                pending_seal_hash,
            ))
        },
    )?;

    let client = crate::sync_client::create_sync_http_client();
    let mut uploaded_to = 0;
    let mut last_uploaded_bytes = 0usize;
    let mut base_uploaded_any = false;
    let mut max_retry_after_secs: Option<u64> = None;

    for target in authorized_targets {
        if skip_urls.contains(&target.norm_write_url) {
            continue;
        }
        if state.is_origin_cooling_down(&target.desc.write_url, std::time::Instant::now()) {
            errors.push(format!(
                "Upload to {} deferred: server origin cooling down after HTTP 429",
                target.desc.write_url
            ));
            continue;
        }
        let upload_now_secs = crate::sync_client::current_unix_timestamp();
        let target_delta_prefix = humoco_market::derive_delta_prefix(target.base_prefix.as_deref());
        let can_use_delta = delta_padded_opt.as_ref().is_some_and(|d| {
            target_can_use_delta(
                &target,
                &backup_sync_state,
                d.len(),
                upload_now_secs,
                &force_base_urls,
            )
        });

        let (upload_prefix, upload_bytes, is_delta_upload, if_match_val) = if can_use_delta {
            (
                Some(target_delta_prefix.as_str()),
                delta_padded_opt.as_ref().expect("checked above"),
                true,
                target.remote_delta_etag.as_deref().unwrap_or("none"),
            )
        } else if let Some(ref base_bytes) = base_padded_bytes {
            (
                target.base_prefix.as_deref(),
                base_bytes,
                false,
                target.remote_base_etag.as_deref().unwrap_or("none"),
            )
        } else {
            continue;
        };

        if upload_bytes.len() > target.effective_max_bytes {
            errors.push(format!(
                "Upload to {} skipped: backup size {} exceeds channel limit {}",
                target.desc.write_url,
                upload_bytes.len(),
                target.effective_max_bytes
            ));
            continue;
        }

        let signer = DropChannelSigner {
            is_public: target.is_public,
            master_seed: &master_seed,
            my_ed_sk: &my_ed_sk,
            drop_desc: &target.desc,
        };
        let nonce = crate::sync_client::generate_sync_nonce();
        let signed_res = signer.sign_request(
            "PUT",
            "backup",
            upload_prefix,
            upload_now_secs,
            &nonce,
            upload_bytes,
        );

        match signed_res {
            Ok(signed) => {
                match crate::sync_client::put_slot(
                    &client,
                    &target.desc.write_url,
                    &signed,
                    "backup",
                    upload_bytes,
                    Some(if_match_val),
                )
                .await
                {
                    Ok(crate::sync_client::PutSlotOutcome::Saved { etag, slot_ack }) => {
                        uploaded_to += 1;
                        last_uploaded_bytes = last_uploaded_bytes.max(upload_bytes.len());
                        state_dirty = true;
                        let normalized_etag = etag
                            .map(|e| e.trim().trim_matches('"').to_string())
                            .filter(|s| !s.is_empty());
                        let verified_ack = slot_ack.filter(|ack| {
                            let (Some(expected_lp), Some(e)) = (
                                probe_lock_pubs.get(&target.norm_write_url),
                                normalized_etag.as_deref(),
                            ) else {
                                return false;
                            };
                            ack.verify(expected_lp, &target.cap_pubkey, upload_prefix, "backup", e)
                        });
                        let anomaly = mirror_anomalies.get(&target.norm_write_url).cloned();
                        if is_delta_upload {
                            if let Some(m) = backup_sync_state
                                .mirrors_by_write_url
                                .get_mut(&target.norm_write_url)
                            {
                                m.delta_active = true;
                                m.delta_slot_etag = normalized_etag;
                                m.last_synced_epoch = local_epoch;
                                m.last_synced_tx_nonce = local_tx_nonce;
                                m.last_synced_seal_hash = local_seal_hash.clone();
                                if let Some(ack) = verified_ack {
                                    m.last_slot_ack = Some(ack);
                                }
                                m.last_known_ttl_days = target.ttl_days.map(u64::from);
                                if anomaly.is_some() {
                                    m.anomaly = anomaly;
                                }
                            }
                        } else {
                            base_uploaded_any = true;
                            let prev_mirror = backup_sync_state
                                .mirrors_by_write_url
                                .get(&target.norm_write_url)
                                .cloned();
                            let had_delta = prev_mirror
                                .as_ref()
                                .map(|m| m.delta_active)
                                .unwrap_or(false)
                                || target.remote_delta_etag.is_some();
                            let mut delta_cleared = !had_delta;
                            if had_delta {
                                let del_now_secs = crate::sync_client::current_unix_timestamp();
                                let del_nonce = crate::sync_client::generate_sync_nonce();
                                if let Ok(del_signed) = signer.sign_request(
                                    "DELETE",
                                    "backup",
                                    Some(target_delta_prefix.as_str()),
                                    del_now_secs,
                                    &del_nonce,
                                    b"",
                                ) {
                                    delta_cleared = crate::sync_client::delete_slot(
                                        &client,
                                        &target.desc.write_url,
                                        &del_signed,
                                        "backup",
                                    )
                                    .await
                                    .is_ok();
                                }
                            }
                            if let Some(ref new_sha) = base_manifest_sha256 {
                                backup_sync_state.mirrors_by_write_url.insert(
                                    target.norm_write_url.clone(),
                                    crate::state::BackupMirrorState {
                                        base_manifest_sha256: new_sha.clone(),
                                        base_slot_etag: normalized_etag,
                                        delta_slot_etag: if delta_cleared {
                                            None
                                        } else {
                                            target.remote_delta_etag.clone()
                                        },
                                        base_uploaded_at: upload_now_secs,
                                        base_padded_bytes: upload_bytes.len(),
                                        delta_active: !delta_cleared,
                                        last_synced_epoch: local_epoch,
                                        last_synced_tx_nonce: local_tx_nonce,
                                        last_synced_seal_hash: local_seal_hash.clone(),
                                        last_slot_ack: verified_ack,
                                        last_known_ttl_days: target.ttl_days.map(u64::from),
                                        anomaly,
                                    },
                                );
                            }
                        }
                    }
                    Ok(crate::sync_client::PutSlotOutcome::PreconditionFailed { .. }) => {
                        errors.push(format!(
                            "Upload skipped for {}: remote backup changed concurrently (HTTP 412)",
                            target.desc.write_url
                        ));
                    }
                    Err(e) => {
                        state.record_origin_cooldown_for_sync_error(&target.desc.write_url, &e);
                        if let Some(secs) = e.retry_after_secs() {
                            max_retry_after_secs =
                                Some(max_retry_after_secs.map_or(secs, |prev| prev.max(secs)));
                        }
                        errors.push(format!("Upload to {} failed: {}", target.desc.write_url, e));
                    }
                }
            }
            Err(e) => {
                errors.push(format!(
                    "Signing failed for {}: {}",
                    target.desc.write_url, e
                ));
            }
        }
    }

    if !mirror_anomalies.is_empty() {
        for (url, anomaly) in &mirror_anomalies {
            if let Some(m) = backup_sync_state.mirrors_by_write_url.get_mut(url) {
                m.anomaly = Some(anomaly.clone());
                state_dirty = true;
            }
        }
    }

    if state_dirty {
        if base_uploaded_any {
            if let (Some(new_bytes), Some(new_sha)) = (base_manifest_bytes, base_manifest_sha256) {
                backup_sync_state.base_manifest_bytes = Some(new_bytes);
                backup_sync_state.base_manifest_sha256 = Some(new_sha);
            }
        }
        state.with_session_required(
            &session,
            "Active wallet session changed before backup sync state could be saved",
            |service| {
                crate::state::save_local_sidecar_json(
                    service,
                    crate::state::BACKUP_SYNC_STATE_KEY,
                    &backup_sync_state,
                    pw,
                )
                .map_err(FrontendError::from)
            },
        )?;
    }

    if let Some(alert) = pending_sync_alert.clone() {
        let _ = state.mutate_internal_lifecycle_for_session(&session, pw, |lc| {
            let keep_higher = lc
                .wallet_sync_alert
                .as_ref()
                .is_some_and(|cur| cur.kind == "stale_local_backup" || cur.kind == "fork_detected");
            if !keep_higher {
                lc.wallet_sync_alert = Some(alert);
                return true;
            }
            false
        });
    }

    let timestamp_str = if uploaded_to > 0 {
        let ts = Utc::now().to_rfc3339();
        let ts_clone = ts.clone();
        let all_mirrors_succeeded = errors.is_empty();
        let _ = state.with_session(&session, |service| {
            let current_seal_hash = service
                .get_current_seal()
                .ok()
                .flatten()
                .and_then(|seal| seal.compute_hash().ok());
            let seal_unchanged =
                pending_seal_hash.is_some() && current_seal_hash == pending_seal_hash;
            let clear_dirty = all_mirrors_succeeded && seal_unchanged;
            let mut lc = state
                .get_or_load_internal_lifecycle(service, pw)
                .map_err(FrontendError::from)?;
            lc.last_sync_backup_at = Some(ts_clone);
            if clear_dirty {
                lc.backup_dirty = false;
            }
            state
                .save_internal_lifecycle(service, lc, pw)
                .map_err(FrontendError::from)?;
            if clear_dirty {
                if let Some(ref seal_hash) = pending_seal_hash {
                    if let Err(err) = service.acknowledge_seal_sync(seal_hash, pw) {
                        log::warn!("Post-sync seal acknowledgement deferred: {}", err);
                    }
                }
            }
            Ok(())
        });
        Some(ts)
    } else {
        None
    };

    Ok(FrontendBackupSyncResult {
        uploaded_to,
        authorized_targets: authorized_count,
        padded_bytes: last_uploaded_bytes,
        timestamp: timestamp_str,
        errors,
        retry_after_secs: max_retry_after_secs,
        skipped_wifi_only: false,
    })
}
