use crate::models::{ProfileInfo, MnemonicLanguage};
use crate::settings::{AppSettings, SETTINGS_KEY};
use crate::{
    commands::actions::{load_history_from_disk, TRANSACTION_HISTORY_KEY},
    AppState,
};
use chrono::{DateTime, Duration, Utc};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

const PROFILE_METADATA_FILE: &str = "profile_metadata.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProfileMetadata {
    last_used: String,
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

fn update_profile_last_used(app: &tauri::AppHandle, folder_name: &str) -> Result<(), String> {
    let mut metadata = load_profile_metadata(app)?;
    
    metadata.insert(
        folder_name.to_string(),
        ProfileMetadata {
            last_used: Utc::now().to_rfc3339(),
        },
    );
    
    save_profile_metadata(app, &metadata)
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
pub fn list_profiles(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Vec<ProfileInfo>, String> {
    info!("Listing available profiles...");
    let service = state.service.lock().unwrap();
    
    // Load profile metadata for last_used timestamps
    let metadata = load_profile_metadata(&app).unwrap_or_default();
    
    service.list_profiles().map(|profiles| {
        let mut profile_infos: Vec<ProfileInfo> = profiles
            .into_iter()
            .map(|p| {
                let last_used = metadata.get(&p.folder_name).map(|m| m.last_used.clone());
                ProfileInfo {
                    profile_name: p.profile_name,
                    folder_name: p.folder_name,
                    last_used,
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
pub fn create_profile(
    profile_name: String,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    password: String,
    local_instance_id: String,
    language: MnemonicLanguage,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    info!("Attempting to create profile '{}' with language {:?}...", profile_name, language);
    let mut service = state.service.lock().unwrap();
    let core_language = language.into();
    match service.create_profile(&profile_name, &mnemonic, passphrase.as_deref(), user_prefix.as_deref(), &password, core_language, local_instance_id) {
        Ok(()) => {
            info!("Profile created successfully!");

            // Get the folder_name by listing profiles and finding the one matching profile_name
            let folder_name = service.list_profiles()
                .ok()
                .and_then(|profiles| profiles.into_iter().find(|p| p.profile_name == profile_name))
                .map(|p| p.folder_name);

            // Update last_used timestamp for the new profile
            if let Some(ref folder) = folder_name {
                if let Err(e) = update_profile_last_used(&app, folder) {
                    error!("Failed to update profile last_used timestamp: {}", e);
                    // Don't fail profile creation if metadata update fails
                }
            }

            // 1. Initialisiere und speichere Standard-Einstellungen für das neue Profil
            let default_settings = AppSettings::default();
            let settings_bytes = serde_json::to_vec(&default_settings).map_err(|e| e.to_string())?;
            
            service
                .save_encrypted_data(SETTINGS_KEY, &settings_bytes, Some(&password))
                .map_err(|e| format!("Failed to save default settings: {}", e))?;

            // 2. Initialisiere die Session und den Cache (lädt Settings & leere History)
            initialize_profile_session(&mut service, &password, &state)
        }
        Err(e) => {
            error!("Profile creation failed: {}", e);
            Err(e)
        }
    }
}


#[tauri::command]
pub fn login(
    folder_name: String,
    password: String,
    cleanup_on_login: bool,
    local_instance_id: String,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    info!("Attempting to login...");
    let mut service = state.service.lock().unwrap();
    match service.login(&folder_name, &password, cleanup_on_login, local_instance_id) {
        Ok(()) => {
            info!("Login successful!");

            // Update last_used timestamp for this profile
            if let Err(e) = update_profile_last_used(&app, &folder_name) {
                error!("Failed to update profile last_used timestamp: {}", e);
                // Don't fail login if metadata update fails
            }

            // Initialisiere die Session und den Cache
            initialize_profile_session(&mut service, &password, &state)
        }
        Err(e) => {
            error!("Login failed: {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub fn recover_wallet_and_set_new_password(
    folder_name: String,
    mnemonic: String,
    passphrase: Option<String>,
    new_password: String,
    local_instance_id: String,
    language: MnemonicLanguage,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Attempting to recover wallet and set new password with language {:?}...", language);
    let mut service = state.service.lock().unwrap();
    let core_language = language.into();
    match service.recover_wallet_and_set_new_password(&folder_name, &mnemonic, passphrase.as_deref(), &new_password, core_language, local_instance_id) {
        Ok(()) => {
            info!("Wallet recovered and new password set successfully!");
            
            // NEU: Nach der Recovery müssen wir die Session und den Cache ebenfalls initialisieren,
            // da der User im Frontend direkt ins Dashboard (logged_in) wechselt.
            initialize_profile_session(&mut service, &new_password, &state)
        }
        Err(e) => {
            error!("Wallet recovery failed: {}", e);
            Err(e)
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
) -> Result<(), String> {
    info!("Attempting device handover for profile in folder '{}'...", folder_name);
    let mut service = state.service.lock().unwrap();
    match service.handover_to_this_device(&folder_name, &password, local_instance_id) {
        Ok(()) => {
            info!("Handover successful! Device bound to wallet.");
            
            // Update last_used timestamp
            if let Err(e) = update_profile_last_used(&app, &folder_name) {
                error!("Failed to update profile last_used timestamp: {}", e);
            }

            // Initialisiere die Session und den Cache
            initialize_profile_session(&mut service, &password, &state)
        }
        Err(e) => {
            error!("Handover failed: {}", e);
            Err(e)
        }
    }
}

/// Helper-Funktion zur Initialisierung der Sitzung und der AppState-Caches (Settings & History).
/// Wird nach Login, Profile-Erstellung (teilweise) und Recovery aufgerufen.
fn initialize_profile_session(
    service: &mut human_money_core::app_service::AppService,
    password: &str,
    state: &AppState,
) -> Result<(), String> {
    info!("Initializing profile session and member caches...");
    
    // 1. Lade oder erstelle die Einstellungen
    let settings: AppSettings = match service.load_encrypted_data(SETTINGS_KEY, Some(password)) {
        Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
        Err(_) => {
            info!("No settings file found, creating default settings.");
            let default_settings = AppSettings::default();
            let bytes = serde_json::to_vec(&default_settings).unwrap();
            service
                .save_encrypted_data(SETTINGS_KEY, &bytes, Some(password))
                .map_err(|e| format!("Failed to save default settings: {}", e))?;
            default_settings
        }
    };

    // 2. Wenn ein Timeout konfiguriert ist, starten wir direkt die Session.
    if settings.session_timeout_seconds > 0 {
        service.unlock_session(password, settings.session_timeout_seconds)?;
    }

    // 3. Lade die Transaktionshistorie und führe die Bereinigung durch
    let mut history = load_history_from_disk(service, Some(password))?;
    let mut changed = false;
    let retention_duration = Duration::days(settings.bundle_retention_days as i64);

    for record in history.iter_mut() {
        if record.bundle_data.is_empty() {
            continue;
        }
        if let Ok(timestamp) = record.timestamp.parse::<chrono::DateTime<Utc>>() {
            if Utc::now().signed_duration_since(timestamp) > retention_duration {
                info!("Clearing bundle data for old transaction record: {}", record.id);
                record.bundle_data.clear();
                changed = true;
            }
        }
    }

    if changed {
        info!("Saving cleaned transaction history back to disk...");
        let history_bytes = serde_json::to_vec(&history)
            .map_err(|e| format!("Failed to serialize cleaned history: {}", e))?;
        service
            .save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, Some(password))
            .map_err(|e| format!("Failed to save cleaned history: {}", e))?;
    }

    // 4. Speichere die geladenen Daten im AppState
    *state.settings.lock().unwrap() = Some(settings);
    *state.history.lock().unwrap() = Some(history);

    Ok(())
}


#[tauri::command]
pub fn logout(state: tauri::State<AppState>) {
    info!("Logging out...");
    // Leere die zwischengespeicherten Daten sicher aus dem Speicher
    *state.history.lock().unwrap() = None;
    *state.settings.lock().unwrap() = None;

    let mut service = state.service.lock().unwrap();
    service.logout();
    info!("Logout complete.");
}

// NEUE SESSION COMMANDS

#[tauri::command]
pub fn unlock_session(password: String, duration_seconds: u64, state: tauri::State<AppState>) -> Result<(), String> {
    info!("Unlocking session for {} seconds...", duration_seconds);
    let mut service = state.service.lock().unwrap();
    service.unlock_session(&password, duration_seconds)
}

#[tauri::command]
pub fn lock_session(state: tauri::State<AppState>) {
    info!("Locking session manually.");
    let mut service = state.service.lock().unwrap();
    service.lock_session();
}

#[tauri::command]
pub fn refresh_session_activity(state: tauri::State<AppState>) {
    // Loggt nicht, um Spam zu vermeiden
    let mut service = state.service.lock().unwrap();
    let _ = service.refresh_session_activity();
}

#[tauri::command]
pub fn verify_profile_password(folder_name: String, password: String, state: tauri::State<AppState>) -> Result<String, String> {
    info!("Verifying password for profile in folder '{}'...", folder_name);
    let service = state.service.lock().unwrap();
    service.get_profile_id_with_password(&folder_name, &password)
}

#[tauri::command]
pub fn delete_profile(folder_name: String, password: String, state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<(), String> {
    info!("Attempting to delete profile in folder '{}'...", folder_name);
    
    // 1. Delete in Core (this also verifies password)
    let mut service = state.service.lock().unwrap();
    service.delete_profile(&folder_name, &password)?;
    
    // 2. Remove metadata entry
    if let Ok(mut metadata) = load_profile_metadata(&app) {
        if metadata.remove(&folder_name).is_some() {
            let _ = save_profile_metadata(&app, &metadata);
        }
    }
    
    info!("Profile '{}' deleted successfully.", folder_name);
    Ok(())
}