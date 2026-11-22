use std::collections::BTreeMap;

use base64::Engine;
use human_money_core::app_service::AppService;
use human_money_core::models::conflict::TrustStatus;
use sha2::Digest;
use zeroize::Zeroizing;

use crate::models::{
    FrontendError, FrontendMemberPolicyDto, FrontendSyncAdminStatus,
    FrontendSyncAdminUpdateRequest, FrontendSyncConnectionTestResult, FrontendSyncDescriptor,
    FrontendSyncSetupDraft, StoredOwnMarketProfile,
};
use crate::services::common::{canonical_drop_pub_hex, get_wallet_master_seed, normalize_sync_url};
use crate::services::market_service::{
    build_card_drops, load_own_market_profile_state, rebuild_and_sign_own_card_with_drops,
};
use crate::session::{AuthorizedSession, SessionToken};
use crate::settings::SecurityPolicy;
use crate::state::{
    canonicalize_did, AppState, EncryptedBatchCommit, StoredSyncAdmin, MARKET_SYNC_ADMIN_KEY,
};
use crate::sync_client::{self, SyncServerAdminStatus};

fn default_policy_for_channel(channel: &str) -> humoco_market::MemberPolicy {
    humoco_market::MemberPolicy {
        direct_min: 2,
        hop2_enabled: channel != "backup",
        bridge_min: 2,
        hop2_min: 2,
    }
}

