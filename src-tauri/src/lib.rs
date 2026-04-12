// src-tauri/src/lib.rs
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod commands;
pub mod models;
pub mod settings;

use chrono::Local;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::Mutex;

use log::info;
use tauri::{AppHandle, Manager};
use tauri_plugin_log::{
    log::LevelFilter, Builder as LogBuilder, Target, TargetKind, TimezoneStrategy,
};
use human_money_core::app_service::AppService;

use crate::commands::{actions::*, auth::*, queries::*, utils::*, contacts::*};
use crate::commands::actions::TransactionRecord;
use crate::settings::AppSettings;

const LOG_TARGET_NAME: &str = "human_money_app.log";

/// This function is called at startup to implement a simple log rotation.
fn setup_log_rotation(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = app.path().app_log_dir()?;
    if !log_dir.exists() {
        fs::create_dir_all(&log_dir)?;
    }
    let log_file_path = log_dir.join(LOG_TARGET_NAME);

    // If the log file doesn't exist at this early stage, create it.
    // This resolves the race condition where the logger creates the file after this function has run.
    if !log_file_path.exists() {
        File::create(&log_file_path)?;
    }

    // Now that we know the file exists, read it and rotate if not empty.
    let lines: Vec<String> =
        BufReader::new(File::open(&log_file_path)?).lines().filter_map(Result::ok).collect();

    if !lines.is_empty() {
        let total_lines = lines.len();
        let lines_to_skip = (total_lines as f64 * 0.20).ceil() as usize;
        let lines_to_keep = lines.into_iter().skip(lines_to_skip).collect::<Vec<String>>();
        let mut log_file = File::create(&log_file_path)?;
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        writeln!(log_file, "--- Log rotated at {} (top 20% removed) ---", timestamp)?;
        log_file.write_all(lines_to_keep.join("\n").as_bytes())?;
    }

    Ok(())
}

pub struct AppState {
    pub service: Mutex<AppService>,
    pub history: Mutex<Option<Vec<TransactionRecord>>>,
    pub settings: Mutex<Option<AppSettings>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Setup log rotation. Panicking here is acceptable because logging is a critical feature.
            if let Err(e) = setup_log_rotation(&app.handle()) {
                // We can't use the logger here yet, so we'll panic with a detailed error.
                panic!("Failed to setup log rotation: {}", e);
            }

            // Get the application's data directory
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to find app data directory");

            let wallet_path = data_dir.join("wallet_data");
            if !wallet_path.exists() {
                fs::create_dir_all(&wallet_path).expect("Failed to create wallet data directory");
            }
            info!("Using wallet data path: {}", wallet_path.display());

            let service = AppService::new(&wallet_path).expect("Failed to create AppService");
            app.manage(AppState {
                service: Mutex::new(service),
                history: Mutex::new(None),
                settings: Mutex::new(None),
            });

            Ok(())
        })
        .plugin(
            LogBuilder::default()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::Webview),
                    Target::new(TargetKind::LogDir { file_name: Some("human_money_app".into()) }),
                ])
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
        // Add the new plugins here
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            profile_exists, list_profiles, create_profile, login, recover_wallet_and_set_new_password, logout,
            unlock_session, lock_session, refresh_session_activity, // <--- NEU
            generate_mnemonic, get_bip39_wordlist, get_voucher_standards, validate_mnemonic,
            get_user_id, get_user_profile, get_total_balance_by_currency, get_voucher_summaries, get_voucher_details,
            create_new_voucher, create_transfer_bundle, receive_bundle, save_transaction_record,
            get_transaction_history,
            get_app_settings, save_app_settings,
            update_user_profile,
            frontend_log, log_to_backend,
            // Multi-signature commands
            create_signing_request_bundle, open_voucher_signing_request, create_detached_signature_response_bundle,
            process_and_attach_signature, get_allowed_signature_roles_from_standard,
            // Address Book
            get_contacts, save_contact, delete_contact
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}