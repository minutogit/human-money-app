// src-tauri/src/lib.rs
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod commands;
mod gateway_resolver;
pub(crate) mod intent_reconciliation;
pub mod l2_lifecycle;
pub mod models;
pub mod pin_keystore;
pub mod services;
pub mod session;
pub mod settings;
pub mod state;
pub mod sync_client;
pub mod sync_zip;
#[cfg(test)]
pub(crate) mod test_support;

pub use state::AppState;

use chrono::Local;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use human_money_core::app_service::AppService;
use log::info;
use rand::RngCore;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_fs::FsExt;
use tauri_plugin_log::{
    log::LevelFilter, Builder as LogBuilder, Target, TargetKind, TimezoneStrategy,
};

use crate::commands::{
    auth::*, backup::*, contacts::*, integrity::*, l2::*, market::*, post::*, profile::*,
    queries::*, recovery::*, settings_cmd::*, standards::*, sync::*, transfers::*, utils::*,
    vouchers::*, wot::*,
};

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
    let lines: Vec<String> = BufReader::new(File::open(&log_file_path)?)
        .lines()
        .map_while(Result::ok)
        .collect();

    if !lines.is_empty() {
        let total_lines = lines.len();
        let lines_to_skip = (total_lines as f64 * 0.20).ceil() as usize;
        let lines_to_keep = lines
            .into_iter()
            .skip(lines_to_skip)
            .collect::<Vec<String>>();
        let mut log_file = File::create(&log_file_path)?;
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        writeln!(
            log_file,
            "--- Log rotated at {} (top 20% removed) ---",
            timestamp
        )?;
        log_file.write_all(lines_to_keep.join("\n").as_bytes())?;
    }

    Ok(())
}

const INSTANCE_IPC_PREFIX: &str = "HUMOCO_IPC_V1 ";
const INSTANCE_IPC_ACK: &[u8] = b"HUMOCO_IPC_OK\n";
const MAX_INSTANCE_IPC_BYTES: usize = 65_536;
const MAX_STARTUP_LAUNCH_ITEMS: usize = 64;
const SUPPORTED_LAUNCH_FILE_EXTENSIONS: &[&str] = &[
    "transfer",
    "hmcbundle",
    "hmc",
    "hmcpost",
    "ask",
    "humocoreq",
    "sig",
    "humocosig",
    "wot",
    "hmcwot",
    "standard",
    "toml",
    "hmcs",
    "hmcproof",
    "hmcard",
    "humoco",
    "hmcbackup",
];

fn is_supported_launch_file(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            SUPPORTED_LAUNCH_FILE_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        })
}

fn allow_launch_file_in_fs_scope(app: &AppHandle, item: &str) {
    if item.to_ascii_lowercase().starts_with("humoco://") {
        return;
    }
    let path = std::path::Path::new(item);
    if is_supported_launch_file(path) {
        if let Some(scope) = app.try_fs_scope() {
            let _ = scope.allow_file(path);
        }
    }
}

pub(crate) fn has_allow_multiple_flag<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .any(|arg| arg.as_ref().trim() == "--allow-multiple")
}

pub(crate) fn extract_launch_items_from_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter_map(|raw| {
            let trimmed = raw.as_ref().trim();
            if trimmed.is_empty() || trimmed.starts_with("--") {
                return None;
            }
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("humoco://") {
                return Some(trimmed.to_string());
            }
            if lower.starts_with("file://") {
                if let Ok(parsed) = url::Url::parse(trimmed) {
                    if let Ok(path) = parsed.to_file_path() {
                        if is_supported_launch_file(&path) {
                            return Some(path.to_string_lossy().to_string());
                        }
                    }
                }
                return None;
            }
            if is_supported_launch_file(std::path::Path::new(trimmed)) {
                return Some(trimmed.to_string());
            }
            None
        })
        .collect()
}

