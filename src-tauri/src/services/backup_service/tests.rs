use super::*;
use crate::models::{BackupDeltaContainer, MnemonicLanguage};
use crate::services::{common::normalize_sync_url, fs_helpers::record_profile_metadata_in_dir};
use chrono::Utc;
use human_money_core::{
    app_service::AppService,
    models::backup::{BackupProtection, BackupRestoreCredentials},
    AuthMethod, FileStorage,
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

struct TempDirGuard(PathBuf);

impl std::ops::Deref for TempDirGuard {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for TempDirGuard {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn temp_test_dir(name: &str) -> TempDirGuard {
    let p = std::env::temp_dir().join(format!(
        "humoco_backup_test_{}_{}",
        name,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&p).unwrap();
    TempDirGuard(p)
}

fn setup_test_app(dir: &Path, profile_name: &str, password: &str) -> AppService {
    let mut service = AppService::new(dir).unwrap();
    service
        .create_profile(
            profile_name,
            PHRASE,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-test-1".into(),
            true,
        )
        .unwrap();
    service
}

#[test]
fn test_file_backup_roundtrip_mnemonic_and_password() {
    let base_dir = temp_test_dir("file_roundtrip");
    let wallet_dir = base_dir.join("wallet");
    let password = "TestPassword123!";
    let mut service = setup_test_app(&wallet_dir, "Alice Test", password);

    // 1. MnemonicOnly file export
    let mnemonic_export_file = base_dir.join("mnemonic_backup.hmcbackup");
    let mut custom_stds = HashMap::new();
    custom_stds.insert("test_std".to_string(), "name = 'Test Standard'".to_string());
    let summary1 = export_backup_file_impl(
        &mnemonic_export_file,
        BackupProtection::MnemonicOnly,
        Some(password),
        &mut service,
        custom_stds.clone(),
    )
    .unwrap();
    assert!(summary1.total_blobs > 0);
    assert_eq!(summary1.custom_standard_count, 1);

    // Probe mnemonic backup
    let probe1 = probe_wallet_backup_impl(&mnemonic_export_file.to_string_lossy()).unwrap();
    assert_eq!(probe1.source_kind, "file");
    assert_eq!(probe1.protection_mode, "mnemonic");
    assert!(!probe1.supports_password_restore);
    assert!(probe1.supports_mnemonic_restore);
    assert_eq!(probe1.custom_standard_count, 1);

    // Inspect mnemonic backup
    let creds_mn = BackupRestoreCredentials::Mnemonic {
        phrase: PHRASE,
        passphrase: None,
        prefix: Some("pc"),
        language: human_money_core::MnemonicLanguage::English,
    };
    let inspect1 = inspect_backup_with_collision_impl(
        &mnemonic_export_file,
        &creds_mn,
        &service,
        None,
        None,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(inspect1.profile_name, "Alice Test");

    // Restore mnemonic backup into a new service
    let restore_dir1 = base_dir.join("restore_mnemonic");
    let mut restore_service1 = AppService::new(&restore_dir1).unwrap();
    let (outcome1, restored_name1, _) = restore_backup_impl(
        &mnemonic_export_file,
        creds_mn,
        "NewPassword456!",
        "inst_1".to_string(),
        &mut restore_service1,
    )
    .unwrap();
    assert_eq!(restored_name1, "Alice Test");
    assert!(outcome1.folder_name.starts_with("restore_"));

    // 2. CustomBackupPassword file export
    let pwd_export_file = base_dir.join("pwd_backup.hmcbackup");
    let backup_pwd = "MySecretBackupPassword!";
    let summary2 = export_backup_file_impl(
        &pwd_export_file,
        BackupProtection::CustomBackupPassword(backup_pwd),
        Some(password),
        &mut service,
        custom_stds,
    )
    .unwrap();
    assert!(summary2.total_blobs > 0);

    // Probe password backup
    let probe2 = probe_wallet_backup_impl(&pwd_export_file.to_string_lossy()).unwrap();
    assert_eq!(probe2.protection_mode, "backup_password");
    assert!(probe2.supports_password_restore);
    assert!(probe2.supports_mnemonic_restore);

    // Inspect with password
    let creds_pwd = BackupRestoreCredentials::BackupPassword(backup_pwd);
    let inspect2 = inspect_backup_with_collision_impl(
        &pwd_export_file,
        &creds_pwd,
        &service,
        None,
        None,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(inspect2.profile_name, "Alice Test");

    // Inspect with mnemonic (dual recovery slot!)
    let inspect2_mn = inspect_backup_with_collision_impl(
        &pwd_export_file,
        &BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        },
        &service,
        None,
        None,
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(inspect2_mn.profile_name, "Alice Test");

    // Restore password backup
    let restore_dir2 = base_dir.join("restore_pwd");
    let mut restore_service2 = AppService::new(&restore_dir2).unwrap();
    let (_outcome2, restored_name2, _) = restore_backup_impl(
        &pwd_export_file,
        creds_pwd,
        "NewPassword789!",
        "inst_2".to_string(),
        &mut restore_service2,
    )
    .unwrap();
    assert_eq!(restored_name2, "Alice Test");
}

#[test]
fn test_differential_folder_sync_and_restore() {
    let base_dir = temp_test_dir("folder_diff");
    let wallet_dir = base_dir.join("wallet");
    let sync_dir = base_dir.join("sync_folder");
    let password = "TestPassword123!";
    let mut service = setup_test_app(&wallet_dir, "Bob Diff", password);

    // 1. Initial sync
    let summary1 = sync_backup_folder_impl(
        &sync_dir,
        "mnemonic",
        BackupProtection::MnemonicOnly,
        Some(password),
        false,
        &mut service,
        HashMap::new(),
    )
    .unwrap();
    assert!(summary1.changed_blobs > 0);
    assert!(sync_dir.join("manifest.enc").exists());
    assert!(sync_dir.join("blobs").exists());

    // Probe folder
    let probe = probe_wallet_backup_impl(&sync_dir.to_string_lossy()).unwrap();
    assert_eq!(probe.source_kind, "folder");
    assert_eq!(probe.protection_mode, "mnemonic");
    assert_eq!(probe.blob_count, summary1.total_blobs);

    // 2. Second sync with unchanged state -> 0 changed blobs!
    let summary2 = sync_backup_folder_impl(
        &sync_dir,
        "mnemonic",
        BackupProtection::MnemonicOnly,
        Some(password),
        false,
        &mut service,
        HashMap::new(),
    )
    .unwrap();
    assert_eq!(summary2.changed_blobs, 0);
    assert_eq!(summary2.deleted_blobs, 0);
    assert_eq!(summary2.total_blobs, summary1.total_blobs);

    // 3. Restore from folder
    let restore_dir = base_dir.join("restore_folder");
    let mut restore_service = AppService::new(&restore_dir).unwrap();
    let creds = BackupRestoreCredentials::Mnemonic {
        phrase: PHRASE,
        passphrase: None,
        prefix: Some("pc"),
        language: human_money_core::MnemonicLanguage::English,
    };
    let (outcome, name, _) = restore_backup_impl(
        &sync_dir,
        creds,
        "NewPasswordFolder!",
        "inst_f".to_string(),
        &mut restore_service,
    )
    .unwrap();
    assert_eq!(name, "Bob Diff");
    assert!(outcome.folder_name.starts_with("restore_"));
}

#[test]
fn test_tampered_blob_hash_rejected_on_restore() {
    // Invariant: Restoring an untampered backup succeeds, whereas flipping any byte in a stored
    // blob must fail integrity verification with a Backup blob hash mismatch error.
    let base_dir = temp_test_dir("tamper_test");
    let wallet_dir = base_dir.join("wallet");
    let sync_dir = base_dir.join("sync_folder");
    let password = "TestPassword123!";
    let mut service = setup_test_app(&wallet_dir, "Tamper User", password);

    sync_backup_folder_impl(
        &sync_dir,
        "mnemonic",
        BackupProtection::MnemonicOnly,
        Some(password),
        false,
        &mut service,
        HashMap::new(),
    )
    .unwrap();

    // Positive baseline before tampering
    let make_creds = || BackupRestoreCredentials::Mnemonic {
        phrase: PHRASE,
        passphrase: None,
        prefix: Some("pc"),
        language: human_money_core::MnemonicLanguage::English,
    };
    let mut ok_service = AppService::new(&base_dir.join("restore_ok")).unwrap();
    let (_, ok_name, _) = restore_backup_impl(
        &sync_dir,
        make_creds(),
        "NewPasswordOk!",
        "inst_ok".to_string(),
        &mut ok_service,
    )
    .expect("untampered backup folder baseline must restore");
    assert_eq!(ok_name, "Tamper User");

    // Tamper a concrete non-empty blob inside blobs/ without conditional skipping
    let blob_path = fs::read_dir(sync_dir.join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|p| p.is_file() && fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false))
        .expect("Expected at least one non-empty blob to tamper");
    let mut data = fs::read(&blob_path).unwrap();
    data[0] ^= 0xFF;
    fs::write(&blob_path, data).unwrap();

    // Attempt restore -> must fail with hash mismatch error
    let mut restore_service = AppService::new(&base_dir.join("restore_tampered")).unwrap();
    let err = restore_backup_impl(
        &sync_dir,
        make_creds(),
        "NewPasswordTamper!",
        "inst_t".to_string(),
        &mut restore_service,
    )
    .expect_err("tampered blob must be rejected on restore");
    assert!(
        err.message.contains("Backup blob hash mismatch"),
        "expected Backup blob hash mismatch, got: {}",
        err.message
    );
}

#[test]
fn test_auto_backup_usb_protection_folder_disconnected() {
    // Invariant: When auto_backup_enabled is true and backup_sync_folder points to a missing
    // drive/directory, run_auto_backup_sync_with_standards_impl MUST NOT create it, MUST mark
    // backup_dirty=true and return status="disconnected"; once present, it syncs and returns "ok".
    let base_dir = temp_test_dir("usb_protection");
    let non_existent_usb = base_dir.join("media").join("usb_token_offline");
    let password = "TestPassword123!";
    let mut service = setup_test_app(&base_dir.join("wallet"), "Alice USB", password);
    service.unlock_session(password, 3600).unwrap();
    let state = crate::test_support::create_test_state(service);
    {
        let mut s = state.lock_service().unwrap();
        state
            .mutate_security_policy(&mut s, Some(password), |p| {
                p.auto_backup_enabled = true;
                p.backup_protection_mode = Some("mnemonic".to_string());
                p.backup_sync_folder = Some(non_existent_usb.to_string_lossy().to_string());
            })
            .unwrap();
        state
            .mutate_internal_lifecycle(&mut s, Some(password), |lc| {
                lc.backup_dirty = false;
            })
            .unwrap();
    }

    let disconnected_dto = run_auto_backup_sync_with_standards_impl(&state, HashMap::new).unwrap();
    assert_eq!(disconnected_dto.status, "disconnected");
    assert!(disconnected_dto.backup_dirty);
    assert!(
        !non_existent_usb.exists(),
        "USB path must NOT be created by auto-backup when disconnected"
    );
    let lc_disconnected = state
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert!(lc_disconnected.backup_dirty);

    // Positive baseline: mount the USB directory -> auto-backup syncs cleanly and clears dirty flag
    fs::create_dir_all(&non_existent_usb).unwrap();
    let connected_dto = run_auto_backup_sync_with_standards_impl(&state, HashMap::new).unwrap();
    assert_eq!(connected_dto.status, "ok");
    assert!(!connected_dto.backup_dirty);
    assert!(non_existent_usb.join("manifest.enc").exists());
    assert!(connected_dto.changed_blobs.is_some_and(|c| c > 0));
}

#[test]
fn test_is_older_than_14_days() {
    assert!(is_older_than_14_days(&None));
    assert!(is_older_than_14_days(&Some(
        "invalid-timestamp".to_string()
    )));

    let recent = (Utc::now() - chrono::Duration::days(2)).to_rfc3339();
    assert!(!is_older_than_14_days(&Some(recent)));

    let old = (Utc::now() - chrono::Duration::days(16)).to_rfc3339();
    assert!(is_older_than_14_days(&Some(old)));
}

async fn export_and_finalize_restore_for_test(
    base_dir: &Path,
    tag: &str,
    src_svc: &mut AppService,
    password: &str,
) -> (crate::models::FrontendRestoreOutcomeDto, crate::AppState) {
    let bkp_file = base_dir.join(format!("{tag}.hmcbackup"));
    export_backup_file_impl(
        &bkp_file,
        BackupProtection::MnemonicOnly,
        Some(password),
        src_svc,
        HashMap::new(),
    )
    .unwrap();
    let restore_dir = base_dir.join(format!("{tag}_restore"));
    let state = crate::test_support::create_test_state(AppService::new(&restore_dir).unwrap());
    let (outcome, _) = super::finalize_restored_wallet_and_reconcile_l2(
        &bkp_file,
        BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        },
        password,
        format!("inst-{tag}"),
        None,
        &restore_dir,
        &base_dir.join(format!("{tag}_stds")),
        &base_dir.join(format!("{tag}_tmp")),
        &state,
        None,
    )
    .await
    .unwrap();
    (outcome, state)
}

#[tokio::test]
async fn test_restore_anchored_voucher_offline_sets_pending_l2_reconciliation_and_blocks_spend() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::{
        encode_quarter,
        manifest::{AnchorAuthority, AnchorManifest, GenesisTicket, NominatedBy, ServerEntry},
    };
    use human_money_core::models::profile::PublicProfile;
    use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
    use human_money_core::services::l2_gateway;

    // Invariant: Restoring a wallet containing an active L2-anchored voucher when its lock server
    // is unreachable MUST keep pending_restore_l2_reconciliation=true and fail spend gating,
    // whereas restoring an unanchored wallet clears pending_restore_l2_reconciliation=false.
    let base_dir = temp_test_dir("restore_pending_l2_reconcile");
    let password = "TestPassword123!";
    let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
        "voucher_standards/freetaler_v1/standard.toml",
    );

    // 1. Positive baseline: wallet with only unanchored (N=0) voucher clears reconciliation gate
    let mut svc_clean = setup_test_app(&base_dir.join("clean_src"), "Alice Clean", password);
    let clean_uid = svc_clean.with_wallet(|w| w.get_user_id().to_string()).unwrap();
    svc_clean
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(clean_uid),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "20".into(),
                    ..Default::default()
                },
                layer2_anchor: None,
                ..Default::default()
            },
            Some(password),
        )
        .unwrap();
    let (clean_outcome, clean_state) =
        export_and_finalize_restore_for_test(&base_dir, "clean", &mut svc_clean, password).await;
    assert_eq!(clean_outcome.active_voucher_queries_count, 0);
    let clean_lc = clean_state
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert!(!clean_lc.pending_restore_l2_reconciliation);
    {
        let mut s = clean_state.lock_service().unwrap();
        crate::services::common::ensure_restore_l2_reconciled(&clean_state, &mut s, Some(password))
            .expect("unanchored restore must pass spend reconciliation gate");
    }

    // 2. Wallet with active L2-anchored voucher whose lock server refuses connection (offline)
    let dead_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = dead_listener.local_addr().unwrap().port();
    drop(dead_listener);

    let mut svc_l2 = setup_test_app(&base_dir.join("l2_src"), "Alice L2", password);
    let l2_uid = svc_l2.with_wallet(|w| w.get_user_id().to_string()).unwrap();
    let anchor_nonce = [44u8; 32];
    let creator_mig_pub = svc_l2
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let sk = SigningKey::from_bytes(&[77u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 1,
        servers: vec![ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", dead_port),
            read_url: format!("http://127.0.0.1:{}/d", dead_port),
            admin_pub: sk.verifying_key().to_bytes(),
            lock_pub: sk.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        }],
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs: vec![],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let mut ticket = GenesisTicket {
        anchor_id,
        retain_until_quarter: retain_q,
        issued_at: 1_760_000_000,
        lock_pub: sk.verifying_key().to_bytes(),
        sig: [0u8; 64],
    };
    ticket.sig = sk.sign(&ticket.digest()).to_bytes();
    svc_l2
        .register_l2_anchor_template_with_nonce(
            manifest,
            vec![ticket],
            Some(&anchor_nonce),
            Some(password),
        )
        .unwrap();
    svc_l2
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(l2_uid),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "20".into(),
                    ..Default::default()
                },
                layer2_anchor: Some(anchor_id),
                ..Default::default()
            },
            Some(password),
        )
        .unwrap();

    let (l2_outcome, l2_state) =
        export_and_finalize_restore_for_test(&base_dir, "l2", &mut svc_l2, password).await;
    assert_eq!(l2_outcome.active_voucher_queries_count, 1);
    let l2_lc = l2_state
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert!(
        l2_lc.pending_restore_l2_reconciliation,
        "unreachable L2 server after restore must keep pending_restore_l2_reconciliation=true"
    );
    let mut s = l2_state.lock_service().unwrap();
    let gate_err =
        crate::services::common::ensure_restore_l2_reconciled(&l2_state, &mut s, Some(password))
            .expect_err("offline restored anchored wallet must fail spend reconciliation gate");
    assert!(
        gate_err
            .message
            .contains("Online-Sicherheitsabgleich erforderlich"),
        "unexpected gate error: {}",
        gate_err.message
    );
}

