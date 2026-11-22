use super::execute::*;
use super::precommit::*;
use super::trusted_keys::*;
use std::collections::HashMap;

use crate::state::canonicalize_did;
use human_money_core::humoco_proto::{
    crypto::DEFAULT_LOCK_POW_BASE_BITS, decode_hex_fixed, manifest::AnchorManifest,
};
use human_money_core::models::layer2_api::LockEntry;
use human_money_core::services::{bundle_processor, l2_gateway};

use crate::{
    settings::{InternalLifecycleState, SecurityPolicy},
    sync_client, AppState,
};
use ed25519_dalek::{Signer, SigningKey};
use human_money_core::humoco_proto::{
    crypto::compute_ticket_pow_hash,
    encode_quarter,
    manifest::{AnchorAuthority, GenesisTicket, NominatedBy, ServerEntry},
    types::{LockReceipt, LockReceiptKind},
};
use human_money_core::models::layer2_api::{
    SyncLockBatchRequest, SyncLockBatchResponse, SyncLockConflict,
};
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
use human_money_core::test_utils::{self, ACTORS};
use human_money_core::wallet::types::{MultiTransferRequest, SourceTransfer};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn temp_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{}-{}", prefix, uuid::Uuid::new_v4()))
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
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

#[tokio::test]
async fn checkout_bundle_returns_offline_only_for_n0_voucher() {
    let dir_alice = temp_dir("app-chk-alice-n0");
    let dir_bob = temp_dir("app-chk-bob-n0");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut standards = HashMap::new();
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let std_uuid = std_def.immutable.identity.uuid;
    standards.insert(std_uuid, std_toml.clone());

    let _ = svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "50".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(pw),
        )
        .unwrap();

    let instances = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap();
    let local_id = instances[0].local_instance_id.clone();

    let transfer = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: local_id,
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    let state_bob = AppState::new(svc_bob);
    state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };
    let completion = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-offline-1",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .unwrap();

    assert!(!completion.newly_committed);
    assert_eq!(completion.verdict.outcome, "OfflineOnly");
    assert!(completion.verdict.requires_merchant_approval);
    assert!(!completion.verdict.can_release_goods_automatically);
    assert!(completion.receive_payload.is_some());

    drop(state_bob);
    drop(svc_alice);
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn checkout_bundle_green_and_yellow_for_anchored_voucher() {
    let dir_alice = temp_dir("app-chk-alice-l2");
    let dir_bob = temp_dir("app-chk-bob-l2");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    svc_alice.unlock_session(pw, 600).unwrap();
    svc_bob.unlock_session(pw, 600).unwrap();

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let std_uuid = std_def.immutable.identity.uuid;
    let mut standards = HashMap::new();
    standards.insert(std_uuid, std_toml.clone());

    let sk1 = SigningKey::from_bytes(&[31u8; 32]);
    let sk2 = SigningKey::from_bytes(&[32u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();

    let anchor_nonce = [44u8; 32];
    let creator_mig_pub = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();

    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![
            ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", p1),
                read_url: format!("http://127.0.0.1:{}/d", p1),
                admin_pub: sk1.verifying_key().to_bytes(),
                lock_pub: sk1.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            },
            ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", p2),
                read_url: format!("http://127.0.0.1:{}/d", p2),
                admin_pub: sk2.verifying_key().to_bytes(),
                lock_pub: sk2.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            },
        ],
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
    let tickets: Vec<GenesisTicket> = [&sk1, &sk2]
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

    svc_alice
        .register_l2_anchor_template_with_nonce(
            manifest.clone(),
            tickets,
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();

    let _ = svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
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

    let local_id = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();

    let transfer = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: local_id,
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    let spawn_lock_srv = |listener: tokio::net::TcpListener, sk: SigningKey| {
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = [0u8; 65_536];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
                let req: human_money_core::models::layer2_api::SyncLockBatchRequest =
                    serde_json::from_str(body_str).unwrap();
                let receipts: Vec<LockReceipt> = req
                    .entries
                    .iter()
                    .enumerate()
                    .map(|(i, hex_str)| {
                        let wire = decode_hex_fixed::<605>(hex_str).unwrap();
                        let entry =
                            human_money_core::humoco_proto::causal::LockEntry::from_wire_bytes(
                                &wire,
                            )
                            .unwrap();
                        let _ = compute_ticket_pow_hash(&req.anchor_id, retain_q, 1, 0);
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
                let resp_json = serde_json::json!({
                    "status": "ok",
                    "bucket": hex::encode(req.bucket),
                    "retain_until_quarter": retain_q,
                    "receipts": receipts,
                    "conflict": null,
                })
                .to_string();
                let http = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    resp_json.len(),
                    resp_json
                );
                let _ = stream.write_all(http.as_bytes()).await;
            }
        })
    };

    let h1 = spawn_lock_srv(l1, sk1.clone());
    let h2 = spawn_lock_srv(l2, sk2.clone());

    // Case A: Bob has NO trusted servers -> Yellow verdict, NOT auto-committed
    let state_bob = AppState::new(svc_bob);
    state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };
    let yellow = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-yellow",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(!yellow.newly_committed);
    assert!(yellow.verdict.outcome.starts_with("Yellow:"));
    assert!(yellow.verdict.requires_merchant_approval);

    // Case B: Bob trusts both servers via own public_syncs + verified probe cache -> Green Confirmed!
    {
        let mut s = state_bob.cache_security_policy().unwrap();
        let mut lc = state_bob.cache_internal_lifecycle().unwrap();
        for (port, sk) in [(p1, &sk1), (p2, &sk2)] {
            let wurl = format!("http://127.0.0.1:{}/sync.php", port);
            let rurl = format!("http://127.0.0.1:{}/d", port);
            s.public_syncs.push(crate::settings::StoredPublicSync {
                write_url: wurl.clone(),
                read_url: rurl.clone(),
                drop_pub: hex::encode(sk.verifying_key().as_bytes()),
                register_url: None,
                access_pubkey: "aa".repeat(32),
                enabled: true,
                authorized_channels: vec!["lock".into()],
                status: "active".into(),
                sunset_at: None,
                revocation_reason: None,
                replacement: None,
            });
            let probe = make_signed_probe_for_test(sk, sk, &wurl, &rurl, retain_q);
            lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
        }
        state_bob.set_cached_security_policy(Some(s));
        state_bob.set_cached_internal_lifecycle(Some(lc));
    }
    let green = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-green",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(green.newly_committed);
    assert!(!green.is_replay);
    assert_eq!(green.verdict.outcome, "Confirmed");
    assert!(green.verdict.can_release_goods_automatically);
    assert!(green.verdict.is_goods_release_authorized);

    let replay = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-green",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
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

    // Case C: Multi-voucher bundle (2 vouchers) must yield Yellow:MultiVoucherManualApproval
    let _ = svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(
                        svc_alice
                            .with_wallet(|w| w.get_user_id().to_string())
                            .unwrap(),
                    ),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "40".into(),
                    ..Default::default()
                },
                layer2_anchor: Some(anchor_id),
                ..Default::default()
            },
            Some(pw),
        )
        .unwrap();
    let all_ids: Vec<String> = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()
        .into_iter()
        .map(|v| v.local_instance_id)
        .collect();
    assert_eq!(all_ids.len(), 2);
    let multi_transfer = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: state_bob
                    .lock_service()
                    .unwrap()
                    .with_wallet(|w| w.get_user_id().to_string())
                    .unwrap(),
                sources: all_ids
                    .into_iter()
                    .map(|lid| SourceTransfer {
                        local_instance_id: lid,
                        amount_to_send: "10".into(),
                    })
                    .collect(),
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();
    let multi_res = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-multi",
        &multi_transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
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
    assert!(multi_res.verdict.merchant_warning.is_some());

    h1.abort();
    h2.abort();
    drop(state_bob);
    drop(svc_alice);
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn k2_precommit_l2_verification_retries_without_quorum_and_detects_conflict() {
    use human_money_core::VoucherStatus;

    let dir_alice = temp_dir("app-k2-alice");
    let dir_bob = temp_dir("app-k2-bob");
    let dir_eve = temp_dir("app-k2-eve");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    let (mut svc_eve, _) =
        test_utils::setup_service_with_profile(&dir_eve, &ACTORS.charlie, "Eve", pw);
    svc_alice.unlock_session(pw, 600).unwrap();
    svc_bob.unlock_session(pw, 600).unwrap();
    svc_eve.unlock_session(pw, 600).unwrap();

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let eve_id_str = svc_eve
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let eve_identity = svc_eve
        .with_wallet_and_identity(|_, id| id.clone())
        .unwrap();

    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let std_uuid = std_def.immutable.identity.uuid;
    let mut standards = HashMap::new();
    standards.insert(std_uuid, std_toml.clone());

    let sk1 = SigningKey::from_bytes(&[61u8; 32]);
    let sk2 = SigningKey::from_bytes(&[62u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();

    let anchor_nonce = [55u8; 32];
    let creator_mig_pub = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![(&sk1, p1), (&sk2, p2)]
            .into_iter()
            .map(|(sk, port)| ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", port),
                read_url: format!("http://127.0.0.1:{}/d", port),
                admin_pub: sk.verifying_key().to_bytes(),
                lock_pub: sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            })
            .collect(),
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs: vec![SigningKey::from_bytes(&[98u8; 32])
                .verifying_key()
                .to_bytes()],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let tickets: Vec<GenesisTicket> = [&sk1, &sk2]
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
    svc_alice
        .register_l2_anchor_template_with_nonce(
            manifest.clone(),
            tickets,
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();
    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
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
    let local_id = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();

    // Snapshot Alice's folder before first transfer to forge a genuine conflicting transfer to Eve
    let dir_alice_clone = temp_dir("app-k2-alice-clone");
    copy_dir_recursive(&dir_alice, &dir_alice_clone);
    let mut svc_alice_clone =
        human_money_core::app_service::AppService::new(&dir_alice_clone).unwrap();
    let profile_folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
    svc_alice_clone
        .login(&profile_folder, pw, false, "test-id".into())
        .unwrap();

    let transfer_bob = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: local_id.clone(),
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    let transfer_eve = svc_alice_clone
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: eve_id_str,
                sources: vec![SourceTransfer {
                    local_instance_id: local_id,
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    let opened_eve =
        bundle_processor::open_and_verify_bundle(&eve_identity, &transfer_eve.bundle_bytes)
            .unwrap();
    let competing_entry = l2_gateway::build_voucher_lock_batch(
        &opened_eve.vouchers[0],
        0,
        &opened_eve.causal_outputs,
        opened_eve.vouchers[0]
            .transactions
            .first()
            .and_then(|t| opened_eve.root_mig_sig_for(&t.t_id)),
        DEFAULT_LOCK_POW_BASE_BITS,
    )
    .unwrap()
    .into_iter()
    .last()
    .unwrap();
    let competing_hex = hex::encode(competing_entry.to_wire_bytes().unwrap());

    // 1. Offline / unreachable committee MUST return RetryLater before touching Bob's store
    let state_bob = AppState::new(svc_bob);
    state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };
    let offline_pre = verify_bundle_l2_precommit(
        &state_bob,
        &bob_session,
        &transfer_bob.bundle_bytes,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(matches!(
        offline_pre,
        PrecommitL2Verification::RetryLater { .. }
    ));

    // 2. Committee reporting a conflicting entry MUST return ConflictDetected and atomically Quarantine on commit
    let spawn_conflict_srv = |listener: tokio::net::TcpListener,
                              sk: SigningKey,
                              comp_hex: String,
                              comp_entry: LockEntry| {
        tokio::spawn(async move {
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
                let _ = stream.write_all(http.as_bytes()).await;
            }
        })
    };

    let h1 = spawn_conflict_srv(l1, sk1, competing_hex.clone(), competing_entry.clone());
    let h2 = spawn_conflict_srv(l2, sk2, competing_hex, competing_entry);

    let conflict_pre = verify_bundle_l2_precommit(
        &state_bob,
        &bob_session,
        &transfer_bob.bundle_bytes,
        Some(pw),
    )
    .await
    .unwrap();
    let (qcs, comp_entries) = match conflict_pre {
        PrecommitL2Verification::ConflictDetected {
            quorum_certificates,
            competing_entries,
        } => (quorum_certificates, competing_entries),
        _ => panic!("Expected ConflictDetected"),
    };
    assert!(!comp_entries.is_empty());

    let evidence = human_money_core::ReceiveL2Evidence {
        quorum_certificates: &qcs,
        competing_entries: &comp_entries,
    };
    {
        let mut s = state_bob.service.lock().unwrap();
        s.receive_bundle_with_companions(
            &transfer_bob.bundle_bytes,
            &standards,
            None,
            Some(pw),
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

    h1.abort();
    h2.abort();
    drop(state_bob);
    drop(svc_alice);
    drop(svc_alice_clone);
    drop(svc_eve);
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_alice_clone);
    let _ = std::fs::remove_dir_all(dir_bob);
    let _ = std::fs::remove_dir_all(dir_eve);
}

#[tokio::test]
async fn k1_precommit_rejects_invalid_competing_entry_and_unsettled_second_voucher() {
    let dir_alice = temp_dir("app-k1-alice");
    let dir_bob = temp_dir("app-k1-bob");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let sk1 = SigningKey::from_bytes(&[71u8; 32]);
    let sk2 = SigningKey::from_bytes(&[72u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();

    let anchor_nonce = [73u8; 32];
    let creator_mig_pub = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![(&sk1, p1), (&sk2, p2)]
            .into_iter()
            .map(|(sk, port)| ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", port),
                read_url: format!("http://127.0.0.1:{}/d", port),
                admin_pub: sk.verifying_key().to_bytes(),
                lock_pub: sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            })
            .collect(),
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs: vec![SigningKey::from_bytes(&[95u8; 32])
                .verifying_key()
                .to_bytes()],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let tickets: Vec<GenesisTicket> = [&sk1, &sk2]
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
    svc_alice
        .register_l2_anchor_template_with_nonce(
            manifest.clone(),
            tickets,
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();

    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id.clone()),
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
    let lid1 = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();

    // Clone before spend so we have a valid competing LockEntry later for the multi-voucher check
    let dir_clone = temp_dir("app-k1-alice-clone");
    copy_dir_recursive(&dir_alice, &dir_clone);
    let mut svc_clone = human_money_core::app_service::AppService::new(&dir_clone).unwrap();
    let folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
    svc_clone
        .login(&folder, pw, false, "test-id".into())
        .unwrap();

    // Issue a SECOND voucher on Alice ONLY (so voucher 2 goes unsettled when server 2 drops)
    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "30".into(),
                    ..Default::default()
                },
                layer2_anchor: Some(anchor_id),
                ..Default::default()
            },
            Some(pw),
        )
        .unwrap();
    let lid2 = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()
        .into_iter()
        .find(|v| v.local_instance_id != lid1)
        .unwrap()
        .local_instance_id;

    let dir_alice_pre_single = temp_dir("app-k1-alice-single");
    copy_dir_recursive(&dir_alice, &dir_alice_pre_single);
    let mut svc_alice_single =
        human_money_core::app_service::AppService::new(&dir_alice_pre_single).unwrap();
    svc_alice_single
        .login(&folder, pw, false, "test-id".into())
        .unwrap();

    let single_bundle = svc_alice_single
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id.clone(),
                sources: vec![SourceTransfer {
                    local_instance_id: lid1.clone(),
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &HashMap::from([(
                "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
                std_toml.clone(),
            )]),
            None,
            Some(pw),
        )
        .unwrap();

    let two_voucher_bundle = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id.clone(),
                sources: vec![
                    SourceTransfer {
                        local_instance_id: lid1.clone(),
                        amount_to_send: "20".into(),
                    },
                    SourceTransfer {
                        local_instance_id: lid2.clone(),
                        amount_to_send: "15".into(),
                    },
                ],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &HashMap::from([(
                "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
                std_toml.clone(),
            )]),
            None,
            Some(pw),
        )
        .unwrap();

    // Build a REAL competing LockEntry for voucher 1, plus a CORRUPTED signature copy
    let alt_bundle = svc_clone
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: ACTORS.charlie.identity.user_id.clone(),
                sources: vec![SourceTransfer {
                    local_instance_id: lid1.clone(),
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &HashMap::from([("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), std_toml)]),
            None,
            Some(pw),
        )
        .unwrap();
    let opened_alt = bundle_processor::open_and_verify_bundle(
        &ACTORS.charlie.identity,
        &alt_bundle.bundle_bytes,
    )
    .unwrap();
    let valid_comp = l2_gateway::build_voucher_lock_batch(
        &opened_alt.vouchers[0],
        0,
        &opened_alt.causal_outputs,
        opened_alt.vouchers[0]
            .transactions
            .first()
            .and_then(|t| opened_alt.root_mig_sig_for(&t.t_id)),
        DEFAULT_LOCK_POW_BASE_BITS,
    )
    .unwrap()
    .into_iter()
    .last()
    .unwrap();
    let mut bogus_comp = valid_comp.clone();
    bogus_comp.causal_sig = [0x55; 64];

    let state_bob = AppState::new(svc_bob);
    state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };

    // Case A: Server 1 returns valid ConflictSeen receipt over `bogus_comp` (invalid causal_sig),
    // Server 2 returns HTTP 500 -> NO quorum -> MUST return RetryLater!
    let srv1_mode = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let srv1_mode_ref = srv1_mode.clone();
    let valid_comp_for_srv = valid_comp.clone();
    let bogus_comp_for_srv = bogus_comp.clone();
    let h1 = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = l1.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let mode = srv1_mode_ref.load(std::sync::atomic::Ordering::SeqCst);
            let chosen_entry = if mode == 0 {
                &bogus_comp_for_srv
            } else {
                &valid_comp_for_srv
            };
            let resp_json = if mode == 0
                || req.entries.iter().any(|h| {
                    decode_hex_fixed::<
                        { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                    >(h)
                    .ok()
                    .and_then(|b| LockEntry::from_wire_bytes(&b).ok())
                    .is_some_and(|e| e.slot == chosen_entry.slot)
                }) {
                let mut receipt = LockReceipt {
                    kind: LockReceiptKind::ConflictSeen,
                    lock_pub: sk1.verifying_key().to_bytes(),
                    bucket: req.bucket,
                    slot: chosen_entry.slot,
                    child_lock: chosen_entry.child_lock,
                    entry_hash: chosen_entry.entry_hash().unwrap(),
                    retain_until_quarter: retain_q,
                    write_seq: 1,
                    bucket_head: [9u8; 32],
                    recipient_nonce: req.recipient_nonce,
                    sig: [0u8; 64],
                };
                receipt.sig = sk1.sign(&receipt.digest()).to_bytes();
                serde_json::to_string(&SyncLockBatchResponse {
                    status: "conflict".to_string(),
                    bucket: req.bucket,
                    retain_until_quarter: retain_q,
                    receipts: vec![receipt.clone()],
                    conflict: Some(SyncLockConflict {
                        entry_index: 0,
                        slot: chosen_entry.slot,
                        receipt,
                        competing_entry: chosen_entry.to_wire_hex().unwrap(),
                    }),
                })
                .unwrap()
            } else {
                // Voucher 2 on server 1 returns FirstWrite, while server 2 fails -> Voucher 2 stays Unsettled!
                let b0 = decode_hex_fixed::<
                    { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                >(&req.entries[0])
                .unwrap();
                let e0 = LockEntry::from_wire_bytes(&b0).unwrap();
                let mut receipt = LockReceipt {
                    kind: LockReceiptKind::FirstWrite,
                    lock_pub: sk1.verifying_key().to_bytes(),
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
                receipt.sig = sk1.sign(&receipt.digest()).to_bytes();
                serde_json::to_string(&SyncLockBatchResponse {
                    status: "ok".to_string(),
                    bucket: req.bucket,
                    retain_until_quarter: retain_q,
                    receipts: vec![receipt],
                    conflict: None,
                })
                .unwrap()
            };
            let status_line = if resp_json.contains("\"conflict\"") {
                "HTTP/1.1 409 Conflict"
            } else {
                "HTTP/1.1 200 OK"
            };
            let http = format!(
                "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status_line,
                resp_json.len(),
                resp_json
            );
            let _ = stream.write_all(http.as_bytes()).await;
        }
    });
    let h2 = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = l2.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            let _ = stream.read(&mut buf).await;
            let http = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 5\r\n\r\nerror";
            let _ = stream.write_all(http.as_bytes()).await;
        }
    });

    let res_a = verify_bundle_l2_precommit(
        &state_bob,
        &bob_session,
        &single_bundle.bundle_bytes,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(
        matches!(res_a, PrecommitL2Verification::RetryLater { .. }),
        "K1: Cryptographically invalid competing LockEntry without quorum must return RetryLater, got {:?}",
        res_a
    );

    // Case B: Voucher 1 has VALID conflict, but Voucher 2 lacks Q=2 quorum -> MUST STILL return RetryLater!
    srv1_mode.store(1, std::sync::atomic::Ordering::SeqCst);
    let res_b = verify_bundle_l2_precommit(
        &state_bob,
        &bob_session,
        &two_voucher_bundle.bundle_bytes,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(
        matches!(res_b, PrecommitL2Verification::RetryLater { .. }),
        "K1: Mixed bundle with VerifiedConflict on voucher A and Unsettled voucher B must return RetryLater, got {:?}",
        res_b
    );

    h1.abort();
    h2.abort();
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_alice_pre_single);
    let _ = std::fs::remove_dir_all(dir_clone);
    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn h7_precommit_epoch_frozen_409_upgrades_manifest_and_retries() {
    use human_money_core::humoco_proto::rotation::{
        CommitteeSignature, ImportAck, MigrationCert, MigrationProposal, ProposalInitiator,
    };

    let dir_alice = temp_dir("app-h7-alice");
    let dir_bob = temp_dir("app-h7-bob");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");

    // Old server s1 (will return 409 epoch_frozen with rev2 manifest) + successor server s2
    let sk1 = SigningKey::from_bytes(&[81u8; 32]);
    let sk2 = SigningKey::from_bytes(&[82u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();

    let anchor_nonce = [83u8; 32];
    let creator_mig_sk = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
        })
        .unwrap()
        .unwrap();
    let creator_mig_pub = creator_mig_sk.verifying_key().to_bytes();
    let retain_q = encode_quarter(2035, 4).unwrap();
    let srv1_entry = ServerEntry {
        write_url: format!("http://127.0.0.1:{}/sync.php", p1),
        read_url: format!("http://127.0.0.1:{}/d", p1),
        admin_pub: sk1.verifying_key().to_bytes(),
        lock_pub: sk1.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let srv2_entry = ServerEntry {
        write_url: format!("http://127.0.0.1:{}/sync.php", p2),
        read_url: format!("http://127.0.0.1:{}/d", p2),
        admin_pub: sk2.verifying_key().to_bytes(),
        lock_pub: sk2.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let rev1 = AnchorManifest {
        revision: 1,
        threshold: 1,
        servers: vec![srv1_entry],
        successors: vec![srv2_entry.clone()],
        authority: AnchorAuthority {
            creator_mig_pub,
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
        lock_pub: sk1.verifying_key().to_bytes(),
        sig: [0u8; 64],
    };
    t1.sig = sk1.sign(&t1.digest()).to_bytes();

    // Build cryptographically valid Rev-2 manifest replacing s1 with s2
    let mut proposal = MigrationProposal {
        anchor_id,
        from_rev: 1,
        to_rev: 2,
        new_threshold: 1,
        new_servers: vec![srv2_entry],
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
        lock_pub: sk2.verifying_key().to_bytes(),
        acked_at: 1_760_050_100,
        sig: [0u8; 64],
    };
    ack2.sig = sk2.sign(&ack2.digest()).to_bytes();
    let c_digest = MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &imported_state_hash);
    let commit_sig = CommitteeSignature {
        lock_pub: sk1.verifying_key().to_bytes(),
        committed_at: 1_760_050_200,
        sig: sk1.sign(&c_digest).to_bytes(),
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

    svc_alice
        .register_l2_anchor_template_with_nonce(
            rev1.clone(),
            vec![t1],
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();
    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
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
    let lid = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();
    let bundle_res = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: lid,
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &HashMap::from([("a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(), std_toml)]),
            None,
            Some(pw),
        )
        .unwrap();

    let rev2_for_s1 = rev2.clone();
    let h1 = tokio::spawn(async move {
        let Ok((mut stream, _)) = l1.accept().await else {
            return;
        };
        let mut buf = [0u8; 65_536];
        let _ = stream.read(&mut buf).await;
        let body = serde_json::json!({
            "error": "epoch_frozen",
            "latest_manifest": rev2_for_s1,
        })
        .to_string();
        let http = format!(
            "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(http.as_bytes()).await;
    });

    let h2 = tokio::spawn(async move {
        let Ok((mut stream, _)) = l2.accept().await else {
            return;
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
                let b = decode_hex_fixed::<
                    { human_money_core::humoco_proto::causal::LOCK_ENTRY_WIRE_LEN },
                >(hex_str)
                .unwrap();
                let entry = LockEntry::from_wire_bytes(&b).unwrap();
                let mut r = LockReceipt {
                    kind: LockReceiptKind::FirstWrite,
                    lock_pub: sk2.verifying_key().to_bytes(),
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
                r.sig = sk2.sign(&r.digest()).to_bytes();
                r
            })
            .collect();
        let resp = serde_json::to_string(&SyncLockBatchResponse {
            status: "ok".to_string(),
            bucket: req.bucket,
            retain_until_quarter: retain_q,
            receipts,
            conflict: None,
        })
        .unwrap();
        let http = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            resp.len(),
            resp
        );
        let _ = stream.write_all(http.as_bytes()).await;
    });

    let state_bob = AppState::new(svc_bob);
    state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };

    let upgraded =
        verify_bundle_l2_precommit(&state_bob, &bob_session, &bundle_res.bundle_bytes, Some(pw))
            .await
            .unwrap();
    assert!(
        matches!(upgraded, PrecommitL2Verification::CleanQuorum { .. }),
        "H7: Precommit must upgrade rev-1 -> rev-2 on 409 epoch_frozen and succeed on s2, got {:?}",
        upgraded
    );

    let stored_rev = state_bob
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

    h1.abort();
    h2.abort();
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_bob);
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

#[tokio::test]
async fn test_s04_f2_already_in_history_replay_never_fabricates_confirmed() {
    let dir_alice = temp_dir("s04-f2-alice");
    let dir_bob = temp_dir("s04-f2-bob");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    svc_alice.unlock_session(pw, 600).unwrap();
    svc_bob.unlock_session(pw, 600).unwrap();

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "50".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(pw),
        )
        .unwrap();
    let lid = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();
    let transfer = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: lid,
                    amount_to_send: "10".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    // Bob receives the N=0 bundle directly into wallet history (NOT via checkout_purchase)
    svc_bob
        .receive_bundle_with_companions(
            &transfer.bundle_bytes,
            &standards,
            None,
            Some(pw),
            false,
            None,
            |_, _| Ok(vec![]),
        )
        .unwrap();

    let state_bob = AppState::new(svc_bob);
    state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };
    let replay = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-replay-hist",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(replay.is_replay);
    assert_ne!(
        replay.verdict.outcome, "Confirmed",
        "S04-F2: already_in_history N=0 replay must NEVER fabricate Confirmed"
    );
    assert_eq!(replay.verdict.outcome, "OfflineOnly");
    assert!(!replay.verdict.is_goods_release_authorized);

    // S04-F2: if the already-received voucher is Quarantined, replay MUST report Quarantined
    {
        let mut s = state_bob.service.lock().unwrap();
        let (wallet, _) = s.get_unlocked_mut_for_test();
        for inst in wallet.voucher_store.vouchers.values_mut() {
            inst.status = human_money_core::VoucherStatus::Quarantined {
                reason: "Upstream conflict".into(),
            };
        }
    }
    let replay_q = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-replay-quarantined",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(replay_q.is_replay);
    assert_eq!(
        replay_q.verdict.outcome,
        "Quarantined:BundleAlreadyProcessedQuarantined"
    );
    assert!(!replay_q.verdict.is_goods_release_authorized);

    // S04-F7: stale SessionToken generation MUST be rejected immediately
    let stale_session = crate::session::SessionToken {
        generation: bob_session.generation.saturating_add(99),
        owner_did: bob_session.owner_did.clone(),
    };
    let stale_err = checkout_bundle(
        &state_bob,
        &stale_session,
        "pos-stale",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .unwrap_err();
    assert!(
        stale_err.message.contains("session changed"),
        "S04-F7: stale session token must be rejected, got: {:?}",
        stale_err
    );

    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn test_s04_f3_f8_f9_trusted_lock_keys_delegation_binding_and_lucke03_isolation() {
    let dir_bob = temp_dir("s04-f3-f8-f9-bob");
    let pw = "test-password";
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    svc_bob.unlock_session(pw, 600).unwrap();

    let retain_q = encode_quarter(2035, 4).unwrap();
    let sk_unsigned = SigningKey::from_bytes(&[181u8; 32]);
    let sk_contact = SigningKey::from_bytes(&[182u8; 32]);
    let sk_offender_bridge = SigningKey::from_bytes(&[183u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[184u8; 32]);

    let url_unsigned = "https://unsigned.example/sync.php";
    let url_contact = "https://hop1-contact.example/sync.php";
    let url_bridge = "https://hop2-via-offender.example/sync.php";

    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![
            ServerEntry {
                write_url: url_unsigned.to_string(),
                read_url: "https://unsigned.example/d".to_string(),
                admin_pub: sk_unsigned.verifying_key().to_bytes(),
                lock_pub: sk_unsigned.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            },
            ServerEntry {
                write_url: url_contact.to_string(),
                read_url: "https://hop1-contact.example/d".to_string(),
                admin_pub: sk_contact.verifying_key().to_bytes(),
                lock_pub: sk_contact.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            },
            ServerEntry {
                write_url: url_bridge.to_string(),
                read_url: "https://hop2-via-offender.example/d".to_string(),
                admin_pub: sk_offender_bridge.verifying_key().to_bytes(),
                lock_pub: sk_offender_bridge.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            },
        ],
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
            guarantor_mig_pubs: vec![[9u8; 32]],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };

    // Mark Alice as KnownOffender in Bob's wallet proof_store (S04-F8 bridge test)
    let alice_did = ACTORS.alice.identity.user_id.clone();
    let charlie_did = ACTORS.charlie.identity.user_id.clone();
    let dave_did = ACTORS.david.identity.user_id.clone();
    {
        use human_money_core::models::conflict::{
            ConflictRole, ProofOfDoubleSpend, ProofStoreEntry,
        };
        let bob_uid = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let (wallet, _) = svc_bob.get_unlocked_mut_for_test();
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

    let state_bob = AppState::new(svc_bob);
    let session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };

    // Security policy pins ONLY url_unsigned (whose probe lacks a signed delegation!)
    let mut policy = SecurityPolicy::default();
    policy.market_selected_syncs.push(
        humoco_market::DropDescriptor {
            write_url: url_unsigned.to_string(),
            read_url: "https://unsigned.example/d".to_string(),
            slot_id: None,
            drop_pub: hex::encode(sk_unsigned.verifying_key().as_bytes()),
        }
        .into(),
    );

    let mut lc = InternalLifecycleState::default();
    // S04-F3: probe with delegation: None must NOT enter VerifiedServerBinding
    let mut unsigned_probe = make_signed_probe_for_test(
        &sk_unsigned,
        &sk_unsigned,
        url_unsigned,
        "https://unsigned.example/d",
        retain_q,
    );
    unsigned_probe.delegation = None;
    lc.l2_probe_cache
        .insert(unsigned_probe.lock_pub.clone(), unsigned_probe);
    // S04-F9: valid signed probe for Hop-1 contact Charlie
    let p_contact = make_signed_probe_for_test(
        &sk_contact,
        &sk_contact,
        url_contact,
        "https://hop1-contact.example/d",
        retain_q,
    );
    lc.l2_probe_cache
        .insert(p_contact.lock_pub.clone(), p_contact);
    // S04-F8: valid signed probe for Hop-2 contact Dave (reachable ONLY through KnownOffender Alice!)
    let p_bridge = make_signed_probe_for_test(
        &sk_offender_bridge,
        &sk_offender_bridge,
        url_bridge,
        "https://hop2-via-offender.example/d",
        retain_q,
    );
    lc.l2_probe_cache
        .insert(p_bridge.lock_pub.clone(), p_bridge);

    // Populate AddressBook (Charlie trusted at Hop-1; Dave untrusted directly, reachable only via Alice)
    let mut book = crate::models::FrontendAddressBook::default();
    book.contacts.insert(
        charlie_did.clone(),
        crate::models::FrontendContact {
            did: charlie_did.clone(),
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
                drop_pub: hex::encode(sk_contact.verifying_key().as_bytes()),
            }
            .into()],
        },
    );
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
                read_url: "https://hop2-via-offender.example/d".to_string(),
                slot_id: None,
                drop_pub: hex::encode(sk_offender_bridge.verifying_key().as_bytes()),
            }
            .into()],
        },
    );

    // Populate WoT graph: Bob -> Alice (KnownOffender, level 2) -> Dave (level 2)
    let bob_can = canonicalize_did(&ACTORS.bob.identity.user_id).unwrap();
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
    if let Some(ref signed) = alice_reg.own_signed_list {
        reg.raw_list_store.insert(alice_can, signed.clone());
        reg.invalidate_graph_cache();
    }
    {
        let s = state_bob.service.lock().unwrap();
        state_bob.write_cache_slot_for_service(&s, |c| &mut c.contacts, Some(book));
        state_bob.write_cache_slot_for_service(&s, |c| &mut c.wot, Some(reg));
    }

    let client = sync_client::create_sync_http_client();
    let (base_trusted, independently_pinned) = resolve_trusted_lock_keys(
        &state_bob,
        &session,
        &policy,
        &lc,
        &client,
        Some(&alice_did),
        Some(&manifest),
        Some(pw),
    )
    .await
    .unwrap();

    assert!(
        !base_trusted.contains(&sk_unsigned.verifying_key().to_bytes()),
        "S04-F3: unsigned cached probe (delegation: None) must NOT be trusted"
    );
    assert!(
        base_trusted.contains(&sk_contact.verifying_key().to_bytes()),
        "S04-F9: Hop-1 contact sync must be in base_trusted_keys"
    );
    assert!(
        !independently_pinned.contains(&sk_contact.verifying_key().to_bytes()),
        "S04-F9: transitive Hop-1 contact sync must NOT enter independently_pinned_creator_keys"
    );
    assert!(
        !base_trusted.contains(&sk_offender_bridge.verifying_key().to_bytes()),
        "S04-F8: Hop-2 contact reachable only via KnownOffender bridge must NOT be trusted"
    );

    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn test_s04_f1_precommit_rejects_wrong_nonce_and_revoked_server_and_uses_default_pow() {
    let dir_alice = temp_dir("s04-f1-pre-alice");
    let dir_alice_clone = temp_dir("s04-f1-pre-clone");
    let dir_bob = temp_dir("s04-f1-pre-bob");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    svc_alice.unlock_session(pw, 600).unwrap();
    svc_bob.unlock_session(pw, 600).unwrap();

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

    let sk1 = SigningKey::from_bytes(&[191u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let anchor_nonce = [192u8; 32];
    let creator_mig_pub = svc_alice
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
            write_url: format!("http://127.0.0.1:{}/sync.php", p1),
            read_url: format!("http://127.0.0.1:{}/d", p1),
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
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let mut t1 = GenesisTicket {
        anchor_id,
        retain_until_quarter: retain_q,
        issued_at: 1_760_000_000,
        lock_pub: sk1.verifying_key().to_bytes(),
        sig: [0u8; 64],
    };
    t1.sig = sk1.sign(&t1.digest()).to_bytes();
    svc_alice
        .register_l2_anchor_template_with_nonce(
            manifest.clone(),
            vec![t1],
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();
    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
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
    let lid = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();

    copy_dir_recursive(&dir_alice, &dir_alice_clone);
    let mut svc_clone = human_money_core::app_service::AppService::new(&dir_alice_clone).unwrap();
    let folder = svc_alice.list_profiles().unwrap()[0].folder_name.clone();
    svc_clone
        .login(&folder, pw, false, "test-id".into())
        .unwrap();
    let alt_bundle = svc_clone
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: ACTORS.charlie.identity.user_id.clone(),
                sources: vec![SourceTransfer {
                    local_instance_id: lid.clone(),
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();
    let opened_alt = bundle_processor::open_and_verify_bundle(
        &ACTORS.charlie.identity,
        &alt_bundle.bundle_bytes,
    )
    .unwrap();
    let valid_comp = l2_gateway::build_voucher_lock_batch(
        &opened_alt.vouchers[0],
        0,
        &opened_alt.causal_outputs,
        opened_alt.vouchers[0]
            .transactions
            .first()
            .and_then(|t| opened_alt.root_mig_sig_for(&t.t_id)),
        DEFAULT_LOCK_POW_BASE_BITS,
    )
    .unwrap()
    .into_iter()
    .last()
    .unwrap();

    let transfer_bob = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: lid,
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    // Mode 0: wrong recipient_nonce; Mode 1: valid FirstWrite (tested against revoked offender); Mode 2: ConflictSeen with 8-bit competing entry
    let mode = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let mode_ref = mode.clone();
    let comp_for_srv = valid_comp.clone();
    let sk1_for_srv = sk1.clone();
    let h1 = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = l1.accept().await else {
                break;
            };
            let mut buf = [0u8; 65_536];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]);
            let body_str = raw.split("\r\n\r\n").nth(1).unwrap_or("");
            let req: SyncLockBatchRequest = serde_json::from_str(body_str).unwrap();
            let m = mode_ref.load(std::sync::atomic::Ordering::SeqCst);
            let (status_line, resp_json) = if m == 2 {
                let mut r = LockReceipt {
                    kind: LockReceiptKind::ConflictSeen,
                    lock_pub: sk1_for_srv.verifying_key().to_bytes(),
                    bucket: req.bucket,
                    slot: comp_for_srv.slot,
                    child_lock: comp_for_srv.child_lock,
                    entry_hash: comp_for_srv.entry_hash().unwrap(),
                    retain_until_quarter: retain_q,
                    write_seq: 1,
                    bucket_head: [9u8; 32],
                    recipient_nonce: req.recipient_nonce,
                    sig: [0u8; 64],
                };
                r.sig = sk1_for_srv.sign(&r.digest()).to_bytes();
                let body = serde_json::to_string(&SyncLockBatchResponse {
                    status: "conflict".into(),
                    bucket: req.bucket,
                    retain_until_quarter: retain_q,
                    receipts: vec![],
                    conflict: Some(SyncLockConflict {
                        entry_index: 0,
                        slot: comp_for_srv.slot,
                        receipt: r,
                        competing_entry: comp_for_srv.to_wire_hex().unwrap(),
                    }),
                })
                .unwrap();
                ("HTTP/1.1 409 Conflict", body)
            } else {
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
                            lock_pub: sk1_for_srv.verifying_key().to_bytes(),
                            bucket: req.bucket,
                            slot: e.slot,
                            child_lock: e.child_lock,
                            entry_hash: e.entry_hash().unwrap(),
                            retain_until_quarter: retain_q,
                            write_seq: (i as u64) + 1,
                            bucket_head: [8u8; 32],
                            recipient_nonce: if m == 0 {
                                [0xee; 32]
                            } else {
                                req.recipient_nonce
                            },
                            sig: [0u8; 64],
                        };
                        r.sig = sk1_for_srv.sign(&r.digest()).to_bytes();
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
                ("HTTP/1.1 200 OK", body)
            };
            let http = format!(
                "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                status_line,
                resp_json.len(),
                resp_json
            );
            let _ = stream.write_all(http.as_bytes()).await;
        }
    });

    let state_bob = AppState::new(svc_bob);
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };
    let mut lc = InternalLifecycleState::default();
    let mut probe = make_signed_probe_for_test(
        &sk1,
        &sk1,
        &format!("http://127.0.0.1:{}/sync.php", p1),
        &format!("http://127.0.0.1:{}/d", p1),
        retain_q,
    );
    probe.pow_base_bits = 18;
    lc.l2_probe_cache.insert(probe.lock_pub.clone(), probe);
    state_bob.set_cached_internal_lifecycle(Some(lc));

    // 1. Wrong nonce MUST NOT yield CleanQuorum
    let res_wrong_nonce = verify_bundle_l2_precommit(
        &state_bob,
        &bob_session,
        &transfer_bob.bundle_bytes,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(
        matches!(res_wrong_nonce, PrecommitL2Verification::RetryLater { .. }),
        "S04-F1: wrong recipient_nonce must return RetryLater, got {:?}",
        res_wrong_nonce
    );

    // 2. Valid FirstWrite from revoked server MUST NOT yield CleanQuorum
    mode.store(1, std::sync::atomic::Ordering::SeqCst);
    {
        let mut offenders = humoco_wot::ServerOffenderList::new();
        offenders
            .revoked_lock_pubs
            .insert(sk1.verifying_key().to_bytes());
        let s = state_bob.service.lock().unwrap();
        state_bob.write_cache_slot_for_service(&s, |c| &mut c.server_offenders, Some(offenders));
    }
    let res_revoked = verify_bundle_l2_precommit(
        &state_bob,
        &bob_session,
        &transfer_bob.bundle_bytes,
        Some(pw),
    )
    .await
    .unwrap();
    assert!(
        matches!(res_revoked, PrecommitL2Verification::RetryLater { .. }),
        "S04-F1: revoked server receipt must return RetryLater, got {:?}",
        res_revoked
    );

    // 3. Historical 8-bit competing entry MUST verify despite live probe pow_base_bits = 18
    mode.store(2, std::sync::atomic::Ordering::SeqCst);
    let res_conflict = verify_bundle_l2_precommit(
        &state_bob,
        &bob_session,
        &transfer_bob.bundle_bytes,
        Some(pw),
    )
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

    h1.abort();
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_alice_clone);
    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn test_s04_f3_live_equivocation_revokes_admin_sibling_before_checkout_evaluation() {
    let dir_alice = temp_dir("s04-f3-equiv-alice");
    let dir_bob = temp_dir("s04-f3-equiv-bob");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    svc_alice.unlock_session(pw, 600).unwrap();
    svc_bob.unlock_session(pw, 600).unwrap();

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

    // N=3, Q=2: Server 1 & sibling Server 2 share admin_pub A; Server 3 has independent admin_pub B.
    // Even though Server 2 + Server 3 return clean FirstWrite (meeting Q=2), Server 1's live
    // equivocation MUST revoke admin_pub A and prune sibling Server 2 before evaluate_bundle_checkout
    // so only 1 trusted server (< N - Q + 1 = 2) remains and verdict is NOT Confirmed!
    let admin_sk = SigningKey::from_bytes(&[201u8; 32]);
    let sk_lock1 = SigningKey::from_bytes(&[202u8; 32]);
    let sk_lock2 = SigningKey::from_bytes(&[203u8; 32]);
    let sk_lock3 = SigningKey::from_bytes(&[205u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();
    let l3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p3 = l3.local_addr().unwrap().port();

    let anchor_nonce = [204u8; 32];
    let creator_mig_pub = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
                .map(|k| k.verifying_key().to_bytes())
        })
        .unwrap()
        .unwrap();
    let retain_q = encode_quarter(2035, 4).unwrap();
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![
            ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", p1),
                read_url: format!("http://127.0.0.1:{}/d", p1),
                admin_pub: admin_sk.verifying_key().to_bytes(),
                lock_pub: sk_lock1.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            },
            ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", p2),
                read_url: format!("http://127.0.0.1:{}/d", p2),
                admin_pub: admin_sk.verifying_key().to_bytes(),
                lock_pub: sk_lock2.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            },
            ServerEntry {
                write_url: format!("http://127.0.0.1:{}/sync.php", p3),
                read_url: format!("http://127.0.0.1:{}/d", p3),
                admin_pub: sk_lock3.verifying_key().to_bytes(),
                lock_pub: sk_lock3.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            },
        ],
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub,
            guarantor_mig_pubs: vec![[9u8; 32]],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: retain_q,
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let tickets: Vec<GenesisTicket> = [&sk_lock1, &sk_lock2, &sk_lock3]
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
    svc_alice
        .register_l2_anchor_template_with_nonce(
            manifest.clone(),
            tickets,
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();
    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
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
    let lid = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();
    let transfer = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: lid,
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    // Server 1 equivocates: signs TWO conflicting FirstWrite receipts for the same slot!
    let sk1_srv = sk_lock1.clone();
    let h1 = tokio::spawn(async move {
        if let Ok((mut stream, _)) = l1.accept().await {
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
                lock_pub: sk1_srv.verifying_key().to_bytes(),
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
            fw_a.sig = sk1_srv.sign(&fw_a.digest()).to_bytes();

            let mut fw_b = fw_a.clone();
            fw_b.child_lock = [0x77; 32];
            fw_b.entry_hash = [0x88; 32];
            fw_b.write_seq = 2;
            fw_b.sig = sk1_srv.sign(&fw_b.digest()).to_bytes();
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
            let _ = stream.write_all(http.as_bytes()).await;
        }
    });
    let spawn_clean_fw = |listener: tokio::net::TcpListener, sk: SigningKey| {
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
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
                            bucket_head: [3u8; 32],
                            recipient_nonce: req.recipient_nonce,
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
                let _ = stream.write_all(http.as_bytes()).await;
            }
        })
    };
    let h2 = spawn_clean_fw(l2, sk_lock2.clone());
    let h3 = spawn_clean_fw(l3, sk_lock3.clone());

    let state_bob = AppState::new(svc_bob);
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };
    let mut policy = SecurityPolicy::default();
    let mut lc = InternalLifecycleState::default();
    for (port, adm_sk, lock_sk) in [
        (p1, &admin_sk, &sk_lock1),
        (p2, &admin_sk, &sk_lock2),
        (p3, &sk_lock3, &sk_lock3),
    ] {
        let wurl = format!("http://127.0.0.1:{}/sync.php", port);
        let rurl = format!("http://127.0.0.1:{}/d", port);
        policy.public_syncs.push(crate::settings::StoredPublicSync {
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
    state_bob.set_cached_security_policy(Some(policy));
    state_bob.set_cached_internal_lifecycle(Some(lc));

    let res = checkout_bundle(
        &state_bob,
        &bob_session,
        "pos-equiv-sibling",
        &transfer.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .unwrap();
    assert_ne!(
        res.verdict.outcome, "Confirmed",
        "S04-F3: pruning equivocating admin_pub sibling must drop trusted count below N-Q+1=2 and prevent Confirmed"
    );

    // Verify sibling lock_pub2 is now revoked via admin_pub A in ServerOffenderList!
    let offenders = {
        let mut s = state_bob.service.lock().unwrap();
        state_bob
            .get_or_load_server_offender_list(&mut s, Some(pw))
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

    h1.abort();
    h2.abort();
    h3.abort();
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn test_s04_f6_inline_less_409_epoch_frozen_triggers_discovery() {
    use human_money_core::humoco_proto::rotation::{
        CommitteeSignature, ImportAck, MigrationCert, MigrationProposal, ProposalInitiator,
    };

    let dir_alice = temp_dir("s04-f6-disc-alice");
    let dir_bob = temp_dir("s04-f6-disc-bob");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    svc_alice.unlock_session(pw, 600).unwrap();
    svc_bob.unlock_session(pw, 600).unwrap();

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

    let sk1 = SigningKey::from_bytes(&[211u8; 32]);
    let sk2 = SigningKey::from_bytes(&[212u8; 32]);
    let l1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = l1.local_addr().unwrap().port();
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p2 = l2.local_addr().unwrap().port();
    let l_disc = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p_disc = l_disc.local_addr().unwrap().port();

    let anchor_nonce = [213u8; 32];
    let creator_mig_sk = svc_alice
        .with_wallet_and_identity(|_, id| {
            l2_gateway::derive_anchor_mig_signing_key(id, &anchor_nonce)
        })
        .unwrap()
        .unwrap();
    let retain_q = encode_quarter(2035, 4).unwrap();
    let srv1 = ServerEntry {
        write_url: format!("http://127.0.0.1:{}/sync.php", p1),
        read_url: format!("http://127.0.0.1:{}/d", p1),
        admin_pub: sk1.verifying_key().to_bytes(),
        lock_pub: sk1.verifying_key().to_bytes(),
        nominated_by: NominatedBy::Creator,
    };
    let srv2 = ServerEntry {
        write_url: format!("http://127.0.0.1:{}/sync.php", p2),
        read_url: format!("http://127.0.0.1:{}/d", p2),
        admin_pub: sk2.verifying_key().to_bytes(),
        lock_pub: sk2.verifying_key().to_bytes(),
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
        lock_pub: sk1.verifying_key().to_bytes(),
        sig: [0u8; 64],
    };
    t1.sig = sk1.sign(&t1.digest()).to_bytes();

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
    let imported_state_hash = [0x44u8; 32];
    let prop_hash = proposal.core_digest().unwrap();
    let mut ack2 = ImportAck {
        anchor_id,
        proposal_hash: prop_hash,
        to_rev: 2,
        imported_state_hash,
        retain_until_quarter: retain_q,
        lock_pub: sk2.verifying_key().to_bytes(),
        acked_at: 1_760_050_100,
        sig: [0u8; 64],
    };
    ack2.sig = sk2.sign(&ack2.digest()).to_bytes();
    let c_digest = MigrationCert::commit_digest(&anchor_id, &prop_hash, 2, &imported_state_hash);
    let commit_sig = CommitteeSignature {
        lock_pub: sk1.verifying_key().to_bytes(),
        committed_at: 1_760_050_200,
        sig: sk1.sign(&c_digest).to_bytes(),
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

    svc_alice
        .register_l2_anchor_template_with_nonce(
            rev1.clone(),
            vec![t1],
            Some(&anchor_nonce),
            Some(pw),
        )
        .unwrap();
    svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
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
    let lid = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();
    let bundle_res = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: lid,
                    amount_to_send: "20".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    // Server 1 returns inline-less `409 epoch_frozen` (NO latest_manifest!)
    let h1 = tokio::spawn(async move {
        if let Ok((mut stream, _)) = l1.accept().await {
            let mut buf = [0u8; 65_536];
            let _ = stream.read(&mut buf).await;
            let body = r#"{"error":"epoch_frozen"}"#;
            let http = format!(
                "HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(http.as_bytes()).await;
        }
    });
    // Discovery server hosts Rev-2 manifest at `/anchors/{anchor_hex}.bin`
    let rev2_bytes = serde_json::to_vec(&rev2).unwrap();
    let expected_path = format!("/d/anchors/{}.bin", hex::encode(anchor_id));
    let h_disc = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = l_disc.accept().await else {
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
                let _ = stream.write_all(hdr.as_bytes()).await;
                let _ = stream.write_all(&rev2_bytes).await;
            } else {
                let _ = stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await;
            }
        }
    });
    // Server 2 handles the retried Round-2 batch on Rev 2
    let h2 = tokio::spawn(async move {
        if let Ok((mut stream, _)) = l2.accept().await {
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
                        lock_pub: sk2.verifying_key().to_bytes(),
                        bucket: req.bucket,
                        slot: e.slot,
                        child_lock: e.child_lock,
                        entry_hash: e.entry_hash().unwrap(),
                        retain_until_quarter: retain_q,
                        write_seq: (i as u64) + 1,
                        bucket_head: [7u8; 32],
                        recipient_nonce: req.recipient_nonce,
                        sig: [0u8; 64],
                    };
                    r.sig = sk2.sign(&r.digest()).to_bytes();
                    r
                })
                .collect();
            let resp = serde_json::to_string(&SyncLockBatchResponse {
                status: "ok".into(),
                bucket: req.bucket,
                retain_until_quarter: retain_q,
                receipts,
                conflict: None,
            })
            .unwrap();
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                resp.len(),
                resp
            );
            let _ = stream.write_all(http.as_bytes()).await;
        }
    });

    let state_bob = AppState::new(svc_bob);
    let mut policy = SecurityPolicy::default();
    policy.public_syncs.push(crate::settings::StoredPublicSync {
        write_url: format!("http://127.0.0.1:{}/sync.php", p_disc),
        read_url: format!("http://127.0.0.1:{}/d", p_disc),
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
    state_bob.set_cached_security_policy(Some(policy));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };

    let upgraded =
        verify_bundle_l2_precommit(&state_bob, &bob_session, &bundle_res.bundle_bytes, Some(pw))
            .await
            .unwrap();
    assert!(
        matches!(upgraded, PrecommitL2Verification::CleanQuorum { .. }),
        "S04-F6: inline-less 409 epoch_frozen must trigger discovery and succeed on Rev 2, got {:?}",
        upgraded
    );

    h1.abort();
    h2.abort();
    h_disc.abort();
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_bob);
}