fn push_and_emit_launch_items(app: &AppHandle, raw_items: Vec<String>) {
    let mut items = extract_launch_items_from_args(raw_items);
    items.truncate(MAX_STARTUP_LAUNCH_ITEMS);
    if !items.is_empty() {
        for item in &items {
            allow_launch_file_in_fs_scope(app, item);
        }
        let state = app.state::<AppState>();
        if let Ok(mut guard) = state.startup_launch_items.lock() {
            for item in &items {
                if guard.len() >= MAX_STARTUP_LAUNCH_ITEMS {
                    break;
                }
                if !guard.contains(item) {
                    guard.push(item.clone());
                }
            }
        }
        let _ = app.emit("humoco://launch-items", &items);
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub(crate) fn write_ipc_addr_file(
    path: &std::path::Path,
    addr: SocketAddr,
    token: &str,
) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp_path = parent.join(format!("instance_ipc.addr.tmp.{}", uuid::Uuid::new_v4()));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let write_res = (|| -> std::io::Result<()> {
        let mut file = opts.open(&tmp_path)?;
        let content = format!("{} {}\n", addr, token);
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(err) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

pub(crate) fn parse_ipc_addr_file(content: &str) -> Option<(SocketAddr, String)> {
    let mut parts = content.split_whitespace();
    let addr_str = parts.next()?;
    let token = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let addr = addr_str.parse::<SocketAddr>().ok()?;
    if !addr.ip().is_loopback() {
        return None;
    }
    if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((addr, token.to_string()))
}

pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub(crate) fn parse_authenticated_ipc_frame(
    buf: &[u8],
    expected_token: &str,
) -> Option<Vec<String>> {
    let newline_pos = buf.iter().position(|&b| b == b'\n')?;
    let expected_header = format!("{}{}", INSTANCE_IPC_PREFIX, expected_token);
    if !constant_time_eq(&buf[..newline_pos], expected_header.as_bytes()) {
        return None;
    }
    let json_slice = &buf[newline_pos + 1..];
    let items = serde_json::from_slice::<Vec<String>>(json_slice).ok()?;
    if items.len() > MAX_STARTUP_LAUNCH_ITEMS {
        return None;
    }
    Some(items)
}

pub(crate) fn read_bounded_ipc_frame<R: Read>(
    stream: &mut R,
    max_bytes: usize,
    deadline: std::time::Instant,
) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if std::time::Instant::now() > deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "IPC frame read deadline exceeded",
            ));
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Ok(buf),
            Ok(n) => {
                if buf.len().saturating_add(n) > max_bytes {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "IPC frame exceeds maximum allowed size",
                    ));
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

fn try_forward_to_running_instance(ipc_addr_path: &std::path::Path, items: &[String]) -> bool {
    let Ok(addr_str) = fs::read_to_string(ipc_addr_path) else {
        return false;
    };
    let Some((addr, token)) = parse_ipc_addr_file(&addr_str) else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(150)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));

    let capped_items = &items[..items.len().min(MAX_STARTUP_LAUNCH_ITEMS)];
    let Ok(payload) = serde_json::to_vec(capped_items) else {
        return false;
    };
    let header = format!("{}{}\n", INSTANCE_IPC_PREFIX, token);
    if stream.write_all(header.as_bytes()).is_err()
        || stream.write_all(&payload).is_err()
        || stream.shutdown(Shutdown::Write).is_err()
    {
        return false;
    }

    let mut ack = [0u8; 32];
    let Ok(n) = stream.read(&mut ack) else {
        return false;
    };
    n == INSTANCE_IPC_ACK.len() && &ack[..n] == INSTANCE_IPC_ACK
}

fn start_instance_ipc_listener(app_handle: AppHandle, ipc_addr_path: std::path::PathBuf) {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let Ok(local_addr) = listener.local_addr() else {
        return;
    };
    let mut token_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut token_bytes);
    let token = hex::encode(token_bytes);
    if write_ipc_addr_file(&ipc_addr_path, local_addr, &token).is_err() {
        return;
    }

    std::thread::spawn(move || {
        for stream_res in listener.incoming() {
            let Ok(mut stream) = stream_res else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
            let deadline = std::time::Instant::now() + Duration::from_millis(500);
            let Ok(buf) = read_bounded_ipc_frame(&mut stream, MAX_INSTANCE_IPC_BYTES, deadline)
            else {
                continue;
            };
            let Some(items) = parse_authenticated_ipc_frame(&buf, &token) else {
                continue;
            };
            if stream.write_all(INSTANCE_IPC_ACK).is_err() || stream.flush().is_err() {
                continue;
            }
            push_and_emit_launch_items(&app_handle, items);
        }
    });
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn format_linux_desktop_entry(exe_str: &str) -> Option<String> {
    if exe_str.is_empty() || exe_str.chars().any(|c| c.is_control()) {
        return None;
    }
    let mut escaped = String::with_capacity(exe_str.len());
    for ch in exe_str.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '`' => escaped.push_str("\\`"),
            '$' => escaped.push_str("\\$"),
            '%' => escaped.push_str("%%"),
            _ => escaped.push(ch),
        }
    }
    Some(format!(
        "[Desktop Entry]\nType=Application\nName=Human Money App\nExec=\"{}\" %u\nTerminal=false\nNoDisplay=true\nMimeType=x-scheme-handler/humoco;\n",
        escaped
    ))
}