#[test]
fn test_compute_backup_status_dto_combined() {
    let mut policy = crate::settings::SecurityPolicy {
        backup_sync_folder: None,
        sync_backup_enabled: false,
        ..Default::default()
    };
    let mut lifecycle = crate::settings::InternalLifecycleState::default();

    // 1. Not configured
    let dto = super::compute_backup_status_dto(&policy, &lifecycle, false);
    assert_eq!(dto.status, "notConfigured");
    assert!(!dto.has_authorized_sync_backup_target);

    // 2. Sync backup only, up to date (< 14 days)
    policy.sync_backup_enabled = true;
    lifecycle.last_sync_backup_at = Some(chrono::Utc::now().to_rfc3339());
    lifecycle.backup_dirty = false;
    let dto = super::compute_backup_status_dto(&policy, &lifecycle, false);
    assert_eq!(dto.status, "ok");
    assert!(dto.sync_backup_enabled);
    assert!(dto.has_authorized_sync_backup_target);

    // 3. Pending dirty changes
    lifecycle.backup_dirty = true;
    let dto = super::compute_backup_status_dto(&policy, &lifecycle, false);
    assert_eq!(dto.status, "pending");

    // 4. Warning: outdated (> 14 days)
    lifecycle.backup_dirty = false;
    lifecycle.last_sync_backup_at = Some("2020-01-01T00:00:00Z".to_string());
    let dto = super::compute_backup_status_dto(&policy, &lifecycle, true);
    assert_eq!(dto.status, "warning");

    // 5. Local folder disconnected
    policy.backup_sync_folder = Some("/non/existent/path/for/humoco/test".to_string());
    policy.sync_backup_enabled = false;
    let dto = super::compute_backup_status_dto(&policy, &lifecycle, false);
    assert_eq!(dto.status, "disconnected");
}

