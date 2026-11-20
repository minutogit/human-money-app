use crate::models::{FrontendError, MnemonicLanguage, ProfileInfo};
use crate::pin_keystore::{self, ProfilePinStatusDto, MAX_PIN_ATTEMPTS};
use crate::services::fs_helpers::{
    load_profile_metadata, load_profile_metadata_map_from_dir, record_profile_metadata,
    resolve_or_create_local_instance_id,
};
use crate::settings::{
    InternalLifecycleState, SecurityPolicy, UiPreferences, INTERNAL_LIFECYCLE_STATE_KEY,
    SECURITY_POLICY_KEY, UI_PREFERENCES_KEY,
};
use crate::state::{PendingProfileLock, RequiredAccess};
use crate::AppState;
use chrono::{DateTime, Utc};
use log::{error, info};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[tauri::command]
pub fn list_profiles(
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<Vec<ProfileInfo>, FrontendError> {
    info!("Listing available profiles...");
    let service = state.service.lock().unwrap();

    // Load profile metadata for last_used timestamps without mutating disk on read errors
    let metadata = match load_profile_metadata(&app) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Ignoring unreadable profile metadata for listing: {}", e);
            HashMap::new()
        }
    };

    service
        .list_profiles()
        .map_err(FrontendError::from)
        .map(|profiles| {
            let mut profile_infos: Vec<ProfileInfo> = profiles
                .into_iter()
                .map(|p| {
                    let meta = metadata.get(&p.folder_name);
                    ProfileInfo {
                        profile_name: p.profile_name,
                        folder_name: p.folder_name,
                        last_used: meta.map(|m| m.last_used.clone()),
                        user_id: meta.and_then(|m| m.user_id.clone()),
                    }
                })
                .collect();

            // Sort by last_used timestamp (most recent first)
            profile_infos.sort_by(|a, b| {
                match (&a.last_used, &b.last_used) {
                    (Some(a_time), Some(b_time)) => {
                        // Parse timestamps and compare (reverse order for most recent first)
                        match (
                            a_time.parse::<DateTime<Utc>>(),
                            b_time.parse::<DateTime<Utc>>(),
                        ) {
                            (Ok(a_dt), Ok(b_dt)) => b_dt.cmp(&a_dt),
                            _ => std::cmp::Ordering::Equal,
                        }
                    }
                    (Some(_), None) => std::cmp::Ordering::Less, // Profiles with timestamp come first
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });

            profile_infos
        })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_profile(
    profile_name: String,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    password: String,
    local_instance_id: String,
    language: MnemonicLanguage,
    retain_mnemonic: Option<bool>,
    initial_reach_preset: Option<String>,
    initial_sync_targets: Option<Vec<crate::models::ResolvedSyncTargetDto>>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, FrontendError> {
    info!(
        "Attempting to create profile '{}' with language {:?}...",
        profile_name, language
    );
    let created_folder_name = {
        let mut service = state.service.lock().unwrap();
        let existing_folders: std::collections::HashSet<String> = service
            .list_profiles()
            .map(|list| list.into_iter().map(|p| p.folder_name).collect())
            .unwrap_or_default();
        let core_language = language.into();
        let retain = retain_mnemonic.unwrap_or(true);
        let cleaned_prefix = user_prefix
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match service.create_profile(
            &profile_name,
            &mnemonic,
            passphrase.as_deref(),
            cleaned_prefix,
            &password,
            core_language,
            local_instance_id,
            retain,
        ) {
            Ok(()) => {
                info!("Profile created successfully!");

                let folder_name = service.list_profiles().ok().and_then(|profiles| {
                    profiles
                        .iter()
                        .find(|p| !existing_folders.contains(&p.folder_name))
                        .or_else(|| profiles.iter().find(|p| p.profile_name == profile_name))
                        .map(|p| p.folder_name.clone())
                });
                let pending_lock = if let Some(ref folder) = folder_name {
                    state.acquire_profile_lock(folder)?
                } else {
                    PendingProfileLock::None
                };
                let user_id = service
                    .with_wallet_and_identity(|_, id| id.user_id.clone())
                    .ok();

                if let Some(ref folder) = folder_name {
                    if let Err(e) = record_profile_metadata(&app, folder, user_id) {
                        error!("Failed to update profile metadata: {}", e);
                    }
                }

                // 1. Initialize and save split default configuration stores for the new profile
                crate::state::save_encrypted_json(
                    &mut service,
                    UI_PREFERENCES_KEY,
                    &UiPreferences::default(),
                    Some(&password),
                )
                .map_err(|e| {
                    FrontendError::from(format!("Failed to save default uiPreferences: {}", e))
                })?;
                crate::state::save_encrypted_json(
                    &mut service,
                    SECURITY_POLICY_KEY,
                    &SecurityPolicy::default(),
                    Some(&password),
                )
                .map_err(|e| {
                    FrontendError::from(format!("Failed to save default securityPolicy: {}", e))
                })?;
                crate::state::save_local_sidecar_json(
                    &mut service,
                    INTERNAL_LIFECYCLE_STATE_KEY,
                    &InternalLifecycleState::default(),
                    Some(&password),
                )
                .map_err(|e| {
                    FrontendError::from(format!("Failed to save default lifecycle state: {}", e))
                })?;

                // 2. Initialize session and cache
                state
                    .initialize_profile_session(&mut service, &password)
                    .map_err(FrontendError::from)?;
                state.commit_profile_lock(pending_lock);
                folder_name.unwrap_or_default()
            }
            Err(e) => {
                error!("Profile creation failed: {}", e);
                return Err(FrontendError::from(e));
            }
        }
    };

    if let Some(preset) = initial_reach_preset {
        if matches!(preset.as_str(), "near" | "medium" | "far") {
            crate::services::settings_service::apply_reach_preset_impl(
                preset,
                Some(password.clone()),
                &state,
            )?;
        }
    }

    if let Some(targets) = initial_sync_targets {
        for target in targets {
            let _ = crate::services::sync_engine::save_public_sync_impl(
                target.write_url,
                target.read_url,
                target.drop_pub,
                target.register_url,
                true,
                Some(password.clone()),
                &state,
            )
            .await;
        }
    }

    Ok(created_folder_name)
}

#[tauri::command]
pub fn login(
    folder_name: String,
    password: String,
    cleanup_on_login: bool,
    local_instance_id: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    info!("Attempting to login...");
    let pending_lock = state.acquire_profile_lock(&folder_name)?;
    let mut service = state.service.lock().unwrap();
    match service.login(&folder_name, &password, cleanup_on_login, local_instance_id) {
        Ok(()) => {
            info!("Login successful!");

            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            if let Ok(config_dir) = app.path().app_config_dir() {
                let _ = pin_keystore::reset_failed_attempts(&config_dir, &folder_name);
            }
            if let Err(e) = record_profile_metadata(&app, &folder_name, user_id) {
                error!("Failed to update profile metadata: {}", e);
            }

            state
                .initialize_profile_session(&mut service, &password)
                .map_err(FrontendError::from)?;
            drop(service);
            state.commit_profile_lock(pending_lock);

            crate::services::common::spawn_pending_l2_reconciliation(&app, &password);

            Ok(())
        }
        Err(e) => {
            error!("Login failed: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn recover_wallet_and_set_new_password(
    folder_name: String,
    mnemonic: String,
    passphrase: Option<String>,
    new_password: String,
    local_instance_id: String,
    language: MnemonicLanguage,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    info!(
        "Attempting to recover wallet and set new password with language {:?}...",
        language
    );
    let pending_lock = state.acquire_profile_lock(&folder_name)?;
    let mut service = state.service.lock().unwrap();
    let core_language = language.into();
    match service.recover_wallet_and_set_new_password(
        &folder_name,
        &mnemonic,
        passphrase.as_deref(),
        &new_password,
        core_language,
        local_instance_id,
    ) {
        Ok(()) => {
            info!("Wallet recovered and new password set successfully!");

            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            if let Ok(config_dir) = app.path().app_config_dir() {
                let _ = pin_keystore::remove_entry(&config_dir, &folder_name);
            }
            if let Err(e) = record_profile_metadata(&app, &folder_name, user_id) {
                error!("Failed to update profile metadata: {}", e);
            }

            state
                .initialize_profile_session(&mut service, &new_password)
                .map_err(FrontendError::from)?;
            drop(service);
            state.commit_profile_lock(pending_lock);

            crate::services::common::spawn_pending_l2_reconciliation(&app, &new_password);

            Ok(())
        }
        Err(e) => {
            error!("Wallet recovery failed: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

#[tauri::command]
pub fn handover_to_this_device(
    folder_name: String,
    password: String,
    local_instance_id: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    info!(
        "Attempting device handover for profile in folder '{}'...",
        folder_name
    );
    let pending_lock = state.acquire_profile_lock(&folder_name)?;
    let mut service = state.service.lock().unwrap();
    match service.handover_to_this_device(&folder_name, &password, local_instance_id) {
        Ok(()) => {
            info!("Handover successful! Device bound to wallet.");

            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            if let Ok(config_dir) = app.path().app_config_dir() {
                let _ = pin_keystore::reset_failed_attempts(&config_dir, &folder_name);
            }
            if let Err(e) = record_profile_metadata(&app, &folder_name, user_id) {
                error!("Failed to update profile metadata: {}", e);
            }

            state
                .initialize_profile_session(&mut service, &password)
                .map_err(FrontendError::from)?;
            drop(service);
            state.commit_profile_lock(pending_lock);

            crate::services::common::spawn_pending_l2_reconciliation(&app, &password);

            Ok(())
        }
        Err(e) => {
            error!("Handover failed: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

pub fn logout_impl(state: &AppState) {
    info!("Logging out...");
    {
        let mut service = state.service.lock().unwrap();
        service.logout();
        // Clear cached profile data while still holding the service lock (`service -> profile_caches`)
        // so no concurrent request can repopulate caches after logout starts.
        state.clear_all_caches();
    }
    state.release_profile_lock();
    state.clear_sync_cooldown();
    info!("Logout complete.");
}

#[tauri::command]
pub fn logout(state: tauri::State<AppState>) {
    logout_impl(&state);
}

// NEUE SESSION COMMANDS

#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusDto {
    pub is_unlocked: bool,
    pub has_cached_session: bool,
    pub access_mode: String,
    pub base_access_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub one_shot_grant: Option<String>,
}

pub fn build_session_status(
    service: &human_money_core::app_service::AppService,
) -> SessionStatusDto {
    if !service.is_wallet_unlocked() {
        return SessionStatusDto {
            is_unlocked: false,
            has_cached_session: false,
            access_mode: "locked".to_string(),
            base_access_mode: "locked".to_string(),
            one_shot_grant: None,
        };
    }
    let access_mode = service
        .get_access_mode()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|_| "locked".to_string());
    let base_access_mode = service
        .get_base_access_mode()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|_| "locked".to_string());
    SessionStatusDto {
        is_unlocked: true,
        has_cached_session: service.is_session_active(),
        access_mode,
        base_access_mode,
        one_shot_grant: None,
    }
}

pub fn get_session_status_impl(state: &AppState) -> Result<SessionStatusDto, FrontendError> {
    let mut service = state.lock_service()?;
    state.sync_if_core_locked(&service);
    if service.is_wallet_unlocked() && !service.is_session_active() {
        let _ = service.refresh_session_activity();
    }
    Ok(build_session_status(&service))
}

#[tauri::command]
pub fn get_session_status(
    state: tauri::State<AppState>,
) -> Result<SessionStatusDto, FrontendError> {
    get_session_status_impl(&state)
}

fn resolve_policy_session_timeout(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    credential: &str,
) -> u64 {
    state
        .get_cached_security_policy()
        .map(|p| p.session_timeout_seconds)
        .or_else(|_| {
            state
                .get_or_load_security_policy(service, Some(credential))
                .map(|p| p.session_timeout_seconds)
        })
        .unwrap_or(0)
}

pub fn unlock_session_impl(
    password: &str,
    state: &AppState,
) -> Result<SessionStatusDto, FrontendError> {
    let mut service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
    let timeout_seconds = resolve_policy_session_timeout(state, &mut service, password);
    info!("Unlocking session for {} seconds...", timeout_seconds);
    service
        .unlock_session(password, timeout_seconds)
        .map_err(FrontendError::from)?;
    state.clear_one_shot_pin_grant();
    if timeout_seconds > 0 {
        state.notify_sync_coordinator();
    }
    Ok(build_session_status(&service))
}

#[tauri::command]
pub fn unlock_session(
    password: String,
    state: tauri::State<AppState>,
) -> Result<SessionStatusDto, FrontendError> {
    unlock_session_impl(&password, &state)
}

#[tauri::command]
pub fn lock_session(state: tauri::State<AppState>) {
    info!("Locking session manually.");
    let mut service = state.service.lock().unwrap();
    service.lock_session();
    state.clear_one_shot_pin_grant();
}

#[tauri::command]
pub fn refresh_session_activity(state: tauri::State<AppState>) {
    // Loggt nicht, um Spam zu vermeiden
    let mut service = state.service.lock().unwrap();
    let _ = service.refresh_session_activity();
}

#[tauri::command]
pub fn verify_profile_password(
    folder_name: String,
    password: String,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    info!(
        "Verifying password for profile in folder '{}'...",
        folder_name
    );
    let service = state.service.lock().unwrap();
    service
        .get_profile_id_with_password(&folder_name, &password)
        .map_err(FrontendError::from)
}

pub(crate) fn delete_profile_impl(
    folder_name: &str,
    password: &str,
    state: &AppState,
    metadata_wallet_dir: Option<&Path>,
    config_dir: Option<&Path>,
) -> Result<(), FrontendError> {
    info!(
        "Attempting to delete profile in folder '{}'...",
        folder_name
    );

    // Hold the exclusive RAII profile lock across the entire delete + cleanup lifecycle.
    let _delete_lock = state.acquire_profile_lock(folder_name)?;

    // 1. Delete in Core (verifies password, holds WalletLockGuard, resets Core if active)
    let was_active = {
        let mut service = state.lock_service()?;
        let active = service.is_active_profile(folder_name);
        service
            .delete_profile(folder_name, password)
            .map_err(FrontendError::from)?;
        if active || !service.is_wallet_unlocked() {
            state.clear_all_caches();
        }
        active
    };

    // 2. Remove metadata entry & device-local PIN keystore entry while still holding `_delete_lock`
    if let Some(dir) = metadata_wallet_dir {
        if let Err(e) = crate::services::fs_helpers::remove_profile_metadata_entry_in_dir(
            dir,
            folder_name,
        ) {
            log::warn!("Failed to remove deleted profile metadata entry: {}", e);
        }
    }
    if let Some(cfg_dir) = config_dir {
        let _ = pin_keystore::remove_entry(cfg_dir, folder_name);
    }

    state.release_profile_lock_if_matching(folder_name);
    if was_active {
        state.clear_sync_cooldown();
    }

    info!("Profile '{}' deleted successfully.", folder_name);
    Ok(())
}

#[tauri::command]
pub fn delete_profile(
    folder_name: String,
    password: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    let metadata_dir = crate::services::fs_helpers::get_wallet_data_dir(&app).ok();
    let config_dir = app.path().app_config_dir().ok();
    delete_profile_impl(
        &folder_name,
        &password,
        &state,
        metadata_dir.as_deref(),
        config_dir.as_deref(),
    )
}

// ============================================================================
// Secondary Auth (Device-Bound Quick-PIN / Cashier-PIN) & Session Elevation
// ============================================================================

fn resolve_pin_keystore_dir_and_device_id(
    app: &tauri::AppHandle,
) -> Result<(PathBuf, String), FrontendError> {
    let keystore_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let device_id = resolve_or_create_local_instance_id(app).map_err(FrontendError::from)?;
    Ok((keystore_dir, device_id))
}

pub(crate) fn resolve_active_profile_folder(
    service: &human_money_core::app_service::AppService,
    explicit_folder: Option<&str>,
    wallet_data_dir: &Path,
) -> Result<String, FrontendError> {
    if let Some(f) = explicit_folder {
        let trimmed = f.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    if let Ok(user_id) = service.with_wallet_and_identity(|_, id| id.user_id.clone()) {
        let meta = load_profile_metadata_map_from_dir(wallet_data_dir);
        if let Some((folder, _)) = meta
            .into_iter()
            .find(|(_, (_, uid))| uid.as_deref() == Some(user_id.as_str()))
        {
            return Ok(folder);
        }
    }
    let profiles = service.list_profiles().map_err(FrontendError::from)?;
    if profiles.len() == 1 {
        return Ok(profiles[0].folder_name.clone());
    }
    Err(FrontendError::from(
        "Could not determine active profile folder for PIN management.".to_string(),
    ))
}

pub fn get_profile_pin_status_impl(
    folder_name: &str,
    keystore_dir: &Path,
    state: &AppState,
) -> Result<ProfilePinStatusDto, FrontendError> {
    let service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;

    let core_has = service.has_secondary_auth(folder_name).unwrap_or(false);
    let entry = pin_keystore::get_entry(keystore_dir, folder_name).map_err(FrontendError::from)?;

    match (core_has, entry) {
        (true, Some(e)) => {
            let remaining = MAX_PIN_ATTEMPTS.saturating_sub(e.failed_attempts);
            Ok(ProfilePinStatusDto {
                enabled: true,
                scope: Some(pin_keystore::scope_to_str(e.scope).to_string()),
                failed_attempts: e.failed_attempts,
                max_attempts: MAX_PIN_ATTEMPTS,
                remaining_attempts: remaining,
            })
        }
        (false, Some(_)) => {
            // Self-heal stale keystore entry when core slot was wiped (e.g. password reset/restore)
            let _ = pin_keystore::remove_entry(keystore_dir, folder_name);
            Ok(ProfilePinStatusDto {
                enabled: false,
                scope: None,
                failed_attempts: 0,
                max_attempts: MAX_PIN_ATTEMPTS,
                remaining_attempts: MAX_PIN_ATTEMPTS,
            })
        }
        _ => Ok(ProfilePinStatusDto {
            enabled: false,
            scope: None,
            failed_attempts: 0,
            max_attempts: MAX_PIN_ATTEMPTS,
            remaining_attempts: MAX_PIN_ATTEMPTS,
        }),
    }
}

#[tauri::command]
pub fn get_profile_pin_status(
    folder_name: Option<String>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<ProfilePinStatusDto, FrontendError> {
    let keystore_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let wallet_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?
        .join("wallet_data");
    let resolved_folder = {
        let service = state
            .service
            .lock()
            .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
        resolve_active_profile_folder(&service, folder_name.as_deref(), &wallet_data_dir)?
    };
    get_profile_pin_status_impl(&resolved_folder, &keystore_dir, &state)
}

pub fn unlock_session_with_pin_impl(
    folder_name: &str,
    pin: &str,
    keystore_dir: &Path,
    device_binding_id: &str,
    state: &AppState,
) -> Result<SessionStatusDto, FrontendError> {
    {
        let service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
        if service
            .get_base_access_mode()
            .map_err(FrontendError::from)?
            == human_money_core::AccessMode::Full
        {
            return Err(FrontendError::from(human_money_core::Error::App(
                human_money_core::error::AppError::InsufficientAccessMode {
                    required: human_money_core::AccessMode::Full.as_str().to_string(),
                    current: human_money_core::AccessMode::DailyPay.as_str().to_string(),
                },
            )));
        }
    }
    let (secret, attempt) =
        pin_keystore::begin_pin_attempt(keystore_dir, device_binding_id, folder_name, pin)
            .map_err(|msg| FrontendError {
                code: "error.auth.invalidPinFormat".to_string(),
                message: msg,
                details: HashMap::new(),
            })?;

    let mut service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
    let timeout_seconds = resolve_policy_session_timeout(state, &mut service, &secret);
    match service.unlock_secondary_session(&secret, timeout_seconds) {
        Ok(()) => {
            let _ = pin_keystore::reset_failed_attempts(keystore_dir, folder_name);
            let mut status = build_session_status(&service);
            if timeout_seconds == 0 {
                status.one_shot_grant =
                    Some(state.issue_one_shot_pin_grant(&service, secret.to_string())?);
            } else {
                state.clear_one_shot_pin_grant();
                state.notify_sync_coordinator();
            }
            Ok(status)
        }
        Err(_) => {
            if attempt >= MAX_PIN_ATTEMPTS {
                let _ = service.wipe_secondary_auth_slot(folder_name);
                let _ = pin_keystore::remove_entry(keystore_dir, folder_name);
                service.lock_session();
                state.clear_one_shot_pin_grant();
                return Err(FrontendError {
                    code: "error.auth.pinWipedMaxAttempts".to_string(),
                    message: "PIN disabled after 5 failed attempts. Please sign in with your master password.".to_string(),
                    details: HashMap::new(),
                });
            }

            let remaining = MAX_PIN_ATTEMPTS.saturating_sub(attempt);
            let mut details = HashMap::new();
            details.insert("remaining".to_string(), remaining.to_string());
            Err(FrontendError {
                code: "error.auth.invalidPin".to_string(),
                message: format!("Invalid PIN ({} attempts remaining).", remaining),
                details,
            })
        }
    }
}

#[tauri::command]
pub fn unlock_session_with_pin(
    pin: String,
    folder_name: Option<String>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<SessionStatusDto, FrontendError> {
    let (keystore_dir, device_id) = resolve_pin_keystore_dir_and_device_id(&app)?;
    let wallet_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?
        .join("wallet_data");
    let resolved_folder = {
        let service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
        resolve_active_profile_folder(&service, None, &wallet_data_dir).or_else(|_| {
            resolve_active_profile_folder(&service, folder_name.as_deref(), &wallet_data_dir)
        })?
    };
    unlock_session_with_pin_impl(
        &resolved_folder,
        &pin,
        &keystore_dir,
        &device_id,
        &state,
    )
}

pub fn enable_profile_pin_impl(
    master_password: &str,
    pin: &str,
    scope_str: &str,
    folder_name: &str,
    keystore_dir: &Path,
    device_binding_id: &str,
    state: &AppState,
) -> Result<ProfilePinStatusDto, FrontendError> {
    let scope = pin_keystore::parse_scope(scope_str).map_err(FrontendError::from)?;
    pin_keystore::validate_pin_format(pin).map_err(|msg| FrontendError {
        code: "error.auth.invalidPinFormat".to_string(),
        message: msg,
        details: HashMap::new(),
    })?;

    // Verify master password cleanly BEFORE mutating the device keystore
    let mut service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    let _ = service
        .export_mnemonic(master_password)
        .map_err(FrontendError::from)?;

    let secret = pin_keystore::create_and_store_pepper(
        keystore_dir,
        device_binding_id,
        folder_name,
        pin,
        scope,
    )
    .map_err(FrontendError::from)?;

    if let Err(err) = service.enable_secondary_auth(master_password, &secret, scope) {
        let _ = pin_keystore::remove_entry(keystore_dir, folder_name);
        return Err(FrontendError::from(err));
    }

    Ok(ProfilePinStatusDto {
        enabled: true,
        scope: Some(pin_keystore::scope_to_str(scope).to_string()),
        failed_attempts: 0,
        max_attempts: MAX_PIN_ATTEMPTS,
        remaining_attempts: MAX_PIN_ATTEMPTS,
    })
}

#[tauri::command]
pub fn enable_profile_pin(
    master_password: String,
    pin: String,
    scope: String,
    folder_name: Option<String>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<ProfilePinStatusDto, FrontendError> {
    let (keystore_dir, device_id) = resolve_pin_keystore_dir_and_device_id(&app)?;
    let wallet_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?
        .join("wallet_data");
    let resolved_folder = {
        let service = state.service.lock().unwrap();
        resolve_active_profile_folder(&service, folder_name.as_deref(), &wallet_data_dir)?
    };
    enable_profile_pin_impl(
        &master_password,
        &pin,
        &scope,
        &resolved_folder,
        &keystore_dir,
        &device_id,
        &state,
    )
}

pub fn disable_profile_pin_impl(
    master_password: &str,
    folder_name: &str,
    keystore_dir: &Path,
    state: &AppState,
) -> Result<(), FrontendError> {
    let mut service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    service
        .disable_secondary_auth(master_password)
        .map_err(FrontendError::from)?;
    pin_keystore::remove_entry(keystore_dir, folder_name).map_err(FrontendError::from)?;
    Ok(())
}

#[tauri::command]
pub fn disable_profile_pin(
    master_password: String,
    folder_name: Option<String>,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    let keystore_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let wallet_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?
        .join("wallet_data");
    let resolved_folder = {
        let service = state.service.lock().unwrap();
        resolve_active_profile_folder(&service, folder_name.as_deref(), &wallet_data_dir)?
    };
    disable_profile_pin_impl(&master_password, &resolved_folder, &keystore_dir, &state)
}

#[allow(clippy::too_many_arguments)]
pub fn login_with_pin_impl(
    folder_name: &str,
    pin: &str,
    cleanup_on_login: bool,
    local_instance_id: String,
    keystore_dir: &Path,
    device_binding_id: &str,
    state: &AppState,
    app: Option<&tauri::AppHandle>,
) -> Result<String, FrontendError> {
    let pending_lock = state.acquire_profile_lock(folder_name)?;
    let (secret, attempt) =
        pin_keystore::begin_pin_attempt(keystore_dir, device_binding_id, folder_name, pin)
            .map_err(|msg| FrontendError {
                code: "error.auth.invalidPinFormat".to_string(),
                message: msg,
                details: HashMap::new(),
            })?;

    let mut service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;

    match service.login_with_secondary(folder_name, &secret, cleanup_on_login, local_instance_id) {
        Ok(mode) => {
            let _ = pin_keystore::reset_failed_attempts(keystore_dir, folder_name);
            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            if let Some(app_handle) = app {
                if let Err(e) = record_profile_metadata(app_handle, folder_name, user_id) {
                    error!("Failed to update profile metadata: {}", e);
                }
            }

            state
                .initialize_profile_session_with_secondary(&mut service, &secret)
                .map_err(FrontendError::from)?;
            drop(service);
            state.commit_profile_lock(pending_lock);

            if let Some(app_handle) = app {
                crate::services::common::spawn_pending_l2_reconciliation(app_handle, &secret);
            }

            Ok(mode.as_str().to_string())
        }
        Err(err) => {
            let is_security_lock = matches!(
                err,
                human_money_core::Error::WalletLockedDueToFork
                    | human_money_core::Error::WalletSupersededByHandover { .. }
                    | human_money_core::Error::WalletConcurrentCloneDetected
                    | human_money_core::Error::StateRollbackDetected
                    | human_money_core::Error::App(
                        human_money_core::error::AppError::DeviceMismatch { .. }
                    )
            );
            if is_security_lock {
                let _ = pin_keystore::reset_failed_attempts(keystore_dir, folder_name);
                return Err(FrontendError::from(err));
            }

            if attempt >= MAX_PIN_ATTEMPTS {
                let _ = service.wipe_secondary_auth_slot(folder_name);
                let _ = pin_keystore::remove_entry(keystore_dir, folder_name);
                return Err(FrontendError {
                    code: "error.auth.pinWipedMaxAttempts".to_string(),
                    message: "PIN disabled after 5 failed attempts. Please sign in with your master password.".to_string(),
                    details: HashMap::new(),
                });
            }

            let remaining = MAX_PIN_ATTEMPTS.saturating_sub(attempt);
            let mut details = HashMap::new();
            details.insert("remaining".to_string(), remaining.to_string());
            Err(FrontendError {
                code: "error.auth.invalidPin".to_string(),
                message: format!("Invalid PIN ({} attempts remaining).", remaining),
                details,
            })
        }
    }
}

#[tauri::command]
pub fn login_with_pin(
    folder_name: String,
    pin: String,
    cleanup_on_login: bool,
    local_instance_id: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<String, FrontendError> {
    info!("Attempting PIN login for folder '{}'...", folder_name);
    let (keystore_dir, device_id) = resolve_pin_keystore_dir_and_device_id(&app)?;
    login_with_pin_impl(
        &folder_name,
        &pin,
        cleanup_on_login,
        local_instance_id,
        &keystore_dir,
        &device_id,
        &state,
        Some(&app),
    )
}

pub fn elevate_session_with_password_impl(
    master_password: &str,
    state: &AppState,
) -> Result<SessionStatusDto, FrontendError> {
    let mut service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
    // Fail-closed: if cached security policy is absent, do not elevate with a non-zero cache timeout.
    let timeout_seconds = state
        .get_cached_security_policy()
        .map(|p| p.session_timeout_seconds)
        .unwrap_or(0);
    service
        .elevate_session(master_password, timeout_seconds)
        .map_err(FrontendError::from)?;
    state.clear_one_shot_pin_grant();
    Ok(build_session_status(&service))
}

#[tauri::command]
pub fn elevate_session_with_password(
    master_password: String,
    state: tauri::State<AppState>,
) -> Result<SessionStatusDto, FrontendError> {
    elevate_session_with_password_impl(&master_password, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::fs_helpers::ProfileMetadata;

    #[test]
    fn test_login_ipc_parsing() {
        // Test that malformed JSON doesn't crash the parsing logic
        let invalid_json = "{ invalid json }";
        let result: Result<serde_json::Value, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err(), "Malformed JSON should fail to parse");

        // Test that valid JSON structure can be parsed
        let valid_json = r#"{
            "folderName": "test_profile",
            "password": "test_password",
            "cleanupOnLogin": true,
            "localInstanceId": "test-instance-123"
        }"#;
        let result: Result<serde_json::Value, _> = serde_json::from_str(valid_json);
        assert!(result.is_ok(), "Valid JSON should parse successfully");
    }

    #[test]
    fn test_profile_metadata_serialization() {
        // Test that ProfileMetadata can be serialized and deserialized
        let metadata = ProfileMetadata {
            last_used: Utc::now().to_rfc3339(),
            user_id: None,
        };

        let serialized = serde_json::to_string(&metadata);
        assert!(serialized.is_ok(), "ProfileMetadata should serialize");

        let deserialized: Result<ProfileMetadata, _> = serde_json::from_str(&serialized.unwrap());
        assert!(deserialized.is_ok(), "ProfileMetadata should deserialize");
    }

    #[test]
    fn test_profile_info_serialization() {
        // Test that ProfileInfo can be serialized and deserialized
        let profile_info = ProfileInfo {
            profile_name: "Test Profile".to_string(),
            folder_name: "test_folder".to_string(),
            last_used: Some(Utc::now().to_rfc3339()),
            user_id: Some("did:key:z6MkTest".to_string()),
        };

        let serialized = serde_json::to_string(&profile_info);
        assert!(serialized.is_ok(), "ProfileInfo should serialize");

        let deserialized: Result<ProfileInfo, _> = serde_json::from_str(&serialized.unwrap());
        assert!(deserialized.is_ok(), "ProfileInfo should deserialize");
        assert_eq!(deserialized.unwrap().profile_name, "Test Profile");
    }

    #[test]
    fn test_logout_clears_all_caches() {
        use human_money_core::app_service::AppService;
        use std::path::PathBuf;

        let service = AppService::new(&PathBuf::from("/tmp")).unwrap();
        let state = AppState::new(service);
        state.set_cached_history(Some(vec![]));
        state.set_cached_events(Some(vec![]));
        state.set_cached_contacts(Some(Default::default()));
        state.set_cached_ui_preferences(Some(UiPreferences::default()));
        state.set_cached_security_policy(Some(SecurityPolicy::default()));
        state.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));

        state.clear_all_caches();

        assert!(state.profile_caches.lock().unwrap().is_none());
        assert!(state.cache_history().is_none());
        assert!(state.cache_events().is_none());
        assert!(state.cache_contacts().is_none());
        assert!(state.cache_ui_preferences().is_none());
        assert!(state.cache_security_policy().is_none());
        assert!(state.cache_internal_lifecycle().is_none());
    }

    #[test]
    fn test_sysinfo_processes() {
        use sysinfo::{Pid, System};
        let mut s = System::new();
        s.refresh_processes();
        let current_pid = std::process::id();
        let proc = s.process(Pid::from_u32(current_pid));
        assert!(
            proc.is_some(),
            "Current process should be found in refreshed processes! PID: {}",
            current_pid
        );
    }

    #[test]
    fn test_pin_enable_login_five_strike_wipe_and_elevate() {
        use human_money_core::app_service::AppService;
        use human_money_core::services::mnemonic::MnemonicLanguage;

        let dir = crate::test_support::temp_test_dir("pin_auth_test");
        let wallet_dir = dir.join("wallet_data");
        let keystore_dir = dir.join("keystore");
        std::fs::create_dir_all(&wallet_dir).unwrap();

        let master_pw = "MasterPass-Strong-2026!";
        let mut service = AppService::new(&wallet_dir).unwrap();
        let mnemonic = AppService::generate_mnemonic(12, MnemonicLanguage::English).unwrap();
        service
            .create_profile(
                "Pin Tester",
                &mnemonic,
                None,
                Some("pt"),
                master_pw,
                MnemonicLanguage::English,
                "dev-pin-1".into(),
                true,
            )
            .unwrap();
        let folder = service.list_profiles().unwrap()[0].folder_name.clone();

        let app_state = AppState::new(service);

        // 1. Short / non-digit PIN rejected
        assert!(enable_profile_pin_impl(
            master_pw,
            "1234",
            "dailyPay",
            &folder,
            &keystore_dir,
            "dev-pin-1",
            &app_state
        )
        .is_err());

        // ReceiveOnly scope is no longer accepted in the App layer
        assert!(enable_profile_pin_impl(
            master_pw,
            "246810",
            "receiveOnly",
            &folder,
            &keystore_dir,
            "dev-pin-1",
            &app_state
        )
        .is_err());

        // 2. Enable 6-digit DailyPay PIN
        let status = enable_profile_pin_impl(
            master_pw,
            "246810",
            "dailyPay",
            &folder,
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap();
        assert!(status.enabled);
        assert_eq!(status.scope.as_deref(), Some("dailyPay"));
        assert_eq!(status.remaining_attempts, 5);

        // Full-baseline session rejects PIN session unlock without touching failed_attempts
        let full_pin_err = unlock_session_with_pin_impl(
            &folder,
            "000000",
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap_err();
        assert_eq!(full_pin_err.code, "error.auth.insufficientAccessMode");
        let after_full_pin_err =
            get_profile_pin_status_impl(&folder, &keystore_dir, &app_state).unwrap();
        assert!(after_full_pin_err.enabled);
        assert_eq!(after_full_pin_err.failed_attempts, 0);
        assert_eq!(after_full_pin_err.remaining_attempts, 5);

        // Logout and login via PIN
        app_state.service.lock().unwrap().logout();
        let mode = login_with_pin_impl(
            &folder,
            "246810",
            false,
            "dev-pin-1".into(),
            &keystore_dir,
            "dev-pin-1",
            &app_state,
            None,
        )
        .unwrap();
        assert_eq!(mode, "dailyPay");
        let st = get_session_status_impl(&app_state).unwrap();
        assert!(st.is_unlocked);
        assert_eq!(st.access_mode, "dailyPay");
        assert_eq!(st.base_access_mode, "dailyPay");

        // Fail-closed elevation when cached security policy is cleared -> timeout 0, no cache, base mode stays dailyPay
        app_state.set_cached_security_policy(None);
        let fail_closed = elevate_session_with_password_impl(master_pw, &app_state).unwrap();
        assert!(fail_closed.is_unlocked);
        assert!(!fail_closed.has_cached_session);
        assert_eq!(fail_closed.access_mode, "dailyPay");
        assert_eq!(fail_closed.base_access_mode, "dailyPay");

        // Elevate session with active timeout -> grants temporary Full access, lock_session demotes back to dailyPay
        let timed_policy = SecurityPolicy {
            session_timeout_seconds: 300,
            ..Default::default()
        };
        app_state.set_cached_security_policy(Some(timed_policy));
        let elevated = elevate_session_with_password_impl(master_pw, &app_state).unwrap();
        assert_eq!(
            elevated,
            SessionStatusDto {
                is_unlocked: true,
                has_cached_session: true,
                access_mode: "full".to_string(),
                base_access_mode: "dailyPay".to_string(),
                one_shot_grant: None,
            }
        );
        app_state.service.lock().unwrap().lock_session();
        let demoted = get_session_status_impl(&app_state).unwrap();
        assert_eq!(
            demoted,
            SessionStatusDto {
                is_unlocked: true,
                has_cached_session: false,
                access_mode: "dailyPay".to_string(),
                base_access_mode: "dailyPay".to_string(),
                one_shot_grant: None,
            }
        );

        // Ephemeral PIN re-unlock when backend policy timeout == 0 leaves has_cached_session == false and issues one-shot grant
        app_state.set_cached_security_policy(Some(SecurityPolicy {
            session_timeout_seconds: 0,
            ..Default::default()
        }));
        let ephemeral = unlock_session_with_pin_impl(
            &folder,
            "246810",
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap();
        assert!(!ephemeral.has_cached_session);
        assert_eq!(ephemeral.access_mode, "dailyPay");
        assert!(ephemeral
            .one_shot_grant
            .as_deref()
            .is_some_and(|g| g.starts_with(crate::state::ONE_SHOT_PIN_GRANT_PREFIX)));

        // Timed PIN re-unlock when backend policy timeout > 0 activates DailyPay cache & resets strike counter
        app_state.set_cached_security_policy(Some(SecurityPolicy {
            session_timeout_seconds: 300,
            ..Default::default()
        }));
        let wrong_once = unlock_session_with_pin_impl(
            &folder,
            "000000",
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap_err();
        assert_eq!(wrong_once.code, "error.auth.invalidPin");
        assert_eq!(
            wrong_once.details.get("remaining").map(String::as_str),
            Some("4")
        );

        let cached_pin = unlock_session_with_pin_impl(
            &folder,
            "246810",
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap();
        assert!(cached_pin.has_cached_session);
        assert!(cached_pin.one_shot_grant.is_none());
        assert_eq!(cached_pin.access_mode, "dailyPay");
        assert_eq!(cached_pin.base_access_mode, "dailyPay");
        assert_eq!(
            get_profile_pin_status_impl(&folder, &keystore_dir, &app_state)
                .unwrap()
                .remaining_attempts,
            5
        );

        // Full-access operations fail cleanly in DailyPay mode without valid master password
        let req_uri = humoco_market::build_recovery_request_uri(&[7u8; 32], 1_760_000_000, None);
        let release_err =
            crate::services::recovery_service::approve_guardian_recovery_request_impl(
                req_uri,
                "did:key:zOwnerDummy".to_string(),
                Some("246810"),
                &app_state,
            )
            .unwrap_err();
        assert_eq!(release_err.code, "error.auth.insufficientAccessMode");

        let access_err = app_state
            .authorize(RequiredAccess::Full, Some("246810"))
            .unwrap_err();
        assert_eq!(access_err.code, "error.auth.insufficientAccessMode");

        // Boundary enforcement across l2, sync, and standards modules:
        // DailyPay fails with insufficientAccessMode; master password override succeeds.
        let l2_err = crate::services::l2_anchor_service::set_own_nominated_lock_server_impl(
            None, None, None, &app_state,
        )
        .unwrap_err();
        assert_eq!(l2_err.code, "error.auth.insufficientAccessMode");
        assert!(
            crate::services::l2_anchor_service::set_own_nominated_lock_server_impl(
                None,
                None,
                Some(master_pw.to_string()),
                &app_state,
            )
            .is_ok()
        );

        let sync_err =
            crate::services::sync_engine::admin::delete_sync_admin_impl(None, &app_state)
                .unwrap_err();
        assert_eq!(sync_err.code, "error.auth.insufficientAccessMode");
        assert!(crate::services::sync_engine::admin::delete_sync_admin_impl(
            Some(master_pw.to_string()),
            &app_state,
        )
        .is_ok());

        let standards_dir = dir.join("standards");
        std::fs::create_dir_all(&standards_dir).unwrap();
        {
            let svc = app_state.service.lock().unwrap();
            let std_del_err = svc
                .delete_voucher_standard("minuto", None, &standards_dir)
                .unwrap_err();
            assert_eq!(
                FrontendError::from(std_del_err).code,
                "error.auth.insufficientAccessMode"
            );
            let std_imp_err = svc
                .import_voucher_standard(b"invalid", None, None, &standards_dir)
                .unwrap_err();
            assert_eq!(
                FrontendError::from(std_imp_err).code,
                "error.auth.insufficientAccessMode"
            );
            assert!(svc
                .delete_voucher_standard("missing_standard", Some(master_pw), &standards_dir)
                .is_ok());
        }

        // 5-strike wipe during unlocked PIN re-auth wipes slot + clears active session cache
        for expected_rem in [4, 3, 2, 1] {
            let err = unlock_session_with_pin_impl(
                &folder,
                "000000",
                &keystore_dir,
                "dev-pin-1",
                &app_state,
            )
            .unwrap_err();
            assert_eq!(err.code, "error.auth.invalidPin");
            assert_eq!(
                err.details.get("remaining").map(String::as_str),
                Some(expected_rem.to_string().as_str())
            );
        }
        let wipe_reauth_err = unlock_session_with_pin_impl(
            &folder,
            "000000",
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap_err();
        assert_eq!(wipe_reauth_err.code, "error.auth.pinWipedMaxAttempts");
        let after_reauth_wipe = get_session_status_impl(&app_state).unwrap();
        assert!(!after_reauth_wipe.has_cached_session);
        assert!(
            !get_profile_pin_status_impl(&folder, &keystore_dir, &app_state)
                .unwrap()
                .enabled
        );

        // Re-enable PIN and verify 5-strike wipe on login_with_pin_impl as well
        enable_profile_pin_impl(
            master_pw,
            "246810",
            "dailyPay",
            &folder,
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap();
        app_state.service.lock().unwrap().logout();

        for expected_rem in [4, 3, 2, 1] {
            let err = login_with_pin_impl(
                &folder,
                "000000",
                false,
                "dev-pin-1".into(),
                &keystore_dir,
                "dev-pin-1",
                &app_state,
                None,
            )
            .unwrap_err();
            assert_eq!(err.code, "error.auth.invalidPin");
            assert_eq!(
                err.details.get("remaining").map(String::as_str),
                Some(expected_rem.to_string().as_str())
            );
        }

        // 5th strike triggers wipe!
        let wipe_err = login_with_pin_impl(
            &folder,
            "000000",
            false,
            "dev-pin-1".into(),
            &keystore_dir,
            "dev-pin-1",
            &app_state,
            None,
        )
        .unwrap_err();
        assert_eq!(wipe_err.code, "error.auth.pinWipedMaxAttempts");

        let post_wipe_status =
            get_profile_pin_status_impl(&folder, &keystore_dir, &app_state).unwrap();
        assert!(!post_wipe_status.enabled);

        // Master password login still works cleanly after PIN wipe!
        app_state
            .service
            .lock()
            .unwrap()
            .login(&folder, master_pw, false, "dev-pin-1".into())
            .unwrap();
    }

    #[test]
    fn h1_h2_one_shot_pin_grant_single_use_rejects_full_and_clear_invalidates() {
        use human_money_core::app_service::AppService;
        use human_money_core::services::mnemonic::MnemonicLanguage;

        let dir = crate::test_support::temp_test_dir("h1_h2_grant_matrix");
        let wallet_dir = dir.join("wallet_data");
        let keystore_dir = dir.join("keystore");
        std::fs::create_dir_all(&wallet_dir).unwrap();

        let master_pw = "MasterPass-H1H2-2026!";
        let mut service = AppService::new(&wallet_dir).unwrap();
        let mnemonic = AppService::generate_mnemonic(12, MnemonicLanguage::English).unwrap();
        service
            .create_profile(
                "Matrix Tester",
                &mnemonic,
                None,
                Some("mx"),
                master_pw,
                MnemonicLanguage::English,
                "dev-h1".into(),
                true,
            )
            .unwrap();
        let folder = service.list_profiles().unwrap()[0].folder_name.clone();
        let app_state = AppState::new(service);

        enable_profile_pin_impl(
            master_pw,
            "246810",
            "dailyPay",
            &folder,
            &keystore_dir,
            "dev-h1",
            &app_state,
        )
        .unwrap();

        app_state.service.lock().unwrap().logout();
        login_with_pin_impl(
            &folder,
            "246810",
            false,
            "dev-h1".into(),
            &keystore_dir,
            "dev-h1",
            &app_state,
            None,
        )
        .unwrap();
        // Force DailyPay cache expiration so timeout=0 unlock issues a single-use grant
        app_state.service.lock().unwrap().lock_session();
        app_state.set_cached_security_policy(Some(SecurityPolicy {
            session_timeout_seconds: 0,
            ..Default::default()
        }));

        // Locked DailyPay session denies Full and DailyPay calls without grant
        let denied_daily = app_state.authorize(RequiredAccess::DailyPay, None).unwrap_err();
        assert!(
            denied_daily.code == "error.auth.passwordRequired"
                || denied_daily.code == "error.auth.sessionNotActive",
            "unexpected locked error code: {}",
            denied_daily.code
        );

        let unlocked = unlock_session_with_pin_impl(
            &folder,
            "246810",
            &keystore_dir,
            "dev-h1",
            &app_state,
        )
        .unwrap();
        let grant = unlocked
            .one_shot_grant
            .expect("timeout=0 PIN unlock must issue a single-use grant");
        assert!(grant.starts_with(crate::state::ONE_SHOT_PIN_GRANT_PREFIX));

        // 1. Grant is REJECTED for RequiredAccess::Full (H1/H2: DailyPay-only grant cannot elevate)
        let full_err = app_state
            .authorize_and_lock_secret(RequiredAccess::Full, Some(&grant))
            .err()
            .unwrap();
        assert_eq!(full_err.code, "error.auth.insufficientAccessMode");

        // 2. Grant is accepted ONCE for RequiredAccess::DailyPay and immediately consumed
        assert!(app_state
            .authorize_and_lock_secret(RequiredAccess::DailyPay, Some(&grant))
            .is_ok());
        let reuse_err = app_state
            .authorize_and_lock_secret(RequiredAccess::DailyPay, Some(&grant))
            .err()
            .unwrap();
        assert_eq!(
            reuse_err.code, "error.auth.sessionTimeout",
            "expected sessionTimeout on one-shot grant reuse, got {}",
            reuse_err.code
        );

        // 3. Issuing a new grant and logging out / clearing caches invalidates it
        let grant2 = unlock_session_with_pin_impl(
            &folder,
            "246810",
            &keystore_dir,
            "dev-h1",
            &app_state,
        )
        .unwrap()
        .one_shot_grant
        .unwrap();
        app_state.clear_all_caches();
        assert!(app_state
            .authorize_and_lock_secret(RequiredAccess::DailyPay, Some(&grant2))
            .is_err());

        // 4. H2: Timed DailyPay session satisfies DailyPay, NOT Full
        app_state.set_cached_security_policy(Some(SecurityPolicy {
            session_timeout_seconds: 300,
            ..Default::default()
        }));
        unlock_session_with_pin_impl(&folder, "246810", &keystore_dir, "dev-h1", &app_state)
            .unwrap();
        assert!(app_state.authorize(RequiredAccess::DailyPay, None).is_ok());
        let daily_full_err = app_state.authorize(RequiredAccess::Full, None).unwrap_err();
        assert_eq!(daily_full_err.code, "error.auth.insufficientAccessMode");
        assert!(app_state
            .authorize(RequiredAccess::Full, Some(master_pw))
            .is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn k2_h2_delete_profile_enforces_lock_contention_and_tears_down_active_session() {
        use human_money_core::app_service::AppService;
        use human_money_core::MnemonicLanguage;

        let dir = crate::test_support::temp_test_dir("k2_h2_delete_profile");
        let wallet_dir = dir.join("wallet_data");
        let lock_dir = dir.join("session_locks");
        let config_dir = dir.join("config");
        std::fs::create_dir_all(&wallet_dir).unwrap();
        std::fs::create_dir_all(&config_dir).unwrap();

        let pw = "DeleteProfileStrongPw-2026!";
        let mut svc_a = AppService::new(&wallet_dir).unwrap();
        let mnemonic = AppService::generate_mnemonic(12, MnemonicLanguage::English).unwrap();
        svc_a
            .create_profile(
                "VictimProfile",
                &mnemonic,
                None,
                None,
                pw,
                MnemonicLanguage::English,
                "dev-a".into(),
                true,
            )
            .unwrap();
        let folder = svc_a.list_profiles().unwrap()[0].folder_name.clone();
        crate::services::fs_helpers::record_profile_metadata_in_dir(
            &wallet_dir,
            &folder,
            Some("did:key:zExample".to_string()),
        )
        .unwrap();

        let state_a = AppState::new(svc_a).with_session_lock_dir(lock_dir.clone());
        let svc_b = AppService::new(&wallet_dir).unwrap();
        let state_b = AppState::new(svc_b).with_session_lock_dir(lock_dir.clone());

        // B holds the profile lock -> A's delete_profile MUST fail closed before touching files!
        let lock_b = state_b.acquire_profile_lock(&folder).unwrap();
        state_b.commit_profile_lock(lock_b);
        let contention_err = delete_profile_impl(
            &folder,
            pw,
            &state_a,
            Some(&wallet_dir),
            Some(&config_dir),
        )
        .unwrap_err();
        assert_eq!(contention_err.code, "error.auth.profileAlreadyOpen");
        assert!(wallet_dir.join(&folder).exists());

        // After B releases and A opens the same active profile, A's delete clears caches/session & removes dir
        state_b.release_profile_lock();
        let lock_a = state_a.acquire_profile_lock(&folder).unwrap();
        state_a.commit_profile_lock(lock_a);
        let gen_before = state_a.current_session_generation();

        delete_profile_impl(
            &folder,
            pw,
            &state_a,
            Some(&wallet_dir),
            Some(&config_dir),
        )
        .expect("Active profile delete after lock release must succeed");

        assert!(!wallet_dir.join(&folder).exists());
        assert!(
            state_a.current_session_generation() > gen_before,
            "K2/H2: deleting active profile must bump session_generation"
        );
        assert!(
            !state_a.service.lock().unwrap().is_wallet_unlocked(),
            "H2: Core session must be locked after deleting active profile"
        );
        // K2: flock-backed .lock file stays on disk and is never unlinked while another instance can re-lock
        assert!(lock_dir.join(format!("{}.lock", folder)).exists());
        assert!(state_b.acquire_profile_lock(&folder).is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
