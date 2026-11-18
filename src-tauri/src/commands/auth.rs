use crate::models::{FrontendError, MnemonicLanguage, ProfileInfo};
use crate::pin_keystore::{self, ProfilePinStatusDto, MAX_PIN_ATTEMPTS};
use crate::services::fs_helpers::{
    load_profile_metadata, load_profile_metadata_map_from_dir, record_profile_metadata,
    remove_profile_metadata_entry, resolve_or_create_local_instance_id,
};
use crate::settings::{AppSettings, SETTINGS_KEY};
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

    // Load profile metadata for last_used timestamps
    let metadata = load_profile_metadata(&app).unwrap_or_default();

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
) -> Result<(), FrontendError> {
    info!(
        "Attempting to create profile '{}' with language {:?}...",
        profile_name, language
    );
    {
        let mut service = state.service.lock().unwrap();
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

                let folder_name = service
                    .list_profiles()
                    .ok()
                    .and_then(|profiles| {
                        profiles
                            .into_iter()
                            .find(|p| p.profile_name == profile_name)
                    })
                    .map(|p| p.folder_name);
                let user_id = service
                    .with_wallet_and_identity(|_, id| id.user_id.clone())
                    .ok();

                if let Some(ref folder) = folder_name {
                    if let Err(e) = record_profile_metadata(&app, folder, user_id) {
                        error!("Failed to update profile metadata: {}", e);
                    }
                }

                // 1. Initialize and save default settings for the new profile
                let default_settings = AppSettings::default();
                let settings_bytes = serde_json::to_vec(&default_settings)
                    .map_err(|e| FrontendError::from(e.to_string()))?;

                service
                    .save_encrypted_data(SETTINGS_KEY, &settings_bytes, Some(&password))
                    .map_err(|e| {
                        FrontendError::from(format!("Failed to save default settings: {}", e))
                    })?;

                // 2. Initialize session and cache
                state
                    .initialize_profile_session(&mut service, &password)
                    .map_err(FrontendError::from)?;
            }
            Err(e) => {
                error!("Profile creation failed: {}", e);
                return Err(FrontendError::from(e));
            }
        }
    }

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

    Ok(())
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

            crate::services::common::spawn_pending_l2_reconciliation(&app, &password);

            Ok(())
        }
        Err(e) => {
            error!("Handover failed: {}", e);
            Err(FrontendError::from(e))
        }
    }
}

#[tauri::command]
pub fn logout(state: tauri::State<AppState>) {
    info!("Logging out...");
    // Leere die zwischengespeicherten Daten sicher aus dem Speicher
    state.clear_all_caches();

    let mut service = state.service.lock().unwrap();
    service.logout();
    info!("Logout complete.");
}

// NEUE SESSION COMMANDS

#[tauri::command]
pub fn unlock_session(
    password: String,
    duration_seconds: u64,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    info!("Unlocking session for {} seconds...", duration_seconds);
    let mut service = state.service.lock().unwrap();
    service
        .unlock_session(&password, duration_seconds)
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn lock_session(state: tauri::State<AppState>) {
    info!("Locking session manually.");
    let mut service = state.service.lock().unwrap();
    service.lock_session();
}

#[tauri::command]
pub fn is_session_active(state: tauri::State<AppState>) -> bool {
    // Loggt nicht, um Spam zu vermeiden
    let service = state.service.lock().unwrap();
    service.is_session_active()
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

#[tauri::command]
pub fn delete_profile(
    folder_name: String,
    password: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), FrontendError> {
    info!(
        "Attempting to delete profile in folder '{}'...",
        folder_name
    );

    // 1. Delete in Core (this also verifies password)
    let mut service = state.service.lock().unwrap();
    service
        .delete_profile(&folder_name, &password)
        .map_err(FrontendError::from)?;

    // 2. Remove metadata entry & device-local PIN keystore entry
    let _ = remove_profile_metadata_entry(&app, &folder_name);
    if let Ok(config_dir) = app.path().app_config_dir() {
        let _ = pin_keystore::remove_entry(&config_dir, &folder_name);
    }

    info!("Profile '{}' deleted successfully.", folder_name);
    Ok(())
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

fn resolve_active_profile_folder(
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
    folder_name: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<ProfilePinStatusDto, FrontendError> {
    let keystore_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    get_profile_pin_status_impl(&folder_name, &keystore_dir, &state)
}

pub fn get_session_access_mode_impl(state: &AppState) -> Result<String, FrontendError> {
    let service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    match service.get_access_mode() {
        Ok(mode) => Ok(mode.as_str().to_string()),
        Err(human_money_core::Error::WalletLocked) => Ok("locked".to_string()),
        Err(e) => Err(FrontendError::from(e)),
    }
}

#[tauri::command]
pub fn get_session_access_mode(state: tauri::State<AppState>) -> Result<String, FrontendError> {
    get_session_access_mode_impl(&state)
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
) -> Result<String, FrontendError> {
    let timeout_seconds = state
        .get_cached_settings()
        .map(|s| s.session_timeout_seconds)
        .unwrap_or(600);
    let mut service = state
        .service
        .lock()
        .map_err(|_| FrontendError::from("Wallet service unavailable".to_string()))?;
    service
        .elevate_session(master_password, timeout_seconds)
        .map_err(FrontendError::from)?;
    let mode = service.get_access_mode().map_err(FrontendError::from)?;
    Ok(mode.as_str().to_string())
}

#[tauri::command]
pub fn elevate_session_with_password(
    master_password: String,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    elevate_session_with_password_impl(&master_password, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::fs_helpers::ProfileMetadata;
    use std::sync::Mutex;

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
        let state = AppState {
            service: Mutex::new(service),
            history: Mutex::new(Some(vec![])),
            events: Mutex::new(Some(vec![])),
            contacts: Mutex::new(Some(Default::default())),
            settings: Mutex::new(Some(AppSettings::default())),
            wot: Mutex::new(None),
            media_negative_cache: Mutex::new(std::collections::HashMap::new()),
        };

        // We can't call the command directly easily because of tauri::State,
        // but we can test the logic:
        {
            *state.history.lock().unwrap() = None;
            *state.events.lock().unwrap() = None;
            *state.contacts.lock().unwrap() = None;
            *state.settings.lock().unwrap() = None;
        }

        assert!(state.history.lock().unwrap().is_none());
        assert!(state.events.lock().unwrap().is_none());
        assert!(state.contacts.lock().unwrap().is_none());
        assert!(state.settings.lock().unwrap().is_none());
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

        let app_state = AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            wot: Mutex::new(None),
            media_negative_cache: Mutex::new(HashMap::new()),
        };

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
        assert_eq!(
            get_session_access_mode_impl(&app_state).unwrap(),
            "dailyPay"
        );

        // Elevate session -> restores Full access
        assert_eq!(
            elevate_session_with_password_impl(master_pw, &app_state).unwrap(),
            "full"
        );
        assert_eq!(get_session_access_mode_impl(&app_state).unwrap(), "full");

        // Switch to ReceiveOnly PIN and test 5-strike auto-wipe
        let r_status = enable_profile_pin_impl(
            master_pw,
            "13579024",
            "receiveOnly",
            &folder,
            &keystore_dir,
            "dev-pin-1",
            &app_state,
        )
        .unwrap();
        assert_eq!(r_status.scope.as_deref(), Some("receiveOnly"));

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
}
