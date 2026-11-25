use std::collections::HashMap;

use chrono::Utc;
use human_money_core::models::secure_container::{ContainerConfig, PrivacyMode};
use human_money_core::test_utils;
use humoco_market::{InquiryTarget, DROP_POST_MAX_BYTES};

use super::*;
use crate::models::{
    FrontendAdInput, FrontendContact, FrontendInquiryTarget, FrontendSaveMyMarketProfileRequest,
    FrontendSyncDescriptor, FrontendUserProfile, TransactionRecord,
};
use crate::services::fs_helpers::TempDirGuard;
use crate::services::market_service::{
    save_my_market_profile_impl, set_card_published_impl, upsert_ad_impl,
};
use crate::services::sync_engine::{probe_public_sync_impl, save_public_sync_impl};
use crate::test_support::{
    create_test_state, setup_test_app, temp_test_dir, MockSyncServer, ALICE_PHRASE, BOB_PHRASE,
};

const ADMIN_A: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const ADMIN_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const DROP_PUB_A: &str = ADMIN_A;
const DROP_PUB_B: &str = ADMIN_B;
const FREETALER_UUID: &str = "a1b2c3d4-e5f6-4789-8012-3456789abcde";

struct TwoPartySyncFixture {
    alice_state: AppState,
    bob_state: AppState,
    alice_did: String,
    bob_did: String,
    #[allow(dead_code)] // Keeps MockSyncServer HTTP background task alive for the fixture's lifetime.
    server_a: MockSyncServer,
    #[allow(dead_code)]
    server_b: MockSyncServer,
    bob_syncs: Vec<FrontendSyncDescriptor>,
}

fn wallet_user_id(state: &AppState) -> String {
    state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap()
}

fn make_sync_desc(
    write_url: &str,
    server: &MockSyncServer,
    drop_pub: &str,
    access_pubkey: &str,
    prefix: Option<&str>,
) -> FrontendSyncDescriptor {
    FrontendSyncDescriptor {
        write_url: write_url.to_string(),
        read_url: format!("{}/{}", server.base_url, server.data_dir),
        slot_id: prefix.map(|p| humoco_market::format_slot_id(access_pubkey, Some(p)).unwrap()),
        drop_pub: drop_pub.to_string(),
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "active".to_string(),
        last_probed_at: None,
        ..Default::default()
    }
}

