use crate::models::{FrontendError, MnemonicLanguage, VoucherStandardInfo};
use crate::services::fs_helpers::{is_safe_filename_component, read_regular_file_bounded};
use crate::state::AppState;
use fs_extra::dir::{copy as copy_dir, CopyOptions};
use human_money_core::app_service::AppService;
use log::{debug, error, info, warn};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use tauri::Manager;

const MAX_STANDARD_TOML_BYTES: usize = 1024 * 1024;
const MAX_FRONTEND_LOG_MESSAGE_CHARS: usize = 2048;
const MAX_LATEST_LOGS_BYTES: usize = 512 * 1024;

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
    let standards = load_voucher_standards_from_dir(&standards_dir_in_data)?;

    debug!(
        "Final standards loaded: {:?}",
        standards.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    info!("Found {} voucher standards.", standards.len());
    Ok(standards)
}

pub(crate) fn load_voucher_standards_from_dir(
    standards_dir_in_data: &Path,
) -> Result<Vec<VoucherStandardInfo>, FrontendError> {
    let mut standards = Vec::new();
    let entries = fs::read_dir(standards_dir_in_data).map_err(|e| {
        FrontendError::from(format!(
            "Failed to read standards directory '{}': {}",
            standards_dir_in_data.display(),
            e
        ))
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        debug!("Found entry in standards directory: {}", path.display());
        if !fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_dir()) {
            continue;
        }
        let Some(standard_id) = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
        else {
            warn!("Skipping non-UTF-8 standard directory: {}", path.display());
            continue;
        };
        if !is_safe_filename_component(&standard_id) {
            warn!("Skipping unsafe standard directory ID: '{}'", standard_id);
            continue;
        }
        let toml_path = path.join("standard.toml");
        if matches!(
            fs::symlink_metadata(&toml_path),
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound
        ) {
            continue;
        }
        let bytes = match read_regular_file_bounded(&toml_path, MAX_STANDARD_TOML_BYTES) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "Skipping unreadable standard '{}': {}",
                    standard_id, e.message
                );
                continue;
            }
        };
        let content = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => {
                warn!("Skipping non-UTF-8 standard '{}': {}", standard_id, e);
                continue;
            }
        };
        let display_name = match human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(&content) {
            Ok((def, _)) => def.immutable.identity.name.clone(),
            Err(e) => {
                warn!("Skipping unparseable standard TOML for '{}': {}", standard_id, e);
                continue;
            }
        };

        standards.push(VoucherStandardInfo {
            id: standard_id,
            display_name,
            content,
        });
    }

    standards.sort_by(|a, b| a.id.cmp(&b.id));
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

pub(crate) fn sanitize_frontend_log_message(message: &str) -> String {
    message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_FRONTEND_LOG_MESSAGE_CHARS)
        .collect()
}

#[tauri::command]
pub async fn log_to_backend(level: String, message: String) -> Result<(), FrontendError> {
    let sanitized = sanitize_frontend_log_message(&message);
    match level.as_str() {
        "debug" => debug!("[Frontend]: {}", sanitized),
        "info" => info!("[Frontend]: {}", sanitized),
        "warn" => warn!("[Frontend]: {}", sanitized),
        "error" => error!("[Frontend]: {}", sanitized),
        _ => info!("[Frontend]: {}", sanitized),
    }
    Ok(())
}

#[tauri::command]
pub fn get_local_instance_id(app: tauri::AppHandle) -> Result<String, FrontendError> {
    crate::services::fs_helpers::resolve_or_create_local_instance_id(&app)
        .map_err(FrontendError::from)
}

pub(crate) fn read_log_tail_bounded(
    log_file_path: &Path,
    max_bytes: usize,
) -> Result<String, FrontendError> {
    let meta = match fs::symlink_metadata(log_file_path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => {
            return Err(FrontendError::from(format!(
                "Failed to inspect log file: {}",
                e
            )))
        }
    };
    if !meta.file_type().is_file() {
        return Err(FrontendError::from(
            "Log file path is not a regular file".to_string(),
        ));
    }

    let mut file = fs::File::open(log_file_path)
        .map_err(|e| FrontendError::from(format!("Failed to read log file: {}", e)))?;
    let file_len = meta.len();
    let max_u64 = max_bytes as u64;
    let truncated = max_u64 > 0 && file_len > max_u64;
    if truncated {
        file.seek(SeekFrom::Start(file_len - max_u64))
            .map_err(|e| FrontendError::from(format!("Failed to seek log file: {}", e)))?;
    }

    let mut buf = Vec::new();
    file.take(max_u64.max(1))
        .read_to_end(&mut buf)
        .map_err(|e| FrontendError::from(format!("Failed to read log file: {}", e)))?;
    if max_bytes == 0 {
        return Ok(String::new());
    }

    let mut start = 0usize;
    if truncated {
        // Advance past any partial leading UTF-8 continuation bytes.
        while start < buf.len() && (buf[start] & 0b1100_0000) == 0b1000_0000 {
            start += 1;
        }
    }
    let text = String::from_utf8_lossy(&buf[start..]);
    if truncated {
        if let Some(nl_pos) = text.find('\n') {
            return Ok(text[nl_pos + 1..].to_string());
        }
    }
    Ok(text.into_owned())
}

#[tauri::command]
pub fn get_latest_logs(app: tauri::AppHandle) -> Result<String, FrontendError> {
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| FrontendError::from(e.to_string()))?;
    let log_file_path = log_dir.join("human_money_app.log");
    read_log_tail_bounded(&log_file_path, MAX_LATEST_LOGS_BYTES)
}

