use std::collections::{BTreeMap, HashSet};

use base64::Engine;
use human_money_core::app_service::AppService;
use human_money_core::models::conflict::TrustStatus;
use sha2::Digest;

use crate::models::{
    FrontendError, FrontendMemberPolicyDto, FrontendSyncAdminStatus,
    FrontendSyncAdminUpdateRequest, FrontendSyncConnectionTestResult, FrontendSyncDescriptor,
    FrontendSyncSetupDraft,
};
use crate::services::common::{get_wallet_master_seed, normalize_sync_url};
use crate::services::market_service::sync_own_card_drops_on_settings_change;
use crate::state::{canonicalize_did, AppState, StoredSyncAdmin, MARKET_SYNC_ADMIN_KEY};
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
) -> Option<StoredSyncAdmin> {
    service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredSyncAdmin>(&bytes).ok())
}

fn load_admin_with_signing_key(
    state: &AppState,
    password: Option<&str>,
) -> Result<(StoredSyncAdmin, ed25519_dalek::SigningKey, Vec<u8>), FrontendError> {
    let mut service = state.lock_service()?;
    let bytes = service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password)
        .map_err(FrontendError::from)?;
    let admin = serde_json::from_slice::<StoredSyncAdmin>(&bytes)
        .map_err(|e| FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e)))?;
    let master_seed = get_wallet_master_seed(&mut service)?;
    let (admin_sk, _, _) = humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
    Ok((admin, admin_sk, bytes))
}

fn save_stored_sync_admin(
    service: &mut AppService,
    admin: &StoredSyncAdmin,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    let bytes = serde_json::to_vec(admin)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredSyncAdmin: {}", e)))?;
    service
        .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, &bytes, password)
        .map_err(FrontendError::from)
}

fn remove_sync_from_selected_settings(
    state: &AppState,
    service: &mut AppService,
    read_url: &str,
    password: Option<&str>,
) {
    let norm_target = normalize_sync_url(read_url);
    if norm_target.is_empty() {
        return;
    }
    let mut settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(service, password))
        .unwrap_or_default();
    let initial_len = settings.market_selected_syncs.len();
    settings
        .market_selected_syncs
        .retain(|d| normalize_sync_url(&d.read_url) != norm_target);
    if settings.market_selected_syncs.len() != initial_len {
        let selected_clone = settings.market_selected_syncs.clone();
        let _ = state.save_settings(service, settings, password);
        sync_own_card_drops_on_settings_change(service, &selected_clone, password);
    }
}