fn save_test_contact(
    state: &AppState,
    did: &str,
    first_name: &str,
    trust_level: Option<u8>,
    syncs: Vec<FrontendSyncDescriptor>,
    password: &str,
) {
    let mut s = state.service.lock().unwrap();
    let contact = FrontendContact {
        did: did.to_string(),
        profile: FrontendUserProfile {
            first_name: Some(first_name.to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level,
        desired_hops: Some(3),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs,
    };
    state.save_contact(&mut s, contact, Some(password)).unwrap();
}

fn load_freetaler_std_map() -> (String, HashMap<String, String>) {
    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(FREETALER_UUID.to_string(), standard_toml.clone());
    (standard_toml, std_map)
}

fn create_freetaler_voucher(
    state: &AppState,
    creator_did: &str,
    amount: &str,
    layer2_anchor: Option<[u8; 32]>,
    password: &str,
) -> String {
    let (standard_toml, _) = load_freetaler_std_map();
    let mut s = state.service.lock().unwrap();
    let voucher = s
        .create_new_voucher(
            &standard_toml,
            human_money_core::NewVoucherData {
                creator_profile: human_money_core::models::profile::PublicProfile {
                    id: Some(creator_did.to_string()),
                    first_name: Some("Alice".to_string()),
                    ..Default::default()
                },
                nominal_value: human_money_core::models::voucher::ValueDefinition {
                    amount: amount.to_string(),
                    unit: "Taler".to_string(),
                    ..Default::default()
                },
                validity_duration: Some("P1Y".to_string()),
                non_redeemable_test_voucher: false,
                collateral: None,
                layer2_anchor,
            },
            Some(password),
        )
        .unwrap();
    human_money_core::Wallet::calculate_local_instance_id(&voucher, creator_did).unwrap()
}

fn create_and_record_transfer_bundle(
    state: &AppState,
    recipient_did: &str,
    local_id: &str,
    amount: &str,
    std_map: &HashMap<String, String>,
    password: &str,
) -> (String, Vec<u8>) {
    let sender_did = wallet_user_id(state);
    let transfer_req = human_money_core::wallet::types::MultiTransferRequest {
        recipient_id: recipient_did.to_string(),
        sources: vec![human_money_core::wallet::types::SourceTransfer {
            local_instance_id: local_id.to_string(),
            amount_to_send: amount.to_string(),
        }],
        notes: Some("Post transfer test".to_string()),
        sender_profile_name: Some("Alice".to_string()),
        use_privacy_mode: Some(true),
        wot_sync_json: None,
        contact_card_json: None,
    };
    let mut s = state.service.lock().unwrap();
    let bundle_res = s
        .create_transfer_bundle(transfer_req, std_map, None, Some(password))
        .unwrap();
    let record = TransactionRecord {
        id: bundle_res.bundle_id.clone(),
        direction: "sent".to_string(),
        recipient_id: recipient_did.to_string(),
        sender_id: sender_did,
        timestamp: Utc::now().to_rfc3339(),
        summable_amounts: HashMap::new(),
        countable_items: HashMap::new(),
        involved_vouchers: vec![local_id.to_string()],
        involved_sources_details: None,
        bundle_data: Vec::new(),
        bundle_id: bundle_res.bundle_id.clone(),
        notes: None,
        sender_profile_name: Some("Alice".to_string()),
    };
    state
        .record_outgoing_transactions_with_batch(
            &mut s,
            vec![(record, bundle_res.bundle_bytes.clone())],
            Some(password),
        )
        .unwrap();
    (bundle_res.bundle_id, bundle_res.bundle_bytes)
}

async fn setup_test_sync(
    server: &MockSyncServer,
    drop_pub: &str,
    password: &str,
    state: &AppState,
) -> (String, crate::models::FrontendPublicSyncInfo) {
    let write_url = format!("{}/sync.php", server.base_url);
    let read_url = format!("{}/{}", server.base_url, server.data_dir);
    let info = probe_public_sync_impl(write_url.clone(), Some(password.to_string()), state)
        .await
        .unwrap();
    server
        .members
        .lock()
        .unwrap()
        .push(info.access_pubkey.clone());
    let saved = save_public_sync_impl(
        write_url.clone(),
        read_url,
        drop_pub.to_string(),
        None,
        true,
        Some(password.to_string()),
        state,
    )
    .await
    .unwrap();
    (write_url, saved)
}

async fn setup_two_party_sync_fixture(
    base_dir: &std::path::Path,
    password: &str,
    trust_level: Option<u8>,
    register_cross_members: bool,
) -> TwoPartySyncFixture {
    let alice_state = create_test_state(setup_test_app(
        &base_dir.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_state = create_test_state(setup_test_app(
        &base_dir.join("bob"),
        "Bob",
        BOB_PHRASE,
        password,
    ));
    let alice_did = wallet_user_id(&alice_state);
    let bob_did = wallet_user_id(&bob_state);

    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;
    let (url_a, alice_drop) = setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
    let (url_b, bob_drop) = setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    if register_cross_members {
        let alice_probe_b =
            probe_public_sync_impl(url_b.clone(), Some(password.to_string()), &alice_state)
                .await
                .unwrap();
        server_b
            .members
            .lock()
            .unwrap()
            .push(alice_probe_b.access_pubkey);
        let bob_probe_a =
            probe_public_sync_impl(url_a.clone(), Some(password.to_string()), &bob_state)
                .await
                .unwrap();
        server_a
            .members
            .lock()
            .unwrap()
            .push(bob_probe_a.access_pubkey);
    }

    let alice_syncs = vec![make_sync_desc(
        &url_a,
        &server_a,
        DROP_PUB_A,
        &alice_drop.access_pubkey,
        Some("pc"),
    )];
    let bob_syncs = vec![make_sync_desc(
        &url_b,
        &server_b,
        DROP_PUB_B,
        &bob_drop.access_pubkey,
        Some("pc"),
    )];

    save_test_contact(
        &alice_state,
        &bob_did,
        "Bob",
        trust_level,
        bob_syncs.clone(),
        password,
    );
    save_test_contact(
        &bob_state,
        &alice_did,
        "Alice",
        trust_level,
        alice_syncs.clone(),
        password,
    );

    TwoPartySyncFixture {
        alice_state,
        bob_state,
        alice_did,
        bob_did,
        server_a,
        server_b,
        bob_syncs,
    }
}

async fn publish_seller_profile(
    state: &AppState,
    name: &str,
    skills: Vec<humoco_market::Skill>,
    password: &str,
) {
    save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some(name.to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills,
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        },
        Some(password.to_string()),
        state,
    )
    .unwrap();
    set_card_published_impl(true, Some(password.to_string()), state)
        .await
        .unwrap();
}

fn create_test_offer_ad(
    state: &AppState,
    title: &str,
    text: Option<&str>,
    pricing: Option<&str>,
    password: &str,
) -> String {
    let prof = upsert_ad_impl(
        FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: title.to_string(),
            text: text.map(str::to_string),
            pricing: pricing.map(str::to_string),
            duration_days: 30,
            allow_inquiries: true,
            is_remote: false,
            image_webp_data_uri: None,
            event_at: None,
            event_all_day: false,
            published: Some(true),
        },
        Some(password.to_string()),
        state,
    )
    .unwrap();
    prof.ads[0].id.clone()
}

fn dummy_known_offender_proof(
    proof_id: &str,
    offender_id: &str,
    reporter_id: &str,
) -> human_money_core::models::conflict::ProofStoreEntry {
    use human_money_core::models::conflict::{ConflictRole, ProofOfDoubleSpend, ProofStoreEntry};
    ProofStoreEntry {
        proof: ProofOfDoubleSpend {
            proof_id: proof_id.to_string(),
            offender_id: offender_id.to_string(),
            suspected_identity: None,
            fork_point_prev_hash: "00".repeat(32),
            conflicting_transactions: Vec::new(),
            fingerprints: Vec::new(),
            victim_witnesses: Vec::new(),
            deletable_at: "2099-01-01T00:00:00Z".to_string(),
            reporter_id: reporter_id.to_string(),
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
    }
}

fn insert_packed_post_slot(
    state: &AppState,
    server: &MockSyncServer,
    did: &str,
    boxes: &[humoco_market::PostBoxEntry],
    password: &str,
) {
    let prefix = human_money_core::crypto::get_prefix_from_user_id(did).map(str::to_string);
    let (slot_key, blob) = {
        let mut s = state.service.lock().unwrap();
        let ctx = crate::services::common::SyncIdentityContext::from_service(&s, Some(password))
            .unwrap();
        let pol = state
            .get_or_load_security_policy(&mut s, Some(password))
            .unwrap();
        let desc = humoco_market::DropDescriptor {
            write_url: pol.public_syncs[0].write_url.clone(),
            read_url: pol.public_syncs[0].read_url.clone(),
            slot_id: Some(pol.public_syncs[0].access_pubkey.clone()),
            drop_pub: pol.public_syncs[0].drop_pub.clone(),
        };
        let spk = crate::services::common::DropChannelSigner {
            is_public: true,
            master_seed: &ctx.master_seed,
            my_ed_sk: &ctx.my_ed_sk,
            drop_desc: &desc,
        }
        .derive_slot_pubkey()
        .unwrap();
        let suffix = prefix.map(|p| format!("_{}", p)).unwrap_or_default();
        let packed = humoco_market::pack_post_slot(&ctx.master_seed, boxes, DROP_POST_MAX_BYTES)
            .unwrap();
        (format!("{}{}.post", spk, suffix), packed.blob)
    };
    server.slots.lock().unwrap().insert(slot_key, blob);
}

fn build_test_anchor_manifest_and_probe_cache() -> (
    human_money_core::humoco_proto::AnchorManifest,
    [u8; 32],
    std::collections::BTreeMap<String, crate::sync_client::FrontendLockServerProbeResult>,
) {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::{
        encode_quarter, AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry,
    };

    let s1_sk = SigningKey::from_bytes(&[61u8; 32]);
    let s2_sk = SigningKey::from_bytes(&[62u8; 32]);
    let creator_mig_sk = SigningKey::from_bytes(&[63u8; 32]);
    let guar_mig_sk = SigningKey::from_bytes(&[64u8; 32]);
    let manifest = AnchorManifest {
        revision: 1,
        threshold: 2,
        servers: vec![
            ServerEntry {
                write_url: "https://post-lock-1.example/sync.php".into(),
                read_url: "https://post-lock-1.example/d".into(),
                admin_pub: s1_sk.verifying_key().to_bytes(),
                lock_pub: s1_sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            },
            ServerEntry {
                write_url: "https://post-lock-2.example/sync.php".into(),
                read_url: "https://post-lock-2.example/d".into(),
                admin_pub: s2_sk.verifying_key().to_bytes(),
                lock_pub: s2_sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Guarantor(0),
            },
        ],
        successors: vec![],
        authority: AnchorAuthority {
            creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
            guarantor_mig_pubs: vec![guar_mig_sk.verifying_key().to_bytes()],
        },
        migration_chain: vec![],
        issued_at_ms: 1_760_000_000_000,
        retain_until_quarter: encode_quarter(2035, 4).unwrap(),
    };
    let anchor_id = manifest.rev1_anchor_id().unwrap();
    let mut cache = std::collections::BTreeMap::new();
    for (srv, sk) in manifest.servers.iter().zip([&s1_sk, &s2_sk]) {
        let admin_hex = hex::encode(srv.admin_pub);
        let lock_hex = hex::encode(srv.lock_pub);
        let msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex,
            &lock_hex,
            &srv.write_url,
            &srv.read_url,
            1_760_000_000,
        );
        let sig = hex::encode(sk.sign(msg.as_bytes()).to_bytes());
        cache.insert(
            lock_hex.clone(),
            crate::sync_client::FrontendLockServerProbeResult {
                write_url: srv.write_url.clone(),
                read_url: srv.read_url.clone(),
                drop_pub: admin_hex.clone(),
                admin_pub: admin_hex.clone(),
                lock_pub: lock_hex.clone(),
                delegation_valid: true,
                waf_probe_passed: true,
                selftest_passed: true,
                key_leak_safe: true,
                passed: true,
                latency_ms: 5,
                max_retention_quarter: manifest.retain_until_quarter,
                pow_base_bits: human_money_core::humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS,
                allow_pow_tickets: true,
                ticket_pow_bits: 16,
                inode_warning: false,
                used_inodes: 0,
                max_inodes: 50_000,
                probed_at_secs: 1_760_000_000,
                delegation: Some(crate::sync_client::SyncLockDelegationDto {
                    admin_pub: admin_hex,
                    lock_pub: lock_hex,
                    write_url: srv.write_url.clone(),
                    read_url: srv.read_url.clone(),
                    issued_at: 1_760_000_000,
                    sig,
                }),
                error: None,
            },
        );
    }
    (manifest, anchor_id, cache)
}

#[tokio::test]
async fn test_dual_delivery_payment_and_receipt_idempotency_sst_v4() {
    let dir_guard = TempDirGuard(temp_test_dir("dual_delivery_test"));
    let password = "TestPassword123!";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(2), true).await;

    let (_, std_map) = load_freetaler_std_map();
    let local_id = create_freetaler_voucher(&fix.alice_state, &fix.alice_did, "100", None, password);
    let (bundle_id, bundle_bytes) = create_and_record_transfer_bundle(
        &fix.alice_state,
        &fix.bob_did,
        &local_id,
        "20",
        &std_map,
        password,
    );

    // 1. Depositing twice reuses identical msg_id and box without rebuilding bundle.
    let dep1 = deposit_transfer_to_post_impl(
        bundle_id.clone(),
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert_eq!(dep1.deposited_to, 2, "errors: {:?}", dep1.errors);
    assert_eq!(dep1.recipient_deposited_to, 1);
    assert!(dep1.errors.is_empty(), "errors: {:?}", dep1.errors);
    let dep2 = deposit_transfer_to_post_impl(
        bundle_id.clone(),
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert_eq!(dep1.msg_id, dep2.msg_id);

    let post_state_alice = load_post_state(&fix.alice_state, Some(password)).unwrap();
    assert_eq!(post_state_alice.outbox_entries.len(), 1);
    assert_eq!(
        post_state_alice.outbox_entries[0].bundle_id.as_deref(),
        Some(bundle_id.as_str())
    );

    // 2. Bob syncs and receives payment + queues receipt.
    let bob_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.imported_payments, 1);

    // 3. Re-importing same bundle offline is idempotently rejected with exact BundleAlreadyProcessed.
    let offline_import_attempt = {
        let mut s = fix.bob_state.service.lock().unwrap();
        s.receive_bundle(&bundle_bytes, &std_map, None, Some(password), false)
    };
    assert!(
        matches!(
            offline_import_attempt,
            Err(human_money_core::Error::BundleAlreadyProcessed {
                bundle_id: ref actual_id
            }) if actual_id == &bundle_id
        ),
        "Duplicate import must return BundleAlreadyProcessed, got {:?}",
        offline_import_attempt
    );

    // 4. Alice syncs receipt and prunes acknowledged outbox entry.
    let alice_sync = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert_eq!(alice_sync.imported_receipts, 1);
    assert_eq!(alice_sync.pruned_boxes, 1);
    let post_state_alice_final = load_post_state(&fix.alice_state, Some(password)).unwrap();
    assert!(post_state_alice_final.outbox_entries.is_empty());
}

#[tokio::test]
async fn test_asymmetry_alice_has_sync_bob_has_none() {
    let dir_guard = TempDirGuard(temp_test_dir("asymmetry_test"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_state = create_test_state(setup_test_app(
        &dir_guard.0.join("bob"),
        "Bob",
        BOB_PHRASE,
        password,
    ));
    let alice_did = wallet_user_id(&alice_state);
    let bob_did = wallet_user_id(&bob_state);

    // Server for Alice only; Bob has NO drop server.
    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
    let alice_sync = vec![make_sync_desc(
        &server_a_url,
        &server_a,
        DROP_PUB_A,
        &alice_drop.access_pubkey,
        Some("pc"),
    )];
    save_test_contact(&bob_state, &alice_did, "Alice", Some(2), alice_sync, password);

    let (_, std_map) = load_freetaler_std_map();
    let local_id = create_freetaler_voucher(&alice_state, &alice_did, "50", None, password);
    let (bundle_id, _) = create_and_record_transfer_bundle(
        &alice_state,
        &bob_did,
        &local_id,
        "10",
        &std_map,
        password,
    );

    deposit_transfer_to_post_impl(bundle_id, Some(password.to_string()), &alice_state)
        .await
        .unwrap();

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.imported_payments, 1);
    assert_eq!(bob_sync.uploaded_to, 0, "Bob has no write drops, 0 uploads");
}

#[tokio::test]
async fn test_asymmetry_bob_has_sync_alice_has_none() {
    let dir_guard = TempDirGuard(temp_test_dir("asymmetry_bob_has_drop"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_state = create_test_state(setup_test_app(
        &dir_guard.0.join("bob"),
        "Bob",
        BOB_PHRASE,
        password,
    ));
    let alice_did = wallet_user_id(&alice_state);
    let bob_did = wallet_user_id(&bob_state);

    // Server for Bob only; Alice registers as member without saving server in her settings.
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;
    let (server_b_url, bob_drop) =
        setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;
    let alice_probe = probe_public_sync_impl(
        server_b_url.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();
    server_b
        .members
        .lock()
        .unwrap()
        .push(alice_probe.access_pubkey.clone());

    {
        let mut s = alice_state.service.lock().unwrap();
        let policy = alice_state
            .load_security_policy(&mut s, Some(password))
            .unwrap();
        assert!(policy.market_selected_syncs.is_empty());
        assert!(policy.public_syncs.is_empty());
    }

    let bob_desc = vec![make_sync_desc(
        &server_b_url,
        &server_b,
        DROP_PUB_B,
        &bob_drop.access_pubkey,
        Some("pc"),
    )];
    let alice_desc = vec![make_sync_desc(
        &server_b_url,
        &server_b,
        DROP_PUB_B,
        &alice_probe.access_pubkey,
        Some("pc"),
    )];
    save_test_contact(&alice_state, &bob_did, "Bob", Some(2), bob_desc, password);
    save_test_contact(&bob_state, &alice_did, "Alice", Some(2), alice_desc, password);

    let (_, std_map) = load_freetaler_std_map();
    let local_id = create_freetaler_voucher(&alice_state, &alice_did, "50", None, password);
    let (bundle_id, _) = create_and_record_transfer_bundle(
        &alice_state,
        &bob_did,
        &local_id,
        "10",
        &std_map,
        password,
    );

    let deposit_res = deposit_transfer_to_post_impl(bundle_id, Some(password.to_string()), &alice_state)
        .await
        .unwrap();
    assert_eq!(
        deposit_res.deposited_to, 1,
        "Must upload to recipient's drop even when sender has no drops"
    );

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.imported_payments, 1);
}

#[tokio::test]
async fn test_multi_device_if_match_412_merge() {
    let dir_guard = TempDirGuard(temp_test_dir("multi_device_merge"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;

    let master_seed = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet_and_identity(|_, id| id.signing_key.to_bytes())
        .unwrap();
    let dummy_slot =
        humoco_market::encode_post_slot(&master_seed, &[], DROP_POST_MAX_BYTES).unwrap();
    let slot_key = format!("{}_pc.post", alice_drop.access_pubkey);
    server_a
        .slots
        .lock()
        .unwrap()
        .insert(slot_key.clone(), dummy_slot);
    server_a
        .etags
        .lock()
        .unwrap()
        .insert(slot_key, "\"etag_device_2\"".to_string());

    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();
    post_state
        .slot_etags_by_write_url
        .insert(server_a_url, "\"etag_stale\"".to_string());
    save_post_state(&alice_state, &post_state, Some(password)).unwrap();

    let session = alice_state.begin_session_op().unwrap();
    let outcome = flush_post_slots_impl(
        &alice_state,
        &session,
        &mut post_state,
        None,
        &[],
        Some(password),
    )
    .await;
    assert_eq!(
        outcome.deposited_to, 1,
        "Must successfully merge and upload on 412: {:?}",
        outcome.errors
    );
    assert!(outcome.errors.is_empty());
}

#[tokio::test]
async fn test_async_co_signing_voucher_for_signing_to_detached_signature() {
    let dir_guard = TempDirGuard(temp_test_dir("co_signing_test"));
    let password = "TestPassword123!";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(2), true).await;

    let (_, std_map) = load_freetaler_std_map();
    let local_id = create_freetaler_voucher(&fix.alice_state, &fix.alice_did, "100", None, password);

    let signing_req_bytes = {
        let s = fix.alice_state.service.lock().unwrap();
        let config = ContainerConfig::TargetDid(fix.bob_did.clone(), PrivacyMode::HashedRouting);
        s.create_signing_request_bundle(&local_id, config, None, Some(password))
            .unwrap()
    };

    send_signing_request_via_post_impl(
        fix.bob_did.clone(),
        signing_req_bytes,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.new_sign_requests, 1);

    let bob_post = load_post_state(&fix.bob_state, Some(password)).unwrap();
    let sign_req = &bob_post.pending_sign_requests[0];
    let sig_response_bytes = {
        let mut bob_s = fix.bob_state.service.lock().unwrap();
        let req_bundle = bob_s
            .open_voucher_signing_request(&sign_req.container_bytes, None)
            .unwrap();
        let config =
            ContainerConfig::TargetDid(fix.alice_did.clone(), PrivacyMode::HashedRouting);
        bob_s
            .create_detached_signature_response_bundle(
                &req_bundle.voucher,
                "guarantor",
                false,
                config,
                Some(password),
            )
            .unwrap()
    };

    reply_detached_signature_via_post_impl(
        Some(sign_req.msg_id.clone()),
        fix.alice_did.clone(),
        sig_response_bytes,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();

    let alice_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert_eq!(alice_sync.imported_signatures, 1);

    let updated_voucher = {
        let s = fix.alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.voucher_store.vouchers.get(&local_id).unwrap().clone())
            .unwrap()
    };
    assert!(updated_voucher
        .voucher
        .signatures
        .iter()
        .any(|s| s.role == "guarantor"));
}

#[tokio::test]
async fn test_async_wot_handshake_auto_accept_and_mutual_sync() {
    let dir_guard = TempDirGuard(temp_test_dir("wot_handshake_test"));
    let password = "TestPassword123!";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(1), true).await;

    send_wot_handshake_via_post_impl(
        fix.bob_did.clone(),
        1,
        true,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.new_wot_handshakes, 0);
    assert_eq!(bob_sync.imported_wot_lists, 1);
    {
        let mut s_bob = fix.bob_state.service.lock().unwrap();
        let reg = fix
            .bob_state
            .get_or_load_wot_register(&mut s_bob, Some(password))
            .unwrap();
        assert!(
            reg.raw_list_store.contains_key(&fix.alice_did)
                || reg
                    .raw_list_store
                    .contains_key(&crate::state::canonicalize_did(&fix.alice_did).unwrap())
        );
    }

    let alice_sync = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert_eq!(alice_sync.new_wot_handshakes, 0);
    assert_eq!(alice_sync.imported_wot_lists, 1);
    {
        let mut s_alice = fix.alice_state.service.lock().unwrap();
        let reg = fix
            .alice_state
            .get_or_load_wot_register(&mut s_alice, Some(password))
            .unwrap();
        assert!(
            reg.raw_list_store.contains_key(&fix.bob_did)
                || reg
                    .raw_list_store
                    .contains_key(&crate::state::canonicalize_did(&fix.bob_did).unwrap())
        );
    }
}

#[tokio::test]
async fn test_1_step_marketplace_inquiry_confirm_chat_and_dismiss() {
    let dir_guard = TempDirGuard(temp_test_dir("market_inquiry_cycle_test"));
    let password = "TestPassword123!";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(1), true).await;

    publish_seller_profile(
        &fix.bob_state,
        "Bob",
        vec![humoco_market::Skill {
            title: "Honey".to_string(),
            text: None,
            pricing: None,
            is_remote: false,
        }],
        password,
    )
    .await;
    let ad_id = create_test_offer_ad(
        &fix.bob_state,
        "Forest Honey",
        Some("500g Jar"),
        Some("10 Minuto"),
        password,
    );

    let inq_item = send_market_inquiry_via_post_impl(
        fix.bob_did.clone(),
        FrontendInquiryTarget::Ad {
            ad_id,
            ad_title: "Forest Honey".to_string(),
        },
        "Do you have 2 jars available?".to_string(),
        fix.bob_syncs.clone(),
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();

    let bob_sync_1 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &fix.bob_state)
        .await
        .unwrap();
    assert_eq!(bob_sync_1.new_inquiries, 1);
    let bob_post_1 = load_post_state(&fix.bob_state, Some(password)).unwrap();
    let seller_inq = &bob_post_1.inquiries[0];
    assert_eq!(seller_inq.role, "seller");
    assert!(seller_inq.unread);

    let pay_uri = format!("humoco://pay?to={}", fix.bob_did);
    let rep_res = send_market_inquiry_message_via_post_impl(
        seller_inq.inquiry_id.clone(),
        Some("Yes, 2 jars reserved!".to_string()),
        Some("confirm".to_string()),
        Some(pay_uri.clone()),
        Some("20 Minuto".to_string()),
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(rep_res.status, "confirmed");
    assert_eq!(rep_res.messages.len(), 2);

    let second_err = send_market_inquiry_message_via_post_impl(
        seller_inq.inquiry_id.clone(),
        None,
        Some("decline".to_string()),
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .expect_err("Second decision change must be rejected");
    assert_eq!(second_err.message, "Inquiry status has already been decided");

    let alice_sync = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert_eq!(alice_sync.new_inquiry_messages, 1);
    assert!(alice_sync.errors.is_empty(), "errors: {:?}", alice_sync.errors);

    let alice_post = load_post_state(&fix.alice_state, Some(password)).unwrap();
    assert_eq!(alice_post.inquiries[0].status, "confirmed");
    assert_eq!(alice_post.inquiries[0].messages.len(), 2);
    assert!(alice_post.inquiries[0].unread);
    assert_eq!(alice_post.inquiries[0].payment_uri.as_deref(), Some(pay_uri.as_str()));

    mark_inquiry_read_impl(inq_item.inquiry_id.clone(), Some(password.to_string()), &fix.alice_state).unwrap();
    assert!(!load_post_state(&fix.alice_state, Some(password)).unwrap().inquiries[0].unread);

    let follow_up = send_market_inquiry_message_via_post_impl(
        inq_item.inquiry_id.clone(),
        Some("Can I pick them up at 5pm?".to_string()),
        None,
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert_eq!(follow_up.messages.len(), 3);

    let bob_sync_2 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &fix.bob_state)
        .await
        .unwrap();
    assert_eq!(bob_sync_2.new_inquiry_messages, 1);
    assert_eq!(
        load_post_state(&fix.bob_state, Some(password)).unwrap().inquiries[0]
            .messages
            .len(),
        3
    );

    dismiss_pending_post_item_impl(
        "inquiry".to_string(),
        inq_item.inquiry_id,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .unwrap();
    assert!(load_post_state(&fix.alice_state, Some(password))
        .unwrap()
        .inquiries
        .is_empty());
}

#[tokio::test]
async fn test_own_subaccount_post_compartment_and_sibling_isolation() {
    let dir_guard = TempDirGuard(temp_test_dir("subaccount_post_isolation"));
    let password = "TestPassword123!";
    let create_sub = |dir: &std::path::Path, name: &str, prefix: Option<&str>| {
        let mut svc = human_money_core::app_service::AppService::new(dir).unwrap();
        svc.create_profile(
            name,
            ALICE_PHRASE,
            None,
            prefix,
            password,
            human_money_core::MnemonicLanguage::English,
            format!("inst-{}", name),
            true,
        )
        .unwrap();
        svc.unlock_session(password, 3600).unwrap();
        create_test_state(svc)
    };

    let root_state = create_sub(&dir_guard.0.join("root"), "Alice Root", None);
    let laden_state = create_sub(&dir_guard.0.join("laden"), "Alice Laden", Some("laden"));
    let privat_state = create_sub(&dir_guard.0.join("privat"), "Alice Privat", Some("privat"));
    let root_did = wallet_user_id(&root_state);
    let laden_did = wallet_user_id(&laden_state);
    assert_ne!(laden_did, wallet_user_id(&privat_state));

    let server = MockSyncServer::start("d-shared".to_string(), ADMIN_A.to_string()).await;
    let (url_r, drop_r) = setup_test_sync(&server, DROP_PUB_A, password, &root_state).await;
    let (server_url, laden_drop) = setup_test_sync(&server, DROP_PUB_A, password, &laden_state).await;
    let (url_p, drop_p) = setup_test_sync(&server, DROP_PUB_A, password, &privat_state).await;
    assert!(!url_r.is_empty() && !drop_r.access_pubkey.is_empty());
    assert!(!url_p.is_empty() && !drop_p.access_pubkey.is_empty());

    let shared_sync = vec![make_sync_desc(
        &server_url,
        &server,
        DROP_PUB_A,
        &laden_drop.access_pubkey,
        None,
    )];
    save_test_contact(&root_state, &laden_did, "Laden", None, shared_sync.clone(), password);
    save_test_contact(&laden_state, &root_did, "Root", None, shared_sync.clone(), password);
    save_test_contact(&privat_state, &root_did, "Root", None, shared_sync, password);

    let (_, std_map) = load_freetaler_std_map();
    let local_id = create_freetaler_voucher(&root_state, &root_did, "50", None, password);
    let (bundle_id, _) = create_and_record_transfer_bundle(
        &root_state,
        &laden_did,
        &local_id,
        "15",
        &std_map,
        password,
    );

    let dep = deposit_transfer_to_post_impl(bundle_id, Some(password.to_string()), &root_state)
        .await
        .unwrap();
    assert_eq!(dep.deposited_to, 1);

    // 1. Sibling subaccount ("privat") must stay error-free and ignore Laden's payment.
    let privat_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &privat_state,
    )
    .await
    .unwrap();
    assert_eq!(privat_sync.imported_payments, 0);
    assert!(privat_sync.errors.is_empty(), "errors: {:?}", privat_sync.errors);

    // 2. Target subaccount ("laden") imports payment and writes ACK into its prefixed compartment.
    let laden_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &laden_state,
    )
    .await
    .unwrap();
    assert_eq!(laden_sync.imported_payments, 1);
    assert!(laden_sync.errors.is_empty(), "errors: {:?}", laden_sync.errors);

    let laden_flush = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &laden_state,
    )
    .await
    .unwrap();
    assert_eq!(laden_flush.imported_payments, 0);
    assert!(laden_flush.errors.is_empty(), "errors: {:?}", laden_flush.errors);

    let expected_laden_slot = format!("{}_laden.post", laden_drop.access_pubkey);
    assert!(
        server.slots.lock().unwrap().contains_key(&expected_laden_slot),
        "Missing compartment slot '{}'",
        expected_laden_slot
    );
}

#[tokio::test]
async fn test_deposit_transfer_to_post_persists_outbox_when_sync_server_unreachable_and_sync_survives_outage(
) {
    let dir_guard = TempDirGuard(temp_test_dir("post_sync_outage_test"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_state = create_test_state(setup_test_app(
        &dir_guard.0.join("bob"),
        "Bob",
        BOB_PHRASE,
        password,
    ));
    let alice_did = wallet_user_id(&alice_state);
    let bob_did = wallet_user_id(&bob_state);

    let closed_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed_listener.local_addr().unwrap().port();
    drop(closed_listener);
    let dead_write_url = format!("http://127.0.0.1:{}/sync.php", closed_port);
    let dead_read_url = format!("http://127.0.0.1:{}/data", closed_port);

    save_test_contact(
        &alice_state,
        &bob_did,
        "Bob",
        Some(2),
        vec![FrontendSyncDescriptor {
            write_url: dead_write_url.clone(),
            read_url: dead_read_url.clone(),
            slot_id: Some(humoco_market::format_slot_id(ADMIN_B, Some("pc")).unwrap()),
            drop_pub: DROP_PUB_B.to_string(),
            status: "active".to_string(),
            ..Default::default()
        }],
        password,
    );
    {
        let mut s_bob = bob_state.service.lock().unwrap();
        bob_state
            .mutate_security_policy(&mut s_bob, Some(password), |policy| {
                policy.public_syncs.push(crate::settings::StoredPublicSync {
                    write_url: dead_write_url,
                    read_url: dead_read_url,
                    drop_pub: DROP_PUB_B.to_string(),
                    access_pubkey: ADMIN_B.to_string(),
                    enabled: true,
                    authorized_channels: vec!["post".to_string()],
                    ..Default::default()
                });
            })
            .unwrap();
    }

    let (_, std_map) = load_freetaler_std_map();
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        alice_state
            .save_ui_preferences(
                &mut s_alice,
                crate::settings::UiPreferences::default(),
                Some(password),
            )
            .unwrap();
        alice_state
            .save_internal_lifecycle(
                &mut s_alice,
                crate::settings::InternalLifecycleState::default(),
                Some(password),
            )
            .unwrap();
    }
    let local_id = create_freetaler_voucher(&alice_state, &alice_did, "100", None, password);

    let bundle_res = crate::services::transfer_service::create_transfer_bundle_impl(
        None,
        bob_did,
        vec![crate::models::FrontendSourceTransfer {
            local_instance_id: local_id,
            amount_to_send: "25".to_string(),
        }],
        Some("Sync outage test".to_string()),
        Some("Alice".to_string()),
        std_map.clone(),
        Some(false),
        Some(false),
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect("L1 transfer creation must succeed even when Sync and L2 are down");

    let dep = deposit_transfer_to_post_impl(
        bundle_res.bundle_id.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect("deposit_transfer_to_post_impl must degrade gracefully");
    assert_eq!(dep.deposited_to, 0);
    assert_eq!(dep.recipient_deposited_to, 0);
    assert!(!dep.errors.is_empty());

    let persisted = load_post_state(&alice_state, Some(password)).unwrap();
    assert_eq!(persisted.outbox_entries.len(), 1);
    assert_eq!(
        persisted.outbox_entries[0].bundle_id.as_deref(),
        Some(bundle_res.bundle_id.as_str())
    );

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &bob_state,
    )
    .await
    .expect("sync_post_channels_impl must not crash on dead sync server");
    assert_eq!(bob_sync.imported_payments, 0);
    assert!(!bob_sync.errors.is_empty());

    crate::services::transfer_service::receive_bundle_impl(
        None,
        bundle_res.bundle_data,
        std_map,
        Some(password.to_string()),
        false,
        Some(crate::models::ReceiveCommitMode::ForceTrustOffline),
        None,
        &bob_state,
    )
    .await
    .expect("Manual offline receive_bundle_impl must succeed while sync is down");
}

#[tokio::test]
async fn test_unreachable_recipient_returns_error_and_locked_dispatch_defers_msg_id() {
    let dir_guard = TempDirGuard(temp_test_dir("post_recipient_unreachable_and_locked_dispatch"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_svc = setup_test_app(&dir_guard.0.join("bob"), "Bob", BOB_PHRASE, password);
    let (bob_did, my_identity) = bob_svc
        .with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.clone()))
        .unwrap();

    let server = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let (write_url, saved) = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;
    assert!(write_url.ends_with("/sync.php"));
    assert!(!saved.access_pubkey.is_empty());

    let inq_err = send_market_inquiry_via_post_impl(
        bob_did.clone(),
        FrontendInquiryTarget::Person { skill_title: None },
        "Hallo Bob".to_string(),
        vec![],
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect_err("Sending market inquiry with zero reachable recipient syncs must return Err");
    assert_eq!(inq_err.code, "error.post.recipientUnreachable");

    let locked_dir = dir_guard.0.join("locked");
    std::fs::create_dir_all(&locked_dir).unwrap();
    let locked_state = create_test_state(
        human_money_core::app_service::AppService::new(&locked_dir).unwrap(),
    );
    let my_ed_sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let envelope = PostEnvelope::new(
        "msg-locked-retry-1",
        &bob_did,
        1_760_000_000,
        vec![],
        vec![1, 2, 3],
    );
    let tag16 = [0u8; 16];
    let locked_session = crate::session::SessionToken {
        generation: locked_state.current_session_generation(),
        owner_did: String::new(),
    };
    let ctx = super::handlers::PostEnvelopeContext {
        state: &locked_state,
        session: &locked_session,
        app: None,
        extra_standards: None,
        password: None,
        my_identity: &my_identity,
        my_ed_sk: &my_ed_sk,
        own_reply_syncs: &[],
        sender_did: &bob_did,
        envelope: &envelope,
        tag16: &tag16,
        decrypted_bytes: &[],
        now_secs: 1_760_000_000,
    };
    let mut post_state = StoredPostState::default();
    let mut baseline = post_state.clone();
    let mut stats = super::handlers::PostDispatchStats::default();
    let outcome = super::handlers::dispatch_post_payload(
        &ctx,
        &PayloadType::DetachedSignature,
        &mut post_state,
        &mut baseline,
        &mut stats,
    )
    .await;
    assert_eq!(outcome, super::handlers::PostDispatchOutcome::RetryLater);
    assert!(!stats.errors.is_empty());
}

#[test]
fn test_merge_and_save_post_state_preserves_concurrent_outbox() {
    let dir_guard = TempDirGuard(temp_test_dir("post_concurrency_merge"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_pk = human_money_core::crypto::generate_ed25519_keypair_for_tests(None).0;
    let bob_did = human_money_core::crypto::create_user_id(&bob_pk, None).unwrap();

    let now_ts = chrono::Utc::now().timestamp().max(0) as u64;
    let baseline = load_post_state(&alice_state, Some(password)).unwrap();
    let mut stale_poll_state = baseline.clone();
    stale_poll_state
        .processed_msg_ids
        .insert("polled-msg-1".to_string(), now_ts);

    {
        let mut concurrent_state = load_post_state(&alice_state, Some(password)).unwrap();
        concurrent_state
            .processed_msg_ids
            .insert("concurrent-outbox-1".to_string(), now_ts);
        concurrent_state
            .outbox_entries
            .push(crate::state::PostOutboxEntry {
                msg_id: "concurrent-outbox-1".to_string(),
                recipient_did: bob_did,
                tag16_hex: "09090909090909090909090909090909".to_string(),
                bundle_id: None,
                message_class: crate::state::OutboxMessageClass::Control,
                entry: humoco_market::PostBoxEntry {
                    msg_nonce: [8u8; 24],
                    tag16: [9u8; 16],
                    ciphertext: vec![1, 2, 3, 4],
                },
                created_at: 1_760_000_000,
            });
        save_post_state(&alice_state, &concurrent_state, Some(password)).unwrap();
    }

    let session = alice_state.begin_session_op().unwrap();
    merge_and_save_post_state(
        &alice_state,
        &session,
        &baseline,
        &mut stale_poll_state,
        Some(password),
    )
    .unwrap();
    let merged = load_post_state(&alice_state, Some(password)).unwrap();
    assert!(merged.processed_msg_ids.contains_key("concurrent-outbox-1"));
    assert!(merged.processed_msg_ids.contains_key("polled-msg-1"));
    assert_eq!(merged.outbox_entries.len(), 1);
    assert_eq!(merged.outbox_entries[0].msg_id, "concurrent-outbox-1");
}

#[tokio::test]
async fn test_sync_post_channels_auto_imports_incoming_payment() {
    let dir_guard = TempDirGuard(temp_test_dir("post_auto_import_payment"));
    let password = "TestPassword123!";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(2), true).await;

    let (_, std_map) = load_freetaler_std_map();
    let local_id = create_freetaler_voucher(&fix.alice_state, &fix.alice_did, "50", None, password);

    let bundle_res = crate::services::transfer_service::create_transfer_bundle_impl(
        None,
        fix.bob_did.clone(),
        vec![crate::models::FrontendSourceTransfer {
            local_instance_id: local_id,
            amount_to_send: "20".to_string(),
        }],
        Some("Ladeneinkauf".to_string()),
        Some("Alice".to_string()),
        std_map.clone(),
        Some(false),
        Some(false),
        None,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();

    let dep_res = deposit_transfer_to_post_impl(
        bundle_res.bundle_id,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();
    assert!(
        dep_res.recipient_deposited_to >= 1 || dep_res.deposited_to >= 1,
        "Deposit failed: {:?}",
        dep_res
    );

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.imported_payments, 1, "bob_sync: {:?}", bob_sync);

    let s_bob = fix.bob_state.service.lock().unwrap();
    let bob_vouchers = s_bob
        .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
        .unwrap();
    assert_eq!(bob_vouchers.len(), 1);
}

#[tokio::test]
async fn test_reply_detached_signature_via_post_requires_full_access() {
    let dir_guard = TempDirGuard(temp_test_dir("post_detached_sig_full_only"));
    let pw = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        pw,
    ));
    let bob_state = create_test_state(setup_test_app(
        &dir_guard.0.join("bob"),
        "Bob",
        BOB_PHRASE,
        pw,
    ));
    let alice_did = wallet_user_id(&alice_state);

    {
        let mut s = bob_state.service.lock().unwrap();
        let folder = s.list_profiles().unwrap()[0].folder_name.clone();
        s.enable_secondary_auth(pw, "123456", human_money_core::SecondaryScope::DailyPay)
            .unwrap();
        s.logout();
        let mode = s
            .login_with_secondary(&folder, "123456", false, "device-drops-test".into())
            .unwrap();
        assert_eq!(mode, human_money_core::AccessMode::DailyPay);
    }

    let err_daily = reply_detached_signature_via_post_impl(
        None,
        alice_did,
        b"dummy-sig-bytes".to_vec(),
        Some("123456".to_string()),
        &bob_state,
    )
    .await
    .unwrap_err();
    assert_eq!(err_daily.code, "error.auth.insufficientAccessMode");
}

#[tokio::test]
async fn test_outbox_windowed_packing_defers_overflow_past_max_bytes_and_drains_next_flush() {
    let dir_guard = TempDirGuard(temp_test_dir("post_outbox_windowed_drain"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_svc = setup_test_app(&dir_guard.0.join("bob"), "Bob", BOB_PHRASE, password);
    let (bob_did, bob_pk) = bob_svc
        .with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.public_key))
        .unwrap();

    let server = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (write_url, saved) = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;
    assert!(write_url.ends_with("/sync.php") && !saved.access_pubkey.is_empty());

    let signing_ctx = load_post_signing_context(&alice_state, Some(password)).unwrap();
    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();

    for (msg_id, fill, ts) in [
        ("msg-large-1", vec![1u8; 40_000], 1_700_000_010u64),
        ("msg-large-2", vec![2u8; 40_000], 1_700_000_020u64),
        ("msg-small-3", vec![3u8; 120], 1_700_000_030u64),
    ] {
        enqueue_post_envelope(
            &mut post_state,
            &signing_ctx.identity.user_id,
            &signing_ctx.sync_ctx.my_ed_sk,
            &bob_did,
            &bob_pk,
            msg_id.to_string(),
            None,
            crate::state::OutboxMessageClass::Control,
            &signing_ctx.own_reply_syncs,
            fill,
            ts,
        )
        .unwrap();
    }

    let tag1 = post_state.outbox_entries[0].entry.tag16;
    let tag2 = post_state.outbox_entries[1].entry.tag16;
    let tag3 = post_state.outbox_entries[2].entry.tag16;
    let session = alice_state.begin_session_op().unwrap();

    let outcome1 = flush_post_slots_impl(
        &alice_state,
        &session,
        &mut post_state,
        None,
        &[],
        Some(password),
    )
    .await;
    assert_eq!(outcome1.deposited_to, 1, "errors: {:?}", outcome1.errors);
    assert!(outcome1.errors.is_empty());
    assert_eq!(outcome1.deferred_count, 1);
    assert!(outcome1.uploaded_counts_by_tag.contains_key(&tag1));
    assert!(outcome1.uploaded_counts_by_tag.contains_key(&tag3));
    assert!(outcome1.deferred_tags.contains(&tag2));
    {
        let slots = server.slots.lock().unwrap();
        let stored_blob = slots.values().next().expect("slot uploaded");
        assert!(stored_blob.len() <= DROP_POST_MAX_BYTES);
        let tags: std::collections::HashSet<[u8; 16]> =
            humoco_market::decode_post_slot(stored_blob)
                .unwrap()
                .into_iter()
                .map(|e| e.tag16)
                .collect();
        assert!(tags.contains(&tag1) && tags.contains(&tag3) && !tags.contains(&tag2));
    }

    post_state
        .outbox_entries
        .retain(|e| e.msg_id != "msg-large-1");
    let outcome2 = flush_post_slots_impl(
        &alice_state,
        &session,
        &mut post_state,
        None,
        &[],
        Some(password),
    )
    .await;
    assert_eq!(outcome2.deposited_to, 1, "errors: {:?}", outcome2.errors);
    assert!(outcome2.errors.is_empty());
    assert_eq!(outcome2.deferred_count, 0);
    assert!(outcome2.uploaded_counts_by_tag.contains_key(&tag2));
    assert!(outcome2.uploaded_counts_by_tag.contains_key(&tag3));
    let slots = server.slots.lock().unwrap();
    let stored_blob = slots.values().next().expect("slot uploaded");
    assert!(stored_blob.len() <= DROP_POST_MAX_BYTES);
    let tags2: std::collections::HashSet<[u8; 16]> = humoco_market::decode_post_slot(stored_blob)
        .unwrap()
        .into_iter()
        .map(|e| e.tag16)
        .collect();
    assert!(tags2.contains(&tag2) && tags2.contains(&tag3));
}

#[tokio::test]
async fn test_sync_post_channels_respects_wifi_only_skip() {
    let dir_guard = TempDirGuard(temp_test_dir("sync_post_wifi_only_skip"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));

    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .mutate_ui_preferences(&mut s, Some(password), |prefs| {
                prefs.market_poll_wifi_only = true;
            })
            .unwrap();
    }

    let metered_res = sync_post_channels_impl(
        None,
        Some(false),
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();
    assert!(metered_res.skipped_wifi_only);
    assert_eq!(metered_res.polled_syncs, 0);
    assert!(metered_res.errors.is_empty());

    let manual_res = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();
    assert!(!manual_res.skipped_wifi_only);
}

#[tokio::test]
async fn test_post_prefix_routing_reply_drops_and_ack_isolation() {
    let dir_guard = TempDirGuard(temp_test_dir("post_prefix_routing"));
    let password = "TestPassword123!";
    let alice_laden_state = create_test_state(setup_test_app(&dir_guard.0.join("alice_laden"), "Alice Laden", ALICE_PHRASE, password));
    let mut alice_privat_svc = human_money_core::app_service::AppService::new(&dir_guard.0.join("alice_privat")).unwrap();
    alice_privat_svc
        .create_profile("Alice Privat", ALICE_PHRASE, None, Some("privat"), password, human_money_core::MnemonicLanguage::English, "device-privat".into(), true)
        .unwrap();
    let alice_privat_state = create_test_state(alice_privat_svc);
    let bob_state = create_test_state(setup_test_app(&dir_guard.0.join("bob"), "Bob", BOB_PHRASE, password));
    let alice_laden_did = wallet_user_id(&alice_laden_state);
    let alice_privat_did = wallet_user_id(&alice_privat_state);
    let bob_did = wallet_user_id(&bob_state);
    assert_ne!(alice_laden_did, alice_privat_did);

    let server_a1 = MockSyncServer::start("d-a1".to_string(), ADMIN_A.to_string()).await;
    let server_a2 = MockSyncServer::start("d-a2".to_string(), ADMIN_A.to_string()).await;
    let server_a3 = MockSyncServer::start("d-a3".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;
    let (a1_w, a1_d) = setup_test_sync(&server_a1, DROP_PUB_A, password, &alice_laden_state).await;
    let (a2_w, a2_d) = setup_test_sync(&server_a2, DROP_PUB_A, password, &alice_privat_state).await;
    let (a3_w, a3_d) = setup_test_sync(&server_a3, DROP_PUB_A, password, &alice_privat_state).await;
    assert!(!a3_d.access_pubkey.is_empty());
    let (b_w, b_d) = setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    let alice_priv_b = probe_public_sync_impl(b_w.clone(), Some(password.to_string()), &alice_privat_state).await.unwrap();
    server_b.members.lock().unwrap().push(alice_priv_b.access_pubkey);
    let mut bob_a_keys = Vec::new();
    for (srv, w) in [(&server_a1, &a1_w), (&server_a2, &a2_w), (&server_a3, &a3_w)] {
        let p = probe_public_sync_impl(w.clone(), Some(password.to_string()), &bob_state).await.unwrap();
        srv.members.lock().unwrap().push(p.access_pubkey.clone());
        bob_a_keys.push(p.access_pubkey);
    }

    let a1_desc = make_sync_desc(&a1_w, &server_a1, DROP_PUB_A, &a1_d.access_pubkey, Some("pc"));
    let a2_desc = make_sync_desc(&a2_w, &server_a2, DROP_PUB_A, &a2_d.access_pubkey, Some("privat"));
    let b_desc = make_sync_desc(&b_w, &server_b, DROP_PUB_B, &b_d.access_pubkey, Some("pc"));
    save_test_contact(&bob_state, &alice_laden_did, "Alice Laden", Some(2), vec![a1_desc], password);
    save_test_contact(&bob_state, &alice_privat_did, "Alice Privat", Some(2), vec![a2_desc], password);
    save_test_contact(&alice_privat_state, &bob_did, "Bob", Some(2), vec![b_desc.clone()], password);
    publish_seller_profile(&bob_state, "Bob", vec![], password).await;

    let item = send_market_inquiry_via_post_impl(
        bob_did,
        FrontendInquiryTarget::Person { skill_title: None },
        "Hallo vom Privatkonto".to_string(),
        vec![b_desc],
        Some(password.to_string()),
        &alice_privat_state,
    )
    .await
    .unwrap();
    assert!(!item.inquiry_id.is_empty());

    let bob_poll = sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
    assert!(bob_poll.errors.is_empty(), "errors: {:?}", bob_poll.errors);
    assert_eq!(bob_poll.new_inquiries, 1);
    assert_eq!(get_post_inbox_summary_impl(Some(password.to_string()), &bob_state).unwrap().inquiries[0].counterparty_did, alice_privat_did);

    let bob_contacts = bob_state.load_contacts(&mut bob_state.service.lock().unwrap(), Some(password)).unwrap();
    let c_laden = bob_contacts.iter().find(|c| c.did == alice_laden_did).unwrap();
    let c_privat = bob_contacts.iter().find(|c| c.did == alice_privat_did).unwrap();
    assert_eq!(c_laden.syncs.len(), 1);
    assert_eq!(c_laden.syncs[0].write_url, a1_w);
    assert_eq!(c_privat.syncs.len(), 2);
    assert!(c_privat.syncs.iter().any(|s| s.write_url == a2_w));
    assert!(c_privat.syncs.iter().any(|s| s.write_url == a3_w));
    assert!(server_a1.slots.lock().unwrap().is_empty());
    assert!(server_a2.slots.lock().unwrap().contains_key(&format!("{}_pc.post", bob_a_keys[1])));
    assert!(server_a3.slots.lock().unwrap().contains_key(&format!("{}_pc.post", bob_a_keys[2])));

    let alice_priv_poll = sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_privat_state).await.unwrap();
    assert!(alice_priv_poll.errors.is_empty());
    assert_eq!(alice_priv_poll.imported_receipts, 1);
    assert!(load_post_state(&alice_privat_state, Some(password)).unwrap().outbox_entries.is_empty());
}

#[tokio::test]
async fn test_h5_outbox_holds_back_value_transfers_under_fork_lock_and_known_offender() {
    let dir_guard = TempDirGuard(temp_test_dir("post_h5_outflow_preflight"));
    let password = "StrongTestPassword123!";
    let alice_state = create_test_state(setup_test_app(&dir_guard.0.join("alice"), "Alice", ALICE_PHRASE, password));
    let bob_svc = setup_test_app(&dir_guard.0.join("bob"), "Bob", BOB_PHRASE, password);
    let bob_id = bob_svc.with_wallet_and_identity(|_, id| id.clone()).unwrap();

    let server = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (write_url, saved) = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;
    assert!(write_url.ends_with("/sync.php") && !saved.access_pubkey.is_empty());

    let signing_ctx = load_post_signing_context(&alice_state, Some(password)).unwrap();
    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();
    for (msg_id, bundle_id, class, fill, ts) in [
        ("msg-ctrl-1", None, crate::state::OutboxMessageClass::Control, 0xAA, 1_700_000_010u64),
        ("msg-value-1", Some("bundle-h5-1".to_string()), crate::state::OutboxMessageClass::ValueTransfer, 0xBB, 1_700_000_020u64),
    ] {
        enqueue_post_envelope(
            &mut post_state,
            &signing_ctx.identity.user_id,
            &signing_ctx.sync_ctx.my_ed_sk,
            &bob_id.user_id,
            &bob_id.public_key,
            msg_id.to_string(),
            bundle_id,
            class,
            &signing_ctx.own_reply_syncs,
            vec![fill; 64],
            ts,
        )
        .unwrap();
    }
    save_post_state(&alice_state, &post_state, Some(password)).unwrap();

    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state.mutate_internal_lifecycle(&mut s, Some(password), |lc| lc.pending_restore_l2_reconciliation = true).unwrap();
    }
    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();
    let session = alice_state.begin_session_op().unwrap();
    let ctrl_tag = post_state.outbox_entries.iter().find(|e| e.msg_id == "msg-ctrl-1").unwrap().entry.tag16;
    let value_tag = post_state.outbox_entries.iter().find(|e| e.msg_id == "msg-value-1").unwrap().entry.tag16;

    let outcome1 = flush_post_slots_impl(&alice_state, &session, &mut post_state, None, &[], Some(password)).await;
    assert!(outcome1.uploaded_counts_by_tag.contains_key(&ctrl_tag));
    assert!(!outcome1.uploaded_counts_by_tag.contains_key(&value_tag));

    post_state.outbox_entries.retain(|e| e.msg_id != "msg-ctrl-1");
    save_post_state(&alice_state, &post_state, Some(password)).unwrap();
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state.mutate_internal_lifecycle(&mut s, Some(password), |lc| lc.pending_restore_l2_reconciliation = false).unwrap();
        let (wallet, _) = s.get_unlocked_mut_for_test();
        wallet.proof_store.proofs.insert(
            "proof-h5-1".to_string(),
            dummy_known_offender_proof("proof-h5-1", &bob_id.user_id, &signing_ctx.identity.user_id),
        );
        assert!(
            matches!(
                wallet.assert_not_known_offender(&bob_id.user_id),
                Err(human_money_core::Error::KnownOffenderTransactionBlocked {
                    ref id,
                    ref proof_id,
                }) if id == &bob_id.user_id && proof_id == "proof-h5-1"
            ),
            "Bob must be blocked as KnownOffender with proof-h5-1"
        );
    }

    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &bob_id.user_id,
        &bob_id.public_key,
        "msg-ctrl-2".to_string(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        vec![0xCC; 64],
        1_700_000_030,
    )
    .unwrap();
    let ctrl2_tag = post_state.outbox_entries.iter().find(|e| e.msg_id == "msg-ctrl-2").unwrap().entry.tag16;

    let outcome2 = flush_post_slots_impl(&alice_state, &session, &mut post_state, None, &[], Some(password)).await;
    assert!(outcome2.uploaded_counts_by_tag.contains_key(&ctrl2_tag));
    assert!(!outcome2.uploaded_counts_by_tag.contains_key(&value_tag));
    assert!(post_state.outbox_entries.iter().any(|e| e.msg_id == "msg-value-1"));
}

#[tokio::test]
async fn h8_post_tolerance_zone_parks_confirms_and_dismisses() {
    use human_money_core::storage::{AuthMethod, FileStorage};

    let dir_guard = TempDirGuard(temp_test_dir("h8_post_tolerance_zone"));
    let password = "TestPassword123!";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(2), true).await;

    // Put Bob into recovery epoch 1 started 30m in future so Alice's transfer hits soft tolerance zone.
    let bob_folder = fix.bob_state.service.lock().unwrap().list_profiles().unwrap()[0]
        .folder_name
        .clone();
    {
        let mut storage = FileStorage::new(dir_guard.0.join("bob").join(&bob_folder));
        let auth = AuthMethod::Password(password);
        let mut seal_rec = storage.load_seal(&auth).unwrap().unwrap();
        seal_rec.seal.payload.epoch = 1;
        seal_rec.seal.payload.epoch_start_time = (Utc::now() + chrono::Duration::minutes(30))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        storage.save_seal(&auth, &seal_rec).unwrap();
    }

    let (_, std_map) = load_freetaler_std_map();
    let lid1 = create_freetaler_voucher(&fix.alice_state, &fix.alice_did, "40", None, password);
    let lid2 = create_freetaler_voucher(&fix.alice_state, &fix.alice_did, "25", None, password);

    for (lid, amt) in [(lid1, "10"), (lid2, "15")] {
        let res = crate::services::transfer_service::create_transfer_bundle_impl(
            None,
            fix.bob_did.clone(),
            vec![crate::models::FrontendSourceTransfer {
                local_instance_id: lid,
                amount_to_send: amt.to_string(),
            }],
            None,
            Some("Alice".to_string()),
            std_map.clone(),
            Some(false),
            Some(false),
            Some(true),
            Some(password.to_string()),
            &fix.alice_state,
        )
        .await
        .unwrap();
        let deposit = res
            .post_deposit
            .expect("transfer bundle must report post_deposit");
        assert_eq!(deposit.deposited_to, 2, "errors: {:?}", deposit.errors);
        assert_eq!(deposit.recipient_deposited_to, 1);
        assert!(deposit.errors.is_empty(), "errors: {:?}", deposit.errors);
    }

    let sync1 = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(sync1.new_tolerance_payments, 2);
    assert_eq!(sync1.imported_payments, 0);
    let post_after_sync1 = load_post_state(&fix.bob_state, Some(password)).unwrap();
    assert_eq!(post_after_sync1.pending_tolerance_payments.len(), 2);
    assert!(
        post_after_sync1.outbox_entries.is_empty(),
        "H8: ACK must NOT be queued until user confirms parked tolerance payment"
    );

    let sync2 = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(sync2.new_tolerance_payments, 0);

    let summary = get_post_inbox_summary_impl(Some(password.to_string()), &fix.bob_state).unwrap();
    assert_eq!(summary.pending_tolerance_payments.len(), 2);
    assert_eq!(summary.pending_tolerance_payments[0].zone, "soft");

    let confirm_id = summary.pending_tolerance_payments[0].msg_id.clone();
    let dismiss_id = summary.pending_tolerance_payments[1].msg_id.clone();

    let payload = confirm_pending_post_payment_impl(
        None,
        confirm_id,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(payload.newly_committed, Some(true));
    let post_after_confirm = load_post_state(&fix.bob_state, Some(password)).unwrap();
    assert_eq!(post_after_confirm.pending_tolerance_payments.len(), 1);
    assert_eq!(post_after_confirm.outbox_entries.len(), 1);

    dismiss_pending_post_item_impl(
        "tolerance_payment".to_string(),
        dismiss_id,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .unwrap();
    assert!(load_post_state(&fix.bob_state, Some(password))
        .unwrap()
        .pending_tolerance_payments
        .is_empty());
}

#[test]
fn test_h4_queue_receipt_is_deterministic_and_idempotent() {
    let dir_guard = TempDirGuard(temp_test_dir("h4_idempotent_ack"));
    let pw = "TestPassword123!";
    let alice_svc = setup_test_app(&dir_guard.0.join("alice"), "Alice", ALICE_PHRASE, pw);
    let bob_svc = setup_test_app(&dir_guard.0.join("bob"), "Bob", BOB_PHRASE, pw);
    let (my_id, my_sk) = alice_svc
        .with_wallet_and_identity(|_, id| (id.clone(), id.signing_key.clone()))
        .unwrap();
    let bob_did = bob_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let mut post_state = StoredPostState::default();
    let tag16 = [7u8; 16];
    for ts in [1_760_000_000u64, 1_760_000_010u64] {
        super::queue_receipt(
            &mut post_state,
            &my_id,
            &my_sk,
            &bob_did,
            "msg-unique-1",
            &tag16,
            Some("bundle-1"),
            &[],
            ts,
        );
    }
    assert_eq!(post_state.outbox_entries.len(), 1);
    assert_eq!(post_state.outbox_entries[0].msg_id, "ack-msg-unique-1");
}

#[tokio::test]
async fn test_h1_h6_claim_bearing_outflow_guard_and_stale_session_rejection() {
    let dir_guard = TempDirGuard(temp_test_dir("h1_h6_outbox_guards"));
    let pw = "TestPassword123!";
    let alice_svc = setup_test_app(&dir_guard.0.join("alice"), "Alice", ALICE_PHRASE, pw);
    let bob_svc = setup_test_app(&dir_guard.0.join("bob"), "Bob", BOB_PHRASE, pw);
    let bob_did = bob_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_pk = *human_money_core::ValidatedUserId::parse(&bob_did)
        .unwrap()
        .public_key();

    let alice_state = create_test_state(alice_svc);
    let stale_session = {
        let s = alice_state.lock_service().unwrap();
        alice_state.session_token_for_service(&s).unwrap()
    };

    crate::commands::auth::logout_impl(&alice_state);
    let stale_err =
        super::load_post_state_for_session(&alice_state, &stale_session, Some(pw)).unwrap_err();
    assert!(
        stale_err.message.contains("session changed"),
        "unexpected H1 err: {:?}",
        stale_err
    );

    let fresh_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice2"),
        "Alice2",
        ALICE_PHRASE,
        pw,
    ));
    let (session, my_id, my_sk) = {
        let mut s = fresh_state.lock_service().unwrap();
        let sess = fresh_state.session_token_for_service(&s).unwrap();
        let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
        s.persist_seal_lock_reason(
            human_money_core::models::seal::SealLockReason::ForkDetected,
            Some(pw),
        )
        .unwrap();
        (sess, id.clone(), id.signing_key.clone())
    };

    let reply_err = reply_detached_signature_via_post_impl(
        None,
        bob_did.clone(),
        b"detached-sig".to_vec(),
        Some(pw.into()),
        &fresh_state,
    )
    .await
    .unwrap_err();
    assert_eq!(reply_err.code, "error.security.walletLockedDueToFork");

    assert!(crate::state::OutboxMessageClass::ClaimBearing.requires_outflow_guard());
    let mut ps = StoredPostState::default();
    enqueue_post_envelope(
        &mut ps,
        &my_id.user_id,
        &my_sk,
        &bob_did,
        &bob_pk,
        "claim-1".into(),
        None,
        crate::state::OutboxMessageClass::ClaimBearing,
        &[],
        b"claim".to_vec(),
        1_760_000_000,
    )
    .unwrap();
    let (blocked, _) =
        super::outbox::collect_flush_security_for_test(&fresh_state, &session, &ps, Some(pw));
    assert!(blocked, "H6: ClaimBearing must be blocked when ForkDetected");
}

#[tokio::test]
async fn k2_h21_dispatch_rejects_spoofed_trust_assertion_and_mismatched_payment_uri() {
    let dir_guard = TempDirGuard(temp_test_dir("k2_h21_post_guards"));
    let pw = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        pw,
    ));
    let (my_id, my_ed_sk) = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|_, id| (id.clone(), id.signing_key.clone()))
            .unwrap()
    };
    let bob_did = human_money_core::test_utils::ACTORS.bob.user_id.clone();
    let charlie_did = human_money_core::test_utils::ACTORS.charlie.user_id.clone();
    let session = alice_state.begin_session_op().unwrap();
    let now_secs = 1_760_000_000u64;

    // 1. K2: TrustAssertion with assertion.sender_did != ctx.sender_did is rejected.
    let assertion = humoco_market::WotHandshakeAssertion {
        sender_did: my_id.user_id.clone(),
        sender_name: Some("Spoofed".into()),
        proposed_level: 1,
        wot_sync_json: "{}".into(),
        reply_requested: false,
        created_at: now_secs,
    };
    let assertion_bytes = serde_json::to_vec(&assertion).unwrap();
    let env_k2 = PostEnvelope::new("k2-msg", &bob_did, now_secs, Vec::new(), Vec::new());
    let ctx_k2 = super::handlers::PostEnvelopeContext {
        state: &alice_state,
        session: &session,
        app: None,
        extra_standards: None,
        password: Some(pw),
        my_identity: &my_id,
        my_ed_sk: &my_ed_sk,
        own_reply_syncs: &[],
        sender_did: &bob_did,
        envelope: &env_k2,
        tag16: &[1u8; 16],
        decrypted_bytes: &assertion_bytes,
        now_secs,
    };
    let mut ps = StoredPostState::default();
    let mut baseline = StoredPostState::default();
    let mut stats = super::handlers::PostDispatchStats::default();
    super::handlers::dispatch_post_payload(
        &ctx_k2,
        &PayloadType::TrustAssertion,
        &mut ps,
        &mut baseline,
        &mut stats,
    )
    .await;
    assert!(ps.pending_wot_handshakes.is_empty());
    assert!(
        stats.errors.iter().any(|e| e.contains("spoofed sender DID")),
        "expected spoofed sender DID error, got {:?}",
        stats.errors
    );

    // 2. H21: MarketInquiryMessage confirm payment_uri pointing to Charlie on Bob's thread is dropped.
    ps.inquiries.push(crate::models::FrontendMarketInquiryItem {
        inquiry_id: "inq-1".into(),
        role: "buyer".into(),
        target: FrontendInquiryTarget::Person { skill_title: None },
        counterparty_did: bob_did.clone(),
        counterparty_name: Some("Bob".into()),
        counterparty_picture_url: None,
        is_contact: true,
        status: "open".into(),
        payment_uri: None,
        amount_hint: None,
        standard_uuid: None,
        created_at: now_secs - 10,
        updated_at: now_secs - 10,
        unread: false,
        reply_syncs: Vec::new(),
        messages: Vec::new(),
    });
    let inq_msg = humoco_market::MarketInquiryMessage {
        inquiry_id: "inq-1".into(),
        msg_id: "m-1".into(),
        text: Some("Pay here".into()),
        decision: Some(humoco_market::MarketInquiryDecision::Confirm),
        payment_uri: Some(format!("humoco://pay?to={}&amount=10.00", charlie_did)),
        amount_hint: None,
        standard_uuid: None,
        created_at: now_secs,
    };
    let inq_bytes = serde_json::to_vec(&inq_msg).unwrap();
    let env_h21 = PostEnvelope::new("m-1", &bob_did, now_secs, Vec::new(), Vec::new());
    let ctx_h21 = super::handlers::PostEnvelopeContext {
        state: &alice_state,
        session: &session,
        app: None,
        extra_standards: None,
        password: Some(pw),
        my_identity: &my_id,
        my_ed_sk: &my_ed_sk,
        own_reply_syncs: &[],
        sender_did: &bob_did,
        envelope: &env_h21,
        tag16: &[2u8; 16],
        decrypted_bytes: &inq_bytes,
        now_secs,
    };
    let mut stats_h21 = super::handlers::PostDispatchStats::default();
    super::handlers::dispatch_post_payload(
        &ctx_h21,
        &PayloadType::Generic(humoco_market::PAYLOAD_MARKET_INQUIRY_MESSAGE_V1.into()),
        &mut ps,
        &mut baseline,
        &mut stats_h21,
    )
    .await;
    assert!(ps.inquiries[0].payment_uri.is_none());
    assert!(
        stats_h21
            .errors
            .iter()
            .any(|e| e.contains("Dropped mismatched inquiry payment URI")),
        "expected mismatched payment_uri error, got {:?}",
        stats_h21.errors
    );
}