pub(crate) fn peek_startup_launch_items_from_state(
    state: &AppState,
) -> Result<Vec<String>, FrontendError> {
    let guard = state
        .startup_launch_items
        .lock()
        .map_err(|_| FrontendError::from("Startup launch items unavailable".to_string()))?;
    Ok(guard.clone())
}

pub(crate) fn ack_startup_launch_item_from_state(
    state: &AppState,
    item: &str,
) -> Result<bool, FrontendError> {
    let mut guard = state
        .startup_launch_items
        .lock()
        .map_err(|_| FrontendError::from("Startup launch items unavailable".to_string()))?;
    if let Some(pos) = guard.iter().position(|existing| existing == item) {
        guard.remove(pos);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub fn peek_startup_launch_items(
    state: tauri::State<AppState>,
) -> Result<Vec<String>, FrontendError> {
    peek_startup_launch_items_from_state(&state)
}

#[tauri::command]
pub fn ack_startup_launch_item(
    item: String,
    state: tauri::State<AppState>,
) -> Result<bool, FrontendError> {
    ack_startup_launch_item_from_state(&state, &item)
}

#[cfg(test)]
mod launch_queue_tests {
    use super::{ack_startup_launch_item_from_state, peek_startup_launch_items_from_state};
    use crate::test_support::{create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE};

    #[test]
    fn h9_peek_is_non_destructive_and_ack_removes_first_matching_fifo_item() {
        let dir = temp_test_dir("h9_launch_queue");
        let service = setup_test_app(&dir, "Alice", ALICE_PHRASE, "pw123");
        let state = create_test_state(service);
        {
            let mut guard = state.startup_launch_items.lock().unwrap();
            *guard = vec![
                "humoco://pay?to=did:key:zAlice".to_string(),
                "/tmp/payment.transfer".to_string(),
            ];
        }

        // Peeking twice never clears the pending launch queue
        let first_peek = peek_startup_launch_items_from_state(&state).unwrap();
        let second_peek = peek_startup_launch_items_from_state(&state).unwrap();
        assert_eq!(first_peek.len(), 2);
        assert_eq!(first_peek, second_peek);

        // Acking removes only the acknowledged item in FIFO order
        assert!(ack_startup_launch_item_from_state(&state, &first_peek[0]).unwrap());
        assert!(!ack_startup_launch_item_from_state(&state, &first_peek[0]).unwrap());
        assert_eq!(
            peek_startup_launch_items_from_state(&state).unwrap(),
            vec!["/tmp/payment.transfer".to_string()]
        );

        assert!(ack_startup_launch_item_from_state(&state, "/tmp/payment.transfer").unwrap());
        assert!(peek_startup_launch_items_from_state(&state)
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn s11_f7_standards_isolation_log_sanitization_and_bounded_tail() {
        let dir = temp_test_dir("s11_f7_utils");
        let valid_toml = include_str!("../../../voucher_standards/minuto_v1/standard.toml");

        std::fs::create_dir_all(dir.join("zeta_std")).unwrap();
        std::fs::write(dir.join("zeta_std/standard.toml"), valid_toml).unwrap();

        std::fs::create_dir_all(dir.join("alpha_std")).unwrap();
        std::fs::write(dir.join("alpha_std/standard.toml"), valid_toml).unwrap();

        std::fs::create_dir_all(dir.join("broken_std")).unwrap();
        std::fs::write(dir.join("broken_std/standard.toml"), b"\xff\xfe\xfd").unwrap();

        std::fs::create_dir_all(dir.join("corrupt_toml")).unwrap();
        std::fs::write(dir.join("corrupt_toml/standard.toml"), "not = [valid").unwrap();

        std::fs::create_dir_all(dir.join("oversized_std")).unwrap();
        std::fs::write(
            dir.join("oversized_std/standard.toml"),
            vec![b'a'; super::MAX_STANDARD_TOML_BYTES + 64],
        )
        .unwrap();

        let loaded = super::load_voucher_standards_from_dir(&dir).unwrap();
        let ids: Vec<&str> = loaded.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["alpha_std", "zeta_std"]);

        // Log message sanitization strips control/newlines and caps to 2048 chars
        let dirty = format!("ok\r\n[FAKE]: forged\x07 {}", "ä".repeat(2500));
        let clean = super::sanitize_frontend_log_message(&dirty);
        assert!(!clean.contains('\r'));
        assert!(!clean.contains('\n'));
        assert!(!clean.contains('\x07'));
        assert_eq!(clean.chars().count(), super::MAX_FRONTEND_LOG_MESSAGE_CHARS);

        // Log tail reading caps bytes cleanly on multi-byte lines
        let log_path = dir.join("app.log");
        let line = format!("LINE {}\n", "ä".repeat(400));
        let mut big = String::new();
        while big.len() < 1024 * 1024 {
            big.push_str(&line);
        }
        std::fs::write(&log_path, &big).unwrap();
        let tail = super::read_log_tail_bounded(&log_path, super::MAX_LATEST_LOGS_BYTES).unwrap();
        assert!(tail.len() <= super::MAX_LATEST_LOGS_BYTES);
        assert!(tail.ends_with(&line));
        assert!(tail.starts_with("LINE "));

        #[cfg(unix)]
        {
            let link_path = dir.join("link.log");
            std::os::unix::fs::symlink(&log_path, &link_path).unwrap();
            assert!(
                super::read_log_tail_bounded(&link_path, super::MAX_LATEST_LOGS_BYTES).is_err()
            );
        }

        let _ = std::fs::remove_dir_all(dir);
    }
}
