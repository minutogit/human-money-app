// src-tauri/src/commands/drops.rs
use std::collections::BTreeMap;
use base64::Engine;
use tauri::State;

use crate::drop_client::{self, IndexFetchResult};
use crate::models::{
    FrontendDiscoveredDrop, FrontendDropAdminStatus, FrontendDropAdminUpdateRequest,
    FrontendDropConnectionTestResult, FrontendDropDescriptor, FrontendDropSetupDraft,
    FrontendDropSyncResult, FrontendError, FrontendMemberPolicyDto, FrontendTrustChangeImpact,
};
use crate::settings::{DROP_CARD_MAX_BYTES, DROP_HEARTBEAT_SECS, MAX_POLLED_DROPS};
use crate::state::{
    canonicalize_did, AppState, StoredDropAdmin, StoredDropSync, StoredOwnCard,
    MARKET_CARD_STORE_KEY, MARKET_DROP_ADMIN_KEY, MARKET_DROP_SYNC_KEY, MARKET_OWN_CARD_KEY,
};
use human_money_core::app_service::AppService;
use sha2::Digest;

/// Computes the members for the admin's drop for channel "card" along with stats and access IDs.
pub fn compute_admin_drop_members_with_stats(
    state: &AppState,
    service: &mut AppService,
    admin: &StoredDropAdmin,
    password: Option<&str>,
) -> Result<(humoco_market::DropMembersStats, Vec<String>), FrontendError> {
    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

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
        &card_policy,
        &allow_list,
        &deny_list,
        admin.max_members,
    );

    // Compute access IDs
    let drop_secret_vec = hex::decode(&admin.drop_secret_hex)
        .map_err(|e| FrontendError::from(format!("Invalid drop_secret_hex: {}", e)))?;
    let drop_secret: [u8; 32] = drop_secret_vec
        .try_into()
        .map_err(|_| FrontendError::from("drop_secret must be 32 bytes".to_string()))?;

    let blocked_set: std::collections::HashSet<&str> =
        admin.blocked_access_ids.iter().map(|s| s.as_str()).collect();

    let mut access_ids = Vec::with_capacity(stats.member_dids.len());
    for did in &stats.member_dids {
        if let Ok(pk) = human_money_core::crypto::get_pubkey_from_user_id(did) {
            let write_token = humoco_market::derive_write_token_admin(&drop_secret, &pk);
            let access_id = humoco_market::derive_access_id(&write_token);
            if !blocked_set.contains(access_id.as_str()) {
                access_ids.push(access_id);
            }
        }
    }
    access_ids.sort();
    access_ids.dedup();

    Ok((stats, access_ids))
}

pub fn prepare_drop_setup_impl(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropSetupDraft, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let (secret_bytes, drop_pub) = humoco_market::generate_drop_keypair(&mut rand::rngs::OsRng);
    let mut random_bytes = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut random_bytes);
    let data_dir = format!("d-{}", hex::encode(random_bytes));

    let admin_token = humoco_market::derive_admin_token(&secret_bytes);
    let admin_token_hash = humoco_market::admin_token_hash_hex(&admin_token);

    let member_policy = policy.unwrap_or_default();
    let max_mem = max_members.unwrap_or(300);

    let stored_admin = StoredDropAdmin {
        descriptor: humoco_market::DropDescriptor {
            write_url: String::new(),
            read_url: String::new(),
            drop_pub: drop_pub.clone(),
        },
        drop_secret_hex: hex::encode(secret_bytes),
        data_dir: data_dir.clone(),
        max_members: max_mem,
        policies: BTreeMap::from([("card".to_string(), member_policy.clone().into())]),
        allow: Vec::new(),
        deny: Vec::new(),
        blocked_access_ids: Vec::new(),
    };

    let bytes = serde_json::to_vec(&stored_admin)
        .map_err(|e| FrontendError::from(format!("Failed to serialize StoredDropAdmin: {}", e)))?;
    service
        .save_encrypted_data(MARKET_DROP_ADMIN_KEY, &bytes, password.as_deref())
        .map_err(FrontendError::from)?;

    let zip_bytes = crate::drop_zip::build_drop_zip_bytes(&admin_token_hash, &data_dir, max_mem);
    let zip_base64 = base64::engine::general_purpose::STANDARD.encode(&zip_bytes);

    Ok(FrontendDropSetupDraft {
        descriptor: stored_admin.descriptor.into(),
        data_dir,
        admin_token_hash,
        max_members: max_mem,
        policy: member_policy,
        zip_base64,
    })
}

