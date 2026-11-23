use std::collections::BTreeMap;
use std::time::Duration;

use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    decode_hex_fixed,
    manifest::{AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry},
};
use human_money_core::models::layer2_api::{LockEntry, VoucherReconciliationResult};
use human_money_core::services::l2_gateway;

use crate::{
    models::FrontendVoucherL2StatusDto,
    services::common::{resolve_verified_server_locator, upsert_verified_lock_probe},
    settings::{InternalLifecycleState, SecurityPolicy},
    sync_client::{self, FrontendLockServerProbeResult},
    AppState,
};

use super::committee::*;
use super::overview::*;
use super::sources::*;
use super::static_reconcile::*;
use ed25519_dalek::{Signer, SigningKey};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[test]
fn test_frontend_voucher_l2_status_dto_serialization() {
    let dto = FrontendVoucherL2StatusDto {
        status: "spent".to_string(),
        archived: true,
        successor_t_id: Some("succ123".to_string()),
        reduced_guarantor_protection: true,
        manifest_revision: Some(2),
    };
    let json = serde_json::to_string(&dto).unwrap();
    assert!(json.contains("\"status\":\"spent\""));
    assert!(json.contains("\"archived\":true"));
    assert!(json.contains("\"successorTId\":\"succ123\""));
    assert!(json.contains("\"reducedGuarantorProtection\":true"));
    assert!(json.contains("\"manifestRevision\":2"));

    let unspent_dto = FrontendVoucherL2StatusDto {
        status: "unspent".to_string(),
        archived: false,
        successor_t_id: None,
        reduced_guarantor_protection: false,
        manifest_revision: None,
    };
    let unspent_json = serde_json::to_string(&unspent_dto).unwrap();
    assert!(unspent_json.contains("\"status\":\"unspent\""));
    assert!(unspent_json.contains("\"archived\":false"));
    assert!(!unspent_json.contains("successorTId"));
}