fn build_admin_status_dto(
    admin: StoredSyncAdmin,
    stats: humoco_market::DropMembersStats,
    active_slot_ids: Vec<String>,
    remote_status: Option<SyncServerAdminStatus>,
) -> FrontendSyncAdminStatus {
    let card_policy = admin.policies.get("card").cloned().unwrap_or_default();
    let admin_pubkey = admin.descriptor.drop_pub.clone();
    let (
        used_bytes,
        max_bytes,
        soft_warning,
        channel_counts,
        eff_max_members,
        eff_total_mb,
        eff_user_mb,
    ) = match remote_status {
        Some(s) => (
            Some(s.used_bytes),
            Some(s.max_bytes),
            s.soft_warning,
            s.channel_counts,
            s.max_members,
            (s.max_bytes / (1024 * 1024)) as usize,
            (s.max_user_bytes / (1024 * 1024)) as usize,
        ),
        None => (
            None,
            Some((admin.max_total_storage_mb as u64) * 1024 * 1024),
            false,
            BTreeMap::new(),
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
        deny: admin.deny,
        blocked_pubkeys: admin.blocked_pubkeys,
        direct_member_count: stats.direct_count,
        hop2_member_count: stats.hop2_count,
        total_member_count: stats.total_count,
        active_slot_ids,
        channel_counts,
        revocation: admin.revocation,
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
    let deny_list: Vec<String> = admin.deny.clone();

    let stats = humoco_market::compute_drop_members_with_stats(
        &direct,
        &bridges,
        &policy,
        &allow_list,
        &deny_list,
        admin.max_members,
    );

    let master_seed = get_wallet_master_seed(service)?;
    let (admin_sk, _admin_vk, _admin_pub) =
        humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);

    let blocked_set: HashSet<&str> = admin.blocked_pubkeys.iter().map(|s| s.as_str()).collect();

    let mut member_pubkeys = Vec::new();
    for did in &stats.member_dids {
        if let Ok(member_vk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
            if let Ok(stealth_pk) =
                humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &member_vk)
            {
                if !blocked_set.contains(stealth_pk.as_str()) {
                    member_pubkeys.push(stealth_pk);
                }
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
    admin: &mut StoredSyncAdmin,
    admin_sk: &ed25519_dalek::SigningKey,
    password: Option<&str>,
) -> Result<usize, FrontendError> {
    if admin.descriptor.write_url.is_empty() {
        return Ok(0);
    }
    let pks_by_channel = {
        let mut service = state.lock_service()?;
        let mut list = Vec::new();
        for ch in ["card", "wot", "backup", "post"] {
            let (_stats, pks) =
                compute_admin_sync_members_with_stats(state, &mut service, admin, ch, password)?;
            list.push((ch, pks));
        }
        list
    };

    let mut pushed_count = 0;
    let mut first_error: Option<FrontendError> = None;

    for (ch, pks) in pks_by_channel {
        let key = format!("{}::{}", &admin.descriptor.write_url, ch);
        let hash = compute_channel_members_hash(&pks);
        if admin.last_members_hash.get(&key) == Some(&hash) {
            log::debug!("Channel {} members unchanged, skipping upload", ch);
            continue;
        }

        match sync_client::push_members(client, &admin.descriptor.write_url, admin_sk, ch, &pks)
            .await
        {
            Ok(()) => {
                admin.last_members_hash.insert(key, hash);
                pushed_count += 1;
            }
            Err(e) => {
                log::warn!("Failed to push members for channel {}: {}", ch, e);
                if first_error.is_none() {
                    first_error = Some(FrontendError::from(e));
                }
            }
        }
    }

    if pushed_count > 0 {
        let mut service = state.lock_service()?;
        save_stored_sync_admin(&mut service, admin, password)?;
    }

    if let Some(err) = first_error {
        return Err(err);
    }

    Ok(pushed_count)
}

pub(crate) async fn refresh_own_admin_members(
    client: &reqwest::Client,
    state: &AppState,
    password: Option<&str>,
) {
    let admin_opt: Option<(StoredSyncAdmin, ed25519_dalek::SigningKey)> = {
        let Ok(mut service) = state.lock_service() else {
            return;
        };
        if let Some(admin) = load_stored_sync_admin_opt(&mut service, password) {
            if let Ok(seed) = get_wallet_master_seed(&mut service) {
                let (admin_sk, _, _) =
                    humoco_market::derive_drop_admin_keypair(&seed, admin.sync_index);
                Some((admin, admin_sk))
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some((mut admin, admin_sk)) = admin_opt {
        let _ = push_all_channel_members(client, state, &mut admin, &admin_sk, password).await;
    }
}

pub async fn sync_channel_members_impl(
    password: Option<&str>,
    state: &AppState,
) -> Result<usize, FrontendError> {
    let (mut admin, admin_sk) = {
        let mut service = state.lock_service()?;
        let Some(admin) = load_stored_sync_admin_opt(&mut service, password) else {
            return Ok(0);
        };
        let master_seed = get_wallet_master_seed(&mut service)?;
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk)
    };

    if admin.descriptor.write_url.is_empty() {
        return Ok(0);
    }

    let client = sync_client::create_sync_http_client();
    push_all_channel_members(&client, state, &mut admin, &admin_sk, password).await
}

pub fn prepare_sync_setup_impl(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    max_total_storage_mb: Option<usize>,
    max_user_storage_mb: Option<usize>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncSetupDraft, FrontendError> {
    let mut service = state.lock_service()?;
    let master_seed = get_wallet_master_seed(&mut service)?;

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
    for ch in ["wot", "backup", "post"] {
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
        register_url: None,
        policies,
        allow: Vec::new(),
        deny: Vec::new(),
        blocked_pubkeys: Vec::new(),
        last_members_hash: BTreeMap::new(),
        revocation: None,
    };

    save_stored_sync_admin(&mut service, &stored_admin, password.as_deref())?;

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
    let mut service = state.lock_service()?;
    if let Some(admin) = load_stored_sync_admin_opt(&mut service, password.as_deref()) {
        return Ok(crate::sync_zip::generate_rust_sync_config_toml(
            &admin.descriptor.drop_pub,
            &admin.data_dir,
            admin.max_members,
            admin.max_total_storage_mb,
            admin.max_user_storage_mb,
            admin.register_url.as_deref(),
        ));
    }

    let master_seed = get_wallet_master_seed(&mut service)?;
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

pub fn save_sync_setup_zip_impl(
    file_path: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.lock_service()?;
    let admin: StoredSyncAdmin = service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
        .map_err(FrontendError::from)
        .and_then(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|e| FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e)))
        })?;

    let zip_bytes = crate::sync_zip::build_sync_zip_bytes(
        &admin.descriptor.drop_pub,
        &admin.data_dir,
        admin.max_members,
        admin.max_total_storage_mb,
        admin.max_user_storage_mb,
        admin.register_url.as_deref(),
    );

    std::fs::write(&file_path, zip_bytes)
        .map_err(|e| FrontendError::from(format!("Failed to write ZIP file: {}", e)))?;

    Ok(())
}

pub async fn finalize_sync_setup_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let write_url = humoco_market::canonicalize_write_url(&base_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&write_url).map_err(FrontendError::from)?;

    let (mut admin, admin_sk, original_admin_bytes) =
        load_admin_with_signing_key(state, password.as_deref())?;

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
    if !info.drop_pub.is_empty() && info.drop_pub != admin.descriptor.drop_pub {
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
    sync_client::validate_sync_url(&server_read_url).map_err(|e| {
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

    sync_client::fetch_admin_status(&client, &write_url, &admin_sk)
        .await
        .map_err(|e| {
            FrontendError::from(format!(
                "Sync server admin verification failed at {}: {}",
                write_url, e
            ))
        })?;

    admin.descriptor.write_url = write_url.clone();
    admin.descriptor.read_url = server_read_url;

    // Push configured admin quotas & members; roll back MARKET_SYNC_ADMIN_KEY on failure.
    let remote_status = match sync_client::put_admin_config(
        &client,
        &write_url,
        &admin_sk,
        admin.max_total_storage_mb,
        admin.max_members,
        admin.max_user_storage_mb,
    )
    .await
    {
        Ok(st) => st,
        Err(err) => {
            let mut service = state.lock_service()?;
            let _ = service.save_encrypted_data(
                MARKET_SYNC_ADMIN_KEY,
                &original_admin_bytes,
                password.as_deref(),
            );
            return Err(FrontendError::from(err));
        }
    };

    if let Err(err) =
        push_all_channel_members(&client, state, &mut admin, &admin_sk, password.as_deref()).await
    {
        let mut service = state.lock_service()?;
        let _ = service.save_encrypted_data(
            MARKET_SYNC_ADMIN_KEY,
            &original_admin_bytes,
            password.as_deref(),
        );
        return Err(err);
    }

    // Add own drop to settings.market_selected_syncs if < 3
    {
        let mut service = state.lock_service()?;
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let desc_dto: FrontendSyncDescriptor = admin.descriptor.clone().into();
        let norm_desc_read = normalize_sync_url(&desc_dto.read_url);
        if settings.market_selected_syncs.len() < 3
            && !settings
                .market_selected_syncs
                .iter()
                .any(|d| normalize_sync_url(&d.read_url) == norm_desc_read)
        {
            settings.market_selected_syncs.push(desc_dto);
            let _ = state.save_settings(&mut service, settings, password.as_deref());
        }

        save_stored_sync_admin(&mut service, &admin, password.as_deref())?;
    }

    let (stats, active_slot_ids) = {
        let mut service = state.lock_service()?;
        compute_admin_sync_members_with_stats(
            state,
            &mut service,
            &admin,
            "card",
            password.as_deref(),
        )?
    };

    Ok(build_admin_status_dto(
        admin,
        stats,
        active_slot_ids,
        Some(remote_status),
    ))
}

pub async fn test_sync_connection_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncConnectionTestResult, FrontendError> {
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
        return Ok(fail_result(e, false));
    }

    let (admin, admin_sk) = {
        let mut service = state.lock_service()?;
        let admin = match service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredSyncAdmin>(&bytes).map_err(|e| {
                    FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e))
                })
            }) {
            Ok(a) => a,
            Err(e) => {
                return Ok(fail_result(
                    format!("Drop admin not configured: {}", e.message),
                    false,
                ));
            }
        };

        let master_seed = match get_wallet_master_seed(&mut service) {
            Ok(s) => s,
            Err(e) => {
                return Ok(fail_result(
                    format!("Failed to obtain wallet seed: {}", e.message),
                    false,
                ));
            }
        };

        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk)
    };

    let client = sync_client::create_sync_http_client();

    // Test write via push_members
    let (stats, member_pubkeys) = {
        let mut service = state.lock_service()?;
        match compute_admin_sync_members_with_stats(
            state,
            &mut service,
            &admin,
            "card",
            password.as_deref(),
        ) {
            Ok(res) => res,
            Err(e) => {
                return Ok(fail_result(
                    format!("Failed to compute drop members: {}", e.message),
                    false,
                ));
            }
        }
    };

    if let Err(e) =
        sync_client::push_members(&client, &write_url, &admin_sk, "card", &member_pubkeys).await
    {
        return Ok(fail_result(format!("Write test failed: {}", e), false));
    }

    // Test read via fetch_sync_info + live read_url index check
    match sync_client::fetch_sync_info(&client, &write_url).await {
        Ok(info) => {
            let server_read_url = normalize_sync_url(&info.read_url);
            if server_read_url.is_empty() {
                return Ok(fail_result(
                    "Read test failed: server returned empty read_url in ?info".to_string(),
                    true,
                ));
            }
            if let Err(e) = sync_client::validate_sync_url(&server_read_url) {
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
        Err(e) => Ok(fail_result(format!("Read test failed: {}", e), true)),
    }
}

pub async fn get_sync_admin_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    let (admin, admin_sk) = {
        let mut service = state.lock_service()?;
        let Some(admin) = load_stored_sync_admin_opt(&mut service, password.as_deref()) else {
            return Ok(None);
        };
        if admin.descriptor.write_url.trim().is_empty() {
            return Ok(None);
        }
        let Ok(master_seed) = get_wallet_master_seed(&mut service) else {
            return Ok(None);
        };
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk)
    };

    let (stats, active_slot_ids) = {
        let mut service = state.lock_service()?;
        compute_admin_sync_members_with_stats(
            state,
            &mut service,
            &admin,
            "card",
            password.as_deref(),
        )?
    };

    let client = sync_client::create_sync_http_client();
    let remote_status = if !admin.descriptor.write_url.is_empty() {
        sync_client::fetch_admin_status(&client, &admin.descriptor.write_url, &admin_sk)
            .await
            .ok()
    } else {
        None
    };

    Ok(Some(build_admin_status_dto(
        admin,
        stats,
        active_slot_ids,
        remote_status,
    )))
}