#[tauri::command]
pub fn prepare_drop_setup(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropSetupDraft, FrontendError> {
    prepare_drop_setup_impl(policy, max_members, password, &state)
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

    let drop_secret_vec = hex::decode(&admin.drop_secret_hex)
        .map_err(|e| FrontendError::from(format!("Invalid drop_secret_hex: {}", e)))?;
    let drop_secret: [u8; 32] = drop_secret_vec
        .try_into()
        .map_err(|_| FrontendError::from("drop_secret must be 32 bytes".to_string()))?;

    let admin_token = humoco_market::derive_admin_token(&drop_secret);
    let admin_token_hash = humoco_market::admin_token_hash_hex(&admin_token);

    let zip_bytes = crate::drop_zip::build_drop_zip_bytes(
        &admin_token_hash,
        &admin.data_dir,
        admin.max_members,
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
    let base_url = base_url.trim().trim_end_matches('/').to_string();
    drop_client::validate_drop_url(&base_url).map_err(FrontendError::from)?;

    let mut admin = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        serde_json::from_slice::<StoredDropAdmin>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e)))?
    };

    let write_url = format!("{}/drop.php", base_url);
    let read_url = format!("{}/{}", base_url, admin.data_dir);
    admin.descriptor.write_url = write_url.clone();
    admin.descriptor.read_url = read_url.clone();

    let (stats, access_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_drop_members_with_stats(state, &mut service, &admin, password.as_deref())?
    };

    let drop_secret_vec = hex::decode(&admin.drop_secret_hex)
        .map_err(|e| FrontendError::from(format!("Invalid drop_secret_hex: {}", e)))?;
    let drop_secret: [u8; 32] = drop_secret_vec
        .try_into()
        .map_err(|_| FrontendError::from("drop_secret must be 32 bytes".to_string()))?;
    let admin_token = humoco_market::derive_admin_token(&drop_secret);
    let admin_token_hex = hex::encode(admin_token);

    let client = drop_client::create_drop_http_client();
    drop_client::push_members(&client, &write_url, &admin_token_hex, "card", &access_ids)
        .await
        .map_err(FrontendError::from)?;

    // Update settings: add own drop if under 3
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

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(FrontendDropAdminStatus {
        descriptor: admin.descriptor.into(),
        data_dir: admin.data_dir,
        max_members: admin.max_members,
        card_policy: card_policy.into(),
        allow: admin.allow,
        deny: admin.deny,
        blocked_access_ids: admin.blocked_access_ids,
        direct_member_count: stats.direct_count,
        hop2_member_count: stats.hop2_count,
        total_member_count: stats.total_count,
        active_slot_ids: access_ids,
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

    let admin_res = {
        let mut service = state.service.lock().unwrap();
        service
            .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)
            .and_then(|bytes| {
                serde_json::from_slice::<StoredDropAdmin>(&bytes).map_err(|e| {
                    FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e))
                })
            })
    };

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

    let (stats, access_ids) = {
        let mut service = state.service.lock().unwrap();
        match compute_admin_drop_members_with_stats(state, &mut service, &admin, password.as_deref()) {
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

    let drop_secret_vec = match hex::decode(&admin.drop_secret_hex) {
        Ok(v) => v,
        Err(e) => {
            return Ok(FrontendDropConnectionTestResult {
                success: false,
                message: format!("Invalid secret: {}", e),
                write_ok: false,
                read_ok: false,
            });
        }
    };
    let drop_secret: [u8; 32] = match drop_secret_vec.try_into() {
        Ok(a) => a,
        Err(_) => {
            return Ok(FrontendDropConnectionTestResult {
                success: false,
                message: "Secret must be 32 bytes".to_string(),
                write_ok: false,
                read_ok: false,
            });
        }
    };
    let admin_token = humoco_market::derive_admin_token(&drop_secret);
    let admin_token_hex = hex::encode(admin_token);

    let client = drop_client::create_drop_http_client();
    let write_url = format!("{}/drop.php", base_url);
    let read_url = format!("{}/{}", base_url, admin.data_dir);

    // 1. Test write
    if let Err(e) =
        drop_client::push_members(&client, &write_url, &admin_token_hex, "card", &access_ids).await
    {
        return Ok(FrontendDropConnectionTestResult {
            success: false,
            message: format!("Write test failed: {}", e),
            write_ok: false,
            read_ok: false,
        });
    }

    // 2. Test read
    match drop_client::get_members(&client, &read_url, "card").await {
        Ok(fetched_ids) => {
            if fetched_ids == access_ids {
                Ok(FrontendDropConnectionTestResult {
                    success: true,
                    message: format!(
                        "Connection verified successfully ({} active members).",
                        stats.total_count
                    ),
                    write_ok: true,
                    read_ok: true,
                })
            } else {
                Ok(FrontendDropConnectionTestResult {
                    success: false,
                    message: format!(
                        "Read members list mismatch: expected {} items, got {}",
                        access_ids.len(),
                        fetched_ids.len()
                    ),
                    write_ok: true,
                    read_ok: false,
                })
            }
        }
        Err(e) => Ok(FrontendDropConnectionTestResult {
            success: false,
            message: format!("Read test failed: {}", e),
            write_ok: true,
            read_ok: false,
        }),
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

pub fn get_drop_admin_status_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendDropAdminStatus>, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let admin: StoredDropAdmin = match service.load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref()) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(a) => a,
            Err(_) => return Ok(None),
        },
        Err(_) => return Ok(None),
    };

    let (stats, access_ids) =
        compute_admin_drop_members_with_stats(state, &mut service, &admin, password.as_deref())?;

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(Some(FrontendDropAdminStatus {
        descriptor: admin.descriptor.into(),
        data_dir: admin.data_dir,
        max_members: admin.max_members,
        card_policy: card_policy.into(),
        allow: admin.allow,
        deny: admin.deny,
        blocked_access_ids: admin.blocked_access_ids,
        direct_member_count: stats.direct_count,
        hop2_member_count: stats.hop2_count,
        total_member_count: stats.total_count,
        active_slot_ids: access_ids,
    }))
}

