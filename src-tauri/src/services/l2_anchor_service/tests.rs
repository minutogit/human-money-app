use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    decode_hex_fixed, encode_quarter,
    manifest::{AnchorAuthority, AnchorManifest, GenesisTicket, NominatedBy, ServerEntry},
};
use human_money_core::models::layer2_api::{LockEntry, VoucherReconciliationResult};
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
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

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(name: &str) -> Self {
        Self(crate::test_support::temp_test_dir(name))
    }
}

impl Deref for TempDirGuard {
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
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct MockServerHandle {
    stop: Arc<AtomicBool>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl MockServerHandle {
    async fn shutdown(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}

impl Drop for MockServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

fn build_signed_probe_info_parts(
    admin_sk: &SigningKey,
    lock_sk: &SigningKey,
    write_url: &str,
    read_url: &str,
) -> (
    sync_client::SyncLockDelegationDto,
    sync_client::SyncLockSelftestReportDto,
) {
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
    (del, selftest)
}

fn build_signed_probe_dto(
    admin_sk: &SigningKey,
    lock_sk: &SigningKey,
    write_url: &str,
    read_url: &str,
    probed_at_secs: u64,
) -> FrontendLockServerProbeResult {
    let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
    let (del, _) = build_signed_probe_info_parts(admin_sk, lock_sk, write_url, read_url);
    FrontendLockServerProbeResult {
        write_url: write_url.to_string(),
        read_url: read_url.to_string(),
        drop_pub: admin_hex.clone(),
        admin_pub: admin_hex,
        lock_pub: lock_hex,
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
        probed_at_secs,
        delegation: Some(del),
        error: None,
    }
}

fn make_rev1_manifest_and_tickets(
    svc: &human_money_core::app_service::AppService,
    anchor_nonce: &[u8; 32],
    threshold: u8,
    retain_q: u16,
    guarantor_mig_pubs: Vec<[u8; 32]>,
    server_specs: &[(&SigningKey, String, String, NominatedBy)],
) -> ([u8; 32], AnchorManifest, Vec<GenesisTicket>) {
    let creator_mig_pub = svc
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let servers = server_specs
        .iter()
        .map(|(sk, write_url, read_url, nominated_by)| ServerEntry {
            write_url: write_url.clone(),
            read_url: read_url.clone(),
            admin_pub: sk.verifying_key().to_bytes(),
            lock_pub: sk.verifying_key().to_bytes(),
            nominated_by: *nominated_by,
        })
        .collect();
    let manifest = AnchorManifest {
        revision: 1,
        threshold,
        servers,
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs,
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let tickets = server_specs
        .iter()
        .map(|(sk, _, _, _)| {
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
    (anchor_id, manifest, tickets)
}

fn spawn_mock_lock_committee_server(
    listener: tokio::net::TcpListener,
    admin_sk: SigningKey,
    lock_sk: SigningKey,
    wurl: String,
    rurl: String,
    retain_q: u16,
    fail_ticket: bool,
) -> MockServerHandle {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (del, selftest) = build_signed_probe_info_parts(&admin_sk, &lock_sk, &wurl, &rurl);
    let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_pub = lock_sk.verifying_key().to_bytes();
    let lock_hex = hex::encode(lock_pub);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);

    let join = tokio::spawn(async move {
        loop {
            let accept = tokio::time::timeout(Duration::from_millis(40), listener.accept()).await;
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
    });

    MockServerHandle {
        stop,
        join: Some(join),
    }
}

fn spawn_404_server(listener: tokio::net::TcpListener) -> MockServerHandle {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let stop = Arc::new(AtomicBool::new(false));
    let stop_bg = Arc::clone(&stop);
    let join = tokio::spawn(async move {
        while !stop_bg.load(Ordering::Relaxed) {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(Duration::from_millis(50), listener.accept()).await
            else {
                continue;
            };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await;
        }
    });
    MockServerHandle {
        stop,
        join: Some(join),
    }
}

fn spawn_manifest_discovery_server(
    listener: tokio::net::TcpListener,
    manifest_path: String,
    manifest_bytes: Vec<u8>,
    info_body: String,
) -> MockServerHandle {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let stop = Arc::new(AtomicBool::new(false));
    let stop_c = Arc::clone(&stop);
    let join = tokio::spawn(async move {
        while !stop_c.load(Ordering::Relaxed) {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(Duration::from_millis(50), listener.accept()).await
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
    MockServerHandle {
        stop,
        join: Some(join),
    }
}

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
    let dir = TempDirGuard::new("k1_quorum_404");
    let pw = "pw-k1-test";
    let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
        "voucher_standards/freetaler_v1/standard.toml",
    );
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let sk1 = SigningKey::from_bytes(&[11u8; 32]);
    let sk2 = SigningKey::from_bytes(&[12u8; 32]);
    let sk3 = SigningKey::from_bytes(&[13u8; 32]);
    let anchor_nonce = [41u8; 32];
    let retain_q = encode_quarter(2035, 4).unwrap();
    let specs = vec![
        (
            &sk1,
            "http://127.0.0.1:18081/sync.php".to_string(),
            "http://127.0.0.1:18081/d".to_string(),
            NominatedBy::Guarantor(0),
        ),
        (
            &sk2,
            "http://127.0.0.1:18082/sync.php".to_string(),
            "http://127.0.0.1:18082/d".to_string(),
            NominatedBy::Guarantor(0),
        ),
        (
            &sk3,
            "http://127.0.0.1:18083/sync.php".to_string(),
            "http://127.0.0.1:18083/d".to_string(),
            NominatedBy::Guarantor(0),
        ),
    ];
    let (anchor_id, manifest, tickets) = make_rev1_manifest_and_tickets(
        &svc,
        &anchor_nonce,
        2, // N=3, Q=2 -> quorum 404 requires N - Q + 1 = 2 servers returning 404
        retain_q,
        vec![SigningKey::from_bytes(&[99u8; 32])
            .verifying_key()
            .to_bytes()],
        &specs,
    );
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
}

#[tokio::test]
async fn test_auto_committee_retry_offline_cache_and_fallback_n0() {
    let dir = TempDirGuard::new("auto_committee_fallback");
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
    let h1 = spawn_mock_lock_committee_server(
        l1,
        admin1_sk.clone(),
        lock1_sk,
        w1.clone(),
        r1.clone(),
        retain_q,
        true,
    );
    let h2 = spawn_mock_lock_committee_server(
        l2,
        admin2_sk.clone(),
        lock2_sk.clone(),
        w2.clone(),
        r2.clone(),
        retain_q,
        false,
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
    let saved_manifest = state
        .lock_service()
        .unwrap()
        .with_wallet(|w| {
            w.profile
                .l2_anchor_manifests
                .get(&hex::encode(aid_retry))
                .cloned()
        })
        .unwrap()
        .unwrap();
    assert_eq!(saved_manifest.servers.len(), 1);
    assert_eq!(saved_manifest.threshold, 1);
    assert_eq!(
        saved_manifest.servers[0].lock_pub,
        lock2_sk.verifying_key().to_bytes()
    );

    // 2. Both servers unreachable + cache invalidated -> reuses cached offline anchor!
    h1.shutdown().await;
    h2.shutdown().await;
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
}

#[tokio::test]
async fn test_k1_unverified_404_never_populates_verified_locator() {
    // Invariant: Without a verified 5-field delegation in probe_cache or from ?info,
    // resolve_verified_server_locator and verify_and_cache_static_read_locator must return None;
    // when a verified signed delegation exists in cache, resolve_verified_server_locator returns it.
    let admin_sk = SigningKey::from_bytes(&[61u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[62u8; 32]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let srv = spawn_404_server(listener);

    let read_url = format!("http://127.0.0.1:{}/d", port);
    let write_url = format!("http://127.0.0.1:{}/sync.php", port);
    let srv_entry = ServerEntry {
        admin_pub: admin_sk.verifying_key().to_bytes(),
        lock_pub: lock_sk.verifying_key().to_bytes(),
        write_url: write_url.clone(),
        read_url: read_url.clone(),
        nominated_by: NominatedBy::Creator,
    };

    let mut cache = BTreeMap::new();
    let mut learned = Vec::new();
    assert!(resolve_verified_server_locator(&srv_entry, &cache).is_none());
    let client = sync_client::create_sync_probe_client();
    assert!(
        verify_and_cache_static_read_locator(&client, &srv_entry, &mut cache, &mut learned)
            .await
            .is_none(),
        "K1: 404 on unverified ?info/read_url must never populate verified locator"
    );
    assert!(learned.is_empty());

    // Positive baseline: verified signed delegation in cache resolves the verified locator
    let verified_probe = build_signed_probe_dto(
        &admin_sk,
        &lock_sk,
        &write_url,
        &read_url,
        1_760_000_010,
    );
    assert!(upsert_verified_lock_probe(&mut cache, verified_probe));
    let resolved = resolve_verified_server_locator(&srv_entry, &cache)
        .expect("verified cached delegation must resolve server locator");
    assert_eq!(resolved, (write_url, read_url));

    srv.shutdown().await;
}

#[test]
fn test_h9_negative_health_probe_overwrites_stale_passed_preserving_delegation() {
    // Invariant: A newer failed health probe marks passed=false while preserving verified delegation
    // and capability metadata, whereas an older failed probe does not downgrade a newer healthy cache entry.
    let admin_sk = SigningKey::from_bytes(&[63u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[64u8; 32]);
    let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
    let write_url = "https://lock.example/sync.php";
    let read_url = "https://lock.example/d";

    let mut cache = BTreeMap::new();
    let healthy = build_signed_probe_dto(&admin_sk, &lock_sk, write_url, read_url, 1_760_000_010);
    assert!(upsert_verified_lock_probe(&mut cache, healthy.clone()));

    let stale_older_fail = FrontendLockServerProbeResult {
        passed: false,
        delegation_valid: false,
        probed_at_secs: 1_760_000_005,
        delegation: None,
        error: Some("stale timeout".into()),
        ..healthy.clone()
    };
    assert!(
        !upsert_verified_lock_probe(&mut cache, stale_older_fail),
        "older failed probe must not downgrade newer healthy probe"
    );
    assert!(cache.get(&lock_hex).unwrap().passed);

    let failed_later = FrontendLockServerProbeResult {
        write_url: write_url.to_string(),
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
    assert!(!cached.passed, "H9: newer failed probe must mark passed=false");
    assert!(
        cached.delegation_valid,
        "H9: verified delegation must remain intact"
    );
    assert_eq!(cached.pow_base_bits, 12);
    assert_eq!(cached.error.as_deref(), Some("connection refused"));
}

#[test]
fn test_h9_lock_protection_overview_excludes_revoked_offender_server() {
    use human_money_core::humoco_proto::types::{EquivocationProof, LockReceipt, LockReceiptKind};

    // Invariant: A healthy trusted cached lock server reports ready/ok before revocation, and once
    // an equivocation proof revokes the operator's admin_pub via record_equivocation_and_refresh_cache,
    // a sibling lock server under that admin_pub is downgraded to unverified and ready_trusted_count=0.
    let dir = TempDirGuard::new("h9_offender_overview");
    let pw = "StrongPassword123!";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let admin_sk = SigningKey::from_bytes(&[65u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[66u8; 32]);
    let sibling_lock_sk = SigningKey::from_bytes(&[67u8; 32]);
    let sibling_lock_pub = sibling_lock_sk.verifying_key().to_bytes();
    let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
    let healthy = build_signed_probe_dto(
        &admin_sk,
        &lock_sk,
        "https://lock-h9.example/sync.php",
        "https://lock-h9.example/d",
        1_760_000_010,
    );
    let sibling_probe = build_signed_probe_dto(
        &admin_sk,
        &sibling_lock_sk,
        "https://lock-h9-sibling.example/sync.php",
        "https://lock-h9-sibling.example/d",
        1_760_000_010,
    );
    let lc = InternalLifecycleState {
        l2_probe_cache: BTreeMap::from([(lock_hex.clone(), healthy.clone())]),
        ..Default::default()
    };

    let state = AppState::new(svc);
    let mut s = state.lock_service().unwrap();
    let policy = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();

    // Negative check before revocation: passed=true without valid delegation must NOT be ready/ok
    let lc_unsigned = InternalLifecycleState {
        l2_probe_cache: BTreeMap::from([(
            lock_hex,
            FrontendLockServerProbeResult {
                delegation_valid: false,
                delegation: None,
                ..healthy
            },
        )]),
        ..Default::default()
    };
    let overview_unsigned = build_lock_protection_overview_from_service(
        &state,
        &mut s,
        &policy,
        &lc_unsigned,
        None,
        None,
        Some(pw),
    )
    .unwrap();
    assert_eq!(overview_unsigned.ready_trusted_count, 0);
    assert_eq!(overview_unsigned.servers[0].status, "unverified");

    // Positive baseline before revocation
    let overview_before = build_lock_protection_overview_from_service(
        &state,
        &mut s,
        &policy,
        &lc,
        None,
        None,
        Some(pw),
    )
    .unwrap();
    assert_eq!(overview_before.ready_trusted_count, 1);
    assert_eq!(overview_before.status, "ready");
    assert_eq!(overview_before.servers[0].status, "ok");

    // Revoke admin_pub via real EquivocationProof on sibling lock server (refreshing offender cache)
    let make_rcpt = |child: [u8; 32], eh: [u8; 32]| {
        let mut r = LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub: sibling_lock_pub,
            bucket: [1u8; 32],
            slot: [2u8; 32],
            child_lock: child,
            entry_hash: eh,
            retain_until_quarter: 8159,
            write_seq: 1,
            bucket_head: [3u8; 32],
            recipient_nonce: [4u8; 32],
            sig: [0u8; 64],
        };
        r.sig = sibling_lock_sk.sign(&r.digest()).to_bytes();
        r
    };
    let eq = EquivocationProof {
        receipt_a: make_rcpt([0xAAu8; 32], [0x11u8; 32]),
        receipt_b: make_rcpt([0xBBu8; 32], [0x22u8; 32]),
    };
    let sibling_cache = BTreeMap::from([(sibling_probe.lock_pub.clone(), sibling_probe)]);
    assert!(crate::l2_lifecycle::record_equivocation_and_refresh_cache(
        &state,
        &mut s,
        &sibling_cache,
        &eq,
        1_760_000_020,
        Some(pw),
    )
    .unwrap());

    let overview_after = build_lock_protection_overview_from_service(
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
        overview_after.ready_trusted_count, 0,
        "H9: revoked server must not count as ready"
    );
    assert_eq!(overview_after.status, "degraded");
    assert_eq!(overview_after.servers[0].status, "unverified");
    assert!(overview_after.servers[0]
        .error
        .as_deref()
        .is_some_and(|e| e.contains("permanently revoked on ServerOffenderList")));
}

#[test]
fn test_s05_f8_get_lock_protection_overview_succeeds_with_zero_timeout() {
    let dir = TempDirGuard::new("s05_f8_zero_timeout");
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
}

#[test]
fn test_s05_f12_ticket_auth_rejects_mismatched_admin_pub_on_same_url() {
    // Invariant: Circle sync stealth ticket auth MUST succeed when probe admin_pub matches
    // the configured circle drop_pub, and MUST fail closed when admin_pub on the same URL mismatches.
    let dir = TempDirGuard::new("s05_f12_endpoint_match");
    let pw = "pw-s05-f12";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();
    let mut policy = SecurityPolicy::default();
    let expected_admin_sk = SigningKey::from_bytes(&[0xAAu8; 32]);
    let expected_drop_pub = hex::encode(expected_admin_sk.verifying_key().as_bytes());
    let hijacked_admin_pub = hex::encode(SigningKey::from_bytes(&[0xBBu8; 32]).verifying_key().as_bytes());
    let lock_pub = hex::encode(SigningKey::from_bytes(&[0xCCu8; 32]).verifying_key().as_bytes());
    policy.market_selected_syncs.push(
        humoco_market::DropDescriptor {
            write_url: "https://sync.example/sync.php".to_string(),
            read_url: "https://sync.example/d".to_string(),
            slot_id: None,
            drop_pub: expected_drop_pub.clone(),
        }
        .into(),
    );
    let base_probe = FrontendLockServerProbeResult {
        write_url: "https://sync.example/sync.php".to_string(),
        read_url: "https://sync.example/d".to_string(),
        drop_pub: expected_drop_pub.clone(),
        admin_pub: expected_drop_pub.clone(),
        lock_pub,
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

    // Positive baseline: matching admin_pub resolves Stealth ticket auth
    assert!(
        has_usable_ticket_auth(&state, &mut s, &policy, &base_probe, Some(pw)).unwrap(),
        "matching admin_pub on circle sync must have usable ticket auth"
    );
    assert!(matches!(
        resolve_ticket_auth_for_server(&mut s, &policy, &base_probe, Some(pw)).unwrap(),
        Some(sync_client::LockTicketAuth::Stealth { ref drop_pub_hex, .. })
            if drop_pub_hex == &expected_drop_pub
    ));

    // Negative invariant: mismatched non-empty admin_pub on exact same URL is rejected
    let hijacked_probe = FrontendLockServerProbeResult {
        drop_pub: hijacked_admin_pub.clone(),
        admin_pub: hijacked_admin_pub,
        ..base_probe
    };
    assert!(
        !has_usable_ticket_auth(&state, &mut s, &policy, &hijacked_probe, Some(pw)).unwrap(),
        "S05-F12: mismatched non-empty admin_pub on same URL must NOT match circle sync"
    );
    assert!(
        resolve_ticket_auth_for_server(&mut s, &policy, &hijacked_probe, Some(pw))
            .unwrap()
            .is_none(),
        "S05-F12: resolve_ticket_auth_for_server must NOT sign stealth auth for mismatched admin_pub"
    );

    // URL fallback when at least one pubkey is empty, and strict rejection when both URLs empty
    assert!(matches_sync_endpoint(
        "",
        "https://sync.example/sync.php",
        &expected_drop_pub,
        "https://sync.example/sync.php/"
    ));
    assert!(matches_sync_endpoint(
        &expected_drop_pub,
        "https://sync.example/sync.php",
        "",
        "https://sync.example/sync.php"
    ));
    assert!(!matches_sync_endpoint(
        "",
        "https://sync.example/sync.php",
        &expected_drop_pub,
        "https://other.example/sync.php"
    ));
    assert!(!matches_sync_endpoint("", "", "", ""));
}

#[test]
fn test_s05_f14_has_offline_cached_anchor_requires_preview_deletable_coverage() {
    // Invariant: has_offline_cached_anchor must stay false when cached anchor retention falls
    // short of preview deletable_ms ("P2Y"), and flip true once a covering anchor is registered.
    let dir = TempDirGuard::new("s05_f14_preview_del");
    let pw = "pw-s05-f14";
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let now_secs = sync_client::current_unix_timestamp();
    let short_ms = now_secs.saturating_add(30 * 86_400).saturating_mul(1000);
    let short_quarter = required_retention_quarter(short_ms).unwrap();

    let sk1 = SigningKey::from_bytes(&[131u8; 32]);
    let specs_short = vec![(
        &sk1,
        "https://short.example/sync.php".to_string(),
        "https://short.example/d".to_string(),
        NominatedBy::Creator,
    )];
    let nonce_short = [132u8; 32];
    let (_, short_manifest, short_tickets) =
        make_rev1_manifest_and_tickets(&svc, &nonce_short, 1, short_quarter, vec![], &specs_short);
    svc.register_l2_anchor_template_with_nonce(
        short_manifest,
        short_tickets,
        Some(&nonce_short),
        Some(pw),
    )
    .unwrap();

    let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
        "voucher_standards/freetaler_v1/standard.toml",
    );
    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        let policy = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
        let lc = InternalLifecycleState::default();
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
        assert_eq!(overview.expected_source, "none");

        // Positive baseline: register a second rev1 creator anchor covering P2Y + retention
        let covering_ms = estimate_deletable_ms(now_secs, Some(&std_toml), Some("P2Y"));
        let covering_quarter = required_retention_quarter(covering_ms).unwrap();
        let nonce_long = [133u8; 32];
        let specs_long = vec![(
            &sk1,
            "https://long.example/sync.php".to_string(),
            "https://long.example/d".to_string(),
            NominatedBy::Creator,
        )];
        let (_, long_manifest, long_tickets) = make_rev1_manifest_and_tickets(
            &s,
            &nonce_long,
            1,
            covering_quarter,
            vec![],
            &specs_long,
        );
        s.register_l2_anchor_template_with_nonce(
            long_manifest,
            long_tickets,
            Some(&nonce_long),
            Some(pw),
        )
        .unwrap();

        let overview_covered = build_lock_protection_overview_from_service(
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
            overview_covered.has_offline_cached_anchor,
            "covering P2Y anchor must be reported as offline cached anchor"
        );
        assert_eq!(overview_covered.expected_source, "cached");
    }
}

#[tokio::test]
async fn test_s05_f4_reconcile_restored_wallet_preserves_fork_or_stale_alert() {
    let dir = TempDirGuard::new("s05_f4_atomic_restore");
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
    let lc_after = state
        .get_or_load_internal_lifecycle_locked(Some(pw))
        .unwrap();
    assert!(lc_after.pending_restore_l2_reconciliation);
    assert_eq!(
        lc_after.wallet_sync_alert.as_ref().map(|a| a.kind.as_str()),
        Some("stale_local_backup"),
        "S05-F4: must preserve wallet_sync_alert"
    );

    // Positive baseline: clearing only the stale_local_backup alert lets reconciliation finish
    {
        let mut s = state.lock_service().unwrap();
        let mut lc = state
            .get_or_load_internal_lifecycle(&mut s, Some(pw))
            .unwrap();
        lc.wallet_sync_alert = None;
        state.save_internal_lifecycle(&mut s, lc, Some(pw)).unwrap();
    }
    let cleared_clean = reconcile_restored_wallet_if_pending(&state, &session, Some(pw))
        .await
        .unwrap();
    assert!(
        cleared_clean,
        "reconciliation must clear pending flag once alert is resolved"
    );
    let lc_final = state
        .get_or_load_internal_lifecycle_locked(Some(pw))
        .unwrap();
    assert!(!lc_final.pending_restore_l2_reconciliation);
}

#[tokio::test]
async fn test_s05_f5_check_voucher_l2_status_discovers_missing_manifest() {
    let dir = TempDirGuard::new("s05_f5_discover_missing");
    let pw = "pw-s05-f5";
    let std_toml = human_money_core::test_utils::generate_signed_standard_toml(
        "voucher_standards/freetaler_v1/standard.toml",
    );
    let mut svc =
        crate::test_support::setup_test_app(&dir, "Alice", crate::test_support::ALICE_PHRASE, pw);
    svc.unlock_session(pw, 600).unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let write_url = format!("{base_url}/sync.php");
    let read_url = format!("{base_url}/d");

    let admin_sk = SigningKey::from_bytes(&[171u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[172u8; 32]);
    let admin_pub = admin_sk.verifying_key().to_bytes();
    let lock_pub = lock_sk.verifying_key().to_bytes();
    let anchor_nonce = [173u8; 32];
    let retain_q = encode_quarter(2035, 4).unwrap();
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

    let (del, _) = build_signed_probe_info_parts(&admin_sk, &lock_sk, &write_url, &read_url);
    let admin_hex = hex::encode(admin_pub);
    let lock_hex = hex::encode(lock_pub);
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
            "delegation": del
        }
    })
    .to_string();

    let srv = spawn_manifest_discovery_server(
        listener,
        format!("/anchors/{anchor_hex}.bin"),
        serde_json::to_vec(&manifest).unwrap(),
        info_body,
    );

    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        let mut sp = state.get_or_load_security_policy(&mut s, Some(pw)).unwrap();
        sp.market_selected_syncs.push(
            humoco_market::DropDescriptor {
                write_url,
                read_url,
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

    srv.shutdown().await;
}
