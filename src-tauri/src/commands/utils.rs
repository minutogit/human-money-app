use crate::models::VoucherStandardInfo;
use bip39::Language;
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use log::{error, info, warn};
use std::fs;
use voucher_lib::app_service::AppService;
use tauri::Manager;

#[tauri::command]
pub fn generate_mnemonic(word_count: u32) -> Result<String, String> {
    info!("Generating a new {}-word mnemonic", word_count);
    AppService::generate_mnemonic(word_count)
}

#[tauri::command]
pub fn validate_mnemonic(mnemonic: String) -> Result<(), String> {
    info!("Validating mnemonic...");
    AppService::validate_mnemonic(&mnemonic)
}

#[tauri::command]
pub fn get_voucher_standards(app: tauri::AppHandle) -> Result<Vec<VoucherStandardInfo>, String> {
    info!("Ensuring voucher standards are available in app data directory...");

    // 1. Define paths.
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let standards_dir_in_data = app_data_dir.join("voucher_standards");
    info!("App data standards directory is set to: {}", standards_dir_in_data.display());

    // 2. "First Run" Logic: If the standards directory doesn't exist, copy it from the app bundle.
    let needs_copy = if !standards_dir_in_data.exists() {
        info!("Standards directory does not exist. Will copy.");
        true
    } else {
        // Directory exists, check if it is empty.
        match fs::read_dir(&standards_dir_in_data) {
            Ok(mut dir) => match dir.next().is_none() {
                true => { info!("Standards directory exists but is empty. Will copy."); true },
                false => { info!("Standards directory exists and is not empty. No copy needed."); false }
            },
            Err(e) => {
                error!("Could not read existing standards directory, will attempt to overwrite: {}", e);
                true // If we can't even read it, better to try a fresh copy.
            }
        }
    };
    if needs_copy {
        fs::create_dir_all(&standards_dir_in_data)
            .map_err(|e| format!("Failed to create standards directory in app data: {}", e))?;

        #[cfg(debug_assertions)]
        {
            let standards_source_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../voucher_standards");
            info!("Attempting to copy standards from DEV source: {}", standards_source_path.display());
            // KORREKTUR: Die Logik wird an die des Release-Builds angeglichen, um Konsistenz zu gewährleisten.
            // Es wird nur der Inhalt in das Zielverzeichnis kopiert.
            let options = CopyOptions { overwrite: true, content_only: true, ..Default::default() };
            copy_dir(&standards_source_path, &standards_dir_in_data, &options)
                .map_err(|e| format!("Failed to copy standards from DEV source: {}", e))?;
        }
        #[cfg(not(debug_assertions))]
        {
            let resource_dir = app.path().resource_dir()
                .map_err(|e| format!("Failed to get resource directory: {}", e))?;
            // The tauri bundler can create a nested `_up_/voucher_standards` dir from `../` paths.
            // We need to specifically target this path as our source.
            let standards_source_path = resource_dir.join("_up_").join("voucher_standards");
            info!("Attempting to copy standards from AppImage resource path: {}", standards_source_path.display());
            let options = CopyOptions { overwrite: true, content_only: true, ..Default::default() };
            copy_dir(&standards_source_path, &standards_dir_in_data, &options)
                .map_err(|e| format!("Failed to copy standards from resources to app data: {}", e))?;
        }
        info!("Default standards copied successfully.");
    }

    // 3. Read standards from the user's app data directory.
    info!("Reading voucher standards from: {}", standards_dir_in_data.display());
    let mut standards = Vec::new();
    match fs::read_dir(&standards_dir_in_data) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    info!("[Debug] Found entry in standards directory: {}", path.display());
                    if path.is_dir() {
                        let standard_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        info!("[Debug] Found potential standard directory with ID: '{}'", standard_id);
                        let toml_path = path.join("standard.toml");
                        info!("[Debug] Checking for standard file at: {}", toml_path.display());
                        if toml_path.exists() {
                             let content = fs::read_to_string(&toml_path)
                                .map_err(|e| format!("Failed to read {}: {}", toml_path.display(), e))?;
                            info!("[Debug] Successfully read '{}', adding to list.", toml_path.display());
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

    info!("[Debug] Final standards loaded: {:?}", standards.iter().map(|s| &s.id).collect::<Vec<_>>());
    info!("Found {} voucher standards.", standards.len());
    Ok(standards)
}

#[tauri::command]
pub fn get_bip39_wordlist() -> Vec<&'static str> {
    info!("Fetching BIP-39 English wordlist for frontend.");
    Language::English.word_list().iter().copied().collect()
}

#[tauri::command]
pub fn frontend_log(message: String) {
    info!("[Frontend]: {}", message);
}


#[tauri::command]
pub async fn log_to_backend(level: String, message: String) -> Result<(), String> {
    match level.as_str() {
        "info" => info!("[Frontend]: {}", message),
        "warn" => warn!("[Frontend]: {}", message),
        "error" => error!("[Frontend]: {}", message),
        _ => info!("[Frontend, {level}]: {}", message),
    }
    Ok(())
}