// src-tauri/src/commands/sync.rs
use std::collections::{BTreeMap, HashSet};
use base64::Engine;
use tauri::State;

use crate::sync_client::{self as sync_client, IndexFetchResult, PutSlotOutcome, SlotFetchResult};
use crate::models::{
    FrontendDiscoveredSync, FrontendSyncAdminStatus, FrontendSyncAdminUpdateRequest,
    FrontendSyncConnectionTestResult, FrontendSyncDescriptor, FrontendSyncOverviewStatus,
    FrontendSyncSetupDraft, FrontendMarketSyncResult, FrontendError, FrontendMemberPolicyDto,
    FrontendPublicSyncInfo, FrontendTrustChangeImpact,
};
use crate::settings::{
    StoredPublicSync, MAX_POLLED_SYNCS, SYNC_BASE_MAX_AGE_SECS, SYNC_CARD_MAX_BYTES,
    SYNC_HEARTBEAT_SECS, SYNC_MARKET_POLL_COOLDOWN_SECS, SYNC_WOT_MAX_BYTES,
};
use crate::state::{
    canonicalize_did, AppState, StoredSyncAdmin, StoredSyncState,
    CONTACTS_DATA_NAME, MARKET_CARD_STORE_KEY, MARKET_SYNC_ADMIN_KEY, MARKET_SYNC_KEY,
};
use human_money_core::app_service::AppService;
use sha2::Digest;

fn get_wallet_master_seed(service: &mut AppService) -> Result<[u8; 32], FrontendError> {
    let signing_key = service
        .with_wallet_and_identity(|_w, id| id.signing_key.clone())
        .map_err(FrontendError::from)?;
    Ok(signing_key.to_bytes())
}

/// Computes the members for the admin's drop for a specific channel along with stats and 64-hex stealth pubkeys.
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
        .unwrap_or_else(|| {
            if channel == "backup" {
                humoco_market::MemberPolicy {
                    direct_min: 2,
                    hop2_enabled: false,
                    bridge_min: 2,
                    hop2_min: 2,
                }
            } else {
                humoco_market::MemberPolicy {
                    direct_min: 2,
                    hop2_enabled: true,
                    bridge_min: 2,
                    hop2_min: 2,
                }
            }
        });

    let admin_own_did = service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)?;
    let admin_own_can_did =
        canonicalize_did(&admin_own_did).unwrap_or_else(|_| admin_own_did.clone());

    let reg = state
        .get_or_load_wot_register(service, password)
        .map_err(FrontendError::from)?;

    let is_offender = |did: &str| -> bool {
        use human_money_core::models::conflict::TrustStatus;
        service
            .with_wallet(|w| matches!(w.check_reputation(did), TrustStatus::KnownOffender(_)))
            .unwrap_or(false)
    };

    // 1. Direct contacts from reg.own_contacts
    let direct: Vec<(String, u8)> = reg
        .own_contacts
        .iter()
        .filter(|(did, _)| !is_offender(did))
        .map(|(did, e)| (did.clone(), e.level))
        .collect();

    // 2. Hop 2 bridges from reg.raw_list_store
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

    // 3. Allow list: explicitly configured allow list PLUS admin's own canonical DID
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

    // Derive admin keys
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
        let mut service = state.service.lock().unwrap();
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

        match sync_client::push_members(client, &admin.descriptor.write_url, admin_sk, ch, &pks).await {
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
        let mut service = state.service.lock().unwrap();
        let admin_bytes = serde_json::to_vec(admin).map_err(|e| {
            FrontendError::from(format!("Failed to serialize StoredSyncAdmin: {}", e))
        })?;
        service
            .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, &admin_bytes, password)
            .map_err(FrontendError::from)?;
    }

    if let Some(err) = first_error {
        return Err(err);
    }

    Ok(pushed_count)
}

#[tauri::command]
pub async fn sync_channel_members(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, FrontendError> {
    sync_channel_members_impl(password.as_deref(), &state).await
}

pub async fn sync_channel_members_impl(
    password: Option<&str>,
    state: &AppState,
) -> Result<usize, FrontendError> {
    let (mut admin, admin_sk) = {
        let mut service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        let bytes = match service.load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password) {
            Ok(b) => b,
            Err(_) => return Ok(0),
        };
        let admin: StoredSyncAdmin = match serde_json::from_slice(&bytes) {
            Ok(a) => a,
            Err(_) => return Ok(0),
        };
        let master_seed = get_wallet_master_seed(&mut service)?;
        let (admin_sk, _, _) = humoco_market::derive_drop_admin_keypair(
            &master_seed,
            admin.sync_index,
        );
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
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncSetupDraft, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let master_seed = get_wallet_master_seed(&mut service)?;

    let drop_index = 0u32;
    let (_admin_sk, _admin_vk, admin_pubkey) =
        humoco_market::derive_drop_admin_keypair(&master_seed, drop_index);
    let data_dir = humoco_market::derive_drop_data_dir(&master_seed, drop_index);

    let card_policy = policy.unwrap_or_default();
    let max_mem = max_members.unwrap_or(300);
    let max_storage_mb = max_total_storage_mb.unwrap_or(256);

    let mut policies = BTreeMap::new();
    policies.insert("card".to_string(), card_policy.clone().into());
    policies.insert(
        "wot".to_string(),
        humoco_market::MemberPolicy {
            direct_min: 2,
            hop2_enabled: true,
            bridge_min: 2,
            hop2_min: 2,
        },
    );
    policies.insert(
        "backup".to_string(),
        humoco_market::MemberPolicy {
            direct_min: 2,
            hop2_enabled: false,
            bridge_min: 2,
            hop2_min: 2,
        },
    );
    policies.insert(
        "post".to_string(),
        humoco_market::MemberPolicy {
            direct_min: 2,
            hop2_enabled: true,
            bridge_min: 2,
            hop2_min: 2,
        },
    );

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
        register_url: None,
        policies,
        allow: Vec::new(),
        deny: Vec::new(),
        blocked_pubkeys: Vec::new(),
        last_members_hash: BTreeMap::new(),
        revocation: None,
    };

    let bytes = serde_json::to_vec(&stored_admin)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredSyncAdmin: {}", e)))?;
    service
        .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    let zip_bytes = crate::sync_zip::build_sync_zip_bytes(
        &admin_pubkey,
        &data_dir,
        max_mem,
        max_storage_mb,
        None,
    );
    let zip_base64 = base64::engine::general_purpose::STANDARD.encode(&zip_bytes);
    let rust_config_toml = crate::sync_zip::generate_rust_sync_config_toml(
        &admin_pubkey,
        &data_dir,
        max_mem,
        max_storage_mb,
        None,
    );

    Ok(FrontendSyncSetupDraft {
        descriptor: stored_admin.descriptor.into(),
        data_dir,
        admin_pubkey,
        max_members: max_mem,
        max_total_storage_mb: max_storage_mb,
        policy: card_policy,
        zip_base64,
        rust_config_toml,
    })
}

#[tauri::command]
pub fn prepare_sync_setup(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    max_total_storage_mb: Option<usize>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncSetupDraft, FrontendError> {
    prepare_sync_setup_impl(policy, max_members, max_total_storage_mb, password, &state)
}

pub fn get_rust_sync_config_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<String, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let admin_opt: Option<StoredSyncAdmin> = service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    if let Some(admin) = admin_opt {
        return Ok(crate::sync_zip::generate_rust_sync_config_toml(
            &admin.descriptor.drop_pub,
            &admin.data_dir,
            admin.max_members,
            admin.max_total_storage_mb,
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
        256,
        None,
    ))
}

#[tauri::command]
pub fn get_rust_sync_config(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, FrontendError> {
    get_rust_sync_config_impl(password, &state)
}

pub fn save_sync_setup_zip_impl(
    file_path: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();
    let admin: StoredSyncAdmin = service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
        .map_err(FrontendError::from)
        .and_then(|bytes| {
            serde_json::from_slice(&bytes).map_err(|e| {
                FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e))
            })
        })?;

    let zip_bytes = crate::sync_zip::build_sync_zip_bytes(
        &admin.descriptor.drop_pub,
        &admin.data_dir,
        admin.max_members,
        admin.max_total_storage_mb,
        admin.register_url.as_deref(),
    );

    std::fs::write(&file_path, zip_bytes)
        .map_err(|e| FrontendError::from(format!("Failed to write ZIP file: {}", e)))?;

    Ok(())
}

#[tauri::command]
pub fn save_sync_setup_zip(
    file_path: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    save_sync_setup_zip_impl(file_path, password, &state)
}

pub async fn finalize_sync_setup_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&base_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;

    let (mut admin, admin_sk, original_admin_bytes) = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        let admin = serde_json::from_slice::<StoredSyncAdmin>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e)))?;
        let master_seed = get_wallet_master_seed(&mut service)?;
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk, bytes)
    };

    let write_url = canonical_url;
    let read_base = write_url.strip_suffix("/sync.php").unwrap_or(&write_url);
    let read_url = format!("{}/{}", read_base, admin.data_dir);

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
    let remote_status = sync_client::fetch_admin_status(&client, &write_url, &admin_sk)
        .await
        .map_err(|e| {
            FrontendError::from(format!(
                "Sync server admin verification failed at {}: {}",
                write_url, e
            ))
        })?;

    admin.descriptor.write_url = write_url.clone();
    admin.descriptor.read_url = read_url.clone();

    // Push members to all 4 channels; roll back MARKET_SYNC_ADMIN_KEY if any channel fails.
    if let Err(err) = push_all_channel_members(
        &client,
        state,
        &mut admin,
        &admin_sk,
        password.as_deref(),
    )
    .await
    {
        let mut service = state.service.lock().unwrap();
        let _ = service.save_encrypted_data(
            MARKET_SYNC_ADMIN_KEY,
            &original_admin_bytes,
            password.as_deref(),
        );
        return Err(err);
    }

    // Add own drop to settings.market_selected_syncs if < 3
    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let desc_dto: FrontendSyncDescriptor = admin.descriptor.clone().into();
        if settings.market_selected_syncs.len() < 3
            && !settings
                .market_selected_syncs
                .iter()
                .any(|d| d.read_url == desc_dto.read_url)
        {
            settings.market_selected_syncs.push(desc_dto);
            let _ = state.save_settings(&mut service, settings, password.as_deref());
        }

        let admin_bytes = serde_json::to_vec(&admin).map_err(|e| {
            FrontendError::from(format!("Failed to serialize StoredSyncAdmin: {}", e))
        })?;
        service
            .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, &admin_bytes, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    let (stats, active_slot_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_sync_members_with_stats(state, &mut service, &admin, "card", password.as_deref())?
    };

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(FrontendSyncAdminStatus {
        descriptor: admin.descriptor.clone().into(),
        admin_pubkey: admin.descriptor.drop_pub.clone(),
        data_dir: admin.data_dir,
        max_members: admin.max_members,
        max_total_storage_mb: admin.max_total_storage_mb,
        used_bytes: Some(remote_status.used_bytes),
        max_bytes: Some(remote_status.max_bytes),
        soft_warning: remote_status.soft_warning,
        card_policy: card_policy.into(),
        allow: admin.allow,
        deny: admin.deny,
        blocked_pubkeys: admin.blocked_pubkeys,
        direct_member_count: stats.direct_count,
        hop2_member_count: stats.hop2_count,
        total_member_count: stats.total_count,
        active_slot_ids,
        channel_counts: remote_status.channel_counts,
        revocation: admin.revocation.clone(),
    })
}

#[tauri::command]
pub async fn finalize_sync_setup(
    base_url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    finalize_sync_setup_impl(base_url, password, &state).await
}

pub async fn test_sync_connection_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncConnectionTestResult, FrontendError> {
    let canonical_url = match humoco_market::canonicalize_write_url(&base_url) {
        Ok(u) => u,
        Err(e) => {
            return Ok(FrontendSyncConnectionTestResult {
                success: false,
                message: format!("Ungültige Sync-Adresse: {}", e),
                write_ok: false,
                read_ok: false,
            });
        }
    };
    if let Err(e) = sync_client::validate_sync_url(&canonical_url) {
        return Ok(FrontendSyncConnectionTestResult {
            success: false,
            message: e,
            write_ok: false,
            read_ok: false,
        });
    }

    let (admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let admin_res = service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredSyncAdmin>(&bytes).map_err(|e| {
                    FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e))
                })
            });

        let admin = match admin_res {
            Ok(a) => a,
            Err(e) => {
                return Ok(FrontendSyncConnectionTestResult {
                    success: false,
                    message: format!("Drop admin not configured: {}", e.message),
                    write_ok: false,
                    read_ok: false,
                });
            }
        };

        let master_seed = match get_wallet_master_seed(&mut service) {
            Ok(s) => s,
            Err(e) => {
                return Ok(FrontendSyncConnectionTestResult {
                    success: false,
                    message: format!("Failed to obtain wallet seed: {}", e.message),
                    write_ok: false,
                    read_ok: false,
                });
            }
        };

        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk)
    };

    let client = sync_client::create_sync_http_client();
    let write_url = canonical_url;

    // Test write via push_members
    let (stats, member_pubkeys) = {
        let mut service = state.service.lock().unwrap();
        match compute_admin_sync_members_with_stats(state, &mut service, &admin, "card", password.as_deref()) {
            Ok(res) => res,
            Err(e) => {
                return Ok(FrontendSyncConnectionTestResult {
                    success: false,
                    message: format!("Failed to compute drop members: {}", e.message),
                    write_ok: false,
                    read_ok: false,
                });
            }
        }
    };

    if let Err(e) = sync_client::push_members(&client, &write_url, &admin_sk, "card", &member_pubkeys).await {
        return Ok(FrontendSyncConnectionTestResult {
            success: false,
            message: format!("Write test failed: {}", e),
            write_ok: false,
            read_ok: false,
        });
    }

    // Test read via fetch_drop_info
    match sync_client::fetch_sync_info(&client, &write_url).await {
        Ok(info) => {
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
        Err(e) => {
            Ok(FrontendSyncConnectionTestResult {
                success: false,
                message: format!("Read test failed: {}", e),
                write_ok: true,
                read_ok: false,
            })
        }
    }
}

#[tauri::command]
pub async fn test_sync_connection(
    base_url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncConnectionTestResult, FrontendError> {
    test_sync_connection_impl(base_url, password, &state).await
}

pub async fn get_sync_admin_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    let (admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let bytes = match service.load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref()) {
            Ok(b) => b,
            Err(_) => return Ok(None),
        };
        let admin: StoredSyncAdmin = match serde_json::from_slice(&bytes) {
            Ok(a) => a,
            Err(_) => return Ok(None),
        };
        if admin.descriptor.write_url.trim().is_empty() {
            return Ok(None);
        }
        let master_seed = match get_wallet_master_seed(&mut service) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk)
    };

    let (stats, active_slot_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_sync_members_with_stats(state, &mut service, &admin, "card", password.as_deref())?
    };

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    let client = sync_client::create_sync_http_client();
    let remote_status = if !admin.descriptor.write_url.is_empty() {
        sync_client::fetch_admin_status(&client, &admin.descriptor.write_url, &admin_sk)
            .await
            .ok()
    } else {
        None
    };

    let (used_bytes, max_bytes, soft_warning, channel_counts) = match remote_status {
        Some(s) => (
            Some(s.used_bytes),
            Some(s.max_bytes),
            s.soft_warning,
            s.channel_counts,
        ),
        None => (
            None,
            Some((admin.max_total_storage_mb as u64) * 1024 * 1024),
            false,
            BTreeMap::new(),
        ),
    };

    Ok(Some(FrontendSyncAdminStatus {
        descriptor: admin.descriptor.clone().into(),
        admin_pubkey: admin.descriptor.drop_pub.clone(),
        data_dir: admin.data_dir,
        max_members: admin.max_members,
        max_total_storage_mb: admin.max_total_storage_mb,
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
        revocation: admin.revocation.clone(),
    }))
}

#[tauri::command]
pub async fn get_sync_admin_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    get_sync_admin_status_impl(password, &state).await
}

pub async fn update_sync_admin_config_impl(
    update: FrontendSyncAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let (mut admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        let admin = serde_json::from_slice::<StoredSyncAdmin>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e)))?;
        let master_seed = get_wallet_master_seed(&mut service)?;
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk)
    };

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

    if push_now && !admin.descriptor.write_url.is_empty() {
        let client = sync_client::create_sync_http_client();
        push_all_channel_members(
            &client,
            state,
            &mut admin,
            &admin_sk,
            password.as_deref(),
        )
        .await?;
    }

    {
        let mut service = state.service.lock().unwrap();
        let bytes = serde_json::to_vec(&admin).map_err(|e| {
            FrontendError::from(format!("Failed to serialize StoredSyncAdmin: {}", e))
        })?;
        service
            .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, &bytes, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    let (stats, active_slot_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_sync_members_with_stats(state, &mut service, &admin, "card", password.as_deref())?
    };

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(FrontendSyncAdminStatus {
        descriptor: admin.descriptor.clone().into(),
        admin_pubkey: admin.descriptor.drop_pub.clone(),
        data_dir: admin.data_dir,
        max_members: admin.max_members,
        max_total_storage_mb: admin.max_total_storage_mb,
        used_bytes: None,
        max_bytes: Some((admin.max_total_storage_mb as u64) * 1024 * 1024),
        soft_warning: false,
        card_policy: card_policy.into(),
        allow: admin.allow,
        deny: admin.deny,
        blocked_pubkeys: admin.blocked_pubkeys,
        direct_member_count: stats.direct_count,
        hop2_member_count: stats.hop2_count,
        total_member_count: stats.total_count,
        active_slot_ids,
        channel_counts: BTreeMap::new(),
        revocation: admin.revocation.clone(),
    })
}

#[tauri::command]
pub async fn update_sync_admin_config(
    update: FrontendSyncAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    update_sync_admin_config_impl(update, push_now, password, &state).await
}

pub async fn block_sync_slot_impl(
    pubkey_hex64: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    let update = FrontendSyncAdminUpdateRequest {
        blocked_pubkeys: Some({
            let mut service = state.service.lock().unwrap();
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
        }),
        ..Default::default()
    };

    update_sync_admin_config_impl(update, true, password, state).await
}

#[tauri::command]
pub async fn block_sync_slot(
    pubkey_hex64: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    block_sync_slot_impl(pubkey_hex64, password, &state).await
}

pub fn delete_sync_admin_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();

    let admin_opt: Option<StoredSyncAdmin> = service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    service
        .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, b"null", password.as_deref())
        .map_err(FrontendError::from)?;

    if let Some(admin) = admin_opt {
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let initial_len = settings.market_selected_syncs.len();
        settings
            .market_selected_syncs
            .retain(|d| d.read_url != admin.descriptor.read_url);
        if settings.market_selected_syncs.len() != initial_len {
            let selected_clone = settings.market_selected_syncs.clone();
            let _ = state.save_settings(&mut service, settings, password.as_deref());
            crate::commands::market::sync_own_card_drops_on_settings_change(
                &mut service,
                &selected_clone,
                password.as_deref(),
            );
        }
    }

    Ok(())
}

#[tauri::command]
pub fn delete_sync_admin(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    delete_sync_admin_impl(password, &state)
}