#[tauri::command]
pub fn get_drop_admin_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<FrontendDropAdminStatus>, FrontendError> {
    get_drop_admin_status_impl(password, &state)
}

pub async fn update_drop_admin_config_impl(
    update: FrontendDropAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    let mut admin = {
        let mut service = state.service.lock().unwrap();
        let bytes = service
            .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
            .map_err(FrontendError::from)?;
        serde_json::from_slice::<StoredDropAdmin>(&bytes)
            .map_err(|e| FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e)))?
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
    if let Some(blocked) = update.blocked_access_ids {
        admin.blocked_access_ids = blocked;
    }
    if let Some(m) = update.max_members {
        admin.max_members = m;
    }

    let (stats, access_ids) = {
        let mut service = state.service.lock().unwrap();
        compute_admin_drop_members_with_stats(state, &mut service, &admin, password.as_deref())?
    };

    if push_now && !admin.descriptor.write_url.is_empty() {
        let drop_secret_vec = hex::decode(&admin.drop_secret_hex)
            .map_err(|e| FrontendError::from(format!("Invalid drop_secret_hex: {}", e)))?;
        let drop_secret: [u8; 32] = drop_secret_vec
            .try_into()
            .map_err(|_| FrontendError::from("drop_secret must be 32 bytes".to_string()))?;
        let admin_token = humoco_market::derive_admin_token(&drop_secret);
        let admin_token_hex = hex::encode(admin_token);

        let client = drop_client::create_drop_http_client();
        drop_client::push_members(
            &client,
            &admin.descriptor.write_url,
            &admin_token_hex,
            "card",
            &access_ids,
        )
        .await
        .map_err(FrontendError::from)?;
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

    let card_policy = admin
        .policies
        .get("card")
        .cloned()
        .unwrap_or_default();

    Ok(FrontendDropAdminStatus {
        descriptor: admin.descriptor.into(),
        data_dir: admin.data_dir,
        max_members: admin.max_members,
        card_policy: card_policy.into(),
        allow: admin.allow,
        deny: admin.deny,
        blocked_access_ids: admin.blocked_access_ids,
        direct_member_count: stats.direct_count,
        hop2_member_count: stats.hop2_count,
        total_member_count: stats.total_count,
        active_slot_ids: access_ids,
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
    access_id: String,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    let update = FrontendDropAdminUpdateRequest {
        blocked_access_ids: Some({
            let mut service = state.service.lock().unwrap();
            let admin: StoredDropAdmin = service
                .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
                .map_err(FrontendError::from)
                .and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|e| {
                        FrontendError::from(format!("Failed to parse StoredDropAdmin: {}", e))
                    })
                })?;
            let mut blocked = admin.blocked_access_ids;
            if !blocked.contains(&access_id) {
                blocked.push(access_id);
            }
            blocked
        }),
        ..Default::default()
    };

    update_drop_admin_config_impl(update, true, password, state).await
}

