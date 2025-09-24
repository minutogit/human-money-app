// src-tauri/src/lib.rs
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::Local;
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use bip39::Language;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use tauri_plugin_log::{log::LevelFilter, Builder as LogBuilder, Target, TargetKind, TimezoneStrategy};
use voucher_lib::app_service::AppService;
use voucher_lib::models::voucher::Voucher;
use voucher_lib::services::voucher_manager::NewVoucherData;
use voucher_lib::wallet::{AggregatedBalance, VoucherSummary};

pub struct AppState(Mutex<AppService>);

// A local struct that mirrors `voucher_lib::...::NewVoucherData` but derives `Deserialize`.
// This is necessary because Tauri needs to deserialize the JSON payload from the frontend,
// and we cannot add `#[derive(Deserialize)]` to the original struct in the upstream library.
#[derive(Deserialize, Debug)]
struct NominalValueData {
    amount: String,
    unit: String,
}

#[derive(Deserialize, Debug)]
struct FrontendAddressData {
    street: String,
    house_number: String,
    zip_code: String,
    city: String,
    country: String,
    full_address: String,
}

#[derive(Deserialize, Debug)]
struct FrontendCollateralData {
    amount: String,
    unit: String,
    abbreviation: String,
}

#[derive(Deserialize, Debug)]
struct FrontendCreatorData {
    first_name: String,
    last_name: String,
    address: FrontendAddressData,
    organization: Option<String>,
    community: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    url: Option<String>,
    gender: String,
    service_offer: Option<String>,
    needs: Option<String>,
    coordinates: String,
}

#[derive(Deserialize, Debug)]
struct FrontendNewVoucherData {
    nominal_value: NominalValueData,
    creator: FrontendCreatorData,
    validity_duration: Option<String>,
    non_redeemable_test_voucher: bool,
    collateral: FrontendCollateralData,
}

#[derive(Serialize, Clone)]
pub struct VoucherStandardInfo {
    id: String,
    content: String,
}

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
fn validate_mnemonic(mnemonic: String) -> Result<(), String> {
    info!("Validating mnemonic...");
    AppService::validate_mnemonic(&mnemonic)
}

#[tauri::command]
fn create_profile(
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!(
        "Attempting to create profile with prefix: {:?}",
        user_prefix.as_deref().unwrap_or("none")
    );
    let mut service = state.0.lock().unwrap();
    match service.create_profile(&mnemonic, passphrase.as_deref(), user_prefix.as_deref(), &password) {
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
fn login(password: String, state: tauri::State<AppState>) -> Result<(), String> {
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
    passphrase: Option<String>,
    new_password: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    info!("Attempting to recover wallet and set new password...");
    let mut service = state.0.lock().unwrap();
    match service.recover_wallet_and_set_new_password(&mnemonic, passphrase.as_deref(), &new_password) {
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
fn get_total_balance_by_currency(state: tauri::State<AppState>) -> Result<Vec<AggregatedBalance>, String> {
    let service = state.0.lock().unwrap();
    service.get_total_balance_by_currency()
}

#[tauri::command]
fn get_voucher_summaries(state: tauri::State<AppState>) -> Result<Vec<VoucherSummary>, String> {
    let service = state.0.lock().unwrap();
    service.get_voucher_summaries(None, None)
}

#[tauri::command]
fn get_voucher_details(local_id: String, state: tauri::State<AppState>) -> Result<Voucher, String> {
    let service = state.0.lock().unwrap();
    service.get_voucher_details(&local_id).map(|details| details.voucher)
}

#[tauri::command]
fn get_voucher_standards(app: tauri::AppHandle) -> Result<Vec<VoucherStandardInfo>, String> {
    info!("Ensuring voucher standards are available in app data directory...");

    // 1. Define paths. We map the `tauri::Error` to a `String` to match the function's error type.
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let standards_dir_in_data = app_data_dir.join("voucher_standards");

    // 2. "First Run" Logic: If the standards directory doesn't exist in the user's data folder, copy it from the app bundle.
    if !standards_dir_in_data.exists() {
        info!("First run detected or standards directory missing. Copying default standards...");
        fs::create_dir_all(&standards_dir_in_data)
            .map_err(|e| format!("Failed to create standards directory in app data: {}", e))?;

        // In debug mode, we point directly to the source directory.
        // In release mode, we use the bundled resource path.
        #[cfg(debug_assertions)]
        let resource_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../voucher_standards");
        #[cfg(not(debug_assertions))]
        let resource_path = app
            .path()
            .resolve("voucher_standards", tauri::path::BaseDirectory::Resource)
            .map_err(|e| format!("Failed to resolve resource path: {}", e))?;

        // Use the robust `fs_extra` library to copy the directory contents.
        // The destination is the parent (`app_data_dir`), and `fs_extra` will place
        // the `voucher_standards` folder inside it.
        let mut options = CopyOptions::new();
        options.overwrite = true;
        copy_dir(resource_path, &app_data_dir, &options)
            .map_err(|e| format!("Failed to copy standards from resources to app data: {}", e))?;
        info!("Default standards copied successfully.");
    }

    // 3. Read standards from the user's app data directory
    info!("Reading voucher standards from: {}", standards_dir_in_data.display());
    let mut standards = Vec::new();
    match fs::read_dir(&standards_dir_in_data) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() {
                        let standard_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let toml_path = path.join("standard.toml");
                        if toml_path.exists() {
                            let content = fs::read_to_string(&toml_path)
                                .map_err(|e| format!("Failed to read {}: {}", toml_path.display(), e))?;
                            standards.push(VoucherStandardInfo {
                                id: standard_id,
                                content,
                            });
                        }
                    }
                }
            }
        }
        Err(e) => {
            return Err(format!(
                "Failed to read standards directory '{}': {}",
                standards_dir_in_data.display(),
                e
            ))
        }
    };

    info!("Found {} voucher standards.", standards.len());
    Ok(standards)
}