#[tokio::test]
async fn test_s04_q2_checkout_bundle_degrades_to_yellow_when_manifest_missing_and_allows_offline_commit(
) {
    let dir_alice = temp_dir("s04-q2-missing-manifest-alice");
    let dir_bob = temp_dir("s04-q2-missing-manifest-bob");
    let pw = "test-password";

    let (mut svc_alice, _) =
        test_utils::setup_service_with_profile(&dir_alice, &ACTORS.alice, "Alice", pw);
    let (mut svc_bob, _) = test_utils::setup_service_with_profile(&dir_bob, &ACTORS.bob, "Bob", pw);
    svc_alice.unlock_session(pw, 600).unwrap();
    svc_bob.unlock_session(pw, 600).unwrap();

    let alice_id = svc_alice
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_id = svc_bob
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let std_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let (std_def, _) = VoucherStandardDefinition::from_toml(&std_toml).unwrap();
    let standards = HashMap::from([(std_def.immutable.identity.uuid, std_toml.clone())]);

    let anchor_id = [0xD4u8; 32];
    let voucher = svc_alice
        .create_new_voucher(
            &std_toml,
            NewVoucherData {
                creator_profile: PublicProfile {
                    id: Some(alice_id),
                    ..Default::default()
                },
                nominal_value: ValueDefinition {
                    amount: "40".into(),
                    ..Default::default()
                },
                layer2_anchor: Some(anchor_id),
                ..Default::default()
            },
            Some(pw),
        )
        .unwrap();
    let lid = svc_alice
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap()[0]
        .local_instance_id
        .clone();
    let bundle_res = svc_alice
        .create_transfer_bundle(
            MultiTransferRequest {
                recipient_id: bob_id,
                sources: vec![SourceTransfer {
                    local_instance_id: lid,
                    amount_to_send: "15".into(),
                }],
                notes: None,
                sender_profile_name: None,
                use_privacy_mode: None,
                wot_sync_json: None,
                contact_card_json: None,
            },
            &standards,
            None,
            Some(pw),
        )
        .unwrap();

    let state_bob = AppState::new(svc_bob);
    state_bob.set_cached_ui_preferences(Some(crate::settings::UiPreferences::default()));
    state_bob.set_cached_security_policy(Some(SecurityPolicy::default()));
    state_bob.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));
    let bob_session = {
        let s = state_bob.service.lock().unwrap();
        state_bob.session_token_for_service(&s).unwrap()
    };

    let completion = checkout_bundle(
        &state_bob,
        &bob_session,
        "purchase-s04-q2-missing-manifest",
        &bundle_res.bundle_bytes,
        &standards,
        false,
        Some(pw),
    )
    .await
    .expect("missing manifest must degrade gracefully to Yellow, not hard-fail");

    assert!(!completion.newly_committed);
    assert!(!completion.is_replay);
    assert!(
        completion.verdict.outcome.starts_with("Yellow"),
        "Expected Yellow outcome, got {}",
        completion.verdict.outcome
    );
    assert!(completion.verdict.requires_merchant_approval);
    assert!(!completion.verdict.can_release_goods_automatically);
    assert!(completion.receive_payload.is_some());

    let recv = crate::services::transfer_service::receive_bundle_impl(
        None,
        bundle_res.bundle_bytes,
        standards,
        Some(pw.to_string()),
        false,
        Some(crate::models::ReceiveCommitMode::ForceTrustOffline),
        None,
        &state_bob,
    )
    .await
    .expect("ForceTrustOffline must commit anchored voucher even without manifest");

    assert_eq!(recv.newly_committed, Some(true));
    let active_count = state_bob
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| {
            w.voucher_store
                .vouchers
                .values()
                .filter(|inst| {
                    inst.voucher.voucher_id == voucher.voucher_id
                        && matches!(inst.status, human_money_core::VoucherStatus::Active)
                })
                .count()
        })
        .unwrap();
    assert_eq!(active_count, 1);

    drop(state_bob);
    drop(svc_alice);
    let _ = std::fs::remove_dir_all(dir_alice);
    let _ = std::fs::remove_dir_all(dir_bob);
}
