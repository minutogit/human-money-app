use super::discovery::*;
use super::offenders::*;
use super::rotation_policy::*;
use super::rotation_runner::*;
use super::*;
use crate::services::common::{
    max_pow_base_bits_for_servers, resolve_server_locator, upsert_verified_lock_probe,
};
use crate::settings::{InternalLifecycleState, SecurityPolicy};
use crate::state::{AppState, MARKET_CARD_STORE_KEY};
use crate::sync_client::{self, FrontendLockServerProbeResult, SyncRevocationNotice};
use ed25519_dalek::{Signer, SigningKey};
use human_money_core::{
    humoco_proto::{
        self,
        crypto::DEFAULT_LOCK_POW_BASE_BITS,
        decode_hex_fixed, encode_quarter, resolve_import_union,
        types::{EquivocationProof, LockReceipt, LockReceiptKind},
        AnchorAuthority, AnchorManifest, CommitteeSignature, GenesisTicket, ImportAck, LockEntry,
        MigrationCert, MigrationProposal, NominatedBy, ProposalInitiator, ServerEntry,
        SnapshotExport,
    },
    services::{l2_gateway, l2_rotation},
    test_utils::{self, ACTORS},
};
use humoco_market::{
    own_envelope, render_wire, sign_sd_card, CardStore, DropDescriptor, WireFilter,
};
use humoco_wot::{KeyRotationReason, KeySuccessorAttestation, ServerOffenderList};
use std::collections::{BTreeMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

struct TempDirGuard(std::path::PathBuf);

impl TempDirGuard {
    fn new(tag: &str) -> Self {
        Self(std::env::temp_dir().join(format!("l2-lc-{}-{}", tag, Uuid::new_v4())))
    }

    fn path_buf(&self) -> std::path::PathBuf {
        self.0.clone()
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.0.exists() {
            if let Err(err) = std::fs::remove_dir_all(&self.0) {
                eprintln!(
                    "TempDirGuard cleanup failed for {}: {}",
                    self.0.display(),
                    err
                );
            }
        }
    }
}

async fn stop_and_join_mocks(
    stop: Arc<AtomicBool>,
    handles: Vec<tokio::task::JoinHandle<()>>,
) {
    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        handle.await.unwrap();
    }
}

async fn write_http_response(stream: &mut tokio::net::TcpStream, payload: &[u8]) {
    if let Err(err) = stream.write_all(payload).await {
        eprintln!("mock HTTP response write failed: {}", err);
    }
}

fn make_server_entry(sk: &SigningKey, base_url: &str, nominated_by: NominatedBy) -> ServerEntry {
    ServerEntry {
        write_url: format!("{base_url}/sync.php"),
        read_url: format!("{base_url}/d"),
        admin_pub: sk.verifying_key().to_bytes(),
        lock_pub: sk.verifying_key().to_bytes(),
        nominated_by,
    }
}

fn make_test_manifest(
    s1_sk: &SigningKey,
    s2_sk: &SigningKey,
    s1_url: &str,
    s2_url: &str,
    successor: Option<ServerEntry>,
    creator_mig_pub: [u8; 32],
    guarantor_mig_pubs: Vec<[u8; 32]>,
) -> AnchorManifest {
    AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![
            make_server_entry(s1_sk, s1_url, NominatedBy::Creator),
            make_server_entry(s2_sk, s2_url, NominatedBy::Guarantor(0)),
        ],
        successors: successor.into_iter().collect(),
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs,
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: encode_quarter(2035, 4).unwrap(),
    }
}

fn make_genesis_tickets(
    anchor_id: [u8; 32],
    retain_until_quarter: u16,
    issued_at: u64,
    keys: &[&SigningKey],
) -> Vec<GenesisTicket> {
    keys.iter()
        .map(|sk| {
            let mut t = GenesisTicket {
                anchor_id,
                retain_until_quarter,
                issued_at,
                lock_pub: sk.verifying_key().to_bytes(),
                sig: [0u8; 64],
            };
            t.sig = sk.sign(&t.digest()).to_bytes();
            t
        })
        .collect()
}

fn make_signed_import_ack(
    anchor_id: [u8; 32],
    proposal_hash: [u8; 32],
    to_rev: u64,
    imported_state_hash: [u8; 32],
    retain_until_quarter: u16,
    acked_at: u64,
    sk: &SigningKey,
) -> ImportAck {
    let mut ack = ImportAck {
        anchor_id,
        proposal_hash,
        to_rev,
        imported_state_hash,
        retain_until_quarter,
        lock_pub: sk.verifying_key().to_bytes(),
        acked_at,
        sig: [0u8; 64],
    };
    ack.sig = sk.sign(&ack.digest()).to_bytes();
    ack
}

fn make_signed_commit_sigs(
    anchor_id: [u8; 32],
    proposal_hash: [u8; 32],
    to_rev: u64,
    imported_state_hash: [u8; 32],
    committed_at: u64,
    keys: &[&SigningKey],
) -> Vec<CommitteeSignature> {
    let c_digest =
        MigrationCert::commit_digest(&anchor_id, &proposal_hash, to_rev, &imported_state_hash);
    keys.iter()
        .map(|sk| CommitteeSignature {
            lock_pub: sk.verifying_key().to_bytes(),
            committed_at,
            sig: sk.sign(&c_digest).to_bytes(),
        })
        .collect()
}

fn build_migrated_rev2_manifest(
    rev1: &AnchorManifest,
    s1_sk: &SigningKey,
    s2_sk: &SigningKey,
    s3_sk: &SigningKey,
    now0: u64,
) -> ([u8; 32], MigrationProposal, AnchorManifest) {
    let anchor_id = rev1.rev1_anchor_id().unwrap();
    let prop = l2_rotation::build_holder_standby_proposal(
        anchor_id,
        rev1,
        &[s1_sk.verifying_key().to_bytes()],
        now0,
    )
    .unwrap();
    let prop_hash = prop.core_digest().unwrap();
    let exports = vec![
        make_freeze_export(anchor_id, s1_sk, rev1.retain_until_quarter, prop_hash, now0),
        make_freeze_export(anchor_id, s2_sk, rev1.retain_until_quarter, prop_hash, now0),
    ];
    let resolved = resolve_import_union(&anchor_id, rev1, &prop, &exports, &[], 0).unwrap();
    let acks = vec![
        make_signed_import_ack(
            anchor_id,
            prop_hash,
            2,
            resolved.imported_state_hash,
            rev1.retain_until_quarter,
            now0 + 1,
            s3_sk,
        ),
        make_signed_import_ack(
            anchor_id,
            prop_hash,
            2,
            resolved.imported_state_hash,
            rev1.retain_until_quarter,
            now0 + 1,
            s2_sk,
        ),
    ];
    let commit_sigs = make_signed_commit_sigs(
        anchor_id,
        prop_hash,
        2,
        resolved.imported_state_hash,
        now0 + 2,
        &[s1_sk, s2_sk],
    );
    let (rev2, _) = l2_rotation::assemble_migrated_manifest(
        rev1,
        &anchor_id,
        prop.clone(),
        resolved.imported_state_hash,
        acks,
        commit_sigs,
    )
    .unwrap();
    (anchor_id, prop, rev2)
}

#[allow(clippy::too_many_arguments)]
fn spawn_committee_mock_server(
    listener: tokio::net::TcpListener,
    anchor_id: [u8; 32],
    retain_q: u16,
    sk: SigningKey,
    revoked_flag: Arc<AtomicBool>,
    voted_prop: Arc<Mutex<Option<MigrationProposal>>>,
    effective_ts: u64,
    include_delegation: bool,
    stop: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let port = listener.local_addr().unwrap().port();
        let write_url = format!("http://127.0.0.1:{}/sync.php", port);
        let read_url = format!("http://127.0.0.1:{}/d", port);
        let lock_pub = sk.verifying_key().to_bytes();
        let lock_hex = hex::encode(lock_pub);
        let bucket = humoco_proto::derive_bucket(&anchor_id, &lock_pub);
        let del_msg = humoco_market::format_lock_delegate_signature_message(
            &lock_hex,
            &lock_hex,
            &write_url,
            &read_url,
            1_760_000_000,
        );
        let del_sig = hex::encode(sk.sign(del_msg.as_bytes()).to_bytes());
        let mut current_rev = 1u64;
        loop {
            let accept =
                tokio::time::timeout(std::time::Duration::from_millis(40), listener.accept()).await;
            let Ok(Ok((mut stream, _))) = accept else {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            };
            let mut buf = [0u8; 32_768];
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
            let make_export = |ph: Option<[u8; 32]>| {
                let mut s = humoco_proto::FreezeSnapshot {
                    anchor_id,
                    bucket,
                    from_rev: 1,
                    retain_until_quarter: retain_q,
                    write_seq: 0,
                    bucket_head: humoco_proto::crypto::compute_bucket_init_head(
                        &bucket, retain_q,
                    ),
                    state_hash: humoco_proto::compute_freeze_state_hash(
                        &anchor_id,
                        1,
                        &[],
                        &[],
                    )
                    .unwrap(),
                    permanent: true,
                    proposal_hash: ph,
                    frozen_at: effective_ts.saturating_sub(100),
                    expires_at: None,
                    lock_pub,
                    sig: [0u8; 64],
                };
                s.sig = sk.sign(&s.digest()).to_bytes();
                SnapshotExport {
                    snapshot: s,
                    entries: vec![],
                    conflicts: vec![],
                }
            };
            let resp_json: serde_json::Value = if path.contains("info") {
                let rev_val = if revoked_flag.load(Ordering::Relaxed) {
                    let n = sync_client::sign_revocation_notice(
                        &sk,
                        1_760_000_000,
                        1_760_000_010,
                        Some("planned".into()),
                        None,
                    )
                    .unwrap();
                    serde_json::to_value(n).unwrap()
                } else {
                    serde_json::Value::Null
                };
                let mut lock_obj = serde_json::json!({
                    "lock_pub": lock_hex,
                    "max_retention_quarter": retain_q,
                    "pow_base_bits": 0,
                    "allow_pow_tickets": true,
                    "ticket_pow_bits": 16
                });
                if include_delegation {
                    lock_obj["delegation"] = serde_json::json!({
                        "admin_pub": lock_hex,
                        "lock_pub": lock_hex,
                        "write_url": write_url,
                        "read_url": read_url,
                        "issued_at": 1_760_000_000u64,
                        "sig": del_sig
                    });
                }
                serde_json::json!({
                    "drop_pub": lock_hex,
                    "read_url": read_url,
                    "revocation": rev_val,
                    "lock": lock_obj
                })
            } else if path.contains("lock_freeze") {
                if head.starts_with("GET ") {
                    let p = voted_prop.lock().unwrap().clone();
                    let snap = p
                        .as_ref()
                        .map(|prop| make_export(Some(prop.core_digest().unwrap())));
                    serde_json::json!({
                        "anchor_id": hex::encode(anchor_id),
                        "bucket": hex::encode(bucket),
                        "active_manifest": serde_json::Value::Null,
                        "last_creator_activity_secs": 1_760_000_000u64,
                        "bucket_freeze_state": {},
                        "voted_proposal": p,
                        "snapshot_export": snap
                    })
                } else {
                    let val: serde_json::Value = serde_json::from_str(body).unwrap();
                    match val.get("action").and_then(|v| v.as_str()) {
                        Some("propose") => {
                            let prop: MigrationProposal =
                                serde_json::from_value(val["proposal"].clone()).unwrap();
                            *voted_prop.lock().unwrap() = Some(prop);
                            serde_json::json!({ "status": "proposed", "effective_at": effective_ts })
                        }
                        Some("fetch_snapshot") => {
                            let ph = voted_prop
                                .lock()
                                .unwrap()
                                .as_ref()
                                .and_then(|p| p.core_digest().ok());
                            serde_json::json!({ "snapshot_export": make_export(ph) })
                        }
                        _ => serde_json::json!({ "status": "ok" }),
                    }
                }
            } else if path.contains("lock_import") {
                let req: serde_json::Value = serde_json::from_str(body).unwrap();
                let prop: MigrationProposal =
                    serde_json::from_value(req["proposal"].clone()).unwrap();
                let ish = humoco_proto::compute_import_union_state_hash(
                    &anchor_id,
                    prop.to_rev,
                    &BTreeMap::new(),
                )
                .unwrap();
                let ack = make_signed_import_ack(
                    anchor_id,
                    prop.core_digest().unwrap(),
                    prop.to_rev,
                    ish,
                    retain_q,
                    effective_ts,
                    &sk,
                );
                serde_json::json!({ "import_ack": ack })
            } else if path.contains("lock_commit") {
                let req: serde_json::Value = serde_json::from_str(body).unwrap();
                if req.get("action").and_then(|v| v.as_str()) == Some("sign_commit") {
                    let prop: MigrationProposal =
                        serde_json::from_value(req["proposal"].clone()).unwrap();
                    let ish =
                        decode_hex_fixed::<32>(req["imported_state_hash"].as_str().unwrap())
                            .unwrap();
                    let cs = make_signed_commit_sigs(
                        anchor_id,
                        prop.core_digest().unwrap(),
                        prop.to_rev,
                        ish,
                        effective_ts,
                        &[&sk],
                    )
                    .remove(0);
                    serde_json::json!({ "committee_signature": cs })
                } else {
                    current_rev += 1;
                    serde_json::json!({ "status": "finalized", "revision": current_rev })
                }
            } else {
                serde_json::json!({})
            };
            let json_str = resp_json.to_string();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json_str.len(),
                json_str
            );
            write_http_response(&mut stream, http.as_bytes()).await;
        }
    })
}