#[tokio::test]
async fn k5_flush_post_slots_412_removes_pruned_outbox_tags_with_fresh_etag() {
    let dir_guard = TempDirGuard(temp_test_dir("k5_412_remove_tags"));
    let pw = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        pw,
    ));
    let server = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (server_url, alice_drop) = setup_test_sync(&server, DROP_PUB_A, pw, &alice_state).await;

    let (my_ed_sk, master_seed, my_did) = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|_, id| {
            (
                id.signing_key.clone(),
                id.signing_key.to_bytes(),
                id.user_id.clone(),
            )
        })
        .unwrap()
    };
    let pruned_env = PostEnvelope::new(
        "pruned-1",
        &my_did,
        1_760_000_000,
        Vec::new(),
        b"acked".to_vec(),
    );
    let kept_env = PostEnvelope::new(
        "kept-1",
        &my_did,
        1_760_000_001,
        Vec::new(),
        b"keep".to_vec(),
    );
    let pruned_box = seal_post_box(&my_ed_sk, &my_ed_sk.verifying_key(), &pruned_env).unwrap();
    let kept_box = seal_post_box(&my_ed_sk, &my_ed_sk.verifying_key(), &kept_env).unwrap();
    let pruned_tag = pruned_box.tag16;
    let kept_tag = kept_box.tag16;
    let remote_slot =
        humoco_market::encode_post_slot(&master_seed, &[pruned_box, kept_box], DROP_POST_MAX_BYTES)
            .unwrap();
    let slot_key = format!("{}_pc.post", alice_drop.access_pubkey);
    server
        .slots
        .lock()
        .unwrap()
        .insert(slot_key.clone(), remote_slot);
    server
        .etags
        .lock()
        .unwrap()
        .insert(slot_key.clone(), "\"etag-remote-2\"".to_string());

    let mut ps = load_post_state(&alice_state, Some(pw)).unwrap();
    ps.slot_etags_by_write_url
        .insert(server_url, "\"etag-stale-1\"".to_string());
    ps.pruned_outbox_tag16s.insert(hex::encode(pruned_tag));
    save_post_state(&alice_state, &ps, Some(pw)).unwrap();

    let session = alice_state.begin_session_op().unwrap();
    let outcome = flush_post_slots_impl(&alice_state, &session, &mut ps, None, &[], Some(pw)).await;
    assert_eq!(outcome.deposited_to, 1, "errors: {:?}", outcome.errors);
    assert!(ps.pruned_outbox_tag16s.is_empty());

    let raw_after = server.slots.lock().unwrap().get(&slot_key).cloned().unwrap();
    let real_entries_after: Vec<_> = humoco_market::decode_post_slot(&raw_after)
        .unwrap()
        .into_iter()
        .filter(|e| !humoco_market::is_own_dummy_post_box(&master_seed, e))
        .collect();
    assert_eq!(real_entries_after.len(), 1);
    assert_eq!(real_entries_after[0].tag16, kept_tag);
    assert!(!real_entries_after.iter().any(|e| e.tag16 == pruned_tag));
}

