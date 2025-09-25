// src-tauri/src/lib.rs
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod commands;
pub mod models;

use crate::commands::{actions::*, auth::*, queries::*, utils::*};
use chrono::Local;
use std::fs;
use std::sync::Mutex;

use log::info;
use tauri::Manager;
use tauri_plugin_log::{log::LevelFilter, Builder as LogBuilder, Target, TargetKind, TimezoneStrategy};
use voucher_lib::app_service::AppService;

pub struct AppState(Mutex<AppService>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Get the application's data directory
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to find app data directory");

            // We'll create a subdirectory for our wallet data to keep things tidy
            let wallet_path = data_dir.join("wallet_data");
            if !wallet_path.exists() {
                fs::create_dir_all(&wallet_path).expect("Failed to create wallet data directory");
            }
            info!("Using wallet data path: {}", wallet_path.display());

            // Create the AppService with the correct, platform-specific path
            let service = AppService::new(&wallet_path).expect("Failed to create AppService");
            app.manage(AppState(Mutex::new(service)));
            Ok(())
        })
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
            get_bip39_wordlist,
            get_voucher_standards,
            validate_mnemonic,
            // Queries
            get_user_id,
            get_total_balance_by_currency,
            get_voucher_summaries,
            get_voucher_details,
            create_new_voucher,
            // Logging
            frontend_log,
            log_to_backend
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}