use std::collections::HashMap;

use chrono::Utc;
use human_money_core::models::secure_container::{ContainerConfig, PrivacyMode};
use human_money_core::test_utils;
use humoco_market::{InquiryTarget, DROP_POST_MAX_BYTES};

use super::*;
use crate::models::{
    FrontendAdInput, FrontendContact, FrontendSaveMyMarketProfileRequest, FrontendSyncDescriptor,
    FrontendUserProfile, TransactionRecord,
};
use crate::services::market_service::{
    save_my_market_profile_impl, set_card_published_impl, upsert_ad_impl,
};
use crate::services::sync_engine::{probe_public_sync_impl, save_public_sync_impl};
use crate::test_support::{
    create_test_state, setup_test_app, temp_test_dir, MockSyncServer, ALICE_PHRASE, BOB_PHRASE,
};

const DROP_PUB_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DROP_PUB_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ADMIN_A: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const ADMIN_B: &str = "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1";

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

#[tokio::test]
async fn test_dual_delivery_payment_and_receipt_idempotency_sst_v4() {
    let base_dir = temp_test_dir("dual_delivery_test");
    let password = "TestPassword123!";

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);

    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    let bob_did = {
        let s = bob_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    // Start Drop Server A for Alice & Drop Server B for Bob
    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
    let (server_b_url, bob_drop) =
        setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    // Alice and Bob add each other to address book with Level 2 and their respective drop
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let contact_bob = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_b_url.clone(),
                read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                slot_id: Some(
                    humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap(),
                ),
                drop_pub: DROP_PUB_B.to_string(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }],
        };
        alice_state
            .save_contact(&mut s_alice, contact_bob, Some(password))
            .unwrap();
    }

    {
        let mut s_bob = bob_state.service.lock().unwrap();
        let contact_alice = FrontendContact {
            did: alice_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_a_url.clone(),
                read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                slot_id: Some(
                    humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap(),
                ),
                drop_pub: DROP_PUB_A.to_string(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }],
        };
        bob_state
            .save_contact(&mut s_bob, contact_alice, Some(password))
            .unwrap();
    }

    // 1. Alice creates standard and voucher
    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(
        "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
        standard_toml.clone(),
    );

    {
        let mut s_alice = alice_state.service.lock().unwrap();
        s_alice
            .create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        first_name: Some("Alice".to_string()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "100".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    non_redeemable_test_voucher: false,
                    collateral: None,
                    layer2_anchor: None,
                },
                Some(password),
            )
            .unwrap();
    }

    let local_id = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone()
    };

    // 2. Alice transfers to Bob
    let transfer_req = human_money_core::wallet::types::MultiTransferRequest {
        recipient_id: bob_did.clone(),
        sources: vec![human_money_core::wallet::types::SourceTransfer {
            local_instance_id: local_id.clone(),
            amount_to_send: "20".to_string(),
        }],
        notes: Some("Dual delivery payment".to_string()),
        sender_profile_name: Some("Alice".to_string()),
        use_privacy_mode: Some(true),
        wot_sync_json: None,
        contact_card_json: None,
    };

    let bundle_res = {
        let mut s = alice_state.service.lock().unwrap();
        s.create_transfer_bundle(transfer_req, &std_map, None, Some(password))
            .unwrap()
    };
    let bundle_bytes = bundle_res.bundle_bytes;
    let bundle_id = bundle_res.bundle_id;

    // Save transaction record to Alice's history
    let record = TransactionRecord {
        id: bundle_id.clone(),
        direction: "sent".to_string(),
        recipient_id: bob_did.clone(),
        sender_id: alice_did.clone(),
        timestamp: Utc::now().to_rfc3339(),
        summable_amounts: HashMap::new(),
        countable_items: HashMap::new(),
        involved_vouchers: vec![local_id.clone()],
        involved_sources_details: None,
        bundle_data: bundle_bytes.clone(),
        bundle_id: bundle_id.clone(),
        notes: None,
        sender_profile_name: Some("Alice".to_string()),
    };
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .append_to_history(&mut s, record, Some(password))
            .unwrap();
    }

    // 3. Alice deposits to post twice: must be identical bytes, NEVER calling create_transfer_bundle again!
    let dep1 =
        deposit_transfer_to_post_impl(bundle_id.clone(), Some(password.to_string()), &alice_state)
            .await
            .unwrap();
    assert_eq!(dep1.deposited_to, 1);
    let dep2 =
        deposit_transfer_to_post_impl(bundle_id.clone(), Some(password.to_string()), &alice_state)
            .await
            .unwrap();
    assert_eq!(dep1.msg_id, dep2.msg_id);

    let post_state_alice = load_post_state(&alice_state, Some(password)).unwrap();
    assert_eq!(post_state_alice.outbox_entries.len(), 1);
    assert_eq!(
        post_state_alice.outbox_entries[0].bundle_id.as_deref(),
        Some(bundle_id.as_str())
    );

    // 4. Bob syncs post drops: receives payment & queues receipt
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

    // 5. Bob tries to import the same bundle offline/manually: strictly idempotent!
    let offline_import_attempt = {
        let mut s = bob_state.service.lock().unwrap();
        s.receive_bundle(&bundle_bytes, &std_map, None, Some(password), false)
    };
    assert!(
        offline_import_attempt.is_err(),
        "Duplicate import must return BundleAlreadyProcessed"
    );

    // 6. Alice syncs post drops: downloads receipt from Bob, prunes acknowledged outbox entry!
    let alice_sync =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_state)
            .await
            .unwrap();
    assert_eq!(alice_sync.imported_receipts, 1);
    assert_eq!(alice_sync.pruned_boxes, 1);

    let post_state_alice_final = load_post_state(&alice_state, Some(password)).unwrap();
    assert!(
        post_state_alice_final.outbox_entries.is_empty(),
        "Outbox entry must be acknowledged and pruned"
    );
}

