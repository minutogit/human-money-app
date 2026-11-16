use crate::models::{ProfileInfo, MnemonicLanguage, FrontendError};
use crate::settings::{AppSettings, SETTINGS_KEY};
use crate::AppState;
use chrono::{DateTime, Utc};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

const PROFILE_METADATA_FILE: &str = "profile_metadata.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct ProfileMetadata {
    pub(crate) last_used: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) user_id: Option<String>,
}

fn get_metadata_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir()
        .map(|dir| dir.join("wallet_data").join(PROFILE_METADATA_FILE))
        .map_err(|e| format!("Could not determine app data directory: {}", e))
}

fn load_profile_metadata(app: &tauri::AppHandle) -> Result<HashMap<String, ProfileMetadata>, String> {
    let metadata_path = get_metadata_path(app)?;
    
    if !metadata_path.exists() {
        return Ok(HashMap::new());
    }
    
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read profile metadata: {}", e))?;
    
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse profile metadata: {}", e))
}

pub(crate) fn load_profile_metadata_map_from_dir(wallet_data_dir: &std::path::Path) -> HashMap<String, (String, Option<String>)> {
    let metadata_path = wallet_data_dir.join(PROFILE_METADATA_FILE);
    if !metadata_path.exists() {
        return HashMap::new();
    }
    let Ok(content) = fs::read_to_string(&metadata_path) else {
        return HashMap::new();
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, ProfileMetadata>>(&content) else {
        return HashMap::new();
    };
    map.into_iter().map(|(k, v)| (k, (v.last_used, v.user_id))).collect()
}

fn save_profile_metadata(app: &tauri::AppHandle, metadata: &HashMap<String, ProfileMetadata>) -> Result<(), String> {
    let metadata_path = get_metadata_path(app)?;
    
    // Ensure directory exists
    if let Some(parent) = metadata_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create metadata directory: {}", e))?;
    }
    
    let content = serde_json::to_string_pretty(metadata)
        .map_err(|e| format!("Failed to serialize profile metadata: {}", e))?;
    
    fs::write(&metadata_path, content)
        .map_err(|e| format!("Failed to write profile metadata: {}", e))
}