#[tauri::command]
pub async fn block_drop_slot(
    access_id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendDropAdminStatus, FrontendError> {
    block_drop_slot_impl(access_id, password, &state).await
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

    let client = drop_client::create_drop_http_client();
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

        let is_authorized = match humoco_market::derive_write_token_member(&my_ed_sk, &desc.drop_pub) {
            Ok(token) => {
                let access_id = humoco_market::derive_access_id(&token);
                match drop_client::get_members(&client, &desc.read_url, "card").await {
                    Ok(ids) => ids.contains(&access_id),
                    Err(_) => false,
                }
            }
            Err(_) => false,
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
        if bs58::decode(&d.drop_pub).into_vec().is_err() {
            return Err(FrontendError::from(format!("Invalid base58 drop_pub: {}", d.drop_pub)));
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
    let settings = {
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
            errors: Vec::new(),
        });
    }

    let client = drop_client::create_drop_http_client();
    let mut errors = Vec::new();
    let mut uploaded_to = 0;
    let mut polled_drops = 0;
    let mut decrypted_slots = 0;
    let mut imported_cards = 0;

    let now_secs = chrono::Utc::now().timestamp().max(0) as u64;

    // 1. Ensure own DropAdmin members are up-to-date on server
    {
        let admin_opt: Option<StoredDropAdmin> = {
            let mut service = state.service.lock().unwrap();
            service
                .load_encrypted_data(MARKET_DROP_ADMIN_KEY, password.as_deref())
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        };

        if let Some(admin) = admin_opt {
            if !admin.descriptor.write_url.is_empty() {
                let res = {
                    let mut service = state.service.lock().unwrap();
                    compute_admin_drop_members_with_stats(state, &mut service, &admin, password.as_deref())
                };
                if let Ok((_stats, access_ids)) = res {
                    if let Ok(drop_secret_vec) = hex::decode(&admin.drop_secret_hex) {
                        if let Ok(drop_secret) = drop_secret_vec.try_into() {
                            let admin_token = humoco_market::derive_admin_token(&drop_secret);
                            let admin_token_hex = hex::encode(admin_token);
                            let _ = drop_client::push_members(
                                &client,
                                &admin.descriptor.write_url,
                                &admin_token_hex,
                                "card",
                                &access_ids,
                            )
                            .await;
                        }
                    }
                }
            }
        }
    }

    // 2. Upload check
    let (my_ed_sk, own_card_opt, readers, readers_hash) = {
        let mut service = state.service.lock().unwrap();
        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;

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

        (my_ed_sk, own_card, reader_keys, readers_hash)
    };

    let mut sync_state: StoredDropSync = {
        let mut service = state.service.lock().unwrap();
        service
            .load_encrypted_data(MARKET_DROP_SYNC_KEY, password.as_deref())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    };

    if let Some(ref own_card) = own_card_opt {
        if now_secs < own_card.card.valid_until && !settings.market_selected_drops.is_empty() {
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
                        for drop_desc in &settings.market_selected_drops {
                            match humoco_market::derive_write_token_member(&my_ed_sk, &drop_desc.drop_pub) {
                                Ok(token) => {
                                    let token_hex = hex::encode(token);
                                    match drop_client::put_slot(
                                        &client,
                                        &drop_desc.write_url,
                                        &token_hex,
                                        "card",
                                        &blob,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            uploaded_to += 1;
                                        }
                                        Err(e) => {
                                            errors.push(format!("Upload to {}: {}", drop_desc.write_url, e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    errors.push(format!("Token derivation for {}: {}", drop_desc.write_url, e));
                                }
                            }
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

    // 3. Download poll
    let candidate_read_urls = {
        let mut service = state.service.lock().unwrap();
        let mut urls = Vec::new();

        // Own selected drops
        for d in &settings.market_selected_drops {
            if !d.read_url.is_empty() {
                urls.push(d.read_url.clone());
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
                            urls.push(d.read_url);
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
                            urls.push(d.read_url.clone());
                        }
                    }
                }
            }
        }

        let mut deduped = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for u in urls {
            if seen.insert(u.clone()) {
                deduped.push(u);
                if deduped.len() >= MAX_POLLED_DROPS {
                    break;
                }
            }
        }
        deduped
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

    for read_url in candidate_read_urls {
        polled_drops += 1;
        let cached_etag = sync_state.etags_by_read_url.get(&read_url).map(|s| s.as_str());
        let cached_lm = sync_state.last_modified_by_read_url.get(&read_url).map(|s| s.as_str());

        match drop_client::list_index(&client, &read_url, cached_etag, cached_lm).await {
            Ok(IndexFetchResult::NotModified) => {
                // No changes on this drop
            }
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
                    if entry.channel != "card" {
                        continue;
                    }
                    let mtime_key = format!("{}:{}", read_url, entry.id);
                    if sync_state.slot_mtimes.get(&mtime_key) == Some(&entry.mtime) {
                        continue;
                    }

                    match drop_client::get_slot(&client, &read_url, &entry.id, "card").await {
                        Ok(blob) => {
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
                                Ok(None) => {
                                    // Not an authorized recipient for this slot
                                }
                                Err(e) => {
                                    errors.push(format!("Error opening slot {}: {}", entry.id, e));
                                }
                            }
                        }
                        Err(e) => {
                            errors.push(format!("Failed to get slot {}: {}", entry.id, e));
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
    let (my_ed_sk, drops) = {
        let mut service = state.service.lock().unwrap();
        let my_ed_sk = service
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .map_err(FrontendError::from)?;
        let settings = state
            .get_cached_settings()
            .or_else(|_| state.load_settings(&mut service, password.as_deref()))
            .unwrap_or_default();
        (my_ed_sk, settings.market_selected_drops)
    };

    let client = drop_client::create_drop_http_client();
    let mut deleted_count = 0;

    for drop_desc in &drops {
        if let Ok(token) = humoco_market::derive_write_token_member(&my_ed_sk, &drop_desc.drop_pub) {
            let token_hex = hex::encode(token);
            if drop_client::delete_slot(&client, &drop_desc.write_url, &token_hex, "card")
                .await
                .is_ok()
            {
                deleted_count += 1;
            }
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

    Ok(FrontendTrustChangeImpact {
        can_read_my_market_before,
        can_read_my_market_after,
        can_write_my_drop_before,
        can_write_my_drop_after,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use crate::models::{
        FrontendCardListingInput, FrontendContact, FrontendListingType,
        FrontendSaveOwnCardRequest, FrontendUserProfile,
    };
    use crate::commands::market::{get_market_view_impl, save_own_contact_card_impl};

    const ALICE_PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const BOB_PHRASE: &str =
        "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
    const CHARLIE_PHRASE: &str =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";
    const READER1_PHRASE: &str =
        "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
    const READER2_PHRASE: &str =
        "vessel ladder alter error federal sibling chat ability sun glass valve picture";

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("humoco_drops_test_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
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

    fn create_test_state(service: AppService) -> AppState {
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

    struct MockDropServer {
        pub base_url: String,
        pub _data_dir: String,
        pub members: Arc<Mutex<Vec<String>>>,
        pub slots: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        _shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl MockDropServer {
        pub async fn start(data_dir: String, admin_token_hash: String) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let base_url = format!("http://127.0.0.1:{}", port);

            let members = Arc::new(Mutex::new(Vec::<String>::new()));
            let slots = Arc::new(Mutex::new(HashMap::<String, Vec<u8>>::new()));
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

            let s_data_dir = data_dir.clone();
            let s_admin_hash = admin_token_hash.clone();
            let s_members = Arc::clone(&members);
            let s_slots = Arc::clone(&slots);

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = &mut shutdown_rx => {
                            break;
                        }
                        Ok((mut stream, _)) = listener.accept() => {
                            let data_dir = s_data_dir.clone();
                            let admin_hash = s_admin_hash.clone();
                            let members = Arc::clone(&s_members);
                            let slots = Arc::clone(&s_slots);

                            tokio::spawn(async move {
                                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                                let mut buf = vec![0u8; 131072];
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

                                let response: Vec<u8> = if method == "PUT" && path.starts_with("/drop.php") && path.contains("members") {
                                    let admin_token = headers.get("x-admin-token").cloned().unwrap_or_default();
                                    let mut hasher = sha2::Sha256::new();
                                    if let Ok(token_bytes) = hex::decode(&admin_token) {
                                        hasher.update(&token_bytes);
                                        let calc_hash = hex::encode(hasher.finalize());
                                        if calc_hash == admin_hash {
                                            #[derive(serde::Deserialize)]
                                            struct MembersJson {
                                                ids: Vec<String>,
                                            }
                                            if let Ok(mj) = serde_json::from_slice::<MembersJson>(&body) {
                                                let mut m = members.lock().unwrap();
                                                *m = mj.ids.clone();
                                                let mut s = slots.lock().unwrap();
                                                s.retain(|k, _| {
                                                    if let Some((id, _)) = k.split_once('.') {
                                                        m.contains(&id.to_string())
                                                    } else {
                                                        false
                                                    }
                                                });
                                                b"HTTP/1.1 204 No Content\r\n\r\n".to_vec()
                                            } else {
                                                b"HTTP/1.1 400 Bad Request\r\n\r\n".to_vec()
                                            }
                                        } else {
                                            b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                                        }
                                    } else {
                                        b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                                    }
                                } else if method == "PUT" && path.starts_with("/drop.php") {
                                    let drop_token = headers.get("x-drop-token").cloned().unwrap_or_default();
                                    let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                    if let Ok(raw_bytes) = hex::decode(&drop_token) {
                                        let access_id = &hex::encode(sha2::Sha256::digest(&raw_bytes))[..32];
                                        let m = members.lock().unwrap();
                                        if m.contains(&access_id.to_string()) {
                                            let clean_body: String = String::from_utf8_lossy(&body)
                                                .chars()
                                                .filter(|c| !c.is_whitespace())
                                                .collect();
                                            if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(clean_body.as_bytes()) {
                                                let mut s = slots.lock().unwrap();
                                                s.insert(format!("{}.{}.bin", access_id, channel), decoded);
                                                b"HTTP/1.1 204 No Content\r\n\r\n".to_vec()
                                            } else {
                                                b"HTTP/1.1 400 Bad Request\r\n\r\n".to_vec()
                                            }
                                        } else {
                                            b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                                        }
                                    } else {
                                        b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                                    }
                                } else if method == "DELETE" && path.starts_with("/drop.php") {
                                    let drop_token = headers.get("x-drop-token").cloned().unwrap_or_default();
                                    let channel = headers.get("x-channel").cloned().unwrap_or_else(|| "card".to_string());
                                    if let Ok(raw_bytes) = hex::decode(&drop_token) {
                                        let access_id = &hex::encode(sha2::Sha256::digest(&raw_bytes))[..32];
                                        let mut s = slots.lock().unwrap();
                                        s.remove(&format!("{}.{}.bin", access_id, channel));
                                        b"HTTP/1.1 204 No Content\r\n\r\n".to_vec()
                                    } else {
                                        b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                                    }
                                } else if method == "GET" && path.contains(&format!("/{}/index.json", data_dir)) {
                                    let s = slots.lock().unwrap();
                                    let mut entries = Vec::new();
                                    for (k, v) in s.iter() {
                                        if let Some((id, rest)) = k.split_once('.') {
                                            if let Some((ch, _)) = rest.split_once('.') {
                                                entries.push(serde_json::json!({
                                                    "id": id,
                                                    "channel": ch,
                                                    "mtime": 1769940000,
                                                    "size": v.len()
                                                }));
                                            }
                                        }
                                    }
                                    let payload = serde_json::to_vec(&entries).unwrap();
                                    let resp_header = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"etag-1\"\r\nContent-Length: {}\r\n\r\n",
                                        payload.len()
                                    );
                                    let mut resp_bytes = resp_header.into_bytes();
                                    resp_bytes.extend(payload);
                                    resp_bytes
                                } else if method == "GET" && path.contains(&format!("/{}/members.", data_dir)) {
                                    let m = members.lock().unwrap();
                                    let payload = serde_json::to_vec(&serde_json::json!({ "ids": *m })).unwrap();
                                    let resp_header = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                                        payload.len()
                                    );
                                    let mut resp_bytes = resp_header.into_bytes();
                                    resp_bytes.extend(payload);
                                    resp_bytes
                                } else if method == "GET" && path.contains(&format!("/{}/", data_dir)) && path.ends_with(".bin") {
                                    let file_name = path.rsplit('/').next().unwrap_or("");
                                    let s = slots.lock().unwrap();
                                    if let Some(bytes) = s.get(file_name) {
                                        let resp_header = format!(
                                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                                            bytes.len()
                                        );
                                        let mut resp_bytes = resp_header.into_bytes();
                                        resp_bytes.extend(bytes.clone());
                                        resp_bytes
                                    } else {
                                        b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                                    }
                                } else {
                                    b"HTTP/1.1 404 Not Found\r\n\r\n".to_vec()
                                };

                                let _ = stream.write_all(&response).await;
                            });
                        }
                    }
                }
            });

            Self {
                base_url,
                _data_dir: data_dir,
                members,
                slots,
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
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();

        let server_a = MockDropServer::start(draft.data_dir.clone(), draft.admin_token_hash.clone()).await;

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

        // Admin A authorizes B directly and C via B's SD-JWT trust list (without invitation!)
        assert_eq!(updated_status.direct_member_count, 2); // Alice + Bob
        assert_eq!(updated_status.hop2_member_count, 1);   // Charlie
        assert_eq!(updated_status.total_member_count, 3);
        assert_eq!(server_a.members.lock().unwrap().len(), 3);

        // 4. Charlie discovers its own authorization via discover_available_drops
        // Add Server A's descriptor to Charlie's contact for Alice
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

        // Charlie cannot grant rights to others (only Alice has admin token)
        let fake_push = drop_client::push_members(
            &drop_client::create_drop_http_client(),
            &finalize_status.descriptor.write_url,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "card",
            &["0123456789abcdef0123456789abcdef".to_string()],
        )
        .await;
        assert!(fake_push.is_err(), "Non-admin cannot push members to drop!");

        // 5. Start Server E for redundant upload
        let (server_e_secret, server_e_pub) = humoco_market::generate_drop_keypair(&mut rand::rngs::OsRng);
        let server_e_token = humoco_market::derive_admin_token(&server_e_secret);
        let server_e_hash = humoco_market::admin_token_hash_hex(&server_e_token);
        let server_e = MockDropServer::start("d-server-e".to_string(), server_e_hash).await;

        // Authorize Bob on Server E
        let bob_pk = human_money_core::crypto::get_pubkey_from_user_id(&bob_did).unwrap();
        let bob_token_e = humoco_market::derive_write_token_admin(&server_e_secret, &bob_pk);
        let bob_access_id_e = humoco_market::derive_access_id(&bob_token_e);
        server_e.members.lock().unwrap().push(bob_access_id_e);

        let drop_e_desc = FrontendDropDescriptor {
            write_url: format!("{}/drop.php", server_e.base_url),
            read_url: format!("{}/d-server-e", server_e.base_url),
            drop_pub: server_e_pub,
        };

        // Bob configures selected upload targets: Drop A and Drop E
        let selected = set_selected_upload_drops_impl(
            vec![finalize_status.descriptor.clone(), drop_e_desc.clone()],
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(selected.len(), 2);

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
            }],
            max_hops: 2,
            validity_days: Some(30),
        };
        save_own_contact_card_impl(save_req, Some(password.to_string()), &bob_state).unwrap();

        // B uploads redundantly to Drop A and Drop E
        let bob_sync = sync_market_drops_impl(
            None,
            Some(true),
            Some(password.to_string()),
            &bob_state,
        )
        .await
        .unwrap();
        assert_eq!(bob_sync.uploaded_to, 2, "Bob should upload to both Drop A and Drop E");
        assert_eq!(server_a.slots.lock().unwrap().len(), 1);
        assert_eq!(server_e.slots.lock().unwrap().len(), 1);

        // 6. Authorized reader L1 decrypts B's slot and imports cards
        // Give L1 Server A's descriptor in contacts
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
        assert_eq!(l1_sync.decrypted_slots, 1, "Authorized reader L1 should decrypt slot");
        assert_eq!(l1_sync.imported_cards, 1, "Authorized reader L1 should import card");

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

        // 8. Admin revoking B or blocking B's access_id removes B from members.card.json and deletes B's slot
        let bob_access_id_a = {
            let admin_stored: StoredDropAdmin = {
                let mut s = alice_state.service.lock().unwrap();
                let b = s.load_encrypted_data(MARKET_DROP_ADMIN_KEY, Some(password)).unwrap();
                serde_json::from_slice(&b).unwrap()
            };
            let drop_secret_arr: [u8; 32] = hex::decode(&admin_stored.drop_secret_hex).unwrap().try_into().unwrap();
            let write_token = humoco_market::derive_write_token_admin(&drop_secret_arr, &bob_pk);
            humoco_market::derive_access_id(&write_token)
        };

        let block_status = block_drop_slot_impl(
            bob_access_id_a.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap();

        assert!(block_status.blocked_access_ids.contains(&bob_access_id_a));
        assert!(!block_status.active_slot_ids.contains(&bob_access_id_a));
        assert!(!server_a.members.lock().unwrap().contains(&bob_access_id_a));
        assert!(!server_a.slots.lock().unwrap().contains_key(&format!("{}.card.bin", bob_access_id_a)), "Slot file should be purged on Server A");
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
        assert!(preview_downgrade.can_write_my_drop_before);
        assert!(!preview_downgrade.can_write_my_drop_after);
    }
}