#[test]
fn test_s07_2_compute_backup_status_dto_excludes_revoked_sync_targets() {
    let mut policy = crate::settings::SecurityPolicy {
        backup_sync_folder: None,
        sync_backup_enabled: true,
        market_selected_syncs: vec![crate::models::FrontendSyncDescriptor {
            write_url: "https://revoked-friend.example.org/sync.php".to_string(),
            read_url: "https://revoked-friend.example.org/d".to_string(),
            drop_pub: "11".repeat(32),
            status: "revoked".to_string(),
            ..Default::default()
        }],
        public_syncs: vec![crate::settings::StoredPublicSync {
            write_url: "https://revoked-pub.example.org/sync.php".to_string(),
            read_url: "https://revoked-pub.example.org/d".to_string(),
            drop_pub: "22".repeat(32),
            access_pubkey: "33".repeat(32),
            enabled: true,
            status: "revoked".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let lifecycle = crate::settings::InternalLifecycleState {
        last_sync_backup_at: None,
        ..Default::default()
    };

    let dto = super::compute_backup_status_dto(&policy, &lifecycle, false);
    assert!(
        !dto.has_authorized_sync_backup_target,
        "S07-2: revoked friend/public syncs must not count as authorized backup targets"
    );
    assert_eq!(dto.status, "notConfigured");

    // Adding a non-revoked active public sync makes target available
    policy.public_syncs.push(crate::settings::StoredPublicSync {
        write_url: "https://active-pub.example.org/sync.php".to_string(),
        read_url: "https://active-pub.example.org/d".to_string(),
        drop_pub: "44".repeat(32),
        access_pubkey: "55".repeat(32),
        enabled: true,
        status: "active".to_string(),
        ..Default::default()
    });
    let dto_active = super::compute_backup_status_dto(&policy, &lifecycle, false);
    assert!(dto_active.has_authorized_sync_backup_target);
}

#[test]
fn test_merge_backup_delta_into_base_and_mismatch() {
    let base_manifest = b"base-manifest-v1";
    let base_sha = super::sha256_hex(base_manifest);

    let mut base = crate::models::PortableBackupContainer {
        container_format: "humoco-app-backup-v1".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        manifest_hex: human_money_core::hex::encode(base_manifest),
        blobs_hex: [
            ("keep.bin".to_string(), "aa".to_string()),
            ("remove.bin".to_string(), "bb".to_string()),
        ]
        .into_iter()
        .collect(),
        custom_standards: HashMap::new(),
    };

    let delta = crate::models::BackupDeltaContainer {
        container_format: "humoco-app-backup-delta-v1".to_string(),
        created_at: "2026-01-02T00:00:00Z".to_string(),
        base_manifest_sha256: base_sha,
        manifest_hex: human_money_core::hex::encode(b"manifest-v2"),
        put_blobs_hex: [("added.bin".to_string(), "cc".to_string())]
            .into_iter()
            .collect(),
        delete_blobs: vec!["remove.bin".to_string()],
        custom_standards: [("std1".to_string(), "toml".to_string())]
            .into_iter()
            .collect(),
    };

    super::merge_backup_delta_into_base(&mut base, &delta).expect("matching base must merge");
    assert_eq!(base.created_at, "2026-01-02T00:00:00Z");
    assert_eq!(
        base.manifest_hex,
        human_money_core::hex::encode(b"manifest-v2")
    );
    assert!(!base.blobs_hex.contains_key("remove.bin"));
    assert_eq!(
        base.blobs_hex.get("keep.bin").map(String::as_str),
        Some("aa")
    );
    assert_eq!(
        base.blobs_hex.get("added.bin").map(String::as_str),
        Some("cc")
    );
    assert_eq!(
        base.custom_standards.get("std1").map(String::as_str),
        Some("toml")
    );

    // Mismatching base_manifest_sha256 must be rejected
    let mut bad_delta = delta.clone();
    bad_delta.base_manifest_sha256 = "0000".to_string();
    let mismatch_err = super::merge_backup_delta_into_base(&mut base, &bad_delta)
        .expect_err("mismatching base_manifest_sha256 must fail");
    assert!(
        mismatch_err.contains("Backup delta base mismatch"),
        "unexpected delta mismatch error: {}",
        mismatch_err
    );

    // Mismatching base container_format must also be rejected
    let mut bad_base = base.clone();
    bad_base.container_format = "wrong-base-v99".to_string();
    let base_fmt_err = super::merge_backup_delta_into_base(&mut bad_base, &delta)
        .expect_err("unsupported base container format must fail");
    assert!(
        base_fmt_err.contains("Unsupported base backup container format: wrong-base-v99"),
        "unexpected base format error: {}",
        base_fmt_err
    );
}

struct SyncBackupMirrorHarness {
    _base_dir: TempDirGuard,
    base_dir_path: PathBuf,
    state: crate::AppState,
    server: crate::test_support::MockSyncServer,
    stealth_pk: String,
    read_url: String,
    base_slot_key: String,
    delta_slot_key: String,
}

async fn setup_base_delta_mirror_harness(name: &str, password: &str) -> SyncBackupMirrorHarness {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, MockSyncServer, ALICE_PHRASE,
    };
    let base_dir = temp_test_dir(name);
    let base_dir_path = base_dir.0.clone();
    let service = setup_sync_test_app(&base_dir.join("wallet"), "Alice", ALICE_PHRASE, password);
    let state = create_test_state(service);

    let (_admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&[0x42u8; 32], 0);
    let drop_pub = hex::encode(admin_vk.to_bytes());
    let server = MockSyncServer::start("alice_bkp".to_string(), drop_pub.clone()).await;

    let stealth_pk = {
        let s = state.service.lock().unwrap();
        let my_ed_sk = s
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .unwrap();
        humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub).unwrap()
    };
    server.members.lock().unwrap().push(stealth_pk.clone());

    let write_url = format!("{}/sync.php", server.base_url);
    let read_url = format!("{}/alice_bkp", server.base_url);
    let base_slot_key = format!("{}_pc.backup", stealth_pk);
    let delta_slot_key = format!("{}_pc-delta.backup", stealth_pk);

    {
        let mut s = state.service.lock().unwrap();
        state
            .mutate_security_policy(&mut s, Some(password), |policy| {
                policy.sync_backup_enabled = true;
                policy.market_selected_syncs = vec![crate::models::FrontendSyncDescriptor {
                    write_url,
                    read_url: read_url.clone(),
                    drop_pub,
                    ..Default::default()
                }];
            })
            .unwrap();
    }

    SyncBackupMirrorHarness {
        _base_dir: base_dir,
        base_dir_path,
        state,
        server,
        stealth_pk,
        read_url,
        base_slot_key,
        delta_slot_key,
    }
}

#[tokio::test]
async fn test_sync_backup_small_vs_large_base_and_delta_restore() {
    use crate::test_support::ALICE_PHRASE;
    use rand::{rngs::OsRng, RngCore};

    let password = "TestPassword123!";
    let h = setup_base_delta_mirror_harness("sync_backup_base_delta_e2e", password).await;

    // 1. Small wallet (< 64 KiB compressed): repeated mutations overwrite Base, never create Delta
    let res1 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &h.state,
    )
    .await
    .unwrap();
    assert_eq!(res1.uploaded_to, 1, "errors: {:?}", res1.errors);
    assert_eq!(res1.padded_bytes, 65_536);
    assert!(h.server.slots.lock().unwrap().contains_key(&h.base_slot_key));
    assert!(!h.server.slots.lock().unwrap().contains_key(&h.delta_slot_key));

    {
        let mut s = h.state.service.lock().unwrap();
        s.save_encrypted_data("small_key", b"small-v2", Some(password))
            .unwrap();
    }
    let res2 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &h.state,
    )
    .await
    .unwrap();
    assert_eq!(res2.uploaded_to, 1, "errors: {:?}", res2.errors);
    assert_eq!(res2.padded_bytes, 65_536);
    assert!(h.server.slots.lock().unwrap().contains_key(&h.base_slot_key));
    assert!(!h.server.slots.lock().unwrap().contains_key(&h.delta_slot_key));

    // 2. Large wallet (>= 128 KiB padded Base): triggers fresh Base rollover
    let mut big_random = vec![0u8; 150 * 1024];
    OsRng.fill_bytes(&mut big_random);
    {
        let mut s = h.state.service.lock().unwrap();
        s.save_encrypted_data("big_blob", &big_random, Some(password))
            .unwrap();
    }
    let res3 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &h.state,
    )
    .await
    .unwrap();
    assert_eq!(res3.uploaded_to, 1, "errors: {:?}", res3.errors);
    assert!(res3.padded_bytes >= 131_072);
    assert!(h.server.slots.lock().unwrap().contains_key(&h.base_slot_key));
    assert!(!h.server.slots.lock().unwrap().contains_key(&h.delta_slot_key));

    // 3. Small mutation after large Base -> uploads Delta slot & restores full merged state
    {
        let mut s = h.state.service.lock().unwrap();
        s.save_encrypted_data("small_after_base", b"delta-payload", Some(password))
            .unwrap();
    }
    let res4 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &h.state,
    )
    .await
    .unwrap();
    assert_eq!(res4.uploaded_to, 1, "errors: {:?}", res4.errors);
    assert_eq!(res4.padded_bytes, 65_536);
    assert!(h.server.slots.lock().unwrap().contains_key(&h.base_slot_key));
    assert!(h.server.slots.lock().unwrap().contains_key(&h.delta_slot_key));

    let fetch_client = crate::sync_client::create_sync_http_client();
    let merged_bytes = super::fetch_and_merge_backup_slot(
        &fetch_client,
        &h.read_url,
        &h.stealth_pk,
        Some("pc"),
    )
    .await
    .expect("merged backup container must be fetched");
    let staged_file = h.base_dir_path.join("merged_sync_restore.hmcbackup");
    crate::services::fs_helpers::write_file_atomic(&staged_file, &merged_bytes).unwrap();

    let restored_wallet_dir = h.base_dir_path.join("restored_wallet");
    let mut restored_service = AppService::new(&restored_wallet_dir).unwrap();
    let restore_creds = BackupRestoreCredentials::Mnemonic {
        phrase: ALICE_PHRASE,
        passphrase: None,
        prefix: Some("pc"),
        language: human_money_core::MnemonicLanguage::English,
    };
    let (outcome, profile_name, _) = super::restore_backup_impl(
        &staged_file,
        restore_creds,
        password,
        "device-restored".to_string(),
        &mut restored_service,
    )
    .unwrap();
    assert_eq!(profile_name, "Alice");
    let mut verify_service = AppService::new(&restored_wallet_dir).unwrap();
    verify_service
        .login(
            &outcome.folder_name,
            password,
            false,
            "device-restored".to_string(),
        )
        .unwrap();
    assert_eq!(
        verify_service
            .load_encrypted_data("big_blob", Some(password))
            .unwrap(),
        big_random
    );
    assert_eq!(
        verify_service
            .load_encrypted_data("small_after_base", Some(password))
            .unwrap(),
        b"delta-payload"
    );
}