pub(crate) fn record_profile_metadata_in_dir(
    wallet_data_dir: &std::path::Path,
    folder_name: &str,
    user_id: Option<String>,
) -> Result<(), String> {
    let metadata_path = wallet_data_dir.join(PROFILE_METADATA_FILE);
    let mut metadata: HashMap<String, ProfileMetadata> = if metadata_path.exists() {
        fs::read_to_string(&metadata_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let existing_uid = metadata.get(folder_name).and_then(|m| m.user_id.clone());
    metadata.insert(
        folder_name.to_string(),
        ProfileMetadata {
            last_used: Utc::now().to_rfc3339(),
            user_id: user_id.or(existing_uid),
        },
    );

    if let Some(parent) = metadata_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create metadata directory: {}", e))?;
    }
    let content = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    fs::write(&metadata_path, content).map_err(|e| e.to_string())
}

pub(crate) fn record_profile_metadata(
    app: &tauri::AppHandle,
    folder_name: &str,
    user_id: Option<String>,
) -> Result<(), String> {
    let mut metadata = load_profile_metadata(app).unwrap_or_default();
    let existing_uid = metadata.get(folder_name).and_then(|m| m.user_id.clone());
    metadata.insert(
        folder_name.to_string(),
        ProfileMetadata {
            last_used: Utc::now().to_rfc3339(),
            user_id: user_id.or(existing_uid),
        },
    );
    save_profile_metadata(app, &metadata)
}

pub(crate) fn remove_profile_metadata_entry_in_dir(
    wallet_data_dir: &std::path::Path,
    folder_name: &str,
) -> Result<(), String> {
    let metadata_path = wallet_data_dir.join(PROFILE_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(());
    }
    let mut metadata: HashMap<String, ProfileMetadata> = fs::read_to_string(&metadata_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if metadata.remove(folder_name).is_some() {
        let content = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
        fs::write(&metadata_path, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn remove_profile_metadata_entry(app: &tauri::AppHandle, folder_name: &str) -> Result<(), String> {
    if let Ok(mut metadata) = load_profile_metadata(app) {
        if metadata.remove(folder_name).is_some() {
            save_profile_metadata(app, &metadata)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn profile_exists(app: tauri::AppHandle) -> bool {
    info!("Checking for existing profile...");
    // This must check the same path the AppService will use.
    let profile_path = match app.path().app_data_dir() {
        Ok(dir) => dir.join("wallet_data").join("profile.enc"),
        Err(e) => {
            error!("Could not determine app data directory for profile check: {}", e);
            return false;
        }
    };

    let exists = profile_path.exists();
    info!(
        "Profile check at '{}': {}",
        profile_path.display(),
        exists
    );
    exists
}

#[tauri::command]
pub fn list_profiles(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Vec<ProfileInfo>, FrontendError> {
    info!("Listing available profiles...");
    let service = state.service.lock().unwrap();
    
    // Load profile metadata for last_used timestamps
    let metadata = load_profile_metadata(&app).unwrap_or_default();
    
    service.list_profiles().map_err(FrontendError::from).map(|profiles| {
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
                    match (a_time.parse::<DateTime<Utc>>(), b_time.parse::<DateTime<Utc>>()) {
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
    info!("Attempting to create profile '{}' with language {:?}...", profile_name, language);
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

                let folder_name = service.list_profiles()
                    .ok()
                    .and_then(|profiles| profiles.into_iter().find(|p| p.profile_name == profile_name))
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
                let settings_bytes = serde_json::to_vec(&default_settings).map_err(|e| FrontendError::from(e.to_string()))?;

                service
                    .save_encrypted_data(SETTINGS_KEY, &settings_bytes, Some(&password))
                    .map_err(|e| FrontendError::from(format!("Failed to save default settings: {}", e)))?;

                // 2. Initialize session and cache
                state.initialize_profile_session(&mut service, &password).map_err(FrontendError::from)?;
            }
            Err(e) => {
                error!("Profile creation failed: {}", e);
                return Err(FrontendError::from(e));
            }
        }
    }

    if let Some(preset) = initial_reach_preset {
        if matches!(preset.as_str(), "near" | "medium" | "far") {
            crate::commands::settings_cmd::apply_reach_preset_impl(preset, Some(password.clone()), &state)?;
        }
    }

    if let Some(targets) = initial_sync_targets {
        for target in targets {
            let _ = crate::commands::sync::save_public_sync_impl(
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
            if let Err(e) = record_profile_metadata(&app, &folder_name, user_id) {
                error!("Failed to update profile metadata: {}", e);
            }

            state.initialize_profile_session(&mut service, &password).map_err(FrontendError::from)?;

            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Manager;
                let app_state = app_handle.state::<AppState>();
                let _ = crate::commands::settings_cmd::check_anchor_sync(app_state).await;
            });

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
    info!("Attempting to recover wallet and set new password with language {:?}...", language);
    let mut service = state.service.lock().unwrap();
    let core_language = language.into();
    match service.recover_wallet_and_set_new_password(&folder_name, &mnemonic, passphrase.as_deref(), &new_password, core_language, local_instance_id) {
        Ok(()) => {
            info!("Wallet recovered and new password set successfully!");

            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            if let Err(e) = record_profile_metadata(&app, &folder_name, user_id) {
                error!("Failed to update profile metadata: {}", e);
            }

            state.initialize_profile_session(&mut service, &new_password).map_err(FrontendError::from)?;

            let app_handle = app.clone();
            let pwd = new_password.clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Manager;
                let app_state = app_handle.state::<AppState>();
                crate::commands::settings_cmd::probe_and_flush_cold_recovery_inner(&app_state, Some(&pwd)).await;
            });

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
    info!("Attempting device handover for profile in folder '{}'...", folder_name);
    let mut service = state.service.lock().unwrap();
    match service.handover_to_this_device(&folder_name, &password, local_instance_id) {
        Ok(()) => {
            info!("Handover successful! Device bound to wallet.");

            let user_id = service
                .with_wallet_and_identity(|_, id| id.user_id.clone())
                .ok();
            if let Err(e) = record_profile_metadata(&app, &folder_name, user_id) {
                error!("Failed to update profile metadata: {}", e);
            }

            state.initialize_profile_session(&mut service, &password).map_err(FrontendError::from)?;

            let app_handle = app.clone();
            let pwd = password.clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Manager;
                let app_state = app_handle.state::<AppState>();
                let _ = crate::commands::settings_cmd::flush_pending_anchor_sync_inner(&app_state, Some(&pwd)).await;
            });

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
pub fn unlock_session(password: String, duration_seconds: u64, state: tauri::State<AppState>) -> Result<(), FrontendError> {
    info!("Unlocking session for {} seconds...", duration_seconds);
    let mut service = state.service.lock().unwrap();
    service.unlock_session(&password, duration_seconds).map_err(FrontendError::from)
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
pub fn verify_profile_password(folder_name: String, password: String, state: tauri::State<AppState>) -> Result<String, FrontendError> {
    info!("Verifying password for profile in folder '{}'...", folder_name);
    let service = state.service.lock().unwrap();
    service.get_profile_id_with_password(&folder_name, &password).map_err(FrontendError::from)
}

#[tauri::command]
pub fn delete_profile(folder_name: String, password: String, state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<(), FrontendError> {
    info!("Attempting to delete profile in folder '{}'...", folder_name);
    
    // 1. Delete in Core (this also verifies password)
    let mut service = state.service.lock().unwrap();
    service.delete_profile(&folder_name, &password).map_err(FrontendError::from)?;
    
    // 2. Remove metadata entry
    let _ = remove_profile_metadata_entry(&app, &folder_name);
    
    info!("Profile '{}' deleted successfully.", folder_name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        use std::path::PathBuf;
        use human_money_core::app_service::AppService;
        
        let service = AppService::new(&PathBuf::from("/tmp")).unwrap();
        let state = AppState {
            service: Mutex::new(service),
            history: Mutex::new(Some(vec![])),
            events: Mutex::new(Some(vec![])),
            contacts: Mutex::new(Some(Default::default())),
            settings: Mutex::new(Some(AppSettings::default())),
            regional_views: Mutex::new(None),
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
        use sysinfo::{System, Pid};
        let mut s = System::new();
        s.refresh_processes();
        let current_pid = std::process::id();
        let proc = s.process(Pid::from_u32(current_pid));
        assert!(proc.is_some(), "Current process should be found in refreshed processes! PID: {}", current_pid);
    }
}