#[tokio::test]
async fn test_post_anchored_signing_request_recovers_encrypted_manifest_and_delegations() {
    let dir_guard = TempDirGuard(temp_test_dir("post_anchored_sign_req_s06_f10"));
    let password = "post-s06-f10-password";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(2), true).await;

    let (manifest, anchor_id, creator_probe_cache) = build_test_anchor_manifest_and_probe_cache();
    {
        let mut s_alice = fix.alice_state.service.lock().unwrap();
        s_alice
            .update_l2_anchor_manifest(manifest, Some(password))
            .unwrap();
    }
    let local_id = create_freetaler_voucher(
        &fix.alice_state,
        &fix.alice_did,
        "100",
        Some(anchor_id),
        password,
    );

    let signing_req_bytes = {
        let s = fix.alice_state.service.lock().unwrap();
        crate::commands::vouchers::create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::TargetDid(fix.bob_did.clone(), PrivacyMode::TrialDecryption),
            Some(&creator_probe_cache),
            Some(password),
            &s,
        )
        .unwrap()
    };
    let req_text = String::from_utf8_lossy(&signing_req_bytes);
    assert!(!req_text.contains("post-lock-1.example"));
    assert!(!req_text.contains("anchor_manifest_json"));

    send_signing_request_via_post_impl(
        fix.bob_did.clone(),
        signing_req_bytes,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.new_sign_requests, 1);

    let bob_post = load_post_state(&fix.bob_state, Some(password)).unwrap();
    let sign_req = &bob_post.pending_sign_requests[0];
    let opened = {
        let s_bob = fix.bob_state.service.lock().unwrap();
        crate::commands::vouchers::open_voucher_signing_request_impl(
            &sign_req.container_bytes,
            None,
            &s_bob,
        )
        .unwrap()
    };
    let manifest_json = opened
        .anchor_manifest_json
        .as_deref()
        .expect("PoST signing request must recover encrypted AnchorManifest");
    let delegations_json = opened
        .anchor_delegations_json
        .as_deref()
        .expect("PoST signing request must recover encrypted delegations");
    assert!(manifest_json.contains("post-lock-1.example"));
    assert!(delegations_json.contains("post-lock-2.example"));
    assert_eq!(opened.manifest_revision, Some(1));
}

