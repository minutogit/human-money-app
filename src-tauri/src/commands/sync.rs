// src-tauri/src/commands/sync.rs
use tauri::State;

use crate::models::{
    FrontendDiscoveredSync, FrontendError, FrontendMarketSyncResult, FrontendMemberPolicyDto,
    FrontendPublicSyncInfo, FrontendSyncAdminStatus, FrontendSyncAdminUpdateRequest,
    FrontendSyncConnectionTestResult, FrontendSyncDescriptor, FrontendSyncOverviewStatus,
    FrontendSyncSetupDraft, FrontendTrustChangeImpact,
};
use crate::services::sync_engine;
use crate::AppState;

#[tauri::command]
pub async fn sync_channel_members(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, FrontendError> {
    sync_engine::sync_channel_members_impl(password.as_deref(), &state).await
}

#[tauri::command]
pub fn prepare_sync_setup(
    policy: Option<FrontendMemberPolicyDto>,
    max_members: Option<usize>,
    max_total_storage_mb: Option<usize>,
    max_user_storage_mb: Option<usize>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncSetupDraft, FrontendError> {
    sync_engine::prepare_sync_setup_impl(
        policy,
        max_members,
        max_total_storage_mb,
        max_user_storage_mb,
        password,
        &state,
    )
}

#[tauri::command]
pub fn get_rust_sync_config(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, FrontendError> {
    sync_engine::get_rust_sync_config_impl(password, &state)
}

#[tauri::command]
pub fn save_sync_setup_zip(
    file_path: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    sync_engine::save_sync_setup_zip_impl(file_path, password, &state)
}

#[tauri::command]
pub async fn finalize_sync_setup(
    base_url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    sync_engine::finalize_sync_setup_impl(base_url, password, &state).await
}

#[tauri::command]
pub async fn test_sync_connection(
    base_url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncConnectionTestResult, FrontendError> {
    sync_engine::test_sync_connection_impl(base_url, password, &state).await
}

#[tauri::command]
pub async fn get_sync_admin_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    sync_engine::get_sync_admin_status_impl(password, &state).await
}

#[tauri::command]
pub async fn update_sync_admin_config(
    update: FrontendSyncAdminUpdateRequest,
    push_now: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    sync_engine::update_sync_admin_config_impl(update, push_now, password, &state).await
}

#[tauri::command]
pub async fn block_sync_slot(
    pubkey_hex64: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncAdminStatus, FrontendError> {
    sync_engine::block_sync_slot_impl(pubkey_hex64, password, &state).await
}

#[tauri::command]
pub fn delete_sync_admin(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    sync_engine::delete_sync_admin_impl(password, &state)
}

#[tauri::command]
pub async fn revoke_own_sync_server(
    effective_at: Option<u64>,
    reason: Option<String>,
    replacement_write_url: Option<String>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<FrontendSyncAdminStatus>, FrontendError> {
    sync_engine::revoke_own_sync_server_impl(
        effective_at,
        reason,
        replacement_write_url,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub async fn discover_available_syncs(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendDiscoveredSync>, FrontendError> {
    sync_engine::discover_available_syncs_impl(password, &state).await
}

#[tauri::command]
pub async fn probe_contact_syncs(
    force: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    sync_engine::probe_contact_syncs_impl(force.unwrap_or(false), password, &state).await
}

#[tauri::command]
pub async fn set_selected_upload_syncs(
    syncs: Vec<FrontendSyncDescriptor>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<FrontendSyncDescriptor>, FrontendError> {
    sync_engine::set_selected_upload_syncs_impl(syncs, password, &state).await
}

#[tauri::command]
pub async fn sync_market_channels(
    is_wifi: Option<bool>,
    force_upload: Option<bool>,
    respect_cooldown: Option<bool>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendMarketSyncResult, FrontendError> {
    sync_engine::sync_market_channels_with_options(
        is_wifi,
        force_upload,
        respect_cooldown,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub async fn delete_own_sync_slots(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, FrontendError> {
    sync_engine::delete_own_sync_slots_impl(password, &state).await
}

#[tauri::command]
pub fn preview_contact_trust_change(
    did: String,
    new_level: u8,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendTrustChangeImpact, FrontendError> {
    sync_engine::preview_contact_trust_change_impl(did, new_level, password, &state)
}

#[tauri::command]
pub async fn probe_public_sync(
    url: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendPublicSyncInfo, FrontendError> {
    sync_engine::probe_public_sync_impl(url, password, &state).await
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
    sync_engine::save_public_sync_impl(
        write_url,
        read_url,
        drop_pub,
        register_url,
        enabled,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub async fn remove_public_sync(
    write_url: String,
    delete_remote_slots: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    sync_engine::remove_public_sync_impl(write_url, delete_remote_slots, password, &state).await
}

#[tauri::command]
pub fn get_sync_overview_status(
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendSyncOverviewStatus, FrontendError> {
    sync_engine::get_sync_overview_status_impl(password, &state)
}

#[tauri::command]
pub fn update_network_status(
    is_online: bool,
    is_wifi: bool,
    is_visible: bool,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    state.update_network_status(is_online, is_wifi, is_visible);
    Ok(())
}

#[tauri::command]
pub fn wake_sync_coordinator(
    scopes: crate::state::SyncWakeRequest,
    state: State<'_, AppState>,
) -> Result<(), FrontendError> {
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    state.wake_sync_coordinator(scopes);
    Ok(())
}