#[tokio::test]
async fn test_sync_backup_remote_base_wipe_heals_and_cleans_orphaned_delta() {
    use rand::{rngs::OsRng, RngCore};

    let password = "TestPassword123!";
    let h = setup_base_delta_mirror_harness("sync_backup_remote_wipe", password).await;

    let mut big_random = vec![0u8; 150 * 1024];
    OsRng.fill_bytes(&mut big_random);
    {
        let mut s = h.state.service.lock().unwrap();
        s.save_encrypted_data("big_blob", &big_random, Some(password))
            .unwrap();
    }
    let res_base = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &h.state,
    )
    .await
    .unwrap();
    assert!(res_base.padded_bytes >= 131_072);

    {
        let mut s = h.state.service.lock().unwrap();
        s.save_encrypted_data("small_after_base", b"delta-payload", Some(password))
            .unwrap();
    }
    super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &h.state,
    )
    .await
    .unwrap();
    assert!(h.server.slots.lock().unwrap().contains_key(&h.delta_slot_key));

    // Remote Base wipe auto-recovery & orphaned Delta cleanup
    h.server.slots.lock().unwrap().remove(&h.base_slot_key);
    h.server.etags.lock().unwrap().remove(&h.base_slot_key);
    assert!(h.server.slots.lock().unwrap().contains_key(&h.delta_slot_key));

    let res_heal = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &h.state,
    )
    .await
    .unwrap();
    assert_eq!(res_heal.uploaded_to, 1, "errors: {:?}", res_heal.errors);
    assert!(res_heal.padded_bytes >= 131_072);
    assert!(h.server.slots.lock().unwrap().contains_key(&h.base_slot_key));
    assert!(
        !h.server.slots.lock().unwrap().contains_key(&h.delta_slot_key),
        "orphaned delta slot must be deleted after Base re-upload"
    );
}

fn copy_dir_recursive_for_test(src: &std::path::Path, dst: &std::path::Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive_for_test(&entry.path(), &dest_path);
        } else {
            fs::copy(entry.path(), &dest_path).unwrap();
        }
    }
}

async fn setup_authorized_backup_mirror(
    state: &crate::AppState,
    password: &str,
) -> crate::test_support::MockSyncServer {
    use crate::models::FrontendMemberPolicyDto;
    use crate::services::sync_engine::{finalize_sync_setup_impl, prepare_sync_setup_impl};
    use crate::test_support::MockSyncServer;

    let draft = prepare_sync_setup_impl(
        Some(FrontendMemberPolicyDto {
            direct_min: 2,
            hop2_enabled: true,
            bridge_min: 2,
            hop2_min: 2,
        }),
        Some(300),
        Some(256),
        None,
        Some(password.to_string()),
        state,
    )
    .unwrap();
    let server = MockSyncServer::start(draft.data_dir.clone(), draft.admin_pubkey.clone()).await;
    let fin = finalize_sync_setup_impl(server.base_url.clone(), Some(password.to_string()), state)
        .await
        .unwrap();

    {
        let mut s = state.service.lock().unwrap();
        state
            .mutate_security_policy(&mut s, Some(password), |policy| {
                policy.sync_backup_enabled = true;
                policy.market_selected_syncs = vec![fin.descriptor.clone()];
            })
            .unwrap();
        let admin_pub = server.admin_pubkey.lock().unwrap().clone();
        let write_url = format!("{}/sync.php", server.base_url);
        let read_url = format!("{}/{}", server.base_url, server.data_dir);
        let lock_key = server.lock_pubkey.to_ascii_lowercase();
        state
            .mutate_internal_lifecycle(&mut s, Some(password), |lc| {
                lc.l2_probe_cache.insert(
                    lock_key.clone(),
                    crate::sync_client::FrontendLockServerProbeResult {
                        write_url,
                        read_url,
                        drop_pub: admin_pub.clone(),
                        lock_pub: lock_key,
                        admin_pub,
                        delegation_valid: true,
                        waf_probe_passed: true,
                        selftest_passed: true,
                        key_leak_safe: true,
                        passed: true,
                        latency_ms: 5,
                        max_retention_quarter: 20304,
                        pow_base_bits: 8,
                        allow_pow_tickets: false,
                        ticket_pow_bits: 16,
                        inode_warning: false,
                        used_inodes: 0,
                        max_inodes: 1000,
                        probed_at_secs: 1_760_000_000,
                        delegation: None,
                        error: None,
                    },
                );
            })
            .unwrap();
    }
    server
}

#[tokio::test]
async fn test_acknowledge_seal_sync_marks_wallet_synced() {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, ALICE_PHRASE,
    };
    use human_money_core::models::seal::SyncStatus;

    let base_dir = temp_test_dir("ack_seal_sync_marks_synced");
    let password = "TestPassword123!";
    let service = setup_sync_test_app(&base_dir.join("wallet"), "Alice", ALICE_PHRASE, password);
    let state = create_test_state(service);
    let server = setup_authorized_backup_mirror(&state, password).await;

    {
        let mut s = state.service.lock().unwrap();
        s.unlock_session(password, 3600).unwrap();
        s.save_encrypted_data("seal_sync_check", b"initial-data", Some(password))
            .unwrap();
        assert_eq!(s.get_seal_sync_status().unwrap(), SyncStatus::PendingUpload);
    }

    let res = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res.uploaded_to, 1, "errors: {:?}", res.errors);
    assert_eq!(server.slots.lock().unwrap().len(), 1);

    {
        let s = state.service.lock().unwrap();
        assert_eq!(s.get_seal_sync_status().unwrap(), SyncStatus::Synced);
    }
    let lc = state
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert!(!lc.pending_restore_l2_reconciliation);
    assert!(lc.wallet_sync_alert.is_none());
}

struct ClonedMirrorSetup {
    _base_dir: TempDirGuard,
    server: crate::test_support::MockSyncServer,
    wallet_b_dir: PathBuf,
    server_slots_after_a: HashMap<String, Vec<u8>>,
    server_etags_after_a: HashMap<String, String>,
}

async fn setup_advanced_mirror_with_v1_clone(name: &str, password: &str) -> ClonedMirrorSetup {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, ALICE_PHRASE,
    };

    let base_dir = temp_test_dir(name);
    let wallet_a_dir = base_dir.join("wallet_a");
    let wallet_b_dir = base_dir.join("wallet_b");

    let service_a = setup_sync_test_app(&wallet_a_dir, "Alice", ALICE_PHRASE, password);
    let state_a = create_test_state(service_a);
    let server = setup_authorized_backup_mirror(&state_a, password).await;

    let res_a1 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state_a,
    )
    .await
    .unwrap();
    assert_eq!(res_a1.uploaded_to, 1, "errors: {:?}", res_a1.errors);

    // Clone Device A at v1 BEFORE A advances
    copy_dir_recursive_for_test(&wallet_a_dir, &wallet_b_dir);

    {
        let mut s = state_a.service.lock().unwrap();
        s.save_encrypted_data("tx_step_1", b"state-a-2", Some(password))
            .unwrap();
        s.save_encrypted_data("tx_step_2", b"state-a-3", Some(password))
            .unwrap();
    }
    let res_a2 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state_a,
    )
    .await
    .unwrap();
    assert_eq!(res_a2.uploaded_to, 1, "errors: {:?}", res_a2.errors);
    let server_slots_after_a = server.slots.lock().unwrap().clone();
    let server_etags_after_a = server.etags.lock().unwrap().clone();

    ClonedMirrorSetup {
        _base_dir: base_dir,
        server,
        wallet_b_dir,
        server_slots_after_a,
        server_etags_after_a,
    }
}

#[tokio::test]
async fn test_sync_backup_stale_clone_aborts_upload_and_blocks_spend() {
    use crate::services::preflight::{run_value_outflow_preflight, ValueOutflowPreflight};
    use crate::test_support::create_test_state;

    // Invariant: An unmodified stale v1 clone detecting a newer remote backup MUST abort upload,
    // persist stale_local_backup alert + pending_restore_l2_reconciliation, and fail spend preflight.
    let password = "TestPassword123!";
    let setup = setup_advanced_mirror_with_v1_clone("sync_backup_stale_guard", password).await;

    let mut service_b_stale = AppService::new(&setup.wallet_b_dir).unwrap();
    let profiles_stale = service_b_stale.list_profiles().unwrap();
    service_b_stale
        .login(
            &profiles_stale[0].folder_name,
            password,
            false,
            "device-drops-test".to_string(),
        )
        .unwrap();
    let state_b_stale = create_test_state(service_b_stale);
    {
        let mut lc = state_b_stale
            .get_or_load_internal_lifecycle_locked(Some(password))
            .unwrap();
        lc.backup_dirty = true;
        let mut s = state_b_stale.service.lock().unwrap();
        crate::state::save_local_sidecar_json(
            &mut s,
            crate::settings::INTERNAL_LIFECYCLE_STATE_KEY,
            &lc,
            Some(password),
        )
        .unwrap();
        state_b_stale.set_cached_internal_lifecycle(Some(lc));

        // Positive baseline BEFORE stale sync detects remote advancement: spend preflight passes
        run_value_outflow_preflight(
            &state_b_stale,
            &mut s,
            Some(password),
            &ValueOutflowPreflight {
                require_restore_reconciled: true,
                check_known_offender_dids: &[],
                check_blocking_intent_local_ids: &[],
                check_anchor_id_quorum_health: None,
                preverified_anchor_manifest: None,
            },
        )
        .expect("baseline preflight must pass before stale clone sync");
    }

    let res_b_stale = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state_b_stale,
    )
    .await
    .unwrap();
    assert_eq!(res_b_stale.uploaded_to, 0);
    assert_eq!(
        *setup.server.slots.lock().unwrap(),
        setup.server_slots_after_a
    );
    assert_eq!(
        *setup.server.etags.lock().unwrap(),
        setup.server_etags_after_a
    );

    let lc_stale = state_b_stale
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert!(lc_stale.pending_restore_l2_reconciliation);
    assert_eq!(
        lc_stale.wallet_sync_alert.as_ref().map(|a| a.kind.as_str()),
        Some("stale_local_backup")
    );

    let spend_err = {
        let mut s = state_b_stale.service.lock().unwrap();
        run_value_outflow_preflight(
            &state_b_stale,
            &mut s,
            Some(password),
            &ValueOutflowPreflight {
                require_restore_reconciled: true,
                check_known_offender_dids: &[],
                check_blocking_intent_local_ids: &[],
                check_anchor_id_quorum_health: None,
                preverified_anchor_manifest: None,
            },
        )
    }
    .expect_err("stale clone must fail spend preflight after detecting newer remote backup");
    assert!(
        spend_err
            .message
            .contains("Online-Sicherheitsabgleich erforderlich"),
        "unexpected preflight error: {}",
        spend_err.message
    );
}