fn spawn_veto_mock_server(
    listener: tokio::net::TcpListener,
    anchor_id: [u8; 32],
    sk: SigningKey,
    prop: MigrationProposal,
    veto_flag: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let lock_hex = hex::encode(sk.verifying_key().to_bytes());
        loop {
            let accept =
                tokio::time::timeout(std::time::Duration::from_millis(40), listener.accept()).await;
            let Ok(Ok((mut stream, _))) = accept else {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            };
            let mut buf = [0u8; 16384];
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
            let resp = if path.contains("info") {
                serde_json::json!({
                    "drop_pub": lock_hex,
                    "read_url": "http://127.0.0.1:9/d",
                    "revocation": null,
                    "lock": {
                        "lock_pub": lock_hex,
                        "max_retention_quarter": 8159,
                        "pow_base_bits": 0,
                        "ticket_pow_bits": 16
                    }
                })
            } else if path.contains("lock_freeze") && head.starts_with("GET ") {
                serde_json::json!({
                    "anchor_id": hex::encode(anchor_id),
                    "bucket": "00".repeat(32),
                    "active_manifest": serde_json::Value::Null,
                    "last_creator_activity_secs": 1_760_000_000u64,
                    "bucket_freeze_state": {},
                    "voted_proposal": prop,
                    "snapshot_export": serde_json::Value::Null
                })
            } else if path.contains("lock_freeze") {
                let val: serde_json::Value = serde_json::from_str(body).unwrap();
                if val.get("action").and_then(|v| v.as_str()) == Some("veto") {
                    veto_flag.store(true, Ordering::Relaxed);
                }
                serde_json::json!({ "status": "vetoed" })
            } else {
                serde_json::json!({})
            };
            let txt = resp.to_string();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                txt.len(),
                txt
            );
            write_http_response(&mut stream, http.as_bytes()).await;
        }
    })
}

fn spawn_anchor_manifest_server(
    listener: tokio::net::TcpListener,
    anchor_id: [u8; 32],
    manifest: &AnchorManifest,
) -> tokio::task::JoinHandle<()> {
    let rev2_json = serde_json::to_vec(manifest).unwrap();
    let expected_path = format!("/anchors/{}.bin", hex::encode(anchor_id));
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let n = stream.read(&mut buf).await.unwrap();
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req
            .lines()
            .next()
            .unwrap_or("")
            .split_whitespace()
            .nth(1)
            .unwrap_or("");
        if path.ends_with(&expected_path) {
            let mut out = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                rev2_json.len()
            )
            .into_bytes();
            out.extend_from_slice(&rev2_json);
            write_http_response(&mut stream, &out).await;
        } else {
            write_http_response(
                &mut stream,
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n",
            )
            .await;
        }
    })
}

#[test]
fn test_server_offender_list_blocks_equivocating_lock_key() {
    // Invariant: valid equivocations revoke lock_pub while corrupt offender/lifecycle blobs fail closed without overwriting.
    let dir_guard = TempDirGuard::new("offender");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();

    let bad_sk = SigningKey::from_bytes(&[77u8; 32]);
    let bad_pub = bad_sk.verifying_key().to_bytes();
    let make_rcpt = |child: [u8; 32], eh: [u8; 32]| {
        let mut r = LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub: bad_pub,
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
        r.sig = bad_sk.sign(&r.digest()).to_bytes();
        r
    };
    let eq = EquivocationProof {
        receipt_a: make_rcpt([10u8; 32], [20u8; 32]),
        receipt_b: make_rcpt([11u8; 32], [21u8; 32]),
    };

    let newly = record_equivocation_proof(&mut svc, None, &eq, 1_760_000_000, Some(pw))
        .expect("record equivocation");
    assert!(newly);

    let loaded = load_server_offender_list(&mut svc, Some(pw)).unwrap();
    assert!(loaded.is_lock_pub_revoked(&bad_pub));

    svc.save_encrypted_data(SERVER_OFFENDER_LIST_KEY, b"{corrupt-offenders", Some(pw))
        .unwrap();
    let load_err = load_server_offender_list(&mut svc, Some(pw)).unwrap_err();
    assert!(
        load_err.message.contains("Failed to parse l2_server_offenders"),
        "expected offender parse error, got: {:?}",
        load_err
    );
    let rec_err = record_equivocation_proof(&mut svc, None, &eq, 1_760_000_000, Some(pw)).unwrap_err();
    assert!(
        rec_err.message.contains("Failed to parse l2_server_offenders"),
        "expected offender parse error on record, got: {:?}",
        rec_err
    );
    assert_eq!(
        svc.load_encrypted_data(SERVER_OFFENDER_LIST_KEY, Some(pw))
            .unwrap(),
        b"{corrupt-offenders"
    );

    svc.save_encrypted_data(L2_LIFECYCLE_STATE_KEY, b"{corrupt-lifecycle", Some(pw))
        .unwrap();
    let lc_err = load_lifecycle_state(&mut svc, Some(pw)).unwrap_err();
    assert!(
        lc_err.message.contains("Failed to parse l2_lifecycle_state"),
        "expected lifecycle parse error, got: {:?}",
        lc_err
    );
}

#[tokio::test]
async fn test_discovery_cascade_resolves_successor_chain_and_card_store() {
    // Invariant: anchor manifest discovery follows KeySuccessorAttestation -> CardStore drops to locate Rev 2 manifests.
    let dir_guard = TempDirGuard::new("discovery");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.bob, "Bob", pw);
    svc.unlock_session(pw, 600).unwrap();

    let s1_sk = SigningKey::from_bytes(&[11u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[12u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[13u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[14u8; 32]);
    let rev1 = make_test_manifest(
        &s1_sk,
        &s2_sk,
        "http://127.0.0.1:9",
        "http://127.0.0.1:9",
        Some(make_server_entry(&s3_sk, "http://127.0.0.1:9", NominatedBy::Creator)),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![SigningKey::from_bytes(&[15u8; 32])
            .verifying_key()
            .to_bytes()],
    );
    let (anchor_id, _prop, rev2) =
        build_migrated_rev2_manifest(&rev1, &s1_sk, &s2_sk, &s3_sk, 1_760_000_100);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_task = spawn_anchor_manifest_server(listener, anchor_id, &rev2);

    let old_creator_did = ACTORS.alice.identity.user_id.clone();
    let new_creator_did = ACTORS.charlie.identity.user_id.clone();
    let att = KeySuccessorAttestation::sign_dual(
        &crate::state::DidWotSigner {
            did: crate::state::canonicalize_did(&old_creator_did).unwrap(),
            signing_key: ACTORS.alice.identity.signing_key.clone(),
        },
        &crate::state::DidWotSigner {
            did: crate::state::canonicalize_did(&new_creator_did).unwrap(),
            signing_key: ACTORS.charlie.identity.signing_key.clone(),
        },
        KeyRotationReason::Routine,
        None,
        sync_client::current_unix_timestamp().saturating_sub(10),
    )
    .unwrap();
    save_key_successor_attestations(&mut svc, &[att], Some(pw)).unwrap();

    let card = sign_sd_card(
        humoco_market::CardPayloadInput {
            did: new_creator_did,
            v: 1,
            valid_until: sync_client::current_unix_timestamp() + 86_400,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![DropDescriptor {
                write_url: format!("http://127.0.0.1:{}/sync.php", port),
                read_url: format!("http://127.0.0.1:{}/d-alice", port),
                slot_id: None,
                drop_pub: "aa".repeat(32),
            }],
        },
        vec![],
        &ACTORS.charlie.identity.signing_key,
    )
    .unwrap();
    let mut store = CardStore::default();
    humoco_market::accept_envelope(
        &mut store,
        &own_envelope(render_wire(&card, 1, WireFilter::Full).unwrap()),
        sync_client::current_unix_timestamp(),
        100,
        None,
        &|_| false,
    );
    svc.save_encrypted_data(
        MARKET_CARD_STORE_KEY,
        &serde_json::to_vec(&store).unwrap(),
        Some(pw),
    )
    .unwrap();

    let state = AppState::new(svc);
    let session = state.begin_session_op().unwrap();
    let client = sync_client::create_sync_probe_client();
    let discovered = discover_anchor_manifest(
        &state,
        &session,
        &client,
        &anchor_id,
        Some(&old_creator_did),
        &[],
        Some(pw),
    )
    .await
    .unwrap()
    .expect("must discover migrated rev 2 manifest");

    assert_eq!(discovered.revision, 2);
    let cached = state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| {
            w.profile
                .l2_anchor_manifests
                .get(&hex::encode(anchor_id))
                .cloned()
        })
        .unwrap()
        .unwrap();
    assert_eq!(cached.revision, 2);

    server_task.await.unwrap();
}

#[tokio::test]
async fn test_hysteresis_5d_vs_immediate_revocation_and_guarantor_auto_veto() {
    // Invariant: active SyncRevocationNotice bypasses 5-day hysteresis and completes standby rotation once timelock expires.
    let dir_guard = TempDirGuard::new("hysteresis");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();

    let s1_sk = SigningKey::from_bytes(&[21u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[22u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[23u8; 32]);
    let anchor_nonce = [88u8; 32];
    let creator_mig_sk = svc
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
        })
        .unwrap()
        .unwrap();
    let g_mig_sk = SigningKey::from_bytes(&[24u8; 32]);

    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();
    let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p3 = l3.local_addr().unwrap().port();

    let s3_entry = make_server_entry(&s3_sk, &format!("http://127.0.0.1:{p3}"), NominatedBy::Creator);
    let manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        &format!("http://127.0.0.1:{p1}"),
        &format!("http://127.0.0.1:{p2}"),
        Some(s3_entry),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_sk.verifying_key().to_bytes()],
    );
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let tickets = make_genesis_tickets(
        anchor_id,
        manifest.retain_until_quarter,
        1_760_000_000,
        &[&s1_sk, &s2_sk],
    );
    svc.register_l2_anchor_template_with_nonce(
        manifest.clone(),
        tickets,
        Some(&anchor_nonce),
        Some(pw),
    )
    .unwrap();

    let s1_revoked = Arc::new(AtomicBool::new(false));
    let stop_servers = Arc::new(AtomicBool::new(false));
    let voted_prop: Arc<Mutex<Option<MigrationProposal>>> = Arc::new(Mutex::new(None));
    let effective_ts = 1_760_500_000u64;
    let retain_q = manifest.retain_until_quarter;

    let h1 = spawn_committee_mock_server(
        l1,
        anchor_id,
        retain_q,
        s1_sk,
        Arc::clone(&s1_revoked),
        Arc::clone(&voted_prop),
        effective_ts,
        false,
        Arc::clone(&stop_servers),
    );
    let h2 = spawn_committee_mock_server(
        l2,
        anchor_id,
        retain_q,
        s2_sk,
        Arc::new(AtomicBool::new(false)),
        Arc::clone(&voted_prop),
        effective_ts,
        false,
        Arc::clone(&stop_servers),
    );
    let h3 = spawn_committee_mock_server(
        l3,
        anchor_id,
        retain_q,
        s3_sk,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        effective_ts,
        false,
        Arc::clone(&stop_servers),
    );

    let state = AppState::new(svc);
    let session = state.begin_session_op().unwrap();
    let client = sync_client::create_sync_probe_client();

    s1_revoked.store(true, Ordering::Relaxed);
    let status1 =
        run_lifecycle_tick_with_now(&state, &session, &client, 1_760_000_050, true, Some(pw))
            .await
            .unwrap();
    assert_eq!(
        status1.anchors[0].active_rotation_phase.as_deref(),
        Some("proposed")
    );

    let status2 =
        run_lifecycle_tick_with_now(&state, &session, &client, effective_ts + 10, true, Some(pw))
            .await
            .unwrap();
    assert_eq!(status2.anchors[0].revision, 2);
    assert!(status2.anchors[0].active_rotation_phase.is_none());

    stop_and_join_mocks(stop_servers, vec![h1, h2, h3]).await;
}

