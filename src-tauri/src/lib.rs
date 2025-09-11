use std::path::Path;
use std::sync::Mutex;

use log::{error, info};
use voucher_lib::app_service::AppService;
use tauri_plugin_log::{Builder as LogBuilder, Target, TargetKind, log::LevelFilter};

pub struct AppState(Mutex<AppService>);

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
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
    // Greife auf den AppService im State zu und sperre ihn für diesen Vorgang
    let mut service = state.0.lock().unwrap();
    // Rufe die eigentliche Funktion der voucher_lib auf
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
        .invoke_handler(tauri::generate_handler![greet, create_profile])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