#[tokio::test]
async fn test_sync_backup_divergent_fork_aborts_upload_and_locks_seal() {
    use crate::test_support::create_test_state;
    use human_money_core::models::seal::SealLockReason;

    // Invariant: A diverged clone (local offline write after v1) attempting sync against newer
    // remote state MUST abort upload, record fork_detected, and persist SealLockReason::ForkDetected.
    let password = "TestPassword123!";
    let setup = setup_advanced_mirror_with_v1_clone("sync_backup_fork_guard", password).await;

    let mut service_b_fork = AppService::new(&setup.wallet_b_dir).unwrap();
    let profiles_fork = service_b_fork.list_profiles().unwrap();
    service_b_fork
        .login(
            &profiles_fork[0].folder_name,
            password,
            false,
            "device-drops-test".to_string(),
        )
        .unwrap();
    service_b_fork
        .save_encrypted_data("tx_fork_b", b"state-b-fork", Some(password))
        .unwrap();
    let state_b_fork = create_test_state(service_b_fork);

    let res_b_fork = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state_b_fork,
    )
    .await
    .unwrap();
    assert_eq!(res_b_fork.uploaded_to, 0);
    assert_eq!(
        *setup.server.slots.lock().unwrap(),
        setup.server_slots_after_a
    );

    let lc_fork = state_b_fork
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert!(lc_fork.pending_restore_l2_reconciliation);
    assert_eq!(
        lc_fork.wallet_sync_alert.as_ref().map(|a| a.kind.as_str()),
        Some("fork_detected")
    );
    let s = state_b_fork.service.lock().unwrap();
    assert_eq!(
        s.get_seal_lock_reason(Some(password)).unwrap(),
        Some(SealLockReason::ForkDetected)
    );
}

#[tokio::test]
async fn test_sync_backup_detects_server_rollback_with_signed_ack_and_heals() {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, ALICE_PHRASE,
    };

    let base_dir = temp_test_dir("sync_backup_server_rollback");
    let password = "TestPassword123!";
    let service = setup_sync_test_app(&base_dir.join("wallet"), "Alice", ALICE_PHRASE, password);
    let state = create_test_state(service);
    let server = setup_authorized_backup_mirror(&state, password).await;

    // 1. Upload v1 and snapshot server state
    let res1 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res1.uploaded_to, 1, "errors: {:?}", res1.errors);
    let v1_slots = server.slots.lock().unwrap().clone();
    let v1_etags = server.etags.lock().unwrap().clone();

    // 2. Advance to v2 and sync -> server issues verified SlotAckReceipt
    {
        let mut s = state.service.lock().unwrap();
        s.save_encrypted_data("rollback_step_2", b"v2-data", Some(password))
            .unwrap();
    }
    let res2 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res2.uploaded_to, 1, "errors: {:?}", res2.errors);
    let v2_etags = server.etags.lock().unwrap().clone();
    assert_ne!(v1_etags, v2_etags);

    // 3. Roll back server storage & ETags to stale v1 and advance local wallet to v3
    *server.slots.lock().unwrap() = v1_slots;
    *server.etags.lock().unwrap() = v1_etags;
    {
        let mut s = state.service.lock().unwrap();
        s.save_encrypted_data("rollback_step_3", b"v3-data", Some(password))
            .unwrap();
    }

    let res3 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res3.uploaded_to, 1, "mirror must heal via Base re-upload");
    assert_ne!(*server.etags.lock().unwrap(), v2_etags);

    let lc = state
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    let alert = lc
        .wallet_sync_alert
        .expect("server_rollback alert must be recorded");
    assert_eq!(alert.kind, "server_rollback");
    assert!(alert.has_signed_ack_proof);
}

#[tokio::test]
async fn test_sync_backup_wipe_alert_respects_server_ttl() {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, ALICE_PHRASE,
    };

    let base_dir = temp_test_dir("sync_backup_wipe_ttl");
    let password = "TestPassword123!";
    let service = setup_sync_test_app(&base_dir.join("wallet"), "Alice", ALICE_PHRASE, password);
    let state = create_test_state(service);
    let server = setup_authorized_backup_mirror(&state, password).await;

    // 1. Initial upload (mock server default ttl_days = 30)
    let res1 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res1.uploaded_to, 1, "errors: {:?}", res1.errors);

    // 2. Wipe server slot WITHIN TTL -> must emit unexpected_data_loss and heal mirror
    server.slots.lock().unwrap().clear();
    server.etags.lock().unwrap().clear();
    {
        let mut s = state.service.lock().unwrap();
        s.save_encrypted_data("wipe_within_ttl", b"step-2", Some(password))
            .unwrap();
    }
    let res2 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res2.uploaded_to, 1, "errors: {:?}", res2.errors);
    let lc2 = state
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert_eq!(
        lc2.wallet_sync_alert.as_ref().map(|a| a.kind.as_str()),
        Some("unexpected_data_loss")
    );

    // 3. Clear alert, age base_uploaded_at past server ttl_days (7d), wipe again -> NO unexpected_data_loss
    *server.ttl_days.lock().unwrap() = 7;
    {
        let mut lc = lc2;
        lc.wallet_sync_alert = None;
        let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
        let mut s = state.service.lock().unwrap();
        crate::state::save_local_sidecar_json(
            &mut s,
            crate::settings::INTERNAL_LIFECYCLE_STATE_KEY,
            &lc,
            Some(password),
        )
        .unwrap();
        state.set_cached_internal_lifecycle(Some(lc));

        let mut bkp_state: crate::state::StoredBackupSyncState =
            crate::state::load_local_sidecar_json_or_default(
                &mut s,
                crate::state::BACKUP_SYNC_STATE_KEY,
                Some(password),
            )
            .unwrap();
        for mirror in bkp_state.mirrors_by_write_url.values_mut() {
            mirror.base_uploaded_at = now_secs.saturating_sub(10 * 86_400);
        }
        crate::state::save_local_sidecar_json(
            &mut s,
            crate::state::BACKUP_SYNC_STATE_KEY,
            &bkp_state,
            Some(password),
        )
        .unwrap();
    }

    server.slots.lock().unwrap().clear();
    server.etags.lock().unwrap().clear();
    {
        let mut s = state.service.lock().unwrap();
        s.save_encrypted_data("wipe_past_ttl", b"step-3", Some(password))
            .unwrap();
    }
    let res3 = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res3.uploaded_to, 1, "errors: {:?}", res3.errors);
    let lc3 = state
        .get_or_load_internal_lifecycle_locked(Some(password))
        .unwrap();
    assert!(
        lc3.wallet_sync_alert.is_none(),
        "expired slot beyond ttl_days must not raise unexpected_data_loss alert: {:?}",
        lc3.wallet_sync_alert
    );
}

#[test]
fn test_preview_mnemonic_identity_collision_and_subaccount() {
    let base_dir = temp_test_dir("preview_collision");
    let wallet_dir = base_dir.join("wallet");
    let password = "TestPassword123!";
    let mut service = AppService::new(&wallet_dir).unwrap();
    service
        .create_profile(
            "Alice Root",
            PHRASE,
            None,
            None,
            password,
            human_money_core::MnemonicLanguage::English,
            "inst-1".to_string(),
            true,
        )
        .unwrap();

    // Same secrets without prefix -> collides with Alice Root
    let root_preview = super::preview_mnemonic_identity_impl(
        PHRASE,
        None,
        None,
        Some(MnemonicLanguage::English),
        &wallet_dir,
        &service,
    )
    .unwrap();
    let coll = root_preview
        .local_collision
        .expect("root profile must collide");
    assert_eq!(coll.profile_name, "Alice Root");
    assert_eq!(coll.local_epoch, Some(0));
    assert_eq!(coll.local_tx_nonce, Some(0));
    assert!(!coll.is_locked_or_conflicted);

    // Same mnemonic with new subaccount prefix -> distinct user_id, NO collision
    let sub_preview = super::preview_mnemonic_identity_impl(
        PHRASE,
        None,
        Some("mobil"),
        Some(MnemonicLanguage::English),
        &wallet_dir,
        &service,
    )
    .unwrap();
    assert!(sub_preview.user_id.starts_with("mobil:"));
    assert!(sub_preview.local_collision.is_none());
}

