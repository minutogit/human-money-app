use crate::AppState;
use crate::models::ProfileInfo;
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
    let service = state.0.lock().unwrap();
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
    let mut service = state.0.lock().unwrap();
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
pub fn login(folder_name: String, password: String, state: tauri::State<AppState>) -> Result<(), String> {
    info!("Attempting to login...");
    let mut service = state.0.lock().unwrap();
    match service.login(&folder_name, &password) {
        Ok(()) => {
            info!("Login successful!");
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
    let mut service = state.0.lock().unwrap();
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
    let mut service = state.0.lock().unwrap();
    service.logout();
    info!("Logout complete.");
}