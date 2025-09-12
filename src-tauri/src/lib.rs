// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use log::{error, info};
use tauri_plugin_log::{Builder as LogBuilder, log::LevelFilter, Target, TargetKind};
use voucher_lib::app_service::AppService;
use voucher_lib::wallet::VoucherSummary;

pub struct AppState(Mutex<AppService>);

#[tauri::command]
fn profile_exists() -> bool {
    // This check is independent of the AppService state and looks for the wallet file.
    info!("Checking for existing profile...");
    let exists = Path::new("./wallet_data/profile.enc").exists();
    info!("Profile exists: {}", exists);
    exists
}

#[tauri::command]
fn generate_mnemonic(word_count: u32) -> Result<String, String> {
    info!("Generating a new {}-word mnemonic", word_count);
    AppService::generate_mnemonic(word_count)
}

#[tauri::command]
fn create_profile(
    mnemonic: String,
    user_prefix: Option<String>,
    password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!(
        "Attempting to create profile with prefix: {:?}",
        user_prefix.as_deref().unwrap_or("none")
    );
    let mut service = state.0.lock().unwrap();
    match service.create_profile(&mnemonic, user_prefix.as_deref(), &password) {
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
fn login(
    password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Attempting to login...");
    let mut service = state.0.lock().unwrap();
    match service.login(&password) {
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
fn recover_wallet_and_set_new_password(
    mnemonic: String,
    new_password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Attempting to recover wallet and set new password...");
    let mut service = state.0.lock().unwrap();
    match service.recover_wallet_and_set_new_password(&mnemonic, &new_password) {
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
fn logout(state: tauri::State<AppState>) {
    info!("Logging out...");
    let mut service = state.0.lock().unwrap();
    service.logout();
    info!("Logout complete.");
}

#[tauri::command]
fn get_user_id(state: tauri::State<AppState>) -> Result<String, String> {
    let service = state.0.lock().unwrap();
    service.get_user_id()
}

#[tauri::command]
fn get_total_balance_by_currency(
    state: tauri::State<AppState>,
) -> Result<HashMap<String, String>, String> {
    let service = state.0.lock().unwrap();
    service.get_total_balance_by_currency()
}

#[tauri::command]
fn get_voucher_summaries(
    state: tauri::State<AppState>,
) -> Result<Vec<VoucherSummary>, String> {
    let service = state.0.lock().unwrap();
    service.get_voucher_summaries()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let service = AppService::new(Path::new("./wallet_data")).expect("Failed to create AppService");

    tauri::Builder::default()
        .plugin(
            LogBuilder::default()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::Webview),
                ])
                .level(LevelFilter::Info)
                .build(),
        )
        .manage(AppState(Mutex::new(service)))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // Profile management
            profile_exists,
            create_profile,
            login,
            recover_wallet_and_set_new_password,
            logout,
            // Helpers
            generate_mnemonic,
            // Queries
            get_user_id,
            get_total_balance_by_currency,
            get_voucher_summaries
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}