#[tokio::test]
async fn s10_f1_outgoing_container_validation_rejects_cleartext_and_mismatched_type() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_f1_outgoing"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_state = create_test_state(setup_test_app(
        &dir_guard.0.join("bob"),
        "Bob",
        BOB_PHRASE,
        password,
    ));
    let alice_id = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet_and_identity(|_, id| id.clone())
        .unwrap();
    let bob_id = bob_state
        .service
        .lock()
        .unwrap()
        .with_wallet_and_identity(|_, id| id.clone())
        .unwrap();
    let bob_did = bob_id.user_id.clone();

    let cleartext = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::Cleartext,
        br#"{"hello":"world"}"#,
        PayloadType::VoucherForSigning,
    )
    .unwrap();
    let cleartext_err = send_signing_request_via_post_impl(
        bob_did.clone(),
        serde_json::to_vec(&cleartext).unwrap(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect_err("Cleartext signing request must be rejected");
    assert!(
        cleartext_err
            .message
            .contains("must be an asymmetrically encrypted VoucherForSigning container"),
        "unexpected cleartext error: {:?}",
        cleartext_err
    );

    let wrong_type = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
        br#"{"hello":"world"}"#,
        PayloadType::VoucherForSigning,
    )
    .unwrap();
    let wrong_type_err = reply_detached_signature_via_post_impl(
        None,
        bob_did.clone(),
        serde_json::to_vec(&wrong_type).unwrap(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect_err("Wrong PayloadType detached signature must be rejected");
    assert!(
        wrong_type_err
            .message
            .contains("must be an asymmetrically encrypted DetachedSignature container"),
        "unexpected wrong_type error: {:?}",
        wrong_type_err
    );

    let bad_json_err = send_signing_request_via_post_impl(
        bob_did.clone(),
        b"{not-json".to_vec(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect_err("Malformed container JSON must be rejected");
    assert!(bad_json_err
        .message
        .contains("Invalid Signing request container JSON"));

    let wrong_sender_container = SecureContainer::seal(
        &bob_id,
        &ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
        br#"{"hello":"world"}"#,
        PayloadType::VoucherForSigning,
    )
    .unwrap();
    let sig_err = send_signing_request_via_post_impl(
        bob_did,
        serde_json::to_vec(&wrong_sender_container).unwrap(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect_err("Container signed by another DID must be rejected");
    assert!(
        !sig_err.message.is_empty(),
        "Expected sender signature verification error"
    );
}

#[tokio::test]
async fn s10_f9_f10_incoming_container_validation_skew_and_ssrf_bounds() {
    use human_money_core::models::secure_container::{EncryptionType, PayloadType, SecureContainer};

    let dir_guard = TempDirGuard(temp_test_dir("s10_f9_f10_incoming"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(&dir_guard.0.join("alice"), "Alice", ALICE_PHRASE, password));
    let bob_state = create_test_state(setup_test_app(&dir_guard.0.join("bob"), "Bob", BOB_PHRASE, password));

    let (alice_did, alice_id, alice_sk) = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet_and_identity(|_, id| (id.user_id.clone(), id.clone(), id.signing_key.clone()))
        .unwrap();
    let bob_did = wallet_user_id(&bob_state);
    let bob_pk = human_money_core::crypto::get_pubkey_from_user_id(&bob_did).unwrap();

    let server = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let (write_url, alice_info) = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;
    let (bob_url, bob_info) = setup_test_sync(&server, DROP_PUB_A, password, &bob_state).await;
    assert_eq!(bob_url, write_url);
    assert!(!bob_info.access_pubkey.is_empty());

    let alice_prefix = human_money_core::crypto::get_prefix_from_user_id(&alice_did).map(str::to_string);
    let alice_desc = make_sync_desc(&write_url, &server, DROP_PUB_A, &alice_info.access_pubkey, alice_prefix.as_deref());
    save_test_contact(&bob_state, &alice_did, "Alice", Some(1), vec![alice_desc], password);
    save_test_contact(&alice_state, &bob_did, "Bob", Some(1), vec![], password);
    publish_seller_profile(&bob_state, "Bob", vec![], password).await;

    let now = crate::sync_client::current_unix_timestamp();
    let inq_bytes = serde_json::to_vec(&humoco_market::MarketInquiry {
        inquiry_id: "inq-unsigned".to_string(),
        target: InquiryTarget::Person { skill_title: None },
        message: "hi".to_string(),
        created_at: now,
    })
    .unwrap();
    let mut unsigned_inq = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
        &inq_bytes,
        PayloadType::Generic(humoco_market::PAYLOAD_MARKET_INQUIRY_V1.to_string()),
    )
    .unwrap();
    unsigned_inq.signature.clear();

    let cleartext_inq = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::Cleartext,
        &inq_bytes,
        PayloadType::Generic(humoco_market::PAYLOAD_MARKET_INQUIRY_V1.to_string()),
    )
    .unwrap();
    assert_eq!(cleartext_inq.et, EncryptionType::None);

    let signed_req_bytes = serde_json::to_vec(
        &SecureContainer::seal(
            &alice_id,
            &ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
            br#"{"dummy":true}"#,
            PayloadType::VoucherForSigning,
        )
        .unwrap(),
    )
    .unwrap();

    let ssrf_drops = vec![humoco_market::DropDescriptor {
        write_url: "http://169.254.169.254/drop".to_string(),
        read_url: "http://169.254.169.254/read".to_string(),
        slot_id: None,
        drop_pub: DROP_PUB_A.to_string(),
    }];
    let boxes = vec![
        seal_post_box(&alice_sk, &bob_pk, &PostEnvelope::new("msg-unsigned", alice_did.clone(), now, vec![], serde_json::to_vec(&unsigned_inq).unwrap())).unwrap(),
        seal_post_box(&alice_sk, &bob_pk, &PostEnvelope::new("msg-cleartext", alice_did.clone(), now, vec![], serde_json::to_vec(&cleartext_inq).unwrap())).unwrap(),
        seal_post_box(&alice_sk, &bob_pk, &PostEnvelope::new("bad\nmsg-id", alice_did.clone(), now, vec![], signed_req_bytes.clone())).unwrap(),
        seal_post_box(&alice_sk, &bob_pk, &PostEnvelope::new("msg-future-skew", alice_did.clone(), now + 999_999, ssrf_drops, signed_req_bytes)).unwrap(),
    ];
    insert_packed_post_slot(&alice_state, &server, &alice_did, &boxes, password);

    let bob_sync = sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state).await.unwrap();
    assert_eq!(bob_sync.new_inquiries, 0);
    assert_eq!(bob_sync.new_sign_requests, 1);
    assert!(bob_sync.errors.iter().any(|e| e.contains("signature")));
    assert!(bob_sync.errors.iter().any(|e| e.contains("non-asymmetric")));
    assert!(bob_sync.errors.iter().any(|e| e.contains("invalid msg_id")));

    let bob_ps = load_post_state(&bob_state, Some(password)).unwrap();
    assert_eq!(bob_ps.pending_sign_requests.len(), 1);
    assert!(bob_ps.pending_sign_requests[0].created_at <= now + MAX_POST_FUTURE_SKEW_SECS + 5);
    assert!(bob_ps.pending_sign_requests[0].reply_syncs.is_empty());

    let mut s_bob = bob_state.service.lock().unwrap();
    let alice_contact = bob_state
        .load_contacts(&mut s_bob, Some(password))
        .unwrap()
        .into_iter()
        .find(|c| c.did == alice_did)
        .unwrap();
    assert!(!alice_contact.syncs.iter().any(|s| s.write_url.contains("169.254.169.254")));
}

#[tokio::test]
async fn s10_f2_tombstones_survive_partial_own_flush_and_per_tag_attribution() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_f2_tombstones"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));

    let live_srv = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let (live_w, live_info) = setup_test_sync(&live_srv, DROP_PUB_A, password, &alice_state).await;
    assert!(live_w.ends_with("/sync.php") && !live_info.access_pubkey.is_empty());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        alice_state
            .mutate_security_policy(&mut s_alice, Some(password), |p| {
                p.public_syncs.push(crate::settings::StoredPublicSync {
                    write_url: format!("http://127.0.0.1:{}/sync.php", dead_port),
                    read_url: format!("http://127.0.0.1:{}/sync", dead_port),
                    drop_pub: DROP_PUB_B.to_string(),
                    access_pubkey: ADMIN_B.to_string(),
                    enabled: true,
                    authorized_channels: vec!["post".to_string()],
                    ..Default::default()
                });
            })
            .unwrap();
    }

    let mut ps = load_post_state(&alice_state, Some(password)).unwrap();
    let tomb = hex::encode([0xAAu8; 16]);
    ps.pruned_outbox_tag16s.insert(tomb.clone());
    save_post_state(&alice_state, &ps, Some(password)).unwrap();

    let session = alice_state.begin_session_op().unwrap();
    let outcome =
        flush_post_slots_impl(&alice_state, &session, &mut ps, None, &[], Some(password)).await;
    assert_eq!(outcome.deposited_to, 1);
    assert!(!outcome.errors.is_empty());
    assert!(ps.pruned_outbox_tag16s.contains(&tomb));

    let (bob_vk, _) = human_money_core::crypto::generate_ed25519_keypair_for_tests(None);
    let bob_did = human_money_core::crypto::create_user_id(&bob_vk, None).unwrap();
    let my_sk = {
        let s_alice = alice_state.service.lock().unwrap();
        crate::services::common::SyncIdentityContext::from_service(&s_alice, Some(password))
            .unwrap()
            .my_ed_sk
    };
    let large_payload = vec![9u8; 40_000];
    let mut tags = Vec::new();
    for (idx, (id, ts)) in [
        ("large-box-1", 1_750_000_000u64),
        ("large-box-2", 1_750_000_001u64),
        ("large-box-3", 1_750_000_002u64),
    ]
    .into_iter()
    .enumerate()
    {
        let env = PostEnvelope::new(id, bob_did.clone(), ts, vec![], large_payload.clone());
        let entry = seal_post_box(&my_sk, &bob_vk, &env).unwrap();
        tags.push(entry.tag16);
        ps.outbox_entries.push(crate::state::PostOutboxEntry {
            msg_id: id.to_string(),
            recipient_did: bob_did.clone(),
            tag16_hex: hex::encode(entry.tag16),
            bundle_id: None,
            message_class: crate::state::OutboxMessageClass::Control,
            entry,
            created_at: ts,
        });
        assert_eq!(ps.outbox_entries.last().unwrap().msg_id, format!("large-box-{}", idx + 1));
    }

    let outcome2 =
        flush_post_slots_impl(&alice_state, &session, &mut ps, None, &[], Some(password)).await;
    assert_eq!(outcome2.uploaded_counts_by_tag.get(&tags[0]).copied(), Some(1));
    assert_eq!(
        outcome2.uploaded_counts_by_tag.get(&tags[2]).copied(),
        None,
        "Deferred overflowing box must have 0 per-tag deposited count"
    );
}

#[tokio::test]
async fn s10_f3_unreachable_recipient_inquiry_never_leaks_phantom_own_upload() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_f3_no_phantom"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_state = create_test_state(setup_test_app(
        &dir_guard.0.join("bob"),
        "Bob",
        BOB_PHRASE,
        password,
    ));
    let bob_did = wallet_user_id(&bob_state);

    let alice_srv = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let (alice_w, alice_info) =
        setup_test_sync(&alice_srv, DROP_PUB_A, password, &alice_state).await;
    assert!(alice_w.ends_with("/sync.php") && !alice_info.access_pubkey.is_empty());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = listener.local_addr().unwrap().port();
    drop(listener);
    let dead_rcpt_sync = FrontendSyncDescriptor {
        write_url: format!("http://127.0.0.1:{}/sync.php", dead_port),
        read_url: format!("http://127.0.0.1:{}/sync", dead_port),
        slot_id: None,
        drop_pub: DROP_PUB_B.to_string(),
        status: "active".to_string(),
        ..Default::default()
    };
    let err = send_market_inquiry_via_post_impl(
        bob_did,
        FrontendInquiryTarget::Person { skill_title: None },
        "Hallo Bob".to_string(),
        vec![dead_rcpt_sync],
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "error.post.recipientUnreachable");
    assert!(
        alice_srv.slots.lock().unwrap().is_empty(),
        "Failed recipient inquiry delivery must not deposit phantom box onto own sync server"
    );
}