#[tokio::test]
async fn test_probe_sync_backup_found_and_not_found() {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, MockSyncServer, ALICE_PHRASE,
    };

    let base_dir = temp_test_dir("probe_sync_backup_test");
    let wallet_dir = base_dir.join("wallet");
    let tmp_restore_dir = base_dir.join("tmp_restore");
    let password = "TestPassword123!";
    let service = setup_sync_test_app(&wallet_dir, "Alice", ALICE_PHRASE, password);
    let state = create_test_state(service);

    let (_admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&[0x55u8; 32], 0);
    let drop_pub = hex::encode(admin_vk.to_bytes());
    let server = MockSyncServer::start("alice_probe".to_string(), drop_pub.clone()).await;

    let stealth_pk = {
        let s = state.service.lock().unwrap();
        let my_ed_sk = s
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .unwrap();
        humoco_market::derive_member_stealth_pubkey(&my_ed_sk, &drop_pub).unwrap()
    };
    server.members.lock().unwrap().push(stealth_pk.clone());

    let write_url = format!("{}/sync.php", server.base_url);
    let read_url = format!("{}/alice_probe", server.base_url);

    {
        let mut s = state.service.lock().unwrap();
        state
            .mutate_security_policy(&mut s, Some(password), |policy| {
                policy.sync_backup_enabled = true;
                policy.market_selected_syncs = vec![crate::models::FrontendSyncDescriptor {
                    write_url: write_url.clone(),
                    read_url: read_url.clone(),
                    drop_pub: drop_pub.clone(),
                    ..Default::default()
                }];
            })
            .unwrap();
    }

    // 1. Before any backup is uploaded -> status must be "not_found" + resolved_sync_targets populated
    let probe_dir_empty = base_dir.join("empty_wallet");
    let empty_state = create_test_state(AppService::new(&probe_dir_empty).unwrap());
    let not_found_res = super::probe_sync_backup_impl(
        vec![write_url.clone()],
        ALICE_PHRASE.to_string(),
        None,
        Some("pc".to_string()),
        Some(MnemonicLanguage::English),
        &probe_dir_empty,
        &tmp_restore_dir,
        &empty_state,
    )
    .await
    .unwrap();
    assert_eq!(not_found_res.status, "not_found");
    assert_eq!(not_found_res.resolved_sync_targets.len(), 1);
    assert_eq!(not_found_res.resolved_sync_targets[0].drop_pub, drop_pub);

    // 2. Upload backup, then probe with only plain write_url (tests fetch_sync_info + stealth resolution!)
    let sync_res = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(sync_res.uploaded_to, 1);

    let found_res = super::probe_sync_backup_impl(
        vec![write_url.clone()],
        ALICE_PHRASE.to_string(),
        None,
        Some("pc".to_string()),
        Some(MnemonicLanguage::English),
        &wallet_dir,
        &tmp_restore_dir,
        &state,
    )
    .await
    .unwrap();
    assert_eq!(found_res.status, "found");
    assert_eq!(found_res.matched_key_kind.as_deref(), Some("stealth"));
    assert!(found_res
        .staged_path
        .as_deref()
        .is_some_and(|p| Path::new(p).exists()));
    let inspection = found_res.inspection.expect("inspection must be populated");
    assert_eq!(inspection.profile_name, "Alice");
    let coll = inspection
        .local_collision
        .expect("found backup must report local profile collision");
    assert_eq!(coll.profile_name, "Alice");
    assert!(!coll.is_locked_or_conflicted);
}

#[tokio::test]
async fn test_replace_local_profile_and_reset_password_via_backup() {
    let base_dir = temp_test_dir("replace_and_pw_reset");
    let wallet_dir = base_dir.join("wallet");
    let standards_dir = base_dir.join("voucher_standards");
    let tmp_restore_dir = base_dir.join("tmp_restore");
    let old_pass = "OldPassword123!";
    let new_pass = "NewPassword456!";
    let bkp_pass = "BackupSecret789!";

    let mut service = setup_test_app(&wallet_dir, "Alice", old_pass);
    let original_folder = service.list_profiles().unwrap()[0].folder_name.clone();
    let alice_uid = service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let bkp_file = base_dir.join("alice.hmcbackup");
    super::export_backup_file_impl(
        &bkp_file,
        BackupProtection::CustomBackupPassword(bkp_pass),
        Some(old_pass),
        &mut service,
        HashMap::new(),
    )
    .unwrap();
    service.logout();

    let state = crate::test_support::create_test_state(service);

    // 1. Reset local password via backup password
    super::reset_local_password_via_backup_impl(
        &original_folder,
        &bkp_file,
        "password",
        Some(bkp_pass),
        None,
        None,
        None,
        new_pass,
        "inst-pw-reset".to_string(),
        &wallet_dir,
        &tmp_restore_dir,
        &state,
        None,
    )
    .unwrap();

    {
        let mut s = state.service.lock().unwrap();
        s.logout();
        let old_login_err = s
            .login(
                &original_folder,
                old_pass,
                false,
                "inst-pw-reset".to_string(),
            )
            .expect_err("old password must be rejected after reset");
        assert!(
            old_login_err
                .to_string()
                .contains("Login failed (check password)"),
            "unexpected login error with old password: {}",
            old_login_err
        );
        s.login(
            &original_folder,
            new_pass,
            false,
            "inst-pw-reset".to_string(),
        )
        .expect("new password must unlock wallet after reset");
        assert_eq!(
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap(),
            alice_uid
        );
        s.logout();
    }

    // 2. Restore with replace_folder_name -> keeps clean name "Alice" and deletes original_folder
    let (outcome, _) = super::finalize_restored_wallet_and_reconcile_l2(
        &bkp_file,
        BackupRestoreCredentials::BackupPassword(bkp_pass),
        new_pass,
        "inst-replace".to_string(),
        Some(&original_folder),
        &wallet_dir,
        &standards_dir,
        &tmp_restore_dir,
        &state,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.profile_name, "Alice");
    assert_ne!(outcome.folder_name, original_folder);
    assert!(!wallet_dir.join(&original_folder).exists());

    let remaining_profiles = {
        let s = state.service.lock().unwrap();
        s.list_profiles().unwrap()
    };
    assert_eq!(remaining_profiles.len(), 1);
    assert_eq!(remaining_profiles[0].profile_name, "Alice");
    assert_eq!(remaining_profiles[0].folder_name, outcome.folder_name);
}

#[tokio::test]
async fn test_sync_backup_respects_wifi_only_skip() {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, ALICE_PHRASE,
    };

    let base_dir = temp_test_dir("sync_backup_wifi_only_skip");
    let wallet_dir = base_dir.join("wallet");
    let password = "TestPassword123!";
    let service = setup_sync_test_app(&wallet_dir, "Alice", ALICE_PHRASE, password);
    let state = create_test_state(service);

    {
        let mut s = state.service.lock().unwrap();
        state
            .mutate_security_policy(&mut s, Some(password), |policy| {
                policy.sync_backup_enabled = true;
            })
            .unwrap();
        state
            .mutate_ui_preferences(&mut s, Some(password), |prefs| {
                prefs.market_poll_wifi_only = true;
            })
            .unwrap();
    }

    let metered_res = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        Some(false),
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert!(metered_res.skipped_wifi_only);
    assert_eq!(metered_res.uploaded_to, 0);
    assert!(metered_res.errors.is_empty());

    let manual_res = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert!(!manual_res.skipped_wifi_only);
}

