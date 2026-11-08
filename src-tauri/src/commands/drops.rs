// src-tauri/src/commands/drops.rs
use std::collections::{BTreeMap, HashSet};
use base64::Engine;
use tauri::State;

use crate::drop_client::{self, IndexFetchResult, PutSlotOutcome};
use crate::models::{
    FrontendDiscoveredDrop, FrontendDropAdminStatus, FrontendDropAdminUpdateRequest,
    FrontendDropConnectionTestResult, FrontendDropDescriptor, FrontendDropOverviewStatus,
    FrontendDropSetupDraft, FrontendDropSyncResult, FrontendError, FrontendMemberPolicyDto,
    FrontendPublicDropInfo, FrontendTrustChangeImpact,
};
use crate::settings::{StoredPublicDrop, DROP_CARD_MAX_BYTES, DROP_HEARTBEAT_SECS, MAX_POLLED_DROPS};
use crate::state::{
    canonicalize_did, AppState, StoredDropAdmin, StoredDropSync, StoredOwnCard,
    MARKET_CARD_STORE_KEY, MARKET_DROP_ADMIN_KEY, MARKET_DROP_SYNC_KEY, MARKET_OWN_CARD_KEY,
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
pub fn compute_admin_drop_members_with_stats(
    state: &AppState,
    service: &mut AppService,
    admin: &StoredDropAdmin,
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
        humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, admin.generation);

    let now = drop_client::current_unix_timestamp();
    let is_grace_active = admin.previous_generation.is_some()
        && admin.rotation_retire_at.is_some_and(|retire| now < retire);
    let prev_admin_sk = if is_grace_active {
        admin.previous_generation.map(|prev_gen| {
            let (prev_sk, _, _) =
                humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, prev_gen);
            prev_sk
        })
    } else {
        None
    };

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

            if let Some(ref prev_sk) = prev_admin_sk {
                if let Ok(prev_stealth_pk) =
                    humoco_market::derive_member_stealth_pubkey_admin(prev_sk, &member_vk)
                {
                    if !blocked_set.contains(prev_stealth_pk.as_str()) {
                        member_pubkeys.push(prev_stealth_pk);
                    }
                }
            }
        }
    }

    member_pubkeys.sort();
    member_pubkeys.dedup();

    Ok((stats, member_pubkeys))
}

async fn push_all_channel_members(
    client: &reqwest::Client,
    state: &AppState,
    admin: &StoredDropAdmin,
    admin_sk: &ed25519_dalek::SigningKey,
    password: Option<&str>,
) -> Result<(), FrontendError> {
    if admin.descriptor.write_url.is_empty() {
        return Ok(());
    }
    let pks_by_channel = {
        let mut service = state.service.lock().unwrap();
        let mut list = Vec::new();
        for ch in ["card", "wot", "backup", "post"] {
            let (_stats, pks) =
                compute_admin_drop_members_with_stats(state, &mut service, admin, ch, password)?;
            list.push((ch, pks));
        }
        list
    };
    for (ch, pks) in pks_by_channel {
        drop_client::push_members(client, &admin.descriptor.write_url, admin_sk, ch, &pks)
            .await
            .map_err(FrontendError::from)?;
    }
    Ok(())
}

pub fn prepare_drop_setup_impl(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    max_total_storage_mb: Option<usize>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropSetupDraft, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let master_seed = get_wallet_master_seed(&mut service)?;

    let drop_index = 0u32;
    let generation = 0u32;
    let (_admin_sk, _admin_vk, admin_pubkey) =
        humoco_market::derive_drop_admin_keypair(&master_seed, drop_index, generation);
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

    let stored_admin = StoredDropAdmin {
        descriptor: humoco_market::DropDescriptor {
            write_url: String::new(),
            read_url: String::new(),
            slot_id: None,
            drop_pub: admin_pubkey.clone(),
        },
        drop_index,
        generation,
        previous_generation: None,
        rotation_retire_at: None,
        data_dir: data_dir.clone(),
        max_members: max_mem,
        max_total_storage_mb: max_storage_mb,
        register_url: None,
        policies,
        allow: Vec::new(),
        deny: Vec::new(),
        blocked_pubkeys: Vec::new(),
    };

    let bytes = serde_json::to_vec(&stored_admin)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredDropAdmin: {}", e)))?;
    service
        .save_encrypted_data(MARKET_DROP_ADMIN_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    let zip_bytes = crate::drop_zip::build_drop_zip_bytes(
        &admin_pubkey,
        &data_dir,
        max_mem,
        max_storage_mb,
        None,
    );
    let zip_base64 = base64::engine::general_purpose::STANDARD.encode(&zip_bytes);

    Ok(FrontendDropSetupDraft {
        descriptor: stored_admin.descriptor.into(),
        data_dir,
        admin_pubkey,
        generation,
        max_members: max_mem,
        max_total_storage_mb: max_storage_mb,
        policy: card_policy,
        zip_base64,
    })
}

#[tauri::command]
pub fn prepare_drop_setup(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    max_total_storage_mb: Option<usize>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropSetupDraft, FrontendError> {
    prepare_drop_setup_impl(policy, max_members, max_total_storage_mb, password, &state)
}

pub fn save_drop_setup_zip_impl(
    file_path: String,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();
    let admin: StoredDropAdmin = service
        .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
        .map_err(FrontendError::from)
        .and_then(|bytes| {
            serde_json::from_slice(&bytes).map_err(|e| {
                FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e))
            })
        })?;

    let zip_bytes = crate::drop_zip::build_drop_zip_bytes(
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
pub fn save_drop_setup_zip(
    file_path: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    save_drop_setup_zip_impl(file_path, password, &state)
}

pub async fn finalize_drop_setup_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    let base_url = humoco_market::canonicalize_write_url(&base_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    drop_client::validate_drop_url(&base_url).map_err(FrontendError::from)?;

    let (mut admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        let admin = serde_json::from_slice::<StoredDropAdmin>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e)))?;
        let master_seed = get_wallet_master_seed(&mut service)?;
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, admin.generation);
        (admin, admin_sk)
    };

    let trimmed = base_url.trim_end_matches('/');
    let write_url = if trimmed.ends_with("/drop.php") {
        trimmed.to_string()
    } else {
        format!("{}/drop.php", trimmed)
    };
    let read_base = write_url.strip_suffix("/drop.php").unwrap_or(trimmed);
    let read_url = format!("{}/{}", read_base, admin.data_dir);
    admin.descriptor.write_url = write_url.clone();
    admin.descriptor.read_url = read_url.clone();

    let client = drop_client::create_drop_http_client();

    // Push members to all 4 channels
    push_all_channel_members(
        &client,
        state,
        &admin,
        &admin_sk,
        password.as_deref(),
    )
    .await?;

    // Add own drop to settings.market_selected_drops if < 3
    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let desc_dto: FrontendDropDescriptor = admin.descriptor.clone().into();
        if settings.market_selected_drops.len() < 3
            && !settings
                .market_selected_drops
                .iter()
                .any(|d| d.read_url == desc_dto.read_url)
        {
            settings.market_selected_drops.push(desc_dto);
            let _ = state.save_settings(&mut service, settings, password.as_deref());
        }

        let admin_bytes = serde_json::to_vec(&admin).map_err(|e| {
            FrontendError::from(format!("Failed to serialize StoredDropAdmin: {}", e))
        })?;
        service
            .save_encrypted_data(MARKET_DROP_ADMIN_KEY, &admin_bytes, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    let (stats, active_slot_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_drop_members_with_stats(state, &mut service, &admin, "card", password.as_deref())?
    };

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(FrontendDropAdminStatus {
        descriptor: admin.descriptor.clone().into(),
        admin_pubkey: admin.descriptor.drop_pub.clone(),
        generation: admin.generation,
        rotation_retire_at: admin.rotation_retire_at,
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
    })
}

#[tauri::command]
pub async fn finalize_drop_setup(
    base_url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    finalize_drop_setup_impl(base_url, password, &state).await
}