pub async fn revoke_own_sync_server_impl(
    effective_at: Option<u64>,
    reason: Option<String>,
    replacement_write_url: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    let (mut admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        let admin = serde_json::from_slice::<StoredSyncAdmin>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredSyncAdmin: {}", e)))?;
        let master_seed = get_wallet_master_seed(&mut service)?;
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.sync_index);
        (admin, admin_sk)
    };

    if admin.descriptor.write_url.trim().is_empty() {
        return Err(FrontendError::from("Sync server write_url is not configured".to_string()));
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
                if canonical_rep.trim_end_matches('/').eq_ignore_ascii_case(admin.descriptor.write_url.trim_end_matches('/')) {
                    return Err(FrontendError::from("Replacement URL must differ from the revoked server URL".to_string()));
                }
                let info = sync_client::fetch_sync_info(&client, &canonical_rep)
                    .await
                    .map_err(FrontendError::from)?;
                if info.read_url.trim_end_matches('/').eq_ignore_ascii_case(admin.descriptor.read_url.trim_end_matches('/')) {
                    return Err(FrontendError::from("Replacement read_url must differ from the revoked server read_url".to_string()));
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

        let notice = sync_client::sign_revocation_notice(
            &admin_sk,
            issued_at,
            eff,
            reason,
            replacement,
        )
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

        // Remove deprecated own server from outgoing SdCard selection so new contacts don't receive it
        let mut service = state.service.lock().unwrap();
        let bytes = serde_json::to_vec(&admin)
            .map_err(|e| FrontendError::from(format!("Failed to serialize StoredSyncAdmin: {}", e)))?;
        service
            .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, &bytes, password.as_deref())
            .map_err(FrontendError::from)?;

        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let old_read = admin.descriptor.read_url.trim().trim_end_matches('/').to_lowercase();
        let initial_len = settings.market_selected_syncs.len();
        settings.market_selected_syncs.retain(|d| {
            d.read_url.trim().trim_end_matches('/').to_lowercase() != old_read
        });
        if settings.market_selected_syncs.len() != initial_len {
            let selected_clone = settings.market_selected_syncs.clone();
            let _ = state.save_settings(&mut service, settings, password.as_deref());
            crate::commands::market::sync_own_card_drops_on_settings_change(
                &mut service,
                &selected_clone,
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
        let mut service = state.service.lock().unwrap();
        let bytes = serde_json::to_vec(&admin)
            .map_err(|e| FrontendError::from(format!("Failed to serialize StoredSyncAdmin: {}", e)))?;
        service
            .save_encrypted_data(MARKET_SYNC_ADMIN_KEY, &bytes, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    get_sync_admin_status_impl(password, state).await
}

#[tauri::command]
pub async fn revoke_own_sync_server(
    effective_at: Option<u64>,
    reason: Option<String>,
    replacement_write_url: Option<String>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    revoke_own_sync_server_impl(effective_at, reason, replacement_write_url, password, &state).await
}

pub async fn discover_available_syncs_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendDiscoveredSync>, FrontendError> {
    let mut descriptors: Vec<(FrontendSyncDescriptor, String)> = Vec::new();

    {
        let mut service = state.service.lock().unwrap();

        // 1. From own drop admin
        if let Ok(admin_bytes) =
            service.load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
        {
            if let Ok(admin) = serde_json::from_slice::<StoredSyncAdmin>(&admin_bytes) {
                if !admin.descriptor.read_url.is_empty() && !admin.descriptor.write_url.is_empty() {
                    let mut desc: FrontendSyncDescriptor = admin.descriptor.into();
                    desc.status = "active".to_string();
                    if let Some(ref rev) = admin.revocation {
                        crate::state::apply_revocation_notice_to_descriptor(
                            &mut desc,
                            rev,
                            sync_client::current_unix_timestamp(),
                        );
                    }
                    descriptors.push((desc, "Eigener Sync-Dienst".to_string()));
                }
            }
        }

        // 2. From contacts
        if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
            for c in contacts {
                let first = c.profile.first_name.as_deref().unwrap_or("").trim();
                let last = c.profile.last_name.as_deref().unwrap_or("").trim();
                let name = match (first.is_empty(), last.is_empty()) {
                    (false, false) => format!("{} {}", first, last),
                    (false, true) => first.to_string(),
                    (true, false) => last.to_string(),
                    (true, true) => c
                        .profile
                        .organization
                        .unwrap_or_else(|| c.did.clone()),
                };
                for d in c.syncs {
                    descriptors.push((d, name.clone()));
                }
            }
        }

        // 3. From card store
        if let Ok(store_bytes) =
            service.load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
        {
            if let Ok(store) = serde_json::from_slice::<humoco_market::CardStore>(&store_bytes) {
                for sc in store.cards.values() {
                    let profile = humoco_market::extract_public_profile(&sc.card);
                    let author = profile.first_name.as_deref().unwrap_or("Visitenkarte");
                    for d in &sc.card.payload.drops {
                        descriptors.push((d.clone().into(), author.to_string()));
                    }
                }
            }
        }
    }

    let settings = state
        .get_cached_settings()
        .or_else(|_| {
            let mut s = state.service.lock().unwrap();
            state.load_settings(&mut s, password.as_deref())
        })
        .unwrap_or_default();

    // 4. From market_selected_syncs
    for sel in &settings.market_selected_syncs {
        descriptors.push((sel.clone(), "Ausgewählter Sync-Dienst".to_string()));
    }

    let mut deduped: BTreeMap<String, (FrontendSyncDescriptor, String)> = BTreeMap::new();
    for (d, label) in descriptors {
        let key = if !d.write_url.is_empty() {
            d.write_url.trim().trim_end_matches('/').to_lowercase()
        } else {
            d.read_url.trim().trim_end_matches('/').to_lowercase()
        };
        if !key.is_empty() {
            match deduped.entry(key) {
                std::collections::btree_map::Entry::Vacant(v) => {
                    v.insert((d, label));
                }
                std::collections::btree_map::Entry::Occupied(mut o) => {
                    let (existing, _) = o.get_mut();
                    if d.status == "deprecating" || d.status == "revoked" {
                        existing.status = d.status;
                        existing.sunset_at = d.sunset_at;
                        existing.revocation_reason = d.revocation_reason;
                        existing.replacement = d.replacement;
                    } else if (existing.status.is_empty() || existing.status == "unverified")
                        && !d.status.is_empty()
                        && d.status != "unverified"
                    {
                        existing.status = d.status;
                    }
                }
            }
        }
    }

    let mut results = Vec::new();
    for (_, (mut desc, label)) in deduped {
        let norm_read = desc.read_url.trim().trim_end_matches('/').to_lowercase();
        let norm_write = desc.write_url.trim().trim_end_matches('/').to_lowercase();

        let is_selected = settings
            .market_selected_syncs
            .iter()
            .any(|d| {
                let sr = d.read_url.trim().trim_end_matches('/').to_lowercase();
                let sw = d.write_url.trim().trim_end_matches('/').to_lowercase();
                (!norm_read.is_empty() && (sr == norm_read || sw == norm_read))
                    || (!norm_write.is_empty() && (sr == norm_write || sw == norm_write))
            });

        let status = if desc.status.is_empty() {
            "unverified".to_string()
        } else {
            desc.status.clone()
        };
        let is_authorized = status == "active" || status == "deprecating";
        desc.status = status.clone();

        results.push(FrontendDiscoveredSync {
            descriptor: desc,
            is_authorized,
            is_selected,
            source_label: label,
            status,
        });
    }

    Ok(results)
}

#[tauri::command]
pub async fn discover_available_syncs(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendDiscoveredSync>, FrontendError> {
    discover_available_syncs_impl(password, &state).await
}

pub async fn probe_contact_syncs_impl(
    force: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    let (my_ed_sk, my_prefix) = {
        let service = state.service.lock().unwrap();
        service
            .with_wallet_and_identity(|_w, id| {
                let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string());
                (id.signing_key.clone(), pfx)
            })
            .map_err(FrontendError::from)?
    };

    let (mut settings, has_own_sync) = {
        let mut service = state.service.lock().unwrap();
        let s = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        let own = service
            .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<StoredSyncAdmin>(&bytes).ok())
            .map(|a| !a.descriptor.write_url.is_empty() && !a.descriptor.read_url.is_empty())
            .unwrap_or(false);
        (s, own)
    };

    let has_active_friend_sync = settings.market_selected_syncs.iter().any(|d| {
        d.status == "active" && d.fail_count == 0 && !d.read_url.is_empty() && !d.drop_pub.is_empty()
    });

    let has_healthy_public_sync = settings.public_syncs.iter().any(|pd| {
        pd.enabled && !pd.authorized_channels.is_empty() && !pd.read_url.is_empty()
    });

    let mut has_healthy_sync = has_own_sync || has_active_friend_sync || has_healthy_public_sync;
    let initial_has_healthy = has_healthy_sync;

    let mut address_book = {
        let mut service = state.service.lock().unwrap();
        match service.load_encrypted_data(CONTACTS_DATA_NAME, password.as_deref()) {
            Ok(data) => serde_json::from_slice::<crate::models::FrontendAddressBook>(&data).unwrap_or_default(),
            Err(_) => crate::models::FrontendAddressBook::default(),
        }
    };

    let now = chrono::Utc::now().timestamp() as u64;
    let client = sync_client::create_sync_probe_client();
    let mut contacts_modified = false;
    let mut settings_modified = false;
    let mut auto_activated = false;
    let mut probe_cache: std::collections::HashMap<
        String,
        (String, String, String, bool, Option<sync_client::SyncRevocationNotice>),
    > = std::collections::HashMap::new();
    let mut verified_revocations: Vec<(String, sync_client::SyncRevocationNotice)> = Vec::new();
    let mut channel_max_bytes_updates: Vec<(String, String, usize)> = Vec::new();

    for contact in address_book.contacts.values_mut() {
        for ep in &mut contact.syncs {
            if initial_has_healthy && !force {
                let needs_initial_probe = ep.last_probed_at.is_none()
                    && (ep.status == "unverified" || ep.read_url.is_empty() || ep.drop_pub.is_empty());
                if !needs_initial_probe {
                    continue;
                }
            } else if !force {
                if let Some(last_probed) = ep.last_probed_at {
                    let elapsed = now.saturating_sub(last_probed);
                    let skip = match ep.status.as_str() {
                        "unverified" => false,
                        "pending_auth" => {
                            let min_wait = if ep.fail_count > 3 { 300 } else { 60 };
                            elapsed < min_wait
                        }
                        "unreachable" => {
                            let exp = ep.fail_count.min(4);
                            let backoff = (60u64 * (1u64 << exp)).min(900);
                            elapsed < backoff
                        }
                        "revoked" => elapsed < 3600,
                        _ => elapsed < 60,
                    };
                    if skip {
                        continue;
                    }
                }
            }

            let target_url = if !ep.write_url.is_empty() {
                ep.write_url.clone()
            } else {
                ep.read_url.clone()
            };
            if target_url.is_empty() {
                continue;
            }

            let canonical_write = humoco_market::canonicalize_write_url(&target_url).unwrap_or(target_url);
            let sync_endpoint_key = canonical_write.trim().trim_end_matches('/').to_lowercase();

            if let Some((c_read, c_drop, c_status, c_ok, c_rev)) = probe_cache.get(&sync_endpoint_key) {
                if *c_ok {
                    ep.write_url = canonical_write.clone();
                    ep.read_url = c_read.clone();
                    ep.drop_pub = c_drop.clone();
                    ep.status = c_status.clone();
                    ep.fail_count = 0;
                    ep.last_seen_at = Some(now);
                    ep.last_probed_at = Some(now);
                } else {
                    ep.status = c_status.clone();
                    ep.fail_count = ep.fail_count.saturating_add(1);
                    ep.last_probed_at = Some(now);
                }
                if let Some(ref rev) = c_rev {
                    crate::state::apply_revocation_notice_to_descriptor(ep, rev, now);
                    verified_revocations.push((c_read.clone(), rev.clone()));
                } else {
                    crate::state::clear_revocation_on_descriptor(ep, c_status);
                }
                contacts_modified = true;
            } else {
                // 1. Fetch server info via ?info
                match sync_client::fetch_sync_info(&client, &canonical_write).await {
                    Err(_) => {
                        ep.status = "unreachable".to_string();
                        ep.fail_count = ep.fail_count.saturating_add(1);
                        ep.last_probed_at = Some(now);
                        contacts_modified = true;
                        probe_cache.insert(
                            sync_endpoint_key.clone(),
                            (ep.read_url.clone(), ep.drop_pub.clone(), "unreachable".to_string(), false, None),
                        );
                    }
                    Ok(info) => {
                        let verified_rev = info.revocation.as_ref().and_then(|n| {
                            sync_client::verify_revocation_notice(n, &info.drop_pub)
                                .ok()
                                .map(|_| n.clone())
                        });
                        let info_read_url = info.read_url.clone();
                        let info_drop_pub = info.drop_pub.clone();

                        ep.write_url = canonical_write.clone();
                        ep.read_url = info.read_url;
                        ep.drop_pub = info.drop_pub;
                        ep.last_seen_at = Some(now);
                        contacts_modified = true;

                        if verified_rev.is_none() {
                            crate::state::clear_revocation_on_descriptor(ep, "unverified");
                        }

                        // 2. Check authorization on resolved drop
                        let stealth_pk_res = if let Some(ref slot_id) = ep.slot_id {
                            if !slot_id.is_empty() {
                                humoco_market::parse_slot_id(slot_id)
                            } else {
                                humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &ep.drop_pub)
                                    .map(|pk| (pk, None))
                            }
                        } else {
                            humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &ep.drop_pub)
                                .map(|pk| (pk, None))
                        };

                        let mut probe_ok = false;
                        let mut probe_status = "unreachable".to_string();

                        match stealth_pk_res {
                            Err(_) => {
                                ep.status = "unreachable".to_string();
                                ep.fail_count = ep.fail_count.saturating_add(1);
                                ep.last_probed_at = Some(now);
                            }
                            Ok((stealth_pk, slot_pfx)) => {
                                let cap_prefix = slot_pfx.as_deref().or(my_prefix.as_deref());
                                let card_cap = sync_client::head_slot_capability(&client, &canonical_write, &stealth_pk, cap_prefix, "card").await;
                                if let Ok(ref cap) = card_cap {
                                    if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                                        channel_max_bytes_updates.push((canonical_write.clone(), "card".to_string(), mb));
                                    }
                                }
                                let post_cap = if card_cap.as_ref().map(|c| c.authorized).unwrap_or(false) {
                                    Ok(sync_client::SyncCapabilityResult { authorized: true, ..Default::default() })
                                } else {
                                    sync_client::head_slot_capability(&client, &canonical_write, &stealth_pk, cap_prefix, "post").await
                                };

                                let is_authorized = card_cap.as_ref().map(|c| c.authorized).unwrap_or(false)
                                    || post_cap.as_ref().map(|c| c.authorized).unwrap_or(false);

                                if is_authorized {
                                    probe_status = "active".to_string();
                                    probe_ok = true;
                                    ep.status = "active".to_string();
                                    ep.fail_count = 0;
                                    ep.last_probed_at = Some(now);
                                } else if card_cap.is_ok() || post_cap.is_ok() {
                                    probe_status = "pending_auth".to_string();
                                    probe_ok = true;
                                    ep.status = "pending_auth".to_string();
                                    ep.fail_count = 0;
                                    ep.last_probed_at = Some(now);
                                } else {
                                    ep.status = "unreachable".to_string();
                                    ep.fail_count = ep.fail_count.saturating_add(1);
                                    ep.last_probed_at = Some(now);
                                }
                            }
                        }

                        if let Some(ref notice) = verified_rev {
                            crate::state::apply_revocation_notice_to_descriptor(ep, notice, now);
                            verified_revocations.push((info_read_url.clone(), notice.clone()));
                        }

                        probe_cache.insert(
                            sync_endpoint_key.clone(),
                            (info_read_url, info_drop_pub, probe_status, probe_ok, verified_rev),
                        );
                    }
                }
            }

            // Update matching entries in settings.market_selected_syncs
            let norm_ep_read = ep.read_url.trim().trim_end_matches('/').to_lowercase();
            let norm_ep_write = ep.write_url.trim().trim_end_matches('/').to_lowercase();
            for sel in &mut settings.market_selected_syncs {
                let norm_sel_read = sel.read_url.trim().trim_end_matches('/').to_lowercase();
                let norm_sel_write = sel.write_url.trim().trim_end_matches('/').to_lowercase();
                let match_read = !norm_ep_read.is_empty() && (norm_sel_read == norm_ep_read || norm_sel_write == norm_ep_read);
                let match_write = !norm_ep_write.is_empty() && (norm_sel_read == norm_ep_write || norm_sel_write == norm_ep_write);
                if match_read || match_write {
                    sel.write_url = ep.write_url.clone();
                    sel.read_url = ep.read_url.clone();
                    sel.drop_pub = ep.drop_pub.clone();
                    sel.status = ep.status.clone();
                    sel.fail_count = ep.fail_count;
                    sel.last_probed_at = ep.last_probed_at;
                    sel.sunset_at = ep.sunset_at;
                    sel.revocation_reason = ep.revocation_reason.clone();
                    sel.replacement = ep.replacement.clone();
                    settings_modified = true;
                }
            }

            // Auto-activation when !has_healthy_sync
            if !has_healthy_sync
                && ep.status == "active"
                && !ep.read_url.is_empty()
                && !ep.drop_pub.is_empty()
            {
                let already_selected = settings.market_selected_syncs.iter().any(|s| {
                    let sr = s.read_url.trim().trim_end_matches('/').to_lowercase();
                    let sw = s.write_url.trim().trim_end_matches('/').to_lowercase();
                    (!norm_ep_read.is_empty() && (sr == norm_ep_read || sw == norm_ep_read))
                        || (!norm_ep_write.is_empty() && (sr == norm_ep_write || sw == norm_ep_write))
                });
                if !already_selected && settings.market_selected_syncs.len() < 3 {
                    settings.market_selected_syncs.push(FrontendSyncDescriptor {
                        write_url: ep.write_url.clone(),
                        read_url: ep.read_url.clone(),
                        slot_id: ep.slot_id.clone(),
                        drop_pub: ep.drop_pub.clone(),
                        last_seen_at: ep.last_seen_at,
                        last_error_at: None,
                        fail_count: 0,
                        status: "active".to_string(),
                        last_probed_at: Some(now),
                        ..Default::default()
                    });
                    has_healthy_sync = true;
                    settings_modified = true;
                    auto_activated = true;
                }
            }
        }
    }

    for (rev_read_url, notice) in verified_revocations {
        if crate::state::apply_revocation_notice_to_address_book(
            &mut address_book,
            &rev_read_url,
            &notice,
            now,
        ) {
            contacts_modified = true;
        }
    }

    if contacts_modified {
        let mut service = state.service.lock().unwrap();
        if let Ok(bytes) = serde_json::to_vec(&address_book) {
            let _ = service.save_encrypted_data(CONTACTS_DATA_NAME, &bytes, password.as_deref());
        }
        *state.contacts.lock().unwrap() = Some(address_book.clone());
    }

    if settings_modified {
        let mut service = state.service.lock().unwrap();
        let _ = state.save_settings(&mut service, settings.clone(), password.as_deref());
        if auto_activated {
            crate::commands::market::sync_own_card_drops_on_settings_change(
                &mut service,
                &settings.market_selected_syncs,
                password.as_deref(),
            );
        }
    }

    save_channel_max_bytes_updates(state, password.as_deref(), &channel_max_bytes_updates);

    let mut all_syncs = Vec::new();
    for c in address_book.contacts.values() {
        all_syncs.extend(c.syncs.clone());
    }
    Ok(all_syncs)
}

#[tauri::command]
pub async fn probe_contact_syncs(
    force: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    probe_contact_syncs_impl(force.unwrap_or(false), password, &state).await
}

pub async fn set_selected_upload_syncs_impl(
    syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    if syncs.len() > 3 {
        return Err(FrontendError::from("Maximum 3 upload drops allowed".to_string()));
    }

    for d in &syncs {
        sync_client::validate_sync_url(&d.write_url).map_err(FrontendError::from)?;
        sync_client::validate_sync_url(&d.read_url).map_err(FrontendError::from)?;
        humoco_market::parse_ed25519_pubkey_hex64(&d.drop_pub)
            .map_err(|e| FrontendError::from(format!("Invalid drop_pub: {}", e)))?;
        if let Some(ref sid) = d.slot_id {
            humoco_market::parse_slot_id(sid)
                .map_err(|e| FrontendError::from(format!("Invalid slot_id: {}", e)))?;
        }
    }

    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        settings.market_selected_syncs = syncs.clone();
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;

        crate::commands::market::sync_own_card_drops_on_settings_change(
            &mut service,
            &syncs,
            password.as_deref(),
        );
    }

    let _ = crate::commands::recovery::enqueue_guardian_locator_updates_if_configured(state, password.clone()).await;

    // Trigger upload to selected drops
    let _ = sync_market_channels_impl(None, Some(true), password, state).await;

    Ok(syncs)
}

#[tauri::command]
pub async fn set_selected_upload_syncs(
    syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    set_selected_upload_syncs_impl(syncs, password, &state).await
}

const DELTA_MAX_BYTES: usize = 32 * 1024;

fn norm_write_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn channel_max_bytes_key(write_url: &str, channel: &str) -> String {
    format!("{}:{}", norm_write_url(write_url), channel)
}

fn save_channel_max_bytes_updates(
    state: &AppState,
    password: Option<&str>,
    updates: &[(String, String, usize)],
) {
    if updates.is_empty() {
        return;
    }
    let mut service = state.service.lock().unwrap();
    let mut sync_state: StoredSyncState = service
        .load_encrypted_data(MARKET_SYNC_KEY, password)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut changed = false;
    for (write_url, channel, mb) in updates {
        if *mb > 0 {
            let key = channel_max_bytes_key(write_url, channel);
            if sync_state.channel_max_bytes_by_write_url.get(&key) != Some(mb) {
                sync_state.channel_max_bytes_by_write_url.insert(key, *mb);
                changed = true;
            }
        }
    }
    if changed {
        if let Ok(bytes) = serde_json::to_vec(&sync_state) {
            let _ = service.save_encrypted_data(MARKET_SYNC_KEY, &bytes, password);
        }
    }
}