#[tokio::test]
async fn test_guarantor_auto_veto_and_5d_hysteresis_timing() {
    // Invariant: guarantors automatically broadcast vetoes for protection-reducing proposals and record the exact veto reason.
    let dir_guard = TempDirGuard::new("veto");
    let pw = "pw";
    let (mut svc, _) = test_utils::setup_service_with_profile(
        &dir_guard.path_buf(),
        &ACTORS.bob,
        "BobGuarantor",
        pw,
    );
    svc.unlock_session(pw, 600).unwrap();

    let s1_sk = SigningKey::from_bytes(&[31u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[32u8; 32]);
    let s_evil_sk = SigningKey::from_bytes(&[33u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[34u8; 32]);
    let ctx_nonce = [44u8; 32];
    let g_mig_pub = svc
        .with_wallet_and_identity(|_, id| {
            l2_rotation::derive_guarantor_mig_signing_key(id, &ctx_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();

    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();
    let manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        &format!("http://127.0.0.1:{p1}"),
        &format!("http://127.0.0.1:{p2}"),
        None,
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_pub],
    );
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let anchor_hex = hex::encode(anchor_id);

    let mut bad_proposal = MigrationProposal {
        anchor_id,
        from_rev: 1,
        to_rev: 2,
        new_threshold: 2,
        new_servers: vec![
            manifest.servers[0].clone(),
            make_server_entry(&s_evil_sk, "https://evil.example", NominatedBy::Creator),
        ],
        new_successors: vec![],
        retain_until_quarter: manifest.retain_until_quarter,
        proposed_at: 1_760_000_000,
        initiator: ProposalInitiator::Creator,
        proposer_sig: None,
        guarantor_cosigs: vec![],
    };
    bad_proposal.proposer_sig =
        Some(creator_mig_sk.sign(&bad_proposal.core_digest().unwrap()).to_bytes());

    svc.update_l2_anchor_manifest(manifest, Some(pw)).unwrap();
    let mut lc = load_lifecycle_state(&mut svc, Some(pw)).unwrap();
    lc.guarantor_roles.insert(
        anchor_hex.clone(),
        StoredGuarantorRole {
            anchor_id: anchor_hex,
            guarantor_index: 0,
            context_nonce_hex: hex::encode(ctx_nonce),
        },
    );
    save_lifecycle_state(&mut svc, &lc, Some(pw)).unwrap();

    let veto_received = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let h1 = spawn_veto_mock_server(
        l1,
        anchor_id,
        s1_sk,
        bad_proposal.clone(),
        Arc::clone(&veto_received),
        Arc::clone(&stop),
    );
    let h2 = spawn_veto_mock_server(
        l2,
        anchor_id,
        s2_sk,
        bad_proposal,
        Arc::clone(&veto_received),
        Arc::clone(&stop),
    );

    let state = AppState::new(svc);
    let session = state.begin_session_op().unwrap();
    let client = sync_client::create_sync_probe_client();

    let dto = run_lifecycle_tick_with_now(&state, &session, &client, 1_760_000_050, true, Some(pw))
        .await
        .unwrap();
    assert!(veto_received.load(Ordering::Relaxed));
    assert!(dto.auto_vetoes_sent >= 1);
    assert_eq!(
        dto.last_veto_reason.as_deref(),
        Some("cannot expand creator-nominated seats without all guarantors co-signing")
    );

    stop_and_join_mocks(stop, vec![h1, h2]).await;
}

#[tokio::test]
async fn test_5day_unhealthy_hysteresis_boundary_blocks_day4_rotates_day5() {
    // Invariant: an unreachable server does not trigger rotation on Day 4 (< 5 days) and triggers Proposed on Day 5.
    let dir_guard = TempDirGuard::new("hyst5d");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();

    let s1_sk = SigningKey::from_bytes(&[51u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[52u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[53u8; 32]);
    let s4_sk = SigningKey::from_bytes(&[56u8; 32]);
    let anchor_nonce = [54u8; 32];
    let creator_mig_sk = svc
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
        })
        .unwrap()
        .unwrap();
    let g_mig_sk = SigningKey::from_bytes(&[55u8; 32]);

    let dead_l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_p = dead_l.local_addr().unwrap().port();
    drop(dead_l);

    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();
    let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p3 = l3.local_addr().unwrap().port();
    let l4 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p4 = l4.local_addr().unwrap().port();

    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![
            make_server_entry(&s1_sk, &format!("http://127.0.0.1:{dead_p}"), NominatedBy::Creator),
            make_server_entry(&s2_sk, &format!("http://127.0.0.1:{p2}"), NominatedBy::Creator),
            make_server_entry(
                &s3_sk,
                &format!("http://127.0.0.1:{p3}"),
                NominatedBy::Guarantor(0),
            ),
        ],
        successors: vec![make_server_entry(
            &s4_sk,
            &format!("http://127.0.0.1:{p4}"),
            NominatedBy::Creator,
        )],
        authority: AnchorAuthority {
            creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
            guarantor_mig_pubs: vec![g_mig_sk.verifying_key().to_bytes()],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    svc.register_l2_anchor_template_with_nonce(manifest, vec![], Some(&anchor_nonce), Some(pw))
        .unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let h2 = spawn_committee_mock_server(
        l2,
        anchor_id,
        retain_q,
        s2_sk,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        1_761_000_000,
        true,
        Arc::clone(&stop),
    );
    let h3 = spawn_committee_mock_server(
        l3,
        anchor_id,
        retain_q,
        s3_sk,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        1_761_000_000,
        true,
        Arc::clone(&stop),
    );
    let h4 = spawn_committee_mock_server(
        l4,
        anchor_id,
        retain_q,
        s4_sk,
        Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)),
        1_761_000_000,
        true,
        Arc::clone(&stop),
    );

    let state = AppState::new(svc);
    let session = state.begin_session_op().unwrap();
    let client = sync_client::create_sync_probe_client();

    let t0 = 1_760_000_000u64;
    let d0 = run_lifecycle_tick_with_now(&state, &session, &client, t0, true, Some(pw))
        .await
        .unwrap();
    assert_eq!(d0.anchors[0].unhealthy_servers.len(), 1);
    assert!(d0.anchors[0].active_rotation_phase.is_none());

    let d4 = run_lifecycle_tick_with_now(
        &state,
        &session,
        &client,
        t0 + 4 * 86_400 + 3_600,
        true,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(d4.anchors[0].active_rotation_phase.is_none());

    let d5 = run_lifecycle_tick_with_now(&state, &session, &client, t0 + 5 * 86_400, true, Some(pw))
        .await
        .unwrap();
    assert_eq!(
        d5.anchors[0].active_rotation_phase.as_deref(),
        Some("proposed")
    );

    stop_and_join_mocks(stop, vec![h2, h3, h4]).await;
}

#[test]
fn test_guarantor_claim_bundle_roundtrip() {
    // Invariant: partial Tx_claim bundle transfers verified voucher slice directly from holder to guarantor.
    let guard_issuer = TempDirGuard::new("claim-issuer");
    let guard_guar = TempDirGuard::new("claim-guarantor");
    let guard_holder = TempDirGuard::new("claim-holder");
    let pw = "pw";

    let (mut svc_issuer, _) = test_utils::setup_service_with_profile(
        &guard_issuer.path_buf(),
        &ACTORS.alice,
        "Alice",
        pw,
    );
    svc_issuer.unlock_session(pw, 600).unwrap();
    let (mut svc_guar, _) = test_utils::setup_service_with_profile(
        &guard_guar.path_buf(),
        &ACTORS.bob,
        "BobGuarantor",
        pw,
    );
    svc_guar.unlock_session(pw, 600).unwrap();
    let (mut svc_holder, _) = test_utils::setup_service_with_profile(
        &guard_holder.path_buf(),
        &ACTORS.charlie,
        "CharlieHolder",
        pw,
    );
    svc_holder.unlock_session(pw, 600).unwrap();

    let issuer_id = svc_issuer
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let guar_id = svc_guar
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let holder_id = svc_holder
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let (std_def, _) = (*test_utils::MINUTO_STANDARD).clone();
    let minuto_toml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../voucher_standards/minuto_v1/standard.toml"
    ))
    .unwrap();
    let mut standards = std::collections::HashMap::new();
    standards.insert(std_def.immutable.identity.uuid.clone(), minuto_toml.clone());

    let anchor_id = [0xAAu8; 32];

    // Create and sign a guaranteed voucher (Minuto requires additional signatures -> let's use Freetaler with optional guarantor signature so 1 guarantor suffices cleanly!)
    let (ft_def, _) = (*test_utils::FREETALER_STANDARD).clone();
    let ft_toml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../voucher_standards/freetaler_v1/standard.toml"
    ))
    .unwrap();
    standards.insert(ft_def.immutable.identity.uuid.clone(), ft_toml.clone());

    let mut v_data =
        test_utils::create_minuto_voucher_data(human_money_core::models::profile::PublicProfile {
            id: Some(issuer_id.clone()),
            ..Default::default()
        });
    v_data.creator_profile.id = Some(issuer_id.clone());
    v_data.nominal_value.amount = "100.00".into();
    v_data.nominal_value.unit = ft_def.immutable.blueprint.unit.clone();
    v_data.layer2_anchor = Some(anchor_id);
    let voucher = svc_issuer
        .create_new_voucher(&ft_toml, v_data, Some(pw))
        .unwrap();
    let issuer_local_id =
        human_money_core::wallet::Wallet::calculate_local_instance_id(&voucher, &issuer_id)
            .unwrap();

    // Guarantor Bob signs
    let sig_req = svc_issuer
        .create_signing_request_bundle(
            &issuer_local_id,
            human_money_core::models::secure_container::ContainerConfig::TargetDid(
                guar_id.clone(),
                human_money_core::models::secure_container::PrivacyMode::TrialDecryption,
            ),
            None,
            Some(pw),
        )
        .unwrap();
    let opened_req = svc_guar
        .open_voucher_signing_request(&sig_req, Some(pw))
        .unwrap();
    let detached = svc_guar
        .create_detached_signature_response_bundle(
            &opened_req.voucher,
            "guarantor",
            false,
            human_money_core::models::secure_container::ContainerConfig::TargetDid(
                issuer_id.clone(),
                human_money_core::models::secure_container::PrivacyMode::TrialDecryption,
            ),
            Some(pw),
        )
        .unwrap();
    svc_issuer
        .process_and_attach_signature(&detached, &standards, None, Some(pw))
        .unwrap();

    // Issuer sends voucher to Holder
    let send_res = svc_issuer
        .create_transfer_bundle(
            human_money_core::wallet::types::MultiTransferRequest {
                recipient_id: holder_id.clone(),
                sources: vec![human_money_core::wallet::types::SourceTransfer {
                    local_instance_id: issuer_local_id,
                    amount_to_send: "100.00".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: Some(false),
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();
    let recv_res = svc_holder
        .receive_bundle(&send_res.bundle_bytes, &standards, None, Some(pw), false)
        .unwrap();
    let holder_local_id = recv_res.involved_vouchers[0].clone();

    // Partial Tx_claim (40.00 of 100.00) directly at Guarantor Bob!
    let (claim_res, _) = svc_holder
        .create_guarantor_claim_bundle(
            &holder_local_id,
            &guar_id,
            "40.00",
            &ft_toml,
            None,
            Some("partial claim".into()),
            human_money_core::L2TransferMode::Offline,
            Some(pw),
        )
        .unwrap();
    let guar_recv = svc_guar
        .receive_bundle(&claim_res.bundle_bytes, &standards, None, Some(pw), false)
        .unwrap();
    assert_eq!(guar_recv.involved_vouchers_details[0].amount, "40.00");
}

fn make_probe_info(
    lock_hex: &str,
    drop_pub_hex: &str,
    admin_sk: &SigningKey,
    revocation: Option<SyncRevocationNotice>,
) -> sync_client::SyncServerInfo {
    let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_lower = lock_hex.trim().to_ascii_lowercase();
    let write_u = "https://node.example/sync.php".to_string();
    let read_u = "https://node.example/d".to_string();
    let issued_at = 1_759_000_000u64;
    let del_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_hex,
        &lock_lower,
        &write_u,
        &read_u,
        issued_at,
    );
    sync_client::SyncServerInfo {
        drop_pub: drop_pub_hex.to_string(),
        read_url: read_u.clone(),
        register_url: None,
        channels: vec![],
        revocation,
        lock: Some(sync_client::SyncLockInfoDto {
            lock_pub: lock_hex.to_string(),
            delegation: Some(sync_client::SyncLockDelegationDto {
                admin_pub: admin_hex,
                lock_pub: lock_lower,
                write_url: write_u,
                read_url: read_u,
                issued_at,
                sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
            }),
            max_retention_quarter: 8159,
            max_inodes: 1000,
            used_inodes: 10,
            inode_warning: false,
            pow_base_bits: 0,
            allow_pow_tickets: true,
            ticket_pow_bits: 16,
            selftest: None,
        }),
    }
}

fn make_freeze_export(
    anchor_id: [u8; 32],
    sk: &SigningKey,
    retain_q: u16,
    proposal_hash: [u8; 32],
    frozen_at: u64,
) -> SnapshotExport {
    let lock_pub = sk.verifying_key().to_bytes();
    let bucket = humoco_proto::derive_bucket(&anchor_id, &lock_pub);
    let mut snap = humoco_proto::FreezeSnapshot {
        anchor_id,
        bucket,
        from_rev: 1,
        retain_until_quarter: retain_q,
        write_seq: 0,
        bucket_head: humoco_proto::crypto::compute_bucket_init_head(&bucket, retain_q),
        state_hash: humoco_proto::compute_freeze_state_hash(&anchor_id, 1, &[], &[]).unwrap(),
        permanent: true,
        proposal_hash: Some(proposal_hash),
        frozen_at,
        expires_at: None,
        lock_pub,
        sig: [0u8; 64],
    };
    snap.sig = sk.sign(&snap.digest()).to_bytes();
    SnapshotExport {
        snapshot: snap,
        entries: vec![],
        conflicts: vec![],
    }
}

#[test]
fn test_server_probe_classification_and_5d_hysteresis_state_machine() {
    let s1_sk = SigningKey::from_bytes(&[61u8; 32]);
    let s1_hex = hex::encode(s1_sk.verifying_key().as_bytes());
    let s2_sk = SigningKey::from_bytes(&[62u8; 32]);
    let wrong_hex = "ff".repeat(32);

    let now0 = 1_760_000_000u64;

    // 1. Classify probe info: healthy, mismatched lock_pub, missing lock, future/invalid/forged/active revocation
    let healthy_info = make_probe_info(&s1_hex.to_uppercase(), &s1_hex, &s1_sk, None);
    assert_eq!(
        classify_server_probe_info(&healthy_info, &s1_hex, &s1_hex, now0),
        ServerProbeOutcome::Healthy
    );

    let mismatched_info = make_probe_info(&wrong_hex, &s1_hex, &s1_sk, None);
    assert_eq!(
        classify_server_probe_info(&mismatched_info, &s1_hex, &s1_hex, now0),
        ServerProbeOutcome::Unhealthy
    );

    let mut no_lock_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, None);
    no_lock_info.lock = None;
    assert_eq!(
        classify_server_probe_info(&no_lock_info, &s1_hex, &s1_hex, now0),
        ServerProbeOutcome::Unhealthy
    );

    let future_rev =
        sync_client::sign_revocation_notice(&s1_sk, now0, now0 + 600, None, None).unwrap();
    let future_rev_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, Some(future_rev));
    assert_eq!(
        classify_server_probe_info(&future_rev_info, &s1_hex, &s1_hex, now0),
        ServerProbeOutcome::Healthy,
        "future revocation notice must not trigger early revocation"
    );

    let mut forged_rev =
        sync_client::sign_revocation_notice(&s2_sk, now0, now0, None, None).unwrap();
    forged_rev.drop_pub = s1_hex.clone();
    let forged_rev_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, Some(forged_rev));
    assert_eq!(
        classify_server_probe_info(&forged_rev_info, &s1_hex, &s1_hex, now0),
        ServerProbeOutcome::Healthy,
        "forged revocation notice must be ignored"
    );

    // H8 regression guard: attacker hijacks domain, advertises own drop_pub & self-signed revocation
    let s2_hex = hex::encode(s2_sk.verifying_key().as_bytes());
    let attacker_self_rev =
        sync_client::sign_revocation_notice(&s2_sk, now0, now0, Some("hijack".into()), None)
            .unwrap();
    let hijacked_info = make_probe_info(&s2_hex, &s2_hex, &s2_sk, Some(attacker_self_rev));
    assert_eq!(
        classify_server_probe_info(&hijacked_info, &s1_hex, &s1_hex, now0),
        ServerProbeOutcome::Unhealthy,
        "self-signed revocation under foreign drop_pub must never mark expected server as Revoked"
    );

    let active_rev =
        sync_client::sign_revocation_notice(&s1_sk, now0, now0, Some("eol".into()), None).unwrap();
    let active_rev_info = make_probe_info(&s1_hex, &s1_hex, &s1_sk, Some(active_rev.clone()));
    assert_eq!(
        classify_server_probe_info(&active_rev_info, &s1_hex, &s1_hex, now0),
        ServerProbeOutcome::Revoked
    );

    // 2. Hysteresis state machine: day 0 -> day 5 - 1s -> recovery resets timer -> day 5 demotes
    let mut health = StoredAnchorHealth::new("aa".repeat(32));
    let mut unhealthy = HashSet::from([s1_hex.clone()]);
    let empty_revoked = HashSet::new();

    let demoted_t0 = apply_health_observations_and_collect_demotions(
        &mut health,
        &unhealthy,
        &empty_revoked,
        now0,
    );
    assert!(demoted_t0.is_empty());
    assert_eq!(health.unhealthy_since_by_lock_pub.get(&s1_hex), Some(&now0));

    // 4d 23h 59m 59s later -> still under 5d threshold, original `since` preserved
    let demoted_before_5d = apply_health_observations_and_collect_demotions(
        &mut health,
        &unhealthy,
        &empty_revoked,
        now0 + UNHEALTHY_HYSTERESIS_SECS - 1,
    );
    assert!(demoted_before_5d.is_empty());
    assert_eq!(health.unhealthy_since_by_lock_pub.get(&s1_hex), Some(&now0));

    // Server recovers for one tick -> unhealthy timestamp must clear!
    unhealthy.clear();
    let demoted_recovered = apply_health_observations_and_collect_demotions(
        &mut health,
        &unhealthy,
        &empty_revoked,
        now0 + UNHEALTHY_HYSTERESIS_SECS - 1,
    );
    assert!(demoted_recovered.is_empty());
    assert!(!health.unhealthy_since_by_lock_pub.contains_key(&s1_hex));

    // Fails again at t_fail2 -> new 5-day clock starts
    let t_fail2 = now0 + UNHEALTHY_HYSTERESIS_SECS;
    unhealthy.insert(s1_hex.clone());
    assert!(apply_health_observations_and_collect_demotions(
        &mut health,
        &unhealthy,
        &empty_revoked,
        t_fail2,
    )
    .is_empty());
    assert_eq!(
        health.unhealthy_since_by_lock_pub.get(&s1_hex),
        Some(&t_fail2)
    );

    // Exactly 5 days after t_fail2 -> demoted!
    let demoted_day5 = apply_health_observations_and_collect_demotions(
        &mut health,
        &unhealthy,
        &empty_revoked,
        t_fail2 + UNHEALTHY_HYSTERESIS_SECS,
    );
    assert!(demoted_day5.contains(&s1_sk.verifying_key().to_bytes()));

    // 3. Immediate revocation bypasses 5-day hysteresis via apply_revocation_notices_to_health
    let mut rev_health = StoredAnchorHealth::new("bb".repeat(32));
    let expected_servers = vec![ServerEntry {
        write_url: "https://s1.example/sync.php".into(),
        read_url: "https://s1.example/d".into(),
        admin_pub: s1_sk.verifying_key().to_bytes(),
        lock_pub: s1_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    }];
    let s2_wrong_notice =
        sync_client::sign_revocation_notice(&s2_sk, now0, now0, None, None).unwrap();
    apply_revocation_notices_to_health(
        &mut rev_health,
        &[
            (s1_sk.verifying_key().to_bytes(), s2_wrong_notice),
            (s1_sk.verifying_key().to_bytes(), active_rev),
        ],
        &expected_servers,
        &HashSet::new(),
        now0,
    );
    assert!(rev_health.revoked_lock_pubs.contains(&s1_hex));
    let demoted_rev = apply_health_observations_and_collect_demotions(
        &mut rev_health,
        &HashSet::new(),
        &HashSet::new(),
        now0,
    );
    assert!(demoted_rev.contains(&s1_sk.verifying_key().to_bytes()));
}