#[tokio::test]
async fn k3_reconcile_restored_wallet_honors_fork_alert_vs_honest_offline() {
    let base_dir = temp_test_dir("k3_restore_reconcile");
    let password = "TestPassword123!";
    let svc = crate::test_support::setup_test_app(
        &base_dir,
        "Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let state = crate::test_support::create_test_state(svc);

    // 1. Honest wallet without fork/stale alert and 0 anchored vouchers clears pending_restore_l2_reconciliation
    {
        let mut s = state.lock_service().unwrap();
        state
            .save_internal_lifecycle(
                &mut s,
                crate::settings::InternalLifecycleState {
                    pending_restore_l2_reconciliation: true,
                    wallet_sync_alert: None,
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();
    }
    let res_honest = super::reconcile_restored_wallet_l2_impl(Some(password.to_string()), &state)
        .await
        .unwrap();
    assert!(!res_honest.pending_restore_l2_reconciliation);

    // 2. Wallet with an active offline (N=0) voucher AND active stale_local_backup alert stays blocked
    {
        let ft_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let mut s = state.lock_service().unwrap();
        let creator = s.with_wallet(|w| w.profile.to_public_profile()).unwrap();
        let v_data = human_money_core::models::voucher::NewVoucherData {
            validity_duration: Some("P1Y".to_string()),
            non_redeemable_test_voucher: false,
            nominal_value: human_money_core::models::voucher::ValueDefinition {
                amount: "50.00".to_string(),
                unit: "FT".to_string(),
                ..Default::default()
            },
            collateral: None,
            creator_profile: creator,
            layer2_anchor: None,
        };
        s.create_new_voucher(&ft_toml, v_data, Some(password))
            .unwrap();
        state
            .save_internal_lifecycle(
                &mut s,
                crate::settings::InternalLifecycleState {
                    pending_restore_l2_reconciliation: true,
                    wallet_sync_alert: Some(crate::settings::WalletSyncAlert {
                        kind: "stale_local_backup".to_string(),
                        mirror_write_url: "https://sync.example/sync.php".to_string(),
                        remote_epoch: Some(1),
                        remote_tx_nonce: Some(5),
                        local_epoch: 1,
                        local_tx_nonce: 2,
                        detected_at: "2026-11-20T00:00:00Z".to_string(),
                        has_signed_ack_proof: true,
                    }),
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();
    }
    let res_alert = super::reconcile_restored_wallet_l2_impl(Some(password.to_string()), &state)
        .await
        .unwrap();
    assert!(
        res_alert.pending_restore_l2_reconciliation,
        "K3: stale_local_backup / fork alert must keep pending_restore_l2_reconciliation locked when offline vouchers exist"
    );
}

#[tokio::test]
async fn s07_f14_container_format_and_sync_input_cap_enforced() {
    // Invariant: Container format parsers reject foreign version tags and probe_sync_backup_impl
    // enforces 1..=MAX_PROBE_SYNC_INPUTS bounds while valid containers/deltas pass.
    let base_dir = temp_test_dir("s07_f14_container_bounds");
    let mut src_svc = setup_test_app(&base_dir.join("src"), "Alice", "pw-src");
    let good_file = base_dir.join("good_format.hmcbackup");
    export_backup_file_impl(
        &good_file,
        BackupProtection::MnemonicOnly,
        Some("pw-src"),
        &mut src_svc,
        HashMap::new(),
    )
    .unwrap();
    let probe_ok = probe_wallet_backup_impl(&good_file.to_string_lossy())
        .expect("valid v1 container format baseline must probe cleanly");
    assert_eq!(probe_ok.protection_mode, "mnemonic");

    let bad_file = base_dir.join("bad_format.hmcbackup");
    let container = PortableBackupContainer {
        container_format: "evil-backup-v99".to_string(),
        created_at: Utc::now().to_rfc3339(),
        manifest_hex: hex::encode(b"{}"),
        blobs_hex: HashMap::new(),
        custom_standards: HashMap::new(),
    };
    fs::write(&bad_file, serde_json::to_vec(&container).unwrap()).unwrap();
    let probe_err = probe_wallet_backup_impl(&bad_file.to_string_lossy())
        .expect_err("S07-F14: probe_wallet_backup_impl must reject unexpected container_format");
    assert!(
        probe_err
            .message
            .contains("Unsupported backup container format: evil-backup-v99"),
        "unexpected container_format error: {}",
        probe_err.message
    );

    let mut base = PortableBackupContainer {
        container_format: "humoco-app-backup-v1".to_string(),
        created_at: Utc::now().to_rfc3339(),
        manifest_hex: hex::encode(b"base"),
        blobs_hex: HashMap::new(),
        custom_standards: HashMap::new(),
    };
    let good_delta = BackupDeltaContainer {
        container_format: "humoco-app-backup-delta-v1".to_string(),
        created_at: Utc::now().to_rfc3339(),
        base_manifest_sha256: sha256_hex(b"base"),
        manifest_hex: hex::encode(b"mid"),
        put_blobs_hex: HashMap::new(),
        delete_blobs: vec![],
        custom_standards: HashMap::new(),
    };
    merge_backup_delta_into_base(&mut base, &good_delta)
        .expect("valid delta container format baseline must merge");

    let bad_delta = BackupDeltaContainer {
        container_format: "evil-delta-v99".to_string(),
        created_at: Utc::now().to_rfc3339(),
        base_manifest_sha256: sha256_hex(b"mid"),
        manifest_hex: hex::encode(b"new"),
        put_blobs_hex: HashMap::new(),
        delete_blobs: vec![],
        custom_standards: HashMap::new(),
    };
    let delta_err = merge_backup_delta_into_base(&mut base, &bad_delta)
        .expect_err("S07-F14: merge_backup_delta_into_base must reject unexpected delta format");
    assert!(
        delta_err.contains("Unsupported backup delta container format: evil-delta-v99"),
        "unexpected delta format error: {}",
        delta_err
    );

    let svc = AppService::new(&base_dir.join("w")).unwrap();
    let state = crate::test_support::create_test_state(svc);
    let empty_err = probe_sync_backup_impl(
        vec!["   ".to_string()],
        PHRASE.to_string(),
        None,
        None,
        Some(MnemonicLanguage::English),
        &base_dir.join("w"),
        &base_dir.join("tmp"),
        &state,
    )
    .await
    .expect_err("empty sync input list must be rejected");
    assert!(
        empty_err
            .message
            .contains("Sync-Dienst-Adresse darf nicht leer sein"),
        "unexpected empty sync input error: {}",
        empty_err.message
    );

    let too_many: Vec<String> = (0..33)
        .map(|i| format!("https://sync{}.example.org/sync.php", i))
        .collect();
    let cap_err = probe_sync_backup_impl(
        too_many,
        PHRASE.to_string(),
        None,
        None,
        Some(MnemonicLanguage::English),
        &base_dir.join("w"),
        &base_dir.join("tmp"),
        &state,
    )
    .await
    .expect_err("S07-F14: probe_sync_backup_impl must reject > 32 sync inputs");
    assert!(
        cap_err
            .message
            .contains("Too many sync inputs (33 > maximum 32)"),
        "unexpected cap error: {}",
        cap_err.message
    );
}

#[tokio::test]
async fn s07_f3_reset_password_via_backup_preserves_fork_lock_reason() {
    let base_dir = temp_test_dir("s07_f3_preserve_fork_lock");
    let wallet_dir = base_dir.join("wallet");
    let tmp_restore_dir = base_dir.join("tmp_restore");
    let old_pass = "OldPassword123!";
    let new_pass = "NewPassword456!";
    let bkp_pass = "BackupSecret789!";

    // Create profile WITHOUT retaining mnemonic so reset_local_password_via_backup takes the direct recovery branch
    let mut service = AppService::new(&wallet_dir).unwrap();
    service
        .create_profile(
            "Alice",
            PHRASE,
            None,
            None,
            old_pass,
            human_money_core::MnemonicLanguage::English,
            "device-f3".into(),
            false,
        )
        .unwrap();
    let folder_name = service.list_profiles().unwrap()[0].folder_name.clone();
    let bkp_file = base_dir.join("alice.hmcbackup");
    super::export_backup_file_impl(
        &bkp_file,
        BackupProtection::CustomBackupPassword(bkp_pass),
        Some(old_pass),
        &mut service,
        HashMap::new(),
    )
    .unwrap();
    service
        .persist_seal_lock_reason(
            human_money_core::models::seal::SealLockReason::ForkDetected,
            Some(old_pass),
        )
        .unwrap();
    service.logout();

    let state = crate::test_support::create_test_state(service);
    let reset_err = super::reset_local_password_via_backup_impl(
        &folder_name,
        &bkp_file,
        "password",
        Some(bkp_pass),
        None,
        None,
        None,
        new_pass,
        "device-f3".to_string(),
        &wallet_dir,
        &tmp_restore_dir,
        &state,
        None,
    )
    .unwrap_err();
    assert_eq!(
        reset_err.code, "error.security.walletLockedDueToFork",
        "S07-F3: fork-locked wallet must NOT silently unlock via backup-password reset"
    );
    assert!(
        !state.lock_service().unwrap().is_wallet_unlocked(),
        "S07-F3: failed login on fork-locked wallet must not leave an unlocked session"
    );

    let disk_storage = FileStorage::new(wallet_dir.join(&folder_name));
    let disk_seal = disk_storage
        .load_seal(&AuthMethod::Password(new_pass))
        .unwrap()
        .expect("persisted seal must exist under new password");
    assert_eq!(
        disk_seal.lock_reason,
        Some(human_money_core::models::seal::SealLockReason::ForkDetected),
        "S07-F3: reset_local_password_via_backup must preserve existing ForkDetected seal lock on disk"
    );
}

#[tokio::test]
async fn s07_f7_f8_restore_rejects_metadata_spoof_replace_and_preserves_unrelated_tmp_restore_file()
{
    let base_dir = temp_test_dir("s07_f7_f8_restore_safety");
    let wallet_dir = base_dir.join("wallet");
    let standards_dir = base_dir.join("standards");
    let tmp_restore_dir = base_dir.join("tmp_restore");
    let pass = "TestPassword123!";

    let mut service = AppService::new(&wallet_dir).unwrap();
    service
        .create_profile(
            "Bob Victim",
            crate::test_support::BOB_PHRASE,
            None,
            None,
            pass,
            human_money_core::MnemonicLanguage::English,
            "inst-bob".into(),
            true,
        )
        .unwrap();
    let bob_folder = service.list_profiles().unwrap()[0].folder_name.clone();
    service.logout();

    // Create Alice in another dir and export a backup into a user folder whose name contains "tmp_restore"
    let alice_src_dir = base_dir.join("alice_src");
    let mut alice_svc = setup_test_app(&alice_src_dir, "Alice", pass);
    let alice_uid = alice_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let user_dir = base_dir.join("my_tmp_restore_backups");
    fs::create_dir_all(&user_dir).unwrap();
    let user_bkp = user_dir.join("alice.hmcbackup");
    super::export_backup_file_impl(
        &user_bkp,
        BackupProtection::MnemonicOnly,
        Some(pass),
        &mut alice_svc,
        HashMap::new(),
    )
    .unwrap();

    // Poison profile_metadata.json so Bob's folder claims Alice's user_id!
    record_profile_metadata_in_dir(&wallet_dir, &bob_folder, Some(alice_uid)).unwrap();

    let state = crate::test_support::create_test_state(service);

    // 1. S07-F7: Attempting to replace Bob's folder using Alice's backup MUST fail (not silently keep or delete Bob!)
    let replace_err = super::finalize_restored_wallet_and_reconcile_l2(
        &user_bkp,
        BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        },
        pass,
        "inst-f7".to_string(),
        Some(&bob_folder),
        &wallet_dir,
        &standards_dir,
        &tmp_restore_dir,
        &state,
        None,
    )
    .await
    .expect_err("S07-F7: replacing another user's folder via poisoned profile_metadata.json must fail closed");
    assert!(
        replace_err
            .message
            .contains("cryptographic ownership could not be verified"),
        "unexpected metadata spoof replace error: {}",
        replace_err.message
    );
    assert!(
        wallet_dir.join(&bob_folder).exists(),
        "S07-F7: victim folder must NOT be deleted"
    );

    // 2. S07-F8: Restoring Alice cleanly must NOT delete user_bkp outside tmp_restore_dir
    let ok_res = super::finalize_restored_wallet_and_reconcile_l2(
        &user_bkp,
        BackupRestoreCredentials::Mnemonic {
            phrase: PHRASE,
            passphrase: None,
            prefix: Some("pc"),
            language: human_money_core::MnemonicLanguage::English,
        },
        pass,
        "inst-f8".to_string(),
        None,
        &wallet_dir,
        &standards_dir,
        &tmp_restore_dir,
        &state,
        None,
    )
    .await;
    assert!(ok_res.is_ok());
    assert!(
        user_bkp.exists(),
        "S07-F8: backup file outside tmp_restore_dir whose path contains 'tmp_restore' must NOT be deleted"
    );
}

#[tokio::test]
async fn s07_f10_f17_uncached_ack_rejected_no_tofu_and_timeout_zero_backup_status() {
    use crate::test_support::{
        create_test_state, setup_test_app as setup_sync_test_app, MockSyncServer, ALICE_PHRASE,
    };

    let base_dir = temp_test_dir("s07_f10_f17_no_tofu_timeout_zero");
    let wallet_dir = base_dir.join("wallet");
    let password = "TestPassword123!";
    let service = setup_sync_test_app(&wallet_dir, "Alice", ALICE_PHRASE, password);
    let state = create_test_state(service);

    // Configure an uncached mirror (no verified l2_probe_cache entry) -> SlotAckReceipt must NOT be accepted via TOFU!
    let (_admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&[0x77u8; 32], 0);
    let drop_pub = hex::encode(admin_vk.to_bytes());
    let server = MockSyncServer::start("no_tofu_bkp".to_string(), drop_pub.clone()).await;
    let stealth_pk = {
        let s = state.service.lock().unwrap();
        let sk = s
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .unwrap();
        humoco_market::derive_member_stealth_pubkey(&sk, &drop_pub).unwrap()
    };
    server.members.lock().unwrap().push(stealth_pk);

    let write_url = format!("{}/sync.php", server.base_url);
    let read_url = format!("{}/no_tofu_bkp", server.base_url);
    {
        let mut s = state.service.lock().unwrap();
        state
            .mutate_security_policy(&mut s, Some(password), |policy| {
                policy.sync_backup_enabled = true;
                policy.market_selected_syncs = vec![crate::models::FrontendSyncDescriptor {
                    write_url: write_url.clone(),
                    read_url: read_url.clone(),
                    drop_pub: drop_pub.clone(),
                    ..Default::default()
                }];
                // Also add a revoked public sync that must be ignored
                policy.public_syncs = vec![crate::settings::StoredPublicSync {
                    write_url: "https://revoked.example.org/sync.php".to_string(),
                    read_url: "https://revoked.example.org/data".to_string(),
                    drop_pub: "99".repeat(32),
                    register_url: None,
                    access_pubkey: "88".repeat(32),
                    enabled: true,
                    authorized_channels: vec![],
                    status: "revoked".to_string(),
                    sunset_at: None,
                    revocation_reason: None,
                    replacement: None,
                }];
            })
            .unwrap();
    }

    let res = super::sync_backup_with_standards_impl(
        Some(password.to_string()),
        None,
        HashMap::new(),
        &state,
    )
    .await
    .unwrap();
    assert_eq!(res.uploaded_to, 1);
    assert_eq!(res.authorized_targets, 1);

    let stored_sync_state: crate::state::StoredBackupSyncState = {
        let mut s = state.service.lock().unwrap();
        crate::state::load_local_sidecar_json_or_default(
            &mut s,
            crate::state::BACKUP_SYNC_STATE_KEY,
            Some(password),
        )
        .unwrap()
    };
    let norm_url = normalize_sync_url(&write_url);
    let mirror = stored_sync_state
        .mirrors_by_write_url
        .get(&norm_url)
        .expect("mirror state must be keyed by normalized write_url");
    assert!(
        mirror.last_slot_ack.is_none(),
        "S07-F10: uncached mirror SlotAckReceipt must NOT be accepted via TOFU"
    );

    // S07-F17: get_backup_status_impl must work under session_timeout_seconds == 0 with cached own_sync_descriptor
    {
        let mut s = state.service.lock().unwrap();
        state
            .mutate_security_policy(&mut s, Some(password), |policy| {
                policy.session_timeout_seconds = 0;
                policy.market_selected_syncs.clear();
                policy.public_syncs.clear();
            })
            .unwrap();
        let own_admin = crate::state::StoredSyncAdmin {
            descriptor: humoco_market::DropDescriptor {
                write_url: write_url.clone(),
                read_url: read_url.clone(),
                slot_id: None,
                drop_pub: drop_pub.clone(),
            },
            sync_index: 0,
            data_dir: "no_tofu_bkp".to_string(),
            max_members: 50,
            max_total_storage_mb: 100,
            max_user_storage_mb: 10,
            policies: Default::default(),
            allow: vec![],
            last_members_hash: Default::default(),
            revocation: None,
        };
        crate::state::save_encrypted_json(
            &mut s,
            crate::state::MARKET_SYNC_ADMIN_KEY,
            &own_admin,
            Some(password),
        )
        .unwrap();
    }

    let mut reloaded_svc = AppService::new(&wallet_dir).unwrap();
    let folder = reloaded_svc.list_profiles().unwrap()[0].folder_name.clone();
    reloaded_svc
        .login(&folder, password, false, "device-drops-test".to_string())
        .unwrap();
    let reloaded_state = create_test_state(AppService::new(&wallet_dir).unwrap());
    {
        reloaded_state
            .initialize_profile_session(&mut reloaded_svc, password)
            .unwrap();
        *reloaded_state.service.lock().unwrap() = reloaded_svc;
    }

    let status = super::get_backup_status_impl(&reloaded_state).unwrap();
    assert!(
        status.has_authorized_sync_backup_target,
        "S07-F17: get_backup_status_impl must read cached own_sync_descriptor under timeout == 0"
    );
}

#[tokio::test]
async fn s07_f1_reconcile_restored_wallet_blocks_clear_when_fork_alert_with_offline_and_anchored_vouchers(
) {
    use crate::test_support::{create_test_state, setup_test_app, ALICE_PHRASE};
    use human_money_core::humoco_proto::manifest::{
        AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry,
    };
    use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};

    let base_dir = temp_test_dir("s07_f1_fork_with_anchored_and_offline");
    let password = "TestPassword123!";
    let svc = setup_test_app(&base_dir, "Alice", ALICE_PHRASE, password);
    let state = create_test_state(svc);

    // Start a mock server acting as a healthy L2 static lock server for the anchored voucher
    let (_admin_sk, admin_vk, _) = humoco_market::derive_drop_admin_keypair(&[0x55u8; 32], 0);
    let drop_pub = hex::encode(admin_vk.to_bytes());
    let server = crate::test_support::MockSyncServer::start("f1_l2".to_string(), drop_pub).await;
    let lock_pub_bytes =
        human_money_core::humoco_proto::decode_hex_fixed::<32>(&server.lock_pubkey).unwrap();

    let manifest = AnchorManifest {
        revision: 1,
        threshold: 1,
        servers: vec![ServerEntry {
            lock_pub: lock_pub_bytes,
            admin_pub: admin_vk.to_bytes(),
            write_url: format!("{}/sync.php", server.base_url),
            read_url: format!("{}/f1_l2", server.base_url),
            nominated_by: NominatedBy::Creator,
        }],
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub: [0x22u8; 32],
            guarantor_mig_pubs: vec![],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: 20294,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();

    {
        let ft_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let mut s = state.lock_service().unwrap();
        let creator = s.with_wallet(|w| w.profile.to_public_profile()).unwrap();

        // 1. Active offline voucher (N=0)
        s.create_new_voucher(
            &ft_toml,
            NewVoucherData {
                validity_duration: Some("P1Y".to_string()),
                non_redeemable_test_voucher: false,
                nominal_value: ValueDefinition {
                    amount: "20.00".to_string(),
                    unit: "FT".to_string(),
                    ..Default::default()
                },
                collateral: None,
                creator_profile: creator.clone(),
                layer2_anchor: None,
            },
            Some(password),
        )
        .unwrap();

        // 2. Active anchored voucher (N=1) with manifest stored in wallet
        s.update_l2_anchor_manifest(manifest, Some(password))
            .unwrap();
        s.create_new_voucher(
            &ft_toml,
            NewVoucherData {
                validity_duration: Some("P1Y".to_string()),
                non_redeemable_test_voucher: false,
                nominal_value: ValueDefinition {
                    amount: "30.00".to_string(),
                    unit: "FT".to_string(),
                    ..Default::default()
                },
                collateral: None,
                creator_profile: creator,
                layer2_anchor: Some(anchor_id),
            },
            Some(password),
        )
        .unwrap();

        // 3. Set active fork_detected alert + ForkDetected seal lock + pending_restore_l2_reconciliation
        s.persist_seal_lock_reason(
            human_money_core::models::seal::SealLockReason::ForkDetected,
            Some(password),
        )
        .unwrap();
        state
            .save_internal_lifecycle(
                &mut s,
                crate::settings::InternalLifecycleState {
                    pending_restore_l2_reconciliation: true,
                    wallet_sync_alert: Some(crate::settings::WalletSyncAlert {
                        kind: "fork_detected".to_string(),
                        mirror_write_url: "https://sync.example.org/sync.php".to_string(),
                        remote_epoch: Some(1),
                        remote_tx_nonce: Some(4),
                        local_epoch: 1,
                        local_tx_nonce: 2,
                        detected_at: "2026-11-21T00:00:00Z".to_string(),
                        has_signed_ack_proof: true,
                    }),
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();
    }

    let res = super::reconcile_restored_wallet_l2_impl(Some(password.to_string()), &state)
        .await
        .unwrap();
    assert_eq!(res.active_voucher_queries_count, 1);
    assert!(
        res.pending_restore_l2_reconciliation,
        "S07-F1: fork alert + active offline voucher must keep pending_restore_l2_reconciliation locked even when anchored voucher resolves"
    );
    let seal_lock = state
        .lock_service()
        .unwrap()
        .get_seal_lock_reason(Some(password))
        .unwrap();
    assert_eq!(
        seal_lock,
        Some(human_money_core::models::seal::SealLockReason::ForkDetected),
        "S07-F1: ForkDetected seal lock must NOT be cleared while offline voucher is exposed"
    );
}