fn validate_admin_quotas(
    max_total_storage_mb: usize,
    max_members: usize,
    max_user_storage_mb: usize,
) -> Result<(), FrontendError> {
    if max_members == 0 || max_members > 100_000 {
        return Err(FrontendError::from(
            "max_members must be between 1 and 100000".to_string(),
        ));
    }
    if max_user_storage_mb < 10 {
        return Err(FrontendError::from(
            "max_user_storage_mb must be at least 10 MB".to_string(),
        ));
    }
    if max_total_storage_mb < max_user_storage_mb || max_total_storage_mb > 10_485_760 {
        return Err(FrontendError::from(
            "max_total_storage_mb must be between max_user_storage_mb and 10485760 MB".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn load_stored_sync_admin_opt(
    service: &mut AppService,
    password: Option<&str>,
) -> Result<Option<StoredSyncAdmin>, FrontendError> {
    crate::state::load_encrypted_json_opt::<Option<StoredSyncAdmin>>(
        service,
        MARKET_SYNC_ADMIN_KEY,
        password,
    )
    .map(Option::flatten)
    .map_err(FrontendError::from)
}

fn load_admin_with_signing_key(
    state: &AppState,
    password: Option<&str>,
) -> Result<
    (
        StoredSyncAdmin,
        ed25519_dalek::SigningKey,
        AuthorizedSession,
    ),
    FrontendError,
> {
    let auth = state.authorize_session(crate::state::RequiredAccess::Full, password)?;
    let (admin, admin_sk) =
        state.with_session_required(&auth.token, "Active wallet session changed", |service| {
            let admin = load_stored_sync_admin_opt(service, auth.pw())?
                .ok_or_else(|| FrontendError::from("Sync admin not configured".to_string()))?;
            let master_seed = get_wallet_master_seed(service, auth.pw())?;
            let (admin_sk, _, _) =
                humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
            Ok((admin, admin_sk))
        })?;
    Ok((admin, admin_sk, auth))
}

fn save_stored_sync_admin(
    state: &AppState,
    service: &mut AppService,
    admin: &StoredSyncAdmin,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    crate::state::save_encrypted_json(service, MARKET_SYNC_ADMIN_KEY, admin, password)
        .map_err(FrontendError::from)?;
    state.write_cache_slot_for_service(
        service,
        |c| &mut c.own_sync_descriptor,
        Some(crate::state::active_own_sync_descriptor(Some(admin))),
    );
    Ok(())
}

pub(crate) fn build_resigned_own_card_profile_for_policy(
    state: &AppState,
    service: &mut AppService,
    policy: &SecurityPolicy,
    password: Option<&str>,
) -> Option<StoredOwnMarketProfile> {
    let mut existing = load_own_market_profile_state(state, service, password).ok()?;
    if !existing.published {
        return None;
    }
    let my_prefix = service
        .with_wallet_and_identity(|_w, id| {
            human_money_core::crypto::get_prefix_from_user_id(&id.user_id).map(|s| s.to_string())
        })
        .ok()
        .flatten();
    let drops = build_card_drops(
        &policy.market_selected_syncs,
        &policy.public_syncs,
        my_prefix.as_deref(),
    );
    let now_unix = sync_client::current_unix_timestamp();
    rebuild_and_sign_own_card_with_drops(service, &mut existing, now_unix, Some(drops)).ok()?;
    Some(existing)
}

fn prepare_policy_without_sync(
    state: &AppState,
    service: &mut AppService,
    read_url: &str,
    password: Option<&str>,
) -> Result<Option<(SecurityPolicy, Option<StoredOwnMarketProfile>)>, FrontendError> {
    let norm_target = normalize_sync_url(read_url);
    if norm_target.is_empty() {
        return Ok(None);
    }
    let mut policy = state
        .get_or_load_security_policy(service, password)
        .map_err(FrontendError::from)?;
    let initial_len = policy.market_selected_syncs.len();
    policy
        .market_selected_syncs
        .retain(|d| normalize_sync_url(&d.read_url) != norm_target);
    if policy.market_selected_syncs.len() == initial_len {
        return Ok(None);
    }
    let updated_profile =
        build_resigned_own_card_profile_for_policy(state, service, &policy, password);
    Ok(Some((policy, updated_profile)))
}

fn build_admin_status_dto(
    admin: StoredSyncAdmin,
    stats: humoco_market::DropMembersStats,
    remote_status: Option<SyncServerAdminStatus>,
) -> FrontendSyncAdminStatus {
    let card_policy = admin.policies.get("card").cloned().unwrap_or_default();
    let admin_pubkey = admin.descriptor.drop_pub.clone();
    let (used_bytes, max_bytes, soft_warning, eff_max_members, eff_total_mb, eff_user_mb) =
        match remote_status {
            Some(s) => (
                Some(s.used_bytes),
                Some(s.max_bytes),
                s.soft_warning,
                s.max_members,
                (s.max_bytes / (1024 * 1024)) as usize,
                (s.max_user_bytes / (1024 * 1024)) as usize,
            ),
            None => (
                None,
                Some((admin.max_total_storage_mb as u64) * 1024 * 1024),
                false,
                admin.max_members,
                admin.max_total_storage_mb,
                admin.max_user_storage_mb,
            ),
        };

    FrontendSyncAdminStatus {
        descriptor: admin.descriptor.into(),
        admin_pubkey,
        data_dir: admin.data_dir,
        max_members: eff_max_members,
        max_total_storage_mb: eff_total_mb,
        max_user_storage_mb: eff_user_mb,
        used_bytes,
        max_bytes,
        soft_warning,
        card_policy: card_policy.into(),
        allow: admin.allow,
        total_member_count: stats.total_count,
        revocation: admin.revocation.map(Into::into),
    }
}

/// Computes the authorized members for the admin's drop for a specific channel
/// along with stats and sorted 64-hex stealth pubkeys.
pub fn compute_admin_sync_members_with_stats(
    state: &AppState,
    service: &mut AppService,
    admin: &StoredSyncAdmin,
    channel: &str,
    password: Option<&str>,
) -> Result<(humoco_market::DropMembersStats, Vec<String>), FrontendError> {
    let policy = admin
        .policies
        .get(channel)
        .cloned()
        .unwrap_or_else(|| default_policy_for_channel(channel));

    let admin_own_did = service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)?;
    let admin_own_can_did =
        canonicalize_did(&admin_own_did).unwrap_or_else(|_| admin_own_did.clone());

    let reg = state
        .get_or_load_wot_register(service, password)
        .map_err(FrontendError::from)?;

    let is_offender = |did: &str| -> bool {
        service
            .with_wallet(|w| matches!(w.check_reputation(did), TrustStatus::KnownOffender(_)))
            .unwrap_or(false)
    };

    let direct: Vec<(String, u8)> = reg
        .own_contacts
        .iter()
        .filter(|(did, _)| !is_offender(did))
        .map(|(did, e)| (did.clone(), e.level))
        .collect();

    let mut bridges: Vec<humoco_market::BridgeContact> = Vec::new();
    for (friend_did, friend_level) in &direct {
        let can_friend = canonicalize_did(friend_did).unwrap_or_else(|_| friend_did.clone());
        if let Some(stored_jwt) = reg.raw_list_store.get(&can_friend) {
            let disclosed: Vec<humoco_market::DisclosedContact> = stored_jwt
                .disclosures
                .values()
                .filter(|d| !is_offender(&d.target_did))
                .map(|d| (d.target_did.clone(), d.level))
                .collect();
            bridges.push((friend_did.clone(), *friend_level, disclosed));
        }
    }

    let mut allow_list: Vec<String> = admin.allow.clone();
    allow_list.push(admin_own_can_did);

    let stats = humoco_market::compute_drop_members_with_stats(
        &direct,
        &bridges,
        &policy,
        &allow_list,
        &[],
        admin.max_members,
    );

    let master_seed = get_wallet_master_seed(service, password)?;
    let (admin_sk, _admin_vk, _admin_pub) =
        humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);

    let mut member_pubkeys = Vec::new();
    for did in &stats.member_dids {
        if let Ok(member_vk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
            if let Ok(stealth_pk) =
                humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &member_vk)
            {
                member_pubkeys.push(stealth_pk);
            }
        }
    }

    member_pubkeys.sort();
    member_pubkeys.dedup();

    Ok((stats, member_pubkeys))
}

pub fn compute_channel_members_hash(pubkeys: &[String]) -> String {
    let mut hasher = sha2::Sha256::new();
    for pk in pubkeys {
        hasher.update(pk.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

pub(crate) async fn push_all_channel_members(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    admin: &mut StoredSyncAdmin,
    admin_sk: &ed25519_dalek::SigningKey,
    password: Option<&str>,
) -> Result<usize, FrontendError> {
    if admin.descriptor.write_url.is_empty() {
        return Ok(0);
    }
    let pks_by_channel = state.with_session_required(
        session,
        "Active wallet session changed before channel members could be computed",
        |service| {
            let mut list = Vec::new();
            for ch in ["card", "wot", "backup", "post", "lock"] {
                let (_stats, pks) =
                    compute_admin_sync_members_with_stats(state, service, admin, ch, password)?;
                list.push((ch, pks));
            }
            Ok(list)
        },
    )?;

    let mut pushed_hashes: Vec<(String, String)> = Vec::new();
    let mut first_error: Option<FrontendError> = None;
    let mut max_retry_after_secs: Option<u64> = None;

    for (ch, pks) in pks_by_channel {
        if !state.is_generation_current(session.generation) {
            return Err(FrontendError::from(
                "Active wallet session changed while pushing channel members".to_string(),
            ));
        }
        let norm_write = normalize_sync_url(&admin.descriptor.write_url);
        let key = format!("{}::{}", norm_write, ch);
        let hash = compute_channel_members_hash(&pks);
        if admin.last_members_hash.get(&key) == Some(&hash) {
            log::debug!("Channel {} members unchanged, skipping upload", ch);
            continue;
        }

        match sync_client::push_members(client, &admin.descriptor.write_url, admin_sk, ch, &pks)
            .await
        {
            Ok(()) => {
                admin.last_members_hash.insert(key.clone(), hash.clone());
                pushed_hashes.push((key, hash));
            }
            Err(e) => {
                state.record_origin_cooldown_for_sync_error(&admin.descriptor.write_url, &e);
                if let Some(secs) = e.retry_after_secs() {
                    max_retry_after_secs =
                        Some(max_retry_after_secs.map_or(secs, |prev| prev.max(secs)));
                }
                log::warn!("Failed to push members for channel {}: {}", ch, e);
                if first_error.is_none() {
                    first_error = Some(FrontendError::from(e));
                }
            }
        }
    }

    let pushed_count = pushed_hashes.len();
    if !pushed_hashes.is_empty() {
        state.with_session_required(
            session,
            "Active wallet session changed before channel member hashes could be saved",
            |service| {
                let Some(mut target_admin) = load_stored_sync_admin_opt(service, password)? else {
                    return Ok(());
                };
                if target_admin.sync_index != admin.sync_index
                    || humoco_market::canonicalize_write_url(&target_admin.descriptor.write_url)
                        .ok()
                        != humoco_market::canonicalize_write_url(&admin.descriptor.write_url).ok()
                {
                    return Ok(());
                }
                for (key, hash) in &pushed_hashes {
                    target_admin
                        .last_members_hash
                        .insert(key.clone(), hash.clone());
                }
                save_stored_sync_admin(state, service, &target_admin, password)
            },
        )?;
    }

    if let Some(mut err) = first_error {
        if let Some(secs) = max_retry_after_secs {
            err.details
                .insert("retryAfterSecs".to_string(), secs.to_string());
        }
        return Err(err);
    }

    Ok(pushed_count)
}

async fn refresh_own_admin_members_inner(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
) -> Option<(String, Option<u64>)> {
    let Ok(Some(admin_opt)) =
        state.with_session(session, |service| {
            match load_stored_sync_admin_opt(service, password) {
                Ok(Some(admin)) => {
                    if let Ok(seed) = get_wallet_master_seed(service, password) {
                        let (admin_sk, _, _) =
                            humoco_market::derive_drop_admin_keypair(&seed, admin.sync_index);
                        Ok(Some((admin, admin_sk)))
                    } else {
                        Ok(None)
                    }
                }
                Ok(None) => Ok(None),
                Err(e) => {
                    log::warn!(
                        "Failed to load StoredSyncAdmin in refresh_own_admin_members: {}",
                        e.message
                    );
                    Ok(None)
                }
            }
        })
    else {
        return None;
    };

    if let Some((mut admin, admin_sk)) = admin_opt {
        if admin.descriptor.write_url.trim().is_empty() {
            return None;
        }
        if state.is_origin_cooling_down(&admin.descriptor.write_url, std::time::Instant::now()) {
            return Some((
                format!(
                    "Member push to {} deferred: server origin cooling down after HTTP 429",
                    admin.descriptor.write_url
                ),
                None,
            ));
        }
        if let Err(err) =
            push_all_channel_members(client, state, session, &mut admin, &admin_sk, password).await
        {
            let retry_after = err
                .details
                .get("retryAfterSecs")
                .and_then(|s| s.parse::<u64>().ok());
            return Some((
                format!(
                    "Failed to push channel members to {}: {}",
                    admin.descriptor.write_url, err.message
                ),
                retry_after,
            ));
        }
    }
    None
}

pub(crate) async fn refresh_own_admin_members(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
) -> Option<String> {
    let _market_guard = state.market_op_lock.lock().await;
    refresh_own_admin_members_inner(client, state, session, password)
        .await
        .map(|(msg, _)| msg)
}

pub(crate) async fn refresh_own_admin_members_locked(
    client: &reqwest::Client,
    state: &AppState,
    session: &SessionToken,
    password: Option<&str>,
    err_tracker: &mut super::SyncErrorTracker,
) {
    if let Some((msg, retry_after)) =
        refresh_own_admin_members_inner(client, state, session, password).await
    {
        err_tracker.record_retry_after(retry_after);
        err_tracker.push(msg);
    }
}

#[cfg(test)]
pub async fn sync_channel_members_impl(
    password: Option<&str>,
    state: &AppState,
) -> Result<usize, FrontendError> {
    let _market_guard = state.market_op_lock.lock().await;
    let auth = state.authorize_session(crate::state::RequiredAccess::Unlocked, password)?;
    let session = &auth.token;
    let pw = auth.pw();
    let Some((mut admin, admin_sk)) =
        state.with_session_required(session, "Active wallet session changed", |service| {
            let Some(admin) = load_stored_sync_admin_opt(service, pw)? else {
                return Ok(None);
            };
            let master_seed = get_wallet_master_seed(service, pw)?;
            let (admin_sk, _, _) =
                humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
            Ok(Some((admin, admin_sk)))
        })?
    else {
        return Ok(0);
    };

    if admin.descriptor.write_url.is_empty() {
        return Ok(0);
    }

    let client = sync_client::create_sync_http_client();
    push_all_channel_members(&client, state, session, &mut admin, &admin_sk, pw).await
}

pub fn prepare_sync_setup_impl(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    max_total_storage_mb: Option<usize>,
    max_user_storage_mb: Option<usize>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncSetupDraft, FrontendError> {
    let mut service =
        state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
    let master_seed = get_wallet_master_seed(&mut service, password.as_deref())?;

    let drop_index = 0u32;
    let (_admin_sk, _admin_vk, admin_pubkey) =
        humoco_market::derive_drop_admin_keypair(&master_seed, drop_index);
    let data_dir = humoco_market::derive_drop_data_dir(&master_seed, drop_index);

    let card_policy = policy.unwrap_or_default();
    let max_mem = max_members.unwrap_or(300);
    let max_storage_mb = max_total_storage_mb.unwrap_or(2048);
    let max_user_mb = max_user_storage_mb.unwrap_or(25);
    validate_admin_quotas(max_storage_mb, max_mem, max_user_mb)?;

    let mut policies = BTreeMap::new();
    policies.insert("card".to_string(), card_policy.clone().into());
    for ch in ["wot", "backup", "post", "lock"] {
        policies.insert(ch.to_string(), default_policy_for_channel(ch));
    }

    let stored_admin = StoredSyncAdmin {
        descriptor: humoco_market::DropDescriptor {
            write_url: String::new(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: admin_pubkey.clone(),
        },
        sync_index: drop_index,
        data_dir: data_dir.clone(),
        max_members: max_mem,
        max_total_storage_mb: max_storage_mb,
        max_user_storage_mb: max_user_mb,
        policies,
        allow: Vec::new(),
        last_members_hash: BTreeMap::new(),
        revocation: None,
    };

    save_stored_sync_admin(state, &mut service, &stored_admin, password.as_deref())?;

    let zip_bytes = crate::sync_zip::build_sync_zip_bytes(
        &admin_pubkey,
        &data_dir,
        max_mem,
        max_storage_mb,
        max_user_mb,
        None,
    );
    let zip_base64 = base64::engine::general_purpose::STANDARD.encode(&zip_bytes);
    let rust_config_toml = crate::sync_zip::generate_rust_sync_config_toml(
        &admin_pubkey,
        &data_dir,
        max_mem,
        max_storage_mb,
        max_user_mb,
        None,
    );

    Ok(FrontendSyncSetupDraft {
        descriptor: stored_admin.descriptor.into(),
        data_dir,
        admin_pubkey,
        max_members: max_mem,
        max_total_storage_mb: max_storage_mb,
        max_user_storage_mb: max_user_mb,
        policy: card_policy,
        zip_base64,
        rust_config_toml,
    })
}

pub fn get_rust_sync_config_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<String, FrontendError> {
    let mut service =
        state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;
    if let Some(admin) = load_stored_sync_admin_opt(&mut service, password.as_deref())? {
        let write_url_trimmed = admin.descriptor.write_url.trim();
        let public_url_hint = (!write_url_trimmed.is_empty()).then_some(write_url_trimmed);
        return Ok(crate::sync_zip::generate_rust_sync_config_toml(
            &admin.descriptor.drop_pub,
            &admin.data_dir,
            admin.max_members,
            admin.max_total_storage_mb,
            admin.max_user_storage_mb,
            public_url_hint,
        ));
    }

    let master_seed = get_wallet_master_seed(&mut service, password.as_deref())?;
    let drop_index = 0u32;
    let (_admin_sk, _admin_vk, admin_pubkey) =
        humoco_market::derive_drop_admin_keypair(&master_seed, drop_index);
    let data_dir = humoco_market::derive_drop_data_dir(&master_seed, drop_index);
    Ok(crate::sync_zip::generate_rust_sync_config_toml(
        &admin_pubkey,
        &data_dir,
        300,
        2048,
        25,
        None,
    ))
}

pub async fn finalize_sync_setup_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let write_url = humoco_market::canonicalize_write_url(&base_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&write_url).map_err(FrontendError::from)?;

    let _market_guard = state.market_op_lock.lock().await;

    let (mut admin, admin_sk, auth) = load_admin_with_signing_key(state, password.as_deref())?;
    let session = &auth.token;
    let pw = auth.pw();
    let expected_sync_index = admin.sync_index;

    let client = sync_client::create_sync_http_client();

    // Live-check the running sync server before activating or persisting write_url.
    let info = sync_client::fetch_sync_info(&client, &write_url)
        .await
        .map_err(|e| {
            FrontendError::from(format!(
                "Sync server unreachable or invalid at {}: {}",
                write_url, e
            ))
        })?;
    let info_drop_pub = canonical_drop_pub_hex(&info.drop_pub).map_err(|e| {
        FrontendError::from(format!(
            "Sync server at {} returned invalid drop_pub in ?info: {}",
            write_url, e.message
        ))
    })?;
    if info_drop_pub != admin.descriptor.drop_pub.trim().to_ascii_lowercase() {
        return Err(FrontendError::from(format!(
            "Sync server admin public key mismatch at {}: expected {}, got {}",
            write_url, admin.descriptor.drop_pub, info.drop_pub
        )));
    }
    let server_read_url = normalize_sync_url(&info.read_url);
    if server_read_url.is_empty() {
        return Err(FrontendError::from(format!(
            "Sync server at {} returned an empty read_url in ?info",
            write_url
        )));
    }
    sync_client::validate_sync_read_url(&server_read_url).map_err(|e| {
        FrontendError::from(format!(
            "Sync server at {} returned an invalid read_url ('{}') in ?info (check public_base_url / HTTPS config): {}",
            write_url, server_read_url, e
        ))
    })?;
    sync_client::list_index(&client, &server_read_url, None, None)
        .await
        .map_err(|e| {
            FrontendError::from(format!(
                "Sync server read_url '{}' (reported by ?info at {}) is unreachable — check public_base_url / data_dir in server config: {}",
                server_read_url, write_url, e
            ))
        })?;

    if !state.is_generation_current(session.generation) {
        return Err(FrontendError::from(
            "Active wallet session changed before sync admin verification completed".to_string(),
        ));
    }

    sync_client::fetch_admin_status(&client, &write_url, &admin_sk)
        .await
        .map_err(|e| {
            FrontendError::from(format!(
                "Sync server admin verification failed at {}: {}",
                write_url, e
            ))
        })?;

    let mut lock_probe_result = None;
    let norm_write_url = normalize_sync_url(&write_url);
    let norm_read_url = normalize_sync_url(&server_read_url);
    if let Some(ref lock_info) = info.lock {
        let lock_pub_hex = lock_info.lock_pub.trim().to_ascii_lowercase();
        if humoco_market::parse_ed25519_pubkey_hex64(&lock_pub_hex).is_ok() {
            let delegation_valid = lock_info.delegation.as_ref().is_some_and(|d| {
                sync_client::verify_lock_delegation(
                    &lock_pub_hex,
                    d,
                    Some(&admin.descriptor.drop_pub),
                ) && normalize_sync_url(&d.write_url) == norm_write_url
                    && normalize_sync_url(&d.read_url) == norm_read_url
            });
            let mut ready = delegation_valid;
            if !delegation_valid {
                let min_next_issued = lock_info
                    .delegation
                    .as_ref()
                    .map(|d| d.issued_at.saturating_add(1))
                    .unwrap_or(0);
                let delegation_issued_at = sync_client::current_unix_timestamp()
                    .max(min_next_issued)
                    .max(1);
                match sync_client::push_lock_delegation(
                    &client,
                    &write_url,
                    &server_read_url,
                    &admin_sk,
                    &lock_pub_hex,
                    delegation_issued_at,
                )
                .await
                {
                    Ok(_) => ready = true,
                    Err(err) => {
                        log::warn!(
                            "Auto-pushing lock delegation for {} failed: {}",
                            write_url,
                            err
                        );
                    }
                }
            }
            if ready {
                let probe = sync_client::probe_lock_server(
                    &client,
                    &write_url,
                    Some(&admin.descriptor.drop_pub),
                )
                .await;
                lock_probe_result = Some(probe);
            }
        }
    }

    admin.descriptor.write_url = write_url.clone();
    admin.descriptor.read_url = server_read_url;

    if !state.is_generation_current(session.generation) {
        return Err(FrontendError::from(
            "Active wallet session changed before sync admin config could be pushed".to_string(),
        ));
    }

    let remote_status = sync_client::put_admin_config(
        &client,
        &write_url,
        &admin_sk,
        admin.max_total_storage_mb,
        admin.max_members,
        admin.max_user_storage_mb,
    )
    .await
    .map_err(FrontendError::from)?;

    push_all_channel_members(&client, state, session, &mut admin, &admin_sk, pw).await?;

    if !state.is_generation_current(session.generation) {
        return Err(FrontendError::from(
            "Active wallet session changed before sync admin could be saved".to_string(),
        ));
    }

    // Atomically persist finalized admin descriptor, selected sync policy, re-signed own card
    // (if published), and verified lock probe cache.
    let (saved_admin, stats) = state.with_session_required(
        session,
        "Active wallet session changed before sync admin could be saved",
        |service| {
            let mut fresh_admin = load_stored_sync_admin_opt(service, pw)?
                .ok_or_else(|| FrontendError::from("Sync admin not configured".to_string()))?;
            if fresh_admin.sync_index != expected_sync_index {
                return Err(FrontendError::from(
                    "Sync admin configuration changed concurrently".to_string(),
                ));
            }
            fresh_admin.descriptor.write_url = admin.descriptor.write_url.clone();
            fresh_admin.descriptor.read_url = admin.descriptor.read_url.clone();
            for (k, v) in &admin.last_members_hash {
                fresh_admin.last_members_hash.insert(k.clone(), v.clone());
            }

            let desc_dto: FrontendSyncDescriptor = fresh_admin.descriptor.clone().into();
            let norm_desc_read = normalize_sync_url(&desc_dto.read_url);
            let mut policy = state
                .get_or_load_security_policy(service, pw)
                .map_err(FrontendError::from)?;
            let policy_changed = if policy.market_selected_syncs.len() < 3
                && !policy
                    .market_selected_syncs
                    .iter()
                    .any(|d| normalize_sync_url(&d.read_url) == norm_desc_read)
            {
                policy.market_selected_syncs.push(desc_dto);
                true
            } else {
                false
            };

            let updated_profile = if policy_changed {
                build_resigned_own_card_profile_for_policy(state, service, &policy, pw)
            } else {
                None
            };

            let mut updated_lifecycle = None;
            if let Some(ref probe) = lock_probe_result {
                if !probe.lock_pub.trim().is_empty() {
                    let mut lc = state
                        .get_or_load_internal_lifecycle(service, pw)
                        .map_err(FrontendError::from)?;
                    if crate::services::common::upsert_verified_lock_probe(
                        &mut lc.l2_probe_cache,
                        probe.clone(),
                    ) {
                        updated_lifecycle = Some(lc);
                    }
                }
            }

            let mut batch = EncryptedBatchCommit::new();
            batch.sync_admin(fresh_admin.clone());
            if policy_changed {
                batch.security_policy(policy);
            }
            if let Some(prof) = updated_profile {
                batch.own_market_profile(prof);
            }
            batch
                .commit(state, service, pw)
                .map_err(FrontendError::from)?;

            if let Some(lc) = updated_lifecycle {
                state
                    .save_internal_lifecycle(service, lc, pw)
                    .map_err(FrontendError::from)?;
            }

            let (stats, _) =
                compute_admin_sync_members_with_stats(state, service, &fresh_admin, "card", pw)?;
            Ok((fresh_admin, stats))
        },
    )?;

    Ok(build_admin_status_dto(
        saved_admin,
        stats,
        Some(remote_status),
    ))
}

pub async fn test_sync_connection_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncConnectionTestResult, FrontendError> {
    let auth = state.authorize_session(crate::state::RequiredAccess::Full, password.as_deref())?;
    let session = &auth.token;
    let pw = auth.pw();
    let fail_result = |message: String, write_ok: bool| FrontendSyncConnectionTestResult {
        success: false,
        message,
        write_ok,
        read_ok: false,
    };

    let write_url = match humoco_market::canonicalize_write_url(&base_url) {
        Ok(u) => u,
        Err(e) => {
            return Ok(fail_result(format!("Invalid sync URL: {}", e), false));
        }
    };
    if let Err(e) = sync_client::validate_sync_url(&write_url) {
        return Ok(fail_result(e.to_string(), false));
    }

    let (admin, admin_sk, stats, member_pubkeys) = match state.with_session_required(
        session,
        "Active wallet session changed before sync connection test",
        |service| {
            let admin = load_stored_sync_admin_opt(service, pw)?
                .ok_or_else(|| FrontendError::from("Not configured".to_string()))?;
            let master_seed = get_wallet_master_seed(service, pw)?;
            let (admin_sk, _, _) =
                humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
            let (stats, member_pubkeys) =
                compute_admin_sync_members_with_stats(state, service, &admin, "card", pw)?;
            Ok((admin, admin_sk, stats, member_pubkeys))
        },
    ) {
        Ok(tup) => tup,
        Err(e) => return Ok(fail_result(e.message, false)),
    };

    let client = sync_client::create_sync_http_client();

    // Verify server identity (?info + exact non-empty drop_pub match) BEFORE pushing signed members.
    let info = match sync_client::fetch_sync_info(&client, &write_url).await {
        Ok(info) => info,
        Err(e) => return Ok(fail_result(format!("Read/info test failed: {}", e), false)),
    };
    let info_drop_pub = match canonical_drop_pub_hex(&info.drop_pub) {
        Ok(p) => p,
        Err(e) => {
            return Ok(fail_result(
                format!("Server returned invalid drop_pub in ?info: {}", e.message),
                false,
            ));
        }
    };
    if info_drop_pub != admin.descriptor.drop_pub.trim().to_ascii_lowercase() {
        return Ok(fail_result(
            format!(
                "Server admin public key mismatch: expected {}, got {}",
                admin.descriptor.drop_pub, info.drop_pub
            ),
            false,
        ));
    }
    if !state.is_generation_current(session.generation) {
        return Ok(fail_result(
            "Active wallet session changed before sync connection test completed".to_string(),
            false,
        ));
    }

    if let Err(e) =
        sync_client::push_members(&client, &write_url, &admin_sk, "card", &member_pubkeys).await
    {
        return Ok(fail_result(format!("Write test failed: {}", e), false));
    }

    // Verify public read_url validity and live index reachability.
    let server_read_url = normalize_sync_url(&info.read_url);
    if server_read_url.is_empty() {
        return Ok(fail_result(
            "Read test failed: server returned empty read_url in ?info".to_string(),
            true,
        ));
    }
    if let Err(e) = sync_client::validate_sync_read_url(&server_read_url) {
        return Ok(fail_result(
            format!(
                "Read test failed: invalid read_url '{}' in ?info (check public_base_url / HTTPS config): {}",
                server_read_url, e
            ),
            true,
        ));
    }
    if let Err(e) = sync_client::list_index(&client, &server_read_url, None, None).await {
        return Ok(fail_result(
            format!(
                "Read test failed on read_url '{}' (check public_base_url / data_dir in server config): {}",
                server_read_url, e
            ),
            true,
        ));
    }
    Ok(FrontendSyncConnectionTestResult {
        success: true,
        message: format!(
            "Connection verified successfully ({} active members, channels: {:?}).",
            stats.total_count, info.channels
        ),
        write_ok: true,
        read_ok: true,
    })
}

pub async fn get_sync_admin_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    let auth =
        state.authorize_session(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    let pw = auth.pw();

    let Some((admin, admin_sk, stats)) = state.with_session_required(
        &auth.token,
        "Active wallet session changed while loading sync admin status",
        |service| {
            if pw.is_none() && !service.is_session_active() {
                let _ = state.read_cache_slot_for_service(service, |c| &c.own_sync_descriptor);
                return Ok(None);
            }
            let Some(admin) = load_stored_sync_admin_opt(service, pw)? else {
                return Ok(None);
            };
            if admin.descriptor.write_url.trim().is_empty() {
                return Ok(None);
            }
            let Ok(master_seed) = get_wallet_master_seed(service, pw) else {
                return Ok(None);
            };
            let (admin_sk, _, _) =
                humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
            let (stats, _) =
                compute_admin_sync_members_with_stats(state, service, &admin, "card", pw)?;
            Ok(Some((admin, admin_sk, stats)))
        },
    )?
    else {
        return Ok(None);
    };

    let client = sync_client::create_sync_http_client();
    let remote_status = if !admin.descriptor.write_url.is_empty()
        && !state.is_origin_cooling_down(&admin.descriptor.write_url, std::time::Instant::now())
    {
        match sync_client::fetch_admin_status(&client, &admin.descriptor.write_url, &admin_sk).await
        {
            Ok(st) => Some(st),
            Err(err) => {
                state.record_origin_cooldown_for_sync_error(&admin.descriptor.write_url, &err);
                None
            }
        }
    } else {
        None
    };

    Ok(Some(build_admin_status_dto(admin, stats, remote_status)))
}

pub async fn update_sync_admin_config_impl(
    update: FrontendSyncAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let _market_guard = state.market_op_lock.lock().await;

    let (mut admin, admin_sk, auth) = load_admin_with_signing_key(state, password.as_deref())?;
    let session = &auth.token;
    let pw = auth.pw();
    let expected_sync_index = admin.sync_index;

    let next_card_policy: Option<humoco_market::MemberPolicy> = update.card_policy.map(Into::into);
    let next_allow = update.allow;
    let next_max_members = update.max_members;
    let next_max_total_mb = update.max_total_storage_mb;
    let next_max_user_mb = update.max_user_storage_mb;

    if let Some(ref cp) = next_card_policy {
        admin.policies.insert("card".to_string(), cp.clone());
    }
    if let Some(ref allow) = next_allow {
        admin.allow = allow.clone();
    }
    if let Some(m) = next_max_members {
        admin.max_members = m;
    }
    if let Some(mb) = next_max_total_mb {
        admin.max_total_storage_mb = mb;
    }
    if let Some(umb) = next_max_user_mb {
        admin.max_user_storage_mb = umb;
    }
    validate_admin_quotas(
        admin.max_total_storage_mb,
        admin.max_members,
        admin.max_user_storage_mb,
    )?;

    let pushed_status = if push_now && !admin.descriptor.write_url.is_empty() {
        if !state.is_generation_current(session.generation) {
            return Err(FrontendError::from(
                "Active wallet session changed before admin config could be pushed".to_string(),
            ));
        }
        let client = sync_client::create_sync_http_client();
        let st = sync_client::put_admin_config(
            &client,
            &admin.descriptor.write_url,
            &admin_sk,
            admin.max_total_storage_mb,
            admin.max_members,
            admin.max_user_storage_mb,
        )
        .await
        .map_err(FrontendError::from)?;
        push_all_channel_members(&client, state, session, &mut admin, &admin_sk, pw).await?;
        Some(st)
    } else {
        None
    };

    let (saved_admin, stats) = state.with_session_required(
        session,
        "Active wallet session changed before admin config could be saved",
        |service| {
            let mut fresh_admin = load_stored_sync_admin_opt(service, pw)?.ok_or_else(|| {
                FrontendError::from("Sync admin configuration was removed concurrently".to_string())
            })?;
            if fresh_admin.sync_index != expected_sync_index {
                return Err(FrontendError::from(
                    "Sync admin configuration changed concurrently".to_string(),
                ));
            }
            if let Some(cp) = next_card_policy.clone() {
                fresh_admin.policies.insert("card".to_string(), cp);
            }
            if let Some(allow) = next_allow.clone() {
                fresh_admin.allow = allow;
            }
            if let Some(m) = next_max_members {
                fresh_admin.max_members = m;
            }
            if let Some(mb) = next_max_total_mb {
                fresh_admin.max_total_storage_mb = mb;
            }
            if let Some(umb) = next_max_user_mb {
                fresh_admin.max_user_storage_mb = umb;
            }
            validate_admin_quotas(
                fresh_admin.max_total_storage_mb,
                fresh_admin.max_members,
                fresh_admin.max_user_storage_mb,
            )?;
            for (k, v) in &admin.last_members_hash {
                fresh_admin.last_members_hash.insert(k.clone(), v.clone());
            }

            save_stored_sync_admin(state, service, &fresh_admin, pw)?;
            let (stats, _) =
                compute_admin_sync_members_with_stats(state, service, &fresh_admin, "card", pw)?;
            Ok((fresh_admin, stats))
        },
    )?;

    Ok(build_admin_status_dto(saved_admin, stats, pushed_status))
}

pub fn delete_sync_admin_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let password_z = password.clone().map(Zeroizing::new);
    let pw = password_z.as_deref().map(String::as_str);
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Full, pw)?;
    let admin_opt = load_stored_sync_admin_opt(&mut service, pw)?;

    let policy_update = if let Some(ref admin) = admin_opt {
        prepare_policy_without_sync(state, &mut service, &admin.descriptor.read_url, pw)?
    } else {
        None
    };

    let (next_policy_opt, next_profile_opt) = match policy_update {
        Some((pol, prof)) => (Some(pol), prof),
        None => (None, None),
    };

    let mut batch = EncryptedBatchCommit::new();
    batch.clear_sync_admin();
    if let Some(pol) = next_policy_opt {
        batch.security_policy(pol);
    }
    if let Some(prof) = next_profile_opt {
        batch.own_market_profile(prof);
    }
    batch
        .commit(state, &mut service, pw)
        .map_err(FrontendError::from)?;

    Ok(())
}

pub async fn revoke_own_sync_server_impl(
    effective_at: Option<u64>,
    reason: Option<String>,
    replacement_write_url: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    if effective_at == Some(0) {
        return Err(FrontendError::from(
            "Revocation effective_at timestamp must be greater than 0".to_string(),
        ));
    }

    let _market_guard = state.market_op_lock.lock().await;

    let (admin, admin_sk, auth) = load_admin_with_signing_key(state, password.as_deref())?;
    let session = &auth.token;
    let pw = auth.pw();
    let expected_sync_index = admin.sync_index;

    if admin.descriptor.write_url.trim().is_empty() {
        return Err(FrontendError::from(
            "Sync server write_url is not configured".to_string(),
        ));
    }

    let client = sync_client::create_sync_http_client();
    let now_secs = sync_client::current_unix_timestamp();
    let min_next_issued = admin
        .revocation
        .as_ref()
        .map(|r| r.issued_at.saturating_add(1))
        .unwrap_or(0);
    let issued_at = now_secs.max(min_next_issued).max(1);

    if let Some(eff) = effective_at {
        let replacement = if let Some(ref rep_raw) = replacement_write_url {
            let trimmed = rep_raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                let canonical_rep = humoco_market::canonicalize_write_url(trimmed)
                    .map_err(|e| FrontendError::from(e.to_string()))?;
                sync_client::validate_sync_url(&canonical_rep).map_err(FrontendError::from)?;
                if normalize_sync_url(&canonical_rep)
                    .eq_ignore_ascii_case(&normalize_sync_url(&admin.descriptor.write_url))
                {
                    return Err(FrontendError::from(
                        "Replacement URL must differ from the revoked server URL".to_string(),
                    ));
                }
                let info = sync_client::fetch_sync_info(&client, &canonical_rep)
                    .await
                    .map_err(FrontendError::from)?;
                let rep_read_url = normalize_sync_url(&info.read_url);
                sync_client::validate_sync_read_url(&rep_read_url).map_err(FrontendError::from)?;
                let rep_drop_pub = canonical_drop_pub_hex(&info.drop_pub).map_err(|e| {
                    FrontendError::from(format!("Invalid replacement drop_pub: {}", e.message))
                })?;
                if rep_read_url
                    .eq_ignore_ascii_case(&normalize_sync_url(&admin.descriptor.read_url))
                {
                    return Err(FrontendError::from(
                        "Replacement read_url must differ from the revoked server read_url"
                            .to_string(),
                    ));
                }
                Some(sync_client::SyncReplacementTarget {
                    write_url: canonical_rep,
                    read_url: rep_read_url,
                    drop_pub: rep_drop_pub,
                })
            }
        } else {
            None
        };

        let notice =
            sync_client::sign_revocation_notice(&admin_sk, issued_at, eff, reason, replacement)
                .map_err(FrontendError::from)?;

        if !state.is_generation_current(session.generation) {
            return Err(FrontendError::from(
                "Active wallet session changed before revocation notice could be pushed"
                    .to_string(),
            ));
        }

        sync_client::push_revocation_notice(
            &client,
            &admin.descriptor.write_url,
            &admin_sk,
            Some(&notice),
            issued_at,
        )
        .await
        .map_err(FrontendError::from)?;

        // Reload fresh admin & atomically remove deprecated server from outgoing SdCard selection.
        state.with_session_required(
            session,
            "Active wallet session changed before revocation state could be saved",
            |service| {
                let Some(mut fresh_admin) = load_stored_sync_admin_opt(service, pw)? else {
                    return Err(FrontendError::from(
                        "Sync admin configuration was removed concurrently".to_string(),
                    ));
                };
                if fresh_admin.sync_index != expected_sync_index {
                    return Err(FrontendError::from(
                        "Sync admin configuration changed concurrently".to_string(),
                    ));
                }
                fresh_admin.revocation = Some(notice);

                let policy_update = prepare_policy_without_sync(
                    state,
                    service,
                    &fresh_admin.descriptor.read_url,
                    pw,
                )?;

                let mut batch = EncryptedBatchCommit::new();
                batch.sync_admin(fresh_admin);
                if let Some((pol, prof_opt)) = policy_update {
                    batch.security_policy(pol);
                    if let Some(prof) = prof_opt {
                        batch.own_market_profile(prof);
                    }
                }
                batch
                    .commit(state, service, pw)
                    .map_err(FrontendError::from)
            },
        )?;
    } else {
        if !state.is_generation_current(session.generation) {
            return Err(FrontendError::from(
                "Active wallet session changed before revocation notice could be cleared"
                    .to_string(),
            ));
        }

        sync_client::push_revocation_notice(
            &client,
            &admin.descriptor.write_url,
            &admin_sk,
            None,
            issued_at,
        )
        .await
        .map_err(FrontendError::from)?;

        state.with_session_required(
            session,
            "Active wallet session changed before revocation clearance could be saved",
            |service| {
                let Some(mut fresh_admin) = load_stored_sync_admin_opt(service, pw)? else {
                    return Err(FrontendError::from(
                        "Sync admin configuration was removed concurrently".to_string(),
                    ));
                };
                if fresh_admin.sync_index != expected_sync_index {
                    return Err(FrontendError::from(
                        "Sync admin configuration changed concurrently".to_string(),
                    ));
                }
                fresh_admin.revocation = None;
                save_stored_sync_admin(state, service, &fresh_admin, pw)
            },
        )?;
    }

    if !state.is_generation_current(session.generation) {
        return Ok(None);
    }

    get_sync_admin_status_impl(pw.map(str::to_string), state).await
}