#[tauri::command]
fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: String,
    state: tauri::State<AppState>,
) -> Result<Voucher, String> {
    info!("Attempting to create a new voucher...");
    let mut service = state.0.lock().unwrap();
    // For the prototype, language preference is hardcoded.
    let lang_preference = "en-US";

    // Get the current user's ID to use as the creator ID. This fixes the "Invalid creator ID" error.
    let user_id = service.get_user_id()?;

    // Convert the frontend data struct to the library's data struct
    let voucher_data = NewVoucherData {
        nominal_value: voucher_lib::models::voucher::NominalValue {
            amount: data.nominal_value.amount,
            unit: data.nominal_value.unit,
            ..Default::default()
        },
        collateral: voucher_lib::models::voucher::Collateral {
            amount: data.collateral.amount,
            unit: data.collateral.unit,
            abbreviation: data.collateral.abbreviation,
            ..Default::default()
        },
        creator: voucher_lib::models::voucher::Creator {
            id: user_id,
            first_name: data.creator.first_name,
            last_name: data.creator.last_name,
            address: voucher_lib::models::voucher::Address {
                street: data.creator.address.street,
                house_number: data.creator.address.house_number,
                zip_code: data.creator.address.zip_code,
                city: data.creator.address.city,
                country: data.creator.address.country,
                full_address: data.creator.address.full_address,
            },
            organization: data.creator.organization,
            community: data.creator.community,
            phone: data.creator.phone,
            email: data.creator.email,
            url: data.creator.url,
            gender: data.creator.gender,
            service_offer: data.creator.service_offer,
            needs: data.creator.needs,
            coordinates: data.creator.coordinates,
            ..Default::default()
        },
        validity_duration: data.validity_duration,
        non_redeemable_test_voucher: data.non_redeemable_test_voucher,
    };

    service.create_new_voucher(&standard_toml_content, lang_preference, voucher_data, &password)
}

#[tauri::command]
fn get_bip39_wordlist() -> Vec<&'static str> {
    info!("Fetching BIP-39 English wordlist for frontend.");
    Language::English.word_list().iter().copied().collect()
}

#[tauri::command]
fn frontend_log(message: String) {
    info!("[Frontend]: {}", message);
}

#[tauri::command]
async fn log_to_backend(level: String, message: String) -> Result<(), String> {
    match level.as_str() {
        "info" => info!("[Frontend]: {}", message),
        "warn" => warn!("[Frontend]: {}", message),
        "error" => error!("[Frontend]: {}", message),
        _ => info!("[Frontend, {level}]: {}", message),
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let service = AppService::new(Path::new("./wallet_data")).expect("Failed to create AppService");

    tauri::Builder::default()
        .plugin(
            LogBuilder::default()
                .targets([Target::new(TargetKind::Stdout), Target::new(TargetKind::Webview)])
                .level(LevelFilter::Info)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .format(move |out, message, record| {
                    let now = Local::now();
                    let date = now.format("%Y-%m-%d");
                    let time = now.format("%H:%M:%S");
                    out.finish(format_args!(
                        "[{date}][{time}][{level}] {message}",
                        date = date,
                        time = time,
                        level = record.level(),
                        message = message
                    ));
                })
                .build(),
        )
        .manage(AppState(Mutex::new(service)))
        .plugin(tauri_plugin_clipboard_manager::init())
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
            validate_mnemonic,
            // Queries
            get_user_id,
            get_total_balance_by_currency,
            get_voucher_summaries,
            get_voucher_details,
            get_bip39_wordlist,
            get_voucher_standards,
            create_new_voucher,
            // Logging
            frontend_log,
            log_to_backend
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}