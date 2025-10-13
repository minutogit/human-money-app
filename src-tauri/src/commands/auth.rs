use crate::models::ProfileInfo;
use crate::settings::{AppSettings, SETTINGS_KEY};
use crate::{
    commands::actions::{load_history_from_disk, TRANSACTION_HISTORY_KEY},
    AppState,
};
use chrono::{Duration, Utc};
use log::{error, info};
use tauri::Manager;

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
pub fn list_profiles(state: tauri::State<AppState>) -> Result<Vec<ProfileInfo>, String> {
    info!("Listing available profiles...");
    let service = state.service.lock().unwrap();
    service.list_profiles().map(|profiles| {
        profiles
            .into_iter()
            .map(|p| ProfileInfo {
                profile_name: p.profile_name,
                folder_name: p.folder_name,
            })
            .collect()
    })
}

#[tauri::command]
pub fn create_profile(
    profile_name: String,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Attempting to create profile '{}'...", profile_name);
    let mut service = state.service.lock().unwrap();
    match service.create_profile(&profile_name, &mnemonic, passphrase.as_deref(), user_prefix.as_deref(), &password) {
        Ok(()) => {
            info!("Profile created successfully!");
            Ok(())
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
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Attempting to login...");
    let mut service = state.service.lock().unwrap();
    match service.login(&folder_name, &password, cleanup_on_login) {
        Ok(()) => {
            info!("Login successful!");

            // Lade oder erstelle die Einstellungen
            let settings: AppSettings = match service.load_encrypted_data(SETTINGS_KEY, &password) {
                Ok(data) => serde_json::from_slice(&data).unwrap_or_default(),
                Err(_) => {
                    info!("No settings file found, creating default settings.");
                    let default_settings = AppSettings::default();
                    let bytes = serde_json::to_vec(&default_settings).unwrap();
                    service
                        .save_encrypted_data(SETTINGS_KEY, &bytes, &password)
                        .map_err(|e| format!("Failed to save default settings: {}", e))?;
                    default_settings
                }
            };

            // Lade die Transaktionshistorie und führe die Bereinigung durch
            let mut history = load_history_from_disk(&service, &password)?;
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
                    .save_encrypted_data(TRANSACTION_HISTORY_KEY, &history_bytes, &password)
                    .map_err(|e| format!("Failed to save cleaned history: {}", e))?;
            }

            // Speichere die geladenen Daten im AppState
            *state.settings.lock().unwrap() = Some(settings);
            *state.history.lock().unwrap() = Some(history);

            Ok(())
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
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Attempting to recover wallet and set new password...");
    let mut service = state.service.lock().unwrap();
    match service.recover_wallet_and_set_new_password(&folder_name, &mnemonic, passphrase.as_deref(), &new_password) {
        Ok(()) => {
            info!("Wallet recovered and new password set successfully!");
            Ok(())
        }
        Err(e) => {
            error!("Wallet recovery failed: {}", e);
            Err(e)
        }
    }
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