#[tokio::test]
async fn s10_f3_canonical_ad_title_and_person_rate_limit_bucket() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_f3_rate_limit"));
    let password = "TestPassword123!";
    let fix = setup_two_party_sync_fixture(&dir_guard.0, password, Some(1), true).await;

    publish_seller_profile(&fix.bob_state, "Bob", vec![], password).await;
    let prof = upsert_ad_impl(
        FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Genuine Honey".to_string(),
            text: None,
            pricing: None,
            duration_days: 30,
            allow_inquiries: true,
            is_remote: false,
            image_webp_data_uri: None,
            event_at: None,
            event_all_day: false,
            published: Some(true),
        },
        Some(password.to_string()),
        &fix.bob_state,
    )
    .unwrap();
    let ad_id = prof.ads[0].id.clone();

    send_market_inquiry_via_post_impl(
        fix.bob_did.clone(),
        FrontendInquiryTarget::Ad {
            ad_id: ad_id.clone(),
            ad_title: "SPOOFED TITLE".to_string(),
        },
        "Interested in ad".to_string(),
        fix.bob_syncs.clone(),
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();

    for (skill, body) in [
        ("Skill 0", "Person inquiry 0"),
        ("Skill BypassAttempt", "Blocked bypass"),
    ] {
        send_market_inquiry_via_post_impl(
            fix.bob_did.clone(),
            FrontendInquiryTarget::Person {
                skill_title: Some(skill.to_string()),
            },
            body.to_string(),
            fix.bob_syncs.clone(),
            Some(password.to_string()),
            &fix.alice_state,
        )
        .await
        .unwrap();
    }

    let bob_sync = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.new_inquiries, 2);

    let bob_ps = load_post_state(&fix.bob_state, Some(password)).unwrap();
    let ad_inq = bob_ps
        .inquiries
        .iter()
        .find(|i| matches!(&i.target, FrontendInquiryTarget::Ad { .. }))
        .unwrap();
    assert_eq!(
        ad_inq.target,
        FrontendInquiryTarget::Ad {
            ad_id,
            ad_title: "Genuine Honey".to_string(),
        }
    );

    let person_inq_id = bob_ps
        .inquiries
        .iter()
        .find(|i| matches!(&i.target, FrontendInquiryTarget::Person { .. }))
        .unwrap()
        .inquiry_id
        .clone();
    dismiss_pending_post_item_impl(
        "inquiry".to_string(),
        person_inq_id,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .unwrap();

    send_market_inquiry_via_post_impl(
        fix.bob_did,
        FrontendInquiryTarget::Person {
            skill_title: Some("Skill AfterDismiss".to_string()),
        },
        "Now allowed".to_string(),
        fix.bob_syncs,
        Some(password.to_string()),
        &fix.alice_state,
    )
    .await
    .unwrap();

    let bob_sync_after = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &fix.bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync_after.new_inquiries, 1);
}