#[test]
fn k1_reconcile_requires_quorum_404_and_survives_corrupt_candidate_entry() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::encode_quarter;
    use human_money_core::humoco_proto::manifest::GenesisTicket;
    use human_money_core::models::profile::PublicProfile;
    use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
    use human_money_core::services::l2_gateway;
    use human_money_core::test_utils;

    let dir = crate::test_support::temp_test_dir("k1_quorum_404");
    let pw = "pw-k1-test";
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let sk1 = SigningKey::from_bytes(&[11u8; 32]);
    let sk2 = SigningKey::from_bytes(&[12u8; 32]);
    let sk3 = SigningKey::from_bytes(&[13u8; 32]);
    let anchor_nonce = [41u8; 32];
    let creator_mig_pub = svc
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2, // N=3, Q=2 -> quorum 404 requires N - Q + 1 = 2 servers returning 404
        servers: vec![&sk1, &sk2, &sk3]
            .into_iter()
            .enumerate()
            .map(|(idx, sk)| ServerEntry {
                write_url: format!("http://127.0.0.1:1808{}/sync.php", idx + 1),
                read_url: format!("http://127.0.0.1:1808{}/d", idx + 1),
                admin_pub: sk.verifying_key().to_bytes(),
                lock_pub: sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            })
            .collect(),
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs: vec![SigningKey::from_bytes(&[99u8; 32])
                .verifying_key()
                .to_bytes()],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let tickets: Vec<GenesisTicket> = [&sk1, &sk2, &sk3]
        .into_iter()
        .map(|sk| {
            let mut t = GenesisTicket {
                anchor_id,
                retain_until_quarter: retain_q,
                issued_at: 1_760_000_000,
                lock_pub: sk.verifying_key().to_bytes(),
                sig: [0u8; 64],
            };
            t.sig = sk.sign(&t.digest()).to_bytes();
            t
        })
        .collect();
    svc.register_l2_anchor_template_with_nonce(manifest, tickets, Some(&anchor_nonce), Some(pw))
        .unwrap();

    let my_id = svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();
    svc.create_new_voucher(
        &std_toml,
        NewVoucherData {
            creator_profile: PublicProfile {
                id: Some(my_id),
                ..Default::default()
            },
            nominal_value: ValueDefinition {
                amount: "50".into(),
                ..Default::default()
            },
            layer2_anchor: Some(anchor_id),
            ..Default::default()
        },
        Some(pw),
    )
    .unwrap();
    let local_id = svc
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();

    // 1. Only 1 of 3 servers answered 404 (2 unreachable) -> MUST stay Inconclusive
    let insufficient_404 = StaticSlotProbeResult {
        candidate_entries: vec![],
        responded_404: 1,
        total_servers: 3,
        threshold: 2,
        any_server_answered: true,
    };
    let out1 = reconcile_static_probe_on_service(
        &mut svc,
        &local_id,
        &insufficient_404,
        DEFAULT_LOCK_POW_BASE_BITS,
        Some(pw),
    )
    .unwrap();
    assert!(matches!(
        out1,
        StaticVoucherReconcileOutcome::Inconclusive {
            any_reachable: true,
            ..
        }
    ));

    // 2. Corrupt candidate entry + 1 of 3 404 -> ignores corrupt entry and stays Inconclusive
    let bogus_entry = LockEntry {
        slot: [9u8; 32],
        parent_slot: [0u8; 32],
        output_branch: 0,
        depth: 0,
        child_lock: [8u8; 32],
        sender_pubkey: [7u8; 32],
        receiver_pubkey_hash: [6u8; 32],
        change_pubkey_hash: None,
        causal_sig: [5u8; 64],
        root_mig_sig: Some([4u8; 64]),
        encrypted_payload: [3u8; 265],
        pow_nonce: 0,
    };
    let corrupt_plus_one_404 = StaticSlotProbeResult {
        candidate_entries: vec![bogus_entry.clone()],
        responded_404: 1,
        total_servers: 3,
        threshold: 2,
        any_server_answered: true,
    };
    let out2 = reconcile_static_probe_on_service(
        &mut svc,
        &local_id,
        &corrupt_plus_one_404,
        DEFAULT_LOCK_POW_BASE_BITS,
        Some(pw),
    )
    .unwrap();
    assert!(matches!(
        out2,
        StaticVoucherReconcileOutcome::Inconclusive {
            any_reachable: true,
            ..
        }
    ));

    // 3. Quorum-intersecting 404 (2 of 3 == N - Q + 1) resolves UnspentSafe even past corrupt entry
    let quorum_404 = StaticSlotProbeResult {
        candidate_entries: vec![bogus_entry],
        responded_404: 2,
        total_servers: 3,
        threshold: 2,
        any_server_answered: true,
    };
    let out3 = reconcile_static_probe_on_service(
        &mut svc,
        &local_id,
        &quorum_404,
        DEFAULT_LOCK_POW_BASE_BITS,
        Some(pw),
    )
    .unwrap();
    assert!(matches!(
        out3,
        StaticVoucherReconcileOutcome::Resolved(res)
            if matches!(*res, VoucherReconciliationResult::UnspentSafe)
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

fn build_signed_probe_info_parts(
    admin_sk: &ed25519_dalek::SigningKey,
    lock_sk: &ed25519_dalek::SigningKey,
    write_url: &str,
    read_url: &str,
    max_retention_quarter: u16,
) -> (
    sync_client::SyncLockDelegationDto,
    sync_client::SyncLockSelftestReportDto,
) {
    use ed25519_dalek::Signer;
    use sha2::Digest;

    let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
    let issued_at = 1_760_000_000u64;
    let del_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_hex, &lock_hex, write_url, read_url, issued_at,
    );
    let del = sync_client::SyncLockDelegationDto {
        admin_pub: admin_hex,
        lock_pub: lock_hex.clone(),
        write_url: write_url.to_string(),
        read_url: read_url.to_string(),
        issued_at,
        sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
    };

    let checks = sync_client::SyncLockSelftestChecksDto {
        ext_sodium: "pass".into(),
        sha3_256: "pass".into(),
        atomic_link_eexist: "pass".into(),
        fsync: "pass".into(),
        ignore_user_abort: "pass".into(),
        storage_writable: "pass".into(),
        inode_budget: "pass".into(),
        fastcgi_finish_request: "pass".into(),
    };
    let line = format!(
        "{},{},{},{},{},{},{},{},{}",
        checks.ext_sodium,
        checks.sha3_256,
        checks.atomic_link_eexist,
        checks.fsync,
        checks.ignore_user_abort,
        checks.storage_writable,
        checks.inode_budget,
        checks.fastcgi_finish_request,
        50_000u64
    );
    let digest = hex::encode(sha2::Sha256::digest(line.as_bytes()));
    let st_msg = format!(
        "HUMOCO-LOCK-SELFTEST-V1\nrust\n{}\n{}\n1\n{}",
        issued_at, lock_hex, digest
    );
    let selftest = sync_client::SyncLockSelftestReportDto {
        runtime: "rust".into(),
        checked_at: issued_at,
        lock_pub: lock_hex,
        php_version: None,
        checks,
        free_inodes: 50_000,
        all_passed: true,
        sig: hex::encode(lock_sk.sign(st_msg.as_bytes()).to_bytes()),
    };
    let _ = max_retention_quarter;
    (del, selftest)
}

#[tokio::test]
async fn test_auto_committee_retry_offline_cache_and_fallback_n0() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::encode_quarter;
    use human_money_core::humoco_proto::manifest::GenesisTicket;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let dir = crate::test_support::temp_test_dir("auto_committee_fallback");
    let pw = "pw-auto-committee";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let admin1_sk = SigningKey::from_bytes(&[91u8; 32]);
    let lock1_sk = SigningKey::from_bytes(&[92u8; 32]);
    let admin2_sk = SigningKey::from_bytes(&[93u8; 32]);
    let lock2_sk = SigningKey::from_bytes(&[94u8; 32]);

    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();

    let w1 = format!("http://127.0.0.1:{}/sync.php", p1);
    let r1 = format!("http://127.0.0.1:{}/d", p1);
    let w2 = format!("http://127.0.0.1:{}/sync.php", p2);
    let r2 = format!("http://127.0.0.1:{}/d", p2);

    let retain_q = encode_quarter(2035, 4).unwrap();
    let stop = Arc::new(AtomicBool::new(false));

    let spawn_srv = |listener: tokio::net::TcpListener,
                     admin_sk: SigningKey,
                     lock_sk: SigningKey,
                     wurl: String,
                     rurl: String,
                     fail_ticket: bool,
                     stop_flag: Arc<AtomicBool>| {
        let (del, selftest) =
            build_signed_probe_info_parts(&admin_sk, &lock_sk, &wurl, &rurl, retain_q);
        let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
        let lock_pub = lock_sk.verifying_key().to_bytes();
        let lock_hex = hex::encode(lock_pub);
        tokio::spawn(async move {
            loop {
                let accept =
                    tokio::time::timeout(Duration::from_millis(40), listener.accept()).await;
                let Ok(Ok((mut stream, _))) = accept else {
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    continue;
                };
                let mut buf = [0u8; 16_384];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
                let path = head
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("");

                let (status, resp_body) = if path.contains(".lock_key.php") {
                    ("404 Not Found", String::new())
                } else if path.contains("info") {
                    (
                        "200 OK",
                        serde_json::json!({
                            "drop_pub": admin_hex,
                            "read_url": rurl,
                            "lock": {
                                "lock_pub": lock_hex,
                                "max_retention_quarter": retain_q,
                                "pow_base_bits": 16,
                                "allow_pow_tickets": false,
                                "ticket_pow_bits": 16,
                                "used_inodes": 10,
                                "max_inodes": 50_000,
                                "inode_warning": false,
                                "delegation": del,
                                "selftest": selftest,
                            }
                        })
                        .to_string(),
                    )
                } else if path.contains("lock_selftest") {
                    ("200 OK", serde_json::to_string(&selftest).unwrap())
                } else if path.contains("lock_ticket") {
                    if fail_ticket {
                        (
                            "500 Internal Server Error",
                            r#"{"error":"ticket_boom"}"#.to_string(),
                        )
                    } else {
                        let req: serde_json::Value = serde_json::from_str(body).unwrap();
                        let req_aid =
                            decode_hex_fixed::<32>(req["anchor_id"].as_str().unwrap()).unwrap();
                        let req_q = req["retain_until_quarter"].as_u64().unwrap() as u16;
                        let mut ticket = GenesisTicket {
                            anchor_id: req_aid,
                            retain_until_quarter: req_q,
                            issued_at: 1_760_000_000,
                            lock_pub,
                            sig: [0u8; 64],
                        };
                        ticket.sig = lock_sk.sign(&ticket.digest()).to_bytes();
                        (
                            "200 OK",
                            serde_json::json!({ "ticket": ticket }).to_string(),
                        )
                    }
                } else if path.contains("?lock") {
                    let req: serde_json::Value = serde_json::from_str(body).unwrap();
                    let nonce = req["client_nonce"]
                        .as_str()
                        .unwrap()
                        .trim()
                        .to_ascii_lowercase();
                    let server_time = 1_760_000_050u64;
                    let msg = format!(
                        "HUMOCO-LOCK-PROBE-V1\n{}\n{}\n{}",
                        lock_hex, nonce, server_time
                    );
                    let resp = sync_client::SyncLockProbeResponseDto {
                        probe_ok: true,
                        lock_pub: lock_hex.clone(),
                        client_nonce: nonce,
                        server_time,
                        sig: hex::encode(lock_sk.sign(msg.as_bytes()).to_bytes()),
                    };
                    ("200 OK", serde_json::to_string(&resp).unwrap())
                } else {
                    ("404 Not Found", String::new())
                };
                let http = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    status,
                    resp_body.len(),
                    resp_body
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        })
    };

    let h1 = spawn_srv(
        l1,
        admin1_sk.clone(),
        lock1_sk.clone(),
        w1.clone(),
        r1.clone(),
        true,
        Arc::clone(&stop),
    );
    let h2 = spawn_srv(
        l2,
        admin2_sk.clone(),
        lock2_sk.clone(),
        w2.clone(),
        r2.clone(),
        false,
        Arc::clone(&stop),
    );

    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        let make_pub =
            |w: &str, r: &str, admin_sk: &SigningKey| crate::settings::StoredPublicSync {
                write_url: w.into(),
                read_url: r.into(),
                drop_pub: hex::encode(admin_sk.verifying_key().as_bytes()),
                register_url: None,
                access_pubkey: "00".repeat(32),
                enabled: true,
                authorized_channels: vec!["lock".into()],
                status: "active".into(),
                sunset_at: None,
                revocation_reason: None,
                replacement: None,
            };
        state
            .mutate_security_policy(&mut s, Some(pw), |p| {
                p.public_syncs = vec![
                    make_pub(&w1, &r1, &admin1_sk),
                    make_pub(&w2, &r2, &admin2_sk),
                ];
            })
            .unwrap();
    }
    let session = {
        let s = state.lock_service().unwrap();
        state.session_token_for_service(&s).unwrap()
    };

    // 1. S1 fails ticket with 500 -> single retry succeeds cleanly with S2 (N = 1)
    let deletable_ms = 1_760_000_000_000u64;
    let aid_retry =
        assemble_auto_committee_for_voucher(&state, &session, deletable_ms, false, Some(pw))
            .await
            .unwrap()
            .expect("retry with surviving server should register N=1 committee")
            .anchor_id;
    let saved_manifest = {
        let s = state.lock_service().unwrap();
        s.with_wallet(|w| {
            w.profile
                .l2_anchor_manifests
                .get(&hex::encode(aid_retry))
                .cloned()
        })
        .unwrap()
        .unwrap()
    };
    assert_eq!(saved_manifest.servers.len(), 1);
    assert_eq!(saved_manifest.threshold, 1);
    assert_eq!(
        saved_manifest.servers[0].lock_pub,
        lock2_sk.verifying_key().to_bytes()
    );

    // 2. Both servers unreachable + cache invalidated -> reuses cached offline anchor!
    stop.store(true, Ordering::Relaxed);
    let _ = h1.await;
    let _ = h2.await;
    {
        let mut s = state.lock_service().unwrap();
        state
            .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                lc.l2_probe_cache.clear();
            })
            .unwrap();
    }
    let aid_offline_cached =
        assemble_auto_committee_for_voucher(&state, &session, deletable_ms, false, Some(pw))
            .await
            .unwrap()
            .expect("offline fallback must reuse cached rev1 creator anchor")
            .anchor_id;
    assert_eq!(aid_offline_cached, aid_retry);

    // 3. Offline with no matching cache (future quarter 2045 beyond 2035) -> clean N = 0 (Ok(None))
    let far_future_ms = 2_380_000_000_000u64;
    let aid_none =
        assemble_auto_committee_for_voucher(&state, &session, far_future_ms, false, Some(pw))
            .await
            .unwrap();
    assert!(aid_none.is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_k1_and_h9_unverified_404_rejected_and_negative_health_overwrites_stale() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let dir = crate::test_support::temp_test_dir("k1_h9_locator_health");
    let pw = "StrongPassword123!";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    let admin_sk = SigningKey::from_bytes(&[61u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[62u8; 32]);
    let admin_pub: [u8; 32] = admin_sk.verifying_key().to_bytes();
    let lock_pub: [u8; 32] = lock_sk.verifying_key().to_bytes();
    let admin_hex = hex::encode(admin_pub);
    let lock_hex = hex::encode(lock_pub);

    // Unverified fake read server returning 404 (not in probe_cache, ?info fails)
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_bg = Arc::clone(&stop);
    let srv = tokio::spawn(async move {
        while !stop_bg.load(Ordering::Relaxed) {
            if let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(Duration::from_millis(50), listener.accept()).await
            {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf).await;
                let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        }
    });

    let evil_read_url = format!("http://127.0.0.1:{}/d", port);
    let dead_write_url = format!("http://127.0.0.1:{}/sync.php", port);
    let srv_entry = ServerEntry {
        admin_pub,
        lock_pub,
        write_url: dead_write_url.clone(),
        read_url: evil_read_url.clone(),
        nominated_by: NominatedBy::Creator,
    };

    let mut cache = BTreeMap::new();
    let mut learned = Vec::new();
    // K1: without a verified 5-field delegation in cache or from ?info, resolve_verified_server_locator MUST be None
    assert!(resolve_verified_server_locator(&srv_entry, &cache).is_none());
    let client = sync_client::create_sync_probe_client();
    assert!(
        verify_and_cache_static_read_locator(&client, &srv_entry, &mut cache, &mut learned)
            .await
            .is_none(),
        "K1: 404 on unverified ?info/read_url must never populate verified locator"
    );
    assert!(learned.is_empty());

    // H9 part A: newer negative health probe overwrites stale healthy passed=true while preserving delegation & capability metadata
    let del_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_hex,
        &lock_hex,
        &dead_write_url,
        &evil_read_url,
        1_760_000_000,
    );
    let del = sync_client::SyncLockDelegationDto {
        admin_pub: admin_hex.clone(),
        lock_pub: lock_hex.clone(),
        write_url: dead_write_url.clone(),
        read_url: evil_read_url.clone(),
        issued_at: 1_760_000_000,
        sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
    };
    let healthy = FrontendLockServerProbeResult {
        write_url: dead_write_url.clone(),
        read_url: evil_read_url.clone(),
        drop_pub: admin_hex.clone(),
        admin_pub: admin_hex.clone(),
        lock_pub: lock_hex.clone(),
        delegation_valid: true,
        waf_probe_passed: true,
        selftest_passed: true,
        key_leak_safe: true,
        passed: true,
        latency_ms: 10,
        max_retention_quarter: 8200,
        pow_base_bits: 12,
        allow_pow_tickets: true,
        ticket_pow_bits: 16,
        inode_warning: false,
        used_inodes: 1,
        max_inodes: 1000,
        probed_at_secs: 1_760_000_010,
        delegation: Some(del),
        error: None,
    };
    assert!(upsert_verified_lock_probe(&mut cache, healthy));

    let failed_later = FrontendLockServerProbeResult {
        write_url: dead_write_url.clone(),
        read_url: String::new(),
        drop_pub: String::new(),
        admin_pub: String::new(),
        lock_pub: lock_hex.clone(),
        delegation_valid: false,
        waf_probe_passed: false,
        selftest_passed: false,
        key_leak_safe: false,
        passed: false,
        latency_ms: 500,
        max_retention_quarter: 0,
        pow_base_bits: 0,
        allow_pow_tickets: false,
        ticket_pow_bits: 0,
        inode_warning: false,
        used_inodes: 0,
        max_inodes: 0,
        probed_at_secs: 1_760_000_100,
        delegation: None,
        error: Some("connection refused".into()),
    };
    assert!(upsert_verified_lock_probe(&mut cache, failed_later));
    let cached = cache.get(&lock_hex).unwrap();
    assert!(
        !cached.passed,
        "H9: newer failed probe must mark passed=false"
    );
    assert!(
        cached.delegation_valid,
        "H9: verified delegation must remain intact"
    );
    assert_eq!(cached.pow_base_bits, 12);
    assert_eq!(cached.error.as_deref(), Some("connection refused"));

    // H9 part B: build_lock_protection_overview_from_service excludes ServerOffenderList servers
    let mut lc = InternalLifecycleState {
        l2_probe_cache: BTreeMap::from([(
            lock_hex.clone(),
            FrontendLockServerProbeResult {
                passed: true,
                delegation_valid: true,
                error: None,
                ..cached.clone()
            },
        )]),
        ..Default::default()
    };
    let mut offenders = humoco_wot::ServerOffenderList::new();
    offenders.revoked_admin_pubs.insert(admin_pub);
    crate::l2_lifecycle::save_server_offender_list(&mut svc, &offenders, Some(pw)).unwrap();
    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        let policy = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
        lc.pending_restore_l2_reconciliation = false;
        let overview = build_lock_protection_overview_from_service(
            &state,
            &mut s,
            &policy,
            &lc,
            None,
            None,
            Some(pw),
        )
        .unwrap();
        assert_eq!(
            overview.ready_trusted_count, 0,
            "H9: revoked server must not count as ready"
        );
    }

    stop.store(true, Ordering::Relaxed);
    let _ = srv.await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_s05_f8_get_lock_protection_overview_succeeds_with_zero_timeout() {
    let dir = crate::test_support::temp_test_dir("s05_f8_zero_timeout");
    let pw = "pw-s05-f8";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();
    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        let mut sp = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
        sp.session_timeout_seconds = 0;
        state.save_security_policy(&mut s, sp, Some(pw)).unwrap();
        s.lock_session();
        state.initialize_profile_session(&mut s, pw).unwrap();
    }
    let overview = get_lock_protection_overview_impl(None, None, &state)
        .expect("S05-F8: get_lock_protection_overview_impl must succeed when timeout == 0 using ActiveProfileCaches");
    assert_eq!(overview.status, "none");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_s05_f12_ticket_auth_rejects_mismatched_admin_pub_on_same_url() {
    let dir = crate::test_support::temp_test_dir("s05_f12_endpoint_match");
    let pw = "pw-s05-f12";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();
    let mut policy = SecurityPolicy::default();
    let expected_drop_pub = hex::encode([0xAAu8; 32]);
    let hijacked_admin_pub = hex::encode([0xBBu8; 32]);
    let hijacked_lock_pub = hex::encode([0xCCu8; 32]);
    policy.market_selected_syncs.push(
        humoco_market::DropDescriptor {
            write_url: "https://sync.example/sync.php".to_string(),
            read_url: "https://sync.example/d".to_string(),
            slot_id: None,
            drop_pub: expected_drop_pub,
        }
        .into(),
    );
    let probe = FrontendLockServerProbeResult {
        write_url: "https://sync.example/sync.php".to_string(),
        read_url: "https://sync.example/d".to_string(),
        drop_pub: hijacked_admin_pub.clone(),
        admin_pub: hijacked_admin_pub,
        lock_pub: hijacked_lock_pub,
        delegation_valid: true,
        waf_probe_passed: true,
        selftest_passed: true,
        key_leak_safe: true,
        passed: true,
        latency_ms: 10,
        max_retention_quarter: 8159,
        pow_base_bits: 8,
        allow_pow_tickets: false,
        ticket_pow_bits: 16,
        inode_warning: false,
        used_inodes: 0,
        max_inodes: 1000,
        probed_at_secs: 1_760_000_000,
        delegation: None,
        error: None,
    };
    let state = AppState::new(svc);
    let mut s = state.lock_service().unwrap();
    assert!(
        !has_usable_ticket_auth(&state, &mut s, &policy, &probe, Some(pw)).unwrap(),
        "S05-F12: mismatched non-empty admin_pub on same URL must NOT match circle sync"
    );
    assert!(
        resolve_ticket_auth_for_server(&mut s, &policy, &probe, Some(pw))
            .unwrap()
            .is_none(),
        "S05-F12: resolve_ticket_auth_for_server must NOT sign stealth auth for mismatched admin_pub"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_s05_f14_has_offline_cached_anchor_requires_preview_deletable_coverage() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::manifest::GenesisTicket;

    let dir = crate::test_support::temp_test_dir("s05_f14_preview_del");
    let pw = "pw-s05-f14";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let now_secs = sync_client::current_unix_timestamp();
    let short_ms = now_secs.saturating_add(30 * 86_400).saturating_mul(1000);
    let short_quarter = required_retention_quarter(short_ms).unwrap();

    let sk1 = SigningKey::from_bytes(&[131u8; 32]);
    let anchor_nonce = [132u8; 32];
    let creator_mig_pub = svc
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 1,
        servers: vec![ServerEntry {
            write_url: "https://short.example/sync.php".into(),
            read_url: "https://short.example/d".into(),
            admin_pub: sk1.verifying_key().to_bytes(),
            lock_pub: sk1.verifying_key().to_bytes(),
            nominated_by: NominatedBy::Creator,
        }],
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs: vec![],
        },
        migration_chain: vec![],
        issued_at_ms: now_secs.saturating_mul(1000),
        retain_until_quarter: short_quarter,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let mut ticket = GenesisTicket {
        anchor_id,
        retain_until_quarter: short_quarter,
        issued_at: now_secs,
        lock_pub: sk1.verifying_key().to_bytes(),
        sig: [0u8; 64],
    };
    ticket.sig = sk1.sign(&ticket.digest()).to_bytes();
    svc.register_l2_anchor_template_with_nonce(
        manifest,
        vec![ticket],
        Some(&anchor_nonce),
        Some(pw),
    )
    .unwrap();

    let state = AppState::new(svc);
    let mut s = state.lock_service().unwrap();
    let policy = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
    let lc = InternalLifecycleState::default();
    let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
        "voucher_standards/freetaler_v1/standard.toml",
    );
    let overview = build_lock_protection_overview_from_service(
        &state,
        &mut s,
        &policy,
        &lc,
        Some(&std_toml),
        Some("P2Y"),
        Some(pw),
    )
    .unwrap();
    assert!(
        !overview.has_offline_cached_anchor,
        "S05-F14: anchor covering only 30 days must NOT report has_offline_cached_anchor for P2Y voucher"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_s05_f4_reconcile_restored_wallet_preserves_fork_or_stale_alert() {
    let dir = crate::test_support::temp_test_dir("s05_f4_atomic_restore");
    let pw = "pw-s05-f4";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();
    let state = AppState::new(svc);
    let session = {
        let mut s = state.lock_service().unwrap();
        let mut lc = state
            .get_or_load_internal_lifecycle(&mut s, Some(pw))
            .unwrap();
        lc.pending_restore_l2_reconciliation = true;
        lc.wallet_sync_alert = Some(crate::settings::WalletSyncAlert {
            kind: "stale_local_backup".to_string(),
            mirror_write_url: "https://mirror.example/sync.php".to_string(),
            remote_epoch: Some(5),
            remote_tx_nonce: Some(12),
            local_epoch: 5,
            local_tx_nonce: 8,
            detected_at: "2026-11-21T12:00:00Z".to_string(),
            has_signed_ack_proof: true,
        });
        state.save_internal_lifecycle(&mut s, lc, Some(pw)).unwrap();
        state.session_token_for_service(&s).unwrap()
    };

    let cleared = reconcile_restored_wallet_if_pending(&state, &session, Some(pw))
        .await
        .unwrap();
    assert!(
        !cleared,
        "S05-F4: must NOT auto-clear pending_restore_l2_reconciliation while stale_local_backup alert is active"
    );
    let lc_after = {
        let mut s = state.lock_service().unwrap();
        state
            .get_or_load_internal_lifecycle(&mut s, Some(pw))
            .unwrap()
    };
    assert!(lc_after.pending_restore_l2_reconciliation);
    assert_eq!(
        lc_after.wallet_sync_alert.as_ref().map(|a| a.kind.as_str()),
        Some("stale_local_backup"),
        "S05-F4: must preserve wallet_sync_alert"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_s05_f5_check_voucher_l2_status_discovers_missing_manifest() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::encode_quarter;
    use human_money_core::models::profile::PublicProfile;
    use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let dir = crate::test_support::temp_test_dir("s05_f5_discover_missing");
    let pw = "pw-s05-f5";
    let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
        "voucher_standards/freetaler_v1/standard.toml",
    );
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://127.0.0.1:{}", addr.port());
    let write_url = format!("{base_url}/sync.php");
    let read_url = format!("{base_url}/d");

    let admin_sk = SigningKey::from_bytes(&[171u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[172u8; 32]);
    let admin_pub = admin_sk.verifying_key().to_bytes();
    let lock_pub = lock_sk.verifying_key().to_bytes();
    let anchor_nonce = [173u8; 32];
    let creator_mig_pub = svc
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 1,
        servers: vec![ServerEntry {
            write_url: write_url.clone(),
            read_url: read_url.clone(),
            admin_pub,
            lock_pub,
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
    let anchor_hex = hex::encode(anchor_id);

    let my_id = svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();
    // Intentionally do NOT register the manifest locally so disk & memory both lack it
    svc.create_new_voucher(
        &std_toml,
        NewVoucherData {
            creator_profile: PublicProfile {
                id: Some(my_id),
                ..Default::default()
            },
            nominal_value: ValueDefinition {
                amount: "25".into(),
                ..Default::default()
            },
            layer2_anchor: Some(anchor_id),
            ..Default::default()
        },
        Some(pw),
    )
    .unwrap();
    let local_id = svc
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();

    let admin_hex = hex::encode(admin_pub);
    let lock_hex = hex::encode(lock_pub);
    let del_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_hex,
        &lock_hex,
        &write_url,
        &read_url,
        1_760_000_000,
    );
    let del_sig = hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes());
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
    let info_body = serde_json::json!({
        "version": 1,
        "drop_pub": admin_hex,
        "read_url": read_url,
        "channels": ["card", "wot", "backup", "post", "lock"],
        "lock": {
            "lock_pub": lock_hex,
            "max_retention_quarter": retain_q,
            "pow_base_bits": 8,
            "allow_pow_tickets": true,
            "ticket_pow_bits": 16,
            "delegation": {
                "admin_pub": admin_hex,
                "lock_pub": lock_hex,
                "write_url": write_url,
                "read_url": read_url,
                "issued_at": 1_760_000_000u64,
                "sig": del_sig
            }
        }
    })
    .to_string();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_c = stop.clone();
    let manifest_path = format!("/anchors/{anchor_hex}.bin");
    let srv = tokio::spawn(async move {
        while !stop_c.load(Ordering::Relaxed) {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept()).await
            else {
                continue;
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or("");
            if first_line.contains(&manifest_path) {
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    manifest_bytes.len()
                );
                let _ = stream.write_all(hdr.as_bytes()).await;
                let _ = stream.write_all(&manifest_bytes).await;
            } else if first_line.contains("?info") {
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    info_body.len(),
                    info_body
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            } else {
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
        }
    });

    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        let mut sp = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
        sp.market_selected_syncs.push(
            humoco_market::DropDescriptor {
                write_url: write_url.clone(),
                read_url: read_url.clone(),
                slot_id: None,
                drop_pub: admin_hex,
            }
            .into(),
        );
        state.save_security_policy(&mut s, sp, Some(pw)).unwrap();
    }

    let dto = check_voucher_l2_status_impl(local_id, Some(pw.to_string()), &state)
        .await
        .expect("S05-F5: check_voucher_l2_status_impl must discover missing manifest instead of failing");
    assert_eq!(dto.status, "unspent");
    assert_eq!(dto.manifest_revision, Some(1));
    assert!(!dto.reduced_guarantor_protection);

    stop.store(true, Ordering::Relaxed);
    let _ = srv.await;
    let _ = std::fs::remove_dir_all(&dir);
}