fn register_os_url_scheme_best_effort() {
    #[cfg(target_os = "linux")]
    {
        std::thread::spawn(|| {
            let Ok(exe) = std::env::current_exe() else {
                return;
            };
            let Some(exe_str) = exe.to_str() else {
                return;
            };
            let Some(content) = format_linux_desktop_entry(exe_str) else {
                return;
            };
            let data_dir = std::env::var_os("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(|h| std::path::PathBuf::from(h).join(".local/share"))
                });
            let Some(applications_dir) = data_dir.map(|d| d.join("applications")) else {
                return;
            };
            if fs::create_dir_all(&applications_dir).is_err() {
                return;
            }
            let desktop_path = applications_dir.join("humoco-url-handler.desktop");
            let needs_write = fs::read_to_string(&desktop_path).ok().as_deref() != Some(&content);
            if needs_write && fs::write(&desktop_path, &content).is_err() {
                return;
            }
            let _ = std::process::Command::new("xdg-mime")
                .args([
                    "default",
                    "humoco-url-handler.desktop",
                    "x-scheme-handler/humoco",
                ])
                .status();
        });
    }
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(|| {
            let Ok(exe) = std::env::current_exe() else {
                return;
            };
            let Some(exe_str) = exe.to_str() else {
                return;
            };
            if exe_str.is_empty() || exe_str.chars().any(|c| c.is_control() || c == '"') {
                return;
            }
            let cmd_val = format!("\"{}\" \"%1\"", exe_str);
            let _ = std::process::Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Classes\humoco",
                    "/ve",
                    "/d",
                    "URL:HuMoCo Protocol",
                    "/f",
                ])
                .status();
            let _ = std::process::Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Classes\humoco",
                    "/v",
                    "URL Protocol",
                    "/d",
                    "",
                    "/f",
                ])
                .status();
            let _ = std::process::Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Classes\humoco\shell\open\command",
                    "/ve",
                    "/d",
                    &cmd_val,
                    "/f",
                ])
                .status();
        });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let raw_args: Vec<String> = std::env::args().skip(1).collect();
            let allow_multiple = has_allow_multiple_flag(&raw_args);
            let mut startup_items = extract_launch_items_from_args(&raw_args);
            startup_items.truncate(MAX_STARTUP_LAUNCH_ITEMS);

            // Get the application's data directory
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to find app data directory");
            let ipc_addr_path = data_dir.join("instance_ipc.addr");

            if !allow_multiple {
                if try_forward_to_running_instance(&ipc_addr_path, &startup_items) {
                    std::process::exit(0);
                }

                // Setup log rotation. Panicking here is acceptable because logging is a critical feature.
                if let Err(e) = setup_log_rotation(app.handle()) {
                    // We can't use the logger here yet, so we'll panic with a detailed error.
                    panic!("Failed to setup log rotation: {}", e);
                }
            } else {
                info!(
                    "Multi-instance mode enabled (--allow-multiple); skipping single-instance IPC forwarding and log rotation."
                );
            }

            let wallet_path = data_dir.join("wallet_data");
            if !wallet_path.exists() {
                fs::create_dir_all(&wallet_path).expect("Failed to create wallet data directory");
            }
            info!("Using wallet data path: {}", wallet_path.display());

            let service = AppService::new(&wallet_path).expect("Failed to create AppService");
            let app_state =
                AppState::new(service).with_session_lock_dir(data_dir.join("session_locks"));
            for item in &startup_items {
                allow_launch_file_in_fs_scope(app.handle(), item);
            }
            if let Ok(mut guard) = app_state.startup_launch_items.lock() {
                *guard = startup_items;
            }
            app.manage(app_state);
            if !allow_multiple {
                start_instance_ipc_listener(app.handle().clone(), ipc_addr_path);
                register_os_url_scheme_best_effort();
            }
            crate::intent_reconciliation::spawn_intent_reconciliation_worker(app.handle().clone());
            crate::services::sync_coordinator::spawn_background_sync_coordinator(app.handle().clone());

            Ok(())
        })
        .plugin(
            LogBuilder::default()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::Webview),
                    Target::new(TargetKind::LogDir {
                        file_name: Some("human_money_app".into()),
                    }),
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
            list_profiles,
            create_profile,
            login,
            login_with_pin,
            get_profile_pin_status,
            get_session_status,
            unlock_session_with_pin,
            enable_profile_pin,
            disable_profile_pin,
            elevate_session_with_password,
            recover_wallet_and_set_new_password,
            logout,
            delete_profile,
            verify_profile_password,
            handover_to_this_device,
            unlock_session,
            lock_session,
            refresh_session_activity,
            generate_mnemonic,
            get_bip39_wordlist,
            get_voucher_standards,
            validate_mnemonic,
            get_local_instance_id,
            get_latest_logs,
            peek_startup_launch_items,
            ack_startup_launch_item,
            get_user_id,
            get_user_profile,
            get_total_balance_by_currency,
            get_voucher_summaries,
            get_voucher_details,
            get_active_asset_classes,
            create_new_voucher,
            create_transfer_bundle,
            receive_bundle,
            get_transaction_history,
            prepare_transfer_plan,
            get_app_config,
            patch_ui_preferences,
            patch_security_policy,
            get_reach_preset,
            apply_reach_preset,
            log_to_backend,
            // Multi-signature commands
            create_signing_request_bundle,
            open_voucher_signing_request,
            create_detached_signature_response_bundle,
            process_and_attach_signature,
            evaluate_signature_suitability,
            evaluate_guarantor_anchor_check,
            remove_voucher_signature,
            get_double_spend_conflicts,
            get_proof_of_double_spend,
            get_proof_id_for_voucher,
            check_reputation,
            set_conflict_local_override,
            get_event_history,
            parse_standard_toml,
            export_voucher_standard,
            inspect_voucher_standard_file,
            import_voucher_standard,
            delete_voucher_standard,
            // Address Book
            get_contacts,
            save_contact,
            delete_contact,
            get_my_quick_contact,
            encode_text_qr_svg,
            // WalletSeal / Sync Locks
            check_voucher_l2_status,
            get_lock_protection_overview,
            set_lock_server_trust,
            refresh_lock_servers,
            prewarm_l2_checkout,
            discover_anchor_manifest_cmd,
            create_guarantor_claim_bundle,
            // Integrity
            check_wallet_integrity,
            repair_wallet_integrity,
            export_double_spend_proof,
            import_double_spend_proof,
            endorse_conflict_resolution,
            // Wallet Backups
            probe_wallet_backup,
            export_wallet_backup_file,
            sync_wallet_backup_folder,
            inspect_wallet_backup,
            restore_wallet_backup,
            get_backup_status,
            sync_backup,
            probe_sync_backup,
            preview_mnemonic_identity,
            stage_dropped_backup_bytes,
            reset_local_password_via_backup,
            reconcile_restored_wallet_l2,
            dismiss_wallet_sync_alert,
            // Web of Trust (WoT)
            get_wot_summary,
            set_wot_max_hops,
            export_wot_package,
            import_wot_package,
            evaluate_voucher_trust_preview,
            get_wot_providers,
            set_wot_provider_trust,
            remove_wot_provider,
            refresh_wot_provider_url,
            // Marketplace
            get_my_market_profile,
            save_my_market_profile,
            set_card_published,
            upsert_ad,
            renew_ad,
            delete_ad,
            get_ad_image,
            get_market_view,
            export_forward_card,
            resolve_card_recipient,
            import_contact_card,
            get_app_notices,
            mark_app_notices_read,
            clear_app_notices,
            // Sync Services
            prepare_sync_setup,
            get_rust_sync_config,
            save_sync_setup_zip,
            finalize_sync_setup,
            test_sync_connection,
            get_sync_admin_status,
            update_sync_admin_config,
            delete_sync_admin,
            revoke_own_sync_server,
            discover_available_syncs,
            probe_contact_syncs,
            set_selected_upload_syncs,
            sync_market_channels,
            update_network_status,
            wake_sync_coordinator,
            preview_contact_trust_change,
            probe_public_sync,
            save_public_sync,
            remove_public_sync,
            get_sync_overview_status,
            deposit_transfer_to_post,
            send_signing_request_via_post,
            reply_detached_signature_via_post,
            accept_wot_handshake,
            send_market_inquiry_via_post,
            send_market_inquiry_message_via_post,
            mark_inquiry_read,
            dismiss_pending_post_item,
            confirm_pending_post_payment,
            get_post_inbox_summary,
            sync_post_channels,
            export_profile_mnemonic,
            confirm_emergency_sheet_saved,
            get_recovery_protection_status,
            setup_social_recovery_guardians,
            revoke_social_recovery,
            start_social_recovery_session,
            get_active_recovery_session,
            cancel_social_recovery_session,
            import_guardian_recovery_reply,
            inspect_guardian_recovery_request,
            approve_guardian_recovery_request,
            complete_social_recovery,
            inspect_emergency_kit_uri,
            verify_recovery_rehearsal,
            snooze_recovery_rehearsal,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, _event| {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
        if let tauri::RunEvent::Opened { urls } = _event {
            let items: Vec<String> = urls.into_iter().map(|u| u.to_string()).collect();
            push_and_emit_launch_items(_app_handle, items);
        }
    });
}

