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
use tauri::{AppHandle, Emitter, Manager};
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

const INSTANCE_IPC_HEADER: &[u8] = b"HUMOCO_IPC_V1\n";
const INSTANCE_IPC_ACK: &[u8] = b"HUMOCO_IPC_OK\n";

pub(crate) fn extract_launch_items_from_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter_map(|raw| {
            let trimmed = raw.as_ref().trim();
            if trimmed.is_empty() {
                return None;
            }
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("humoco://") {
                return Some(trimmed.to_string());
            }
            if lower.starts_with("file://") {
                if let Ok(parsed) = url::Url::parse(trimmed) {
                    if let Ok(path) = parsed.to_file_path() {
                        if path.is_file() {
                            return Some(path.to_string_lossy().to_string());
                        }
                    }
                }
                return None;
            }
            if std::path::Path::new(trimmed).is_file() {
                return Some(trimmed.to_string());
            }
            None
        })
        .collect()
}

fn push_and_emit_launch_items(app: &AppHandle, raw_items: Vec<String>) {
    let items = extract_launch_items_from_args(raw_items);
    if !items.is_empty() {
        let state = app.state::<AppState>();
        if let Ok(mut guard) = state.startup_launch_items.lock() {
            for item in &items {
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

fn try_forward_to_running_instance(ipc_addr_path: &std::path::Path, items: &[String]) -> bool {
    let Ok(addr_str) = fs::read_to_string(ipc_addr_path) else {
        return false;
    };
    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(150)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));

    let Ok(payload) = serde_json::to_vec(items) else {
        return false;
    };
    if stream.write_all(INSTANCE_IPC_HEADER).is_err()
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
    let _ = fs::write(&ipc_addr_path, local_addr.to_string());

    std::thread::spawn(move || {
        for stream_res in listener.incoming() {
            let Ok(mut stream) = stream_res else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let mut buf = Vec::new();
            let mut reader = (&stream).take(65_536);
            if reader.read_to_end(&mut buf).is_err() {
                continue;
            }
            if !buf.starts_with(INSTANCE_IPC_HEADER) {
                continue;
            }
            let json_slice = &buf[INSTANCE_IPC_HEADER.len()..];
            let Ok(items) = serde_json::from_slice::<Vec<String>>(json_slice) else {
                continue;
            };
            let _ = stream.write_all(INSTANCE_IPC_ACK);
            let _ = stream.flush();
            push_and_emit_launch_items(&app_handle, items);
        }
    });
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
            let data_dir = std::env::var_os("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
                });
            let Some(applications_dir) = data_dir.map(|d| d.join("applications")) else {
                return;
            };
            if fs::create_dir_all(&applications_dir).is_err() {
                return;
            }
            let desktop_path = applications_dir.join("humoco-url-handler.desktop");
            let escaped_exe = exe_str.replace('"', "\\\"");
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=Human Money App\nExec=\"{}\" %u\nTerminal=false\nNoDisplay=true\nMimeType=x-scheme-handler/humoco;\n",
                escaped_exe
            );
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
            // Setup log rotation. Panicking here is acceptable because logging is a critical feature.
            if let Err(e) = setup_log_rotation(app.handle()) {
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

            let startup_items = extract_launch_items_from_args(std::env::args().skip(1));
            let ipc_addr_path = data_dir.join("instance_ipc.addr");
            if try_forward_to_running_instance(&ipc_addr_path, &startup_items) {
                std::process::exit(0);
            }

            let service = AppService::new(&wallet_path).expect("Failed to create AppService");
            let app_state = AppState::new(service);
            if let Ok(mut guard) = app_state.startup_launch_items.lock() {
                *guard = startup_items;
            }
            app.manage(app_state);
            start_instance_ipc_listener(app.handle().clone(), ipc_addr_path);
            register_os_url_scheme_best_effort();
            crate::intent_reconciliation::spawn_intent_reconciliation_worker(app.handle().clone());

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
            take_startup_launch_items,
            get_user_id,
            get_user_profile,
            get_total_balance_by_currency,
            get_voucher_summaries,
            get_voucher_details,
            get_active_asset_classes,
            create_new_voucher,
            create_transfer_bundle,
            receive_bundle,
            save_transaction_record,
            get_transaction_history,
            prepare_transfer_plan,
            checkout_received_bundle,
            get_app_settings,
            save_app_settings,
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
            get_seal_sync_status,
            check_voucher_l2_status,
            probe_lock_server_cmd,
            set_own_nominated_lock_server,
            assemble_anchor_template,
            prewarm_l2_checkout,
            check_l2_lock_status,
            get_l2_lifecycle_status,
            run_l2_lifecycle_tick,
            discover_anchor_manifest_cmd,
            register_guarantor_anchor_role,
            create_key_successor_attestation_cmd,
            import_key_successor_attestation_cmd,
            create_guarantor_claim_bundle,
            create_dead_quorum_swap_offer_cmd,
            create_dead_quorum_swap_claim_cmd,
            adjudicate_dead_quorum_swap_cmd,
            // Integrity
            check_wallet_integrity,
            repair_wallet_integrity,
            export_double_spend_proof,
            import_double_spend_proof,
            // Wallet Backups
            probe_wallet_backup,
            export_wallet_backup_file,
            sync_wallet_backup_folder,
            inspect_wallet_backup,
            restore_wallet_backup,
            get_backup_status,
            run_auto_backup_sync,
            sync_backup,
            probe_sync_backup,
            preview_mnemonic_identity,
            stage_dropped_backup_bytes,
            reset_local_password_via_backup,
            reconcile_restored_wallet_l2,
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
            import_contact_card,
            delete_foreign_contact_card,
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
            block_sync_slot,
            delete_sync_admin,
            revoke_own_sync_server,
            discover_available_syncs,
            probe_contact_syncs,
            set_selected_upload_syncs,
            sync_market_channels,
            sync_channel_members,
            delete_own_sync_slots,
            preview_contact_trust_change,
            probe_public_sync,
            save_public_sync,
            remove_public_sync,
            get_sync_overview_status,
            deposit_transfer_to_post,
            send_signing_request_via_post,
            reply_detached_signature_via_post,
            send_wot_handshake_via_post,
            accept_wot_handshake,
            send_market_inquiry_via_post,
            send_market_inquiry_message_via_post,
            mark_inquiry_read,
            dismiss_pending_post_item,
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
    use super::extract_launch_items_from_args;

    #[test]
    fn test_extract_launch_items_accepts_humoco_links_and_file_urls() {
        let dir = std::env::temp_dir().join(format!("launch-args-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("sample.transfer");
        std::fs::write(&file_path, b"dummy").unwrap();
        let file_url = url::Url::from_file_path(&file_path).unwrap().to_string();

        let extracted = extract_launch_items_from_args([
            "  humoco://pay?to=did:key:abc&amount=5 ",
            "HUMOCO://RECOVERY-REPLY?v=1&data=xyz",
            &file_url,
            file_path.to_str().unwrap(),
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
}

mod checkout;