fn rotation_test_fixture() -> (
    SigningKey,
    SigningKey,
    SigningKey,
    SigningKey,
    ServerEntry,
    AnchorManifest,
    [u8; 32],
    u64,
) {
    let s1_sk = SigningKey::from_bytes(&[71u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[72u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[73u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[76u8; 32]);
    let g_mig_sk = SigningKey::from_bytes(&[77u8; 32]);
    let s3_standby = make_server_entry(
        &s3_sk,
        "https://standby.gamma.example",
        NominatedBy::Creator,
    );
    let manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        "https://srv1.alpha.example",
        "https://srv2.beta.example",
        Some(s3_standby.clone()),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_sk.verifying_key().to_bytes()],
    );
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    (
        s1_sk,
        s2_sk,
        s3_sk,
        creator_mig_sk,
        s3_standby,
        manifest,
        anchor_id,
        1_760_000_100,
    )
}

#[test]
fn test_rotation_select_proposal_and_timelock_phases() {
    // Invariant: proposal selection promotes standby or probe fallback and Phase 2 enforces timelock and valid snapshot quorum.
    let (s1_sk, s2_sk, _s3_sk, creator_mig_sk, s3_standby, manifest, anchor_id, now0) =
        rotation_test_fixture();
    let s4_sk = SigningKey::from_bytes(&[74u8; 32]);
    let bad_sk = SigningKey::from_bytes(&[75u8; 32]);

    assert!(select_rotation_proposal(
        anchor_id,
        &manifest,
        &HashSet::new(),
        Some(&creator_mig_sk),
        None,
        &SecurityPolicy::default(),
        &InternalLifecycleState::default(),
        &ServerOffenderList::new(),
        now0,
    )
    .is_none());

    let failed = HashSet::from([s1_sk.verifying_key().to_bytes()]);
    let standby_prop = select_rotation_proposal(
        anchor_id,
        &manifest,
        &failed,
        Some(&creator_mig_sk),
        None,
        &SecurityPolicy::default(),
        &InternalLifecycleState::default(),
        &ServerOffenderList::new(),
        now0,
    )
    .expect("standby successor must be promoted");
    assert_eq!(standby_prop.initiator, ProposalInitiator::HolderStandby);
    assert_eq!(standby_prop.new_servers[0], s3_standby);

    let mut manifest_no_standby = manifest.clone();
    manifest_no_standby.successors.clear();
    let mut lifecycle_with_probe = InternalLifecycleState::default();
    let s4_probe = make_signed_probe_result(
        &s4_sk,
        &s4_sk,
        "https://srv4.delta.example/sync.php",
        "https://srv4.delta.example/d",
        now0,
        manifest.retain_until_quarter,
    );
    lifecycle_with_probe
        .l2_probe_cache
        .insert(s4_probe.lock_pub.clone(), s4_probe);
    let creator_prop = select_rotation_proposal(
        anchor_id,
        &manifest_no_standby,
        &failed,
        Some(&creator_mig_sk),
        None,
        &SecurityPolicy::default(),
        &lifecycle_with_probe,
        &ServerOffenderList::new(),
        now0,
    )
    .expect("creator probe cache fallback must select non-sybil replacement");
    assert_eq!(creator_prop.initiator, ProposalInitiator::Creator);
    assert_eq!(
        creator_prop.new_servers[0].lock_pub,
        s4_sk.verifying_key().to_bytes()
    );

    assert!(compute_proposal_phase(
        &manifest,
        standby_prop.clone(),
        &[Some(now0 + 100)],
        manifest.threshold,
        now0,
    )
    .is_none());
    let StoredRotationPhase::Proposed {
        proposal: prop,
        effective_at_secs,
    } = compute_proposal_phase(
        &manifest,
        standby_prop,
        &[Some(now0 + 400), Some(now0 + 500)],
        manifest.threshold,
        now0,
    )
    .expect("threshold acks must enter Proposed phase")
    else {
        panic!("expected Proposed phase");
    };
    assert_eq!(effective_at_secs, now0 + 500);

    let d_wait = evaluate_proposed_phase_transition(
        anchor_id,
        &manifest,
        prop.clone(),
        effective_at_secs,
        vec![],
        &[],
        0,
        effective_at_secs - 1,
    )
    .unwrap();
    assert!(matches!(d_wait, ProposedPhaseDecision::WaitTimelock(_)));

    let prop_hash = prop.core_digest().unwrap();
    let exp1 = make_freeze_export(
        anchor_id,
        &s1_sk,
        manifest.retain_until_quarter,
        prop_hash,
        effective_at_secs,
    );
    let mut bad_exp = make_freeze_export(
        anchor_id,
        &bad_sk,
        manifest.retain_until_quarter,
        prop_hash,
        effective_at_secs,
    );
    bad_exp.snapshot.sig = [9u8; 64];
    let d_await_snaps = evaluate_proposed_phase_transition(
        anchor_id,
        &manifest,
        prop.clone(),
        effective_at_secs,
        vec![exp1.clone(), exp1.clone(), bad_exp],
        &[],
        0,
        effective_at_secs,
    )
    .unwrap();
    assert!(matches!(
        d_await_snaps,
        ProposedPhaseDecision::AwaitSnapshots(_)
    ));

    let exp2 = make_freeze_export(
        anchor_id,
        &s2_sk,
        manifest.retain_until_quarter,
        prop_hash,
        effective_at_secs,
    );
    let d_ready = evaluate_proposed_phase_transition(
        anchor_id,
        &manifest,
        prop,
        effective_at_secs,
        vec![exp1, exp2],
        &[],
        0,
        effective_at_secs,
    )
    .unwrap();
    assert!(matches!(
        d_ready,
        ProposedPhaseDecision::ReadyToImport { .. }
    ));
}

#[test]
fn test_rotation_import_ack_and_commit_transition() {
    // Invariant: ImportAck requires matching expected_state_hash and commit phase requires distinct valid threshold sigs.
    let (s1_sk, s2_sk, s3_sk, _, _, manifest, anchor_id, now0) = rotation_test_fixture();
    let prop = l2_rotation::build_holder_standby_proposal(
        anchor_id,
        &manifest,
        &[s1_sk.verifying_key().to_bytes()],
        now0,
    )
    .unwrap();
    let prop_hash = prop.core_digest().unwrap();
    let effective_at_secs = now0 + 500;
    let exp1 = make_freeze_export(
        anchor_id,
        &s1_sk,
        manifest.retain_until_quarter,
        prop_hash,
        effective_at_secs,
    );
    let exp2 = make_freeze_export(
        anchor_id,
        &s2_sk,
        manifest.retain_until_quarter,
        prop_hash,
        effective_at_secs,
    );
    let ProposedPhaseDecision::ReadyToImport {
        expected_state_hash,
        ..
    } = evaluate_proposed_phase_transition(
        anchor_id,
        &manifest,
        prop.clone(),
        effective_at_secs,
        vec![exp1, exp2],
        &[],
        0,
        effective_at_secs,
    )
    .unwrap()
    else {
        panic!("expected ReadyToImport");
    };

    let ack_s3 = make_signed_import_ack(
        anchor_id,
        prop_hash,
        2,
        expected_state_hash,
        manifest.retain_until_quarter,
        effective_at_secs + 1,
        &s3_sk,
    );
    let ack_s2 = make_signed_import_ack(
        anchor_id,
        prop_hash,
        2,
        expected_state_hash,
        manifest.retain_until_quarter,
        effective_at_secs + 1,
        &s2_sk,
    );
    verify_collected_import_acks(
        &[ack_s3.clone(), ack_s2.clone()],
        &anchor_id,
        &prop,
        &expected_state_hash,
    )
    .unwrap();

    let mut bad_ack = ack_s3.clone();
    bad_ack.imported_state_hash = [0xAAu8; 32];
    let bad_ack_err = verify_collected_import_acks(
        &[bad_ack, ack_s2.clone()],
        &anchor_id,
        &prop,
        &expected_state_hash,
    )
    .unwrap_err();
    assert_eq!(
        bad_ack_err.message,
        "Target server ImportAck state hash mismatch"
    );

    let sigs = make_signed_commit_sigs(
        anchor_id,
        prop_hash,
        2,
        expected_state_hash,
        effective_at_secs + 2,
        &[&s1_sk, &s2_sk],
    );
    let c_wait = evaluate_imported_commit_transition(
        &manifest,
        &anchor_id,
        prop.clone(),
        expected_state_hash,
        vec![ack_s3.clone(), ack_s2.clone()],
        vec![sigs[0].clone(), sigs[0].clone()],
    )
    .unwrap();
    assert!(matches!(c_wait, CommitPhaseDecision::AwaitCommitSigs(_)));

    let c_done = evaluate_imported_commit_transition(
        &manifest,
        &anchor_id,
        prop,
        expected_state_hash,
        vec![ack_s3, ack_s2],
        sigs,
    )
    .unwrap();
    let CommitPhaseDecision::Finalized(rev2_manifest) = c_done else {
        panic!("expected Finalized");
    };
    assert_eq!(rev2_manifest.revision, 2);
}

#[test]
fn test_apply_rotation_step_outcome_state_transitions() {
    // Invariant: transient errors and Aborted preserve health/revoked sets while Completed clears them and advances last_rotated_rev.
    let (s1_sk, s2_sk, s3_sk, _, _, manifest, anchor_id, now0) = rotation_test_fixture();
    let prop = l2_rotation::build_holder_standby_proposal(
        anchor_id,
        &manifest,
        &[s1_sk.verifying_key().to_bytes()],
        now0,
    )
    .unwrap();
    let imported_phase = StoredRotationPhase::Imported {
        proposal: prop,
        imported_state_hash_hex: "11".repeat(32),
        import_acks: vec![make_signed_import_ack(
            anchor_id,
            [2u8; 32],
            2,
            [1u8; 32],
            manifest.retain_until_quarter,
            now0 + 1,
            &s3_sk,
        )],
    };

    let mut health = StoredAnchorHealth::new(hex::encode(anchor_id));
    health
        .unhealthy_since_by_lock_pub
        .insert(hex::encode(s1_sk.verifying_key().as_bytes()), now0);
    health
        .revoked_lock_pubs
        .insert(hex::encode(s2_sk.verifying_key().as_bytes()));

    apply_rotation_step_outcome(
        &mut health,
        Ok(&RotationStepOutcome::InProgress(Box::new(imported_phase))),
        now0 + 10,
    );
    assert!(matches!(
        health.active_rotation,
        Some(StoredRotationPhase::Imported { .. })
    ));
    assert_eq!(health.last_rotated_rev, 1);

    apply_rotation_step_outcome(&mut health, Err("temporary timeout"), now0 + 20);
    assert_eq!(health.last_error.as_deref(), Some("temporary timeout"));
    assert!(matches!(
        health.active_rotation,
        Some(StoredRotationPhase::Imported { .. })
    ));

    apply_rotation_step_outcome(
        &mut health,
        Ok(&RotationStepOutcome::Aborted {
            reason: "Rotation proposal was vetoed or cleared on committee".into(),
        }),
        now0 + 25,
    );
    assert!(health.active_rotation.is_none());
    assert_eq!(
        health.last_error.as_deref(),
        Some("Rotation proposal was vetoed or cleared on committee")
    );
    assert!(!health.unhealthy_since_by_lock_pub.is_empty());
    assert!(!health.revoked_lock_pubs.is_empty());
    assert_eq!(health.last_rotated_rev, 1);

    apply_rotation_step_outcome(
        &mut health,
        Ok(&RotationStepOutcome::Completed { new_revision: 2 }),
        now0 + 30,
    );
    assert!(health.active_rotation.is_none());
    assert!(health.last_error.is_none());
    assert!(health.unhealthy_since_by_lock_pub.is_empty());
    assert!(health.revoked_lock_pubs.is_empty());
    assert_eq!(health.last_rotated_rev, 2);
}

#[test]
fn test_conflict_veto_checks_and_deduplication() {
    let s1_sk = SigningKey::from_bytes(&[81u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[82u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[83u8; 32]);
    let s_evil_sk = SigningKey::from_bytes(&[84u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[85u8; 32]);
    let g_mig_sk = SigningKey::from_bytes(&[86u8; 32]);

    let s3_standby_g0 = ServerEntry {
        write_url: "https://gstandby.gamma.example/sync.php".into(),
        read_url: "https://gstandby.gamma.example/d".into(),
        admin_pub: s3_sk.verifying_key().to_bytes(),
        lock_pub: s3_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Guarantor(0),
    };
    let manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        "https://srv1.alpha.example",
        "https://srv2.beta.example",
        Some(s3_standby_g0),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_sk.verifying_key().to_bytes()],
    );
    let anchor_id = manifest.rev1_anchor_id().unwrap();

    // 1. Authorized HolderStandby promotion of guarantor's own standby seat -> NO veto
    let ok_standby_prop = l2_rotation::build_holder_standby_proposal(
        anchor_id,
        &manifest,
        &[s2_sk.verifying_key().to_bytes()],
        1_760_000_100,
    )
    .unwrap();
    assert!(
        evaluate_polled_proposals_for_veto(&manifest, &[ok_standby_prop], 0).is_empty(),
        "authorized standby promotion of guarantor seat must not trigger veto"
    );

    // 2. Unauthorized proposal replacing guarantor seat with creator server (and reducing protection)
    let mut bad_prop = MigrationProposal {
        anchor_id,
        from_rev: 1,
        to_rev: 2,
        new_threshold: 2,
        new_servers: vec![
            manifest.servers[0].clone(),
            ServerEntry {
                write_url: "https://evil.example/sync.php".into(),
                read_url: "https://evil.example/d".into(),
                admin_pub: s_evil_sk.verifying_key().to_bytes(),
                lock_pub: s_evil_sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            },
        ],
        new_successors: vec![],
        retain_until_quarter: manifest.retain_until_quarter,
        proposed_at: 1_760_000_200,
        initiator: ProposalInitiator::Creator,
        proposer_sig: None,
        guarantor_cosigs: vec![],
    };
    let digest = bad_prop.core_digest().unwrap();
    bad_prop.proposer_sig = Some(creator_mig_sk.sign(&digest).to_bytes());

    // Duplicate identical proposals polled across multiple servers deduplicate to 1 VetoDecision
    let decisions = evaluate_polled_proposals_for_veto(&manifest, &[bad_prop.clone(), bad_prop], 0);
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].proposal_hash, digest);

    let mut lc = StoredL2LifecycleState::default();
    record_guarantor_veto_sent(&mut lc, decisions[0].reason);
    assert_eq!(lc.auto_vetoes_sent, 1);
    assert_eq!(lc.last_veto_reason.as_deref(), Some(decisions[0].reason));
}

