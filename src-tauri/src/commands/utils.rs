use crate::models::{VoucherStandardInfo, MnemonicLanguage};
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use log::{error, info, warn};
use std::fs;
use human_money_core::app_service::AppService;
use tauri::Manager;

#[tauri::command]
pub fn generate_mnemonic(word_count: u32, language: MnemonicLanguage) -> Result<String, String> {
    info!("Generating a new {}-word mnemonic in language {:?}", word_count, language);
    let core_language = language.into();
    AppService::generate_mnemonic(word_count, core_language)
}

#[tauri::command]
pub fn validate_mnemonic(mnemonic: String, language: MnemonicLanguage) -> Result<(), String> {
    info!("Validating mnemonic with language {:?}...", language);
    let core_language = language.into();
    AppService::validate_mnemonic(&mnemonic, core_language)
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
            for entry in entries.flatten() {
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
                        info!("[Debug] Successfully read '{}', parsing for display name.", toml_path.display());
                        
                        let display_name = match human_money_core::services::standard_manager::verify_and_parse_standard(&content) {
                            Ok((def, _)) => def.immutable.identity.name.clone(),
                            Err(e) => {
                                warn!("Failed to parse standard TOML for {}: {}. Falling back to ID.", standard_id, e);
                                standard_id.clone()
                            }
                        };

                        standards.push(VoucherStandardInfo {
                            id: standard_id,
                            display_name,
                            content,
                        });
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
pub fn get_bip39_wordlist(language: MnemonicLanguage) -> Vec<&'static str> {
    info!("Fetching BIP-39 wordlist for language {:?} for frontend.", language);
    let core_language = language.into();
    AppService::get_mnemonic_wordlist(core_language)
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
#[tauri::command]
pub fn get_local_instance_id(app: tauri::AppHandle) -> Result<String, String> {
    // 1. Primary: OS-level Machine ID (clone-resistant) - only on Desktop
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "freebsd"))]
    {
        match machine_uid::get() {
            Ok(id) if !id.trim().is_empty() => {
                info!("Device binding: Using OS Machine ID.");
                return Ok(id.trim().to_string());
            }
            Ok(_) => {
                warn!("OS Machine ID returned empty. Falling back to file-based ID.");
            }
            Err(e) => {
                warn!("Failed to read OS Machine ID: {}. Falling back to file-based ID.", e);
            }
        }
    }

    // 2. Fallback: File-based UUID for sandboxed/restricted environments
    let app_config_dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    let fallback_path = app_config_dir.join("instance_id_fallback");

    if fallback_path.exists() {
        let id = std::fs::read_to_string(&fallback_path).map_err(|e| e.to_string())?;
        let trimmed = id.trim().to_string();
        if !trimmed.is_empty() {
            info!("Device binding: Using fallback file-based ID.");
            return Ok(trimmed);
        }
    }

    // 3. Generate new fallback (first run in sandboxed environment)
    if !app_config_dir.exists() {
        std::fs::create_dir_all(&app_config_dir).map_err(|e| e.to_string())?;
    }
    let new_id = uuid::Uuid::new_v4().to_string();
    std::fs::write(&fallback_path, &new_id).map_err(|e| e.to_string())?;
    warn!("Device binding: Generated new fallback instance ID for sandboxed environment.");
    Ok(new_id)
}