pub async fn update_sync_admin_config_impl(
    update: FrontendSyncAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let (mut admin, admin_sk, _) = load_admin_with_signing_key(state, password.as_deref())?;

    if let Some(cp) = update.card_policy {
        admin.policies.insert("card".to_string(), cp.into());
    }
    if let Some(allow) = update.allow {
        admin.allow = allow;
    }
    if let Some(deny) = update.deny {
        admin.deny = deny;
    }
    if let Some(blocked) = update.blocked_pubkeys {
        admin.blocked_pubkeys = blocked;
    }
    if let Some(m) = update.max_members {
        admin.max_members = m;
    }
    if let Some(mb) = update.max_total_storage_mb {
        admin.max_total_storage_mb = mb;
    }
    if let Some(umb) = update.max_user_storage_mb {
        admin.max_user_storage_mb = umb;
    }
    validate_admin_quotas(
        admin.max_total_storage_mb,
        admin.max_members,
        admin.max_user_storage_mb,
    )?;

    let pushed_status = if push_now && !admin.descriptor.write_url.is_empty() {
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
        push_all_channel_members(&client, state, &mut admin, &admin_sk, password.as_deref())
            .await?;
        Some(st)
    } else {
        None
    };

    let (stats, active_slot_ids) = {
        let mut service = state.lock_service()?;
        save_stored_sync_admin(&mut service, &admin, password.as_deref())?;
        compute_admin_sync_members_with_stats(
            state,
            &mut service,
            &admin,
            "card",
            password.as_deref(),
        )?
    };

    Ok(build_admin_status_dto(
        admin,
        stats,
        active_slot_ids,
        pushed_status,
    ))
}