#[test]
fn s10_f4_f5_timeout_zero_summary_and_3way_merge_safety() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_f4_f5"));
    let password = "TestPassword123!";
    let bob_pk = human_money_core::crypto::generate_ed25519_keypair_for_tests(None).0;
    let bob_did = human_money_core::crypto::create_user_id(&bob_pk, None).unwrap();
    let carol_pk = human_money_core::crypto::generate_ed25519_keypair_for_tests(None).0;
    let carol_did = human_money_core::crypto::create_user_id(&carol_pk, None).unwrap();

    let wallet_dir = dir_guard.0.join("alice");
    let alice_state = create_test_state(setup_test_app(&wallet_dir, "Alice", ALICE_PHRASE, password));

    let mut ps = StoredPostState::default();
    ps.inquiries.push(crate::models::FrontendMarketInquiryItem {
        inquiry_id: "inq-zero".to_string(),
        role: "seller".to_string(),
        target: FrontendInquiryTarget::Person { skill_title: None },
        counterparty_did: bob_did.clone(),
        counterparty_name: None,
        counterparty_picture_url: None,
        is_contact: false,
        status: "open".to_string(),
        payment_uri: None,
        amount_hint: None,
        standard_uuid: None,
        created_at: 100,
        updated_at: 100,
        unread: true,
        reply_syncs: vec![],
        messages: vec![crate::models::FrontendInquiryChatMessage {
            msg_id: "m1".to_string(),
            sender: "them".to_string(),
            text: Some("hi".to_string()),
            decision: None,
            created_at: 100,
        }],
    });
    save_test_contact(&alice_state, &bob_did, "Cached Bob", Some(1), vec![], password);
    {
        let mut s = alice_state.service.lock().unwrap();
        super::save_post_state_with_service(&alice_state, &mut s, &ps, Some(password)).unwrap();
        alice_state
            .mutate_security_policy(&mut s, Some(password), |p| {
                p.session_timeout_seconds = 0;
            })
            .unwrap();
    }

    let mut reloaded_svc = human_money_core::app_service::AppService::new(&wallet_dir).unwrap();
    let folder = reloaded_svc.list_profiles().unwrap()[0].folder_name.clone();
    reloaded_svc
        .login(&folder, password, false, "device-drops-test".to_string())
        .unwrap();
    let reloaded_state =
        create_test_state(human_money_core::app_service::AppService::new(&wallet_dir).unwrap());
    reloaded_state
        .initialize_profile_session(&mut reloaded_svc, password)
        .unwrap();
    *reloaded_state.service.lock().unwrap() = reloaded_svc;
    assert!(!reloaded_state.service.lock().unwrap().is_session_active());

    let summary = get_post_inbox_summary_impl(None, &reloaded_state)
        .expect("get_post_inbox_summary_impl must succeed under timeout == 0 using caches");
    assert_eq!(summary.inquiries.len(), 1);
    assert_eq!(
        summary.inquiries[0].counterparty_name.as_deref(),
        Some("Cached Bob")
    );
    assert!(summary.inquiries[0].is_contact);

    let mut base = StoredPostState::default();
    base.pruned_outbox_tag16s.insert("tomb-a".to_string());
    base.replied_sign_request_ids
        .insert(scoped_processed_msg_key(&bob_did, "req-1"));
    let mut inq_base = summary.inquiries[0].clone();
    inq_base.unread = true;
    base.inquiries.push(inq_base);

    let mut latest = base.clone();
    latest.pruned_outbox_tag16s.clear();
    latest
        .pending_wot_handshakes
        .push(crate::models::FrontendPendingWotHandshake {
            msg_id: "shared-id".to_string(),
            sender_did: carol_did,
            sender_name: Some("Carol".to_string()),
            proposed_level: 1,
            reply_requested: false,
            created_at: 200,
        });

    let mut working = base.clone();
    working.inquiries[0].unread = false;
    working
        .pending_wot_handshakes
        .push(crate::models::FrontendPendingWotHandshake {
            msg_id: "shared-id".to_string(),
            sender_did: bob_did,
            sender_name: Some("Bob".to_string()),
            proposed_level: 1,
            reply_requested: false,
            created_at: 201,
        });

    {
        let mut s = alice_state.service.lock().unwrap();
        super::save_post_state_with_service(&alice_state, &mut s, &latest, Some(password)).unwrap();
        super::merge_post_state_with_service(&mut s, &base, &mut working, Some(password)).unwrap();
    }
    assert!(!working.pruned_outbox_tag16s.contains("tomb-a"));
    assert_eq!(working.pending_wot_handshakes.len(), 2);
    assert!(!working.inquiries[0].unread);
}

