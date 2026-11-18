use crate::models::{FrontendError, MnemonicLanguage, VoucherStandardInfo};
use crate::state::AppState;
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use human_money_core::app_service::AppService;
use log::{debug, error, info, warn};
use std::fs;
use tauri::Manager;

#[tauri::command]
pub fn generate_mnemonic(
    word_count: u32,
    language: MnemonicLanguage,
) -> Result<String, FrontendError> {
    info!(
        "Generating a new {}-word mnemonic in language {:?}",
        word_count, language
    );
    let core_language = language.into();
    AppService::generate_mnemonic(word_count, core_language).map_err(FrontendError::from)
}

#[tauri::command]
pub fn validate_mnemonic(
    mnemonic: String,
    language: MnemonicLanguage,
) -> Result<(), FrontendError> {
    info!("Validating mnemonic with language {:?}...", language);
    let core_language = language.into();
    AppService::validate_mnemonic(&mnemonic, core_language).map_err(FrontendError::from)
}

#[tauri::command]
pub fn get_voucher_standards(
    app: tauri::AppHandle,
) -> Result<Vec<VoucherStandardInfo>, FrontendError> {
    info!("Ensuring voucher standards are available in app data directory...");

    // 1. Define paths.
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let standards_dir_in_data = app_data_dir.join("voucher_standards");
    info!(
        "App data standards directory is set to: {}",
        standards_dir_in_data.display()
    );

    // 2. "First Run" Logic: If the standards directory doesn't exist, copy it from the app bundle.
    let needs_copy = if !standards_dir_in_data.exists() {
        info!("Standards directory does not exist. Will copy.");
        true
    } else {
        // Directory exists, check if it is empty.
        match fs::read_dir(&standards_dir_in_data) {
            Ok(mut dir) => match dir.next().is_none() {
                true => {
                    info!("Standards directory exists but is empty. Will copy.");
                    true
                }
                false => {
                    info!("Standards directory exists and is not empty. No copy needed.");
                    false
                }
            },
            Err(e) => {
                error!(
                    "Could not read existing standards directory, will attempt to overwrite: {}",
                    e
                );
                true // If we can't even read it, better to try a fresh copy.
            }
        }
    };
    if needs_copy {
        fs::create_dir_all(&standards_dir_in_data).map_err(|e| {
            FrontendError::from(format!(
                "Failed to create standards directory in app data: {}",
                e
            ))
        })?;

        #[cfg(debug_assertions)]
        {
            let standards_source_path =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../voucher_standards");
            info!(
                "Attempting to copy standards from DEV source: {}",
                standards_source_path.display()
            );
            let options = CopyOptions {
                overwrite: true,
                content_only: true,
                ..Default::default()
            };
            copy_dir(&standards_source_path, &standards_dir_in_data, &options).map_err(|e| {
                FrontendError::from(format!("Failed to copy standards from DEV source: {}", e))
            })?;
        }
        #[cfg(not(debug_assertions))]
        {
            let resource_dir = app.path().resource_dir().map_err(|e| {
                FrontendError::from(format!("Failed to get resource directory: {}", e))
            })?;
            // The tauri bundler can create a nested `_up_/voucher_standards` dir from `../` paths.
            // We need to specifically target this path as our source.
            let standards_source_path = resource_dir.join("_up_").join("voucher_standards");
            info!(
                "Attempting to copy standards from AppImage resource path: {}",
                standards_source_path.display()
            );
            let options = CopyOptions {
                overwrite: true,
                content_only: true,
                ..Default::default()
            };
            copy_dir(&standards_source_path, &standards_dir_in_data, &options).map_err(|e| {
                FrontendError::from(format!(
                    "Failed to copy standards from resources to app data: {}",
                    e
                ))
            })?;
        }
        info!("Default standards copied successfully.");
    }

    // 3. Read standards from the user's app data directory.
    info!(
        "Reading voucher standards from: {}",
        standards_dir_in_data.display()
    );
    let mut standards = Vec::new();
    match fs::read_dir(&standards_dir_in_data) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                debug!("Found entry in standards directory: {}", path.display());
                if path.is_dir() {
                    let standard_id = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    debug!(
                        "Found potential standard directory with ID: '{}'",
                        standard_id
                    );
                    let toml_path = path.join("standard.toml");
                    debug!("Checking for standard file at: {}", toml_path.display());
                    if toml_path.exists() {
                        let content = fs::read_to_string(&toml_path).map_err(|e| {
                            FrontendError::from(format!(
                                "Failed to read {}: {}",
                                toml_path.display(),
                                e
                            ))
                        })?;
                        debug!(
                            "Successfully read '{}', parsing for display name.",
                            toml_path.display()
                        );

                        let display_name = match human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(&content) {
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
            return Err(FrontendError::from(format!(
                "Failed to read standards directory '{}': {}",
                standards_dir_in_data.display(),
                e
            )));
        }
    };

    debug!(
        "Final standards loaded: {:?}",
        standards.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    info!("Found {} voucher standards.", standards.len());
    Ok(standards)
}

#[tauri::command]
pub fn get_bip39_wordlist(language: MnemonicLanguage) -> Vec<&'static str> {
    info!(
        "Fetching BIP-39 wordlist for language {:?} for frontend.",
        language
    );
    let core_language = language.into();
    AppService::get_mnemonic_wordlist(core_language)
}

#[tauri::command]
pub async fn log_to_backend(level: String, message: String) -> Result<(), FrontendError> {
    match level.as_str() {
        "info" => info!("[Frontend]: {}", message),
        "warn" => warn!("[Frontend]: {}", message),
        "error" => error!("[Frontend]: {}", message),
        _ => info!("[Frontend, {level}]: {}", message),
    }
    Ok(())
}

#[tauri::command]
pub fn get_local_instance_id(app: tauri::AppHandle) -> Result<String, FrontendError> {
    crate::services::fs_helpers::resolve_or_create_local_instance_id(&app)
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn get_latest_logs(app: tauri::AppHandle) -> Result<String, FrontendError> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let log_file_path = log_dir.join("human_money_app.log");
    if !log_file_path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&log_file_path)
        .map_err(|e| FrontendError::from(format!("Failed to read log file: {}", e)))
}

#[tauri::command]
pub fn take_startup_launch_items(
    state: tauri::State<AppState>,
) -> Result<Vec<String>, FrontendError> {
    let mut guard = state
        .startup_launch_items
        .lock()
        .map_err(|_| FrontendError::from("Startup launch items unavailable".to_string()))?;
    Ok(std::mem::take(&mut *guard))
}