fn derive_drop_slot_pubkey(
    master_seed: &[u8; 32],
    my_ed_sk: &ed25519_dalek::SigningKey,
    drop_desc: &humoco_market::DropDescriptor,
) -> Result<String, String> {
    if drop_desc.slot_id.is_some() {
        humoco_market::derive_public_drop_keypair(master_seed, &drop_desc.write_url)
            .map(|(_, _, pk)| pk)
            .map_err(|e| e.to_string())
    } else {
        humoco_market::derive_member_stealth_pubkey(my_ed_sk, &drop_desc.drop_pub)
            .map_err(|e| e.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
async fn ensure_cached_channel_max_bytes(
    client: &reqwest::Client,
    sync_state: &mut StoredSyncState,
    master_seed: &[u8; 32],
    my_ed_sk: &ed25519_dalek::SigningKey,
    drop_desc: &humoco_market::DropDescriptor,
    prefix: Option<&str>,
    channel: &str,
    default_max: usize,
) -> usize {
    let key = channel_max_bytes_key(&drop_desc.write_url, channel);
    if let Some(&cached) = sync_state.channel_max_bytes_by_write_url.get(&key) {
        return cached;
    }
    if let Ok(slot_pk) = derive_drop_slot_pubkey(master_seed, my_ed_sk, drop_desc) {
        if let Ok(cap) = sync_client::head_slot_capability(
            client,
            &drop_desc.write_url,
            &slot_pk,
            prefix,
            channel,
        )
        .await
        {
            if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                sync_state.channel_max_bytes_by_write_url.insert(key, mb);
                return mb;
            }
        }
    }
    default_max
}

#[allow(clippy::too_many_arguments)]
fn sign_drop_channel_request(
    master_seed: &[u8; 32],
    my_ed_sk: &ed25519_dalek::SigningKey,
    drop_desc: &humoco_market::DropDescriptor,
    method: &str,
    channel: &str,
    prefix: Option<&str>,
    now_secs: u64,
    payload: &[u8],
) -> Result<humoco_market::SignedDropHeaders, String> {
    let nonce = sync_client::generate_sync_nonce();
    if drop_desc.slot_id.is_some() {
        let (r_sk, _, _) = humoco_market::derive_public_drop_keypair(master_seed, &drop_desc.write_url)
            .map_err(|e| format!("Public drop key derivation error: {}", e))?;
        Ok(humoco_market::sign_member_request_direct(
            &r_sk, method, channel, prefix, now_secs, &nonce, payload,
        ))
    } else {
        humoco_market::sign_member_request_stealth(
            my_ed_sk,
            &drop_desc.drop_pub,
            method,
            channel,
            prefix,
            now_secs,
            &nonce,
            payload,
        )
        .map_err(|e| format!("Stealth signing error: {}", e))
    }
}

async fn delete_delta_card_slot(
    client: &reqwest::Client,
    master_seed: &[u8; 32],
    my_ed_sk: &ed25519_dalek::SigningKey,
    drop_desc: &humoco_market::DropDescriptor,
    delta_prefix: &str,
    now_secs: u64,
) -> bool {
    let Ok(del_signed) = sign_drop_channel_request(
        master_seed,
        my_ed_sk,
        drop_desc,
        "DELETE",
        "card",
        Some(delta_prefix),
        now_secs,
        b"",
    ) else {
        return false;
    };
    sync_client::delete_slot(client, &drop_desc.write_url, &del_signed, "card")
        .await
        .is_ok()
}

#[allow(clippy::too_many_arguments)]
fn import_decrypted_card_bundle(
    bundle: humoco_market::CardBundle,
    read_url: &str,
    now_secs: u64,
    max_stored_cards: usize,
    state: &AppState,
    store: &mut humoco_market::CardStore,
    sync_state: &mut StoredSyncState,
    password: Option<&str>,
) -> usize {
    let mut count = 0;
    for env in bundle.envelopes {
        let mut service = state.service.lock().unwrap();
        let outcome = service.with_wallet(|wallet| {
            let is_offender = |did: &str| -> bool {
                use human_money_core::models::conflict::TrustStatus;
                matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
            };
            let own_coords = wallet
                .profile
                .coordinates
                .as_deref()
                .and_then(humoco_market::parse_coordinates_pair);
            humoco_market::accept_envelope(
                store,
                &env,
                now_secs,
                max_stored_cards,
                own_coords,
                &is_offender,
            )
        });

        if let Ok(humoco_market::AcceptOutcome::Accepted { did, .. }) = outcome {
            count += 1;
            let can_did = canonicalize_did(&did).unwrap_or_else(|_| did.clone());
            sync_state
                .sync_sources_by_did
                .insert(can_did, read_url.to_string());

            if let Some(stored) = store.cards.values().find(|sc| sc.card.payload.did == did) {
                let card_syncs: Vec<FrontendSyncDescriptor> = stored
                    .card
                    .payload
                    .drops
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect();
                state.update_contact_syncs_from_card(
                    &mut service,
                    &did,
                    &card_syncs,
                    password,
                );
            }
        }
    }
    count
}

fn import_decrypted_wot_package(
    pkg: &humoco_wot::WotSyncPackage,
    read_url: &str,
    now_secs: u64,
    state: &AppState,
    password: Option<&str>,
    errors: &mut Vec<String>,
) -> usize {
    let mut service = state.service.lock().unwrap();
    if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password) {
        match reg.import_sync_package(pkg, None, now_secs) {
            Ok(stats) => {
                let accepted = stats.accepted_total() + stats.merged;
                if accepted > 0 || reg.own_list_dirty {
                    if reg.own_list_dirty {
                        let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
                    }
                    let _ = state.save_wot_register(&mut service, &reg, password);
                }
                return accepted;
            }
            Err(e) => {
                errors.push(format!("Failed to import wot package from {}: {}", read_url, e));
            }
        }
    }
    0
}

pub async fn sync_market_channels_impl(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketSyncResult, FrontendError> {
    sync_market_channels_with_options(is_wifi, force_upload, None, password, state).await
}

pub async fn sync_market_channels_with_options(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    respect_cooldown: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendMarketSyncResult, FrontendError> {
    let mut settings = {
        let mut service = state.service.lock().unwrap();
        state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default()
    };

    if settings.market_poll_wifi_only && is_wifi == Some(false) {
        return Ok(FrontendMarketSyncResult {
            skipped_wifi_only: true,
            uploaded_to: 0,
            polled_syncs: 0,
            decrypted_slots: 0,
            imported_cards: 0,
            imported_wot_lists: 0,
            errors: Vec::new(),
            retry_after_secs: None,
            revoked_own_syncs: Vec::new(),
        });
    }

    let client = sync_client::create_sync_http_client();
    let mut errors = Vec::new();
    let mut uploaded_to = 0;
    let mut polled_syncs = 0;
    let mut decrypted_slots = 0;
    let mut imported_cards = 0;
    let mut imported_wot_lists = 0;

    let now_secs = sync_client::current_unix_timestamp();

    // 1. Ensure own DropAdmin members are up-to-date on server
    {
        let admin_opt: Option<(StoredSyncAdmin, ed25519_dalek::SigningKey)> = {
            let mut service = state.service.lock().unwrap();
            let admin_opt = service
                .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<StoredSyncAdmin>(&bytes).ok());
            if let Some(admin) = admin_opt {
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
            let _ = push_all_channel_members(
                &client,
                state,
                &mut admin,
                &admin_sk,
                password.as_deref(),
            )
            .await;
        }
    }

    // 2. Upload check
    let (my_ed_sk, master_seed, my_prefix, own_card_opt, own_profile_opt, readers, readers_hash) = {
        let mut service = state.service.lock().unwrap();
        let (my_ed_sk, my_prefix) = service
            .with_wallet_and_identity(|_w, id| {
                let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string());
                (id.signing_key.clone(), pfx)
            })
            .map_err(FrontendError::from)?;
        let master_seed = my_ed_sk.to_bytes();

        let own_profile: Option<crate::models::StoredOwnMarketProfile> = service
            .load_encrypted_data(crate::state::MARKET_OWN_PROFILE_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let own_card = own_profile.as_ref().and_then(|p| if p.published { p.last_card.clone() } else { None });

        let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();

        let mut reader_keys = Vec::new();
        if let Some(r) = reg {
            for (did, entry) in &r.own_contacts {
                if entry.level >= settings.share_min_level {
                    if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
                        reader_keys.push(pk);
                    }
                }
            }
        }
        reader_keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        reader_keys.dedup();

        let mut hasher = sha2::Sha256::new();
        for k in &reader_keys {
            hasher.update(k.as_bytes());
        }
        let readers_hash = hex::encode(hasher.finalize());

        (my_ed_sk, master_seed, my_prefix, own_card, own_profile, reader_keys, readers_hash)
    };

    let mut sync_state: StoredSyncState = {
        let mut service = state.service.lock().unwrap();
        service
            .load_encrypted_data(MARKET_SYNC_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    };

    // Gather upload targets: selected drops + enabled public drops
    let mut upload_targets: Vec<humoco_market::DropDescriptor> = settings
        .market_selected_syncs
        .iter()
        .cloned()
        .map(Into::into)
        .collect();

    for pd in &settings.public_syncs {
        if pd.enabled
            && !pd.write_url.is_empty()
            && (pd.authorized_channels.is_empty() || pd.authorized_channels.iter().any(|c| c == "card"))
        {
            upload_targets.push(humoco_market::DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(pd.access_pubkey.clone()),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }

    if settings.market_sync_enabled && !upload_targets.is_empty() && !readers.is_empty() {
        let active_own_card = own_card_opt
            .as_ref()
            .filter(|c| now_secs < c.payload.valid_until);
        let own_wire_info: Option<(String, String, u64)> = active_own_card.and_then(|own_card| {
            let can_did =
                canonicalize_did(&own_card.payload.did).unwrap_or_else(|_| own_card.payload.did.clone());
            humoco_market::render_wire(own_card, 1, humoco_market::WireFilter::Full)
                .map(|wire| (can_did, wire, own_card.payload.v))
        });

        let (store, own_coords, trust_by_did, offenders, active_media_blobs) = {
            let mut service = state.service.lock().unwrap();
            let store: humoco_market::CardStore = service
                .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let reg = state
                .get_or_load_wot_register(&mut service, password.as_deref())
                .ok();
            let own_coords = service
                .with_wallet(|wallet| {
                    wallet
                        .profile
                        .coordinates
                        .as_deref()
                        .and_then(humoco_market::parse_coordinates_pair)
                })
                .ok()
                .flatten();

            let mut trust_by_did = std::collections::HashMap::new();
            let mut offenders = HashSet::new();
            if let Some(ref r) = reg {
                for (did, entry) in &r.own_contacts {
                    let can = canonicalize_did(did).unwrap_or_else(|_| did.clone());
                    trust_by_did.insert(can, entry.level);
                }
            }
            for sc in store.cards.values() {
                let can = canonicalize_did(&sc.card.payload.did)
                    .unwrap_or_else(|_| sc.card.payload.did.clone());
                let is_off = service
                    .with_wallet(|wallet| {
                        use human_money_core::models::conflict::TrustStatus;
                        matches!(
                            wallet.check_reputation(&sc.card.payload.did),
                            TrustStatus::KnownOffender(_)
                        )
                    })
                    .unwrap_or(false);
                if is_off {
                    offenders.insert(can);
                }
            }

            let media_blobs: Vec<(String, Vec<u8>)> =
                if let (Some(own_card), Some(prof)) = (active_own_card, own_profile_opt.as_ref()) {
                    let mut blobs = Vec::new();
                    for ad in &prof.ads {
                        if ad.expires_at > now_secs {
                            if let Some(ref token) = ad.image_token {
                                let storage_key = crate::state::own_media_storage_key(token);
                                if let Ok(webp_bytes) =
                                    service.load_encrypted_data(&storage_key, password.as_deref())
                                {
                                    if let Ok((_tok, blob_id, ciphertext)) =
                                        humoco_market::seal_media_blob(
                                            &own_card.payload.did,
                                            &webp_bytes,
                                        )
                                    {
                                        blobs.push((blob_id, ciphertext));
                                    }
                                }
                            }
                        }
                    }
                    blobs
                } else {
                    Vec::new()
                };

            (store, own_coords, trust_by_did, offenders, media_blobs)
        };

        let trust_level_of = |did: &str| -> u8 {
            let can = canonicalize_did(did).unwrap_or_else(|_| did.to_string());
            trust_by_did.get(&can).copied().unwrap_or(0)
        };

        let own_can_did_opt = own_wire_info.as_ref().map(|(d, _, _)| d.clone());
        let mut wire_meta: std::collections::HashMap<String, (String, u64)> =
            std::collections::HashMap::new();
        let mut card_by_did: std::collections::HashMap<String, &humoco_market::StoredCard> =
            std::collections::HashMap::new();

        if let Some((ref own_did, ref own_wire, own_v)) = own_wire_info {
            wire_meta.insert(own_wire.clone(), (own_did.clone(), own_v));
        }

        let mut forward_candidates: Vec<&humoco_market::StoredCard> = Vec::new();
        for sc in store.cards.values() {
            if sc.card.payload.valid_until <= now_secs {
                continue;
            }
            let can = canonicalize_did(&sc.card.payload.did)
                .unwrap_or_else(|_| sc.card.payload.did.clone());
            if own_can_did_opt.as_ref() == Some(&can) || offenders.contains(&can) {
                continue;
            }
            if let Some(env) = humoco_market::forward_envelope(sc) {
                wire_meta.insert(env.card_wire, (can.clone(), sc.card.payload.v));
                card_by_did.insert(can, sc);
                forward_candidates.push(sc);
            }
        }

        if own_wire_info.is_some() || !forward_candidates.is_empty() {
            let readers_changed = sync_state.last_readers_hash.as_deref() != Some(&readers_hash);
            let heartbeat_due =
                now_secs.saturating_sub(sync_state.last_upload_at.unwrap_or(0)) >= SYNC_HEARTBEAT_SECS;
            let delta_prefix = humoco_market::derive_delta_prefix(my_prefix.as_deref());
            let mut any_card_uploaded = false;
            let mut any_card_failed = false;

            for drop_desc in &mut upload_targets {
                let target_key = norm_write_url(&drop_desc.write_url);
                if target_key.is_empty() {
                    continue;
                }

                let effective_card_max_bytes = ensure_cached_channel_max_bytes(
                    &client,
                    &mut sync_state,
                    &master_seed,
                    &my_ed_sk,
                    drop_desc,
                    my_prefix.as_deref(),
                    "card",
                    SYNC_CARD_MAX_BYTES,
                )
                .await;

                let full_bundle = humoco_market::build_card_bundle(
                    own_wire_info.as_ref().map(|(_, w, _)| w.as_str()),
                    &forward_candidates,
                    own_coords,
                    now_secs,
                    &trust_level_of,
                    readers.len(),
                    effective_card_max_bytes,
                );

                if !full_bundle.envelopes.is_empty() {
                    let mut desired_versions: BTreeMap<String, u64> = BTreeMap::new();
                    for env in &full_bundle.envelopes {
                        if let Some((did, v)) = wire_meta.get(&env.card_wire) {
                            desired_versions.insert(did.clone(), *v);
                        }
                    }

                    let prev_base_at = sync_state
                        .card_base_uploaded_at_by_write_url
                        .get(&target_key)
                        .copied();
                    let prev_base_size = sync_state
                        .card_base_size_by_write_url
                        .get(&target_key)
                        .copied();
                    let prev_base_versions = sync_state
                        .card_base_versions_by_write_url
                        .get(&target_key)
                        .cloned();
                    let prev_last_versions = sync_state
                        .card_last_uploaded_versions_by_write_url
                        .get(&target_key)
                        .cloned();
                    let delta_active = sync_state
                        .card_delta_active_by_write_url
                        .get(&target_key)
                        .copied()
                        .unwrap_or(false);
                    let base_fresh = prev_base_at
                        .is_some_and(|ts| now_secs.saturating_sub(ts) < SYNC_BASE_MAX_AGE_SECS);

                    let skip_card_put = force_upload != Some(true)
                        && !readers_changed
                        && base_fresh
                        && !heartbeat_due
                        && prev_last_versions.as_ref() == Some(&desired_versions);

                    if skip_card_put
                        && delta_active
                        && prev_base_versions.as_ref() == Some(&desired_versions)
                        && delete_delta_card_slot(
                            &client,
                            &master_seed,
                            &my_ed_sk,
                            drop_desc,
                            &delta_prefix,
                            now_secs,
                        )
                        .await
                    {
                        sync_state
                            .card_delta_active_by_write_url
                            .insert(target_key.clone(), false);
                    }

                    if !skip_card_put {
                        let maybe_delta_blob: Option<Vec<u8>> = if !readers_changed && base_fresh {
                            if let Some(ref base_vers) = prev_base_versions {
                                let any_removed =
                                    base_vers.keys().any(|did| !desired_versions.contains_key(did));
                                let diff_dids: HashSet<String> = desired_versions
                                    .iter()
                                    .filter(|(did, v)| base_vers.get(*did) != Some(*v))
                                    .map(|(did, _)| did.clone())
                                    .collect();
                                if !any_removed && !diff_dids.is_empty() {
                                    let delta_own_wire = own_wire_info
                                        .as_ref()
                                        .filter(|(did, _, _)| diff_dids.contains(did))
                                        .map(|(_, w, _)| w.as_str());
                                    let delta_cards: Vec<&humoco_market::StoredCard> = diff_dids
                                        .iter()
                                        .filter_map(|did| card_by_did.get(did).copied())
                                        .collect();
                                    let delta_bundle = humoco_market::build_card_bundle(
                                        delta_own_wire,
                                        &delta_cards,
                                        own_coords,
                                        now_secs,
                                        &trust_level_of,
                                        readers.len(),
                                        effective_card_max_bytes,
                                    );
                                    let base_sz = prev_base_size.unwrap_or(0);
                                    humoco_market::seal_slot_blob(
                                        &delta_bundle,
                                        &readers,
                                        effective_card_max_bytes,
                                    )
                                    .ok()
                                    .filter(|blob| {
                                        blob.len() <= DELTA_MAX_BYTES
                                            && blob.len().saturating_mul(4) <= base_sz
                                            && base_sz.saturating_add(blob.len())
                                                <= effective_card_max_bytes
                                    })
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(delta_blob) = maybe_delta_blob {
                            match sign_drop_channel_request(
                                &master_seed,
                                &my_ed_sk,
                                drop_desc,
                                "PUT",
                                "card",
                                Some(&delta_prefix),
                                now_secs,
                                &delta_blob,
                            ) {
                                Ok(signed) => {
                                    match sync_client::put_slot(
                                        &client,
                                        &drop_desc.write_url,
                                        &signed,
                                        "card",
                                        &delta_blob,
                                        None,
                                    )
                                    .await
                                    {
                                        Ok(PutSlotOutcome::Saved { .. }) => {
                                            uploaded_to += 1;
                                            any_card_uploaded = true;
                                            sync_state
                                                .card_delta_active_by_write_url
                                                .insert(target_key.clone(), true);
                                            sync_state
                                                .card_last_uploaded_versions_by_write_url
                                                .insert(target_key.clone(), desired_versions);
                                        }
                                        Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                            any_card_failed = true;
                                            errors.push(format!(
                                                "Upload delta to {}: Precondition failed (ETag)",
                                                drop_desc.write_url
                                            ));
                                        }
                                        Err(e) => {
                                            any_card_failed = true;
                                            errors.push(format!(
                                                "Upload delta to {}: {}",
                                                drop_desc.write_url, e
                                            ));
                                        }
                                    }
                                }
                                Err(e) => {
                                    any_card_failed = true;
                                    errors.push(format!(
                                        "Signing delta error for {}: {}",
                                        drop_desc.write_url, e
                                    ));
                                }
                            }
                        } else {
                            match humoco_market::seal_slot_blob(
                                &full_bundle,
                                &readers,
                                effective_card_max_bytes,
                            ) {
                                Ok(base_blob) => {
                                    let base_len = base_blob.len();
                                    match sign_drop_channel_request(
                                        &master_seed,
                                        &my_ed_sk,
                                        drop_desc,
                                        "PUT",
                                        "card",
                                        my_prefix.as_deref(),
                                        now_secs,
                                        &base_blob,
                                    ) {
                                        Ok(signed) => {
                                            match sync_client::put_slot(
                                                &client,
                                                &drop_desc.write_url,
                                                &signed,
                                                "card",
                                                &base_blob,
                                                None,
                                            )
                                            .await
                                            {
                                                Ok(PutSlotOutcome::Saved { .. }) => {
                                                    uploaded_to += 1;
                                                    any_card_uploaded = true;
                                                    sync_state
                                                        .card_base_uploaded_at_by_write_url
                                                        .insert(target_key.clone(), now_secs);
                                                    sync_state
                                                        .card_base_size_by_write_url
                                                        .insert(target_key.clone(), base_len);
                                                    sync_state
                                                        .card_base_versions_by_write_url
                                                        .insert(
                                                            target_key.clone(),
                                                            desired_versions.clone(),
                                                        );
                                                    sync_state
                                                        .card_last_uploaded_versions_by_write_url
                                                        .insert(target_key.clone(), desired_versions);
                                                    let delta_cleared = if delta_active {
                                                        delete_delta_card_slot(
                                                            &client,
                                                            &master_seed,
                                                            &my_ed_sk,
                                                            drop_desc,
                                                            &delta_prefix,
                                                            now_secs,
                                                        )
                                                        .await
                                                    } else {
                                                        true
                                                    };
                                                    sync_state
                                                        .card_delta_active_by_write_url
                                                        .insert(target_key.clone(), !delta_cleared);
                                                }
                                                Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                                    any_card_failed = true;
                                                    errors.push(format!(
                                                        "Upload to {}: Precondition failed (ETag)",
                                                        drop_desc.write_url
                                                    ));
                                                }
                                                Err(e) => {
                                                    any_card_failed = true;
                                                    errors.push(format!(
                                                        "Upload to {}: {}",
                                                        drop_desc.write_url, e
                                                    ));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            any_card_failed = true;
                                            errors.push(format!(
                                                "Signing error for {}: {}",
                                                drop_desc.write_url, e
                                            ));
                                        }
                                    }
                                }
                                Err(e) => {
                                    any_card_failed = true;
                                    errors.push(format!("Failed to seal slot blob: {}", e));
                                }
                            }
                        }
                    }
                }

                // Upload active media blobs to this drop target
                for (blob_id, ciphertext) in &active_media_blobs {
                    let upload_key = format!("{}::{}", drop_desc.write_url, blob_id);
                    let force = force_upload == Some(true);
                    if !force && sync_state.uploaded_media_blobs.contains(&upload_key) {
                        continue;
                    }

                    let media_ch = format!("media:{}", blob_id);
                    match sign_drop_channel_request(
                        &master_seed,
                        &my_ed_sk,
                        drop_desc,
                        "PUT",
                        &media_ch,
                        None,
                        now_secs,
                        ciphertext,
                    ) {
                        Ok(signed) => {
                            match sync_client::put_media_blob(
                                &client,
                                &drop_desc.write_url,
                                &signed,
                                blob_id,
                                ciphertext,
                            )
                            .await
                            {
                                Ok(PutSlotOutcome::Saved { .. }) => {
                                    sync_state.uploaded_media_blobs.insert(upload_key);
                                }
                                Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                    log::warn!(
                                        "Media upload to {} precondition failed",
                                        drop_desc.write_url
                                    );
                                }
                                Err(e) => {
                                    log::warn!(
                                        "Media upload to {} non-fatal error: {}",
                                        drop_desc.write_url,
                                        e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!(
                                "Media request signing for {} non-fatal error: {}",
                                drop_desc.write_url,
                                e
                            );
                        }
                    }
                }
            }

            if any_card_uploaded {
                sync_state.last_upload_at = Some(now_secs);
                if !any_card_failed {
                    sync_state.last_readers_hash = Some(readers_hash);
                }
            }
        }
    } else if settings.market_sync_enabled
        && !upload_targets.is_empty()
        && sync_state.last_readers_hash.is_some()
    {
        let delta_prefix = humoco_market::derive_delta_prefix(my_prefix.as_deref());
        for drop_desc in &upload_targets {
            if drop_desc.write_url.is_empty() {
                continue;
            }
            for pfx in [my_prefix.as_deref(), Some(delta_prefix.as_str())] {
                if let Ok(signed) = sign_drop_channel_request(
                    &master_seed,
                    &my_ed_sk,
                    drop_desc,
                    "DELETE",
                    "card",
                    pfx,
                    now_secs,
                    b"",
                ) {
                    let _ =
                        sync_client::delete_slot(&client, &drop_desc.write_url, &signed, "card")
                            .await;
                }
            }
        }
        sync_state.last_readers_hash = None;
        sync_state.card_base_uploaded_at_by_write_url.clear();
        sync_state.card_base_size_by_write_url.clear();
        sync_state.card_base_versions_by_write_url.clear();
        sync_state.card_delta_active_by_write_url.clear();
        sync_state.card_last_uploaded_versions_by_write_url.clear();
    }

    // 2b. Upload Channel "wot"
    if settings.wot_share_via_sync {
        let (reg_opt, is_offender_set) = {
            let mut service = state.service.lock().unwrap();
            let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();
            let offenders: HashSet<String> = if let Some(ref r) = reg {
                r.own_contacts
                    .keys()
                    .filter(|did| {
                        use human_money_core::models::conflict::TrustStatus;
                        service
                            .with_wallet(|w| matches!(w.check_reputation(did), TrustStatus::KnownOffender(_)))
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect()
            } else {
                HashSet::new()
            };
            (reg, offenders)
        };

        if let Some(mut reg) = reg_opt {
            let needs_signing = reg.own_list_dirty || (reg.own_signed_list.is_none() && !reg.own_contacts.is_empty());
            if needs_signing {
                let mut service = state.service.lock().unwrap();
                let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
                let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
            }

            let auth_readers = reg.authorized_drop_readers();
            let filtered_readers: Vec<(humoco_wot::Did, u8)> = auth_readers
                .into_iter()
                .filter(|(did, _)| !is_offender_set.contains(did))
                .collect();

            if !filtered_readers.is_empty() {
                let readers_refs: Vec<(&str, u8)> = filtered_readers
                    .iter()
                    .map(|(did, class)| (did.as_str(), *class))
                    .collect();

                let mut wot_upload_targets: Vec<humoco_market::DropDescriptor> = settings
                    .market_selected_syncs
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect();

                for pd in &settings.public_syncs {
                    if pd.enabled
                        && !pd.write_url.is_empty()
                        && (pd.authorized_channels.is_empty() || pd.authorized_channels.iter().any(|c| c == "wot"))
                    {
                        wot_upload_targets.push(humoco_market::DropDescriptor {
                            write_url: pd.write_url.clone(),
                            read_url: pd.read_url.clone(),
                            slot_id: Some(pd.access_pubkey.clone()),
                            drop_pub: pd.drop_pub.clone(),
                        });
                    }
                }

                let active_classes = (1u8..=3)
                    .filter(|&d| filtered_readers.iter().any(|(_, cls)| *cls == d))
                    .count()
                    .max(1);
                let mut total_overhead: usize = 32;
                for d in 1u8..=3 {
                    let cnt = filtered_readers.iter().filter(|(_, cls)| *cls == d).count();
                    if cnt > 0 {
                        total_overhead += humoco_market::slot_overhead_bytes(cnt).unwrap_or(8_250);
                    }
                }

                let mut reg_modified = false;

                for drop_desc in &mut wot_upload_targets {
                    if drop_desc.write_url.is_empty() {
                        continue;
                    }

                    let effective_wot_max_bytes = ensure_cached_channel_max_bytes(
                        &client,
                        &mut sync_state,
                        &master_seed,
                        &my_ed_sk,
                        drop_desc,
                        None,
                        "wot",
                        SYNC_WOT_MAX_BYTES,
                    )
                    .await;
                    let initial_class_budget = effective_wot_max_bytes
                        .saturating_sub(total_overhead)
                        / active_classes;

                    for attempt in 0..2 {
                        let class_budget = if attempt == 0 {
                            initial_class_budget
                        } else {
                            (initial_class_budget / 2).max(1024)
                        };

                        match reg.prepare_drop_sync_subpackages(
                            &drop_desc.write_url,
                            Some(&readers_refs),
                            Some(class_budget),
                            force_upload.unwrap_or(false),
                        ) {
                            Ok(Some((subpackages, receipt))) => {
                                if subpackages.is_empty() {
                                    break;
                                }

                                let mut class_payloads = Vec::with_capacity(subpackages.len());
                                for sub in &subpackages {
                                    match serde_json::to_vec(&sub.package) {
                                        Ok(pkg_bytes) => {
                                            let mut reader_vks = Vec::with_capacity(sub.reader_dids.len());
                                            for did in &sub.reader_dids {
                                                if let Ok(vk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
                                                    reader_vks.push(vk);
                                                }
                                            }
                                            class_payloads.push((pkg_bytes, reader_vks));
                                        }
                                        Err(e) => {
                                            errors.push(format!("Failed to serialize WoT subpackage: {}", e));
                                        }
                                    }
                                }

                                let class_slices: Vec<(&[u8], &[ed25519_dalek::VerifyingKey])> = class_payloads
                                    .iter()
                                    .map(|(b, v)| (b.as_slice(), v.as_slice()))
                                    .filter(|(_, v)| !v.is_empty())
                                    .collect();

                                if class_slices.is_empty() {
                                    break;
                                }

                                match humoco_market::seal_multi_class_slot_blob(
                                    &class_slices,
                                    effective_wot_max_bytes,
                                ) {
                                    Ok(blob) => {
                                        match sign_drop_channel_request(
                                            &master_seed,
                                            &my_ed_sk,
                                            drop_desc,
                                            "PUT",
                                            "wot",
                                            None,
                                            now_secs,
                                            &blob,
                                        ) {
                                            Ok(signed) => {
                                                match sync_client::put_slot(
                                                    &client,
                                                    &drop_desc.write_url,
                                                    &signed,
                                                    "wot",
                                                    &blob,
                                                    None,
                                                )
                                                .await
                                                {
                                                    Ok(PutSlotOutcome::Saved { .. }) => {
                                                        uploaded_to += 1;
                                                        sync_state.last_upload_at = Some(now_secs);
                                                        reg.commit_drop_sync_receipt(receipt, now_secs);
                                                        reg_modified = true;
                                                    }
                                                    Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                                        errors.push(format!(
                                                            "Upload wot to {}: Precondition failed (ETag)",
                                                            drop_desc.write_url
                                                        ));
                                                    }
                                                    Err(e) => {
                                                        errors.push(format!(
                                                            "Upload wot to {}: {}",
                                                            drop_desc.write_url, e
                                                        ));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                errors.push(format!(
                                                    "Signing error for wot on {}: {}",
                                                    drop_desc.write_url, e
                                                ));
                                            }
                                        }
                                        break;
                                    }
                                    Err(humoco_market::MarketError::PayloadTooLarge(_))
                                        if attempt == 0 =>
                                    {
                                        continue;
                                    }
                                    Err(e) => {
                                        errors.push(format!(
                                            "Failed to seal multi-class wot slot blob: {}",
                                            e
                                        ));
                                        break;
                                    }
                                }
                            }
                            Ok(None) => {
                                // Slot on transport_id is already 100% up-to-date!
                                break;
                            }
                            Err(e) => {
                                errors.push(format!(
                                    "Failed to prepare drop sync subpackages for {}: {}",
                                    drop_desc.write_url, e
                                ));
                                break;
                            }
                        }
                    }
                }

                if reg_modified {
                    let mut service = state.service.lock().unwrap();
                    let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
                }
            }
        }
    }

    // 2b. Check revocation.json (max once per 24h per read_url) across own syncs & contact syncs
    let mut revoked_own_syncs: Vec<FrontendSyncDescriptor> = Vec::new();
    let revocation_candidates: Vec<(String, String)> = {
        let mut service = state.service.lock().unwrap();
        let mut seen_urls = HashSet::new();
        let mut list = Vec::new();
        let mut push_pair = |read_url: &str, drop_pub: &str| {
            let r = read_url.trim();
            let p = drop_pub.trim().to_lowercase();
            if !r.is_empty() && p.len() == 64 && p.chars().all(|c| c.is_ascii_hexdigit()) {
                let norm = r.trim_end_matches('/').to_lowercase();
                if seen_urls.insert(norm) {
                    list.push((r.to_string(), p));
                }
            }
        };
        for d in &settings.market_selected_syncs {
            push_pair(&d.read_url, &d.drop_pub);
        }
        for ps in &settings.public_syncs {
            push_pair(&ps.read_url, &ps.drop_pub);
        }
        if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
            for c in contacts {
                for d in &c.syncs {
                    push_pair(&d.read_url, &d.drop_pub);
                }
            }
        }
        list
    };

    for (rev_read_url, rev_drop_pub) in revocation_candidates {
        let last_chk = sync_state
            .last_revocation_check_at
            .get(&rev_read_url)
            .copied()
            .unwrap_or(0);
        if now_secs.saturating_sub(last_chk) < 86_400 {
            continue;
        }
        sync_state
            .last_revocation_check_at
            .insert(rev_read_url.clone(), now_secs);

        match sync_client::fetch_revocation_notice(&client, &rev_read_url, &rev_drop_pub).await {
            Ok(Some(notice)) => {
                let norm_target = rev_read_url.trim().trim_end_matches('/').to_lowercase();
                let mut service = state.service.lock().unwrap();

                // Update contacts
                if let Ok(data) =
                    service.load_encrypted_data(crate::state::CONTACTS_DATA_NAME, password.as_deref())
                {
                    if let Ok(mut ab) =
                        serde_json::from_slice::<crate::models::FrontendAddressBook>(&data)
                    {
                        if crate::state::apply_revocation_notice_to_address_book(
                            &mut ab,
                            &rev_read_url,
                            &notice,
                            now_secs,
                        ) {
                            if let Ok(bytes) = serde_json::to_vec(&ab) {
                                let _ = service.save_encrypted_data(
                                    crate::state::CONTACTS_DATA_NAME,
                                    &bytes,
                                    password.as_deref(),
                                );
                            }
                            *state.contacts.lock().unwrap() = Some(ab);
                        }
                    }
                }

                // Update own settings (market_selected_syncs & public_syncs)
                let mut settings_changed = false;
                for d in &mut settings.market_selected_syncs {
                    if d.read_url.trim().trim_end_matches('/').to_lowercase() == norm_target
                        && d.drop_pub.trim().eq_ignore_ascii_case(&notice.drop_pub)
                    {
                        crate::state::apply_revocation_notice_to_descriptor(d, &notice, now_secs);
                        revoked_own_syncs.push(d.clone());
                        settings_changed = true;
                    }
                }
                for ps in &mut settings.public_syncs {
                    if ps.read_url.trim().trim_end_matches('/').to_lowercase() == norm_target
                        && ps.drop_pub.trim().eq_ignore_ascii_case(&notice.drop_pub)
                    {
                        crate::state::apply_revocation_notice_to_public_sync(ps, &notice, now_secs);
                        revoked_own_syncs.push(FrontendSyncDescriptor {
                            write_url: ps.write_url.clone(),
                            read_url: ps.read_url.clone(),
                            slot_id: None,
                            drop_pub: ps.drop_pub.clone(),
                            status: ps.status.clone(),
                            sunset_at: ps.sunset_at,
                            revocation_reason: ps.revocation_reason.clone(),
                            replacement: ps.replacement.clone(),
                            ..Default::default()
                        });
                        settings_changed = true;
                    }
                }
                if settings_changed {
                    let _ = state.save_settings(&mut service, settings.clone(), password.as_deref());
                }
            }
            Ok(None) => {
                // Server returned 404 (no revocation / revocation cleared): clear stale local state
                let norm_target = rev_read_url.trim().trim_end_matches('/').to_lowercase();
                let mut service = state.service.lock().unwrap();
                if let Ok(data) =
                    service.load_encrypted_data(crate::state::CONTACTS_DATA_NAME, password.as_deref())
                {
                    if let Ok(mut ab) =
                        serde_json::from_slice::<crate::models::FrontendAddressBook>(&data)
                    {
                        let mut ab_changed = false;
                        for contact in ab.contacts.values_mut() {
                            for ep in &mut contact.syncs {
                                if ep.read_url.trim().trim_end_matches('/').to_lowercase() == norm_target
                                    && ep.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                                    && crate::state::clear_revocation_on_descriptor(ep, "unverified")
                                {
                                    ab_changed = true;
                                }
                            }
                        }
                        if ab_changed {
                            if let Ok(bytes) = serde_json::to_vec(&ab) {
                                let _ = service.save_encrypted_data(
                                    crate::state::CONTACTS_DATA_NAME,
                                    &bytes,
                                    password.as_deref(),
                                );
                            }
                            *state.contacts.lock().unwrap() = Some(ab);
                        }
                    }
                }

                let mut settings_changed = false;
                for d in &mut settings.market_selected_syncs {
                    if d.read_url.trim().trim_end_matches('/').to_lowercase() == norm_target
                        && d.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                        && crate::state::clear_revocation_on_descriptor(d, "active")
                    {
                        settings_changed = true;
                    }
                }
                for ps in &mut settings.public_syncs {
                    if ps.read_url.trim().trim_end_matches('/').to_lowercase() == norm_target
                        && ps.drop_pub.trim().eq_ignore_ascii_case(&rev_drop_pub)
                    {
                        let fallback = if ps.authorized_channels.is_empty() {
                            "pending_auth"
                        } else {
                            "active"
                        };
                        if crate::state::clear_revocation_on_public_sync(ps, fallback) {
                            settings_changed = true;
                        }
                    }
                }
                if settings_changed {
                    let _ = state.save_settings(&mut service, settings.clone(), password.as_deref());
                }
            }
            Err(_) => {}
        }
    }

    // 3. Download poll
    let skip_poll = respect_cooldown == Some(true)
        && force_upload != Some(true)
        && sync_state
            .last_poll_at
            .is_some_and(|last| now_secs.saturating_sub(last) < SYNC_MARKET_POLL_COOLDOWN_SECS);

    let mut sync_health_updates: Vec<(String, bool)> = Vec::new();
    let mut updated_card_store: Option<humoco_market::CardStore> = None;

    if !skip_poll {
        sync_state.last_poll_at = Some(now_secs);

        // Split into direct O(1) slot fetches (for drops with slot_id) and index-based fetches
        let (direct_slot_fetches, candidate_read_urls) = {
            let mut service = state.service.lock().unwrap();
            let mut direct_fetches: Vec<(String, String, Option<String>)> = Vec::new(); // (read_url, slot_id, prefix)
            let mut index_urls = Vec::new();

            // Own selected drops
            for d in &settings.market_selected_syncs {
                if !d.read_url.is_empty() {
                    if let Some(ref sid) = d.slot_id {
                        direct_fetches.push((d.read_url.clone(), sid.clone(), my_prefix.clone()));
                    } else {
                        index_urls.push(d.read_url.clone());
                    }
                }
            }

            let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();
            let card_store: Option<humoco_market::CardStore> = service
                .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());

            // Contacts drops with level >= share_min_level
            if let Ok(contacts) = state.load_contacts(&mut service, password.as_deref()) {
                for c in contacts {
                    let lvl = c.trust_level.unwrap_or_else(|| {
                        if let Some(ref r) = reg {
                            let can = canonicalize_did(&c.did).unwrap_or_else(|_| c.did.clone());
                            r.own_contacts.get(&can).map(|e| e.level).unwrap_or(0)
                        } else {
                            0
                        }
                    });
                    if lvl >= settings.share_min_level {
                        for d in c.syncs {
                            if !d.read_url.is_empty() {
                                if let Some(ref sid) = d.slot_id {
                                    direct_fetches.push((d.read_url.clone(), sid.clone(), None));
                                } else {
                                    index_urls.push(d.read_url.clone());
                                }
                            }
                            if let Some(ref rep) = d.replacement {
                                if !rep.read_url.is_empty() && rep.read_url != d.read_url {
                                    index_urls.push(rep.read_url.clone());
                                }
                            }
                        }
                    }
                }
            }

            // Stored cards drops where author has level >= share_min_level
            if let Some(ref store) = card_store {
                for sc in store.cards.values() {
                    let author_can = canonicalize_did(&sc.card.payload.did)
                        .unwrap_or_else(|_| sc.card.payload.did.clone());
                    let lvl = if let Some(ref r) = reg {
                        r.own_contacts.get(&author_can).map(|e| e.level).unwrap_or(0)
                    } else {
                        0
                    };
                    if lvl >= settings.share_min_level {
                        for d in &sc.card.payload.drops {
                            if !d.read_url.is_empty() {
                                if let Some(ref sid) = d.slot_id {
                                    direct_fetches.push((d.read_url.clone(), sid.clone(), None));
                                } else {
                                    index_urls.push(d.read_url.clone());
                                }
                            }
                        }
                    }
                }
            }

            let mut deduped_urls = Vec::new();
            let mut seen_urls = HashSet::new();
            for u in index_urls {
                if seen_urls.insert(u.clone()) {
                    deduped_urls.push(u);
                    if deduped_urls.len() >= MAX_POLLED_SYNCS {
                        break;
                    }
                }
            }

            let mut deduped_direct = Vec::new();
            let mut seen_direct = HashSet::new();
            for item in direct_fetches {
                if seen_direct.insert(item.clone()) {
                    deduped_direct.push(item);
                }
            }

            (deduped_direct, deduped_urls)
        };

        let mut store: humoco_market::CardStore = {
            let mut service = state.service.lock().unwrap();
            service
                .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default()
        };

        let max_stored_cards = settings.max_stored_cards.clamp(100, 10_000);
        let my_x25519_sk = human_money_core::crypto::ed25519_sk_to_x25519_sk(&my_ed_sk);

        // 3a. Direct O(1) fetches
        for (read_url, slot_id, prefix_opt) in direct_slot_fetches {
            polled_syncs += 1;
            let (base_slot_pk, slot_prefix) =
                humoco_market::parse_slot_id(&slot_id).unwrap_or_else(|_| (slot_id.clone(), None));
            let card_prefix = slot_prefix.as_deref().or(prefix_opt.as_deref());

            // Fetch Base + Delta card slots (Base first, then optional Delta)
            if settings.market_sync_enabled {
                let base_pfx_str = card_prefix.unwrap_or("");
                let base_etag_key = format!("{}:card:{}:{}", read_url, base_slot_pk, base_pfx_str);
                let base_cached_etag = sync_state.slot_etags.get(&base_etag_key).map(|s| s.as_str());
                match sync_client::get_slot(
                    &client,
                    &read_url,
                    &base_slot_pk,
                    card_prefix,
                    "card",
                    base_cached_etag,
                )
                .await
                {
                    Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                        sync_health_updates.push((read_url.clone(), true));
                        if let Some(et) = etag {
                            sync_state.slot_etags.insert(base_etag_key, et);
                        }
                        match humoco_market::open_slot_blob(&blob, &my_x25519_sk) {
                            Ok(Some(bundle)) => {
                                decrypted_slots += 1;
                                imported_cards += import_decrypted_card_bundle(
                                    bundle,
                                    &read_url,
                                    now_secs,
                                    max_stored_cards,
                                    state,
                                    &mut store,
                                    &mut sync_state,
                                    password.as_deref(),
                                );
                            }
                            Ok(None) => {}
                            Err(e) => {
                                errors.push(format!("Error opening card slot {}: {}", slot_id, e));
                            }
                        }
                    }
                    Ok(SlotFetchResult::NotModified) => {
                        sync_health_updates.push((read_url.clone(), true));
                    }
                    Ok(SlotFetchResult::NotFound) => {
                        sync_health_updates.push((read_url.clone(), true));
                        sync_state.slot_etags.remove(&base_etag_key);
                    }
                    Err(e) => {
                        sync_health_updates.push((read_url.clone(), false));
                        errors.push(format!(
                            "Failed to get direct card slot {} from {}: {}",
                            slot_id, read_url, e
                        ));
                    }
                }

                let delta_pfx = humoco_market::derive_delta_prefix(card_prefix);
                let delta_etag_key = format!("{}:card:{}:{}", read_url, base_slot_pk, delta_pfx);
                let delta_cached_etag = sync_state.slot_etags.get(&delta_etag_key).map(|s| s.as_str());
                match sync_client::get_slot(
                    &client,
                    &read_url,
                    &base_slot_pk,
                    Some(&delta_pfx),
                    "card",
                    delta_cached_etag,
                )
                .await
                {
                    Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                        if let Some(et) = etag {
                            sync_state.slot_etags.insert(delta_etag_key, et);
                        }
                        match humoco_market::open_slot_blob(&blob, &my_x25519_sk) {
                            Ok(Some(bundle)) => {
                                decrypted_slots += 1;
                                imported_cards += import_decrypted_card_bundle(
                                    bundle,
                                    &read_url,
                                    now_secs,
                                    max_stored_cards,
                                    state,
                                    &mut store,
                                    &mut sync_state,
                                    password.as_deref(),
                                );
                            }
                            Ok(None) => {}
                            Err(e) => {
                                errors.push(format!("Error opening delta card slot {}: {}", slot_id, e));
                            }
                        }
                    }
                    Ok(SlotFetchResult::NotModified) => {}
                    Ok(SlotFetchResult::NotFound) => {
                        sync_state.slot_etags.remove(&delta_etag_key);
                    }
                    Err(e) => {
                        errors.push(format!(
                            "Failed to get direct delta card slot {} from {}: {}",
                            slot_id, read_url, e
                        ));
                    }
                }
            }

            // Fetch wot slot
            let wot_etag_key = format!("{}:wot:{}:", read_url, base_slot_pk);
            let wot_cached_etag = sync_state.slot_etags.get(&wot_etag_key).map(|s| s.as_str());
            match sync_client::get_slot(
                &client,
                &read_url,
                &base_slot_pk,
                None,
                "wot",
                wot_cached_etag,
            )
            .await
            {
                Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                    sync_health_updates.push((read_url.clone(), true));
                    if let Some(et) = etag {
                        sync_state.slot_etags.insert(wot_etag_key, et);
                    }
                    match humoco_market::open_multi_class_slot_blob(&blob, &my_x25519_sk) {
                        Ok(Some(bytes)) => {
                            decrypted_slots += 1;
                            match serde_json::from_slice::<humoco_wot::WotSyncPackage>(&bytes) {
                                Ok(pkg) => {
                                    imported_wot_lists += import_decrypted_wot_package(
                                        &pkg,
                                        &read_url,
                                        now_secs,
                                        state,
                                        password.as_deref(),
                                        &mut errors,
                                    );
                                }
                                Err(e) => {
                                    errors.push(format!(
                                        "Failed to parse direct wot sync package from {}: {}",
                                        read_url, e
                                    ));
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            errors.push(format!("Error opening direct wot slot {}: {}", slot_id, e));
                        }
                    }
                }
                Ok(SlotFetchResult::NotModified) => {
                    sync_health_updates.push((read_url.clone(), true));
                }
                Ok(SlotFetchResult::NotFound) => {
                    sync_health_updates.push((read_url.clone(), true));
                    sync_state.slot_etags.remove(&wot_etag_key);
                }
                Err(e) => {
                    sync_health_updates.push((read_url.clone(), false));
                    errors.push(format!(
                        "Failed to get direct wot slot {} from {}: {}",
                        slot_id, read_url, e
                    ));
                }
            }
        }

        // 3b. Index-based fetches
        for read_url in candidate_read_urls {
            polled_syncs += 1;
            let cached_etag = sync_state.etags_by_read_url.get(&read_url).map(|s| s.as_str());
            let cached_lm = sync_state.last_modified_by_read_url.get(&read_url).map(|s| s.as_str());

            match sync_client::list_index(&client, &read_url, cached_etag, cached_lm).await {
                Ok(IndexFetchResult::NotModified) => {
                    sync_health_updates.push((read_url.clone(), true));
                }
                Ok(IndexFetchResult::Modified {
                    mut entries,
                    etag,
                    last_modified,
                }) => {
                    sync_health_updates.push((read_url.clone(), true));
                    if let Some(et) = etag {
                        sync_state.etags_by_read_url.insert(read_url.clone(), et);
                    }
                    if let Some(lm) = last_modified {
                        sync_state.last_modified_by_read_url.insert(read_url.clone(), lm);
                    }

                    // Order Base card entries before Delta card entries
                    entries.sort_by_key(|e| {
                        let ch_rank = match e.channel.as_str() {
                            "card" => 0u8,
                            "wot" => 1u8,
                            _ => 2u8,
                        };
                        let delta_rank = if e.channel == "card"
                            && e.prefix
                                .as_deref()
                                .is_some_and(|p| p == "delta" || p.ends_with("-delta"))
                        {
                            1u8
                        } else {
                            0u8
                        };
                        (ch_rank, delta_rank)
                    });

                    // Prune slot ETags for entries no longer advertised by this read_url index
                    let present_keys: HashSet<String> = entries
                        .iter()
                        .filter(|e| e.channel == "card" || e.channel == "wot")
                        .map(|e| {
                            let pfx = if e.channel == "wot" {
                                ""
                            } else {
                                e.prefix.as_deref().unwrap_or("")
                            };
                            format!("{}:{}:{}:{}", read_url, e.channel, e.id, pfx)
                        })
                        .collect();
                    let url_prefix = format!("{}:", read_url);
                    sync_state.slot_etags.retain(|k, _| {
                        if k.starts_with(&url_prefix) {
                            present_keys.contains(k)
                        } else {
                            true
                        }
                    });

                    for entry in entries {
                        if entry.channel != "card" && entry.channel != "wot" {
                            continue;
                        }
                        if entry.channel == "card" && !settings.market_sync_enabled {
                            continue;
                        }
                        let pfx_opt = if entry.channel == "wot" {
                            None
                        } else {
                            entry.prefix.as_deref()
                        };
                        let pfx_str = pfx_opt.unwrap_or("");
                        let etag_key = format!("{}:{}:{}:{}", read_url, entry.channel, entry.id, pfx_str);
                        let cached_slot_etag =
                            sync_state.slot_etags.get(&etag_key).map(|s| s.as_str());
                        if let (Some(e_tag), Some(c_tag)) = (
                            entry
                                .etag
                                .as_deref()
                                .map(|s| s.trim().trim_matches('"'))
                                .filter(|s| !s.is_empty()),
                            cached_slot_etag,
                        ) {
                            if e_tag == c_tag.trim().trim_matches('"') {
                                continue;
                            }
                        }

                        match sync_client::get_slot(
                            &client,
                            &read_url,
                            &entry.id,
                            pfx_opt,
                            &entry.channel,
                            cached_slot_etag,
                        )
                        .await
                        {
                            Ok(SlotFetchResult::Modified { bytes: blob, etag }) => {
                                if let Some(et) = etag.or_else(|| entry.etag.clone()) {
                                    sync_state.slot_etags.insert(etag_key, et);
                                }
                                if entry.channel == "card" {
                                    match humoco_market::open_slot_blob(&blob, &my_x25519_sk) {
                                        Ok(Some(bundle)) => {
                                            decrypted_slots += 1;
                                            imported_cards += import_decrypted_card_bundle(
                                                bundle,
                                                &read_url,
                                                now_secs,
                                                max_stored_cards,
                                                state,
                                                &mut store,
                                                &mut sync_state,
                                                password.as_deref(),
                                            );
                                        }
                                        Ok(None) => {}
                                        Err(e) => {
                                            errors.push(format!(
                                                "Error opening card slot {}: {}",
                                                entry.id, e
                                            ));
                                        }
                                    }
                                } else {
                                    match humoco_market::open_multi_class_slot_blob(
                                        &blob,
                                        &my_x25519_sk,
                                    ) {
                                        Ok(Some(bytes)) => {
                                            decrypted_slots += 1;
                                            match serde_json::from_slice::<humoco_wot::WotSyncPackage>(
                                                &bytes,
                                            ) {
                                                Ok(pkg) => {
                                                    imported_wot_lists +=
                                                        import_decrypted_wot_package(
                                                            &pkg,
                                                            &read_url,
                                                            now_secs,
                                                            state,
                                                            password.as_deref(),
                                                            &mut errors,
                                                        );
                                                }
                                                Err(e) => {
                                                    errors.push(format!(
                                                        "Failed to parse wot sync package from {}: {}",
                                                        read_url, e
                                                    ));
                                                }
                                            }
                                        }
                                        Ok(None) => {}
                                        Err(e) => {
                                            errors.push(format!(
                                                "Error opening wot slot {}: {}",
                                                entry.id, e
                                            ));
                                        }
                                    }
                                }
                            }
                            Ok(SlotFetchResult::NotModified) => {
                                if cached_slot_etag.is_none() {
                                    if let Some(et) = entry.etag.clone() {
                                        sync_state.slot_etags.insert(etag_key, et);
                                    }
                                }
                            }
                            Ok(SlotFetchResult::NotFound) => {
                                sync_state.slot_etags.remove(&etag_key);
                            }
                            Err(e) => {
                                errors.push(format!(
                                    "Failed to get {} slot {}: {}",
                                    entry.channel, entry.id, e
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    sync_health_updates.push((read_url.clone(), false));
                    errors.push(format!("Failed to list index from {}: {}", read_url, e));
                }
            }
        }

        updated_card_store = Some(store);
    }

    // Save updated CardStore, StoredSyncState, and contact health updates
    {
        let mut service = state.service.lock().unwrap();
        if let Some(ref store) = updated_card_store {
            if let Ok(bytes) = serde_json::to_vec(store) {
                let _ = service.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref());
            }
        }
        if let Ok(bytes) = serde_json::to_vec(&sync_state) {
            let _ = service.save_encrypted_data(MARKET_SYNC_KEY, &bytes, password.as_deref());
        }
        if !sync_health_updates.is_empty() {
            state.record_contact_sync_health_batch(&mut service, &sync_health_updates, now_secs, password.as_deref());
        }
    }

    let mut retry_after_secs: Option<u64> = None;
    for e in &errors {
        if let Some(secs) = sync_client::extract_retry_after_from_error_str(e) {
            retry_after_secs = Some(retry_after_secs.map_or(secs, |prev| prev.max(secs)));
        }
    }

    Ok(FrontendMarketSyncResult {
        skipped_wifi_only: false,
        uploaded_to,
        polled_syncs,
        decrypted_slots,
        imported_cards,
        imported_wot_lists,
        errors,
        retry_after_secs,
        revoked_own_syncs,
    })
}

#[tauri::command]
pub async fn sync_market_channels(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    respect_cooldown: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketSyncResult, FrontendError> {
    sync_market_channels_with_options(is_wifi, force_upload, respect_cooldown, password, &state).await
}

pub async fn delete_own_sync_slots_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<usize, FrontendError> {
    let (my_ed_sk, master_seed, my_prefix, drops) = {
        let mut service = state.service.lock().unwrap();
        let (my_ed_sk, my_prefix) = service
            .with_wallet_and_identity(|_w, id| {
                let pfx = human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string());
                (id.signing_key.clone(), pfx)
            })
            .map_err(FrontendError::from)?;
        let master_seed = my_ed_sk.to_bytes();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        (my_ed_sk, master_seed, my_prefix, settings.market_selected_syncs)
    };

    let client = sync_client::create_sync_http_client();
    let mut deleted_count = 0;
    let now = sync_client::current_unix_timestamp();
    let delta_prefix = humoco_market::derive_delta_prefix(my_prefix.as_deref());

    for drop_desc in &drops {
        let desc: humoco_market::DropDescriptor = drop_desc.clone().into();
        for (ch, ch_prefix) in [
            ("card", my_prefix.as_deref()),
            ("card", Some(delta_prefix.as_str())),
            ("wot", None),
            ("backup", my_prefix.as_deref()),
            ("backup", Some(delta_prefix.as_str())),
            ("post", my_prefix.as_deref()),
        ] {
            if let Ok(signed) = sign_drop_channel_request(
                &master_seed,
                &my_ed_sk,
                &desc,
                "DELETE",
                ch,
                ch_prefix,
                now,
                b"",
            ) {
                if sync_client::delete_slot(&client, &drop_desc.write_url, &signed, ch)
                    .await
                    .is_ok()
                {
                    deleted_count += 1;
                }
            }
        }
    }

    // Clear sync_sent_log in TrustRegister
    {
        let mut service = state.service.lock().unwrap();
        if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
            reg.clear_drop_sent_log(None);
            let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
        }
    }

    // Clear upload and poll state in sync_state
    {
        let mut service = state.service.lock().unwrap();
        if let Ok(mut sync_state) = service
            .load_encrypted_data(MARKET_SYNC_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredSyncState>(&bytes)
                    .map_err(|e| FrontendError::from(format!("Parse error: {}", e)))
            })
        {
            sync_state.last_upload_at = None;
            sync_state.last_poll_at = None;
            sync_state.last_readers_hash = None;
            sync_state.card_base_uploaded_at_by_write_url.clear();
            sync_state.card_base_size_by_write_url.clear();
            sync_state.card_base_versions_by_write_url.clear();
            sync_state.card_delta_active_by_write_url.clear();
            sync_state.card_last_uploaded_versions_by_write_url.clear();
            if let Ok(bytes) = serde_json::to_vec(&sync_state) {
                let _ = service.save_encrypted_data(MARKET_SYNC_KEY, &bytes, password.as_deref());
            }
        }
    }

    // Clear slot_etags_by_write_url in StoredPostState
    {
        let mut service = state.service.lock().unwrap();
        if let Ok(mut post_state) = service
            .load_encrypted_data(crate::state::MARKET_POST_STATE_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<crate::state::StoredPostState>(&bytes)
                    .map_err(|e| FrontendError::from(format!("Parse error: {}", e)))
            })
        {
            post_state.slot_etags_by_write_url.clear();
            if let Ok(bytes) = serde_json::to_vec(&post_state) {
                let _ = service.save_encrypted_data(crate::state::MARKET_POST_STATE_KEY, &bytes, password.as_deref());
            }
        }

        if let Ok(bytes) = serde_json::to_vec(&crate::state::StoredBackupSyncState::default()) {
            let _ = service.save_encrypted_data(
                crate::state::BACKUP_SYNC_STATE_KEY,
                &bytes,
                password.as_deref(),
            );
        }
    }

    Ok(deleted_count)
}

#[tauri::command]
pub async fn delete_own_sync_slots(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, FrontendError> {
    delete_own_sync_slots_impl(password, &state).await
}

pub fn preview_contact_trust_change_impl(
    did: String,
    new_level: u8,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendTrustChangeImpact, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();

    let can_target = canonicalize_did(&did).unwrap_or_else(|_| did.clone());

    let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();
    let current_level = reg
        .as_ref()
        .and_then(|r| r.own_contacts.get(&can_target).map(|e| e.level));

    let can_read_my_market_before = current_level.unwrap_or(0) >= settings.share_min_level;
    let can_read_my_market_after = new_level >= settings.share_min_level;

    let is_offender = {
        use human_money_core::models::conflict::TrustStatus;
        service
            .with_wallet(|w| matches!(w.check_reputation(&can_target), TrustStatus::KnownOffender(_)))
            .unwrap_or(false)
    };

    let can_read_my_wot_before = settings.wot_share_via_sync
        && current_level.unwrap_or(0) >= 1
        && !is_offender;
    let can_read_my_wot_after = settings.wot_share_via_sync
        && new_level >= 1
        && !is_offender;

    let admin_opt: Option<StoredSyncAdmin> = service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let (can_write_my_sync_before, can_write_my_sync_after) = if let Some(admin) = admin_opt {
        if admin.deny.iter().any(|d| canonicalize_did(d).ok().as_deref() == Some(&can_target)) {
            (false, false)
        } else if admin.allow.iter().any(|d| canonicalize_did(d).ok().as_deref() == Some(&can_target)) {
            (true, true)
        } else {
            let policy = admin.policies.get("card").cloned().unwrap_or_default();
            (
                current_level.is_some_and(|lvl| lvl >= policy.direct_min),
                new_level >= policy.direct_min,
            )
        }
    } else {
        (false, false)
    };

    let can_send_me_post_before =
        settings.post_accept_from_contacts && current_level.unwrap_or(0) >= 1;
    let can_send_me_post_after = settings.post_accept_from_contacts && new_level >= 1;

    Ok(FrontendTrustChangeImpact {
        can_read_my_market_before,
        can_read_my_market_after,
        can_read_my_wot_before,
        can_read_my_wot_after,
        can_write_my_sync_before,
        can_write_my_sync_after,
        can_send_me_post_before,
        can_send_me_post_after,
    })
}

#[tauri::command]
pub fn preview_contact_trust_change(
    did: String,
    new_level: u8,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendTrustChangeImpact, FrontendError> {
    preview_contact_trust_change_impl(did, new_level, password, &state)
}

// =========================================================================
//  STAGE 1B: PUBLIC DROPS & ROTATION & BADGE COMMANDS
// =========================================================================

pub async fn probe_public_sync_impl(
    url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;

    let probe_client = sync_client::create_sync_probe_client();
    let info = sync_client::fetch_sync_info(&probe_client, &canonical_url)
        .await
        .map_err(FrontendError::from)?;

    let (master_seed, my_prefix) = {
        let mut service = state.service.lock().unwrap();
        let _ = password; // kept for consistency
        let seed = get_wallet_master_seed(&mut service)?;
        let pfx = service
            .with_wallet_and_identity(|_w, id| {
                human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string())
            })
            .ok()
            .flatten();
        (seed, pfx)
    };

    let (_r_sk, _r_vk, access_pubkey) =
        humoco_market::derive_public_drop_keypair(&master_seed, &canonical_url)
            .map_err(|e| FrontendError::from(e.to_string()))?;

    let mut authorized_channels = Vec::new();
    let mut channel_max_bytes_updates = Vec::new();
    for ch in ["card", "wot", "backup", "post"] {
        let ch_prefix = if ch == "wot" { None } else { my_prefix.as_deref() };
        if let Ok(cap) =
            sync_client::head_slot_capability(&probe_client, &canonical_url, &access_pubkey, ch_prefix, ch).await
        {
            if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                channel_max_bytes_updates.push((canonical_url.clone(), ch.to_string(), mb));
            }
            if cap.authorized {
                authorized_channels.push(ch.to_string());
            }
        }
    }
    save_channel_max_bytes_updates(state, password.as_deref(), &channel_max_bytes_updates);

    Ok(FrontendPublicSyncInfo {
        write_url: canonical_url,
        read_url: info.read_url,
        drop_pub: info.drop_pub,
        register_url: info.register_url,
        access_pubkey,
        authorized_channels,
    })
}

#[tauri::command]
pub async fn probe_public_sync(
    url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    probe_public_sync_impl(url, password, &state).await
}

pub async fn save_public_sync_impl(
    write_url: String,
    read_url: String,
    drop_pub: String,
    register_url: Option<String>,
    enabled: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    sync_client::validate_sync_url(&canonical_url).map_err(FrontendError::from)?;
    sync_client::validate_sync_url(&read_url).map_err(FrontendError::from)?;
    humoco_market::parse_ed25519_pubkey_hex64(&drop_pub)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let (master_seed, my_prefix) = {
        let mut service = state.service.lock().unwrap();
        let seed = get_wallet_master_seed(&mut service)?;
        let pfx = service
            .with_wallet_and_identity(|_w, id| {
                human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                    .map(|s| s.to_string())
            })
            .ok()
            .flatten();
        (seed, pfx)
    };

    let (_r_sk, _r_vk, access_pubkey) =
        humoco_market::derive_public_drop_keypair(&master_seed, &canonical_url)
            .map_err(|e| FrontendError::from(e.to_string()))?;

    let probe_client = sync_client::create_sync_probe_client();
    let info_opt = sync_client::fetch_sync_info(&probe_client, &canonical_url).await.ok();
    let clean_register_url = register_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty() && sync_client::validate_sync_url(u).is_ok())
        .or_else(|| info_opt.as_ref().and_then(|i| i.register_url.clone()));

    let mut authorized_channels = Vec::new();
    let mut channel_max_bytes_updates = Vec::new();
    for ch in ["card", "wot", "backup", "post"] {
        let ch_prefix = if ch == "wot" { None } else { my_prefix.as_deref() };
        if let Ok(cap) =
            sync_client::head_slot_capability(&probe_client, &canonical_url, &access_pubkey, ch_prefix, ch).await
        {
            if let Some(mb) = cap.max_bytes.filter(|&v| v > 0) {
                channel_max_bytes_updates.push((canonical_url.clone(), ch.to_string(), mb));
            }
            if cap.authorized {
                authorized_channels.push(ch.to_string());
            }
        }
    }
    save_channel_max_bytes_updates(state, password.as_deref(), &channel_max_bytes_updates);

    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let mut stored = StoredPublicSync {
            write_url: canonical_url.clone(),
            read_url: read_url.clone(),
            drop_pub: drop_pub.clone(),
            register_url: clean_register_url.clone(),
            access_pubkey: access_pubkey.clone(),
            enabled,
            authorized_channels: authorized_channels.clone(),
            status: if !authorized_channels.is_empty() {
                "active".to_string()
            } else {
                "pending_auth".to_string()
            },
            ..Default::default()
        };

        if let Some(ref info) = info_opt {
            if let Some(ref notice) = info.revocation {
                if sync_client::verify_revocation_notice(notice, &drop_pub).is_ok() {
                    crate::state::apply_revocation_notice_to_public_sync(
                        &mut stored,
                        notice,
                        sync_client::current_unix_timestamp(),
                    );
                }
            }
        }

        if let Some(pos) = settings.public_syncs.iter().position(|d| d.write_url == canonical_url) {
            settings.public_syncs[pos] = stored;
        } else {
            settings.public_syncs.push(stored);
        }

        let selected_syncs = settings.market_selected_syncs.clone();
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;

        crate::commands::market::sync_own_card_drops_on_settings_change(
            &mut service,
            &selected_syncs,
            password.as_deref(),
        );
    }

    let _ = crate::commands::recovery::enqueue_guardian_locator_updates_if_configured(state, password.clone()).await;

    Ok(FrontendPublicSyncInfo {
        write_url: canonical_url,
        read_url,
        drop_pub,
        register_url: clean_register_url,
        access_pubkey,
        authorized_channels,
    })
}

#[tauri::command]
pub async fn save_public_sync(
    write_url: String,
    read_url: String,
    drop_pub: String,
    register_url: Option<String>,
    enabled: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    save_public_sync_impl(write_url, read_url, drop_pub, register_url, enabled, password, &state).await
}

pub async fn remove_public_sync_impl(
    write_url: String,
    delete_remote_slots: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if delete_remote_slots {
        let (master_seed, my_prefix) = {
            let mut service = state.service.lock().unwrap();
            let seed = get_wallet_master_seed(&mut service)?;
            let pfx = service
                .with_wallet_and_identity(|_w, id| {
                    human_money_core::crypto::get_prefix_from_user_id(&id.user_id)
                        .map(|s| s.to_string())
                })
                .ok()
                .flatten();
            (seed, pfx)
        };
        if let Ok((r_sk, _, _)) = humoco_market::derive_public_drop_keypair(&master_seed, &canonical_url) {
            let client = sync_client::create_sync_http_client();
            let now = sync_client::current_unix_timestamp();
            let acct_nonce = sync_client::generate_sync_nonce();
            let acct_signed = humoco_market::sign_delete_account_request(&r_sk, now, &acct_nonce);
            if sync_client::delete_account(&client, &canonical_url, &acct_signed)
                .await
                .is_err()
            {
                let delta_prefix = humoco_market::derive_delta_prefix(my_prefix.as_deref());
                for (ch, ch_prefix) in [
                    ("card", my_prefix.as_deref()),
                    ("card", Some(delta_prefix.as_str())),
                    ("wot", None),
                    ("backup", my_prefix.as_deref()),
                    ("backup", Some(delta_prefix.as_str())),
                    ("post", my_prefix.as_deref()),
                ] {
                    let nonce = sync_client::generate_sync_nonce();
                    let signed = humoco_market::sign_member_request_direct(&r_sk, "DELETE", ch, ch_prefix, now, &nonce, b"");
                    let _ = sync_client::delete_slot(&client, &canonical_url, &signed, ch).await;
                }
                let media_nonce = sync_client::generate_sync_nonce();
                let media_signed = humoco_market::sign_member_request_direct(&r_sk, "DELETE", "media", None, now, &media_nonce, b"");
                let _ = sync_client::delete_media_blob(&client, &canonical_url, &media_signed, None).await;
            }
        }
    }

    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        settings.public_syncs.retain(|d| d.write_url != canonical_url);
        let selected_syncs = settings.market_selected_syncs.clone();
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;

        crate::commands::market::sync_own_card_drops_on_settings_change(
            &mut service,
            &selected_syncs,
            password.as_deref(),
        );

        if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
            reg.clear_drop_sent_log(Some(&canonical_url));
            let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
        }

        if let Ok(mut post_state) = service
            .load_encrypted_data(crate::state::MARKET_POST_STATE_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<crate::state::StoredPostState>(&bytes)
                    .map_err(|e| FrontendError::from(format!("Parse error: {}", e)))
            })
        {
            post_state.slot_etags_by_write_url.remove(&canonical_url);
            post_state.slot_etags_by_write_url.remove(&write_url);
            if let Ok(bytes) = serde_json::to_vec(&post_state) {
                let _ = service.save_encrypted_data(crate::state::MARKET_POST_STATE_KEY, &bytes, password.as_deref());
            }
        }

        if let Ok(bytes) = service.load_encrypted_data(crate::state::BACKUP_SYNC_STATE_KEY, password.as_deref()) {
            if let Ok(mut backup_state) = serde_json::from_slice::<crate::state::StoredBackupSyncState>(&bytes) {
                let removed = backup_state.mirrors_by_write_url.remove(&canonical_url).is_some()
                    || backup_state.mirrors_by_write_url.remove(&write_url).is_some();
                if removed {
                    if let Ok(encoded) = serde_json::to_vec(&backup_state) {
                        let _ = service.save_encrypted_data(
                            crate::state::BACKUP_SYNC_STATE_KEY,
                            &encoded,
                            password.as_deref(),
                        );
                    }
                }
            }
        }
    }

    let _ = crate::commands::recovery::enqueue_guardian_locator_updates_if_configured(state, password.clone()).await;

    Ok(())
}

#[tauri::command]
pub async fn remove_public_sync(
    write_url: String,
    delete_remote_slots: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    remove_public_sync_impl(write_url, delete_remote_slots, password, &state).await
}

pub fn get_sync_overview_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendSyncOverviewStatus, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();

    let admin_opt: Option<StoredSyncAdmin> = service
        .load_encrypted_data(MARKET_SYNC_ADMIN_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let has_own_sync = admin_opt.as_ref().is_some_and(|a| !a.descriptor.write_url.is_empty());
    let pending_own_sync = admin_opt.as_ref().is_some_and(|a| a.descriptor.write_url.is_empty());
    let own_read_url = admin_opt.as_ref().map(|a| a.descriptor.read_url.as_str()).unwrap_or("");

    let friend_syncs = settings
        .market_selected_syncs
        .iter()
        .filter(|d| {
            d.read_url != own_read_url
                && !d.read_url.is_empty()
                && !d.drop_pub.is_empty()
                && d.fail_count == 0
                && d.status == "active"
        })
        .count();

    let public_syncs = settings
        .public_syncs
        .iter()
        .filter(|d| d.enabled && !d.authorized_channels.is_empty())
        .count();

    let pending_public_syncs = settings
        .public_syncs
        .iter()
        .filter(|d| d.authorized_channels.is_empty())
        .count();

    let contacts = state.load_contacts(&mut service, password.as_deref()).unwrap_or_default();
    let mut pending_keys = std::collections::HashSet::new();
    for c in &contacts {
        for ep in &c.syncs {
            if ep.status == "pending_auth" {
                let key = crate::state::sync_endpoint_key(ep);
                if !key.is_empty() {
                    pending_keys.insert(key);
                }
            }
        }
    }
    for sel in &settings.market_selected_syncs {
        if sel.status == "pending_auth" {
            let key = crate::state::sync_endpoint_key(sel);
            if !key.is_empty() {
                pending_keys.insert(key);
            }
        }
    }
    let pending_auth_count = pending_keys.len();

    let failing_selected = settings
        .market_selected_syncs
        .iter()
        .filter(|d| d.fail_count > 0 || d.status == "unreachable")
        .count();
    let failing_count = failing_selected;

    let connected_count = (if has_own_sync { 1 } else { 0 }) + friend_syncs + public_syncs;
    let pending_count = (if pending_own_sync { 1 } else { 0 }) + pending_public_syncs;

    let sync_state: Option<StoredSyncState> = service
        .load_encrypted_data(MARKET_SYNC_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let last_sync_at = sync_state.and_then(|s| s.last_upload_at.into_iter().chain(s.last_poll_at).max());

    Ok(FrontendSyncOverviewStatus {
        connected_count,
        pending_count,
        friend_syncs,
        public_syncs,
        has_own_sync,
        own_sync_soft_warning: false,
        last_sync_at,
        pending_auth_count,
        failing_count,
    })
}

#[tauri::command]
pub fn get_sync_overview_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncOverviewStatus, FrontendError> {
    get_sync_overview_status_impl(password, &state)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use crate::models::{
        FrontendContact, FrontendSaveMyMarketProfileRequest, FrontendUserProfile,
    };
    use crate::commands::market::{
        get_market_view_impl, save_my_market_profile_impl, set_card_published_impl,
    };

    pub(crate) const ALICE_PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    pub(crate) const BOB_PHRASE: &str =
        "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
    pub(crate) const CHARLIE_PHRASE: &str =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";
    pub(crate) const READER1_PHRASE: &str =
        "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
    pub(crate) const READER2_PHRASE: &str =
        "vessel ladder alter error federal sibling chat ability sun glass valve picture";

    pub(crate) fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("humoco_sync_test_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    pub(crate) fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service
            .create_profile(
                profile_name,
                phrase,
                None,
                Some("pc"),
                password,
                human_money_core::MnemonicLanguage::English,
                "device-drops-test".into(),
                true,
            )
            .unwrap();
        service
    }

    pub(crate) fn create_test_state(service: AppService) -> AppState {
        AppState::new(service)
    }

    #[allow(dead_code)]
    pub(crate) struct MockSyncServer {
        pub base_url: String,
        pub data_dir: String,
        pub admin_pubkey: Arc<Mutex<String>>,
        pub members: Arc<Mutex<Vec<String>>>,
        pub channel_members: Arc<Mutex<HashMap<String, Vec<String>>>>,
        pub slots: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        pub etags: Arc<Mutex<HashMap<String, String>>>,
        pub rotation_cert: Arc<Mutex<Option<String>>>,
        pub put_members_count: Arc<Mutex<usize>>,
        pub _shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl MockSyncServer {
        pub async fn start(data_dir: String, admin_pubkey: String) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let base_url = format!("http://127.0.0.1:{}", port);

            let admin_pub = Arc::new(Mutex::new(admin_pubkey));
            let members = Arc::new(Mutex::new(Vec::<String>::new()));
            let channel_members = Arc::new(Mutex::new(HashMap::<String, Vec<String>>::new()));
            let slots = Arc::new(Mutex::new(HashMap::<String, Vec<u8>>::new()));
            let etags = Arc::new(Mutex::new(HashMap::<String, String>::new()));
            let rotation_cert = Arc::new(Mutex::new(None));
            let put_members_count = Arc::new(Mutex::new(0));
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

            let s_data_dir = data_dir.clone();
            let s_base_url = base_url.clone();
            let s_admin_pub = Arc::clone(&admin_pub);
            let s_members = Arc::clone(&members);
            let s_channel_members = Arc::clone(&channel_members);
            let s_slots = Arc::clone(&slots);
            let s_etags = Arc::clone(&etags);
            let s_rotation = Arc::clone(&rotation_cert);
            let s_put_members = Arc::clone(&put_members_count);

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            break;
                        }
                        Ok((mut stream, _)) = listener.accept() => {
                            let data_dir = s_data_dir.clone();
                            let base_url = s_base_url.clone();
                            let admin_pub = Arc::clone(&s_admin_pub);
                            let members = Arc::clone(&s_members);
                            let channel_members = Arc::clone(&s_channel_members);
                            let slots = Arc::clone(&s_slots);
                            let etags = Arc::clone(&s_etags);
                            let rotation = Arc::clone(&s_rotation);
                            let put_members = Arc::clone(&s_put_members);

                            tokio::spawn(async move {
                                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                                let mut buf = vec![0u8; 1_048_576];
                                let mut read_bytes = 0;

                                while read_bytes < buf.len() {
                                    let n = match stream.read(&mut buf[read_bytes..]).await {
                                        Ok(0) => break,
                                        Ok(n) => n,
                                        Err(_) => break,
                                    };
                                    read_bytes += n;
                                    if buf[..read_bytes].windows(4).any(|w| w == b"\r\n\r\n") {
                                        break;
                                    }
                                }

                                if read_bytes == 0 {
                                    return;
                                }

                                let header_end = buf[..read_bytes]
                                    .windows(4)
                                    .position(|w| w == b"\r\n\r\n")
                                    .unwrap_or(read_bytes);
                                let header_str = String::from_utf8_lossy(&buf[..header_end]);
                                let mut lines = header_str.lines();
                                let req_line = lines.next().unwrap_or("");
                                let mut parts = req_line.split_whitespace();
                                let method = parts.next().unwrap_or("");
                                let path = parts.next().unwrap_or("");

                                let mut headers = HashMap::new();
                                for l in lines {
                                    if let Some((k, v)) = l.split_once(':') {
                                        headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                                    }
                                }

                                let content_len: usize = headers
                                    .get("content-length")
                                    .and_then(|v| v.parse().ok())
                                    .unwrap_or(0);
                                let body_start = header_end + 4;
                                let mut body = buf[body_start..read_bytes].to_vec();

                                while body.len() < content_len {
                                    let mut chunk = vec![0u8; content_len - body.len()];
                                    match stream.read(&mut chunk).await {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            body.extend_from_slice(&chunk[..n]);
                                        }
                                        Err(_) => break,
                                    }
                                }

                                let rot_header_opt = rotation.lock().unwrap().clone();
                                let rot_hdr_line = if let Some(ref r) = rot_header_opt {
                                    format!("X-Humoco-Sync-Rotation: {}\r\n", r)
                                } else {
                                    String::new()
                                };

                                let response: Vec<u8> = if method == "GET" && path.contains("info") {
                                    let cur_admin = admin_pub.lock().unwrap().clone();
                                    let info_json = serde_json::json!({
                                        "drop_pub": cur_admin,
                                        "read_url": format!("{}/{}", base_url, data_dir),
                                        "register_url": null,
                                        "channels": ["card", "wot", "backup", "post"]
                                    });
                                    let body_str = serde_json::to_string(&info_json).unwrap();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\n\r\n{}",
                                        rot_hdr_line, body_str.len(), body_str
                                    ).into_bytes()
                                } else if method == "HEAD" && path.starts_with("/sync.php") {
                                    let req_pub = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                    let ch = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                    let prefix_opt = headers.get("x-drop-prefix").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                    let key = match prefix_opt {
                                        Some(ref pfx) => format!("{}_{}.{}", req_pub, pfx, ch),
                                        None => format!("{}.{}", req_pub, ch),
                                    };
                                    let is_auth = members.lock().unwrap().contains(&req_pub);
                                    let occupied = slots.lock().unwrap().contains_key(&key);
                                    let etag_hdr = if is_auth && occupied {
                                        etags
                                            .lock()
                                            .unwrap()
                                            .get(&key)
                                            .map(|e| format!("ETag: {}\r\n", e))
                                            .unwrap_or_default()
                                    } else {
                                        String::new()
                                    };
                                    format!(
                                        "HTTP/1.1 200 OK\r\nCache-Control: no-store\r\nX-Drop-Authorized: {}\r\nX-Drop-Channels: card,wot,backup,post\r\nX-Channel-Max-Bytes: 524288\r\nX-Channel-Ttl-Days: 30\r\nX-Slot-Occupied: {}\r\n{}{}Content-Length: 0\r\n\r\n",
                                        if is_auth { "true" } else { "false" },
                                        if occupied { "true" } else { "false" },
                                        etag_hdr,
                                        rot_hdr_line
                                    ).into_bytes()
                                } else if method == "PUT" && path.starts_with("/sync.php") && path.contains("members") {
                                    *put_members.lock().unwrap() += 1;
                                    #[derive(serde::Deserialize)]
                                    struct MembersJson {
                                        pubkeys: Vec<String>,
                                    }
                                    if let Ok(mj) = serde_json::from_slice::<MembersJson>(&body) {
                                        let ch = if let Some((_, rest)) = path.split_once("channel=") {
                                            rest.split('&').next().unwrap_or("card")
                                        } else {
                                            "card"
                                        };
                                        channel_members.lock().unwrap().insert(ch.to_string(), mj.pubkeys);
                                        let mut m = members.lock().unwrap();
                                        let mut union_members = HashSet::new();
                                        for pks in channel_members.lock().unwrap().values() {
                                            for pk in pks {
                                                union_members.insert(pk.clone());
                                            }
                                        }
                                        *m = union_members.into_iter().collect();
                                        let mut s = slots.lock().unwrap();
                                        s.retain(|k, _| {
                                            if let Some((id_part, _)) = k.split_once('.') {
                                                let base_pk = id_part.split_once('_').map(|(pk, _)| pk).unwrap_or(id_part);
                                                m.iter().any(|id| id == base_pk)
                                            } else {
                                                false
                                            }
                                        });
                                        format!("HTTP/1.1 204 No Content\r\n{}Content-Length: 0\r\n\r\n", rot_hdr_line).into_bytes()
                                    } else {
                                        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec()
                                    }
                                } else if method == "GET" && path.starts_with("/sync.php") && path.contains("status") {
                                    let cur_admin = admin_pub.lock().unwrap().clone();
                                    let s_map = slots.lock().unwrap();
                                    let status_json = serde_json::json!({
                                        "used_bytes": s_map.values().map(|v| v.len()).sum::<usize>(),
                                        "max_bytes": 268435456,
                                        "soft_limit_bytes": 214748364,
                                        "soft_warning": false,
                                        "slot_count": s_map.len(),
                                        "channel_counts": {"card": s_map.len()},
                                        "admin_pubkey": cur_admin,
                                        "rotation": null
                                    });
                                    let body_str = serde_json::to_string(&status_json).unwrap();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\n\r\n{}",
                                        rot_hdr_line, body_str.len(), body_str
                                    ).into_bytes()
                                } else if method == "PUT" && path.starts_with("/sync.php") {
                                    let pubkey = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                    let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                    let prefix_opt = headers.get("x-drop-prefix").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                    let is_member = members.lock().unwrap().contains(&pubkey);
                                    if is_member {
                                        let key = match prefix_opt {
                                            Some(ref pfx) => format!("{}_{}.{}", pubkey, pfx, channel),
                                            None => format!("{}.{}", pubkey, channel),
                                        };
                                        let if_match_hdr = headers.get("if-match").cloned();
                                        let cur_etag = etags.lock().unwrap().get(&key).cloned().unwrap_or_else(|| "\"etag123\"".to_string());
                                        if let Some(ref im) = if_match_hdr {
                                            let im_trim = im.trim_matches('"');
                                            let cur_trim = cur_etag.trim_matches('"');
                                            if im != "none" && im_trim != cur_trim {
                                                format!(
                                                    "HTTP/1.1 412 Precondition Failed\r\nETag: {}\r\nContent-Length: 0\r\n\r\n",
                                                    cur_etag
                                                ).into_bytes()
                                            } else {
                                                slots.lock().unwrap().insert(key.clone(), body);
                                                let new_etag = format!("\"etag_{}\"", rand::random::<u32>());
                                                etags.lock().unwrap().insert(key, new_etag.clone());
                                                format!("HTTP/1.1 204 No Content\r\n{}ETag: {}\r\nContent-Length: 0\r\n\r\n", rot_hdr_line, new_etag).into_bytes()
                                            }
                                        } else {
                                            slots.lock().unwrap().insert(key.clone(), body);
                                            let new_etag = format!("\"etag_{}\"", rand::random::<u32>());
                                            etags.lock().unwrap().insert(key, new_etag.clone());
                                            format!("HTTP/1.1 204 No Content\r\n{}ETag: {}\r\nContent-Length: 0\r\n\r\n", rot_hdr_line, new_etag).into_bytes()
                                        }
                                    } else {
                                        format!("HTTP/1.1 403 Forbidden\r\n{}Content-Length: 0\r\n\r\n", rot_hdr_line).into_bytes()
                                    }
                                } else if method == "DELETE" && path.starts_with("/sync.php") {
                                    let pubkey = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                    if path.contains("account") {
                                        let dot_prefix = format!("{}.", pubkey);
                                        let under_prefix = format!("{}_", pubkey);
                                        slots.lock().unwrap().retain(|k, _| !k.starts_with(&dot_prefix) && !k.starts_with(&under_prefix));
                                        etags.lock().unwrap().retain(|k, _| !k.starts_with(&dot_prefix) && !k.starts_with(&under_prefix));
                                    } else {
                                        let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                        let prefix_opt = headers.get("x-drop-prefix").map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                        let key = match prefix_opt {
                                            Some(ref pfx) => format!("{}_{}.{}", pubkey, pfx, channel),
                                            None => format!("{}.{}", pubkey, channel),
                                        };
                                        slots.lock().unwrap().remove(&key);
                                        etags.lock().unwrap().remove(&key);
                                    }
                                    format!("HTTP/1.1 204 No Content\r\n{}Content-Length: 0\r\n\r\n", rot_hdr_line).into_bytes()
                                } else if method == "GET" && path.contains("index.json") {
                                    let s_map = slots.lock().unwrap();
                                    let e_map = etags.lock().unwrap();
                                    let mut entries = Vec::new();
                                    for (k, v) in s_map.iter() {
                                        if let Some((id_part, ch)) = k.split_once('.') {
                                            let (id, prefix) = match id_part.split_once('_') {
                                                Some((pk, pfx)) => (pk, Some(pfx)),
                                                None => (id_part, None),
                                            };
                                            let slot_etag = e_map
                                                .get(k)
                                                .map(|e| e.trim_matches('"').to_string())
                                                .unwrap_or_else(|| hex::encode(&sha2::Sha256::digest(v)[..16]));
                                            let mut obj = serde_json::json!({
                                                "id": id,
                                                "channel": ch,
                                                "mtime": 1000,
                                                "size": v.len(),
                                                "etag": slot_etag
                                            });
                                            if let Some(pfx) = prefix {
                                                obj["prefix"] = serde_json::Value::String(pfx.to_string());
                                            }
                                            entries.push(obj);
                                        }
                                    }
                                    let body_str = serde_json::to_string(&entries).unwrap();
                                    let idx_hash = hex::encode(&sha2::Sha256::digest(body_str.as_bytes())[..16]);
                                    let idx_etag = format!("\"{}\"", idx_hash);
                                    let not_modified = headers
                                        .get("if-none-match")
                                        .is_some_and(|inm| inm.trim().trim_matches('"') == idx_hash || inm.trim() == "*");
                                    if not_modified {
                                        format!("HTTP/1.1 304 Not Modified\r\nETag: {}\r\nContent-Length: 0\r\n\r\n", idx_etag).into_bytes()
                                    } else {
                                        format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: {}\r\nContent-Length: {}\r\n\r\n{}",
                                            idx_etag, body_str.len(), body_str
                                        ).into_bytes()
                                    }
                                } else if method == "GET" && path.ends_with(".bin") {
                                    let fname = path.rsplit('/').next().unwrap_or("");
                                    let slot_key = fname.strip_suffix(".bin").unwrap_or(fname);
                                    let s_map = slots.lock().unwrap();
                                    if let Some(data) = s_map.get(slot_key) {
                                        let cur_etag = etags
                                            .lock()
                                            .unwrap()
                                            .get(slot_key)
                                            .cloned()
                                            .unwrap_or_else(|| format!("\"{}\"", hex::encode(&sha2::Sha256::digest(data)[..16])));
                                        let clean_etag = cur_etag.trim_matches('"');
                                        let not_modified = headers
                                            .get("if-none-match")
                                            .is_some_and(|inm| inm.trim().trim_matches('"') == clean_etag || inm.trim() == "*");
                                        if not_modified {
                                            format!("HTTP/1.1 304 Not Modified\r\nETag: {}\r\nContent-Length: 0\r\n\r\n", cur_etag).into_bytes()
                                        } else {
                                            let mut resp = format!(
                                                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nETag: {}\r\nContent-Length: {}\r\n\r\n",
                                                cur_etag, data.len()
                                            ).into_bytes();
                                            resp.extend_from_slice(data);
                                            resp
                                        }
                                    } else {
                                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                                    }
                                } else {
                                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                                };

                                if let Some(crlf_pos) = response.windows(2).position(|w| w == b"\r\n") {
                                    let insert_at = crlf_pos + 2;
                                    let mut framed = Vec::with_capacity(response.len() + 19);
                                    framed.extend_from_slice(&response[..insert_at]);
                                    framed.extend_from_slice(b"Connection: close\r\n");
                                    framed.extend_from_slice(&response[insert_at..]);
                                    let _ = stream.write_all(&framed).await;
                                } else {
                                    let _ = stream.write_all(&response).await;
                                }
                                let _ = stream.shutdown().await;
                            });
                        }
                    }
                }
            });

            Self {
                base_url,
                data_dir,
                admin_pubkey: admin_pub,
                members,
                channel_members,
                slots,
                etags,
                rotation_cert,
                put_members_count,
                _shutdown_tx: Some(shutdown_tx),
            }
        }
    }

    #[tokio::test]
    async fn test_phase_3_acceptance_criteria_flow() {
        let base_dir = temp_test_dir("phase3_flow");
        let password = "TestPassword123!";

        // 1. Setup actors: Alice (Admin), Bob, Charlie, Reader L1, Reader L2
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let charlie_service = setup_test_app(&base_dir.join("charlie"), "Charlie", CHARLIE_PHRASE, password);
        let l1_service = setup_test_app(&base_dir.join("l1"), "Reader1", READER1_PHRASE, password);
        let l2_service = setup_test_app(&base_dir.join("l2"), "Reader2", READER2_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let charlie_state = create_test_state(charlie_service);
        let l1_state = create_test_state(l1_service);
        let l2_state = create_test_state(l2_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let charlie_did = {
            let s = charlie_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let l1_did = {
            let s = l1_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // 2. Alice prepares drop setup and finalize on MockSyncServer A
        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let server_a = MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_sync_setup_impl(
            server_a.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        assert_eq!(finalize_status.data_dir, draft.data_dir);
        assert_eq!(finalize_status.total_member_count, 1); // Alice is in allow list automatically

        // Test connection
        let conn_test = test_sync_connection_impl(
            server_a.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert!(conn_test.success);
        assert!(conn_test.write_ok);
        assert!(conn_test.read_ok);

        // 3. Alice authorizes Bob directly (Level 2). Bob authorizes Charlie (Level 2).
        {
            let mut s = alice_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            alice_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        {
            let mut s = bob_state.service.lock().unwrap();
            let charlie_contact = FrontendContact {
                did: charlie_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Charlie".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            bob_state.save_contact(&mut s, charlie_contact, Some(password)).unwrap();
        }

        // Bob signs his SD-JWT and Alice stores it in her raw_list_store
        let bob_sd_jwt = {
            let mut s = bob_state.service.lock().unwrap();
            let mut reg = bob_state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            bob_state.ensure_wot_list_signed(&mut s, &mut reg).unwrap();
            reg.own_signed_list.unwrap()
        };

        {
            let mut s = alice_state.service.lock().unwrap();
            let mut reg = alice_state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let can_bob = canonicalize_did(&bob_did).unwrap();
            reg.raw_list_store.insert(can_bob, bob_sd_jwt);
            alice_state.save_wot_register(&mut s, &reg, Some(password)).unwrap();
        }

        // Alice updates drop admin config and pushes to Server A
        let updated_status = update_sync_admin_config_impl(
            FrontendSyncAdminUpdateRequest::default(),
            true,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Admin A authorizes B directly and C via B's SD-JWT trust list
        assert_eq!(updated_status.direct_member_count, 2); // Alice + Bob
        assert_eq!(updated_status.hop2_member_count, 1);   // Charlie
        assert_eq!(updated_status.total_member_count, 3);
        assert_eq!(server_a.members.lock().unwrap().len(), 3);

        // 4. Charlie discovers its own authorization via discover_available_syncs
        {
            let mut s = charlie_state.service.lock().unwrap();
            let alice_contact = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(1),
                desired_hops: Some(2),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![finalize_status.descriptor.clone()],
            };
            charlie_state.save_contact(&mut s, alice_contact, Some(password)).unwrap();
        }

        // Charlie probes contacts to check reachability and authorization on Server A
        let probe_results = probe_contact_syncs_impl(
            false,
            Some(password.to_string()),
            &charlie_state,
        )
        .await
        .unwrap();
        assert!(!probe_results.is_empty(), "Charlie should probe Server A");

        let charlie_discovered = discover_available_syncs_impl(
            Some(password.to_string()),
            &charlie_state,
        )
        .await
        .unwrap();

        assert_eq!(charlie_discovered.len(), 1);
        assert!(charlie_discovered[0].is_authorized, "Charlie should discover authorization on Server A!");

        // 5. Start Server E for redundant public drop upload
        let server_e = MockSyncServer::start("d-server-e".to_string(), "2222222222222222222222222222222222222222222222222222222222222222".to_string()).await;
        let server_e_url = format!("{}/sync.php", server_e.base_url);

        // Bob probes Server E
        let e_info = probe_public_sync_impl(server_e_url.clone(), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(e_info.access_pubkey.len(), 64);

        // Authorize Bob's access pubkey on Server E
        server_e.members.lock().unwrap().push(e_info.access_pubkey.clone());

        // Bob saves Server E as a public drop
        save_public_sync_impl(
            server_e_url.clone(),
            format!("{}/d-server-e", server_e.base_url),
            e_info.drop_pub.clone(),
            None,
            true,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();

        // Bob configures selected upload targets: Drop A
        let selected = set_selected_upload_syncs_impl(
            vec![finalize_status.descriptor.clone()],
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(selected.len(), 1);

        // Bob adds Reader 1 (L1) with level 2 (>= share_min_level 1)
        {
            let mut s = bob_state.service.lock().unwrap();
            let l1_contact = FrontendContact {
                did: l1_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Reader1".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            bob_state.save_contact(&mut s, l1_contact, Some(password)).unwrap();
        }

        // Bob creates and saves his own market profile and publishes it
        let save_req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                last_name: Some("Baker".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![humoco_market::Skill {
                title: "Fresh Bread".to_string(),
                text: Some("Warm and crispy".to_string()),
                pricing: Some("5 M".to_string()),
                is_remote: false,
            }],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
        };
        save_my_market_profile_impl(save_req, Some(password.to_string()), &bob_state).unwrap();
        set_card_published_impl(true, false, Some(password.to_string()), &bob_state).await.unwrap();

        // Bob uploads redundantly to Drop A (selected) and Drop E (active public drop)
        let bob_sync = sync_market_channels_impl(
            None,
            Some(true),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(bob_sync.uploaded_to, 4, "Bob should upload card and wot channels to both Drop A and Drop E");
        assert_eq!(server_a.slots.lock().unwrap().len(), 2);
        assert_eq!(server_e.slots.lock().unwrap().len(), 2);

        // 6. Authorized reader L1 decrypts Bob's slot and imports cards and wot list
        {
            let mut s = l1_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![finalize_status.descriptor.clone()],
            };
            l1_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        let l1_sync = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &l1_state,
        )
        .await
        .unwrap();
        assert_eq!(l1_sync.decrypted_slots, 2, "Authorized reader L1 should decrypt both card and wot slots");
        assert_eq!(l1_sync.imported_cards, 1, "Authorized reader L1 should import card");
        assert_eq!(l1_sync.imported_wot_lists, 1, "Authorized reader L1 should import wot list");

        // Verify Bob's card appears in L1's market view with Drop entry source
        let l1_market = get_market_view_impl(Some(password.to_string()), &l1_state).unwrap();
        let can_bob_did = canonicalize_did(&bob_did).unwrap();
        let bob_person = l1_market.people.iter().find(|p| p.did == can_bob_did);
        assert!(bob_person.is_some(), "Bob should be in L1 market view");
        let has_sync_source = bob_person.unwrap().sources.iter().any(|s| {
            matches!(s, crate::models::FrontendEntrySource::Sync { read_url } if *read_url == finalize_status.descriptor.read_url)
        });
        assert!(has_sync_source, "Bob's market person should have Drop entry source!");

        // 7. Unauthorized reader L2 gets 0 decrypted slots
        {
            let mut s = l2_state.service.lock().unwrap();
            let mut settings = l2_state.get_cached_settings().unwrap_or_default();
            settings.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            l2_state.save_settings(&mut s, settings, Some(password)).unwrap();
        }

        let l2_sync = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &l2_state,
        )
        .await
        .unwrap();
        assert_eq!(l2_sync.decrypted_slots, 0, "Unauthorized reader L2 should decrypt 0 slots");
        assert_eq!(l2_sync.imported_cards, 0, "Unauthorized reader L2 should import 0 cards");

        // 8. Admin revoking Bob or blocking Bob's stealth pubkey removes Bob from members and purges slot
        let bob_stealth_pk_a = {
            let mut s = alice_state.service.lock().unwrap();
            let seed = get_wallet_master_seed(&mut s).unwrap();
            let (admin_sk, _, _) = humoco_market::derive_drop_admin_keypair(&seed, 0);
            let bob_vk = human_money_core::crypto::get_pubkey_from_user_id(&bob_did).unwrap();
            humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &bob_vk).unwrap()
        };

        let block_status = block_sync_slot_impl(
            bob_stealth_pk_a.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        assert!(block_status.blocked_pubkeys.contains(&bob_stealth_pk_a));
        assert!(!block_status.active_slot_ids.contains(&bob_stealth_pk_a));
        assert!(!server_a.members.lock().unwrap().contains(&bob_stealth_pk_a));
        assert!(!server_a.slots.lock().unwrap().contains_key(&format!("{}.card", bob_stealth_pk_a)), "Slot file should be purged on Server A");
        assert!(!server_a.slots.lock().unwrap().contains_key(&format!("{}_pc.card", bob_stealth_pk_a)), "Prefixed slot file should be purged on Server A");
        assert!(!server_a.slots.lock().unwrap().contains_key(&format!("{}.wot", bob_stealth_pk_a)), "WoT slot file should be purged on Server A");
        assert!(!server_a.slots.lock().unwrap().contains_key(&format!("{}_pc.wot", bob_stealth_pk_a)), "Prefixed WoT slot file should be purged on Server A");

        // 9. Check overview status
        let overview = get_sync_overview_status_impl(
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(overview.has_own_sync);
        assert_eq!(overview.connected_count, 1);
        assert_eq!(overview.pending_count, 0);
    }

    #[test]
    fn test_get_sync_overview_status_pending_vs_connected() {
        let base_dir = temp_test_dir("overview_status");
        let password = "TestPassword123!";
        let service = setup_test_app(&base_dir.join("user"), "User", ALICE_PHRASE, password);
        let state = create_test_state(service);

        // 1. Initial state: Disconnected
        let initial = get_sync_overview_status_impl(Some(password.to_string()), &state).unwrap();
        assert_eq!(initial.connected_count, 0);
        assert_eq!(initial.pending_count, 0);
        assert_eq!(initial.public_syncs, 0);

        // 2. Add public drop with empty authorized_channels (pending / unauthorized)
        {
            let mut s = state.service.lock().unwrap();
            let mut settings = state.load_settings(&mut s, Some(password)).unwrap();
            settings.public_syncs.push(crate::settings::StoredPublicSync {
                write_url: "https://sync.example.com/sync.php".to_string(),
                read_url: "https://sync.example.com/data".to_string(),
                drop_pub: "a".repeat(64),
                register_url: None,
                access_pubkey: "b".repeat(64),
                enabled: false,
                authorized_channels: vec![],
                ..Default::default()
            });
            state.save_settings(&mut s, settings, Some(password)).unwrap();
        }

        let pending_overview = get_sync_overview_status_impl(Some(password.to_string()), &state).unwrap();
        assert_eq!(pending_overview.connected_count, 0);
        assert_eq!(pending_overview.pending_count, 1);
        assert_eq!(pending_overview.public_syncs, 0);

        // 3. Mark public drop as authorized and enabled
        {
            let mut s = state.service.lock().unwrap();
            let mut settings = state.load_settings(&mut s, Some(password)).unwrap();
            settings.public_syncs[0].enabled = true;
            settings.public_syncs[0].authorized_channels = vec!["card".to_string(), "wot".to_string()];
            state.save_settings(&mut s, settings, Some(password)).unwrap();
        }

        let connected_overview = get_sync_overview_status_impl(Some(password.to_string()), &state).unwrap();
        assert_eq!(connected_overview.connected_count, 1);
        assert_eq!(connected_overview.pending_count, 0);
        assert_eq!(connected_overview.public_syncs, 1);
    }

    #[tokio::test]
    async fn test_probe_contact_syncs_lifecycle_and_auto_activation() {
        let base_dir = temp_test_dir("probe_lifecycle");
        let password = "TestPassword123!";
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_state = create_test_state(bob_service);

        let server_admin_pub = "11".repeat(32);
        let server = MockSyncServer::start("alice_data".to_string(), server_admin_pub.clone()).await;

        // 1. Initially Bob has no syncs in settings
        {
            let mut s = bob_state.service.lock().unwrap();
            let settings = bob_state.load_settings(&mut s, Some(password)).unwrap();
            assert!(settings.market_selected_syncs.is_empty());
        }

        // 2. Add Alice contact with unverified sync
        let alice_did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let unverified_sync = FrontendSyncDescriptor {
            write_url: format!("{}/sync.php", server.base_url),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let alice_contact = FrontendContact {
            did: alice_did.to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![unverified_sync],
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state.save_contact(&mut s, alice_contact, Some(password)).unwrap();
        }

        // 3. First probe: Bob not in server members -> becomes pending_auth, NOT auto-activated
        let probed = probe_contact_syncs_impl(false, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(probed.len(), 1);
        assert_eq!(probed[0].status, "pending_auth");
        assert_eq!(probed[0].drop_pub, server_admin_pub);
        assert!(probed[0].last_probed_at.is_some());

        // Verify settings still empty (no auto-activation for pending_auth)
        {
            let mut s = bob_state.service.lock().unwrap();
            let settings = bob_state.load_settings(&mut s, Some(password)).unwrap();
            assert!(settings.market_selected_syncs.is_empty());
        }

        // 4. Backoff check: immediate re-probe with force=false skips pending_auth
        let initial_probed_at = probed[0].last_probed_at;
        let reprobed = probe_contact_syncs_impl(false, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(reprobed[0].status, "pending_auth");
        assert_eq!(reprobed[0].last_probed_at, initial_probed_at);

        // 5. Authorize Bob on the mock drop server
        let bob_signing_key = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet_and_identity(|_w, id| id.signing_key.clone()).unwrap()
        };
        let stealth_pk = humoco_market::derive_member_stealth_pubkey(&bob_signing_key, &server_admin_pub).unwrap();
        server.members.lock().unwrap().push(stealth_pk);

        // 6. Force probe: Bob is now authorized -> becomes active and auto-activates into selected syncs
        let active_probed = probe_contact_syncs_impl(true, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(active_probed.len(), 1);
        assert_eq!(active_probed[0].status, "active");

        {
            let mut s = bob_state.service.lock().unwrap();
            let settings = bob_state.load_settings(&mut s, Some(password)).unwrap();
            assert_eq!(settings.market_selected_syncs.len(), 1);
            assert_eq!(settings.market_selected_syncs[0].status, "active");
            assert_eq!(settings.market_selected_syncs[0].drop_pub, server_admin_pub);
        }

        // 7. Test unreachable handling with invalid endpoint
        let charlie_service = setup_test_app(&base_dir.join("charlie"), "Charlie", CHARLIE_PHRASE, password);
        let charlie_did = charlie_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let unreachable_sync = FrontendSyncDescriptor {
            write_url: "http://127.0.0.1:9/sync.php".to_string(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };
        let charlie_contact = FrontendContact {
            did: charlie_did,
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![unreachable_sync],
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state.save_contact(&mut s, charlie_contact, Some(password)).unwrap();
        }

        let all_probed = probe_contact_syncs_impl(true, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        let unreach = all_probed.iter().find(|s| s.write_url.contains(":9/sync.php")).unwrap();
        assert_eq!(unreach.status, "unreachable");
        assert_eq!(unreach.fail_count, 1);
        assert!(unreach.last_probed_at.is_some());
    }

    #[tokio::test]
    async fn test_probe_contact_syncs_healthy_sync_probes_unverified_once_without_reprobing_pending() {
        let base_dir = temp_test_dir("healthy_probe_unverified");
        let password = "TestPassword123!";
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_state = create_test_state(bob_service);

        let server_admin_pub = "22".repeat(32);
        let server = MockSyncServer::start("server_data".to_string(), server_admin_pub.clone()).await;

        // 1. Give Bob an active, healthy friend sync in settings
        let healthy_sync = FrontendSyncDescriptor {
            write_url: "https://healthy.example.com/sync.php".to_string(),
            read_url: "https://healthy.example.com/read".to_string(),
            slot_id: None,
            drop_pub: "healthy_drop_pub".to_string(),
            last_seen_at: Some(chrono::Utc::now().timestamp() as u64),
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: Some(chrono::Utc::now().timestamp() as u64),
            ..Default::default()
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut settings = bob_state.load_settings(&mut s, Some(password)).unwrap();
            settings.market_selected_syncs.push(healthy_sync);
            bob_state.save_settings(&mut s, settings, Some(password)).unwrap();
        }

        // 2. Add Alice contact with an already-probed "pending_auth" sync
        let alice_did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let pending_sync = FrontendSyncDescriptor {
            write_url: "http://127.0.0.1:9/pending.php".to_string(),
            read_url: "http://127.0.0.1:9/pending_read".to_string(),
            slot_id: None,
            drop_pub: "pending_drop_pub".to_string(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "pending_auth".to_string(),
            last_probed_at: Some(1000),
            ..Default::default()
        };

        // 3. Add Charlie contact with a newly added "unverified" sync (last_probed_at == None, read_url == "")
        let charlie_service = setup_test_app(&base_dir.join("charlie"), "Charlie", CHARLIE_PHRASE, password);
        let charlie_did = charlie_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let unverified_sync = FrontendSyncDescriptor {
            write_url: format!("{}/sync.php", server.base_url),
            read_url: String::new(),
            slot_id: None,
            drop_pub: String::new(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        let alice_contact = FrontendContact {
            did: alice_did.to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![pending_sync],
        };
        let charlie_contact = FrontendContact {
            did: charlie_did.to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(1),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![unverified_sync],
        };
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state.save_contact(&mut s, alice_contact, Some(password)).unwrap();
            bob_state.save_contact(&mut s, charlie_contact, Some(password)).unwrap();
        }

        // 4. Probe with force=false.
        // Even though Bob already has a healthy sync, Charlie's unverified sync MUST be probed once!
        // Meanwhile, Alice's already-probed pending_auth sync must NOT be re-probed (retaining last_probed_at == 1000).
        let probed = probe_contact_syncs_impl(false, Some(password.to_string()), &bob_state)
            .await
            .unwrap();

        let alice_probed = probed.iter().find(|s| s.write_url.contains("pending.php")).unwrap();
        assert_eq!(alice_probed.status, "pending_auth");
        assert_eq!(alice_probed.last_probed_at, Some(1000), "Already probed pending sync should not be re-probed when has_healthy_sync");

        let charlie_probed = probed.iter().find(|s| s.write_url.contains(&server.base_url)).unwrap();
        assert_ne!(charlie_probed.status, "unverified", "Newly added unverified sync must be probed");
        assert_eq!(charlie_probed.drop_pub, server_admin_pub, "Drop pub must be populated from probe info");
        assert!(!charlie_probed.read_url.is_empty(), "Read URL must be populated from probe info");
        assert!(charlie_probed.last_probed_at.is_some(), "last_probed_at must be populated");
    }

    #[test]
    fn test_preview_contact_trust_change() {
        let base_dir = temp_test_dir("trust_preview");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let alice_state = create_test_state(alice_service);

        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        // Prepare drop setup on Alice with direct_min 2
        let _draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        // 1. Initially Bob has no trust level (0)
        let preview_upgrade = preview_contact_trust_change_impl(
            bob_did.clone(),
            2,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(!preview_upgrade.can_read_my_market_before);
        assert!(preview_upgrade.can_read_my_market_after);
        assert!(!preview_upgrade.can_write_my_sync_before);
        assert!(preview_upgrade.can_write_my_sync_after);
        assert!(!preview_upgrade.can_send_me_post_before);
        assert!(preview_upgrade.can_send_me_post_after);

        // 2. Save Bob with level 2
        {
            let mut s = alice_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            alice_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        // Preview downgrade to level 0
        let preview_downgrade = preview_contact_trust_change_impl(
            bob_did.clone(),
            0,
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(preview_downgrade.can_read_my_market_before);
        assert!(!preview_downgrade.can_read_my_market_after);
        assert!(preview_downgrade.can_read_my_wot_before);
        assert!(!preview_downgrade.can_read_my_wot_after);
        assert!(preview_downgrade.can_write_my_sync_before);
        assert!(!preview_downgrade.can_write_my_sync_after);
        assert!(preview_downgrade.can_send_me_post_before);
        assert!(!preview_downgrade.can_send_me_post_after);
    }

    #[tokio::test]
    async fn test_wot_channel_sync_flow_and_idempotency() {
        let base_dir = temp_test_dir("wot_sync_flow");
        let password = "TestPassword123!";

        // 1. Setup actors: Alice (Hoster/Admin), Bob (Direct Friend, Level 2), Carol (Hop-2 from Alice via Bob, Level 2), David (Outsider)
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let carol_service = setup_test_app(&base_dir.join("carol"), "Carol", CHARLIE_PHRASE, password);
        let david_service = setup_test_app(&base_dir.join("david"), "David", READER1_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);
        let carol_state = create_test_state(carol_service);
        let david_state = create_test_state(david_service);

        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let carol_did = {
            let s = carol_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // 2. Alice sets up her drop
        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let server = MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Configure Alice's drop as target drop for Alice, Bob, and Carol
        {
            let mut s = alice_state.service.lock().unwrap();
            let mut set = alice_state.load_settings(&mut s, Some(password)).unwrap();
            set.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            alice_state.save_settings(&mut s, set, Some(password)).unwrap();
        }
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut set = bob_state.load_settings(&mut s, Some(password)).unwrap();
            set.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            bob_state.save_settings(&mut s, set, Some(password)).unwrap();
        }
        {
            let mut s = carol_state.service.lock().unwrap();
            let mut set = carol_state.load_settings(&mut s, Some(password)).unwrap();
            set.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            carol_state.save_settings(&mut s, set, Some(password)).unwrap();
        }

        // 3. Establish Trust Network:
        // Alice rates Bob as Level 2 (Direct, class 1)
        {
            let mut s = alice_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            alice_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        // Bob rates Alice as Level 2 (so Bob recognizes Alice) and Carol as Level 2
        {
            let mut s = bob_state.service.lock().unwrap();
            let alice_contact = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            bob_state.save_contact(&mut s, alice_contact, Some(password)).unwrap();

            let carol_contact = FrontendContact {
                did: carol_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Carol".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            bob_state.save_contact(&mut s, carol_contact, Some(password)).unwrap();
        }

        // Carol rates Bob as Level 2 and Alice as Level 1
        {
            let mut s = carol_state.service.lock().unwrap();
            let bob_contact = FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Bob".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            carol_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();

            let alice_contact = FrontendContact {
                did: alice_did.clone(),
                profile: FrontendUserProfile {
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: "2026-01-01T00:00:00Z".to_string(),
                notes: None,
                trust_level: Some(1),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            };
            carol_state.save_contact(&mut s, alice_contact, Some(password)).unwrap();
        }

        // Alice learns Bob's list so Carol is in Alice's graph as reachable Hop-2:
        {
            let can_alice = canonicalize_did(&alice_did).unwrap();
            let can_bob = canonicalize_did(&bob_did).unwrap();

            let mut bob_s = bob_state.service.lock().unwrap();
            let mut bob_reg = bob_state.get_or_load_wot_register(&mut bob_s, Some(password)).unwrap();
            bob_state.ensure_wot_list_signed(&mut bob_s, &mut bob_reg).unwrap();
            let bob_sync_pkg = bob_reg.prepare_sync_package(Some(&can_alice), None, true).unwrap().0;

            let mut alice_s = alice_state.service.lock().unwrap();
            let mut alice_reg = alice_state.get_or_load_wot_register(&mut alice_s, Some(password)).unwrap();
            let now = sync_client::current_unix_timestamp();
            let stats = alice_reg.import_sync_package(&bob_sync_pkg, Some(&can_bob), now).unwrap();
            assert!(stats.accepted_total() >= 1);
            alice_state.save_wot_register(&mut alice_s, &alice_reg, Some(password)).unwrap();
        }

        // Verify Alice's authorized drop readers: Bob (class 1) and Carol (class 2)
        {
            let mut alice_s = alice_state.service.lock().unwrap();
            let alice_reg = alice_state.get_or_load_wot_register(&mut alice_s, Some(password)).unwrap();
            let readers = alice_reg.authorized_drop_readers();
            let can_bob = canonicalize_did(&bob_did).unwrap();
            let can_carol = canonicalize_did(&carol_did).unwrap();
            assert!(readers.iter().any(|(d, c)| d == &can_bob && *c == 1), "Bob must be in class 1");
            assert!(readers.iter().any(|(d, c)| d == &can_carol && *c == 2), "Carol must be in class 2");
        }

        // 4. Alice Syncs (Uploads "wot" channel to drop server)
        let alice_sync_res = sync_market_channels_impl(
            None,
            Some(true),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert!(alice_sync_res.uploaded_to >= 1, "Alice must upload to drop server");
        assert!(alice_sync_res.errors.is_empty(), "Alice sync should have 0 errors: {:?}", alice_sync_res.errors);

        // Verify slot exists on mock drop server and has multi-class magic
        let slot_keys: Vec<String> = server.slots.lock().unwrap().keys().cloned().collect();
        let wot_slot_key = slot_keys.iter().find(|k| k.ends_with(".wot")).expect("Wot slot must exist on server");
        let wot_slot_bytes = server.slots.lock().unwrap().get(wot_slot_key).unwrap().clone();
        assert_eq!(&wot_slot_bytes[0..8], humoco_market::SLOT_MULTI_MAGIC);

        // 5. Bob (Class 1) Syncs (Downloads Alice's list)
        let bob_sync_res = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert!(bob_sync_res.imported_wot_lists >= 1, "Bob must import Alice's wot list: {:?}", bob_sync_res);
        assert!(bob_sync_res.errors.is_empty(), "Bob sync should have 0 errors: {:?}", bob_sync_res.errors);

        // Verify Bob's register now has Alice's list
        {
            let mut bob_s = bob_state.service.lock().unwrap();
            let bob_reg = bob_state.get_or_load_wot_register(&mut bob_s, Some(password)).unwrap();
            let can_alice = canonicalize_did(&alice_did).unwrap();
            assert!(bob_reg.raw_list_store.contains_key(&can_alice), "Bob must have Alice's raw list");
        }

        // Bob second sync before any new uploads -> strict idempotency (0 new imports)
        let bob_idempotent_sync = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(bob_idempotent_sync.imported_wot_lists, 0, "Bob second sync before new uploads must be 0 imports");

        // 6. Carol (Class 2) Syncs (Downloads Alice's list with Selective Disclosure)
        let carol_sync_res = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();
        assert!(carol_sync_res.imported_wot_lists >= 1, "Carol must import Alice's wot list");
        assert!(carol_sync_res.errors.is_empty(), "Carol sync should have 0 errors: {:?}", carol_sync_res.errors);

        // Verify Carol's register now has Alice's list
        {
            let mut carol_s = carol_state.service.lock().unwrap();
            let carol_reg = carol_state.get_or_load_wot_register(&mut carol_s, Some(password)).unwrap();
            let can_alice = canonicalize_did(&alice_did).unwrap();
            assert!(carol_reg.raw_list_store.contains_key(&can_alice), "Carol must have Alice's raw list");
        }

        // Carol second sync -> strict idempotency (0 new imports)
        let carol_idempotent_sync = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();
        assert_eq!(carol_idempotent_sync.imported_wot_lists, 0, "Carol second sync must be 0 imports");

        // 7. Alice second sync -> no new upload needed
        let alice_second_sync = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(alice_second_sync.uploaded_to, 0, "Alice second sync without changes must be no-op upload");

        // Bob now syncs to pick up Carol's newly uploaded list
        let bob_third_sync = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(bob_third_sync.imported_wot_lists, 1, "Bob should import Carol's list");

        // Bob fourth sync -> again fully idempotent
        let bob_fourth_sync = sync_market_channels_impl(
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(bob_fourth_sync.imported_wot_lists, 0, "Bob fourth sync must be 0 imports");

        // 8. Outsider (David) cannot decrypt Alice's wot slot
        {
            let david_s = david_state.service.lock().unwrap();
            let david_sk = david_s.with_wallet_and_identity(|_w, id| id.signing_key.clone()).unwrap();
            let david_x_sk = human_money_core::crypto::ed25519_sk_to_x25519_sk(&david_sk);
            let decrypted = humoco_market::open_multi_class_slot_blob(&wot_slot_bytes, &david_x_sk).unwrap();
            assert_eq!(decrypted, None, "Outsider cannot decrypt any subpackage in multi-class slot blob");
        }

        // 9. Slot Deletion
        let deleted = delete_own_sync_slots_impl(Some(password.to_string()), &alice_state)
            .await
            .unwrap();
        assert!(deleted >= 1, "Must delete slots");

        let alice_sk = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet_and_identity(|_w, id| id.signing_key.clone()).unwrap()
        };
        let alice_stealth = humoco_market::derive_member_stealth_pubkey(
            &alice_sk,
            &finalize_status.descriptor.drop_pub,
        )
        .unwrap();

        let remaining_slots = server.slots.lock().unwrap().clone();
        assert!(
            !remaining_slots.contains_key(&format!("{}.wot", alice_stealth))
                && !remaining_slots.contains_key(&format!("{}_pc.wot", alice_stealth)),
            "Alice's WOT slot must be deleted from server"
        );
        assert!(
            !remaining_slots.contains_key(&format!("{}.card", alice_stealth))
                && !remaining_slots.contains_key(&format!("{}_pc.card", alice_stealth)),
            "Alice's card slot must be deleted from server"
        );

        // Verify sync_sent_log is cleared in Alice's register
        {
            let mut alice_s = alice_state.service.lock().unwrap();
            let alice_reg = alice_state.get_or_load_wot_register(&mut alice_s, Some(password)).unwrap();
            assert!(alice_reg.drop_sent_log.is_empty(), "Alice sync_sent_log must be cleared");
        }
    }

    #[tokio::test]
    async fn test_push_channel_members_hash_check_and_trust_change() {
        let base_dir = temp_test_dir("members_hash_check");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // 1. Prepare drop setup and finalize
        let draft = prepare_sync_setup_impl(
            Some(FrontendMemberPolicyDto {
                direct_min: 2,
                hop2_enabled: true,
                bridge_min: 2,
                hop2_min: 2,
            }),
            Some(300),
            Some(256),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let server = MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        assert_eq!(finalize_status.data_dir, draft.data_dir);

        // Finalize pushes the 4 channels
        let initial_puts = *server.put_members_count.lock().unwrap();
        assert_eq!(initial_puts, 4, "Initial finalize should push all 4 channels");

        // 2. Call sync_channel_members_impl immediately without any changes
        let pushed_again = sync_channel_members_impl(Some(password), &alice_state).await.unwrap();
        assert_eq!(pushed_again, 0, "No channels should be pushed when members hash has not changed");
        assert_eq!(*server.put_members_count.lock().unwrap(), initial_puts, "Server should receive zero new PUT requests");

        // 3. Add Bob as a trusted contact (Level 2)
        let bob_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".into(),
            notes: None,
            trust_level: Some(2),
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };

        {
            let mut service = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut service, bob_contact.clone(), Some(password)).unwrap();
        }

        // 4. Call sync_channel_members_impl after trust level update
        let pushed_after_add = sync_channel_members_impl(Some(password), &alice_state).await.unwrap();
        assert!(pushed_after_add > 0, "Channels with modified members should be pushed");
        let puts_after_add = *server.put_members_count.lock().unwrap();
        assert!(puts_after_add > initial_puts, "Server should have received new PUT requests");

        // 5. Calling again immediately should skip
        let pushed_noop = sync_channel_members_impl(Some(password), &alice_state).await.unwrap();
        assert_eq!(pushed_noop, 0, "Subsequent call without changes must be skipped");
        assert_eq!(*server.put_members_count.lock().unwrap(), puts_after_add);

        // 6. Change Bob's trust level from 2 to 1 (below direct_min=2 for drop access)
        let mut bob_untrusted = bob_contact.clone();
        bob_untrusted.trust_level = Some(1);
        {
            let mut service = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut service, bob_untrusted, Some(password)).unwrap();
        }

        let pushed_after_downgrade = sync_channel_members_impl(Some(password), &alice_state).await.unwrap();
        assert!(pushed_after_downgrade > 0, "Trust change must trigger channel member re-upload");
        let puts_after_downgrade = *server.put_members_count.lock().unwrap();
        assert!(puts_after_downgrade > puts_after_add, "Server must receive updated members PUT");
    }

    fn build_signed_test_card_envelope(
        seed_byte: u8,
        v: u64,
        valid_until: u64,
        item_count: usize,
        hops: u8,
    ) -> humoco_market::CardEnvelope {
        use ed25519_dalek::SigningKey;
        use rand::rngs::StdRng;
        use rand::{RngCore, SeedableRng};

        let sk = SigningKey::from_bytes(&[seed_byte; 32]);
        let did = human_money_core::crypto::create_user_id(&sk.verifying_key(), Some("tx")).unwrap();
        let mut rng = StdRng::seed_from_u64((seed_byte as u64) * 1000 + v);
        let mut items = Vec::with_capacity(item_count + 1);
        items.push((
            humoco_market::CardItem::Name {
                first_name: Some(format!("Maker-{}", seed_byte)),
                last_name: Some(format!("V{}", v)),
            },
            1,
        ));
        for idx in 0..item_count {
            let mut entropy = [0u8; 24];
            rng.fill_bytes(&mut entropy);
            let title = format!("Craft-{}-{}", idx, hex::encode(&entropy[..6]));
            let desc = format!("Local work batch {}", hex::encode(&entropy[6..24]));
            items.push((
                humoco_market::CardItem::Skill(humoco_market::Skill {
                    title,
                    text: Some(desc),
                    pricing: Some("10 M/h".to_string()),
                    is_remote: idx % 2 == 0,
                }),
                1,
            ));
        }

        let card = humoco_market::sign_sd_card(
            humoco_market::CardPayloadInput {
                did,
                v,
                valid_until,
                max_hops: 3,
                ad_count: 0,
                allow_inquiries: true,
                drops: vec![],
            },
            items,
            &sk,
        )
        .unwrap();
        let wire = humoco_market::render_wire(&card, hops, humoco_market::WireFilter::Full).unwrap();
        humoco_market::CardEnvelope {
            card_wire: wire,
            hops,
        }
    }

    #[tokio::test]
    async fn test_base_and_delta_card_bundle_and_compaction_on_new_reader() {
        let base_dir = temp_test_dir("base_delta_compaction");
        let password = "TestPassword123!";

        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
        let r1_service = setup_test_app(&base_dir.join("r1"), "Reader1", READER1_PHRASE, password);
        let r2_service = setup_test_app(&base_dir.join("r2"), "Reader2", READER2_PHRASE, password);

        let bob_state = create_test_state(bob_service);
        let r1_state = create_test_state(r1_service);
        let r2_state = create_test_state(r2_service);

        let r1_did = r1_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let r2_did = r2_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let draft = prepare_sync_setup_impl(None, None, None, Some(password.to_string()), &bob_state).unwrap();
        let server = MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let finalize_status = finalize_sync_setup_impl(server.base_url.clone(), Some(password.to_string()), &bob_state)
            .await
            .unwrap();

        set_selected_upload_syncs_impl(
            vec![finalize_status.descriptor.clone()],
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();

        // Bob trusts Reader 1 initially and disables WoT share so we assert pure card upload counts cleanly
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut settings = bob_state.get_cached_settings().unwrap_or_default();
            settings.wot_share_via_sync = false;
            bob_state.save_settings(&mut s, settings, Some(password)).unwrap();
            bob_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: r1_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Reader1".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".into(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![],
                    },
                    Some(password),
                )
                .unwrap();
        }
        sync_channel_members_impl(Some(password), &bob_state).await.unwrap();

        // Populate Bob's CardStore with 18 rich cards (~8-10 KB compressed Base)
        let now_secs = sync_client::current_unix_timestamp();
        let valid_until = now_secs + 365 * 86_400;
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut store = humoco_market::CardStore::default();
            for seed in 20u8..38u8 {
                let env = build_signed_test_card_envelope(seed, now_secs, valid_until, 5, 1);
                humoco_market::accept_envelope(&mut store, &env, now_secs, 2000, None, &|_| false);
            }
            let bytes = serde_json::to_vec(&store).unwrap();
            s.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, Some(password))
                .unwrap();
        }

        // 1. Cold upload creates Base card slot (no delta yet)
        let first_up = sync_market_channels_impl(None, Some(true), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(first_up.uploaded_to, 1);

        let keys_after_base: Vec<String> = server.slots.lock().unwrap().keys().cloned().collect();
        assert_eq!(keys_after_base.len(), 1);
        let base_key = keys_after_base[0].clone();
        assert!(!base_key.contains("delta"));
        let base_bytes_before = server.slots.lock().unwrap().get(&base_key).unwrap().clone();
        assert!(base_bytes_before.len() > 4096, "Base should be large enough for <25% delta");

        // 2. Update 1 small card -> Delta upload ONLY (Base bytes untouched!)
        let updated_v = now_secs + 120;
        let updated_env = build_signed_test_card_envelope(20, updated_v, valid_until, 0, 1);
        let updated_did = humoco_market::verify_sd_card(&updated_env.card_wire).unwrap().payload.did;
        {
            let mut s = bob_state.service.lock().unwrap();
            let raw = s.load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password)).unwrap();
            let mut store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            humoco_market::accept_envelope(&mut store, &updated_env, updated_v, 2000, None, &|_| false);
            let bytes = serde_json::to_vec(&store).unwrap();
            s.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, Some(password))
                .unwrap();
        }

        let delta_up = sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(delta_up.uploaded_to, 1);
        let keys_after_delta: Vec<String> = server.slots.lock().unwrap().keys().cloned().collect();
        assert_eq!(keys_after_delta.len(), 2, "Server must hold Base + Delta slots");
        assert!(keys_after_delta.iter().any(|k| k.contains("-delta.card")));
        assert_eq!(
            server.slots.lock().unwrap().get(&base_key).unwrap().as_slice(),
            base_bytes_before.as_slice(),
            "Base slot must stay unchanged on Delta upload"
        );

        // 3. Reader 1 syncs and gets both Base and Delta (including updated_v), and subsequent background poll hits 20m cooldown
        {
            let mut s = r1_state.service.lock().unwrap();
            let mut settings = r1_state.get_cached_settings().unwrap_or_default();
            settings.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            r1_state.save_settings(&mut s, settings, Some(password)).unwrap();
        }
        let r1_res = sync_market_channels_impl(None, None, Some(password.to_string()), &r1_state)
            .await
            .unwrap();
        assert_eq!(r1_res.decrypted_slots, 2, "Reader 1 must decrypt Base and Delta slots");
        assert_eq!(r1_res.imported_cards, 19);
        {
            let mut s = r1_state.service.lock().unwrap();
            let raw = s.load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password)).unwrap();
            let r1_store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            assert_eq!(r1_store.cards.len(), 18);
            let imported = r1_store.cards.values().find(|c| c.card.payload.did == updated_did).unwrap();
            assert_eq!(imported.card.payload.v, updated_v);
        }

        let r1_cooldown = sync_market_channels_with_options(
            None,
            None,
            Some(true),
            Some(password.to_string()),
            &r1_state,
        )
        .await
        .unwrap();
        assert_eq!(r1_cooldown.polled_syncs, 0, "Background poll inside 20m cooldown must skip download polling");

        // 4. Bob authorizes Reader 2 -> readers_hash changes -> Compaction (new Base + DELETE Delta)
        {
            let mut s = bob_state.service.lock().unwrap();
            bob_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: r2_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Reader2".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".into(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![],
                    },
                    Some(password),
                )
                .unwrap();
        }
        sync_channel_members_impl(Some(password), &bob_state).await.unwrap();

        let compact_up = sync_market_channels_impl(None, Some(true), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(compact_up.uploaded_to, 1);
        let keys_after_compact: Vec<String> = server.slots.lock().unwrap().keys().cloned().collect();
        assert_eq!(
            keys_after_compact.len(),
            1,
            "Compaction must delete Delta slot after Base PUT (keys={:?}, errors={:?})",
            keys_after_compact,
            compact_up.errors
        );
        assert!(!keys_after_compact[0].contains("delta"));

        // Reader 2 decrypts consolidated Base and receives all 18 cards incl. updated_v
        {
            let mut s = r2_state.service.lock().unwrap();
            let mut settings = r2_state.get_cached_settings().unwrap_or_default();
            settings.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            r2_state.save_settings(&mut s, settings, Some(password)).unwrap();
        }
        let r2_res = sync_market_channels_impl(None, None, Some(password.to_string()), &r2_state)
            .await
            .unwrap();
        assert_eq!(r2_res.decrypted_slots, 1);
        assert_eq!(r2_res.imported_cards, 18);
        {
            let mut s = r2_state.service.lock().unwrap();
            let raw = s.load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password)).unwrap();
            let r2_store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            let imported = r2_store.cards.values().find(|c| c.card.payload.did == updated_did).unwrap();
            assert_eq!(imported.card.payload.v, updated_v);
        }
    }

    #[tokio::test]
    async fn test_quiet_friend_without_own_card_forwards_stored_cards() {
        let base_dir = temp_test_dir("quiet_friend_forward");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let bob_did = bob_state.service.lock().unwrap().with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let draft = prepare_sync_setup_impl(None, None, None, Some(password.to_string()), &alice_state).unwrap();
        let server = MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let finalize_status = finalize_sync_setup_impl(server.base_url.clone(), Some(password.to_string()), &alice_state)
            .await
            .unwrap();

        set_selected_upload_syncs_impl(
            vec![finalize_status.descriptor.clone()],
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        // Alice trusts Bob (Level 2), has NO published own market profile, and holds Carol's stored card
        let now_secs = sync_client::current_unix_timestamp();
        let third_party_env = build_signed_test_card_envelope(77, now_secs, now_secs + 365 * 86_400, 1, 1);
        let third_party_did = humoco_market::verify_sd_card(&third_party_env.card_wire)
            .unwrap()
            .payload
            .did;

        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_contact(
                    &mut s,
                    FrontendContact {
                        did: bob_did.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Bob".to_string()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".into(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(3),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![],
                    },
                    Some(password),
                )
                .unwrap();

            let mut store = humoco_market::CardStore::default();
            humoco_market::accept_envelope(&mut store, &third_party_env, now_secs, 2000, None, &|_| false);
            let bytes = serde_json::to_vec(&store).unwrap();
            s.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, Some(password))
                .unwrap();
        }
        sync_channel_members_impl(Some(password), &alice_state).await.unwrap();

        // Quiet Alice uploads stored third-party card even without own card
        let alice_sync = sync_market_channels_impl(None, Some(true), Some(password.to_string()), &alice_state)
            .await
            .unwrap();
        assert!(alice_sync.uploaded_to >= 1, "Quiet friend must still upload forwarded card bundle");

        // Bob syncs and receives the forwarded card with hops == 2
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut settings = bob_state.get_cached_settings().unwrap_or_default();
            settings.market_selected_syncs = vec![finalize_status.descriptor.clone()];
            bob_state.save_settings(&mut s, settings, Some(password)).unwrap();
        }
        let bob_sync = sync_market_channels_impl(None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert!(bob_sync.imported_cards >= 1, "Newcomer Bob must receive forwarded card from quiet friend");
        {
            let mut s = bob_state.service.lock().unwrap();
            let raw = s.load_encrypted_data(MARKET_CARD_STORE_KEY, Some(password)).unwrap();
            let bob_store: humoco_market::CardStore = serde_json::from_slice(&raw).unwrap();
            let imported = bob_store
                .cards
                .values()
                .find(|c| c.card.payload.did == third_party_did)
                .expect("Forwarded card must be in Bob's CardStore");
            assert_eq!(imported.hops, 2);
        }
    }

    #[tokio::test]
    async fn test_sync_admin_status_requires_finalized_live_server() {
        let base_dir = temp_test_dir("admin_status_finalized");
        let password = "pw";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let state = create_test_state(alice_service);

        // Before any setup, admin status is None
        assert!(
            get_sync_admin_status_impl(Some(password.to_string()), &state)
                .await
                .unwrap()
                .is_none()
        );

        // Preparing setup (copying config.toml / saving ZIP) must NOT mark service as active
        let draft = prepare_sync_setup_impl(None, None, None, Some(password.to_string()), &state)
            .unwrap();
        let _toml = get_rust_sync_config_impl(Some(password.to_string()), &state).unwrap();
        let zip_path = base_dir.join("humoco-sync-setup.zip");
        save_sync_setup_zip_impl(
            zip_path.to_string_lossy().to_string(),
            Some(password.to_string()),
            &state,
        )
        .unwrap();
        assert!(
            get_sync_admin_status_impl(Some(password.to_string()), &state)
                .await
                .unwrap()
                .is_none(),
            "Prepared draft with empty write_url must still return None"
        );

        // Finalizing against an unreachable server must fail and keep status as None
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_port = listener.local_addr().unwrap().port();
        drop(listener);
        let dead_url = format!("http://127.0.0.1:{}/sync.php", closed_port);

        let err = finalize_sync_setup_impl(dead_url, Some(password.to_string()), &state)
            .await
            .expect_err("Finalizing against unreachable server must fail");
        assert!(
            !err.message.is_empty(),
            "Unreachable server must return a clear error"
        );
        assert!(
            get_sync_admin_status_impl(Some(password.to_string()), &state)
                .await
                .unwrap()
                .is_none(),
            "Failed finalize must not persist active admin status"
        );

        // Finalizing against a live, valid server succeeds and activates admin status
        let server = MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
        let finalized = finalize_sync_setup_impl(
            server.base_url.clone(),
            Some(password.to_string()),
            &state,
        )
        .await
        .unwrap();
        let expected_write_url = format!("{}/sync.php", server.base_url);
        assert_eq!(finalized.descriptor.write_url, expected_write_url);

        let status = get_sync_admin_status_impl(Some(password.to_string()), &state)
            .await
            .unwrap()
            .expect("Finalized service must return Some(FrontendSyncAdminStatus)");
        assert_eq!(status.descriptor.write_url, expected_write_url);
    }
}