pub async fn block_sync_slot_impl(
    pubkey_hex64: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let blocked = {
        let mut service = state.lock_service()?;
        let admin: StoredSyncAdmin = service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice(&bytes).map_err(|e| {
                    FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e))
                })
            })?;
        let mut blocked = admin.blocked_pubkeys;
        if !blocked.contains(&pubkey_hex64) {
            blocked.push(pubkey_hex64);
        }
        blocked
    };

    let update = FrontendSyncAdminUpdateRequest {
        blocked_pubkeys: Some(blocked),
        ..Default::default()
    };

    update_sync_admin_config_impl(update, true, password, state).await
}

pub fn delete_sync_admin_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.lock_service()?;
    let admin_opt = load_stored_sync_admin_opt(&mut service, password.as_deref());

    service
        .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, b"null", password.as_deref())
        .map_err(FrontendError::from)?;

    if let Some(admin) = admin_opt {
        remove_sync_from_selected_settings(
            state,
            &mut service,
            &admin.descriptor.read_url,
            password.as_deref(),
        );
    }

    Ok(())
}

pub async fn revoke_own_sync_server_impl(
    effective_at: Option<u64>,
    reason: Option<String>,
    replacement_write_url: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    let (mut admin, admin_sk, _) = load_admin_with_signing_key(state, password.as_deref())?;

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
    let issued_at = now_secs.max(min_next_issued);

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
                if normalize_sync_url(&info.read_url)
                    .eq_ignore_ascii_case(&normalize_sync_url(&admin.descriptor.read_url))
                {
                    return Err(FrontendError::from(
                        "Replacement read_url must differ from the revoked server read_url"
                            .to_string(),
                    ));
                }
                Some(sync_client::SyncReplacementTarget {
                    write_url: canonical_rep,
                    read_url: info.read_url,
                    drop_pub: info.drop_pub,
                })
            }
        } else {
            None
        };

        let notice =
            sync_client::sign_revocation_notice(&admin_sk, issued_at, eff, reason, replacement)
                .map_err(FrontendError::from)?;

        sync_client::push_revocation_notice(
            &client,
            &admin.descriptor.write_url,
            &admin_sk,
            Some(&notice),
            issued_at,
        )
        .await
        .map_err(FrontendError::from)?;

        admin.revocation = Some(notice);

        // Remove deprecated own server from outgoing SdCard selection so new contacts don't receive it.
        {
            let mut service = state.lock_service()?;
            save_stored_sync_admin(&mut service, &admin, password.as_deref())?;
            remove_sync_from_selected_settings(
                state,
                &mut service,
                &admin.descriptor.read_url,
                password.as_deref(),
            );
        }
    } else {
        sync_client::push_revocation_notice(
            &client,
            &admin.descriptor.write_url,
            &admin_sk,
            None,
            issued_at,
        )
        .await
        .map_err(FrontendError::from)?;

        admin.revocation = None;
        let mut service = state.lock_service()?;
        save_stored_sync_admin(&mut service, &admin, password.as_deref())?;
    }

    get_sync_admin_status_impl(password, state).await
}
