use super::execute::*;
use super::precommit::*;
use super::trusted_keys::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::state::canonicalize_did;
use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS,
    decode_hex_fixed,
    encode_quarter,
    manifest::{AnchorAuthority, AnchorManifest, GenesisTicket, NominatedBy, ServerEntry},
    rotation::{
        CommitteeSignature, ImportAck, MigrationCert, MigrationProposal, ProposalInitiator,
    },
    types::{LockReceipt, LockReceiptKind},
};
use human_money_core::models::conflict::{ConflictRole, ProofOfDoubleSpend, ProofStoreEntry};
use human_money_core::models::layer2_api::{
    LockEntry, SyncLockBatchRequest, SyncLockBatchResponse, SyncLockConflict,
};
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{NewVoucherData, ValueDefinition, Voucher};
use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
use human_money_core::services::{bundle_processor, l2_gateway};
use human_money_core::test_utils::{self, TestUser, ACTORS};
use human_money_core::wallet::types::{MultiTransferRequest, SourceTransfer};
use human_money_core::VoucherStatus;

use crate::{
    settings::{InternalLifecycleState, SecurityPolicy},
    sync_client, AppState,
};
use ed25519_dalek::{Signer, SigningKey};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(prefix: &str) -> Self {
        Self(std::env::temp_dir().join(format!("{}-{}", prefix, uuid::Uuid::new_v4())))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

struct MockServerHandle {
    port: u16,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for MockServerHandle {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let target = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct TwoPartyEnv {
    svc_alice: human_money_core::app_service::AppService,
    state_bob: AppState,
    bob_session: crate::session::SessionToken,
    alice_id: String,
    bob_id: String,
    std_toml: String,
    standards: HashMap<String, String>,
    pw: &'static str,
    dir_alice: TempDirGuard,
    _dir_bob: TempDirGuard,
}

impl TwoPartyEnv {
    fn new(prefix: &str) -> Self {
        let dir_alice = TempDirGuard::new(&format!("{}-alice", prefix));
        let dir_bob = TempDirGuard::new(&format!("{}-bob", prefix));
        let pw = "test-password";

        let (mut svc_alice, _) =
            test_utils::setup_service_with_profile(dir_alice.path(), &ACTORS.alice, "Alice", pw);
        let (mut svc_bob, _) =
            test_utils::setup_service_with_profile(dir_bob.path(), &ACTORS.bob, "Bob", pw);
        svc_alice.unlock_session(pw, 600).unwrap();
        svc_bob.unlock_session(pw, 600).unwrap();

        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        let std_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
        let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

        let state_bob = AppState::new(svc_bob);
        state_bob.set_cached_ui_preferences(Some(crate::settings::UiPreferences::default()));
        state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
        state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
        let bob_session = {
            let s = state_bob.service.lock().unwrap();
            state_bob.session_token_for_service(&s).unwrap()
        };

        Self {
            svc_alice,
            state_bob,
            bob_session,
            alice_id,
            bob_id,
            std_toml,
            standards,
            pw,
            dir_alice,
            _dir_bob: dir_bob,
        }
    }

    fn issue_voucher(&mut self, amount: &str, layer2_anchor: Option<[u8; 32]>) -> (Voucher, String) {
        let voucher = self
            .svc_alice
            .create_new_voucher(
                &self.std_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(self.alice_id.clone()),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: amount.into(),
                        ..Default::default()
                    },
                    layer2_anchor,
                    ..Default::default()
                },
                Some(self.pw),
            )
            .unwrap();
        let lid = self
            .svc_alice
            .with_wallet(|w| {
                w.voucher_store
                    .vouchers
                    .values()
                    .find(|inst| inst.voucher.voucher_id == voucher.voucher_id)
                    .expect("newly created voucher must exist in alice voucher_store")
                    .local_instance_id
                    .clone()
            })
            .unwrap();
        (voucher, lid)
    }

    fn send_sources_to(&mut self, recipient_id: &str, sources: &[(&str, &str)]) -> Vec<u8> {
        self.svc_alice
            .create_transfer_bundle(
                MultiTransferRequest {
                    recipient_id: recipient_id.to_string(),
                    sources: sources
                        .iter()
                        .map(|(lid, amount)| SourceTransfer {
                            local_instance_id: (*lid).to_string(),
                            amount_to_send: (*amount).to_string(),
                        })
                        .collect(),
                    notes: None,
                    sender_profile_name: None,
                    use_privacy_mode: None,
                    wot_sync_json: None,
                    contact_card_json: None,
                },
                &self.standards,
                None,
                Some(self.pw),
            )
            .unwrap()
            .bundle_bytes
    }

    fn register_rev1_anchor(
        &mut self,
        anchor_nonce: &[u8; 32],
        retain_q: u16,
        threshold: u8,
        server_specs: &[(&SigningKey, &SigningKey, u16, NominatedBy)],
        guarantor_mig_pubs: Vec<[u8; 32]>,
    ) -> [u8; 32] {
        let (manifest, anchor_id, tickets) = make_rev1_manifest_and_tickets(
            &self.svc_alice,
            anchor_nonce,
            retain_q,
            threshold,
            server_specs,
            guarantor_mig_pubs,
        );
        self.svc_alice
            .register_l2_anchor_template_with_nonce(
                manifest,
                tickets,
                Some(anchor_nonce),
                Some(self.pw),
            )
            .unwrap();
        anchor_id
    }
}

fn make_rev1_manifest_and_tickets(
    svc_alice: &human_money_core::app_service::AppService,
    anchor_nonce: &[u8; 32],
    retain_q: u16,
    threshold: u8,
    server_specs: &[(&SigningKey, &SigningKey, u16, NominatedBy)],
    guarantor_mig_pubs: Vec<[u8; 32]>,
) -> (AnchorManifest, [u8; 32], Vec<GenesisTicket>) {
    let creator_mig_pub = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();

    let servers = server_specs
        .iter()
        .map(|(adm_sk, lock_sk, port, nominated_by)| ServerEntry {
            write_url: format!("http://127.0.0.1:{}/sync.php", port),
            read_url: format!("http://127.0.0.1:{}/d", port),
            admin_pub: adm_sk.verifying_key().to_bytes(),
            lock_pub: lock_sk.verifying_key().to_bytes(),
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
        .map(|(_, lock_sk, _, _)| {
            let mut t = GenesisTicket {
                anchor_id,
                retain_until_quarter: retain_q,
                issued_at: 1_760_000_000,
                lock_pub: lock_sk.verifying_key().to_bytes(),
                sig: [0u8; 64],
            };
            t.sig = lock_sk.sign(&t.digest()).to_bytes();
            t
        })
        .collect();
    (manifest, anchor_id, tickets)
}

fn make_rev1_to_rev2_migration(
    svc_alice: &human_money_core::app_service::AppService,
    anchor_nonce: &[u8; 32],
    retain_q: u16,
    old_sk: &SigningKey,
    old_port: u16,
    new_sk: &SigningKey,
    new_port: u16,
) -> (AnchorManifest, AnchorManifest, [u8; 32], Vec<GenesisTicket>) {
    let creator_mig_sk = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, anchor_nonce)
        })
        .unwrap()
        .unwrap();
    let srv1 = ServerEntry {
        write_url: format!("http://127.0.0.1:{}/sync.php", old_port),
        read_url: format!("http://127.0.0.1:{}/d", old_port),
        admin_pub: old_sk.verifying_key().to_bytes(),
        lock_pub: old_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let srv2 = ServerEntry {
        write_url: format!("http://127.0.0.1:{}/sync.php", new_port),
        read_url: format!("http://127.0.0.1:{}/d", new_port),
        admin_pub: new_sk.verifying_key().to_bytes(),
        lock_pub: new_sk.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let rev1 = AnchorManifest {
        revision: 1,
        threshold: 1,
        servers: vec![srv1],
        successors: vec![srv2.clone()],
        authority: AnchorAuthority {
            creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
            guarantor_mig_pubs: vec![],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = rev1.rev1_anchor_id().unwrap();
    let mut t1 = GenesisTicket {
        anchor_id,
        retain_until_quarter: retain_q,
        issued_at: 1_760_000_000,
        lock_pub: old_sk.verifying_key().to_bytes(),
        sig: [0u8; 64],
    };
    t1.sig = old_sk.sign(&t1.digest()).to_bytes();

    let mut proposal = MigrationProposal {
        anchor_id,
        from_rev: 1,
        to_rev: 2,
        new_threshold: 1,
        new_servers: vec![srv2],
        new_successors: vec![],
        retain_until_quarter: retain_q,
        proposed_at: 1_760_050_000,
        initiator: ProposalInitiator::Creator,
        proposer_sig: None,
        guarantor_cosigs: vec![],
    };
    proposal.proposer_sig = Some(
        creator_mig_sk
            .sign(&proposal.core_digest().unwrap())
            .to_bytes(),
    );
    let imported_state_hash = [0x33u8; 32];
    let prop_hash = proposal.core_digest().unwrap();
    let mut ack2 = ImportAck {
        anchor_id,
        proposal_hash: prop_hash,
        to_rev: 2,
        imported_state_hash,
        retain_until_quarter: retain_q,
        lock_pub: new_sk.verifying_key().to_bytes(),
        acked_at: 1_760_050_100,
        sig: [0u8; 64],
    };
    ack2.sig = new_sk.sign(&ack2.digest()).to_bytes();
    let c_digest = MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &imported_state_hash);
    let commit_sig = CommitteeSignature {
        lock_pub: old_sk.verifying_key().to_bytes(),
        committed_at: 1_760_050_200,
        sig: old_sk.sign(&c_digest).to_bytes(),
    };
    let cert = MigrationCert {
        anchor_id,
        genesis_manifest: Some(Box::new(rev1.clone())),
        proposal,
        imported_state_hash,
        import_acks: vec![ack2],
        commit_sigs: vec![commit_sig],
    };
    let (rev2, _) = cert.verify_step(&anchor_id, &rev1).unwrap();
    (rev1, rev2, anchor_id, vec![t1])
}

fn build_competing_lock_entry(
    dir_alice: &Path,
    svc_alice: &human_money_core::app_service::AppService,
    pw: &str,
    standards: &HashMap<String, String>,
    local_id: &str,
    victim_user: &TestUser,
    send_amount: &str,
) -> LockEntry {
    let clone_dir = TempDirGuard::new("app-chk-clone");
    copy_dir_recursive(dir_alice, clone_dir.path());
    let mut svc_clone = human_money_core::app_service::AppService::new(clone_dir.path()).unwrap();
    let folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
    svc_clone
        .login(&folder, pw, false, "test-id".into())
        .unwrap();
    let alt_bundle = svc_clone
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: victim_user.identity.user_id.clone(),
                sources: vec![SourceTransfer {
                    local_instance_id: local_id.to_string(),
                    amount_to_send: send_amount.to_string(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            standards,
            None,
            Some(pw),
        )
        .unwrap();
    let opened =
        bundle_processor::open_and_verify_bundle(&victim_user.identity, &alt_bundle.bundle_bytes)
            .unwrap();
    l2_gateway::build_voucher_lock_batch(
        &opened.vouchers[0],
        0,
        &opened.causal_outputs,
        opened.vouchers[0]
            .transactions
            .first()
            .and_then(|t| opened.root_mig_sig_for(&t.t_id)),
        DEFAULT_LOCK_POW_BASE_BITS,
    )
    .unwrap()
    .into_iter()
    .last()
    .unwrap()
}

fn trust_servers_in_bob_state(
    state_bob: &AppState,
    retain_q: u16,
    specs: &[(u16, &SigningKey, &SigningKey)],
) {
    let mut s = state_bob.cache_security_policy().unwrap();
    let mut lc = state_bob.cache_internal_lifecycle().unwrap();
    for &(port, adm_sk, lock_sk) in specs {
        let wurl = format!("http://127.0.0.1:{}/sync.php", port);
        let rurl = format!("http://127.0.0.1:{}/d", port);
        s.public_syncs.push(crate::settings::StoredPublicSync {
            write_url: wurl.clone(),
            read_url: rurl.clone(),
            drop_pub: hex::encode(adm_sk.verifying_key().as_bytes()),
            register_url: None,
            access_pubkey: "aa".repeat(32),
            enabled: true,
            authorized_channels: vec!["lock".into()],
            status: "active".into(),
            sunset_at: None,
            revocation_reason: None,
            replacement: None,
        });
        let probe = make_signed_probe_for_test(adm_sk, lock_sk, &wurl, &rurl, retain_q);
        lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
    }
    state_bob.set_cached_security_policy(Some(s));
    state_bob.set_cached_internal_lifecycle(Some(lc));
}

async fn spawn_first_write_server(sk: SigningKey, retain_q: u16) -> MockServerHandle {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let receipts: Vec<LockReceipt> = req
                .entries
                .iter()
                .enumerate()
                .map(|(i, hex_str)| {
                    let wire = decode_hex_fixed::<
                        { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                    >(hex_str)
                    .unwrap();
                    let entry = LockEntry::from_wire_bytes(&wire).unwrap();
                    let mut r = LockReceipt {
                        kind: LockReceiptKind::FirstWrite,
                        lock_pub: sk.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: entry.slot,
                        child_lock: entry.child_lock,
                        entry_hash: entry.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: (i as u64) + 1,
                        bucket_head: [7u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    r.sig = sk.sign(&r.digest()).to_bytes();
                    r
                })
                .collect();
            let resp_json = serde_json::to_string(&SyncLockBatchResponse {
                status: "ok".into(),
                bucket: req.bucket,
                retain_until_quarter: retain_q,
                receipts,
                conflict: None,
            })
            .unwrap();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                resp_json.len(),
                resp_json
            );
            if stream.write_all(http.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    MockServerHandle { port, handle }
}

async fn spawn_wrong_nonce_first_write_server(sk: SigningKey, retain_q: u16) -> MockServerHandle {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let receipts: Vec<LockReceipt> = req
                .entries
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let b = decode_hex_fixed::<
                        { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                    >(h)
                    .unwrap();
                    let e = LockEntry::from_wire_bytes(&b).unwrap();
                    let mut r = LockReceipt {
                        kind: LockReceiptKind::FirstWrite,
                        lock_pub: sk.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: e.slot,
                        child_lock: e.child_lock,
                        entry_hash: e.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: (i as u64) + 1,
                        bucket_head: [8u8; 32],
                        recipient_nonce: [0xee; 32],
                        sig: [0u8; 64],
                    };
                    r.sig = sk.sign(&r.digest()).to_bytes();
                    r
                })
                .collect();
            let body = serde_json::to_string(&SyncLockBatchResponse {
                status: "ok".into(),
                bucket: req.bucket,
                retain_until_quarter: retain_q,
                receipts,
                conflict: None,
            })
            .unwrap();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            if stream.write_all(http.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    MockServerHandle { port, handle }
}

fn spawn_conflict_on_listener(
    listener: tokio::net::TcpListener,
    sk: SigningKey,
    retain_q: u16,
    comp_entry: LockEntry,
) -> MockServerHandle {
    let port = listener.local_addr().unwrap().port();
    let comp_hex = comp_entry.to_wire_hex().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let mut receipt = LockReceipt {
                kind: LockReceiptKind::ConflictSeen,
                lock_pub: sk.verifying_key().to_bytes(),
                bucket: req.bucket,
                slot: comp_entry.slot,
                child_lock: comp_entry.child_lock,
                entry_hash: comp_entry.entry_hash().unwrap(),
                retain_until_quarter: retain_q,
                write_seq: 1,
                bucket_head: [9u8; 32],
                recipient_nonce: req.recipient_nonce,
                sig: [0u8; 64],
            };
            receipt.sig = sk.sign(&receipt.digest()).to_bytes();
            let resp_json = serde_json::to_string(&SyncLockBatchResponse {
                status: "conflict".to_string(),
                bucket: req.bucket,
                retain_until_quarter: retain_q,
                receipts: vec![receipt.clone()],
                conflict: Some(SyncLockConflict {
                    entry_index: 0,
                    slot: comp_entry.slot,
                    receipt,
                    competing_entry: comp_hex.clone(),
                }),
            })
            .unwrap();
            let http = format!(
                "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                resp_json.len(),
                resp_json
            );
            if stream.write_all(http.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    MockServerHandle { port, handle }
}

fn spawn_http_status_on_listener(
    listener: tokio::net::TcpListener,
    status_line: &'static str,
    body: String,
) -> MockServerHandle {
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            if stream.read(&mut buf).await.is_err() {
                continue;
            }
            let http = format!(
                "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status_line,
                body.len(),
                body
            );
            if stream.write_all(http.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    MockServerHandle { port, handle }
}

async fn spawn_http_status_server(status_line: &'static str, body: String) -> MockServerHandle {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    spawn_http_status_on_listener(listener, status_line, body)
}

async fn spawn_manifest_discovery_server(
    anchor_id: [u8; 32],
    rev2_bytes: Vec<u8>,
) -> MockServerHandle {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let expected_path = format!("/d/anchors/{}.bin", hex::encode(anchor_id));
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            if raw.starts_with(&format!("GET {} ", expected_path)) {
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                    rev2_bytes.len()
                );
                if stream.write_all(hdr.as_bytes()).await.is_err() {
                    break;
                }
                if stream.write_all(&rev2_bytes).await.is_err() {
                    break;
                }
            } else if stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await
                .is_err()
            {
                break;
            }
        }
    });
    MockServerHandle { port, handle }
}

async fn spawn_equivocating_first_write_server(sk: SigningKey, retain_q: u16) -> MockServerHandle {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let b = decode_hex_fixed::<
                { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
            >(req.entries.last().unwrap())
            .unwrap();
            let entry = LockEntry::from_wire_bytes(&b).unwrap();
            let mut fw_a = LockReceipt {
                kind: LockReceiptKind::FirstWrite,
                lock_pub: sk.verifying_key().to_bytes(),
                bucket: req.bucket,
                slot: entry.slot,
                child_lock: entry.child_lock,
                entry_hash: entry.entry_hash().unwrap(),
                retain_until_quarter: retain_q,
                write_seq: 1,
                bucket_head: [1u8; 32],
                recipient_nonce: req.recipient_nonce,
                sig: [0u8; 64],
            };
            fw_a.sig = sk.sign(&fw_a.digest()).to_bytes();

            let mut fw_b = fw_a.clone();
            fw_b.child_lock = [0x77; 32];
            fw_b.entry_hash = [0x88; 32];
            fw_b.write_seq = 2;
            fw_b.sig = sk.sign(&fw_b.digest()).to_bytes();
            let body = serde_json::to_string(&SyncLockBatchResponse {
                status: "ok".into(),
                bucket: req.bucket,
                retain_until_quarter: retain_q,
                receipts: vec![fw_a, fw_b],
                conflict: None,
            })
            .unwrap();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            drop(stream.write_all(http.as_bytes()).await);
        }
    });
    MockServerHandle { port, handle }
}

fn spawn_k1_mixed_on_listener(
    listener: tokio::net::TcpListener,
    sk: SigningKey,
    retain_q: u16,
    comp_entry: LockEntry,
) -> MockServerHandle {
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let is_conflict_voucher = req.entries.iter().any(|h| {
                decode_hex_fixed::<{ human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN }>(
                    h,
                )
                .ok()
                .and_then(|b| LockEntry::from_wire_bytes(&b).ok())
                .is_some_and(|e| e.slot == comp_entry.slot)
            });
            let (status_line, resp_json) = if is_conflict_voucher {
                let mut receipt = LockReceipt {
                    kind: LockReceiptKind::ConflictSeen,
                    lock_pub: sk.verifying_key().to_bytes(),
                    bucket: req.bucket,
                    slot: comp_entry.slot,
                    child_lock: comp_entry.child_lock,
                    entry_hash: comp_entry.entry_hash().unwrap(),
                    retain_until_quarter: retain_q,
                    write_seq: 1,
                    bucket_head: [9u8; 32],
                    recipient_nonce: req.recipient_nonce,
                    sig: [0u8; 64],
                };
                receipt.sig = sk.sign(&receipt.digest()).to_bytes();
                let body = serde_json::to_string(&SyncLockBatchResponse {
                    status: "conflict".to_string(),
                    bucket: req.bucket,
                    retain_until_quarter: retain_q,
                    receipts: vec![receipt.clone()],
                    conflict: Some(SyncLockConflict {
                        entry_index: 0,
                        slot: comp_entry.slot,
                        receipt,
                        competing_entry: comp_entry.to_wire_hex().unwrap(),
                    }),
                })
                .unwrap();
                ("HTTP/1.1 409 Conflict", body)
            } else {
                let b0 = decode_hex_fixed::<
                    { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                >(&req.entries[0])
                .unwrap();
                let e0 = LockEntry::from_wire_bytes(&b0).unwrap();
                let mut receipt = LockReceipt {
                    kind: LockReceiptKind::FirstWrite,
                    lock_pub: sk.verifying_key().to_bytes(),
                    bucket: req.bucket,
                    slot: e0.slot,
                    child_lock: e0.child_lock,
                    entry_hash: e0.entry_hash().unwrap(),
                    retain_until_quarter: retain_q,
                    write_seq: 1,
                    bucket_head: [8u8; 32],
                    recipient_nonce: req.recipient_nonce,
                    sig: [0u8; 64],
                };
                receipt.sig = sk.sign(&receipt.digest()).to_bytes();
                let body = serde_json::to_string(&SyncLockBatchResponse {
                    status: "ok".to_string(),
                    bucket: req.bucket,
                    retain_until_quarter: retain_q,
                    receipts: vec![receipt],
                    conflict: None,
                })
                .unwrap();
                ("HTTP/1.1 200 OK", body)
            };
            let http = format!(
                "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status_line,
                resp_json.len(),
                resp_json
            );
            if stream.write_all(http.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    MockServerHandle { port, handle }
}

#[tokio::test]
async fn checkout_bundle_returns_offline_only_for_n0_voucher() {
    let mut env = TwoPartyEnv::new("app-chk-n0");
    let (_, lid) = env.issue_voucher("50", None);
    let transfer_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);

    let completion = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-offline-1",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();

    assert!(!completion.newly_committed);
    assert_eq!(completion.verdict.outcome, "OfflineOnly");
    assert!(completion.verdict.requires_merchant_approval);
    assert!(!completion.verdict.can_release_goods_automatically);
    let payload = completion
        .receive_payload
        .as_ref()
        .expect("OfflineOnly checkout must return receive_payload");
    assert_eq!(payload.newly_committed, Some(false));
    assert_eq!(payload.is_replay, Some(false));
    assert_eq!(payload.involved_vouchers.len(), 1);
    assert_eq!(
        payload.checkout_verdict.as_ref().map(|v| v.outcome.as_str()),
        Some("OfflineOnly")
    );
}

#[tokio::test]
async fn checkout_bundle_untrusted_committee_yields_yellow_then_trusted_confirms_and_replays() {
    let mut env = TwoPartyEnv::new("app-chk-l2");
    let sk1 = SigningKey::from_bytes(&[31u8; 32]);
    let sk2 = SigningKey::from_bytes(&[32u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let srv1 = spawn_first_write_server(sk1.clone(), retain_q).await;
    let srv2 = spawn_first_write_server(sk2.clone(), retain_q).await;

    let anchor_id = env.register_rev1_anchor(
        &[44u8; 32],
        retain_q,
        2,
        &[
            (&sk1, &sk1, srv1.port, NominatedBy::Guarantor(0)),
            (&sk2, &sk2, srv2.port, NominatedBy::Guarantor(0)),
        ],
        vec![SigningKey::from_bytes(&[99u8; 32]).verifying_key().to_bytes()],
    );
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let transfer_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);

    // Untrusted committee -> Yellow, NOT auto-committed
    let yellow = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-yellow",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(!yellow.newly_committed);
    assert!(yellow.verdict.outcome.starts_with("Yellow:"));
    assert!(yellow.verdict.requires_merchant_approval);

    // Bob trusts both servers via own public_syncs + verified probe cache -> Green Confirmed!
    trust_servers_in_bob_state(
        &env.state_bob,
        retain_q,
        &[(srv1.port, &sk1, &sk1), (srv2.port, &sk2, &sk2)],
    );
    let green = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-green",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(green.newly_committed);
    assert!(!green.is_replay);
    assert_eq!(green.verdict.outcome, "Confirmed");
    assert!(green.verdict.can_release_goods_automatically);
    assert!(green.verdict.is_goods_release_authorized);

    let replay = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-green",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(replay.is_replay);
    assert!(!replay.newly_committed);
    assert_eq!(replay.verdict.outcome, "Confirmed");
    assert_eq!(
        replay.receive_payload.as_ref().and_then(|p| p.is_replay),
        Some(true)
    );
}

#[tokio::test]
async fn checkout_bundle_multi_voucher_requires_manual_approval() {
    let mut env = TwoPartyEnv::new("app-chk-multi");
    let sk1 = SigningKey::from_bytes(&[33u8; 32]);
    let sk2 = SigningKey::from_bytes(&[34u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let srv1 = spawn_first_write_server(sk1.clone(), retain_q).await;
    let srv2 = spawn_first_write_server(sk2.clone(), retain_q).await;

    let anchor_id = env.register_rev1_anchor(
        &[45u8; 32],
        retain_q,
        2,
        &[
            (&sk1, &sk1, srv1.port, NominatedBy::Guarantor(0)),
            (&sk2, &sk2, srv2.port, NominatedBy::Guarantor(0)),
        ],
        vec![SigningKey::from_bytes(&[99u8; 32]).verifying_key().to_bytes()],
    );
    let (_, lid1) = env.issue_voucher("50", Some(anchor_id));
    let (_, lid2) = env.issue_voucher("40", Some(anchor_id));
    let multi_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid1, "10"), (&lid2, "10")]);

    let multi_res = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-multi",
        &multi_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(!multi_res.newly_committed);
    assert!(!multi_res.is_replay);
    assert_eq!(
        multi_res.verdict.outcome,
        "Yellow:MultiVoucherManualApproval"
    );
    assert!(multi_res.verdict.requires_merchant_approval);
    let warning = multi_res
        .verdict
        .merchant_warning
        .as_deref()
        .expect("MultiVoucherManualApproval must include merchant_warning");
    assert!(!warning.trim().is_empty());
}

#[tokio::test]
async fn k2_precommit_returns_retry_later_when_committee_unreachable() {
    let mut env = TwoPartyEnv::new("app-k2-offline");
    let sk1 = SigningKey::from_bytes(&[61u8; 32]);
    let sk2 = SigningKey::from_bytes(&[62u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();
    drop(l1);
    drop(l2);

    let retain_q = encode_quarter(2035, 4).unwrap();
    let anchor_id = env.register_rev1_anchor(
        &[55u8; 32],
        retain_q,
        2,
        &[
            (&sk1, &sk1, p1, NominatedBy::Guarantor(0)),
            (&sk2, &sk2, p2, NominatedBy::Guarantor(0)),
        ],
        vec![SigningKey::from_bytes(&[98u8; 32]).verifying_key().to_bytes()],
    );
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let transfer_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);

    let offline_pre =
        verify_bundle_l2_precommit(&env.state_bob, &env.bob_session, &transfer_bytes, Some(env.pw))
            .await
            .unwrap();
    assert!(
        matches!(offline_pre, PrecommitL2Verification::RetryLater { .. }),
        "Offline/unreachable committee MUST return RetryLater, got {:?}",
        offline_pre
    );
}

#[tokio::test]
async fn k2_precommit_detects_conflict_and_atomically_quarantines_voucher() {
    let mut env = TwoPartyEnv::new("app-k2-conflict");
    let sk1 = SigningKey::from_bytes(&[61u8; 32]);
    let sk2 = SigningKey::from_bytes(&[62u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();

    let retain_q = encode_quarter(2035, 4).unwrap();
    let anchor_id = env.register_rev1_anchor(
        &[56u8; 32],
        retain_q,
        2,
        &[
            (&sk1, &sk1, p1, NominatedBy::Guarantor(0)),
            (&sk2, &sk2, p2, NominatedBy::Guarantor(0)),
        ],
        vec![SigningKey::from_bytes(&[98u8; 32]).verifying_key().to_bytes()],
    );
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let competing_entry = build_competing_lock_entry(
        env.dir_alice.path(),
        &env.svc_alice,
        env.pw,
        &env.standards,
        &lid,
        &ACTORS.charlie,
        "20",
    );
    let transfer_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);

    let srv1 = spawn_conflict_on_listener(l1, sk1, retain_q, competing_entry.clone());
    let srv2 = spawn_conflict_on_listener(l2, sk2, retain_q, competing_entry);
    assert_eq!((srv1.port, srv2.port), (p1, p2));

    let conflict_pre =
        verify_bundle_l2_precommit(&env.state_bob, &env.bob_session, &transfer_bytes, Some(env.pw))
            .await
            .unwrap();
    let (qcs, comp_entries) = match conflict_pre {
        PrecommitL2Verification::ConflictDetected {
            quorum_certificates,
            competing_entries,
        } => (quorum_certificates, competing_entries),
        other => panic!("Expected ConflictDetected, got {:?}", other),
    };
    assert_eq!(comp_entries.len(), 1);

    let evidence = human_money_core::ReceiveL2Evidence {
        quorum_certificates: &qcs,
        competing_entries: &comp_entries,
    };
    let mut s = env.state_bob.service.lock().unwrap();
    s.receive_bundle_with_companions(
        &transfer_bytes,
        &env.standards,
        None,
        Some(env.pw),
        false,
        Some(&evidence),
        |_, _| Ok(vec![]),
    )
    .unwrap();
    let vouchers = s
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap();
    assert_eq!(vouchers.len(), 1);
    assert!(
        matches!(vouchers[0].status, VoucherStatus::Quarantined { .. }),
        "K2: Conflicting anchored PoST voucher MUST commit atomically as Quarantined, never Active (got {:?})",
        vouchers[0].status
    );
}

#[tokio::test]
async fn k1_precommit_rejects_invalid_competing_entry_without_quorum() {
    let mut env = TwoPartyEnv::new("app-k1-bogus");
    let sk1 = SigningKey::from_bytes(&[71u8; 32]);
    let sk2 = SigningKey::from_bytes(&[72u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let srv2 = spawn_http_status_server("HTTP/1.1 500 Internal Server Error", "error".into()).await;

    let anchor_id = env.register_rev1_anchor(
        &[73u8; 32],
        retain_q,
        2,
        &[
            (&sk1, &sk1, p1, NominatedBy::Guarantor(0)),
            (&sk2, &sk2, srv2.port, NominatedBy::Guarantor(0)),
        ],
        vec![SigningKey::from_bytes(&[95u8; 32]).verifying_key().to_bytes()],
    );
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let mut bogus_comp = build_competing_lock_entry(
        env.dir_alice.path(),
        &env.svc_alice,
        env.pw,
        &env.standards,
        &lid,
        &ACTORS.charlie,
        "20",
    );
    bogus_comp.causal_sig = [0x55; 64];
    let single_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);
    let srv1 = spawn_conflict_on_listener(l1, sk1, retain_q, bogus_comp);
    assert_eq!(srv1.port, p1);

    let res =
        verify_bundle_l2_precommit(&env.state_bob, &env.bob_session, &single_bytes, Some(env.pw))
            .await
            .unwrap();
    assert!(
        matches!(res, PrecommitL2Verification::RetryLater { .. }),
        "K1: Cryptographically invalid competing LockEntry without quorum must return RetryLater, got {:?}",
        res
    );
}

#[tokio::test]
async fn k1_precommit_retries_mixed_bundle_with_unsettled_second_voucher() {
    let mut env = TwoPartyEnv::new("app-k1-mixed");
    let sk1 = SigningKey::from_bytes(&[74u8; 32]);
    let sk2 = SigningKey::from_bytes(&[75u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let srv2 = spawn_http_status_server("HTTP/1.1 500 Internal Server Error", "error".into()).await;
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();

    let anchor_id = env.register_rev1_anchor(
        &[76u8; 32],
        retain_q,
        2,
        &[
            (&sk1, &sk1, p1, NominatedBy::Guarantor(0)),
            (&sk2, &sk2, srv2.port, NominatedBy::Guarantor(0)),
        ],
        vec![SigningKey::from_bytes(&[95u8; 32]).verifying_key().to_bytes()],
    );
    let (_, lid1) = env.issue_voucher("50", Some(anchor_id));
    let valid_comp = build_competing_lock_entry(
        env.dir_alice.path(),
        &env.svc_alice,
        env.pw,
        &env.standards,
        &lid1,
        &ACTORS.charlie,
        "20",
    );
    let (_, lid2) = env.issue_voucher("30", Some(anchor_id));
    let two_voucher_bytes =
        env.send_sources_to(&env.bob_id.clone(), &[(&lid1, "20"), (&lid2, "15")]);
    let srv1 = spawn_k1_mixed_on_listener(l1, sk1, retain_q, valid_comp);
    assert_eq!(srv1.port, p1);

    let res = verify_bundle_l2_precommit(
        &env.state_bob,
        &env.bob_session,
        &two_voucher_bytes,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(
        matches!(res, PrecommitL2Verification::RetryLater { .. }),
        "K1: Mixed bundle with VerifiedConflict on voucher A and Unsettled voucher B must return RetryLater, got {:?}",
        res
    );
}

#[tokio::test]
async fn h7_precommit_epoch_frozen_409_upgrades_manifest_and_retries() {
    let mut env = TwoPartyEnv::new("app-h7");
    let sk1 = SigningKey::from_bytes(&[81u8; 32]);
    let sk2 = SigningKey::from_bytes(&[82u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let srv2 = spawn_first_write_server(sk2.clone(), retain_q).await;
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();

    let anchor_nonce = [83u8; 32];
    let (rev1, rev2, anchor_id, tickets) =
        make_rev1_to_rev2_migration(&env.svc_alice, &anchor_nonce, retain_q, &sk1, p1, &sk2, srv2.port);
    env.svc_alice
        .register_l2_anchor_template_with_nonce(rev1, tickets, Some(&anchor_nonce), Some(env.pw))
        .unwrap();
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let bundle_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);

    let srv1 = spawn_http_status_on_listener(
        l1,
        "HTTP/1.1 409 Conflict",
        serde_json::json!({
            "error": "epoch_frozen",
            "latest_manifest": rev2,
        })
        .to_string(),
    );
    assert_eq!(srv1.port, p1);

    let upgraded =
        verify_bundle_l2_precommit(&env.state_bob, &env.bob_session, &bundle_bytes, Some(env.pw))
            .await
            .unwrap();
    assert!(
        matches!(upgraded, PrecommitL2Verification::CleanQuorum { .. }),
        "H7: Precommit must upgrade rev-1 -> rev-2 on 409 epoch_frozen and succeed on s2, got {:?}",
        upgraded
    );

    let stored_rev = env
        .state_bob
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| {
            w.profile
                .l2_anchor_manifests
                .get(&hex::encode(anchor_id))
                .map(|m| m.revision)
        })
        .unwrap();
    assert_eq!(
        stored_rev,
        Some(2),
        "Upgraded rev-2 manifest must be persisted"
    );
}

fn make_signed_probe_for_test(
    admin_sk: &SigningKey,
    lock_sk: &SigningKey,
    write_url: &str,
    read_url: &str,
    retain_q: u16,
) -> sync_client::FrontendLockServerProbeResult {
    let admin_hex = hex::encode(admin_sk.verifying_key().as_bytes());
    let lock_hex = hex::encode(lock_sk.verifying_key().as_bytes());
    let msg = humoco_market::format_lock_delegate_signature_message(
        &admin_hex,
        &lock_hex,
        write_url,
        read_url,
        1_760_000_000,
    );
    let sig = hex::encode(admin_sk.sign(msg.as_bytes()).to_bytes());
    sync_client::FrontendLockServerProbeResult {
        write_url: write_url.to_string(),
        read_url: read_url.to_string(),
        drop_pub: admin_hex.clone(),
        admin_pub: admin_hex.clone(),
        lock_pub: lock_hex.clone(),
        delegation_valid: true,
        waf_probe_passed: true,
        selftest_passed: true,
        key_leak_safe: true,
        passed: true,
        latency_ms: 5,
        max_retention_quarter: retain_q,
        pow_base_bits: DEFAULT_LOCK_POW_BASE_BITS,
        allow_pow_tickets: true,
        ticket_pow_bits: 16,
        inode_warning: false,
        used_inodes: 0,
        max_inodes: 50000,
        probed_at_secs: 1_760_000_000,
        delegation: Some(sync_client::SyncLockDelegationDto {
            admin_pub: admin_hex,
            lock_pub: lock_hex,
            write_url: write_url.to_string(),
            read_url: read_url.to_string(),
            issued_at: 1_760_000_000,
            sig,
        }),
        error: None,
    }
}

fn make_manifest_from_entries(retain_q: u16, servers: Vec<ServerEntry>) -> AnchorManifest {
    AnchorManifest {
        revision: 1,
        threshold: 1,
        servers,
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub: [8u8; 32],
            guarantor_mig_pubs: vec![[9u8; 32]],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    }
}

#[tokio::test]
async fn test_s04_f2_already_in_history_replay_never_fabricates_confirmed() {
    let mut env = TwoPartyEnv::new("s04-f2");
    let (_, lid) = env.issue_voucher("50", None);
    let transfer_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "10")]);

    // Bob receives the N=0 bundle directly into wallet history (NOT via checkout_purchase)
    env.state_bob
        .service
        .lock()
        .unwrap()
        .receive_bundle_with_companions(
            &transfer_bytes,
            &env.standards,
            None,
            Some(env.pw),
            false,
            None,
            |_, _| Ok(vec![]),
        )
        .unwrap();

    let replay = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-replay-hist",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(replay.is_replay);
    assert_eq!(replay.verdict.outcome, "OfflineOnly");
    assert!(!replay.verdict.is_goods_release_authorized);

    // If the already-received voucher is Quarantined, replay MUST report Quarantined
    {
        let mut s = env.state_bob.service.lock().unwrap();
        let (wallet, _) = s.get_unlocked_mut_for_test();
        for inst in wallet.voucher_store.vouchers.values_mut() {
            inst.status = VoucherStatus::Quarantined {
                reason: "Upstream conflict".into(),
            };
        }
    }
    let replay_q = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-replay-quarantined",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(replay_q.is_replay);
    assert_eq!(
        replay_q.verdict.outcome,
        "Quarantined:BundleAlreadyProcessedQuarantined"
    );
    assert!(!replay_q.verdict.is_goods_release_authorized);
}

#[tokio::test]
async fn test_s04_f7_stale_session_token_rejected_by_checkout_bundle() {
    let mut env = TwoPartyEnv::new("s04-f7");
    let (_, lid) = env.issue_voucher("50", None);
    let transfer_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "10")]);

    // Positive baseline: current session token succeeds
    let valid_res = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-valid-session",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert_eq!(valid_res.verdict.outcome, "OfflineOnly");

    // Stale SessionToken generation MUST be rejected immediately
    let stale_session = crate::session::SessionToken {
        generation: env.bob_session.generation.saturating_add(99),
        owner_did: env.bob_session.owner_did.clone(),
    };
    let stale_err = checkout_bundle(
        &env.state_bob,
        &stale_session,
        "pos-stale",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap_err();
    assert!(
        stale_err.message.contains("session changed"),
        "S04-F7: stale session token must be rejected, got: {:?}",
        stale_err
    );
}

#[tokio::test]
async fn test_s04_f3_unsigned_cached_probe_excluded_while_signed_probe_trusted() {
    let env = TwoPartyEnv::new("s04-f3-probe");
    let retain_q = encode_quarter(2035, 4).unwrap();
    let sk_srv = SigningKey::from_bytes(&[181u8; 32]);
    let pk_srv = sk_srv.verifying_key().to_bytes();
    let wurl = "https://unsigned-check.example/sync.php";
    let rurl = "https://unsigned-check.example/d";

    let manifest = make_manifest_from_entries(
        retain_q,
        vec![ServerEntry {
            write_url: wurl.to_string(),
            read_url: rurl.to_string(),
            admin_pub: pk_srv,
            lock_pub: pk_srv,
            nominated_by: NominatedBy::Creator,
        }],
    );

    let mut policy = SecurityPolicy::default();
    policy.market_selected_syncs.push(
        humoco_market::DropDescriptor {
            write_url: wurl.to_string(),
            read_url: rurl.to_string(),
            slot_id: None,
            drop_pub: hex::encode(pk_srv),
        }
        .into(),
    );

    // Positive baseline: valid signed delegation enters both sets
    let mut lc = InternalLifecycleState::default();
    let probe = make_signed_probe_for_test(&sk_srv, &sk_srv, wurl, rurl, retain_q);
    lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
    let client = sync_client::create_sync_http_client();
    let alice_did = ACTORS.alice.identity.user_id.clone();

    let (base_ok, pinned_ok) = resolve_trusted_lock_keys(
        &env.state_bob,
        &env.bob_session,
        &policy,
        &lc,
        &client,
        Some(&alice_did),
        Some(&manifest),
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(base_ok.contains(&pk_srv));
    assert!(pinned_ok.contains(&pk_srv));

    // Negative check: clearing probe delegation strips trust immediately
    for p in lc.l2_probe_cache.values_mut() {
        p.delegation = None;
    }
    let (base_unsigned, pinned_unsigned) = resolve_trusted_lock_keys(
        &env.state_bob,
        &env.bob_session,
        &policy,
        &lc,
        &client,
        Some(&alice_did),
        Some(&manifest),
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(
        !base_unsigned.contains(&pk_srv),
        "S04-F3: unsigned cached probe (delegation: None) must NOT be trusted"
    );
    assert!(!pinned_unsigned.contains(&pk_srv));
}

#[tokio::test]
async fn test_s04_f9_hop1_contact_sync_in_base_trusted_not_independently_pinned() {
    let env = TwoPartyEnv::new("s04-f9-hop1");
    let retain_q = encode_quarter(2035, 4).unwrap();
    let sk_pinned = SigningKey::from_bytes(&[181u8; 32]);
    let sk_contact = SigningKey::from_bytes(&[182u8; 32]);
    let pk_pinned = sk_pinned.verifying_key().to_bytes();
    let pk_contact = sk_contact.verifying_key().to_bytes();
    let url_pinned = "https://own-pinned.example/sync.php";
    let url_contact = "https://hop1-contact.example/sync.php";

    let manifest = make_manifest_from_entries(
        retain_q,
        vec![
            ServerEntry {
                write_url: url_pinned.to_string(),
                read_url: "https://own-pinned.example/d".to_string(),
                admin_pub: pk_pinned,
                lock_pub: pk_pinned,
                nominated_by: NominatedBy::Creator,
            },
            ServerEntry {
                write_url: url_contact.to_string(),
                read_url: "https://hop1-contact.example/d".to_string(),
                admin_pub: pk_contact,
                lock_pub: pk_contact,
                nominated_by: NominatedBy::Creator,
            },
        ],
    );

    let mut policy = SecurityPolicy::default();
    policy.market_selected_syncs.push(
        humoco_market::DropDescriptor {
            write_url: url_pinned.to_string(),
            read_url: "https://own-pinned.example/d".to_string(),
            slot_id: None,
            drop_pub: hex::encode(pk_pinned),
        }
        .into(),
    );

    let mut lc = InternalLifecycleState::default();
    let p_pinned = make_signed_probe_for_test(
        &sk_pinned,
        &sk_pinned,
        url_pinned,
        "https://own-pinned.example/d",
        retain_q,
    );
    let p_contact = make_signed_probe_for_test(
        &sk_contact,
        &sk_contact,
        url_contact,
        "https://hop1-contact.example/d",
        retain_q,
    );
    lc.l2_probe_cache.insert(p_pinned.lock_pub.clone(), p_pinned);
    lc.l2_probe_cache
        .insert(p_contact.lock_pub.clone(), p_contact);

    let charlie_did = ACTORS.charlie.identity.user_id.clone();
    let mut book = crate::models::FrontendAddressBook::default();
    book.contacts.insert(
        charlie_did.clone(),
        crate::models::FrontendContact {
            did: charlie_did,
            profile: crate::models::FrontendUserProfile::default(),
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![humoco_market::DropDescriptor {
                write_url: url_contact.to_string(),
                read_url: "https://hop1-contact.example/d".to_string(),
                slot_id: None,
                drop_pub: hex::encode(pk_contact),
            }
            .into()],
        },
    );
    {
        let s = env.state_bob.service.lock().unwrap();
        env.state_bob
            .write_cache_slot_for_service(&s, |c| &mut c.contacts, Some(book));
    }

    let client = sync_client::create_sync_http_client();
    let (base_trusted, independently_pinned) = resolve_trusted_lock_keys(
        &env.state_bob,
        &env.bob_session,
        &policy,
        &lc,
        &client,
        Some(&ACTORS.alice.identity.user_id),
        Some(&manifest),
        Some(env.pw),
    )
    .await
    .unwrap();

    assert!(base_trusted.contains(&pk_pinned));
    assert!(independently_pinned.contains(&pk_pinned));
    assert!(
        base_trusted.contains(&pk_contact),
        "S04-F9: Hop-1 contact sync must be in base_trusted_keys"
    );
    assert!(
        !independently_pinned.contains(&pk_contact),
        "S04-F9: transitive Hop-1 contact sync must NOT enter independently_pinned_creator_keys"
    );
}

#[tokio::test]
async fn test_s04_f8_hop2_contact_trusted_until_bridge_marked_known_offender() {
    let env = TwoPartyEnv::new("s04-f8-bridge");
    let retain_q = encode_quarter(2035, 4).unwrap();
    let sk_bridge = SigningKey::from_bytes(&[183u8; 32]);
    let pk_bridge = sk_bridge.verifying_key().to_bytes();
    let url_bridge = "https://hop2-via-offender.example/sync.php";
    let read_bridge = "https://hop2-via-offender.example/d";

    let manifest = make_manifest_from_entries(
        retain_q,
        vec![ServerEntry {
            write_url: url_bridge.to_string(),
            read_url: read_bridge.to_string(),
            admin_pub: pk_bridge,
            lock_pub: pk_bridge,
            nominated_by: NominatedBy::Guarantor(0),
        }],
    );

    let mut lc = InternalLifecycleState::default();
    let p_bridge =
        make_signed_probe_for_test(&sk_bridge, &sk_bridge, url_bridge, read_bridge, retain_q);
    lc.l2_probe_cache.insert(p_bridge.lock_pub.clone(), p_bridge);

    let alice_did = ACTORS.alice.identity.user_id.clone();
    let dave_did = ACTORS.david.identity.user_id.clone();
    let mut book = crate::models::FrontendAddressBook::default();
    book.contacts.insert(
        dave_did.clone(),
        crate::models::FrontendContact {
            did: dave_did.clone(),
            profile: crate::models::FrontendUserProfile::default(),
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![humoco_market::DropDescriptor {
                write_url: url_bridge.to_string(),
                read_url: read_bridge.to_string(),
                slot_id: None,
                drop_pub: hex::encode(pk_bridge),
            }
            .into()],
        },
    );

    let bob_can = canonicalize_did(&env.bob_id).unwrap();
    let alice_can = canonicalize_did(&alice_did).unwrap();
    let dave_can = canonicalize_did(&dave_did).unwrap();
    let mut reg =
        humoco_wot::TrustRegister::new(&bob_can, humoco_wot::WotPolicy::default()).unwrap();
    reg.set_contact_trust(&alice_can, 2, 2, 1_760_000_000)
        .unwrap();
    let mut alice_reg =
        humoco_wot::TrustRegister::new(&alice_can, humoco_wot::WotPolicy::default()).unwrap();
    alice_reg
        .set_contact_trust(&dave_can, 2, 2, 1_760_000_000)
        .unwrap();
    let alice_signer = crate::state::DidWotSigner::from_identity(&ACTORS.alice.identity);
    alice_reg
        .ensure_own_list_signed(&alice_signer, 1_760_000_000)
        .unwrap();
    let signed = alice_reg
        .own_signed_list
        .as_ref()
        .expect("ensure_own_list_signed must populate own_signed_list")
        .clone();
    reg.raw_list_store.insert(alice_can, signed);
    reg.invalidate_graph_cache();

    {
        let s = env.state_bob.service.lock().unwrap();
        env.state_bob
            .write_cache_slot_for_service(&s, |c| &mut c.contacts, Some(book));
        env.state_bob
            .write_cache_slot_for_service(&s, |c| &mut c.wot, Some(reg));
    }

    let policy = SecurityPolicy::default();
    let client = sync_client::create_sync_http_client();

    // Positive baseline: before Alice is marked KnownOffender, Hop-2 Dave IS trusted
    let (trusted_clean, _) = resolve_trusted_lock_keys(
        &env.state_bob,
        &env.bob_session,
        &policy,
        &lc,
        &client,
        Some(&alice_did),
        Some(&manifest),
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(
        trusted_clean.contains(&pk_bridge),
        "Positive baseline: Hop-2 Dave via non-offender Alice must be trusted"
    );

    // Mark Alice as KnownOffender in Bob's proof_store -> Hop-2 bridge MUST be severed
    {
        let mut s = env.state_bob.service.lock().unwrap();
        let bob_uid = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let (wallet, _) = s.get_unlocked_mut_for_test();
        wallet.proof_store.proofs.insert(
            "proof-s04-f8".to_string(),
            ProofStoreEntry {
                proof: ProofOfDoubleSpend {
                    proof_id: "proof-s04-f8".to_string(),
                    offender_id: alice_did.clone(),
                    suspected_identity: None,
                    fork_point_prev_hash: "00".repeat(32),
                    conflicting_transactions: Vec::new(),
                    fingerprints: Vec::new(),
                    victim_witnesses: Vec::new(),
                    deletable_at: "2099-01-01T00:00:00Z".to_string(),
                    reporter_id: bob_uid,
                    report_timestamp: "2026-01-01T00:00:00Z".to_string(),
                    reporter_signature: "sig".to_string(),
                    affected_voucher_name: None,
                    voucher_standard_uuid: None,
                    resolutions: None,
                    non_redeemable_test_voucher: false,
                },
                local_override: false,
                local_note: None,
                conflict_role: ConflictRole::default(),
            },
        );
    }

    let (trusted_after_offense, _) = resolve_trusted_lock_keys(
        &env.state_bob,
        &env.bob_session,
        &policy,
        &lc,
        &client,
        Some(&alice_did),
        Some(&manifest),
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(
        !trusted_after_offense.contains(&pk_bridge),
        "S04-F8: Hop-2 contact reachable only via KnownOffender bridge must NOT be trusted"
    );
}

fn configure_live_probe(state: &AppState, sk: &SigningKey, port: u16, retain_q: u16, pow_base_bits: u32) {
    let mut lc = InternalLifecycleState::default();
    let mut probe = make_signed_probe_for_test(
        sk,
        sk,
        &format!("http://127.0.0.1:{}/sync.php", port),
        &format!("http://127.0.0.1:{}/d", port),
        retain_q,
    );
    probe.pow_base_bits = pow_base_bits;
    lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
    state.set_cached_internal_lifecycle(Some(lc));
}

#[tokio::test]
async fn test_s04_f1_precommit_rejects_wrong_recipient_nonce() {
    let retain_q = encode_quarter(2035, 4).unwrap();

    // Positive baseline: matching recipient_nonce yields CleanQuorum
    let mut env_ok = TwoPartyEnv::new("s04-f1-nonce-ok");
    let sk_ok = SigningKey::from_bytes(&[190u8; 32]);
    let srv_ok = spawn_first_write_server(sk_ok.clone(), retain_q).await;
    let anchor_ok = env_ok.register_rev1_anchor(
        &[192u8; 32],
        retain_q,
        1,
        &[(&sk_ok, &sk_ok, srv_ok.port, NominatedBy::Creator)],
        vec![],
    );
    let (_, lid_ok) = env_ok.issue_voucher("50", Some(anchor_ok));
    let bundle_ok = env_ok.send_sources_to(&env_ok.bob_id.clone(), &[(&lid_ok, "20")]);
    configure_live_probe(&env_ok.state_bob, &sk_ok, srv_ok.port, retain_q, 18);
    let res_ok =
        verify_bundle_l2_precommit(&env_ok.state_bob, &env_ok.bob_session, &bundle_ok, Some(env_ok.pw))
            .await
            .unwrap();
    assert!(
        matches!(res_ok, PrecommitL2Verification::CleanQuorum { .. }),
        "Positive baseline: matching recipient_nonce must yield CleanQuorum, got {:?}",
        res_ok
    );

    // Wrong nonce MUST NOT yield CleanQuorum
    let mut env_bad = TwoPartyEnv::new("s04-f1-nonce-bad");
    let sk_bad = SigningKey::from_bytes(&[191u8; 32]);
    let srv_bad = spawn_wrong_nonce_first_write_server(sk_bad.clone(), retain_q).await;
    let anchor_bad = env_bad.register_rev1_anchor(
        &[192u8; 32],
        retain_q,
        1,
        &[(&sk_bad, &sk_bad, srv_bad.port, NominatedBy::Creator)],
        vec![],
    );
    let (_, lid_bad) = env_bad.issue_voucher("50", Some(anchor_bad));
    let bundle_bad = env_bad.send_sources_to(&env_bad.bob_id.clone(), &[(&lid_bad, "20")]);
    configure_live_probe(&env_bad.state_bob, &sk_bad, srv_bad.port, retain_q, 18);
    let res_bad = verify_bundle_l2_precommit(
        &env_bad.state_bob,
        &env_bad.bob_session,
        &bundle_bad,
        Some(env_bad.pw),
    )
    .await
    .unwrap();
    assert!(
        matches!(res_bad, PrecommitL2Verification::RetryLater { .. }),
        "S04-F1: wrong recipient_nonce must return RetryLater, got {:?}",
        res_bad
    );
}

#[tokio::test]
async fn test_s04_f1_precommit_rejects_revoked_server_receipt() {
    let mut env = TwoPartyEnv::new("s04-f1-revoked");
    let retain_q = encode_quarter(2035, 4).unwrap();
    let sk1 = SigningKey::from_bytes(&[193u8; 32]);
    let srv1 = spawn_first_write_server(sk1.clone(), retain_q).await;
    let anchor_id = env.register_rev1_anchor(
        &[194u8; 32],
        retain_q,
        1,
        &[(&sk1, &sk1, srv1.port, NominatedBy::Creator)],
        vec![],
    );
    let (_, lid_clean) = env.issue_voucher("50", Some(anchor_id));
    let (_, lid_revoked) = env.issue_voucher("50", Some(anchor_id));
    let bundle_clean = env.send_sources_to(&env.bob_id.clone(), &[(&lid_clean, "15")]);
    let bundle_revoked = env.send_sources_to(&env.bob_id.clone(), &[(&lid_revoked, "10")]);
    configure_live_probe(&env.state_bob, &sk1, srv1.port, retain_q, 18);

    // Positive baseline: non-revoked server yields CleanQuorum
    let ok_res =
        verify_bundle_l2_precommit(&env.state_bob, &env.bob_session, &bundle_clean, Some(env.pw))
            .await
            .unwrap();
    assert!(
        matches!(ok_res, PrecommitL2Verification::CleanQuorum { .. }),
        "Positive baseline before revocation must yield CleanQuorum, got {:?}",
        ok_res
    );

    // Revoke server lock_pub -> valid FirstWrite receipt MUST return RetryLater
    {
        let mut offenders = humoco_wot::ServerOffenderList::new();
        offenders
            .revoked_lock_pubs
            .insert(sk1.verifying_key().to_bytes());
        let s = env.state_bob.service.lock().unwrap();
        env.state_bob
            .write_cache_slot_for_service(&s, |c| &mut c.server_offenders, Some(offenders));
    }
    let revoked_res = verify_bundle_l2_precommit(
        &env.state_bob,
        &env.bob_session,
        &bundle_revoked,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(
        matches!(revoked_res, PrecommitL2Verification::RetryLater { .. }),
        "S04-F1: revoked server receipt must return RetryLater, got {:?}",
        revoked_res
    );
}

#[tokio::test]
async fn test_s04_f1_precommit_verifies_historical_default_pow_conflict_despite_higher_live_pow() {
    let mut env = TwoPartyEnv::new("s04-f1-pow");
    let retain_q = encode_quarter(2035, 4).unwrap();
    let sk1 = SigningKey::from_bytes(&[195u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let anchor_id = env.register_rev1_anchor(
        &[196u8; 32],
        retain_q,
        1,
        &[(&sk1, &sk1, p1, NominatedBy::Creator)],
        vec![],
    );
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let valid_comp = build_competing_lock_entry(
        env.dir_alice.path(),
        &env.svc_alice,
        env.pw,
        &env.standards,
        &lid,
        &ACTORS.charlie,
        "20",
    );
    let bundle_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);
    let srv1 = spawn_conflict_on_listener(l1, sk1.clone(), retain_q, valid_comp);
    assert_eq!(srv1.port, p1);
    configure_live_probe(&env.state_bob, &sk1, p1, retain_q, 18);

    let res_conflict =
        verify_bundle_l2_precommit(&env.state_bob, &env.bob_session, &bundle_bytes, Some(env.pw))
            .await
            .unwrap();
    match res_conflict {
        PrecommitL2Verification::ConflictDetected {
            competing_entries, ..
        } => {
            assert_eq!(competing_entries.len(), 1);
            assert_eq!(competing_entries[0].1, DEFAULT_LOCK_POW_BASE_BITS);
        }
        other => panic!(
            "S04-F1: expected ConflictDetected with DEFAULT_LOCK_POW_BASE_BITS despite live PoW=18, got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_s04_f3_live_equivocation_revokes_admin_sibling_before_checkout_evaluation() {
    let mut env = TwoPartyEnv::new("s04-f3-equiv");
    let admin_sk = SigningKey::from_bytes(&[201u8; 32]);
    let sk_lock1 = SigningKey::from_bytes(&[202u8; 32]);
    let sk_lock2 = SigningKey::from_bytes(&[203u8; 32]);
    let sk_lock3 = SigningKey::from_bytes(&[205u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();

    // N=3, Q=2: Server 1 & sibling Server 2 share admin_pub A; Server 3 has independent admin_pub B.
    let srv1 = spawn_equivocating_first_write_server(sk_lock1.clone(), retain_q).await;
    let srv2 = spawn_first_write_server(sk_lock2.clone(), retain_q).await;
    let srv3 = spawn_first_write_server(sk_lock3.clone(), retain_q).await;

    let anchor_id = env.register_rev1_anchor(
        &[204u8; 32],
        retain_q,
        2,
        &[
            (&admin_sk, &sk_lock1, srv1.port, NominatedBy::Guarantor(0)),
            (&admin_sk, &sk_lock2, srv2.port, NominatedBy::Guarantor(0)),
            (&sk_lock3, &sk_lock3, srv3.port, NominatedBy::Guarantor(0)),
        ],
        vec![[9u8; 32]],
    );
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let transfer_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);

    trust_servers_in_bob_state(
        &env.state_bob,
        retain_q,
        &[
            (srv1.port, &admin_sk, &sk_lock1),
            (srv2.port, &admin_sk, &sk_lock2),
            (srv3.port, &sk_lock3, &sk_lock3),
        ],
    );

    let res = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "pos-equiv-sibling",
        &transfer_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .unwrap();
    assert!(!res.newly_committed);
    assert!(
        res.verdict.outcome.starts_with("Yellow:"),
        "S04-F3: pruning equivocating admin_pub sibling must drop trusted count below N-Q+1=2 -> Yellow, got {}",
        res.verdict.outcome
    );
    assert!(res.verdict.requires_merchant_approval);
    assert!(!res.verdict.is_goods_release_authorized);

    let offenders = {
        let mut s = env.state_bob.service.lock().unwrap();
        env.state_bob
            .get_or_load_server_offender_list(&mut s, Some(env.pw))
            .unwrap()
    };
    assert!(
        crate::l2_lifecycle::is_server_key_revoked(
            &offenders,
            &sk_lock2.verifying_key().to_bytes(),
            Some(&admin_sk.verifying_key().to_bytes()),
        ),
        "S04-F3: ADR-10 equivocation must revoke shared admin_pub so sibling lock_pub2 is banned"
    );
}

#[tokio::test]
async fn test_s04_f6_inline_less_409_epoch_frozen_triggers_discovery() {
    let mut env = TwoPartyEnv::new("s04-f6-disc");
    let sk1 = SigningKey::from_bytes(&[211u8; 32]);
    let sk2 = SigningKey::from_bytes(&[212u8; 32]);
    let retain_q = encode_quarter(2035, 4).unwrap();
    let srv2 = spawn_first_write_server(sk2.clone(), retain_q).await;
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();

    let anchor_nonce = [213u8; 32];
    let (rev1, rev2, anchor_id, tickets) =
        make_rev1_to_rev2_migration(&env.svc_alice, &anchor_nonce, retain_q, &sk1, p1, &sk2, srv2.port);
    env.svc_alice
        .register_l2_anchor_template_with_nonce(rev1, tickets, Some(&anchor_nonce), Some(env.pw))
        .unwrap();
    let (_, lid) = env.issue_voucher("50", Some(anchor_id));
    let bundle_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "20")]);

    let srv1 = spawn_http_status_on_listener(
        l1,
        "HTTP/1.1 409 Conflict",
        r#"{"error":"epoch_frozen"}"#.to_string(),
    );
    assert_eq!(srv1.port, p1);
    let srv_disc =
        spawn_manifest_discovery_server(anchor_id, serde_json::to_vec(&rev2).unwrap()).await;

    let mut policy = SecurityPolicy::default();
    policy.public_syncs.push(crate::settings::StoredPublicSync {
        write_url: format!("http://127.0.0.1:{}/sync.php", srv_disc.port),
        read_url: format!("http://127.0.0.1:{}/d", srv_disc.port),
        drop_pub: "cc".repeat(32),
        register_url: None,
        access_pubkey: "dd".repeat(32),
        enabled: true,
        authorized_channels: vec!["lock".into()],
        status: "active".into(),
        sunset_at: None,
        revocation_reason: None,
        replacement: None,
    });
    env.state_bob.set_cached_security_policy(Some(policy));

    let upgraded =
        verify_bundle_l2_precommit(&env.state_bob, &env.bob_session, &bundle_bytes, Some(env.pw))
            .await
            .unwrap();
    assert!(
        matches!(upgraded, PrecommitL2Verification::CleanQuorum { .. }),
        "S04-F6: inline-less 409 epoch_frozen must trigger discovery and succeed on Rev 2, got {:?}",
        upgraded
    );

    let stored_rev = env
        .state_bob
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| {
            w.profile
                .l2_anchor_manifests
                .get(&hex::encode(anchor_id))
                .map(|m| m.revision)
        })
        .unwrap();
    assert_eq!(stored_rev, Some(2));
}

#[tokio::test]
async fn test_s04_q2_checkout_bundle_degrades_to_yellow_when_manifest_missing_and_allows_offline_commit(
) {
    let mut env = TwoPartyEnv::new("s04-q2-missing");
    let (voucher, lid) = env.issue_voucher("40", Some([0xD4u8; 32]));
    let bundle_bytes = env.send_sources_to(&env.bob_id.clone(), &[(&lid, "15")]);

    let completion = checkout_bundle(
        &env.state_bob,
        &env.bob_session,
        "purchase-s04-q2-missing-manifest",
        &bundle_bytes,
        &env.standards,
        false,
        Some(env.pw),
    )
    .await
    .expect("missing manifest must degrade gracefully to Yellow, not hard-fail");

    assert!(!completion.newly_committed);
    assert!(!completion.is_replay);
    assert_eq!(
        completion.verdict.outcome,
        "Yellow:Protection committee manifest unavailable (offline/unreachable); manual trust approval required"
    );
    assert!(completion.verdict.requires_merchant_approval);
    assert!(!completion.verdict.can_release_goods_automatically);
    let payload = completion
        .receive_payload
        .as_ref()
        .expect("Yellow missing-manifest checkout must return receive_payload");
    assert_eq!(payload.newly_committed, Some(false));
    assert_eq!(payload.involved_vouchers.len(), 1);

    let recv = crate::services::transfer_service::receive_bundle_impl(
        None,
        bundle_bytes,
        env.standards.clone(),
        Some(env.pw.to_string()),
        false,
        Some(crate::models::ReceiveCommitMode::ForceTrustOffline),
        None,
        &env.state_bob,
    )
    .await
    .expect("ForceTrustOffline must commit anchored voucher even without manifest");

    assert_eq!(recv.newly_committed, Some(true));
    let active_count = env
        .state_bob
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| {
            w.voucher_store
                .vouchers
                .values()
                .filter(|inst| {
                    inst.voucher.voucher_id == voucher.voucher_id
                        && matches!(inst.status, VoucherStatus::Active)
                })
                .count()
        })
        .unwrap();
    assert_eq!(active_count, 1);
}