fn make_signed_probe_result(
    admin_sk: &SigningKey,
    lock_sk: &SigningKey,
    write_url: &str,
    read_url: &str,
    issued_at: u64,
    max_retention_quarter: u16,
) -> FrontendLockServerProbeResult {
    let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
    let del_msg = humoco_market::format_lock_delegate_signature_message(
        &admin_hex, &lock_hex, write_url, read_url, issued_at,
    );
    let del = sync_client::SyncLockDelegationDto {
        admin_pub: admin_hex.clone(),
        lock_pub: lock_hex.clone(),
        write_url: write_url.to_string(),
        read_url: read_url.to_string(),
        issued_at,
        sig: hex::encode(admin_sk.sign(del_msg.as_bytes()).to_bytes()),
    };
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
        latency_ms: 5,
        max_retention_quarter,
        pow_base_bits: 16,
        allow_pow_tickets: false,
        ticket_pow_bits: 16,
        inode_warning: false,
        used_inodes: 0,
        max_inodes: 50_000,
        probed_at_secs: issued_at,
        delegation: Some(del),
        error: None,
    }
}

#[test]
fn test_relocation_by_delegation_and_revocation_notice() {
    let admin_sk = SigningKey::from_bytes(&[71u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[72u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let entry = ServerEntry {
        write_url: "https://old.host.example/sync.php".into(),
        read_url: "https://old.host.example/d".into(),
        admin_pub: admin_sk.verifying_key().to_bytes(),
        lock_pub: lock_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let lock_hex = hex::encode(entry.lock_pub);
    let mut cache = BTreeMap::new();

    // 1. Initial probe at old URL, then relocate via newer signed 5-field delegation
    let p_old = make_signed_probe_result(
        &admin_sk,
        &lock_sk,
        "https://old.host.example/sync.php",
        "https://old.host.example/d",
        100,
        retain_q,
    );
    assert!(upsert_verified_lock_probe(&mut cache, p_old));

    let p_moved1 = make_signed_probe_result(
        &admin_sk,
        &lock_sk,
        "https://moved1.host.example/sync.php",
        "https://moved1.host.example/d",
        200,
        retain_q,
    );
    assert!(upsert_verified_lock_probe(&mut cache, p_moved1));
    assert_eq!(
        resolve_server_locator(&entry, &cache),
        (
            "https://moved1.host.example/sync.php".to_string(),
            "https://moved1.host.example/d".to_string()
        )
    );

    // Stale issued_at cannot overwrite newer delegation
    let p_stale = make_signed_probe_result(
        &admin_sk,
        &lock_sk,
        "https://stale.host.example/sync.php",
        "https://stale.host.example/d",
        150,
        retain_q,
    );
    assert!(!upsert_verified_lock_probe(&mut cache, p_stale));

    // 2. Relocate via signed SyncRevocationNotice pointing to same lock_pub at new URL
    let notice = sync_client::sign_revocation_notice(
        &admin_sk,
        250,
        300,
        Some("relocated".into()),
        Some(sync_client::SyncReplacementTarget {
            write_url: "https://moved2.host.example/sync.php".into(),
            read_url: "https://moved2.host.example/d".into(),
            drop_pub: hex::encode(admin_sk.verifying_key().as_bytes()),
        }),
    )
    .unwrap();
    let p_moved2 = make_signed_probe_result(
        &admin_sk,
        &lock_sk,
        "https://moved2.host.example/sync.php",
        "https://moved2.host.example/d",
        300,
        retain_q,
    );
    let relocated = try_apply_relocation_from_notice(&mut cache, &entry, &notice, p_moved2)
        .expect("signed relocation notice must update probe cache");
    assert_eq!(relocated.write_url, "https://moved2.host.example/sync.php");
    assert_eq!(
        resolve_server_locator(&entry, &cache),
        (
            "https://moved2.host.example/sync.php".to_string(),
            "https://moved2.host.example/d".to_string()
        )
    );

    let mut health = StoredAnchorHealth::new("aa".repeat(32));
    let demotions = apply_health_observations_and_collect_demotions(
        &mut health,
        &HashSet::new(),
        &HashSet::new(),
        350,
    );
    assert!(demotions.is_empty());
    assert!(!health.revoked_lock_pubs.contains(&lock_hex));
}

fn make_public_sync_entry(sk: &SigningKey, w: &str, r: &str) -> crate::settings::StoredPublicSync {
    crate::settings::StoredPublicSync {
        write_url: w.to_string(),
        read_url: r.to_string(),
        drop_pub: hex::encode(sk.verifying_key().to_bytes()),
        register_url: None,
        access_pubkey: "11".repeat(32),
        enabled: true,
        authorized_channels: vec!["lock".to_string()],
        status: "active".to_string(),
        sunset_at: None,
        revocation_reason: None,
        replacement: None,
    }
}

#[tokio::test]
async fn test_auto_growth_n1_to_n3_and_stops_at_n3() {
    // Invariant: healthy creator-only N=1 anchor grows to N=3 (threshold=2) in one tick and stops at N=3 even when S4 is added.
    let dir_guard = TempDirGuard::new("auto_growth_n1_n3");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();

    let s1_sk = SigningKey::from_bytes(&[101u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[102u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[103u8; 32]);
    let s4_sk = SigningKey::from_bytes(&[104u8; 32]);
    let anchor_nonce = [105u8; 32];
    let creator_mig_pub = svc
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();

    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();
    let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p3 = l3.local_addr().unwrap().port();
    let l4 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p4 = l4.local_addr().unwrap().port();

    let retain_q = encode_quarter(2035, 4).unwrap();
    let now0 = 1_760_000_100u64;
    let s1 = make_server_entry(&s1_sk, &format!("http://127.0.0.1:{p1}"), NominatedBy::Creator);
    let s2 = make_server_entry(&s2_sk, &format!("http://127.0.0.1:{p2}"), NominatedBy::Creator);
    let s3 = make_server_entry(&s3_sk, &format!("http://127.0.0.1:{p3}"), NominatedBy::Creator);
    let s4 = make_server_entry(&s4_sk, &format!("http://127.0.0.1:{p4}"), NominatedBy::Creator);

    let manifest = AnchorManifest {
        revision: 1,
        threshold: 1,
        servers: vec![s1.clone()],
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs: vec![],
        },
        migration_chain: vec![],
        issued_at_ms: now0 * 1000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let tickets = make_genesis_tickets(anchor_id, retain_q, now0, &[&s1_sk]);
    svc.register_l2_anchor_template_with_nonce(
        manifest,
        tickets,
        Some(&anchor_nonce),
        Some(pw),
    )
    .unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let h1 = spawn_committee_mock_server(
        l1, anchor_id, retain_q, s1_sk.clone(), Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)), now0, false, Arc::clone(&stop),
    );
    let h2 = spawn_committee_mock_server(
        l2, anchor_id, retain_q, s2_sk.clone(), Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)), now0, false, Arc::clone(&stop),
    );
    let h3 = spawn_committee_mock_server(
        l3, anchor_id, retain_q, s3_sk.clone(), Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)), now0, false, Arc::clone(&stop),
    );
    let h4 = spawn_committee_mock_server(
        l4, anchor_id, retain_q, s4_sk.clone(), Arc::new(AtomicBool::new(false)),
        Arc::new(Mutex::new(None)), now0, false, Arc::clone(&stop),
    );

    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        state
            .mutate_security_policy(&mut s, Some(pw), |pol| {
                pol.public_syncs.push(make_public_sync_entry(&s1_sk, &s1.write_url, &s1.read_url));
                pol.public_syncs.push(make_public_sync_entry(&s2_sk, &s2.write_url, &s2.read_url));
                pol.public_syncs.push(make_public_sync_entry(&s3_sk, &s3.write_url, &s3.read_url));
            })
            .unwrap();
        let unk_sk = SigningKey::from_bytes(&[99u8; 32]);
        let probes = [
            make_signed_probe_result(&s1_sk, &s1_sk, &s1.write_url, &s1.read_url, now0, retain_q),
            make_signed_probe_result(&s2_sk, &s2_sk, &s2.write_url, &s2.read_url, now0, retain_q),
            make_signed_probe_result(&s3_sk, &s3_sk, &s3.write_url, &s3.read_url, now0, retain_q),
            make_signed_probe_result(
                &unk_sk, &unk_sk, "http://127.0.0.1:49999/sync.php",
                "http://127.0.0.1:49999/d", now0, retain_q,
            ),
        ];
        state
            .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                for p in probes {
                    upsert_verified_lock_probe(&mut lc.l2_probe_cache, p);
                }
            })
            .unwrap();
    }
    let session = state.begin_session_op().unwrap();
    let client = sync_client::create_sync_probe_client();

    let status1 = run_lifecycle_tick_with_now(&state, &session, &client, now0, true, Some(pw))
        .await
        .unwrap();
    assert_eq!(status1.anchors[0].revision, 2);
    assert!(status1.anchors[0].active_rotation_phase.is_none());
    let anchor_hex = hex::encode(anchor_id);
    let rev2_manifest = state
        .lock_service()
        .unwrap()
        .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
        .unwrap()
        .unwrap();
    assert_eq!(rev2_manifest.servers.len(), 3);
    assert_eq!(rev2_manifest.threshold, 2);

    {
        let mut s = state.lock_service().unwrap();
        state
            .mutate_security_policy(&mut s, Some(pw), |pol| {
                pol.public_syncs
                    .push(make_public_sync_entry(&s4_sk, &s4.write_url, &s4.read_url));
            })
            .unwrap();
        let p4 = make_signed_probe_result(&s4_sk, &s4_sk, &s4.write_url, &s4.read_url, now0 + 10, retain_q);
        state
            .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                upsert_verified_lock_probe(&mut lc.l2_probe_cache, p4);
            })
            .unwrap();
    }
    let status2 = run_lifecycle_tick_with_now(&state, &session, &client, now0 + 100, true, Some(pw))
        .await
        .unwrap();
    assert_eq!(status2.anchors[0].revision, 2);
    assert!(status2.anchors[0].active_rotation_phase.is_none());

    stop_and_join_mocks(stop, vec![h1, h2, h3, h4]).await;
}