#[tokio::test]
async fn s10_f8_accept_wot_handshake_preserves_existing_contact_metadata() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_f8_preserve_contact"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_state = create_test_state(setup_test_app(
        &dir_guard.0.join("bob"),
        "Bob",
        BOB_PHRASE,
        password,
    ));
    let bob_did = wallet_user_id(&bob_state);

    let existing_sync = FrontendSyncDescriptor {
        write_url: "https://sync.example.com/write".to_string(),
        read_url: "https://sync.example.com/read".to_string(),
        slot_id: None,
        drop_pub: DROP_PUB_B.to_string(),
        status: "active".to_string(),
        ..Default::default()
    };
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s_alice,
                FrontendContact {
                    did: bob_did.clone(),
                    profile: FrontendUserProfile {
                        first_name: Some("Bob".to_string()),
                        ..Default::default()
                    },
                    tags: vec!["friend".to_string(), "neighbor".to_string()],
                    added_at: "2026-01-01T00:00:00Z".to_string(),
                    notes: Some("Met in Berlin".to_string()),
                    trust_level: None,
                    desired_hops: Some(3),
                    has_peer_list: false,
                    peer_list_updated_at: None,
                    syncs: vec![existing_sync.clone()],
                },
                Some(password),
            )
            .unwrap();
        let mut ps = load_post_state_with_service(&mut s_alice, Some(password)).unwrap();
        let scoped_k = scoped_processed_msg_key(&bob_did, "hs-1");
        ps.pending_wot_handshakes
            .push(crate::models::FrontendPendingWotHandshake {
                msg_id: "hs-1".to_string(),
                sender_did: bob_did.clone(),
                sender_name: Some("Bob".to_string()),
                proposed_level: 1,
                reply_requested: false,
                created_at: 1_750_000_000,
            });
        let bob_reg = {
            let mut sb = bob_state.service.lock().unwrap();
            let mut r = bob_state
                .get_or_load_wot_register(&mut sb, Some(password))
                .unwrap();
            bob_state.ensure_wot_list_signed(&mut sb, &mut r).unwrap();
            r
        };
        let (pkg, _) = bob_reg.prepare_sync_package(None, None, true).unwrap();
        ps.pending_wot_packages_by_msg_id
            .insert(scoped_k, serde_json::to_string(&pkg).unwrap());
        save_post_state_with_service(&alice_state, &mut s_alice, &ps, Some(password)).unwrap();
    }

    accept_wot_handshake_impl(
        "hs-1".to_string(),
        2,
        false,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    let updated_bob = {
        let mut s_alice = alice_state.service.lock().unwrap();
        alice_state
            .load_contacts(&mut s_alice, Some(password))
            .unwrap()
            .into_iter()
            .find(|c| c.did == bob_did)
            .unwrap()
    };
    assert_eq!(updated_bob.trust_level, Some(2));
    assert_eq!(updated_bob.notes.as_deref(), Some("Met in Berlin"));
    assert_eq!(
        updated_bob.tags,
        vec!["friend".to_string(), "neighbor".to_string()]
    );
    assert_eq!(updated_bob.syncs.len(), 1);
    assert_eq!(updated_bob.syncs[0].write_url, existing_sync.write_url);
}

#[tokio::test]
async fn s10_f7_guardian_locator_retries_before_assign_and_stops_on_revoked_epoch() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_f7_guardian_locator"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_did = human_money_core::test_utils::ACTORS.bob.user_id.clone();
    let alice_id = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet_and_identity(|_, id| id.clone())
        .unwrap();

    let session = alice_state.begin_session_op().unwrap();
    let now = 1_760_000_000u64;
    let update = humoco_market::GuardianLocatorUpdate {
        owner_did: bob_did.clone(),
        rotation_epoch: 1,
        backup_sync_descriptors: vec![],
        updated_at: now,
    };
    let env = PostEnvelope::new("loc-1", bob_did.clone(), now, vec![], vec![]);
    let my_sk = alice_id.signing_key.clone();
    let payload_bytes = serde_json::to_vec(&update).unwrap();
    let ctx = super::handlers::PostEnvelopeContext {
        state: &alice_state,
        session: &session,
        app: None,
        extra_standards: None,
        password: Some(password),
        my_identity: &alice_id,
        my_ed_sk: &my_sk,
        own_reply_syncs: &[],
        sender_did: &bob_did,
        envelope: &env,
        tag16: &[1u8; 16],
        decrypted_bytes: &payload_bytes,
        now_secs: now,
    };
    let mut ps = StoredPostState::default();
    let mut base_ps = ps.clone();
    let mut stats = super::handlers::PostDispatchStats::default();
    let outcome_early = super::handlers::dispatch_post_payload(
        &ctx,
        &PayloadType::Generic(humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1.to_string()),
        &mut ps,
        &mut base_ps,
        &mut stats,
    )
    .await;
    assert_eq!(
        outcome_early,
        super::handlers::PostDispatchOutcome::RetryLater
    );

    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let mut vault = humoco_market::GuardianVaultStore::new();
        vault.revoked_epoch_by_owner.insert(
            humoco_market::canonicalize_owner_account_id(&bob_did).unwrap(),
            1,
        );
        crate::services::recovery_service::save_guardian_vault_with_cache(
            &alice_state,
            &mut s_alice,
            &vault,
            Some(password),
        )
        .unwrap();
    }
    let outbox_before = ps.outbox_entries.len();
    let outcome_revoked = super::handlers::dispatch_post_payload(
        &ctx,
        &PayloadType::Generic(humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1.to_string()),
        &mut ps,
        &mut base_ps,
        &mut stats,
    )
    .await;
    assert_eq!(outcome_revoked, super::handlers::PostDispatchOutcome::Handled);
    assert_eq!(ps.outbox_entries.len(), outbox_before);
}

#[test]
fn s10_q2_processed_msg_ids_30d_retention_and_no_resurrection_on_merge() {
    let dir_guard = TempDirGuard(temp_test_dir("s10_q2_retention"));
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &dir_guard.0.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));

    let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
    let exact_expiry = now_secs.saturating_sub(super::PROCESSED_MSG_RETENTION_SECS);
    let fresh_ts = now_secs.saturating_sub(humoco_market::POST_TTL_SECS);

    let mut map = std::collections::BTreeMap::new();
    map.insert("expired-exact".to_string(), exact_expiry);
    map.insert("expired-older".to_string(), exact_expiry.saturating_sub(100));
    map.insert("fresh-within-ttl".to_string(), fresh_ts);
    map.insert("fresh-recent".to_string(), now_secs);
    super::prune_expired_processed_msg_ids(&mut map, now_secs);
    assert!(!map.contains_key("expired-exact"));
    assert!(!map.contains_key("expired-older"));
    assert!(map.contains_key("fresh-within-ttl"));
    assert!(map.contains_key("fresh-recent"));

    let mut baseline = StoredPostState::default();
    baseline
        .processed_msg_ids
        .insert("old-key".to_string(), exact_expiry.saturating_sub(50));
    baseline
        .processed_msg_ids
        .insert("keep-key".to_string(), now_secs);

    let mut latest = baseline.clone();
    latest
        .processed_msg_ids
        .insert("concurrent-new".to_string(), now_secs);

    let mut working = baseline.clone();
    super::prune_expired_processed_msg_ids(&mut working.processed_msg_ids, now_secs);
    working
        .processed_msg_ids
        .insert("worker-new".to_string(), now_secs);

    {
        let mut s = alice_state.service.lock().unwrap();
        super::save_post_state_with_service(&alice_state, &mut s, &latest, Some(password)).unwrap();
        super::merge_post_state_with_service(&mut s, &baseline, &mut working, Some(password))
            .unwrap();
    }

    assert!(!working.processed_msg_ids.contains_key("old-key"));
    assert!(working.processed_msg_ids.contains_key("keep-key"));
    assert!(working.processed_msg_ids.contains_key("concurrent-new"));
    assert!(working.processed_msg_ids.contains_key("worker-new"));
}

#[test]
fn test_post_time_window_clamp_and_tombstone_cap_invariants() {
    let now = 1_760_000_000u64;
    assert_eq!(clamp_post_ts(now - 50, now), now - 50);
    assert_eq!(clamp_post_ts(now, now), now);
    assert_eq!(
        clamp_post_ts(now + MAX_POST_FUTURE_SKEW_SECS + 500, now),
        now + MAX_POST_FUTURE_SKEW_SECS
    );

    assert!(is_within_retry_window(now, now));
    assert!(is_within_retry_window(now + MAX_POST_FUTURE_SKEW_SECS, now));
    assert!(!is_within_retry_window(
        now + MAX_POST_FUTURE_SKEW_SECS + 1,
        now
    ));
    assert!(is_within_retry_window(
        now - POST_DEFERRED_RETRY_WINDOW_SECS,
        now
    ));
    assert!(!is_within_retry_window(
        now - POST_DEFERRED_RETRY_WINDOW_SECS - 1,
        now
    ));

    assert_eq!(
        inquiry_rate_limit_key(&FrontendInquiryTarget::Ad {
            ad_id: "ad-42".to_string(),
            ad_title: "Honey".to_string(),
        }),
        "ad:ad-42"
    );
    assert_eq!(
        inquiry_rate_limit_key(&FrontendInquiryTarget::Person {
            skill_title: Some("Honey".to_string()),
        }),
        "person"
    );
    assert_eq!(
        inquiry_rate_limit_key(&FrontendInquiryTarget::Person { skill_title: None }),
        "person"
    );

    let mut tombstones = std::collections::BTreeSet::new();
    for i in 0..(MAX_PRUNED_OUTBOX_TAG16S + 5) {
        push_pruned_outbox_tag16(&mut tombstones, format!("tag-{:04}", i));
    }
    push_pruned_outbox_tag16(&mut tombstones, format!("tag-{:04}", MAX_PRUNED_OUTBOX_TAG16S));
    assert_eq!(tombstones.len(), MAX_PRUNED_OUTBOX_TAG16S);
    assert!(!tombstones.contains("tag-0000"));
    assert!(!tombstones.contains("tag-0004"));
    assert!(tombstones.contains("tag-0005"));
    assert!(tombstones.contains(&format!("tag-{:04}", MAX_PRUNED_OUTBOX_TAG16S + 4)));
}