#[tokio::test]
async fn test_asymmetry_alice_has_sync_bob_has_none() {
    let base_dir = temp_test_dir("asymmetry_test");
    let password = "TestPassword123!";

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);

    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    let bob_did = {
        let s = bob_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    // Server for Alice only; Bob has NO drop server
    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;

    // Bob has Alice in his address book with Alice's drop
    {
        let mut s_bob = bob_state.service.lock().unwrap();
        let contact_alice = FrontendContact {
            did: alice_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_a_url.clone(),
                read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                slot_id: Some(
                    humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap(),
                ),
                drop_pub: DROP_PUB_A.to_string(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }],
        };
        bob_state
            .save_contact(&mut s_bob, contact_alice, Some(password))
            .unwrap();
    }

    // Alice creates and deposits payment
    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(
        "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
        standard_toml.clone(),
    );

    {
        let mut s = alice_state.service.lock().unwrap();
        s.create_new_voucher(
            &standard_toml,
            human_money_core::NewVoucherData {
                creator_profile: human_money_core::models::profile::PublicProfile {
                    id: Some(alice_did.clone()),
                    ..Default::default()
                },
                nominal_value: human_money_core::models::voucher::ValueDefinition {
                    amount: "50".to_string(),
                    unit: "Taler".to_string(),
                    ..Default::default()
                },
                validity_duration: Some("P1Y".to_string()),
                non_redeemable_test_voucher: false,
                collateral: None,
                layer2_anchor: None,
            },
            Some(password),
        )
        .unwrap();
    }

    let local_id = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone()
    };

    let transfer_req = human_money_core::wallet::types::MultiTransferRequest {
        recipient_id: bob_did.clone(),
        sources: vec![human_money_core::wallet::types::SourceTransfer {
            local_instance_id: local_id.clone(),
            amount_to_send: "10".to_string(),
        }],
        notes: Some("Asymmetry test".to_string()),
        sender_profile_name: Some("Alice".to_string()),
        use_privacy_mode: Some(true),
        wot_sync_json: None,
        contact_card_json: None,
    };

    let bundle_res = {
        let mut s = alice_state.service.lock().unwrap();
        s.create_transfer_bundle(transfer_req, &std_map, None, Some(password))
            .unwrap()
    };

    let record = TransactionRecord {
        id: bundle_res.bundle_id.clone(),
        direction: "sent".to_string(),
        recipient_id: bob_did.clone(),
        sender_id: alice_did.clone(),
        timestamp: Utc::now().to_rfc3339(),
        summable_amounts: HashMap::new(),
        countable_items: HashMap::new(),
        involved_vouchers: vec![local_id.clone()],
        involved_sources_details: None,
        bundle_data: bundle_res.bundle_bytes,
        bundle_id: bundle_res.bundle_id.clone(),
        notes: None,
        sender_profile_name: Some("Alice".to_string()),
    };
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .append_to_history(&mut s, record, Some(password))
            .unwrap();
    }

    deposit_transfer_to_post_impl(
        bundle_res.bundle_id,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    // Bob syncs: Bob has 0 write drops, but can read Alice statically and import payment!
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
    let base_dir = temp_test_dir("asymmetry_bob_has_drop");
    let password = "TestPassword123!";

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);

    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    let bob_did = {
        let s = bob_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    // Server for Bob only; Alice has NO drop server configured in settings
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;
    let (server_b_url, bob_drop) =
        setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    // Alice registers as a member on the drop server, but does NOT save it to her settings
    let alice_drop_probe = probe_public_sync_impl(
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
        .push(alice_drop_probe.access_pubkey.clone());

    // Verify Alice has 0 drops in settings
    {
        let mut s = alice_state.service.lock().unwrap();
        let s_settings = alice_state.load_settings(&mut s, Some(password)).unwrap();
        assert_eq!(s_settings.market_selected_syncs.len(), 0);
        assert_eq!(s_settings.public_syncs.len(), 0);
    }

    // Alice has Bob in her address book with Bob's drop
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let contact_bob = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_b_url.clone(),
                read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                slot_id: Some(
                    humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap(),
                ),
                drop_pub: DROP_PUB_B.to_string(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }],
        };
        alice_state
            .save_contact(&mut s_alice, contact_bob, Some(password))
            .unwrap();
    }

    // Bob has Alice in his address book with level 2
    {
        let mut s_bob = bob_state.service.lock().unwrap();
        let contact_alice = FrontendContact {
            did: alice_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_b_url.clone(),
                read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                slot_id: Some(
                    humoco_market::format_slot_id(&alice_drop_probe.access_pubkey, Some("pc"))
                        .unwrap(),
                ),
                drop_pub: DROP_PUB_B.to_string(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }],
        };
        bob_state
            .save_contact(&mut s_bob, contact_alice, Some(password))
            .unwrap();
    }

    // Alice creates standard and voucher
    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(
        "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
        standard_toml.clone(),
    );

    {
        let mut s = alice_state.service.lock().unwrap();
        s.create_new_voucher(
            &standard_toml,
            human_money_core::NewVoucherData {
                creator_profile: human_money_core::models::profile::PublicProfile {
                    id: Some(alice_did.clone()),
                    ..Default::default()
                },
                nominal_value: human_money_core::models::voucher::ValueDefinition {
                    amount: "50".to_string(),
                    unit: "Taler".to_string(),
                    ..Default::default()
                },
                validity_duration: Some("P1Y".to_string()),
                non_redeemable_test_voucher: false,
                collateral: None,
                layer2_anchor: None,
            },
            Some(password),
        )
        .unwrap();
    }

    let local_id = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone()
    };

    let transfer_req = human_money_core::wallet::types::MultiTransferRequest {
        recipient_id: bob_did.clone(),
        sources: vec![human_money_core::wallet::types::SourceTransfer {
            local_instance_id: local_id.clone(),
            amount_to_send: "10".to_string(),
        }],
        notes: Some("Alice uploads to Bob's drop".to_string()),
        sender_profile_name: Some("Alice".to_string()),
        use_privacy_mode: Some(true),
        wot_sync_json: None,
        contact_card_json: None,
    };

    let bundle_res = {
        let mut s = alice_state.service.lock().unwrap();
        s.create_transfer_bundle(transfer_req, &std_map, None, Some(password))
            .unwrap()
    };

    let record = TransactionRecord {
        id: bundle_res.bundle_id.clone(),
        direction: "sent".to_string(),
        recipient_id: bob_did.clone(),
        sender_id: alice_did.clone(),
        timestamp: Utc::now().to_rfc3339(),
        summable_amounts: HashMap::new(),
        countable_items: HashMap::new(),
        involved_vouchers: vec![local_id.clone()],
        involved_sources_details: None,
        bundle_data: bundle_res.bundle_bytes,
        bundle_id: bundle_res.bundle_id.clone(),
        notes: None,
        sender_profile_name: Some("Alice".to_string()),
    };
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .append_to_history(&mut s, record, Some(password))
            .unwrap();
    }

    // Alice has no drops in settings, but deposit_transfer_to_post_impl successfully uploads to Bob's drop!
    let deposit_res = deposit_transfer_to_post_impl(
        bundle_res.bundle_id,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();
    assert_eq!(
        deposit_res.deposited_to, 1,
        "Must upload to recipient's drop even when sender has no drops"
    );

    // Bob syncs from his own drop and imports payment!
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
    let base_dir = temp_test_dir("multi_device_merge");
    let password = "TestPassword123!";

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    let _alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;

    // Pre-populate server with a slot containing dummy/remote entries and a higher ETag
    let (_my_ed_sk, master_seed) = {
        let s = alice_state.service.lock().unwrap();
        let sk = s
            .with_wallet_and_identity(|_w, id| id.signing_key.clone())
            .unwrap();
        let seed = sk.to_bytes();
        (sk, seed)
    };
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

    // Alice local state has an out-of-date or empty ETag
    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();
    post_state
        .slot_etags_by_write_url
        .insert(server_a_url.clone(), "\"etag_stale\"".to_string());
    save_post_state(&alice_state, &post_state, Some(password)).unwrap();

    // Flush post slots: PUT with stale ETag -> 412 Precondition Failed -> fetch remote -> merge -> retry with "etag_device_2" -> saved!
    let (uploaded, _recipient_uploaded, errors) =
        flush_post_slots_impl(&alice_state, &mut post_state, &[], Some(password)).await;
    assert_eq!(
        uploaded, 1,
        "Must successfully merge and upload on 412: {:?}",
        errors
    );
    assert!(errors.is_empty());
}