#[test]
fn test_h7_delta_probe_merge_preserves_concurrent_cache_entries() {
    // Invariant: sequential delta probe updates preserve unrelated cached probe entries.
    let dir_guard = TempDirGuard::new("h7_delta_merge");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();
    let state = AppState::new(svc);
    let session = state.begin_session_op().unwrap();

    let s1_admin = SigningKey::from_bytes(&[31u8; 32]);
    let s1_lock = SigningKey::from_bytes(&[32u8; 32]);
    let s2_admin = SigningKey::from_bytes(&[33u8; 32]);
    let s2_lock = SigningKey::from_bytes(&[34u8; 32]);

    let p1 = make_signed_probe_result(
        &s1_admin, &s1_lock, "https://s1.example/sync.php", "https://s1.example/d", 100, 8159,
    );
    let p2 = make_signed_probe_result(
        &s2_admin, &s2_lock, "https://s2.example/sync.php", "https://s2.example/d", 200, 8159,
    );
    let p2_key = p2.lock_pub.clone();

    state
        .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
            upsert_verified_lock_probe(&mut lc.l2_probe_cache, p1.clone())
        })
        .unwrap();
    state
        .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
            upsert_verified_lock_probe(&mut lc.l2_probe_cache, p2.clone())
        })
        .unwrap();

    let p1_newer = make_signed_probe_result(
        &s1_admin,
        &s1_lock,
        "https://s1-new.example/sync.php",
        "https://s1-new.example/d",
        300,
        8159,
    );
    state
        .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
            upsert_verified_lock_probe(&mut lc.l2_probe_cache, p1_newer.clone())
        })
        .unwrap();

    let lc = {
        let mut s = state.service.lock().unwrap();
        state.load_internal_lifecycle(&mut s, Some(pw)).unwrap()
    };
    assert!(
        lc.l2_probe_cache.contains_key(&p2_key),
        "H7: concurrent cache entry S2 preserved"
    );
    assert_eq!(
        lc.l2_probe_cache.get(&p1.lock_pub).unwrap().write_url,
        "https://s1-new.example/sync.php"
    );
}