pub async fn test_drop_connection_impl(
    base_url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropConnectionTestResult, FrontendError> {
    let base_url = base_url.trim().trim_end_matches('/').to_string();
    if let Err(e) = drop_client::validate_drop_url(&base_url) {
        return Ok(FrontendDropConnectionTestResult {
            success: false,
            message: e,
            write_ok: false,
            read_ok: false,
        });
    }

    let (admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let admin_res = service
            .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredDropAdmin>(&bytes).map_err(|e| {
                    FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e))
                })
            });

        let admin = match admin_res {
            Ok(a) => a,
            Err(e) => {
                return Ok(FrontendDropConnectionTestResult {
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
                return Ok(FrontendDropConnectionTestResult {
                    success: false,
                    message: format!("Failed to obtain wallet seed: {}", e.message),
                    write_ok: false,
                    read_ok: false,
                });
            }
        };

        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, admin.generation);
        (admin, admin_sk)
    };

    let client = drop_client::create_drop_http_client();
    let write_url = if base_url.ends_with("/drop.php") {
        base_url.clone()
    } else {
        format!("{}/drop.php", base_url)
    };

    // Test write via push_members
    let (stats, member_pubkeys) = {
        let mut service = state.service.lock().unwrap();
        match compute_admin_drop_members_with_stats(state, &mut service, &admin, "card", password.as_deref()) {
            Ok(res) => res,
            Err(e) => {
                return Ok(FrontendDropConnectionTestResult {
                    success: false,
                    message: format!("Failed to compute drop members: {}", e.message),
                    write_ok: false,
                    read_ok: false,
                });
            }
        }
    };

    if let Err(e) = drop_client::push_members(&client, &write_url, &admin_sk, "card", &member_pubkeys).await {
        return Ok(FrontendDropConnectionTestResult {
            success: false,
            message: format!("Write test failed: {}", e),
            write_ok: false,
            read_ok: false,
        });
    }

    // Test read via fetch_drop_info
    match drop_client::fetch_drop_info(&client, &write_url).await {
        Ok(info) => {
            Ok(FrontendDropConnectionTestResult {
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
            Ok(FrontendDropConnectionTestResult {
                success: false,
                message: format!("Read test failed: {}", e),
                write_ok: true,
                read_ok: false,
            })
        }
    }
}

#[tauri::command]
pub async fn test_drop_connection(
    base_url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropConnectionTestResult, FrontendError> {
    test_drop_connection_impl(base_url, password, &state).await
}

pub async fn get_drop_admin_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendDropAdminStatus>, FrontendError> {
    let (admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let bytes = match service.load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref()) {
            Ok(b) => b,
            Err(_) => return Ok(None),
        };
        let admin: StoredDropAdmin = match serde_json::from_slice(&bytes) {
            Ok(a) => a,
            Err(_) => return Ok(None),
        };
        let master_seed = match get_wallet_master_seed(&mut service) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, admin.generation);
        (admin, admin_sk)
    };

    let (stats, active_slot_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_drop_members_with_stats(state, &mut service, &admin, "card", password.as_deref())?
    };

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    let client = drop_client::create_drop_http_client();
    let remote_status = if !admin.descriptor.write_url.is_empty() {
        drop_client::fetch_admin_status(&client, &admin.descriptor.write_url, &admin_sk)
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

    Ok(Some(FrontendDropAdminStatus {
        descriptor: admin.descriptor.clone().into(),
        admin_pubkey: admin.descriptor.drop_pub.clone(),
        generation: admin.generation,
        rotation_retire_at: admin.rotation_retire_at,
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
    }))
}

#[tauri::command]
pub async fn get_drop_admin_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<FrontendDropAdminStatus>, FrontendError> {
    get_drop_admin_status_impl(password, &state).await
}

pub async fn update_drop_admin_config_impl(
    update: FrontendDropAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    let (mut admin, admin_sk) = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        let admin = serde_json::from_slice::<StoredDropAdmin>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e)))?;
        let master_seed = get_wallet_master_seed(&mut service)?;
        let (admin_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, admin.generation);
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
        let client = drop_client::create_drop_http_client();
        push_all_channel_members(
            &client,
            state,
            &admin,
            &admin_sk,
            password.as_deref(),
        )
        .await?;
    }

    {
        let mut service = state.service.lock().unwrap();
        let bytes = serde_json::to_vec(&admin).map_err(|e| {
            FrontendError::from(format!("Failed to serialize StoredDropAdmin: {}", e))
        })?;
        service
            .save_encrypted_data(MARKET_DROP_ADMIN_KEY, &bytes, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    let (stats, active_slot_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_drop_members_with_stats(state, &mut service, &admin, "card", password.as_deref())?
    };

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(FrontendDropAdminStatus {
        descriptor: admin.descriptor.clone().into(),
        admin_pubkey: admin.descriptor.drop_pub.clone(),
        generation: admin.generation,
        rotation_retire_at: admin.rotation_retire_at,
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
    })
}

#[tauri::command]
pub async fn update_drop_admin_config(
    update: FrontendDropAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    update_drop_admin_config_impl(update, push_now, password, &state).await
}

pub async fn block_drop_slot_impl(
    pubkey_hex64: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    let update = FrontendDropAdminUpdateRequest {
        blocked_pubkeys: Some({
            let mut service = state.service.lock().unwrap();
            let admin: StoredDropAdmin = service
                .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
                .map_err(FrontendError::from)
                .and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|e| {
                        FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e))
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

    update_drop_admin_config_impl(update, true, password, state).await
}

#[tauri::command]
pub async fn block_drop_slot(
    pubkey_hex64: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    block_drop_slot_impl(pubkey_hex64, password, &state).await
}

pub fn delete_drop_admin_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state.service.lock().unwrap();

    let admin_opt: Option<StoredDropAdmin> = service
        .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    service
        .save_encrypted_data(MARKET_DROP_ADMIN_KEY, b"null", password.as_deref())
        .map_err(FrontendError::from)?;

    if let Some(admin) = admin_opt {
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let initial_len = settings.market_selected_drops.len();
        settings
            .market_selected_drops
            .retain(|d| d.read_url != admin.descriptor.read_url);
        if settings.market_selected_drops.len() != initial_len {
            let _ = state.save_settings(&mut service, settings, password.as_deref());
        }
    }

    Ok(())
}

#[tauri::command]
pub fn delete_drop_admin(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    delete_drop_admin_impl(password, &state)
}

pub async fn discover_available_drops_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendDiscoveredDrop>, FrontendError> {
    let (my_ed_sk, known_descriptors) = {
        let mut service = state.service.lock().unwrap();

        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;

        let mut descriptors: Vec<(FrontendDropDescriptor, String)> = Vec::new();

        // 1. From own drop admin
        if let Ok(admin_bytes) =
            service.load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
        {
            if let Ok(admin) = serde_json::from_slice::<StoredDropAdmin>(&admin_bytes) {
                if !admin.descriptor.read_url.is_empty() && !admin.descriptor.write_url.is_empty() {
                    descriptors.push((admin.descriptor.into(), "Eigene Ablage".to_string()));
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
                for d in c.drops {
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
                    let author = sc.card.profile.first_name.as_deref().unwrap_or("Kontakt-Karte");
                    for d in &sc.card.drops {
                        descriptors.push((d.clone().into(), author.to_string()));
                    }
                }
            }
        }

        (my_ed_sk, descriptors)
    };

    let settings = state
        .get_cached_settings()
        .or_else(|_| {
            let mut s = state.service.lock().unwrap();
            state.load_settings(&mut s, password.as_deref())
        })
        .unwrap_or_default();

    let client = drop_client::create_drop_probe_client();
    let mut deduped: BTreeMap<String, (FrontendDropDescriptor, String)> = BTreeMap::new();
    for (d, label) in known_descriptors {
        if !d.read_url.is_empty() && !d.drop_pub.is_empty() {
            deduped.entry(d.read_url.clone()).or_insert((d, label));
        }
    }

    let mut results = Vec::new();
    for (_, (desc, label)) in deduped {
        let is_selected = settings
            .market_selected_drops
            .iter()
            .any(|d| d.read_url == desc.read_url);

        let is_authorized = if let Some(ref slot_id) = desc.slot_id {
            match drop_client::head_slot_capability(&client, &desc.write_url, slot_id, "card").await {
                Ok(cap) => cap.authorized,
                Err(_) => false,
            }
        } else {
            match humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &desc.drop_pub) {
                Ok(stealth_pk) => {
                    match drop_client::head_slot_capability(&client, &desc.write_url, &stealth_pk, "card").await {
                        Ok(cap) => cap.authorized,
                        Err(_) => false,
                    }
                }
                Err(_) => false,
            }
        };

        results.push(FrontendDiscoveredDrop {
            descriptor: desc,
            is_authorized,
            is_selected,
            source_label: label,
        });
    }

    Ok(results)
}

#[tauri::command]
pub async fn discover_available_drops(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendDiscoveredDrop>, FrontendError> {
    discover_available_drops_impl(password, &state).await
}

pub async fn set_selected_upload_drops_impl(
    drops: Vec<FrontendDropDescriptor>,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<FrontendDropDescriptor>, FrontendError> {
    if drops.len() > 3 {
        return Err(FrontendError::from("Maximum 3 upload drops allowed".to_string()));
    }

    for d in &drops {
        drop_client::validate_drop_url(&d.write_url).map_err(FrontendError::from)?;
        drop_client::validate_drop_url(&d.read_url).map_err(FrontendError::from)?;
        humoco_market::parse_ed25519_pubkey_hex64(&d.drop_pub)
            .map_err(|e| FrontendError::from(format!("Invalid drop_pub: {}", e)))?;
        if let Some(ref sid) = d.slot_id {
            humoco_market::parse_ed25519_pubkey_hex64(sid)
                .map_err(|e| FrontendError::from(format!("Invalid slot_id: {}", e)))?;
        }
    }

    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        settings.market_selected_drops = drops.clone();
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;

        // Update drops on StoredOwnCard & re-sign if present
        let existing_opt: Option<StoredOwnCard> = service
            .load_encrypted_data(MARKET_OWN_CARD_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());

        if let Some(mut existing) = existing_opt {
            let now_unix = chrono::Utc::now().timestamp().max(0) as u64;
            let v = humoco_market::next_card_version(Some(existing.card.v), now_unix);
            existing.card.v = v;
            existing.card.drops = drops.iter().cloned().map(Into::into).collect();

            let card_jws = service
                .with_wallet_and_identity(|_wallet, identity| {
                    humoco_market::sign_card(existing.card.clone(), &identity.signing_key)
                        .map_err(|e| human_money_core::app_service::AppFacadeError::Generic(e.to_string()))
                })
                .map_err(FrontendError::from)??;

            let stored_own = StoredOwnCard {
                card: existing.card,
                card_jws,
            };
            if let Ok(bytes) = serde_json::to_vec(&stored_own) {
                let _ = service.save_encrypted_data(MARKET_OWN_CARD_KEY, &bytes, password.as_deref());
            }
        }
    }

    // Trigger upload to selected drops
    let _ = sync_market_drops_impl(None, Some(true), password, state).await;

    Ok(drops)
}

#[tauri::command]
pub async fn set_selected_upload_drops(
    drops: Vec<FrontendDropDescriptor>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendDropDescriptor>, FrontendError> {
    set_selected_upload_drops_impl(drops, password, &state).await
}

pub async fn sync_market_drops_impl(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropSyncResult, FrontendError> {
    let mut settings = {
        let mut service = state.service.lock().unwrap();
        state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default()
    };

    if settings.market_poll_wifi_only && is_wifi == Some(false) {
        return Ok(FrontendDropSyncResult {
            skipped_wifi_only: true,
            uploaded_to: 0,
            polled_drops: 0,
            decrypted_slots: 0,
            imported_cards: 0,
            imported_wot_lists: 0,
            errors: Vec::new(),
        });
    }

    let client = drop_client::create_drop_http_client();
    let mut errors = Vec::new();
    let mut uploaded_to = 0;
    let mut polled_drops = 0;
    let mut decrypted_slots = 0;
    let mut imported_cards = 0;
    let mut imported_wot_lists = 0;

    let now_secs = drop_client::current_unix_timestamp();

    // 1. Ensure own DropAdmin members are up-to-date on server
    {
        let admin_opt: Option<(StoredDropAdmin, ed25519_dalek::SigningKey)> = {
            let mut service = state.service.lock().unwrap();
            let admin_opt = service
                .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<StoredDropAdmin>(&bytes).ok());
            if let Some(admin) = admin_opt {
                if let Ok(seed) = get_wallet_master_seed(&mut service) {
                    let (admin_sk, _, _) =
                        humoco_market::derive_drop_admin_keypair(&seed, admin.drop_index, admin.generation);
                    Some((admin, admin_sk))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some((admin, admin_sk)) = admin_opt {
            let _ = push_all_channel_members(
                &client,
                state,
                &admin,
                &admin_sk,
                password.as_deref(),
            )
            .await;
        }
    }

    // 2. Upload check
    let (my_ed_sk, master_seed, own_card_opt, readers, readers_hash) = {
        let mut service = state.service.lock().unwrap();
        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;
        let master_seed = my_ed_sk.to_bytes();

        let own_card: Option<StoredOwnCard> = service
            .load_encrypted_data(MARKET_OWN_CARD_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());

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

        (my_ed_sk, master_seed, own_card, reader_keys, readers_hash)
    };

    let mut sync_state: StoredDropSync = {
        let mut service = state.service.lock().unwrap();
        service
            .load_encrypted_data(MARKET_DROP_SYNC_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    };

    // Gather upload targets: selected drops + enabled public drops
    let mut upload_targets: Vec<humoco_market::DropDescriptor> = settings
        .market_selected_drops
        .iter()
        .cloned()
        .map(Into::into)
        .collect();

    for pd in &settings.public_drops {
        if pd.enabled && !pd.write_url.is_empty() {
            upload_targets.push(humoco_market::DropDescriptor {
                write_url: pd.write_url.clone(),
                read_url: pd.read_url.clone(),
                slot_id: Some(pd.access_pubkey.clone()),
                drop_pub: pd.drop_pub.clone(),
            });
        }
    }

    if let Some(ref own_card) = own_card_opt {
        if now_secs < own_card.card.valid_until && !upload_targets.is_empty() {
            let needs_upload = force_upload == Some(true)
                || sync_state.last_uploaded_card_v != Some(own_card.card.v)
                || sync_state.last_readers_hash.as_deref() != Some(&readers_hash)
                || now_secs.saturating_sub(sync_state.last_upload_at.unwrap_or(0)) >= DROP_HEARTBEAT_SECS;

            if needs_upload {
                let bundle = {
                    let mut service = state.service.lock().unwrap();
                    let store: humoco_market::CardStore = service
                        .load_encrypted_data(MARKET_CARD_STORE_KEY, password.as_deref())
                        .ok()
                        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                        .unwrap_or_default();
                    let reg = state.get_or_load_wot_register(&mut service, password.as_deref()).ok();
                    let trust_level_of = |did: &str| -> u8 {
                        if let Some(ref r) = reg {
                            let can_did = canonicalize_did(did).unwrap_or_else(|_| did.to_string());
                            r.own_contacts.get(&can_did).map(|e| e.level).unwrap_or(0)
                        } else {
                            0
                        }
                    };
                    humoco_market::build_card_bundle(
                        Some(&own_card.card_jws),
                        &store,
                        &trust_level_of,
                        DROP_CARD_MAX_BYTES,
                    )
                };

                match humoco_market::seal_slot_blob(&bundle, &readers, DROP_CARD_MAX_BYTES) {
                    Ok(blob) => {
                        let mut settings_changed = false;
                        for drop_desc in &mut upload_targets {
                            let nonce = drop_client::generate_drop_nonce();
                            let signed_res = if drop_desc.slot_id.is_some() {
                                // Public drop upload signed with derived r
                                match humoco_market::derive_public_drop_keypair(&master_seed, &drop_desc.write_url) {
                                    Ok((r_sk, _, _)) => {
                                        Ok(humoco_market::sign_member_request_direct(
                                            &r_sk,
                                            "PUT",
                                            "card",
                                            now_secs,
                                            &nonce,
                                            &blob,
                                        ))
                                    }
                                    Err(e) => Err(format!("Public drop key derivation error: {}", e)),
                                }
                            } else {
                                // Stealth drop upload signed with stealth key
                                humoco_market::sign_member_request_stealth(
                                    &my_ed_sk,
                                    &drop_desc.drop_pub,
                                    "PUT",
                                    "card",
                                    now_secs,
                                    &nonce,
                                    &blob,
                                )
                                .map_err(|e| format!("Stealth signing error: {}", e))
                            };

                            match signed_res {
                                Ok(signed) => {
                                    match drop_client::put_slot(
                                        &client,
                                        &drop_desc.write_url,
                                        &signed,
                                        "card",
                                        &blob,
                                        None,
                                    )
                                    .await
                                    {
                                        Ok(PutSlotOutcome::Saved { rotation, .. }) => {
                                            uploaded_to += 1;
                                            if let Some(ref cert) = rotation {
                                                if drop_client::apply_verified_rotation(drop_desc, cert, now_secs) {
                                                    // Update in settings
                                                    for s_drop in &mut settings.market_selected_drops {
                                                        if s_drop.read_url == drop_desc.read_url {
                                                            s_drop.drop_pub = drop_desc.drop_pub.clone();
                                                            settings_changed = true;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                            errors.push(format!("Upload to {}: Precondition failed (ETag)", drop_desc.write_url));
                                        }
                                        Err(e) => {
                                            errors.push(format!("Upload to {}: {}", drop_desc.write_url, e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    errors.push(format!("Signing error for {}: {}", drop_desc.write_url, e));
                                }
                            }
                        }

                        if settings_changed {
                            let mut service = state.service.lock().unwrap();
                            let _ = state.save_settings(&mut service, settings.clone(), password.as_deref());
                        }

                        if uploaded_to > 0 {
                            sync_state.last_upload_at = Some(now_secs);
                            sync_state.last_readers_hash = Some(readers_hash);
                            sync_state.last_uploaded_card_v = Some(own_card.card.v);
                        }
                    }
                    Err(e) => {
                        errors.push(format!("Failed to seal slot blob: {}", e));
                    }
                }
            }
        }
    }

    // 2b. Upload Channel "wot"
    if settings.wot_share_via_drop {
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
                    .market_selected_drops
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect();

                for pd in &settings.public_drops {
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

                let class_budget = crate::settings::DROP_WOT_MAX_BYTES / 3;
                let mut reg_modified = false;
                let mut settings_changed = false;

                for drop_desc in &mut wot_upload_targets {
                    if drop_desc.write_url.is_empty() {
                        continue;
                    }

                    match reg.prepare_drop_sync_subpackages(
                        &drop_desc.write_url,
                        Some(&readers_refs),
                        Some(class_budget),
                        force_upload.unwrap_or(false),
                    ) {
                        Ok(Some((subpackages, receipt))) => {
                            if subpackages.is_empty() {
                                continue;
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
                                continue;
                            }

                            match humoco_market::seal_multi_class_slot_blob(&class_slices, crate::settings::DROP_WOT_MAX_BYTES) {
                                Ok(blob) => {
                                    let nonce = drop_client::generate_drop_nonce();
                                    let signed_res = if drop_desc.slot_id.is_some() {
                                        match humoco_market::derive_public_drop_keypair(&master_seed, &drop_desc.write_url) {
                                            Ok((r_sk, _, _)) => {
                                                Ok(humoco_market::sign_member_request_direct(
                                                    &r_sk,
                                                    "PUT",
                                                    "wot",
                                                    now_secs,
                                                    &nonce,
                                                    &blob,
                                                ))
                                            }
                                            Err(e) => Err(format!("Public drop key derivation error: {}", e)),
                                        }
                                    } else {
                                        humoco_market::sign_member_request_stealth(
                                            &my_ed_sk,
                                            &drop_desc.drop_pub,
                                            "PUT",
                                            "wot",
                                            now_secs,
                                            &nonce,
                                            &blob,
                                        )
                                        .map_err(|e| format!("Stealth signing error: {}", e))
                                    };

                                    match signed_res {
                                        Ok(signed) => {
                                            match drop_client::put_slot(
                                                &client,
                                                &drop_desc.write_url,
                                                &signed,
                                                "wot",
                                                &blob,
                                                None,
                                            )
                                            .await
                                            {
                                                Ok(PutSlotOutcome::Saved { rotation, .. }) => {
                                                    uploaded_to += 1;
                                                    reg.commit_drop_sync_receipt(receipt, now_secs);
                                                    reg_modified = true;

                                                    if let Some(ref cert) = rotation {
                                                        if drop_client::apply_verified_rotation(drop_desc, cert, now_secs) {
                                                            for s_drop in &mut settings.market_selected_drops {
                                                                if s_drop.read_url == drop_desc.read_url {
                                                                    s_drop.drop_pub = drop_desc.drop_pub.clone();
                                                                    settings_changed = true;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                Ok(PutSlotOutcome::PreconditionFailed { .. }) => {
                                                    errors.push(format!("Upload wot to {}: Precondition failed (ETag)", drop_desc.write_url));
                                                }
                                                Err(e) => {
                                                    errors.push(format!("Upload wot to {}: {}", drop_desc.write_url, e));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            errors.push(format!("Signing error for wot on {}: {}", drop_desc.write_url, e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    errors.push(format!("Failed to seal multi-class wot slot blob: {}", e));
                                }
                            }
                        }
                        Ok(None) => {
                            // Slot on transport_id is already 100% up-to-date!
                        }
                        Err(e) => {
                            errors.push(format!("Failed to prepare drop sync subpackages for {}: {}", drop_desc.write_url, e));
                        }
                    }
                }

                if settings_changed {
                    let mut service = state.service.lock().unwrap();
                    let _ = state.save_settings(&mut service, settings.clone(), password.as_deref());
                }

                if reg_modified {
                    let mut service = state.service.lock().unwrap();
                    let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
                }
            }
        }
    }

    // 3. Download poll
    // Split into direct O(1) slot fetches (for drops with slot_id) and index-based fetches
    let (direct_slot_fetches, candidate_read_urls) = {
        let mut service = state.service.lock().unwrap();
        let mut direct_fetches: Vec<(String, String)> = Vec::new(); // (read_url, slot_id)
        let mut index_urls = Vec::new();

        // Own selected drops
        for d in &settings.market_selected_drops {
            if !d.read_url.is_empty() {
                if let Some(ref sid) = d.slot_id {
                    direct_fetches.push((d.read_url.clone(), sid.clone()));
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
                    for d in c.drops {
                        if !d.read_url.is_empty() {
                            if let Some(ref sid) = d.slot_id {
                                direct_fetches.push((d.read_url, sid.clone()));
                            } else {
                                index_urls.push(d.read_url);
                            }
                        }
                    }
                }
            }
        }

        // Stored cards drops where author has level >= share_min_level
        if let Some(ref store) = card_store {
            for sc in store.cards.values() {
                let author_can = canonicalize_did(&sc.card.did).unwrap_or_else(|_| sc.card.did.clone());
                let lvl = if let Some(ref r) = reg {
                    r.own_contacts.get(&author_can).map(|e| e.level).unwrap_or(0)
                } else {
                    0
                };
                if lvl >= settings.share_min_level {
                    for d in &sc.card.drops {
                        if !d.read_url.is_empty() {
                            if let Some(ref sid) = d.slot_id {
                                direct_fetches.push((d.read_url.clone(), sid.clone()));
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
                if deduped_urls.len() >= MAX_POLLED_DROPS {
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

    let max_store_hops = settings.max_store_hops.clamp(1, 3);
    let my_x25519_sk = human_money_core::crypto::ed25519_sk_to_x25519_sk(&my_ed_sk);

    // 3a. Direct O(1) fetches
    for (read_url, slot_id) in direct_slot_fetches {
        polled_drops += 1;

        // Fetch card slot
        match drop_client::get_slot(&client, &read_url, &slot_id, "card").await {
            Ok(Some(blob)) => {
                match humoco_market::open_slot_blob(&blob, &my_x25519_sk) {
                    Ok(Some(bundle)) => {
                        decrypted_slots += 1;
                        for env in bundle.envelopes {
                            let mut service = state.service.lock().unwrap();
                            let outcome = service.with_wallet(|wallet| {
                                let is_offender = |did: &str| -> bool {
                                    use human_money_core::models::conflict::TrustStatus;
                                    matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
                                };
                                humoco_market::accept_envelope(
                                    &mut store,
                                    &env,
                                    now_secs,
                                    max_store_hops,
                                    &is_offender,
                                )
                            });

                            if let Ok(humoco_market::AcceptOutcome::Accepted { did, .. }) = outcome {
                                imported_cards += 1;
                                let can_did = canonicalize_did(&did).unwrap_or_else(|_| did.clone());
                                sync_state.drop_sources_by_did.insert(can_did, read_url.clone());

                                if let Some(stored) = store.cards.values().find(|sc| sc.card.did == did) {
                                    let card_drops: Vec<FrontendDropDescriptor> = stored
                                        .card
                                        .drops
                                        .iter()
                                        .cloned()
                                        .map(Into::into)
                                        .collect();
                                    state.update_contact_drops_from_card(
                                        &mut service,
                                        &did,
                                        &card_drops,
                                        password.as_deref(),
                                    );
                                }
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        errors.push(format!("Error opening card slot {}: {}", slot_id, e));
                    }
                }
            }
            Ok(None) => {
                // Graceful 404: slot not uploaded yet
            }
            Err(e) => {
                errors.push(format!("Failed to get direct card slot {} from {}: {}", slot_id, read_url, e));
            }
        }

        // Fetch wot slot
        match drop_client::get_slot(&client, &read_url, &slot_id, "wot").await {
            Ok(Some(blob)) => {
                match humoco_market::open_multi_class_slot_blob(&blob, &my_x25519_sk) {
                    Ok(Some(bytes)) => {
                        decrypted_slots += 1;
                        match serde_json::from_slice::<humoco_wot::WotSyncPackage>(&bytes) {
                            Ok(pkg) => {
                                let mut service = state.service.lock().unwrap();
                                if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
                                    match reg.import_sync_package(&pkg, None, now_secs) {
                                        Ok(stats) => {
                                            let accepted = stats.accepted_total() + stats.merged;
                                            imported_wot_lists += accepted;
                                            if accepted > 0 || reg.own_list_dirty {
                                                if reg.own_list_dirty {
                                                    let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
                                                }
                                                let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
                                            }
                                        }
                                        Err(e) => {
                                            errors.push(format!("Failed to import direct wot package from {}: {}", read_url, e));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                errors.push(format!("Failed to parse direct wot sync package from {}: {}", read_url, e));
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        errors.push(format!("Error opening direct wot slot {}: {}", slot_id, e));
                    }
                }
            }
            Ok(None) => {
                // Graceful 404: slot not uploaded yet
            }
            Err(e) => {
                errors.push(format!("Failed to get direct wot slot {} from {}: {}", slot_id, read_url, e));
            }
        }
    }

    // 3b. Index-based fetches
    for read_url in candidate_read_urls {
        polled_drops += 1;
        let cached_etag = sync_state.etags_by_read_url.get(&read_url).map(|s| s.as_str());
        let cached_lm = sync_state.last_modified_by_read_url.get(&read_url).map(|s| s.as_str());

        match drop_client::list_index(&client, &read_url, cached_etag, cached_lm).await {
            Ok(IndexFetchResult::NotModified) => {}
            Ok(IndexFetchResult::Modified {
                entries,
                etag,
                last_modified,
            }) => {
                if let Some(et) = etag {
                    sync_state.etags_by_read_url.insert(read_url.clone(), et);
                }
                if let Some(lm) = last_modified {
                    sync_state.last_modified_by_read_url.insert(read_url.clone(), lm);
                }

                for entry in entries {
                    if entry.channel != "card" && entry.channel != "wot" {
                        continue;
                    }
                    let mtime_key = format!("{}:{}:{}", read_url, entry.channel, entry.id);
                    if sync_state.slot_mtimes.get(&mtime_key) == Some(&entry.mtime) {
                        continue;
                    }

                    if entry.channel == "card" {
                        match drop_client::get_slot(&client, &read_url, &entry.id, "card").await {
                            Ok(Some(blob)) => {
                                sync_state.slot_mtimes.insert(mtime_key, entry.mtime);
                                match humoco_market::open_slot_blob(&blob, &my_x25519_sk) {
                                    Ok(Some(bundle)) => {
                                        decrypted_slots += 1;
                                        for env in bundle.envelopes {
                                            let mut service = state.service.lock().unwrap();
                                            let outcome = service.with_wallet(|wallet| {
                                                let is_offender = |did: &str| -> bool {
                                                    use human_money_core::models::conflict::TrustStatus;
                                                    matches!(wallet.check_reputation(did), TrustStatus::KnownOffender(_))
                                                };
                                                humoco_market::accept_envelope(
                                                    &mut store,
                                                    &env,
                                                    now_secs,
                                                    max_store_hops,
                                                    &is_offender,
                                                )
                                            });

                                            if let Ok(humoco_market::AcceptOutcome::Accepted { did, .. }) = outcome {
                                                imported_cards += 1;
                                                let can_did = canonicalize_did(&did).unwrap_or_else(|_| did.clone());
                                                sync_state.drop_sources_by_did.insert(can_did, read_url.clone());

                                                if let Some(stored) = store.cards.values().find(|sc| sc.card.did == did) {
                                                    let card_drops: Vec<FrontendDropDescriptor> = stored
                                                        .card
                                                        .drops
                                                        .iter()
                                                        .cloned()
                                                        .map(Into::into)
                                                        .collect();
                                                    state.update_contact_drops_from_card(
                                                        &mut service,
                                                        &did,
                                                        &card_drops,
                                                        password.as_deref(),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        errors.push(format!("Error opening card slot {}: {}", entry.id, e));
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                errors.push(format!("Failed to get card slot {}: {}", entry.id, e));
                            }
                        }
                    } else if entry.channel == "wot" {
                        match drop_client::get_slot(&client, &read_url, &entry.id, "wot").await {
                            Ok(Some(blob)) => {
                                sync_state.slot_mtimes.insert(mtime_key, entry.mtime);
                                match humoco_market::open_multi_class_slot_blob(&blob, &my_x25519_sk) {
                                    Ok(Some(bytes)) => {
                                        decrypted_slots += 1;
                                        match serde_json::from_slice::<humoco_wot::WotSyncPackage>(&bytes) {
                                            Ok(pkg) => {
                                                let mut service = state.service.lock().unwrap();
                                                if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
                                                    match reg.import_sync_package(&pkg, None, now_secs) {
                                                        Ok(stats) => {
                                                            let accepted = stats.accepted_total() + stats.merged;
                                                            imported_wot_lists += accepted;
                                                            if accepted > 0 || reg.own_list_dirty {
                                                                if reg.own_list_dirty {
                                                                    let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
                                                                }
                                                                let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
                                                            }
                                                        }
                                                        Err(e) => {
                                                            errors.push(format!("Failed to import wot package from {}: {}", read_url, e));
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                errors.push(format!("Failed to parse wot sync package from {}: {}", read_url, e));
                                            }
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        errors.push(format!("Error opening wot slot {}: {}", entry.id, e));
                                    }
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                errors.push(format!("Failed to get wot slot {}: {}", entry.id, e));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Failed to list index from {}: {}", read_url, e));
            }
        }
    }

    // Save updated CardStore and StoredDropSync
    {
        let mut service = state.service.lock().unwrap();
        if let Ok(bytes) = serde_json::to_vec(&store) {
            let _ = service.save_encrypted_data(MARKET_CARD_STORE_KEY, &bytes, password.as_deref());
        }
        if let Ok(bytes) = serde_json::to_vec(&sync_state) {
            let _ = service.save_encrypted_data(MARKET_DROP_SYNC_KEY, &bytes, password.as_deref());
        }
    }

    Ok(FrontendDropSyncResult {
        skipped_wifi_only: false,
        uploaded_to,
        polled_drops,
        decrypted_slots,
        imported_cards,
        imported_wot_lists,
        errors,
    })
}

#[tauri::command]
pub async fn sync_market_drops(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropSyncResult, FrontendError> {
    sync_market_drops_impl(is_wifi, force_upload, password, &state).await
}

pub async fn delete_own_drop_slots_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<usize, FrontendError> {
    let (my_ed_sk, master_seed, drops) = {
        let mut service = state.service.lock().unwrap();
        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;
        let master_seed = my_ed_sk.to_bytes();
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        (my_ed_sk, master_seed, settings.market_selected_drops)
    };

    let client = drop_client::create_drop_http_client();
    let mut deleted_count = 0;
    let now = drop_client::current_unix_timestamp();

    for drop_desc in &drops {
        for ch in ["card", "wot", "backup", "post"] {
            let nonce = drop_client::generate_drop_nonce();
            let signed_res = if drop_desc.slot_id.is_some() {
                match humoco_market::derive_public_drop_keypair(&master_seed, &drop_desc.write_url) {
                    Ok((r_sk, _, _)) => {
                        Ok(humoco_market::sign_member_request_direct(&r_sk, "DELETE", ch, now, &nonce, b""))
                    }
                    Err(e) => Err(e.to_string()),
                }
            } else {
                humoco_market::sign_member_request_stealth(
                    &my_ed_sk,
                    &drop_desc.drop_pub,
                    "DELETE",
                    ch,
                    now,
                    &nonce,
                    b"",
                )
                .map_err(|e| e.to_string())
            };

            if let Ok(signed) = signed_res {
                if drop_client::delete_slot(&client, &drop_desc.write_url, &signed, ch)
                    .await
                    .is_ok()
                {
                    deleted_count += 1;
                }
            }
        }
    }

    // Clear drop_sent_log in TrustRegister
    {
        let mut service = state.service.lock().unwrap();
        if let Ok(mut reg) = state.get_or_load_wot_register(&mut service, password.as_deref()) {
            reg.clear_drop_sent_log(None);
            let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
        }
    }

    // Clear upload state in sync_state
    {
        let mut service = state.service.lock().unwrap();
        if let Ok(mut sync_state) = service
            .load_encrypted_data(MARKET_DROP_SYNC_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredDropSync>(&bytes)
                    .map_err(|e| FrontendError::from(format!("Parse error: {}", e)))
            })
        {
            sync_state.last_upload_at = None;
            sync_state.last_readers_hash = None;
            sync_state.last_uploaded_card_v = None;
            if let Ok(bytes) = serde_json::to_vec(&sync_state) {
                let _ = service.save_encrypted_data(MARKET_DROP_SYNC_KEY, &bytes, password.as_deref());
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
    }

    Ok(deleted_count)
}

#[tauri::command]
pub async fn delete_own_drop_slots(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, FrontendError> {
    delete_own_drop_slots_impl(password, &state).await
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

    let can_read_my_wot_before = settings.wot_share_via_drop
        && current_level.unwrap_or(0) >= 1
        && !is_offender;
    let can_read_my_wot_after = settings.wot_share_via_drop
        && new_level >= 1
        && !is_offender;

    let admin_opt: Option<StoredDropAdmin> = service
        .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let (can_write_my_drop_before, can_write_my_drop_after) = if let Some(admin) = admin_opt {
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
        can_write_my_drop_before,
        can_write_my_drop_after,
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

pub async fn probe_public_drop_impl(
    url: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicDropInfo, FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    drop_client::validate_drop_url(&canonical_url).map_err(FrontendError::from)?;

    let probe_client = drop_client::create_drop_probe_client();
    let info = drop_client::fetch_drop_info(&probe_client, &canonical_url)
        .await
        .map_err(FrontendError::from)?;

    let master_seed = {
        let mut service = state.service.lock().unwrap();
        let _ = password; // kept for consistency
        get_wallet_master_seed(&mut service)?
    };

    let (_r_sk, _r_vk, access_pubkey) =
        humoco_market::derive_public_drop_keypair(&master_seed, &canonical_url)
            .map_err(|e| FrontendError::from(e.to_string()))?;

    let mut authorized_channels = Vec::new();
    for ch in ["card", "wot", "backup", "post"] {
        if let Ok(cap) =
            drop_client::head_slot_capability(&probe_client, &canonical_url, &access_pubkey, ch).await
        {
            if cap.authorized {
                authorized_channels.push(ch.to_string());
            }
        }
    }

    Ok(FrontendPublicDropInfo {
        write_url: canonical_url,
        read_url: info.read_url,
        drop_pub: info.drop_pub,
        register_url: info.register_url,
        access_pubkey,
        authorized_channels,
    })
}

#[tauri::command]
pub async fn probe_public_drop(
    url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPublicDropInfo, FrontendError> {
    probe_public_drop_impl(url, password, &state).await
}

pub async fn save_public_drop_impl(
    write_url: String,
    read_url: String,
    drop_pub: String,
    register_url: Option<String>,
    enabled: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendPublicDropInfo, FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;
    drop_client::validate_drop_url(&canonical_url).map_err(FrontendError::from)?;
    drop_client::validate_drop_url(&read_url).map_err(FrontendError::from)?;
    humoco_market::parse_ed25519_pubkey_hex64(&drop_pub)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let master_seed = {
        let mut service = state.service.lock().unwrap();
        get_wallet_master_seed(&mut service)?
    };

    let (_r_sk, _r_vk, access_pubkey) =
        humoco_market::derive_public_drop_keypair(&master_seed, &canonical_url)
            .map_err(|e| FrontendError::from(e.to_string()))?;

    let probe_client = drop_client::create_drop_probe_client();
    let mut authorized_channels = Vec::new();
    for ch in ["card", "wot", "backup", "post"] {
        if let Ok(cap) =
            drop_client::head_slot_capability(&probe_client, &canonical_url, &access_pubkey, ch).await
        {
            if cap.authorized {
                authorized_channels.push(ch.to_string());
            }
        }
    }

    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        let stored = StoredPublicDrop {
            write_url: canonical_url.clone(),
            read_url: read_url.clone(),
            drop_pub: drop_pub.clone(),
            register_url: register_url.clone(),
            access_pubkey: access_pubkey.clone(),
            enabled,
            authorized_channels: authorized_channels.clone(),
        };

        if let Some(pos) = settings.public_drops.iter().position(|d| d.write_url == canonical_url) {
            settings.public_drops[pos] = stored;
        } else {
            settings.public_drops.push(stored);
        }

        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    Ok(FrontendPublicDropInfo {
        write_url: canonical_url,
        read_url,
        drop_pub,
        register_url,
        access_pubkey,
        authorized_channels,
    })
}

#[tauri::command]
pub async fn save_public_drop(
    write_url: String,
    read_url: String,
    drop_pub: String,
    register_url: Option<String>,
    enabled: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPublicDropInfo, FrontendError> {
    save_public_drop_impl(write_url, read_url, drop_pub, register_url, enabled, password, &state).await
}

pub async fn remove_public_drop_impl(
    write_url: String,
    delete_remote_slots: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<(), FrontendError> {
    let canonical_url = humoco_market::canonicalize_write_url(&write_url)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if delete_remote_slots {
        let master_seed = {
            let mut service = state.service.lock().unwrap();
            get_wallet_master_seed(&mut service)?
        };
        if let Ok((r_sk, _, _)) = humoco_market::derive_public_drop_keypair(&master_seed, &canonical_url) {
            let client = drop_client::create_drop_http_client();
            let now = drop_client::current_unix_timestamp();
            for ch in ["card", "wot", "backup", "post"] {
                let nonce = drop_client::generate_drop_nonce();
                let signed = humoco_market::sign_member_request_direct(&r_sk, "DELETE", ch, now, &nonce, b"");
                let _ = drop_client::delete_slot(&client, &canonical_url, &signed, ch).await;
            }
        }
    }

    {
        let mut service = state.service.lock().unwrap();
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();

        settings.public_drops.retain(|d| d.write_url != canonical_url);
        state
            .save_settings(&mut service, settings, password.as_deref())
            .map_err(FrontendError::from)?;

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
    }

    Ok(())
}

#[tauri::command]
pub async fn remove_public_drop(
    write_url: String,
    delete_remote_slots: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    remove_public_drop_impl(write_url, delete_remote_slots, password, &state).await
}

pub async fn rotate_drop_admin_key_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    let (mut admin, _old_admin_sk, new_admin_sk, _new_admin_vk, new_admin_pubkey, retire_at, cert) = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        let admin: StoredDropAdmin = serde_json::from_slice(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e)))?;

        let master_seed = get_wallet_master_seed(&mut service)?;
        let (old_sk, _, _) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, admin.generation);
        let next_gen = admin.generation + 1;
        let (new_sk, new_vk, new_pub) =
            humoco_market::derive_drop_admin_keypair(&master_seed, admin.drop_index, next_gen);

        let now = drop_client::current_unix_timestamp();
        let retire_at = now + humoco_market::DROP_ROTATION_GRACE_SECS;
        let cert = humoco_market::create_drop_rotation_certificate(&old_sk, &new_vk, retire_at);

        (admin, old_sk, new_sk, new_vk, new_pub, retire_at, cert)
    };

    let client = drop_client::create_drop_http_client();
    drop_client::rotate_admin_key(&client, &admin.descriptor.write_url, &new_admin_sk, &cert)
        .await
        .map_err(FrontendError::from)?;

    admin.previous_generation = Some(admin.generation);
    admin.generation += 1;
    admin.rotation_retire_at = Some(retire_at);
    admin.descriptor.drop_pub = new_admin_pubkey.clone();

    // Push dual-key member lists (P_k + P_{k+1}) to drop server
    push_all_channel_members(
        &client,
        state,
        &admin,
        &new_admin_sk,
        password.as_deref(),
    )
    .await?;

    {
        let mut service = state.service.lock().unwrap();
        let admin_bytes = serde_json::to_vec(&admin).map_err(|e| {
            FrontendError::from(format!("Failed to serialize StoredDropAdmin: {}", e))
        })?;
        service
            .save_encrypted_data(MARKET_DROP_ADMIN_KEY, &admin_bytes, password.as_deref())
            .map_err(FrontendError::from)?;

        // Update own drop in settings if present
        let mut settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        for d in &mut settings.market_selected_drops {
            if d.read_url == admin.descriptor.read_url {
                d.drop_pub = new_admin_pubkey.clone();
            }
        }
        let _ = state.save_settings(&mut service, settings, password.as_deref());
    }

    let (stats, active_slot_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_drop_members_with_stats(state, &mut service, &admin, "card", password.as_deref())?
    };

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(FrontendDropAdminStatus {
        descriptor: admin.descriptor.clone().into(),
        admin_pubkey: new_admin_pubkey,
        generation: admin.generation,
        rotation_retire_at: admin.rotation_retire_at,
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
    })
}

#[tauri::command]
pub async fn rotate_drop_admin_key(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    rotate_drop_admin_key_impl(password, &state).await
}

pub fn get_drop_overview_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropOverviewStatus, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let settings = state
        .get_cached_settings()
        .or_else(|_| state.load_settings(&mut service, password.as_deref()))
        .unwrap_or_default();

    let admin_opt: Option<StoredDropAdmin> = service
        .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let has_own_drop = admin_opt.as_ref().is_some_and(|a| !a.descriptor.write_url.is_empty());
    let own_read_url = admin_opt.as_ref().map(|a| a.descriptor.read_url.as_str()).unwrap_or("");

    let friend_drops = settings
        .market_selected_drops
        .iter()
        .filter(|d| d.read_url != own_read_url)
        .count();

    let public_drops = settings.public_drops.iter().filter(|d| d.enabled).count();

    let connected_count = (if has_own_drop { 1 } else { 0 }) + friend_drops + public_drops;

    let sync_state: Option<StoredDropSync> = service
        .load_encrypted_data(MARKET_DROP_SYNC_KEY, password.as_deref())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    let last_sync_at = sync_state.and_then(|s| s.last_upload_at);

    Ok(FrontendDropOverviewStatus {
        connected_count,
        friend_drops,
        public_drops,
        has_own_drop,
        own_drop_soft_warning: false,
        last_sync_at,
    })
}

#[tauri::command]
pub fn get_drop_overview_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropOverviewStatus, FrontendError> {
    get_drop_overview_status_impl(password, &state)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use crate::models::{
        FrontendCardListingInput, FrontendContact, FrontendListingType,
        FrontendSaveOwnCardRequest, FrontendUserProfile,
    };
    use crate::commands::market::{get_market_view_impl, save_own_contact_card_impl};

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
        let p = std::env::temp_dir().join(format!("humoco_drops_test_{}_{}", name, uuid::Uuid::new_v4()));
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
            )
            .unwrap();
        service
    }

    pub(crate) fn create_test_state(service: AppService) -> AppState {
        AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            regional_views: Mutex::new(None),
            wot: Mutex::new(None),
        }
    }

    #[allow(dead_code)]
    pub(crate) struct MockDropServer {
        pub base_url: String,
        pub data_dir: String,
        pub admin_pubkey: Arc<Mutex<String>>,
        pub members: Arc<Mutex<Vec<String>>>,
        pub channel_members: Arc<Mutex<HashMap<String, Vec<String>>>>,
        pub slots: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        pub etags: Arc<Mutex<HashMap<String, String>>>,
        pub rotation_cert: Arc<Mutex<Option<String>>>,
        pub _shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl MockDropServer {
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
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

            let s_data_dir = data_dir.clone();
            let s_admin_pub = Arc::clone(&admin_pub);
            let s_members = Arc::clone(&members);
            let s_channel_members = Arc::clone(&channel_members);
            let s_slots = Arc::clone(&slots);
            let s_etags = Arc::clone(&etags);
            let s_rotation = Arc::clone(&rotation_cert);

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            break;
                        }
                        Ok((mut stream, _)) = listener.accept() => {
                            let data_dir = s_data_dir.clone();
                            let admin_pub = Arc::clone(&s_admin_pub);
                            let members = Arc::clone(&s_members);
                            let channel_members = Arc::clone(&s_channel_members);
                            let slots = Arc::clone(&s_slots);
                            let etags = Arc::clone(&s_etags);
                            let rotation = Arc::clone(&s_rotation);

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
                                    format!("X-Humoco-Drop-Rotation: {}\r\n", r)
                                } else {
                                    String::new()
                                };

                                let response: Vec<u8> = if method == "GET" && path.contains("info") {
                                    let cur_admin = admin_pub.lock().unwrap().clone();
                                    let info_json = serde_json::json!({
                                        "drop_pub": cur_admin,
                                        "read_url": format!("http://localhost/{}", data_dir),
                                        "register_url": null,
                                        "channels": ["card", "wot", "backup", "post"]
                                    });
                                    let body_str = serde_json::to_string(&info_json).unwrap();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\n\r\n{}",
                                        rot_hdr_line, body_str.len(), body_str
                                    ).into_bytes()
                                } else if method == "HEAD" && path.starts_with("/drop.php") {
                                    let req_pub = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                    let m = members.lock().unwrap();
                                    if m.contains(&req_pub) {
                                        format!(
                                            "HTTP/1.1 200 OK\r\nX-Drop-Max-Bytes: 65536\r\nX-Drop-TTL-Days: 30\r\n{}Content-Length: 0\r\n\r\n",
                                            rot_hdr_line
                                        ).into_bytes()
                                    } else {
                                        format!("HTTP/1.1 403 Forbidden\r\n{}Content-Length: 0\r\n\r\n", rot_hdr_line).into_bytes()
                                    }
                                } else if method == "PUT" && path.starts_with("/drop.php") && path.contains("members") {
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
                                            if let Some((id, _)) = k.split_once('.') {
                                                m.contains(&id.to_string())
                                            } else {
                                                false
                                            }
                                        });
                                        format!("HTTP/1.1 204 No Content\r\n{}Content-Length: 0\r\n\r\n", rot_hdr_line).into_bytes()
                                    } else {
                                        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec()
                                    }
                                } else if method == "POST" && path.starts_with("/drop.php") && path.contains("rotate") {
                                    if let Ok(cert) = serde_json::from_slice::<humoco_market::DropRotationCertificate>(&body) {
                                        *admin_pub.lock().unwrap() = cert.new_pubkey.clone();
                                        *rotation.lock().unwrap() = Some(cert.to_header_value());
                                        format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-Humoco-Drop-Rotation: {}\r\nContent-Length: 11\r\n\r\n{{\"ok\":true}}",
                                            cert.to_header_value()
                                        ).into_bytes()
                                    } else {
                                        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec()
                                    }
                                } else if method == "GET" && path.starts_with("/drop.php") && path.contains("status") {
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
                                } else if method == "PUT" && path.starts_with("/drop.php") {
                                    let pubkey = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                    let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                    let is_member = members.lock().unwrap().contains(&pubkey);
                                    if is_member {
                                        let key = format!("{}.{}", pubkey, channel);
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
                                } else if method == "DELETE" && path.starts_with("/drop.php") {
                                    let pubkey = headers.get("x-drop-pubkey").cloned().unwrap_or_default();
                                    let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                    let key = format!("{}.{}", pubkey, channel);
                                    slots.lock().unwrap().remove(&key);
                                    format!("HTTP/1.1 204 No Content\r\n{}Content-Length: 0\r\n\r\n", rot_hdr_line).into_bytes()
                                } else if method == "GET" && path.contains("index.json") {
                                    let s_map = slots.lock().unwrap();
                                    let mut entries = Vec::new();
                                    for (k, v) in s_map.iter() {
                                        if let Some((id, ch)) = k.split_once('.') {
                                            entries.push(serde_json::json!({
                                                "id": id,
                                                "channel": ch,
                                                "mtime": 1000,
                                                "size": v.len(),
                                                "etag": "etag123"
                                            }));
                                        }
                                    }
                                    let body_str = serde_json::to_string(&entries).unwrap();
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"index_etag\"\r\nContent-Length: {}\r\n\r\n{}",
                                        body_str.len(), body_str
                                    ).into_bytes()
                                } else if method == "GET" && path.ends_with(".bin") {
                                    let fname = path.rsplit('/').next().unwrap_or("");
                                    let slot_key = fname.strip_suffix(".bin").unwrap_or(fname);
                                    let s_map = slots.lock().unwrap();
                                    if let Some(data) = s_map.get(slot_key) {
                                        let mut resp = format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                                            data.len()
                                        ).into_bytes();
                                        resp.extend_from_slice(data);
                                        resp
                                    } else {
                                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                                    }
                                } else {
                                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                                };

                                let _ = stream.write_all(&response).await;
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

        // 2. Alice prepares drop setup and finalize on MockDropServer A
        let draft = prepare_drop_setup_impl(
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

        let server_a = MockDropServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_drop_setup_impl(
            server_a.base_url.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        assert_eq!(finalize_status.data_dir, draft.data_dir);
        assert_eq!(finalize_status.total_member_count, 1); // Alice is in allow list automatically

        // Test connection
        let conn_test = test_drop_connection_impl(
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
                drops: vec![],
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
                drops: vec![],
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
        let updated_status = update_drop_admin_config_impl(
            FrontendDropAdminUpdateRequest::default(),
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

        // 4. Charlie discovers its own authorization via discover_available_drops
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
                drops: vec![finalize_status.descriptor.clone()],
            };
            charlie_state.save_contact(&mut s, alice_contact, Some(password)).unwrap();
        }

        let charlie_discovered = discover_available_drops_impl(
            Some(password.to_string()),
            &charlie_state,
        )
        .await
        .unwrap();

        assert_eq!(charlie_discovered.len(), 1);
        assert!(charlie_discovered[0].is_authorized, "Charlie should discover authorization on Server A!");

        // 5. Start Server E for redundant public drop upload
        let server_e = MockDropServer::start("d-server-e".to_string(), "2222222222222222222222222222222222222222222222222222222222222222".to_string()).await;
        let server_e_url = format!("{}/drop.php", server_e.base_url);

        // Bob probes Server E
        let e_info = probe_public_drop_impl(server_e_url.clone(), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
        assert_eq!(e_info.access_pubkey.len(), 64);

        // Authorize Bob's access pubkey on Server E
        server_e.members.lock().unwrap().push(e_info.access_pubkey.clone());

        // Bob saves Server E as a public drop
        save_public_drop_impl(
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
        let selected = set_selected_upload_drops_impl(
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
                drops: vec![],
            };
            bob_state.save_contact(&mut s, l1_contact, Some(password)).unwrap();
        }

        // Bob creates and saves his own contact card with a listing
        let save_req = FrontendSaveOwnCardRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                last_name: Some("Baker".to_string()),
                service_offer: Some("Fresh Bread".to_string()),
                ..Default::default()
            },
            listings: vec![FrontendCardListingInput {
                listing_type: FrontendListingType::Offer,
                title: "Sourdough Bread".to_string(),
                text: Some("Warm and crispy".to_string()),
                pricing: Some("5 M".to_string()),
                allow_inquiries: true,
            }],
            max_hops: 2,
            validity_days: Some(30),
        };
        save_own_contact_card_impl(save_req, Some(password.to_string()), &bob_state).unwrap();

        // Bob uploads redundantly to Drop A (selected) and Drop E (active public drop)
        let bob_sync = sync_market_drops_impl(
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
                drops: vec![finalize_status.descriptor.clone()],
            };
            l1_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        let l1_sync = sync_market_drops_impl(
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
        let bob_person = l1_market.iter().find(|p| p.did == can_bob_did);
        assert!(bob_person.is_some(), "Bob should be in L1 market view");
        let has_drop_source = bob_person.unwrap().sources.iter().any(|s| {
            matches!(s, crate::models::FrontendEntrySource::Drop { read_url } if read_url == &finalize_status.descriptor.read_url)
        });
        assert!(has_drop_source, "Bob's market person should have Drop entry source!");

        // 7. Unauthorized reader L2 gets 0 decrypted slots
        {
            let mut s = l2_state.service.lock().unwrap();
            let mut settings = l2_state.get_cached_settings().unwrap_or_default();
            settings.market_selected_drops = vec![finalize_status.descriptor.clone()];
            l2_state.save_settings(&mut s, settings, Some(password)).unwrap();
        }

        let l2_sync = sync_market_drops_impl(
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
            let (admin_sk, _, _) = humoco_market::derive_drop_admin_keypair(&seed, 0, 0);
            let bob_vk = human_money_core::crypto::get_pubkey_from_user_id(&bob_did).unwrap();
            humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &bob_vk).unwrap()
        };

        let block_status = block_drop_slot_impl(
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
        assert!(!server_a.slots.lock().unwrap().contains_key(&format!("{}.wot", bob_stealth_pk_a)), "WoT slot file should be purged on Server A");

        // 9. Admin rotates key to generation 1
        let rot_status = rotate_drop_admin_key_impl(
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(rot_status.generation, 1);
        assert_ne!(rot_status.admin_pubkey, finalize_status.admin_pubkey);
        assert!(rot_status.rotation_retire_at.is_some());

        // 10. Check overview status
        let overview = get_drop_overview_status_impl(
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        assert!(overview.has_own_drop);
        assert_eq!(overview.connected_count, 1);
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
        let _draft = prepare_drop_setup_impl(
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
        assert!(!preview_upgrade.can_write_my_drop_before);
        assert!(preview_upgrade.can_write_my_drop_after);
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
                drops: vec![],
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
        assert!(preview_downgrade.can_write_my_drop_before);
        assert!(!preview_downgrade.can_write_my_drop_after);
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
        let draft = prepare_drop_setup_impl(
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

        let server = MockDropServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;

        let finalize_status = finalize_drop_setup_impl(
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
            set.market_selected_drops = vec![finalize_status.descriptor.clone()];
            alice_state.save_settings(&mut s, set, Some(password)).unwrap();
        }
        {
            let mut s = bob_state.service.lock().unwrap();
            let mut set = bob_state.load_settings(&mut s, Some(password)).unwrap();
            set.market_selected_drops = vec![finalize_status.descriptor.clone()];
            bob_state.save_settings(&mut s, set, Some(password)).unwrap();
        }
        {
            let mut s = carol_state.service.lock().unwrap();
            let mut set = carol_state.load_settings(&mut s, Some(password)).unwrap();
            set.market_selected_drops = vec![finalize_status.descriptor.clone()];
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
                drops: vec![],
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
                drops: vec![],
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
                drops: vec![],
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
                drops: vec![],
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
                drops: vec![],
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
            let now = drop_client::current_unix_timestamp();
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
        let alice_sync_res = sync_market_drops_impl(
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
        let bob_sync_res = sync_market_drops_impl(
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
        let bob_idempotent_sync = sync_market_drops_impl(
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(bob_idempotent_sync.imported_wot_lists, 0, "Bob second sync before new uploads must be 0 imports");

        // 6. Carol (Class 2) Syncs (Downloads Alice's list with Selective Disclosure)
        let carol_sync_res = sync_market_drops_impl(
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
        let carol_idempotent_sync = sync_market_drops_impl(
            None,
            None,
            Some(password.to_string()),
            &carol_state,
        )
        .await
        .unwrap();
        assert_eq!(carol_idempotent_sync.imported_wot_lists, 0, "Carol second sync must be 0 imports");

        // 7. Alice second sync -> no new upload needed
        let alice_second_sync = sync_market_drops_impl(
            None,
            None,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();
        assert_eq!(alice_second_sync.uploaded_to, 0, "Alice second sync without changes must be no-op upload");

        // Bob now syncs to pick up Carol's newly uploaded list
        let bob_third_sync = sync_market_drops_impl(
            None,
            None,
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(bob_third_sync.imported_wot_lists, 1, "Bob should import Carol's list");

        // Bob fourth sync -> again fully idempotent
        let bob_fourth_sync = sync_market_drops_impl(
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
        let deleted = delete_own_drop_slots_impl(Some(password.to_string()), &alice_state)
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
            !remaining_slots.contains_key(&format!("{}.wot", alice_stealth)),
            "Alice's WOT slot must be deleted from server"
        );
        assert!(
            !remaining_slots.contains_key(&format!("{}.card", alice_stealth)),
            "Alice's card slot must be deleted from server"
        );

        // Verify drop_sent_log is cleared in Alice's register
        {
            let mut alice_s = alice_state.service.lock().unwrap();
            let alice_reg = alice_state.get_or_load_wot_register(&mut alice_s, Some(password)).unwrap();
            assert!(alice_reg.drop_sent_log.is_empty(), "Alice drop_sent_log must be cleared");
        }
    }
}