#[tokio::test]
async fn test_async_co_signing_voucher_for_signing_to_detached_signature() {
    let base_dir = temp_test_dir("co_signing_test");
    let password = "TestPassword123!";

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);

    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    let bob_did = {
        let s = bob_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
    let (server_b_url, bob_drop) =
        setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let contact_bob = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_b_url.clone(),
                read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
                slot_id: Some(
                    humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap(),
                ),
                drop_pub: DROP_PUB_B.to_string(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }],
        };
        alice_state
            .save_contact(&mut s_alice, contact_bob, Some(password))
            .unwrap();
    }

    {
        let mut s_bob = bob_state.service.lock().unwrap();
        let contact_alice = FrontendContact {
            did: alice_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: server_a_url.clone(),
                read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
                slot_id: Some(
                    humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap(),
                ),
                drop_pub: DROP_PUB_A.to_string(),
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }],
        };
        bob_state
            .save_contact(&mut s_bob, contact_alice, Some(password))
            .unwrap();
    }

    // Alice creates voucher
    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(
        "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
        standard_toml.clone(),
    );
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        s_alice
            .create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        first_name: Some("Alice".to_string()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "100".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    non_redeemable_test_voucher: false,
                    collateral: None,
                    layer2_anchor: None,
                },
                Some(password),
            )
            .unwrap();
    }

    let local_id = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone()
    };

    let signing_req_bytes = {
        let s = alice_state.service.lock().unwrap();
        let config = ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::HashedRouting);
        s.create_signing_request_bundle(&local_id, config).unwrap()
    };

    // Alice sends signing request via post
    send_signing_request_via_post_impl(
        bob_did.clone(),
        signing_req_bytes,
        Some(local_id.clone()),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    // Bob syncs post drops: receives sign request
    let bob_sync =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(bob_sync.new_sign_requests, 1);

    let bob_post = load_post_state(&bob_state, Some(password)).unwrap();
    let sign_req = &bob_post.pending_sign_requests[0];

    // Bob signs and creates detached signature response
    let sig_response_bytes = {
        let mut bob_s = bob_state.service.lock().unwrap();
        let voucher = bob_s
            .open_voucher_signing_request(&sign_req.container_bytes, None)
            .unwrap();
        let config = ContainerConfig::TargetDid(alice_did.clone(), PrivacyMode::HashedRouting);
        bob_s
            .create_detached_signature_response_bundle(
                &voucher,
                "guarantor",
                false,
                config,
                Some(password),
            )
            .unwrap()
    };

    // Bob replies with detached signature via post
    reply_detached_signature_via_post_impl(
        Some(sign_req.msg_id.clone()),
        alice_did.clone(),
        sig_response_bytes,
        Some(password.to_string()),
        &bob_state,
    )
    .await
    .unwrap();

    // Alice syncs post drops: detached signature is automatically attached!
    let alice_sync = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();
    assert_eq!(alice_sync.imported_signatures, 1);

    let updated_voucher = {
        let s = alice_state.service.lock().unwrap();
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
async fn test_async_wot_handshake_and_1_step_marketplace_inquiry_cycle() {
    let base_dir = temp_test_dir("wot_and_market_test");
    let password = "TestPassword123!";

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);

    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    let bob_did = {
        let s = bob_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
    let (server_b_url, bob_drop) =
        setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    let alice_probe_b = probe_public_sync_impl(
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
        .push(alice_probe_b.access_pubkey.clone());
    let bob_probe_a =
        probe_public_sync_impl(server_a_url.clone(), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    server_a
        .members
        .lock()
        .unwrap()
        .push(bob_probe_a.access_pubkey.clone());

    let alice_syncs = vec![FrontendSyncDescriptor {
        write_url: server_a_url.clone(),
        read_url: format!("{}/{}", server_a.base_url, server_a.data_dir),
        slot_id: Some(
            humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc")).unwrap(),
        ),
        drop_pub: DROP_PUB_A.to_string(),
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "active".to_string(),
        last_probed_at: None,
        ..Default::default()
    }];

    let bob_syncs = vec![FrontendSyncDescriptor {
        write_url: server_b_url.clone(),
        read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
        slot_id: Some(humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc")).unwrap()),
        drop_pub: DROP_PUB_B.to_string(),
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "active".to_string(),
        last_probed_at: None,
        ..Default::default()
    }];

    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let contact_bob = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: bob_syncs.clone(),
        };
        alice_state
            .save_contact(&mut s_alice, contact_bob, Some(password))
            .unwrap();
    }

    {
        let mut s_bob = bob_state.service.lock().unwrap();
        let contact_alice = FrontendContact {
            did: alice_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: alice_syncs.clone(),
        };
        bob_state
            .save_contact(&mut s_bob, contact_alice, Some(password))
            .unwrap();
    }

    // Part A: WoT Handshake
    send_wot_handshake_via_post_impl(
        bob_did.clone(),
        1,
        true,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    // Bob syncs post drops: since Bob already has Alice in contacts as trust_level 1, it is SILENTLY auto-accepted! (Zero-Click)
    let bob_sync_1 =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(
        bob_sync_1.new_wot_handshakes, 0,
        "Known trusted contacts must be silently auto-accepted without pending handshake banner"
    );

    // Verify Bob's WoT register now contains Alice's list
    {
        let mut s_bob = bob_state.service.lock().unwrap();
        let reg = bob_state
            .get_or_load_wot_register(&mut s_bob, Some(password))
            .unwrap();
        assert!(
            reg.raw_list_store.contains_key(&alice_did)
                || reg
                    .raw_list_store
                    .contains_key(&crate::state::canonicalize_did(&alice_did).unwrap())
        );
    }

    // Alice already has Bob rated as trust_level 1, so Bob's auto-replied handshake is also silently auto-accepted!
    let alice_sync_1 =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_state)
            .await
            .unwrap();
    assert_eq!(alice_sync_1.new_wot_handshakes, 0);

    // Verify Alice's WoT register now contains Bob's list
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let reg = alice_state
            .get_or_load_wot_register(&mut s_alice, Some(password))
            .unwrap();
        assert!(
            reg.raw_list_store.contains_key(&bob_did)
                || reg
                    .raw_list_store
                    .contains_key(&crate::state::canonicalize_did(&bob_did).unwrap())
        );
    }

    // Part B: 1-Step Marketplace Inquiry
    // Bob creates and saves profile with allow_inquiries: true and adds an ad
    let save_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        visibility: Default::default(),
        skills: vec![humoco_market::Skill {
            title: "Honey".to_string(),
            text: None,
            pricing: None,
            is_remote: false,
        }],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };
    save_my_market_profile_impl(save_req, Some(password.to_string()), &bob_state).unwrap();
    set_card_published_impl(true, false, Some(password.to_string()), &bob_state)
        .await
        .unwrap();

    let ad_input = FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Offer,
        title: "Forest Honey".to_string(),
        text: Some("500g Jar".to_string()),
        pricing: Some("10 Minuto".to_string()),
        duration_days: 30,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
    };
    let updated_bob_profile =
        upsert_ad_impl(ad_input, Some(password.to_string()), &bob_state).unwrap();
    let ad_id = updated_bob_profile.ads[0].id.clone();

    // Alice sends inquiry
    let inq_item = send_market_inquiry_via_post_impl(
        bob_did.clone(),
        InquiryTarget::Ad {
            ad_id,
            ad_title: "Forest Honey".to_string(),
        },
        "Do you have 2 jars available?".to_string(),
        bob_syncs.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    // Bob syncs: receives inquiry
    let bob_sync_2 =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(bob_sync_2.new_inquiries, 1);

    let bob_post_2 = load_post_state(&bob_state, Some(password)).unwrap();
    let seller_inq = &bob_post_2.inquiries[0];
    assert_eq!(seller_inq.role, "seller");
    assert!(seller_inq.unread);

    // Bob replies: confirm + payment link
    let rep_res = send_market_inquiry_message_via_post_impl(
        seller_inq.inquiry_id.clone(),
        Some("Yes, 2 jars reserved!".to_string()),
        Some("confirm".to_string()),
        Some("humoco://pay/123".to_string()),
        Some("20 Minuto".to_string()),
        None,
        Some(password.to_string()),
        &bob_state,
    )
    .await
    .unwrap();
    assert_eq!(rep_res.status, "confirmed");
    assert_eq!(rep_res.messages.len(), 2);

    // Second decision change after already decided is rejected, but follow-up text chat works
    let second_decision = send_market_inquiry_message_via_post_impl(
        seller_inq.inquiry_id.clone(),
        None,
        Some("decline".to_string()),
        None,
        None,
        None,
        Some(password.to_string()),
        &bob_state,
    )
    .await;
    assert!(
        second_decision.is_err(),
        "Second decision change must be rejected"
    );

    // Alice syncs: receives reply
    let alice_sync_2 =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_state)
            .await
            .unwrap();
    assert_eq!(alice_sync_2.new_inquiry_messages, 1);

    let alice_post_2 = load_post_state(&alice_state, Some(password)).unwrap();
    assert_eq!(alice_post_2.inquiries[0].status, "confirmed");
    assert_eq!(alice_post_2.inquiries[0].messages.len(), 2);
    assert!(alice_post_2.inquiries[0].unread);

    // Mark inquiry read works
    mark_inquiry_read_impl(
        inq_item.inquiry_id.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    let alice_post_read = load_post_state(&alice_state, Some(password)).unwrap();
    assert!(!alice_post_read.inquiries[0].unread);

    // Alice sends a follow-up chat question to Bob
    let follow_up = send_market_inquiry_message_via_post_impl(
        inq_item.inquiry_id.clone(),
        Some("Can I pick them up at 5pm?".to_string()),
        None,
        None,
        None,
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();
    assert_eq!(follow_up.messages.len(), 3);

    // Bob syncs and receives Alice's follow-up message
    let bob_sync_3 =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(bob_sync_3.new_inquiry_messages, 1);
    let bob_post_3 = load_post_state(&bob_state, Some(password)).unwrap();
    assert_eq!(bob_post_3.inquiries[0].messages.len(), 3);

    // Dismiss pending post item works
    dismiss_pending_post_item_impl(
        "inquiry".to_string(),
        inq_item.inquiry_id.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    let alice_post_3 = load_post_state(&alice_state, Some(password)).unwrap();
    assert!(alice_post_3.inquiries.is_empty());
}

#[tokio::test]
async fn test_own_subaccount_post_compartment_and_sibling_isolation() {
    let base_dir = temp_test_dir("subaccount_post_isolation");
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

    let root_state = create_sub(&base_dir.join("root"), "Alice Root", None);
    let laden_state = create_sub(&base_dir.join("laden"), "Alice Laden", Some("laden"));
    let privat_state = create_sub(&base_dir.join("privat"), "Alice Privat", Some("privat"));

    let root_did = root_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let laden_did = laden_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let privat_did = privat_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let server = MockSyncServer::start("d-shared".to_string(), ADMIN_A.to_string()).await;
    let (_url_r, _drop_r) = setup_test_sync(&server, DROP_PUB_A, password, &root_state).await;
    let (server_url, laden_drop) =
        setup_test_sync(&server, DROP_PUB_A, password, &laden_state).await;
    let (_url_p, _drop_p) = setup_test_sync(&server, DROP_PUB_A, password, &privat_state).await;

    // Root saves laden subaccount as contact (no trust level), and laden + privat save Root so post_accept_from_contacts accepts Root
    let make_contact = |did: String, name: &str| FrontendContact {
        did,
        profile: FrontendUserProfile {
            first_name: Some(name.to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: None,
        desired_hops: None,
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![FrontendSyncDescriptor {
            write_url: server_url.clone(),
            read_url: format!("{}/{}", server.base_url, server.data_dir),
            slot_id: None,
            drop_pub: DROP_PUB_A.to_string(),
            status: "active".to_string(),
            ..Default::default()
        }],
    };
    {
        let mut s = root_state.service.lock().unwrap();
        root_state
            .save_contact(
                &mut s,
                make_contact(laden_did.clone(), "Laden"),
                Some(password),
            )
            .unwrap();
    }
    {
        let mut s = laden_state.service.lock().unwrap();
        laden_state
            .save_contact(
                &mut s,
                make_contact(root_did.clone(), "Root"),
                Some(password),
            )
            .unwrap();
    }
    {
        let mut s = privat_state.service.lock().unwrap();
        privat_state
            .save_contact(
                &mut s,
                make_contact(root_did.clone(), "Root"),
                Some(password),
            )
            .unwrap();
    }

    // Root creates voucher and sends to Laden subaccount
    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(
        "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
        standard_toml.clone(),
    );

    {
        let mut s = root_state.service.lock().unwrap();
        s.create_new_voucher(
            &standard_toml,
            human_money_core::NewVoucherData {
                creator_profile: human_money_core::models::profile::PublicProfile {
                    id: Some(root_did.clone()),
                    first_name: Some("Root".to_string()),
                    ..Default::default()
                },
                nominal_value: human_money_core::models::voucher::ValueDefinition {
                    amount: "50".to_string(),
                    unit: "Taler".to_string(),
                    ..Default::default()
                },
                validity_duration: Some("P1Y".to_string()),
                non_redeemable_test_voucher: false,
                collateral: None,
                layer2_anchor: None,
            },
            Some(password),
        )
        .unwrap();
    }
    let local_id = {
        let s = root_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone()
    };
    let bundle_res = {
        let mut s = root_state.service.lock().unwrap();
        s.create_transfer_bundle(
            human_money_core::wallet::types::MultiTransferRequest {
                recipient_id: laden_did.clone(),
                sources: vec![human_money_core::wallet::types::SourceTransfer {
                    local_instance_id: local_id.clone(),
                    amount_to_send: "15".to_string(),
                }],
                notes: Some("To shop subaccount".to_string()),
                sender_profile_name: Some("Root".to_string()),
                use_privacy_mode: Some(true),
                wot_sync_json: None,
                contact_card_json: None,
            },
            &std_map,
            None,
            Some(password),
        )
        .unwrap()
    };
    {
        let mut s = root_state.service.lock().unwrap();
        root_state
            .append_to_history(
                &mut s,
                TransactionRecord {
                    id: bundle_res.bundle_id.clone(),
                    direction: "sent".to_string(),
                    recipient_id: laden_did.clone(),
                    sender_id: root_did.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    summable_amounts: HashMap::new(),
                    countable_items: HashMap::new(),
                    involved_vouchers: vec![local_id],
                    involved_sources_details: None,
                    bundle_data: bundle_res.bundle_bytes,
                    bundle_id: bundle_res.bundle_id.clone(),
                    notes: None,
                    sender_profile_name: Some("Root".to_string()),
                },
                Some(password),
            )
            .unwrap();
    }

    let dep = deposit_transfer_to_post_impl(
        bundle_res.bundle_id.clone(),
        Some(password.to_string()),
        &root_state,
    )
    .await
    .unwrap();
    assert_eq!(dep.deposited_to, 1);

    // 1. Sibling subaccount ("privat") polls: must NOT fail with NotAnIntendedRecipient or import Laden's payment
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
    assert!(
        privat_sync.errors.is_empty(),
        "Sibling subaccount poll must stay error-free, got {:?}",
        privat_sync.errors
    );

    // 2. Target subaccount ("laden") polls: receives payment cleanly and writes receipt to its own prefixed compartment
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
    assert!(
        laden_sync.errors.is_empty(),
        "Target subaccount poll must succeed without errors, got {:?}",
        laden_sync.errors
    );

    // Flush laden's receipt and verify its prefixed compartment key exists on server
    let _ = sync_post_channels_impl(
        None,
        None,
        Some(&std_map),
        Some(password.to_string()),
        &laden_state,
    )
    .await
    .unwrap();
    let expected_laden_slot = format!("{}_laden.post", laden_drop.access_pubkey);
    assert!(
        server
            .slots
            .lock()
            .unwrap()
            .contains_key(&expected_laden_slot),
        "Subaccount must store its Post slot in dedicated prefixed compartment '{}'",
        expected_laden_slot
    );
    let _ = privat_did;
    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_deposit_transfer_to_post_persists_outbox_when_sync_server_unreachable_and_sync_survives_outage(
) {
    let base_dir = temp_test_dir("post_sync_outage_test");
    let password = "TestPassword123!";

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

    let alice_did = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let bob_did = bob_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    // Bind and drop a loopback listener to obtain a guaranteed unreachable port
    let closed_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed_port = closed_listener.local_addr().unwrap().port();
    drop(closed_listener);
    let dead_write_url = format!("http://127.0.0.1:{}/sync.php", closed_port);
    let dead_read_url = format!("http://127.0.0.1:{}/data", closed_port);

    // Configure Bob's dead sync target in Alice's address book
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let contact_bob = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: dead_write_url.clone(),
                read_url: dead_read_url.clone(),
                slot_id: Some(
                    humoco_market::format_slot_id(
                        "1111111111111111111111111111111111111111111111111111111111111111",
                        Some("pc"),
                    )
                    .unwrap(),
                ),
                drop_pub: DROP_PUB_B.to_string(),
                status: "active".to_string(),
                ..Default::default()
            }],
        };
        alice_state
            .save_contact(&mut s_alice, contact_bob, Some(password))
            .unwrap();
    }

    // Configure Bob's own public sync to the dead port as well
    {
        let mut s_bob = bob_state.service.lock().unwrap();
        let mut bob_settings = bob_state
            .load_settings(&mut s_bob, Some(password))
            .unwrap_or_default();
        bob_settings
            .public_syncs
            .push(crate::settings::StoredPublicSync {
                write_url: dead_write_url,
                read_url: dead_read_url,
                drop_pub: DROP_PUB_B.to_string(),
                access_pubkey: "1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
                enabled: true,
                authorized_channels: vec!["pc".to_string()],
                ..Default::default()
            });
        bob_state
            .save_settings(&mut s_bob, bob_settings, Some(password))
            .unwrap();
    }

    // Alice creates a voucher and transfer bundle
    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(
        "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
        standard_toml.clone(),
    );

    {
        let mut s_alice = alice_state.service.lock().unwrap();
        alice_state
            .save_settings(
                &mut s_alice,
                crate::settings::AppSettings::default(),
                Some(password),
            )
            .unwrap();
        s_alice
            .create_new_voucher(
                &standard_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "100".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();
    }

    let local_id = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone()
    };

    let bundle_res = crate::services::transfer_service::create_transfer_bundle_impl(
        None,
        bob_did.clone(),
        vec![crate::models::FrontendSourceTransfer {
            local_instance_id: local_id,
            amount_to_send: "25".to_string(),
        }],
        Some("Sync outage test".to_string()),
        Some("Alice".to_string()),
        std_map.clone(),
        Some(false),
        Some(false),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect("L1 transfer creation must succeed even when Sync and L2 are down");

    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let record = crate::models::TransactionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            direction: "sent".to_string(),
            recipient_id: bob_did,
            sender_id: alice_did,
            timestamp: Utc::now().to_rfc3339(),
            summable_amounts: HashMap::from([("Taler".to_string(), "25".to_string())]),
            countable_items: HashMap::new(),
            involved_vouchers: bundle_res
                .involved_sources_details
                .iter()
                .map(|d| d.local_instance_id.clone())
                .collect(),
            involved_sources_details: None,
            bundle_data: bundle_res.bundle_data.clone(),
            bundle_id: bundle_res.bundle_id.clone(),
            notes: None,
            sender_profile_name: Some("Alice".to_string()),
        };
        alice_state
            .append_to_history(&mut s_alice, record, Some(password))
            .unwrap();
    }

    // Deposit against dead sync server returns degraded result, and outbox entry is persisted on disk
    let dep = deposit_transfer_to_post_impl(
        bundle_res.bundle_id.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect("deposit_transfer_to_post_impl must degrade gracefully without Err");
    assert_eq!(dep.deposited_to, 0);
    assert_eq!(dep.recipient_deposited_to, 0);
    assert!(!dep.errors.is_empty());

    let persisted_post_state = load_post_state(&alice_state, Some(password)).unwrap();
    assert_eq!(persisted_post_state.outbox_entries.len(), 1);
    assert_eq!(
        persisted_post_state.outbox_entries[0].bundle_id.as_deref(),
        Some(bundle_res.bundle_id.as_str())
    );

    // Bob polling dead sync server returns cleanly with error count, and manual offline receive succeeds
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
        &bob_state,
    )
    .expect("Manual offline receive_bundle_impl must succeed while sync is down");

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_unreachable_recipient_returns_error_and_locked_dispatch_defers_msg_id() {
    let base_dir = temp_test_dir("post_recipient_unreachable_and_locked_dispatch");
    let password = "TestPassword123!";

    let alice_state = create_test_state(setup_test_app(
        &base_dir.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));
    let bob_svc = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
    let (bob_did, my_identity) = bob_svc
        .with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.clone()))
        .unwrap();

    // 1. Alice gives Bob NO reachable syncs -> send_market_inquiry_via_post_impl must fail loudly with error.post.recipientUnreachable
    let server = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let _ = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;

    let inq_err = send_market_inquiry_via_post_impl(
        bob_did.clone(),
        InquiryTarget::Person { skill_title: None },
        "Hallo Bob".to_string(),
        vec![],
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect_err("Sending market inquiry with zero reachable recipient syncs must return Err");
    assert_eq!(inq_err.code, "error.post.recipientUnreachable");

    // 2. Dispatching a DetachedSignature while wallet is locked must return false so msg_id is NOT marked processed
    let locked_dir = base_dir.join("locked");
    std::fs::create_dir_all(&locked_dir).unwrap();
    let locked_svc = human_money_core::app_service::AppService::new(&locked_dir).unwrap();
    let locked_state = create_test_state(locked_svc);
    let my_ed_sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let envelope = PostEnvelope::new("msg-locked-retry-1", 1_760_000_000, vec![], vec![1, 2, 3]);
    let tag16 = [0u8; 16];
    let ctx = super::handlers::PostEnvelopeContext {
        state: &locked_state,
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
    let mut stats = super::handlers::PostDispatchStats::default();
    let may_mark_processed = super::handlers::dispatch_post_payload(
        &ctx,
        &PayloadType::DetachedSignature,
        &mut post_state,
        &mut stats,
    );
    assert!(
        !may_mark_processed,
        "Locked wallet must defer DetachedSignature processing instead of marking msg_id processed"
    );
    assert!(!stats.errors.is_empty());

    let _ = std::fs::remove_dir_all(&base_dir);
}