#[cfg(test)]
mod launch_tests {
    use super::{
        extract_launch_items_from_args, format_linux_desktop_entry, has_allow_multiple_flag,
        parse_authenticated_ipc_frame, parse_ipc_addr_file, read_bounded_ipc_frame,
        write_ipc_addr_file, MAX_STARTUP_LAUNCH_ITEMS,
    };

    #[test]
    fn test_has_allow_multiple_flag() {
        assert!(has_allow_multiple_flag(["--allow-multiple"]));
        assert!(has_allow_multiple_flag([
            "  --allow-multiple ",
            "humoco://pay?to=did:key:abc"
        ]));
        assert!(!has_allow_multiple_flag(["--allow-multiple=true"]));
        assert!(!has_allow_multiple_flag(["--other", "sample.transfer"]));
        assert!(!has_allow_multiple_flag::<[&str; 0], &str>([]));
    }

    #[test]
    fn test_extract_launch_items_accepts_humoco_links_and_file_urls() {
        let dir = std::env::temp_dir().join(format!("launch-args-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("sample.transfer");
        std::fs::write(&file_path, b"dummy").unwrap();
        let file_url = url::Url::from_file_path(&file_path).unwrap().to_string();

        let txt_path = dir.join("secret.txt");
        std::fs::write(&txt_path, b"secret").unwrap();
        let json_path = dir.join("keys.json");
        std::fs::write(&json_path, b"{}").unwrap();
        let extless_path = dir.join("passwd");
        std::fs::write(&extless_path, b"root").unwrap();
        let txt_url = url::Url::from_file_path(&txt_path).unwrap().to_string();

        let extracted = extract_launch_items_from_args([
            "  humoco://pay?to=did:key:abc&amount=5 ",
            "HUMOCO://RECOVERY-REPLY?v=1&data=xyz",
            &file_url,
            file_path.to_str().unwrap(),
            txt_path.to_str().unwrap(),
            json_path.to_str().unwrap(),
            extless_path.to_str().unwrap(),
            &txt_url,
            "--allow-multiple",
            "--some-flag",
            "/nonexistent/path/to/file.transfer",
        ]);

        assert_eq!(extracted.len(), 4);
        assert_eq!(extracted[0], "humoco://pay?to=did:key:abc&amount=5");
        assert_eq!(extracted[1], "HUMOCO://RECOVERY-REPLY?v=1&data=xyz");
        assert_eq!(extracted[2], file_path.to_string_lossy());
        assert_eq!(extracted[3], file_path.to_string_lossy());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_ipc_addr_file_parse_and_atomic_write() {
        let valid_token = "0123456789abcdef0123456789abcdef";
        // Non-loopback IP rejected
        assert!(parse_ipc_addr_file(&format!("192.168.1.10:12345 {}", valid_token)).is_none());
        // Missing token rejected
        assert!(parse_ipc_addr_file("127.0.0.1:12345").is_none());
        // Malformed token (wrong length / non-hex) rejected
        assert!(parse_ipc_addr_file("127.0.0.1:12345 deadbeef").is_none());
        assert!(parse_ipc_addr_file("127.0.0.1:12345 zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
        // Extra trailing field rejected
        assert!(parse_ipc_addr_file(&format!("127.0.0.1:12345 {} extra", valid_token)).is_none());

        let dir = std::env::temp_dir().join(format!("ipc-addr-{}", uuid::Uuid::new_v4()));
        let path = dir.join("instance_ipc.addr");
        let addr: std::net::SocketAddr = "127.0.0.1:45678".parse().unwrap();
        write_ipc_addr_file(&path, addr, valid_token).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let raw = std::fs::read_to_string(&path).unwrap();
        let (parsed_addr, parsed_token) = parse_ipc_addr_file(&raw).unwrap();
        assert_eq!(parsed_addr, addr);
        assert_eq!(parsed_token, valid_token);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_parse_authenticated_ipc_frame_and_bounded_reader() {
        let token = "abcdef0123456789abcdef0123456789";
        let wrong_token = "11111111111111111111111111111111";
        let payload = serde_json::to_vec(&vec!["humoco://pay?to=did:key:zAlice"]).unwrap();

        let mut valid_frame = format!("HUMOCO_IPC_V1 {}\n", token).into_bytes();
        valid_frame.extend_from_slice(&payload);
        assert_eq!(
            parse_authenticated_ipc_frame(&valid_frame, token).unwrap(),
            vec!["humoco://pay?to=did:key:zAlice".to_string()]
        );

        // Wrong token rejected
        assert!(parse_authenticated_ipc_frame(&valid_frame, wrong_token).is_none());
        // Missing token (legacy header) rejected
        let mut legacy_frame = b"HUMOCO_IPC_V1\n".to_vec();
        legacy_frame.extend_from_slice(&payload);
        assert!(parse_authenticated_ipc_frame(&legacy_frame, token).is_none());

        // > MAX_STARTUP_LAUNCH_ITEMS rejected
        let too_many: Vec<String> = (0..=MAX_STARTUP_LAUNCH_ITEMS)
            .map(|i| format!("humoco://pay?i={i}"))
            .collect();
        let mut big_frame = format!("HUMOCO_IPC_V1 {}\n", token).into_bytes();
        big_frame.extend_from_slice(&serde_json::to_vec(&too_many).unwrap());
        assert!(parse_authenticated_ipc_frame(&big_frame, token).is_none());

        // Bounded reader rejects oversized input and expired deadline
        let mut cursor = std::io::Cursor::new(vec![b'a'; 128]);
        let err = read_bounded_ipc_frame(
            &mut cursor,
            64,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

        let mut cursor2 = std::io::Cursor::new(b"abc".to_vec());
        let timeout_err = read_bounded_ipc_frame(
            &mut cursor2,
            64,
            std::time::Instant::now() - std::time::Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(timeout_err.kind(), std::io::ErrorKind::TimedOut);
    }

    #[test]
    fn test_format_linux_desktop_entry_rejects_control_and_escapes() {
        assert!(format_linux_desktop_entry("/tmp/app\nExec=sh").is_none());
        assert!(format_linux_desktop_entry("/tmp/app\r").is_none());
        assert!(format_linux_desktop_entry("").is_none());

        let entry = format_linux_desktop_entry(r#"/opt/hu"mo\co/$bin/`x`/%f"#).unwrap();
        assert!(entry.contains(r#"Exec="/opt/hu\"mo\\co/\$bin/\`x\`/%%f" %u"#));
    }
}

mod checkout;