#[test]
fn test_s05_f1_rotation_rejects_unverified_snapshot_and_bad_commit_sig() {
    // Invariant: unverified/tampered snapshots and forged commit signatures cannot shadow valid ones or count toward threshold.
    let s1_sk = SigningKey::from_bytes(&[101u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[102u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[103u8; 32]);
    let non_member_sk = SigningKey::from_bytes(&[104u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[105u8; 32]);
    let g_mig_sk = SigningKey::from_bytes(&[106u8; 32]);

    let s3_standby = make_server_entry(&s3_sk, "https://s3.gamma.example", NominatedBy::Creator);
    let manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        "https://s1.alpha.example",
        "https://s2.beta.example",
        Some(s3_standby),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_sk.verifying_key().to_bytes()],
    );
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let now0 = 1_760_100_000u64;
    let prop = l2_rotation::build_holder_standby_proposal(
        anchor_id,
        &manifest,
        &[s1_sk.verifying_key().to_bytes()],
        now0,
    )
    .unwrap();
    let prop_hash = prop.core_digest().unwrap();

    let exp1 = make_freeze_export(anchor_id, &s1_sk, manifest.retain_until_quarter, prop_hash, now0);
    let exp2 = make_freeze_export(anchor_id, &s2_sk, manifest.retain_until_quarter, prop_hash, now0);
    let exp_non_member = make_freeze_export(
        anchor_id, &non_member_sk, manifest.retain_until_quarter, prop_hash, now0,
    );

    let d_non_member = evaluate_proposed_phase_transition(
        anchor_id, &manifest, prop.clone(), now0, vec![exp1.clone(), exp_non_member], &[], 0, now0,
    )
    .expect("non-member snapshot must be ignored and yield AwaitSnapshots");
    assert!(matches!(d_non_member, ProposedPhaseDecision::AwaitSnapshots(_)));

    let mut exp1_tampered = exp1.clone();
    exp1_tampered.entries.push("deadbeef".to_string());
    let ProposedPhaseDecision::ReadyToImport {
        expected_state_hash, ..
    } = evaluate_proposed_phase_transition(
        anchor_id,
        &manifest,
        prop.clone(),
        now0,
        vec![exp1_tampered, exp1, exp2],
        &[],
        0,
        now0,
    )
    .expect("tampered export must not shadow valid export for same lock_pub")
    else {
        panic!("expected ReadyToImport");
    };

    let ack_s3 = make_signed_import_ack(
        anchor_id, prop_hash, 2, expected_state_hash, manifest.retain_until_quarter, now0 + 1, &s3_sk,
    );
    let ack_s2 = make_signed_import_ack(
        anchor_id, prop_hash, 2, expected_state_hash, manifest.retain_until_quarter, now0 + 1, &s2_sk,
    );

    let mut sigs_good = make_signed_commit_sigs(
        anchor_id, prop_hash, 2, expected_state_hash, now0 + 2, &[&s1_sk, &s2_sk],
    );
    let cs2_good = sigs_good.pop().unwrap();
    let cs1_good = sigs_good.pop().unwrap();
    let cs1_bad = CommitteeSignature {
        sig: [0xEEu8; 64],
        ..cs1_good.clone()
    };

    let c_wait = evaluate_imported_commit_transition(
        &manifest,
        &anchor_id,
        prop.clone(),
        expected_state_hash,
        vec![ack_s3.clone(), ack_s2.clone()],
        vec![cs1_bad.clone(), cs2_good.clone()],
    )
    .expect("forged commit_sig must be filtered -> AwaitCommitSigs");
    assert!(matches!(c_wait, CommitPhaseDecision::AwaitCommitSigs(_)));

    let c_done = evaluate_imported_commit_transition(
        &manifest,
        &anchor_id,
        prop,
        expected_state_hash,
        vec![ack_s3, ack_s2],
        vec![cs1_bad, cs1_good, cs2_good],
    )
    .expect("forged sig must not block subsequent valid sig for same lock_pub");
    assert!(matches!(c_done, CommitPhaseDecision::Finalized(_)));
}

#[test]
fn test_s05_f2_max_pow_ignores_unverified_probes_and_clamps_to_max() {
    let mut lc = InternalLifecycleState::default();
    let lp_unverified = [0x11u8; 32];
    let lp_verified = [0x22u8; 32];
    let lp_unver_hex = hex::encode(lp_unverified);
    let lp_ver_hex = hex::encode(lp_verified);

    lc.l2_probe_cache.insert(
        lp_unver_hex.clone(),
        sync_client::FrontendLockServerProbeResult {
            write_url: "https://evil.example/sync.php".into(),
            read_url: "https://evil.example/d".into(),
            drop_pub: lp_unver_hex.clone(),
            admin_pub: lp_unver_hex.clone(),
            lock_pub: lp_unver_hex,
            delegation_valid: false,
            waf_probe_passed: false,
            selftest_passed: false,
            key_leak_safe: false,
            passed: false,
            latency_ms: 5,
            max_retention_quarter: 8159,
            pow_base_bits: 24,
            allow_pow_tickets: false,
            ticket_pow_bits: 16,
            inode_warning: false,
            used_inodes: 0,
            max_inodes: 1000,
            probed_at_secs: 100,
            delegation: None,
            error: None,
        },
    );
    assert_eq!(
        max_pow_base_bits_for_servers(&lc, [lp_unverified]),
        humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS,
        "unverified probe must not inflate pow_base_bits"
    );

    lc.l2_probe_cache.insert(
        lp_ver_hex.clone(),
        sync_client::FrontendLockServerProbeResult {
            write_url: "https://good.example/sync.php".into(),
            read_url: "https://good.example/d".into(),
            drop_pub: lp_ver_hex.clone(),
            admin_pub: lp_ver_hex.clone(),
            lock_pub: lp_ver_hex,
            delegation_valid: true,
            waf_probe_passed: true,
            selftest_passed: true,
            key_leak_safe: true,
            passed: true,
            latency_ms: 5,
            max_retention_quarter: 8159,
            pow_base_bits: 99,
            allow_pow_tickets: false,
            ticket_pow_bits: 16,
            inode_warning: false,
            used_inodes: 0,
            max_inodes: 1000,
            probed_at_secs: 100,
            delegation: None,
            error: None,
        },
    );
    assert_eq!(
        max_pow_base_bits_for_servers(&lc, [lp_verified]),
        humoco_proto::crypto::MAX_LOCK_POW_BITS,
        "verified probe pow_base_bits must clamp to MAX_LOCK_POW_BITS"
    );
}

#[test]
fn test_s05_f6_standby_and_probe_filtering_preserves_unsigned_successors() {
    let s1_sk = SigningKey::from_bytes(&[111u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[112u8; 32]);
    let s3_standby_sk = SigningKey::from_bytes(&[113u8; 32]);
    let g_succ_sk = SigningKey::from_bytes(&[114u8; 32]);
    let bad_probe_sk = SigningKey::from_bytes(&[115u8; 32]);
    let good_probe_sk = SigningKey::from_bytes(&[116u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[117u8; 32]);
    let g_mig_sk = SigningKey::from_bytes(&[118u8; 32]);

    let s3_standby = ServerEntry {
        write_url: "https://standby.gamma.example/sync.php".into(),
        read_url: "https://standby.gamma.example/d".into(),
        admin_pub: s3_standby_sk.verifying_key().to_bytes(),
        lock_pub: s3_standby_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let g_succ = ServerEntry {
        write_url: "https://gsucc.epsilon.example/sync.php".into(),
        read_url: "https://gsucc.epsilon.example/d".into(),
        admin_pub: g_succ_sk.verifying_key().to_bytes(),
        lock_pub: g_succ_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Guarantor(0),
    };
    let mut manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        "https://s1.alpha.example",
        "https://s2.beta.example",
        Some(s3_standby.clone()),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_sk.verifying_key().to_bytes()],
    );
    manifest.successors.push(g_succ.clone());
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let now0 = 1_760_200_000u64;
    let failed = HashSet::from([s1_sk.verifying_key().to_bytes()]);

    // (a) Revoked Creator standby MUST NOT be promoted by HolderStandby
    let mut offenders = ServerOffenderList::new();
    offenders.revoked_lock_pubs.insert(s3_standby.lock_pub);
    // Mark Guarantor's successor admin untrusted locally -> Creator rotation MUST STILL preserve g_succ in new_successors!
    let mut policy = SecurityPolicy::default();
    policy
        .lock_trust_by_admin_pub
        .insert(hex::encode(g_succ.admin_pub), false);

    // Add one probe with delegation_valid=false (must be ignored) and one valid probe
    let mut lc = InternalLifecycleState::default();
    let bad_hex = hex::encode(bad_probe_sk.verifying_key().as_bytes());
    let good_hex = hex::encode(good_probe_sk.verifying_key().as_bytes());
    lc.l2_probe_cache.insert(
        bad_hex.clone(),
        sync_client::FrontendLockServerProbeResult {
            write_url: "https://badprobe.zeta.example/sync.php".into(),
            read_url: "https://badprobe.zeta.example/d".into(),
            drop_pub: bad_hex.clone(),
            admin_pub: bad_hex.clone(),
            lock_pub: bad_hex,
            delegation_valid: false,
            waf_probe_passed: true,
            selftest_passed: true,
            key_leak_safe: true,
            passed: true,
            latency_ms: 1,
            max_retention_quarter: manifest.retain_until_quarter,
            pow_base_bits: 8,
            allow_pow_tickets: true,
            ticket_pow_bits: 16,
            inode_warning: false,
            used_inodes: 0,
            max_inodes: 1000,
            probed_at_secs: now0,
            delegation: None,
            error: None,
        },
    );
    lc.l2_probe_cache.insert(
        good_hex.clone(),
        sync_client::FrontendLockServerProbeResult {
            write_url: "https://goodprobe.eta.example/sync.php".into(),
            read_url: "https://goodprobe.eta.example/d".into(),
            drop_pub: good_hex.clone(),
            admin_pub: good_hex.clone(),
            lock_pub: good_hex,
            delegation_valid: true,
            waf_probe_passed: true,
            selftest_passed: true,
            key_leak_safe: true,
            passed: true,
            latency_ms: 50,
            max_retention_quarter: manifest.retain_until_quarter,
            pow_base_bits: 8,
            allow_pow_tickets: true,
            ticket_pow_bits: 16,
            inode_warning: false,
            used_inodes: 0,
            max_inodes: 1000,
            probed_at_secs: now0,
            delegation: None,
            error: None,
        },
    );

    let prop = select_rotation_proposal(
        anchor_id,
        &manifest,
        &failed,
        Some(&creator_mig_sk),
        None,
        &policy,
        &lc,
        &offenders,
        now0,
    )
    .expect("must fall back to Creator proposal while preserving guarantor's successor");
    assert_eq!(prop.initiator, ProposalInitiator::Creator);
    assert_eq!(
        prop.new_servers[0].lock_pub,
        good_probe_sk.verifying_key().to_bytes(),
        "must skip delegation_valid=false probe and banned standby"
    );
    assert!(
        prop.new_successors.iter().any(|s| s.same_identity(&g_succ)),
        "must preserve non-signing Guarantor successor in new_successors"
    );
    assert!(
        prop.verify_against_manifest(&anchor_id, &manifest).is_ok(),
        "resulting proposal must satisfy humoco-proto verify_against_manifest"
    );
}

#[test]
fn test_s05_f13_rogue_server_cannot_shorten_proposal_timelock() {
    let s1_sk = SigningKey::from_bytes(&[121u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[122u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[123u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[124u8; 32]);
    let g_mig_sk = SigningKey::from_bytes(&[125u8; 32]);
    let s3_standby = ServerEntry {
        write_url: "https://s3.gamma.example/sync.php".into(),
        read_url: "https://s3.gamma.example/d".into(),
        admin_pub: s3_sk.verifying_key().to_bytes(),
        lock_pub: s3_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        "https://s1.alpha.example",
        "https://s2.beta.example",
        Some(s3_standby),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_sk.verifying_key().to_bytes()],
    );
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let now0 = 1_760_300_000u64;
    let prop = l2_rotation::build_holder_standby_proposal(
        anchor_id,
        &manifest,
        &[s1_sk.verifying_key().to_bytes()],
        now0,
    )
    .unwrap();

    // 1 rogue server claiming effective_at=now0+1 + 1 honest server claiming now0+500 -> must use 2nd order statistic (now0+500)
    let phase_quorum = compute_proposal_phase(
        &manifest,
        prop.clone(),
        &[Some(now0 + 1), Some(now0 + 500)],
        2,
        now0,
    )
    .unwrap();
    let StoredRotationPhase::Proposed {
        effective_at_secs, ..
    } = phase_quorum
    else {
        panic!("expected Proposed");
    };
    assert_eq!(effective_at_secs, now0 + 500);

    // Only 1 server returned effective_at (< threshold=2) -> must fall back to full timelock
    let phase_partial =
        compute_proposal_phase(&manifest, prop, &[None, Some(now0 + 1)], 2, now0).unwrap();
    let StoredRotationPhase::Proposed {
        effective_at_secs: eff_partial,
        ..
    } = phase_partial
    else {
        panic!("expected Proposed");
    };
    assert_eq!(
        eff_partial,
        now0 + humoco_proto::PROPOSAL_TIMELOCK_SECS,
        "single server effective_at must not shorten timelock below threshold"
    );
}

fn make_three_server_manifest_with_standby() -> (
    SigningKey,
    SigningKey,
    SigningKey,
    SigningKey,
    AnchorManifest,
    [u8; 32],
    MigrationProposal,
) {
    let s1_sk = SigningKey::from_bytes(&[131u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[132u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[133u8; 32]);
    let s4_sk = SigningKey::from_bytes(&[134u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[135u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![
            make_server_entry(&s1_sk, "https://s1.example", NominatedBy::Creator),
            make_server_entry(&s2_sk, "https://s2.example", NominatedBy::Creator),
            make_server_entry(&s3_sk, "https://s3.example", NominatedBy::Creator),
        ],
        successors: vec![make_server_entry(
            &s4_sk,
            "https://s4.example",
            NominatedBy::Creator,
        )],
        authority: AnchorAuthority {
            creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
            guarantor_mig_pubs: vec![],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let prop = l2_rotation::build_holder_standby_proposal(
        anchor_id,
        &manifest,
        &[s3_sk.verifying_key().to_bytes()],
        1_760_400_000,
    )
    .unwrap();
    (s1_sk, s2_sk, s3_sk, s4_sk, manifest, anchor_id, prop)
}

#[test]
fn test_s05_f1_byzantine_export_filtered_out() {
    // Invariant: 1 Byzantine export among 3 (threshold=2) is filtered out when causal PoW check fails.
    let (s1_sk, s2_sk, s3_sk, _, manifest, anchor_id, prop) =
        make_three_server_manifest_with_standby();
    let retain_q = manifest.retain_until_quarter;
    let now0 = 1_760_400_000u64;
    let prop_hash = prop.core_digest().unwrap();

    let exp1 = make_freeze_export(anchor_id, &s1_sk, retain_q, prop_hash, now0);
    let exp2 = make_freeze_export(anchor_id, &s2_sk, retain_q, prop_hash, now0);
    let bogus_entry = LockEntry {
        slot: [0x99u8; 32],
        parent_slot: [0u8; 32],
        output_branch: 0,
        depth: 0,
        child_lock: [0x88u8; 32],
        sender_pubkey: [0x77u8; 32],
        receiver_pubkey_hash: [0x66u8; 32],
        change_pubkey_hash: None,
        causal_sig: [0x55u8; 64],
        root_mig_sig: Some([0x44u8; 64]),
        encrypted_payload: [0x33u8; 265],
        pow_nonce: 0,
    };
    let s3_pub = s3_sk.verifying_key().to_bytes();
    let mut s3_snap = humoco_proto::FreezeSnapshot {
        anchor_id,
        bucket: humoco_proto::derive_bucket(&anchor_id, &s3_pub),
        from_rev: 1,
        retain_until_quarter: retain_q,
        write_seq: 1,
        bucket_head: [0x22u8; 32],
        state_hash: humoco_proto::compute_freeze_state_hash(
            &anchor_id,
            1,
            std::slice::from_ref(&bogus_entry),
            &[],
        )
        .unwrap(),
        permanent: true,
        proposal_hash: Some(prop_hash),
        frozen_at: now0,
        expires_at: None,
        lock_pub: s3_pub,
        sig: [0u8; 64],
    };
    s3_snap.sig = s3_sk.sign(&s3_snap.digest()).to_bytes();
    let exp3_byzantine = SnapshotExport {
        snapshot: s3_snap,
        entries: vec![bogus_entry.to_wire_hex().unwrap()],
        conflicts: vec![],
    };

    let dec = evaluate_proposed_phase_transition(
        anchor_id,
        &manifest,
        prop,
        now0,
        vec![exp1, exp2, exp3_byzantine],
        &[],
        DEFAULT_LOCK_POW_BASE_BITS,
        now0,
    )
    .expect("Byzantine export must be filtered out when >= threshold honest exports remain");
    let ProposedPhaseDecision::ReadyToImport {
        verified_exports, ..
    } = dec
    else {
        panic!("expected ReadyToImport");
    };
    assert_eq!(verified_exports.len(), 2);
    assert!(verified_exports
        .iter()
        .all(|e| e.snapshot.lock_pub != s3_pub));
}

#[test]
fn test_s05_f1_import_ack_verification_rejects_forgeries_and_duplicates() {
    // Invariant: verify_single_import_ack and verify_collected_import_acks reject bad sigs, lock_pub mismatches, duplicates, and wrong sets.
    let (s1_sk, s2_sk, _s3_sk, s4_sk, manifest, anchor_id, prop) =
        make_three_server_manifest_with_standby();
    let now0 = 1_760_400_000u64;
    let prop_hash = prop.core_digest().unwrap();
    let expected_state_hash = [0x42u8; 32];

    let a1 = make_signed_import_ack(
        anchor_id, prop_hash, prop.to_rev, expected_state_hash, manifest.retain_until_quarter, now0 + 1, &s1_sk,
    );
    let a2 = make_signed_import_ack(
        anchor_id, prop_hash, prop.to_rev, expected_state_hash, manifest.retain_until_quarter, now0 + 1, &s2_sk,
    );
    let a4 = make_signed_import_ack(
        anchor_id, prop_hash, prop.to_rev, expected_state_hash, manifest.retain_until_quarter, now0 + 1, &s4_sk,
    );
    verify_collected_import_acks(
        &[a1.clone(), a2.clone(), a4.clone()],
        &anchor_id,
        &prop,
        &expected_state_hash,
    )
    .unwrap();

    let mut a4_bad_sig = a4;
    a4_bad_sig.sig[0] ^= 0xFF;
    let sig_err = verify_single_import_ack(
        &a4_bad_sig,
        &s4_sk.verifying_key().to_bytes(),
        &anchor_id,
        &prop,
        &expected_state_hash,
    )
    .unwrap_err();
    assert_eq!(
        sig_err.message,
        "Target server ImportAck failed Ed25519 signature verification"
    );

    let key_err = verify_single_import_ack(
        &a1,
        &s4_sk.verifying_key().to_bytes(),
        &anchor_id,
        &prop,
        &expected_state_hash,
    )
    .unwrap_err();
    assert_eq!(key_err.message, "Target server ImportAck lock_pub mismatch");

    let dup_err = verify_collected_import_acks(
        &[a1.clone(), a1.clone(), a2.clone()],
        &anchor_id,
        &prop,
        &expected_state_hash,
    )
    .unwrap_err();
    assert_eq!(
        dup_err.message,
        "Duplicate ImportAck from same target server"
    );

    let count_err =
        verify_collected_import_acks(&[a1, a2], &anchor_id, &prop, &expected_state_hash)
            .unwrap_err();
    assert_eq!(
        count_err.message,
        "ImportAck set does not match proposal.new_servers count"
    );
}

fn spawn_freeze_status_server(
    listener: tokio::net::TcpListener,
    anchor_hex: String,
    rev2_manifest: AnchorManifest,
    now0: u64,
    serve_rev2: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while !stop.load(Ordering::Relaxed) {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept()).await
            else {
                continue;
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or("");
            let resp = if first_line.contains("lock_freeze") {
                let active_manifest = if serve_rev2.load(Ordering::Relaxed) {
                    serde_json::to_value(&rev2_manifest).unwrap()
                } else {
                    serde_json::Value::Null
                };
                let s = serde_json::json!({
                    "anchor_id": anchor_hex,
                    "bucket": "00".repeat(32),
                    "active_manifest": active_manifest,
                    "last_creator_activity_secs": now0,
                    "bucket_freeze_state": "active",
                    "voted_proposal": serde_json::Value::Null,
                    "snapshot_export": serde_json::Value::Null,
                })
                .to_string();
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    s.len(),
                    s
                )
            } else {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            };
            write_http_response(&mut stream, resp.as_bytes()).await;
        }
    })
}

#[tokio::test]
async fn test_s05_f3_vetoed_committee_aborts_preserving_health_and_active_manifest_completes() {
    // Invariant: active committee with no proposal aborts rotation preserving health/revocations, while active Rev 2 completes atomically.
    let dir_guard = TempDirGuard::new("s05_f3_veto_abort");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();

    let s1_sk = SigningKey::from_bytes(&[141u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[142u8; 32]);
    let s3_sk = SigningKey::from_bytes(&[143u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[144u8; 32]);
    let g_mig_sk = SigningKey::from_bytes(&[145u8; 32]);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let s3_standby = make_server_entry(&s3_sk, &format!("{base_url}/s3"), NominatedBy::Creator);
    let manifest = make_test_manifest(
        &s1_sk,
        &s2_sk,
        &format!("{base_url}/s1"),
        &format!("{base_url}/s2"),
        Some(s3_standby),
        creator_mig_sk.verifying_key().to_bytes(),
        vec![g_mig_sk.verifying_key().to_bytes()],
    );
    let now0 = 1_760_500_000u64;
    let (anchor_id, prop, rev2_manifest) =
        build_migrated_rev2_manifest(&manifest, &s1_sk, &s2_sk, &s3_sk, now0);
    let anchor_hex = hex::encode(anchor_id);
    let tickets =
        make_genesis_tickets(anchor_id, manifest.retain_until_quarter, now0, &[&s1_sk, &s2_sk]);
    svc.register_l2_anchor_template(manifest, tickets, Some(pw))
        .unwrap();

    let serve_rev2 = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let server = spawn_freeze_status_server(
        listener,
        anchor_hex.clone(),
        rev2_manifest,
        now0,
        Arc::clone(&serve_rev2),
        Arc::clone(&stop),
    );

    let state = AppState::new(svc);
    let session = {
        let mut s = state.service.lock().unwrap();
        let mut lc = StoredL2LifecycleState::default();
        let mut health = StoredAnchorHealth::new(anchor_hex.clone());
        let s1_hex = hex::encode(s1_sk.verifying_key().as_bytes());
        health.unhealthy_since_by_lock_pub.insert(s1_hex.clone(), now0 - 10);
        health.revoked_lock_pubs.insert(s1_hex);
        health.active_rotation = Some(StoredRotationPhase::Proposed {
            proposal: prop.clone(),
            effective_at_secs: now0,
        });
        lc.anchors.insert(anchor_hex.clone(), health);
        save_lifecycle_state(&mut s, &lc, Some(pw)).unwrap();
        state.session_token_for_service(&s).unwrap()
    };

    let client = sync_client::create_sync_probe_client();
    let status_abort =
        run_lifecycle_tick_with_now(&state, &session, &client, now0 + 5, true, Some(pw))
            .await
            .unwrap();
    let a_abort = status_abort
        .anchors
        .iter()
        .find(|a| a.anchor_id == anchor_hex)
        .unwrap();
    assert!(a_abort.active_rotation_phase.is_none());
    assert!(a_abort
        .last_error
        .as_deref()
        .unwrap_or("")
        .contains("vetoed or cleared"));
    assert_eq!(a_abort.unhealthy_servers.len(), 1);
    assert_eq!(a_abort.revoked_servers.len(), 1);
    assert_eq!(a_abort.revision, 1);

    serve_rev2.store(true, Ordering::Relaxed);
    {
        let mut s = state.service.lock().unwrap();
        let mut lc = load_lifecycle_state(&mut s, Some(pw)).unwrap();
        lc.anchors.get_mut(&anchor_hex).unwrap().active_rotation =
            Some(StoredRotationPhase::Proposed {
                proposal: prop,
                effective_at_secs: now0,
            });
        save_lifecycle_state(&mut s, &lc, Some(pw)).unwrap();
    }
    let status_done =
        run_lifecycle_tick_with_now(&state, &session, &client, now0 + 10, true, Some(pw))
            .await
            .unwrap();
    let a_done = status_done
        .anchors
        .iter()
        .find(|a| a.anchor_id == anchor_hex)
        .unwrap();
    assert_eq!(a_done.revision, 2);
    assert!(a_done.active_rotation_phase.is_none());
    assert!(a_done.last_error.is_none());
    assert!(a_done.unhealthy_servers.is_empty());
    assert!(a_done.revoked_servers.is_empty());

    stop_and_join_mocks(stop, vec![server]).await;
}

#[tokio::test]
async fn test_s05_f7_cashier_receive_only_blocked_from_background_tick() {
    let dir_guard = TempDirGuard::new("s05_f7_receive_only");
    let pw = "pw-full";
    let cashier_pin = "pin-cashier-1234";
    let (mut svc, profile_info) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();
    svc.enable_secondary_auth(
        pw,
        cashier_pin,
        human_money_core::SecondaryScope::ReceiveOnly,
    )
    .unwrap();
    svc.logout();
    let granted = svc
        .login_with_secondary(
            &profile_info.folder_name,
            cashier_pin,
            true,
            "test-id".into(),
        )
        .unwrap();
    assert_eq!(granted, human_money_core::AccessMode::ReceiveOnly);
    svc.unlock_secondary_session(cashier_pin, 600).unwrap();

    let state = AppState::new(svc);
    // Background tick MUST skip ReceiveOnly session without advancing last_tick_secs
    maybe_run_background_lifecycle_tick(&state).await;
    let last_tick = {
        let mut s = state.service.lock().unwrap();
        load_lifecycle_state(&mut s, None).unwrap().last_tick_secs
    };
    assert_eq!(
        last_tick, 0,
        "S05-F7: ReceiveOnly session must not execute background lifecycle tick"
    );
}

#[test]
fn test_s05_f9_in_place_delegation_refresh_validation() {
    let admin_sk = SigningKey::from_bytes(&[151u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[152u8; 32]);
    let other_sk = SigningKey::from_bytes(&[153u8; 32]);
    let srv = ServerEntry {
        write_url: "https://old.example/sync.php".into(),
        read_url: "https://old.example/d".into(),
        admin_pub: admin_sk.verifying_key().to_bytes(),
        lock_pub: lock_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let valid_probe = make_signed_probe_result(
        &admin_sk,
        &lock_sk,
        "https://new.example/sync.php",
        "https://new.example/d",
        200,
        8159,
    );
    let valid_del = valid_probe.delegation.clone().unwrap();
    let mut offenders = ServerOffenderList::new();

    assert!(validate_in_place_delegation_refresh(
        &srv,
        &valid_del,
        &valid_probe,
        &offenders
    ));

    // Failed probe rejected
    let mut failed_probe = valid_probe.clone();
    failed_probe.passed = false;
    assert!(!validate_in_place_delegation_refresh(
        &srv,
        &valid_del,
        &failed_probe,
        &offenders
    ));

    // Mismatched lock_pub rejected
    let mut wrong_lock_probe = valid_probe.clone();
    wrong_lock_probe.lock_pub = hex::encode(other_sk.verifying_key().as_bytes());
    assert!(!validate_in_place_delegation_refresh(
        &srv,
        &valid_del,
        &wrong_lock_probe,
        &offenders
    ));

    // Mismatched write_url rejected
    let mut wrong_url_probe = valid_probe.clone();
    wrong_url_probe.write_url = "https://attacker.example/sync.php".into();
    assert!(!validate_in_place_delegation_refresh(
        &srv,
        &valid_del,
        &wrong_url_probe,
        &offenders
    ));

    // Private/loopback read_url rejected
    let mut bad_read_del = valid_del.clone();
    bad_read_del.read_url = "http://169.254.169.254/latest".into();
    assert!(!validate_in_place_delegation_refresh(
        &srv,
        &bad_read_del,
        &valid_probe,
        &offenders
    ));

    // Revoked server on ServerOffenderList rejected
    offenders.revoked_lock_pubs.insert(srv.lock_pub);
    assert!(!validate_in_place_delegation_refresh(
        &srv,
        &valid_del,
        &valid_probe,
        &offenders
    ));
}

#[test]
fn test_s05_f11_record_equivocation_and_refresh_cache_revokes_admin_pub_and_updates_cache() {
    let dir_guard = TempDirGuard::new("s05_f11_equiv_delegation");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();

    let admin_sk = SigningKey::from_bytes(&[161u8; 32]);
    let lock_sk = SigningKey::from_bytes(&[162u8; 32]);
    let admin_pub = admin_sk.verifying_key().to_bytes();
    let lock_pub = lock_sk.verifying_key().to_bytes();

    let probe = make_signed_probe_result(
        &admin_sk,
        &lock_sk,
        "https://equiv.example/sync.php",
        "https://equiv.example/d",
        100,
        8159,
    );
    let mut probe_cache = BTreeMap::new();
    probe_cache.insert(probe.lock_pub.clone(), probe);

    let make_rcpt = |child: [u8; 32], eh: [u8; 32]| {
        let mut r = LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub,
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
        r.sig = lock_sk.sign(&r.digest()).to_bytes();
        r
    };
    let eq = EquivocationProof {
        receipt_a: make_rcpt([0xAAu8; 32], [0x11u8; 32]),
        receipt_b: make_rcpt([0xBBu8; 32], [0x22u8; 32]),
    };

    let state = AppState::new(svc);
    {
        let mut s = state.lock_service().unwrap();
        let newly = record_equivocation_and_refresh_cache(
            &state,
            &mut s,
            &probe_cache,
            &eq,
            1_760_000_000,
            Some(pw),
        )
        .unwrap();
        assert!(newly);

        // Lock session to verify ActiveProfileCaches holds the refreshed ServerOffenderList!
        s.lock_session();
        let cached = state
            .get_or_load_server_offender_list(&mut s, None)
            .unwrap();
        assert!(
            cached.is_lock_pub_revoked(&lock_pub),
            "S05-F11: lock_pub must be revoked in refreshed cache"
        );
        assert!(
            cached.is_admin_pub_revoked(&admin_pub),
            "S05-F11: ADR-10 admin_pub must be revoked via probe_cache delegation"
        );
    }
}

#[test]
fn test_s05_f13_quorum_creator_activity_resists_byzantine_outlier() {
    let now = 1_800_000_000u64;
    let baseline = 1_700_000_000u64;
    let t_old = 1_750_000_000u64;

    // 1 Byzantine server reporting u64::MAX among 3 (threshold=2) cannot mask inactivity
    assert_eq!(
        compute_quorum_creator_activity_secs(baseline, &[t_old, t_old, u64::MAX], 2, now),
        t_old
    );
    // 1 stale server reporting 0 among 3 (threshold=2) cannot forge premature inactivity
    assert_eq!(
        compute_quorum_creator_activity_secs(baseline, &[0, now - 10, now - 10], 2, now),
        now - 10
    );
    // Future timestamp clamped to <= now even when < threshold responses arrive
    assert_eq!(
        compute_quorum_creator_activity_secs(baseline, &[u64::MAX], 2, now),
        now
    );
    // Zero threshold must be panic-free
    assert_eq!(
        compute_quorum_creator_activity_secs(baseline, &[t_old], 0, now),
        t_old
    );
}

#[test]
fn test_s04_s05_same_child_lock_distinct_entry_hash_and_dedup() {
    let dir_guard = TempDirGuard::new("s04_s05_equiv_dedup");
    let pw = "pw";
    let (mut svc, _) =
        test_utils::setup_service_with_profile(&dir_guard.path_buf(), &ACTORS.alice, "Alice", pw);
    svc.unlock_session(pw, 600).unwrap();

    let make_rcpt = |sk: &SigningKey, child: [u8; 32], eh: [u8; 32]| {
        let mut r = LockReceipt {
            kind: LockReceiptKind::FirstWrite,
            lock_pub: sk.verifying_key().to_bytes(),
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
        r.sig = sk.sign(&r.digest()).to_bytes();
        r
    };

    let make_wot_del = |admin_sk: &SigningKey, lock_pub: &[u8; 32], url: &str| {
        let probe = make_signed_probe_result(
            admin_sk,
            &SigningKey::from_bytes(lock_pub), // unused for verifying key here; let's build directly
            url,
            "https://equiv.example/d",
            100,
            8159,
        );
        let mut del = sync_delegation_to_wot(probe.delegation.as_ref().unwrap()).unwrap();
        del.lock_pub = *lock_pub;
        let msg = del.canonical_message();
        del.sig = admin_sk.sign(msg.as_bytes()).to_bytes();
        del
    };

    // 1. Same child_lock + distinct entry_hash must succeed and revoke lock_pub & admin_pub
    let admin_sk1 = SigningKey::from_bytes(&[171u8; 32]);
    let lock_sk1 = SigningKey::from_bytes(&[172u8; 32]);
    let admin_pub1 = admin_sk1.verifying_key().to_bytes();
    let lock_pub1 = lock_sk1.verifying_key().to_bytes();
    let del1 = make_wot_del(&admin_sk1, &lock_pub1, "https://equiv1.example/sync.php");
    let eq_same_child = EquivocationProof {
        receipt_a: make_rcpt(&lock_sk1, [0xCCu8; 32], [0x11u8; 32]),
        receipt_b: make_rcpt(&lock_sk1, [0xCCu8; 32], [0x22u8; 32]),
    };

    let newly1 = record_equivocation_proof(
        &mut svc,
        Some(del1.clone()),
        &eq_same_child,
        1_760_000_000,
        Some(pw),
    )
    .expect("S04: same child_lock with distinct entry_hash must be accepted");
    assert!(newly1);

    let list1 = load_server_offender_list(&mut svc, Some(pw)).unwrap();
    assert!(list1.is_lock_pub_revoked(&lock_pub1));
    assert!(list1.is_admin_pub_revoked(&admin_pub1));
    assert_eq!(list1.records.len(), 1);

    // 2. Repeated recording with same lock_pub + admin_pub must deduplicate cleanly
    let newly_dup = record_equivocation_proof(
        &mut svc,
        Some(del1),
        &eq_same_child,
        1_760_000_005,
        Some(pw),
    )
    .unwrap();
    assert!(
        !newly_dup,
        "S05-2: duplicate equivocation must return newly=false"
    );
    let list_after_dup = load_server_offender_list(&mut svc, Some(pw)).unwrap();
    assert_eq!(
        list_after_dup.records.len(),
        1,
        "S05-2: ServerOffenderList must not append duplicate records"
    );

    // 3. First recording without delegation, then a second call newly supplying verified delegation
    let admin_sk2 = SigningKey::from_bytes(&[173u8; 32]);
    let lock_sk2 = SigningKey::from_bytes(&[174u8; 32]);
    let admin_pub2 = admin_sk2.verifying_key().to_bytes();
    let lock_pub2 = lock_sk2.verifying_key().to_bytes();
    let eq2 = EquivocationProof {
        receipt_a: make_rcpt(&lock_sk2, [0xAAu8; 32], [0x31u8; 32]),
        receipt_b: make_rcpt(&lock_sk2, [0xBBu8; 32], [0x32u8; 32]),
    };
    assert!(record_equivocation_proof(&mut svc, None, &eq2, 1_760_000_010, Some(pw)).unwrap());
    assert!(!record_equivocation_proof(&mut svc, None, &eq2, 1_760_000_011, Some(pw)).unwrap());
    let del2 = make_wot_del(&admin_sk2, &lock_pub2, "https://equiv2.example/sync.php");
    assert!(
        record_equivocation_proof(&mut svc, Some(del2.clone()), &eq2, 1_760_000_012, Some(pw))
            .unwrap(),
        "S05-2: newly supplied verified delegation must update revoked_admin_pubs"
    );
    assert!(
        !record_equivocation_proof(&mut svc, Some(del2), &eq2, 1_760_000_013, Some(pw)).unwrap()
    );
    let list_final = load_server_offender_list(&mut svc, Some(pw)).unwrap();
    assert!(list_final.is_lock_pub_revoked(&lock_pub2));
    assert!(list_final.is_admin_pub_revoked(&admin_pub2));
    assert_eq!(list_final.records.len(), 3);
}
