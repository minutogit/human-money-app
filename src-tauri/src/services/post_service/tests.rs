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

    // Save transaction record + canonical outgoing bundle bytes
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
        bundle_data: Vec::new(),
        bundle_id: bundle_id.clone(),
        notes: None,
        sender_profile_name: Some("Alice".to_string()),
    };
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .record_outgoing_transactions_with_batch(
                &mut s,
                vec![(record, bundle_bytes.clone())],
                Some(password),
            )
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
        bundle_data: Vec::new(),
        bundle_id: bundle_res.bundle_id.clone(),
        notes: None,
        sender_profile_name: Some("Alice".to_string()),
    };
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .record_outgoing_transactions_with_batch(
                &mut s,
                vec![(record, bundle_res.bundle_bytes.clone())],
                Some(password),
            )
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

    // Verify Alice has 0 drops in security_policy
    {
        let mut s = alice_state.service.lock().unwrap();
        let s_policy = alice_state
            .load_security_policy(&mut s, Some(password))
            .unwrap();
        assert_eq!(s_policy.market_selected_syncs.len(), 0);
        assert_eq!(s_policy.public_syncs.len(), 0);
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
        bundle_data: Vec::new(),
        bundle_id: bundle_res.bundle_id.clone(),
        notes: None,
        sender_profile_name: Some("Alice".to_string()),
    };
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .record_outgoing_transactions_with_batch(
                &mut s,
                vec![(record, bundle_res.bundle_bytes.clone())],
                Some(password),
            )
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
        s.create_signing_request_bundle(&local_id, config, None, Some(password))
            .unwrap()
    };

    // Alice sends signing request via post
    send_signing_request_via_post_impl(
        bob_did.clone(),
        signing_req_bytes,
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
        let req_bundle = bob_s
            .open_voucher_signing_request(&sign_req.container_bytes, None)
            .unwrap();
        let config = ContainerConfig::TargetDid(alice_did.clone(), PrivacyMode::HashedRouting);
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
    assert_eq!(
        bob_sync_1.imported_wot_lists, 1,
        "Silent auto-accept of a trusted contact's WoT sync package must increment imported_wot_lists"
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
    assert_eq!(alice_sync_1.imported_wot_lists, 1);

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
    set_card_published_impl(true, Some(password.to_string()), &bob_state)
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
        event_at: None,
        event_all_day: false,
        published: Some(true),
    };
    let updated_bob_profile =
        upsert_ad_impl(ad_input, Some(password.to_string()), &bob_state).unwrap();
    let ad_id = updated_bob_profile.ads[0].id.clone();

    // Alice sends inquiry
    let inq_item = send_market_inquiry_via_post_impl(
        bob_did.clone(),
        FrontendInquiryTarget::Ad {
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
        Some(format!("humoco://pay?to={}", bob_did)),
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
    assert!(
        alice_sync_2.errors.is_empty(),
        "unexpected inquiry sync errors: {:?}",
        alice_sync_2.errors
    );

    let alice_post_2 = load_post_state(&alice_state, Some(password)).unwrap();
    assert_eq!(alice_post_2.inquiries[0].status, "confirmed");
    assert_eq!(alice_post_2.inquiries[0].messages.len(), 2);
    assert!(alice_post_2.inquiries[0].unread);
    assert!(alice_post_2.inquiries[0].payment_uri.is_some());

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
        let record = TransactionRecord {
            id: bundle_res.bundle_id.clone(),
            direction: "sent".to_string(),
            recipient_id: laden_did.clone(),
            sender_id: root_did.clone(),
            timestamp: Utc::now().to_rfc3339(),
            summable_amounts: HashMap::new(),
            countable_items: HashMap::new(),
            involved_vouchers: vec![local_id],
            involved_sources_details: None,
            bundle_data: Vec::new(),
            bundle_id: bundle_res.bundle_id.clone(),
            notes: None,
            sender_profile_name: Some("Root".to_string()),
        };
        root_state
            .record_outgoing_transactions_with_batch(
                &mut s,
                vec![(record, bundle_res.bundle_bytes.clone())],
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
        bob_state
            .mutate_security_policy(&mut s_bob, Some(password), |policy| {
                policy.public_syncs.push(crate::settings::StoredPublicSync {
                    write_url: dead_write_url,
                    read_url: dead_read_url,
                    drop_pub: DROP_PUB_B.to_string(),
                    access_pubkey:
                        "1111111111111111111111111111111111111111111111111111111111111111"
                            .to_string(),
                    enabled: true,
                    authorized_channels: vec!["post".to_string()],
                    ..Default::default()
                });
            })
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
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .expect("L1 transfer creation must succeed even when Sync and L2 are down");
    let _ = alice_did;

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
        Some(crate::models::ReceiveCommitMode::ForceTrustOffline),
        None,
        &bob_state,
    )
    .await
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
        FrontendInquiryTarget::Person { skill_title: None },
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
    let mut baseline_post_state = post_state.clone();
    let mut stats = super::handlers::PostDispatchStats::default();
    let outcome = super::handlers::dispatch_post_payload(
        &ctx,
        &PayloadType::DetachedSignature,
        &mut post_state,
        &mut baseline_post_state,
        &mut stats,
    )
    .await;
    assert_eq!(
        outcome,
        super::handlers::PostDispatchOutcome::RetryLater,
        "Locked wallet must defer DetachedSignature processing instead of marking msg_id processed"
    );
    assert!(!stats.errors.is_empty());

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_merge_and_save_post_state_preserves_concurrent_outbox_and_auto_imports_payment() {
    let base_dir = temp_test_dir("post_concurrency_and_auto_import");
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

    let bob_did = bob_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    // Finding 2: merge_and_save_post_state preserves concurrent outbox entries written while poll ran
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
        let dummy_box = humoco_market::PostBoxEntry {
            msg_nonce: [8u8; 24],
            tag16: [9u8; 16],
            ciphertext: vec![1, 2, 3, 4],
        };
        concurrent_state
            .outbox_entries
            .push(crate::state::PostOutboxEntry {
                msg_id: "concurrent-outbox-1".to_string(),
                recipient_did: bob_did.clone(),
                tag16_hex: "09090909090909090909090909090909".to_string(),
                bundle_id: None,
                message_class: crate::state::OutboxMessageClass::Control,
                entry: dummy_box,
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
    save_post_state(&alice_state, &StoredPostState::default(), Some(password)).unwrap();

    // Syncing PoST auto-imports incoming payment directly into Bob's wallet
    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;
    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
    let (server_b_url, bob_drop) =
        setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let alice_did = s_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        alice_state
            .save_contact(
                &mut s_alice,
                FrontendContact {
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
                        read_url: bob_drop.read_url.clone(),
                        write_url: server_b_url.clone(),
                        slot_id: Some(
                            humoco_market::format_slot_id(&bob_drop.access_pubkey, Some("pc"))
                                .unwrap(),
                        ),
                        drop_pub: DROP_PUB_B.to_string(),
                        status: "active".to_string(),
                        fail_count: 0,
                        ..Default::default()
                    }],
                },
                Some(password),
            )
            .unwrap();

        let mut s_bob = bob_state.service.lock().unwrap();
        bob_state
            .save_contact(
                &mut s_bob,
                FrontendContact {
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
                        write_url: server_a_url,
                        read_url: alice_drop.read_url,
                        slot_id: Some(
                            humoco_market::format_slot_id(&alice_drop.access_pubkey, Some("pc"))
                                .unwrap(),
                        ),
                        drop_pub: DROP_PUB_A.to_string(),
                        status: "active".to_string(),
                        fail_count: 0,
                        ..Default::default()
                    }],
                },
                Some(password),
            )
            .unwrap();
        drop(s_bob);

        let std_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        s_alice
            .create_new_voucher(
                &std_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "50".to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();
    }

    let alice_voucher_id = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap()[0]
            .local_instance_id
            .clone()
    };

    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let mut std_map = HashMap::new();
    std_map.insert(
        "a1b2c3d4-e5f6-4789-8012-3456789abcde".to_string(),
        standard_toml,
    );

    let bundle_res = crate::services::transfer_service::create_transfer_bundle_impl(
        None,
        bob_did.clone(),
        vec![crate::models::FrontendSourceTransfer {
            local_instance_id: alice_voucher_id,
            amount_to_send: "20".to_string(),
        }],
        Some("Ladeneinkauf".to_string()),
        Some("Alice".to_string()),
        std_map.clone(),
        Some(false),
        Some(false),
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    let dep_res = deposit_transfer_to_post_impl(
        bundle_res.bundle_id,
        Some(password.to_string()),
        &alice_state,
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
        &bob_state,
    )
    .await
    .unwrap();
    assert_eq!(bob_sync.imported_payments, 1, "bob_sync: {:?}", bob_sync);
    {
        let s_bob = bob_state.service.lock().unwrap();
        let bob_vouchers = s_bob
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap();
        assert_eq!(bob_vouchers.len(), 1);
    }

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_reply_detached_signature_via_post_requires_full_access() {
    let base_dir = temp_test_dir("post_detached_sig_full_only");
    let pw = "TestPassword123!";
    let alice_svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, pw);
    let bob_svc = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, pw);
    let alice_state = create_test_state(alice_svc);
    let bob_state = create_test_state(bob_svc);

    let alice_did = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

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

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_outbox_windowed_packing_defers_overflow_past_max_bytes_and_drains_next_flush() {
    let base_dir = temp_test_dir("post_outbox_windowed_drain");
    let password = "TestPassword123!";
    let alice_svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_svc = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
    let alice_state = create_test_state(alice_svc);
    let bob_state = create_test_state(bob_svc);

    let (bob_did, bob_pk) = {
        let s = bob_state.service.lock().unwrap();
        s.with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.public_key))
            .unwrap()
    };

    let server = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (_write_url, _saved) = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;

    let signing_ctx = load_post_signing_context(&alice_state, Some(password)).unwrap();
    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();

    // Two 256 KB tier boxes + one 4 KB tier box queued FIFO: raw sum > DROP_POST_MAX_BYTES (512 KB).
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &bob_did,
        &bob_pk,
        "msg-large-1".to_string(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        vec![1u8; 40_000],
        1_700_000_010,
    )
    .unwrap();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &bob_did,
        &bob_pk,
        "msg-large-2".to_string(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        vec![2u8; 40_000],
        1_700_000_020,
    )
    .unwrap();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &bob_did,
        &bob_pk,
        "msg-small-3".to_string(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        vec![3u8; 120],
        1_700_000_030,
    )
    .unwrap();

    let tag1 = post_state.outbox_entries[0].entry.tag16;
    let tag2 = post_state.outbox_entries[1].entry.tag16;
    let tag3 = post_state.outbox_entries[2].entry.tag16;

    // 1st flush: packs msg-large-1 + msg-small-3 within 512 KB, defers msg-large-2 cleanly.
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
    assert_eq!(post_state.outbox_entries.len(), 3);

    {
        let slots = server.slots.lock().unwrap();
        let stored_blob = slots.values().next().expect("slot uploaded");
        assert!(stored_blob.len() <= DROP_POST_MAX_BYTES);
        let decoded = humoco_market::decode_post_slot(stored_blob).unwrap();
        let decoded_tags: std::collections::HashSet<[u8; 16]> =
            decoded.into_iter().map(|e| e.tag16).collect();
        assert!(decoded_tags.contains(&tag1));
        assert!(decoded_tags.contains(&tag3));
        assert!(!decoded_tags.contains(&tag2));
    }

    // Simulate delivery-receipt pruning for msg-large-1; flush again -> msg-large-2 drains now!
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

    {
        let slots = server.slots.lock().unwrap();
        let stored_blob = slots.values().next().expect("slot uploaded");
        assert!(stored_blob.len() <= DROP_POST_MAX_BYTES);
        let decoded = humoco_market::decode_post_slot(stored_blob).unwrap();
        let decoded_tags: std::collections::HashSet<[u8; 16]> =
            decoded.into_iter().map(|e| e.tag16).collect();
        assert!(decoded_tags.contains(&tag2));
        assert!(decoded_tags.contains(&tag3));
    }

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_sync_post_channels_respects_wifi_only_skip() {
    let base_dir = temp_test_dir("sync_post_wifi_only_skip");
    let password = "TestPassword123!";
    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

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

    let manual_res =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &alice_state)
            .await
            .unwrap();
    assert!(!manual_res.skipped_wifi_only);

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_post_prefix_routing_reply_drops_and_ack_isolation() {
    let base_dir = temp_test_dir("post_prefix_routing");
    let password = "TestPassword123!";

    // Alice has TWO separate prefix wallets on the same seed/key: `pc` (Server A1) and `privat` (Server A2)
    let alice_laden_svc = setup_test_app(
        &base_dir.join("alice_laden"),
        "Alice Laden",
        ALICE_PHRASE,
        password,
    );
    let mut alice_privat_svc =
        human_money_core::app_service::AppService::new(&base_dir.join("alice_privat")).unwrap();
    alice_privat_svc
        .create_profile(
            "Alice Privat",
            ALICE_PHRASE,
            None,
            Some("privat"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-privat".into(),
            true,
        )
        .unwrap();
    let bob_svc = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

    let alice_laden_state = create_test_state(alice_laden_svc);
    let alice_privat_state = create_test_state(alice_privat_svc);
    let bob_state = create_test_state(bob_svc);

    let alice_laden_did = alice_laden_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let alice_privat_did = alice_privat_state
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
    assert_ne!(alice_laden_did, alice_privat_did);

    // Separate sync servers: A1 for alice_laden, A2 (known) + A3 (new reply_drop) for alice_privat, B for bob
    let server_a1 = MockSyncServer::start("d-a1".to_string(), ADMIN_A.to_string()).await;
    let server_a2 = MockSyncServer::start("d-a2".to_string(), ADMIN_A.to_string()).await;
    let server_a3 = MockSyncServer::start("d-a3".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;

    let (a1_write, a1_drop) =
        setup_test_sync(&server_a1, DROP_PUB_A, password, &alice_laden_state).await;
    let (a2_write, a2_drop) =
        setup_test_sync(&server_a2, DROP_PUB_A, password, &alice_privat_state).await;
    let (a3_write, _a3_drop) =
        setup_test_sync(&server_a3, DROP_PUB_A, password, &alice_privat_state).await;
    let (b_write, b_drop) = setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    let a1_desc = FrontendSyncDescriptor {
        write_url: a1_write.clone(),
        read_url: format!("{}/{}", server_a1.base_url, server_a1.data_dir),
        slot_id: Some(humoco_market::format_slot_id(&a1_drop.access_pubkey, Some("pc")).unwrap()),
        drop_pub: DROP_PUB_A.to_string(),
        status: "active".to_string(),
        ..Default::default()
    };
    let a2_desc = FrontendSyncDescriptor {
        write_url: a2_write.clone(),
        read_url: format!("{}/{}", server_a2.base_url, server_a2.data_dir),
        slot_id: Some(
            humoco_market::format_slot_id(&a2_drop.access_pubkey, Some("privat")).unwrap(),
        ),
        drop_pub: DROP_PUB_A.to_string(),
        status: "active".to_string(),
        ..Default::default()
    };
    let b_desc = FrontendSyncDescriptor {
        write_url: b_write.clone(),
        read_url: format!("{}/{}", server_b.base_url, server_b.data_dir),
        slot_id: Some(humoco_market::format_slot_id(&b_drop.access_pubkey, Some("pc")).unwrap()),
        drop_pub: DROP_PUB_B.to_string(),
        status: "active".to_string(),
        ..Default::default()
    };

    // Authorize peer slot keys on each MockSyncServer (setup_test_sync already enabled public_syncs)
    let alice_priv_probe_b = probe_public_sync_impl(
        b_write.clone(),
        Some(password.to_string()),
        &alice_privat_state,
    )
    .await
    .unwrap();
    server_b
        .members
        .lock()
        .unwrap()
        .push(alice_priv_probe_b.access_pubkey);

    let bob_probe_a1 =
        probe_public_sync_impl(a1_write.clone(), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    server_a1
        .members
        .lock()
        .unwrap()
        .push(bob_probe_a1.access_pubkey.clone());
    let bob_probe_a2 =
        probe_public_sync_impl(a2_write.clone(), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    server_a2
        .members
        .lock()
        .unwrap()
        .push(bob_probe_a2.access_pubkey.clone());
    let bob_probe_a3 =
        probe_public_sync_impl(a3_write.clone(), Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    server_a3
        .members
        .lock()
        .unwrap()
        .push(bob_probe_a3.access_pubkey.clone());

    // Bob adds alice_laden (Sync A1) FIRST and alice_privat (only Sync A2 initially) SECOND
    {
        let mut s = bob_state.service.lock().unwrap();
        let mk_contact =
            |did: String, name: &str, syncs: Vec<FrontendSyncDescriptor>| FrontendContact {
                did,
                profile: FrontendUserProfile {
                    first_name: Some(name.to_string()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(3),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs,
            };
        bob_state
            .save_contact(
                &mut s,
                mk_contact(
                    alice_laden_did.clone(),
                    "Alice Laden",
                    vec![a1_desc.clone()],
                ),
                Some(password),
            )
            .unwrap();
        bob_state
            .save_contact(
                &mut s,
                mk_contact(
                    alice_privat_did.clone(),
                    "Alice Privat",
                    vec![a2_desc.clone()],
                ),
                Some(password),
            )
            .unwrap();
    }
    // alice_privat adds Bob with Sync B
    {
        let mut s = alice_privat_state.service.lock().unwrap();
        alice_privat_state
            .save_contact(
                &mut s,
                FrontendContact {
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
                    syncs: vec![b_desc.clone()],
                },
                Some(password),
            )
            .unwrap();
    }

    // Bob enables marketplace inquiries so his inbox accepts incoming MarketInquiry messages
    save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        },
        Some(password.to_string()),
        &bob_state,
    )
    .unwrap();
    set_card_published_impl(true, Some(password.to_string()), &bob_state)
        .await
        .unwrap();

    // 1. alice_privat sends a MarketInquiry to Bob (carries reply_drops = [Sync A2, Sync A3])
    let item = send_market_inquiry_via_post_impl(
        bob_did.clone(),
        FrontendInquiryTarget::Person { skill_title: None },
        "Hallo vom Privatkonto".to_string(),
        vec![b_desc.clone()],
        Some(password.to_string()),
        &alice_privat_state,
    )
    .await
    .unwrap();
    assert!(!item.inquiry_id.is_empty());

    // 2. Bob polls Post:
    // - Inquiry counterparty must be alice_privat_did (NOT alice_laden_did!)
    // - Automatic reply_drops update must add Sync A2 ONLY to alice_privat, leaving alice_laden at [Sync A1]
    // - Bob's ACK receipt must be sent ONLY to alice_privat's Sync A2 (not Server A1!)
    let bob_poll =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert!(
        bob_poll.errors.is_empty(),
        "Bob poll errors: {:?}",
        bob_poll.errors
    );
    assert_eq!(bob_poll.new_inquiries, 1);

    let bob_summary = get_post_inbox_summary_impl(Some(password.to_string()), &bob_state).unwrap();
    assert_eq!(bob_summary.inquiries[0].counterparty_did, alice_privat_did);

    let bob_contacts = bob_state
        .load_contacts(&mut bob_state.service.lock().unwrap(), Some(password))
        .unwrap();
    let c_laden = bob_contacts
        .iter()
        .find(|c| c.did == alice_laden_did)
        .unwrap();
    let c_privat = bob_contacts
        .iter()
        .find(|c| c.did == alice_privat_did)
        .unwrap();
    assert_eq!(c_laden.syncs.len(), 1);
    assert_eq!(c_laden.syncs[0].write_url, a1_write);
    assert_eq!(c_privat.syncs.len(), 2);
    assert!(c_privat.syncs.iter().any(|s| s.write_url == a2_write));
    assert!(c_privat.syncs.iter().any(|s| s.write_url == a3_write));

    assert!(
        server_a1.slots.lock().unwrap().is_empty(),
        "ACK receipt for alice_privat must NOT be uploaded to alice_laden's server A1"
    );
    let bob_ack_slot_a2 = format!("{}_pc.post", bob_probe_a2.access_pubkey);
    let bob_ack_slot_a3 = format!("{}_pc.post", bob_probe_a3.access_pubkey);
    assert!(
        server_a2
            .slots
            .lock()
            .unwrap()
            .contains_key(&bob_ack_slot_a2),
        "ACK receipt for alice_privat must be uploaded to server A2"
    );
    assert!(
        server_a3
            .slots
            .lock()
            .unwrap()
            .contains_key(&bob_ack_slot_a3),
        "ACK receipt for alice_privat must also be uploaded to server A3"
    );

    // 3. alice_privat polls Server A2 and consumes the ACK receipt cleanly without NotAnIntendedRecipient
    let alice_priv_poll = sync_post_channels_impl(
        None,
        None,
        None,
        Some(password.to_string()),
        &alice_privat_state,
    )
    .await
    .unwrap();
    assert!(
        alice_priv_poll.errors.is_empty(),
        "alice_privat poll errors: {:?}",
        alice_priv_poll.errors
    );
    assert_eq!(alice_priv_poll.imported_receipts, 1);
    let priv_post_state = load_post_state(&alice_privat_state, Some(password)).unwrap();
    assert!(priv_post_state.outbox_entries.is_empty());

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_h5_outbox_holds_back_value_transfers_under_fork_lock_and_known_offender() {
    use human_money_core::models::conflict::{ConflictRole, ProofOfDoubleSpend, ProofStoreEntry};

    let base_dir = temp_test_dir("post_h5_outflow_preflight");
    let password = "StrongTestPassword123!";
    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
    let bob_state = create_test_state(bob_service);

    let bob_id = {
        let s = bob_state.service.lock().unwrap();
        s.with_wallet_and_identity(|_, id| id.clone()).unwrap()
    };

    let server = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let (_write_url, _saved) = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;

    let signing_ctx = load_post_signing_context(&alice_state, Some(password)).unwrap();
    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();

    // Queue 1 Control message and 1 ValueTransfer message to Bob.
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &bob_id.user_id,
        &bob_id.public_key,
        "msg-ctrl-1".to_string(),
        None,
        crate::state::OutboxMessageClass::Control,
        &signing_ctx.own_reply_syncs,
        vec![0xAA; 64],
        1_700_000_010,
    )
    .unwrap();
    enqueue_post_envelope(
        &mut post_state,
        &signing_ctx.identity.user_id,
        &signing_ctx.sync_ctx.my_ed_sk,
        &bob_id.user_id,
        &bob_id.public_key,
        "msg-value-1".to_string(),
        Some("bundle-h5-1".to_string()),
        crate::state::OutboxMessageClass::ValueTransfer,
        &signing_ctx.own_reply_syncs,
        vec![0xBB; 64],
        1_700_000_020,
    )
    .unwrap();
    save_post_state(&alice_state, &post_state, Some(password)).unwrap();

    // 1. Activate pending_restore_l2_reconciliation lock -> Control flushes, ValueTransfer stays held back.
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .mutate_internal_lifecycle(&mut s, Some(password), |lc| {
                lc.pending_restore_l2_reconciliation = true;
            })
            .unwrap();
    }

    let mut post_state = load_post_state(&alice_state, Some(password)).unwrap();
    let session = alice_state.begin_session_op().unwrap();
    let ctrl_tag = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == "msg-ctrl-1")
        .unwrap()
        .entry
        .tag16;
    let value_tag = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == "msg-value-1")
        .unwrap()
        .entry
        .tag16;

    let outcome1 = crate::services::post_service::outbox::flush_post_slots_impl(
        &alice_state,
        &session,
        &mut post_state,
        None,
        &[],
        Some(password),
    )
    .await;
    assert!(outcome1.uploaded_counts_by_tag.contains_key(&ctrl_tag));
    assert!(!outcome1.uploaded_counts_by_tag.contains_key(&value_tag));

    // Remove flushed Control entry, clear restore lock, then mark Bob as KnownOffender.
    post_state
        .outbox_entries
        .retain(|e| e.msg_id != "msg-ctrl-1");
    save_post_state(&alice_state, &post_state, Some(password)).unwrap();
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .mutate_internal_lifecycle(&mut s, Some(password), |lc| {
                lc.pending_restore_l2_reconciliation = false;
            })
            .unwrap();
        let (wallet, _) = s.get_unlocked_mut_for_test();
        wallet.proof_store.proofs.insert(
            "proof-h5-1".to_string(),
            ProofStoreEntry {
                proof: ProofOfDoubleSpend {
                    proof_id: "proof-h5-1".to_string(),
                    offender_id: bob_id.user_id.clone(),
                    suspected_identity: None,
                    fork_point_prev_hash: "00".repeat(32),
                    conflicting_transactions: Vec::new(),
                    fingerprints: Vec::new(),
                    victim_witnesses: Vec::new(),
                    deletable_at: "2099-01-01T00:00:00Z".to_string(),
                    reporter_id: signing_ctx.identity.user_id.clone(),
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
        assert!(wallet.assert_not_known_offender(&bob_id.user_id).is_err());
    }

    // Re-queue a fresh Control message for Bob -> Control MUST flush while ValueTransfer to KnownOffender stays held!
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
    let ctrl2_tag = post_state
        .outbox_entries
        .iter()
        .find(|e| e.msg_id == "msg-ctrl-2")
        .unwrap()
        .entry
        .tag16;

    let outcome2 = crate::services::post_service::outbox::flush_post_slots_impl(
        &alice_state,
        &session,
        &mut post_state,
        None,
        &[],
        Some(password),
    )
    .await;
    assert!(outcome2.uploaded_counts_by_tag.contains_key(&ctrl2_tag));
    assert!(!outcome2.uploaded_counts_by_tag.contains_key(&value_tag));
    assert!(post_state
        .outbox_entries
        .iter()
        .any(|e| e.msg_id == "msg-value-1"));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn h8_post_tolerance_zone_parks_confirms_and_dismisses() {
    use human_money_core::storage::{AuthMethod, FileStorage};

    let base_dir = temp_test_dir("h8_post_tolerance_zone");
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

    // Put Bob into recovery epoch 1 started 30 minutes IN THE FUTURE relative to bundle timestamp
    // so Alice's transfer hits soft tolerance zone (15m..24h).
    let bob_folder = bob_state.service.lock().unwrap().list_profiles().unwrap()[0]
        .folder_name
        .clone();
    {
        let mut storage = FileStorage::new(base_dir.join("bob").join(&bob_folder));
        let auth = AuthMethod::Password(password);
        let mut seal_rec = storage.load_seal(&auth).unwrap().unwrap();
        seal_rec.seal.payload.epoch = 1;
        seal_rec.seal.payload.epoch_start_time = (Utc::now() + chrono::Duration::minutes(30))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        storage.save_seal(&auth, &seal_rec).unwrap();
    }

    let server_a = MockSyncServer::start("d-alice".to_string(), ADMIN_A.to_string()).await;
    let server_b = MockSyncServer::start("d-bob".to_string(), ADMIN_B.to_string()).await;
    let (server_a_url, alice_drop) =
        setup_test_sync(&server_a, DROP_PUB_A, password, &alice_state).await;
    let (server_b_url, bob_drop) =
        setup_test_sync(&server_b, DROP_PUB_B, password, &bob_state).await;

    let make_desc =
        |url: &str, srv: &MockSyncServer, pub_hex: &str, access_pk: &str| FrontendSyncDescriptor {
            write_url: url.to_string(),
            read_url: format!("{}/{}", srv.base_url, srv.data_dir),
            slot_id: Some(humoco_market::format_slot_id(access_pk, Some("pc")).unwrap()),
            drop_pub: pub_hex.to_string(),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s,
                FrontendContact {
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
                    syncs: vec![make_desc(
                        &server_b_url,
                        &server_b,
                        DROP_PUB_B,
                        &bob_drop.access_pubkey,
                    )],
                },
                Some(password),
            )
            .unwrap();
    }
    {
        let mut s = bob_state.service.lock().unwrap();
        bob_state
            .save_contact(
                &mut s,
                FrontendContact {
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
                    syncs: vec![make_desc(
                        &server_a_url,
                        &server_a,
                        DROP_PUB_A,
                        &alice_drop.access_pubkey,
                    )],
                },
                Some(password),
            )
            .unwrap();
    }

    let std_map = crate::services::common::load_all_standard_tomls(None, None);
    let freetaler_toml = std_map
        .get("a1b2c3d4-e5f6-4789-8012-3456789abcde")
        .expect("embedded freetaler standard missing")
        .clone();

    let (lid1, lid2) = {
        let mut s = alice_state.service.lock().unwrap();
        for amt in ["40", "25"] {
            s.create_new_voucher(
                &freetaler_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(alice_did.clone()),
                        first_name: Some("Alice".to_string()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: amt.to_string(),
                        unit: "Taler".to_string(),
                        ..Default::default()
                    },
                    validity_duration: Some("P1Y".to_string()),
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();
        }
        let vs = s
            .with_wallet_and_identity(|w, id| w.list_vouchers(Some(id), None, None, None))
            .unwrap();
        (
            vs[0].local_instance_id.clone(),
            vs[1].local_instance_id.clone(),
        )
    };

    // Send 2 unanchored bundles -> both deposit over PoST and hit Bob's soft tolerance zone
    for (lid, amt) in [(lid1, "10"), (lid2, "15")] {
        let res = crate::services::transfer_service::create_transfer_bundle_impl(
            None,
            bob_did.clone(),
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
            &alice_state,
        )
        .await
        .unwrap();
        assert!(res.post_deposit.is_some());
    }

    // Bob syncs -> parks 2 pending tolerance payments, imports 0, queues 0 ACK receipts yet
    let sync1 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
        .await
        .unwrap();
    assert_eq!(sync1.new_tolerance_payments, 2);
    assert_eq!(sync1.imported_payments, 0);
    let post_bob_after_sync1 = load_post_state(&bob_state, Some(password)).unwrap();
    assert_eq!(post_bob_after_sync1.pending_tolerance_payments.len(), 2);
    assert!(
        post_bob_after_sync1.outbox_entries.is_empty(),
        "H8: ACK must NOT be queued until user confirms parked tolerance payment"
    );

    // Re-sync must stay idempotent & deduplicated
    let sync2 = sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
        .await
        .unwrap();
    assert_eq!(sync2.new_tolerance_payments, 0);

    let summary = get_post_inbox_summary_impl(Some(password.to_string()), &bob_state).unwrap();
    assert_eq!(summary.pending_tolerance_payments.len(), 2);
    assert_eq!(summary.pending_tolerance_payments[0].zone, "soft");

    let confirm_id = summary.pending_tolerance_payments[0].msg_id.clone();
    let dismiss_id = summary.pending_tolerance_payments[1].msg_id.clone();

    // Confirm #1 -> commits into Bob's wallet, removes pending item, queues ACK receipt
    let payload = confirm_pending_post_payment_impl(
        None,
        confirm_id.clone(),
        Some(password.to_string()),
        &bob_state,
    )
    .await
    .unwrap();
    assert_eq!(payload.newly_committed, Some(true));

    let post_bob_after_confirm = load_post_state(&bob_state, Some(password)).unwrap();
    assert_eq!(post_bob_after_confirm.pending_tolerance_payments.len(), 1);
    assert_eq!(
        post_bob_after_confirm.outbox_entries.len(),
        1,
        "H8: Confirming parked tolerance payment must queue ACK receipt"
    );

    // Dismiss #2 -> removes parked item without committing or queueing ACK
    dismiss_pending_post_item_impl(
        "tolerance_payment".to_string(),
        dismiss_id,
        Some(password.to_string()),
        &bob_state,
    )
    .unwrap();
    let post_bob_final = load_post_state(&bob_state, Some(password)).unwrap();
    assert!(post_bob_final.pending_tolerance_payments.is_empty());

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn test_h4_queue_receipt_is_deterministic_and_idempotent() {
    let dir = temp_test_dir("h4_idempotent_ack");
    let pw = "TestPassword123!";
    let alice_svc = setup_test_app(&dir.join("alice"), "Alice", ALICE_PHRASE, pw);
    let bob_svc = setup_test_app(&dir.join("bob"), "Bob", BOB_PHRASE, pw);
    let (my_id, my_sk) = alice_svc
        .with_wallet_and_identity(|_, id| (id.clone(), id.signing_key.clone()))
        .unwrap();
    let bob_did = bob_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let mut post_state = StoredPostState::default();
    let tag16 = [7u8; 16];
    super::queue_receipt(
        &mut post_state,
        &my_id,
        &my_sk,
        &bob_did,
        "msg-unique-1",
        &tag16,
        Some("bundle-1"),
        &[],
        1_760_000_000,
    );
    super::queue_receipt(
        &mut post_state,
        &my_id,
        &my_sk,
        &bob_did,
        "msg-unique-1",
        &tag16,
        Some("bundle-1"),
        &[],
        1_760_000_010,
    );
    assert_eq!(post_state.outbox_entries.len(), 1);
    assert_eq!(post_state.outbox_entries[0].msg_id, "ack-msg-unique-1");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_h1_h6_claim_bearing_outflow_guard_and_stale_session_rejection() {
    let dir = temp_test_dir("h1_h6_outbox_guards");
    let pw = "TestPassword123!";
    let alice_svc = setup_test_app(&dir.join("alice"), "Alice", ALICE_PHRASE, pw);
    let bob_svc = setup_test_app(&dir.join("bob"), "Bob", BOB_PHRASE, pw);
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

    // H1: stale session token is rejected after logout / generation bump
    crate::commands::auth::logout_impl(&alice_state);
    let stale_err =
        super::load_post_state_for_session(&alice_state, &stale_session, Some(pw)).unwrap_err();
    assert!(
        stale_err.message.contains("session changed"),
        "unexpected H1 err: {:?}",
        stale_err
    );

    // Re-open fresh profile session for H6 check
    let fresh_svc = setup_test_app(&dir.join("alice2"), "Alice2", ALICE_PHRASE, pw);
    let fresh_state = create_test_state(fresh_svc);
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

    // Enqueue ClaimBearing fails immediately while ForkDetected seal lock is active
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

    // Pre-queued ClaimBearing entry MUST be blocked from flush while seal lock is active
    assert!(crate::state::OutboxMessageClass::ClaimBearing.requires_outflow_guard());
    let mut ps = StoredPostState::default();
    super::enqueue_post_envelope(
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
    assert!(
        blocked,
        "H6: ClaimBearing must be blocked when ForkDetected"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn k2_h21_dispatch_rejects_spoofed_trust_assertion_and_mismatched_payment_uri() {
    let base_dir = temp_test_dir("k2_h21_post_guards");
    let pw = "TestPassword123!";
    let alice_svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, pw);
    let alice_state = create_test_state(alice_svc);
    let (my_id, my_ed_sk) = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|_, id| (id.clone(), id.signing_key.clone()))
            .unwrap()
    };
    let bob_did = human_money_core::test_utils::ACTORS.bob.user_id.clone();
    let charlie_did = human_money_core::test_utils::ACTORS.charlie.user_id.clone();
    let alice_did = my_id.user_id.clone();
    let session = alice_state.begin_session_op().unwrap();
    let now_secs = 1_760_000_000u64;

    // 1. K2: TrustAssertion with assertion.sender_did != ctx.sender_did must be rejected
    let assertion = humoco_market::WotHandshakeAssertion {
        sender_did: alice_did.clone(),
        sender_name: Some("Spoofed".into()),
        proposed_level: 1,
        wot_sync_json: "{}".into(),
        reply_requested: false,
        created_at: now_secs,
    };
    let assertion_bytes = serde_json::to_vec(&assertion).unwrap();
    let env_k2 =
        humoco_market::PostEnvelope::new("k2-msg", &bob_did, now_secs, Vec::new(), Vec::new());
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
        &human_money_core::models::secure_container::PayloadType::TrustAssertion,
        &mut ps,
        &mut baseline,
        &mut stats,
    )
    .await;
    assert!(ps.pending_wot_handshakes.is_empty());
    assert!(
        stats
            .errors
            .iter()
            .any(|e| e.contains("spoofed sender DID")),
        "expected spoofed sender DID error, got {:?}",
        stats.errors
    );

    // 2. H21: MarketInquiryMessage confirm payment_uri pointing to Charlie on Bob's thread is dropped
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
    let env_h21 =
        humoco_market::PostEnvelope::new("m-1", &bob_did, now_secs, Vec::new(), Vec::new());
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
        &human_money_core::models::secure_container::PayloadType::Generic(
            humoco_market::PAYLOAD_MARKET_INQUIRY_MESSAGE_V1.into(),
        ),
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

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn k5_flush_post_slots_412_removes_pruned_outbox_tags_with_fresh_etag() {
    let base_dir = temp_test_dir("k5_412_remove_tags");
    let pw = "TestPassword123!";
    let alice_svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, pw);
    let alice_state = create_test_state(alice_svc);
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
    let pruned_env = humoco_market::PostEnvelope::new(
        "pruned-1",
        &my_did,
        1_760_000_000,
        Vec::new(),
        b"acked".to_vec(),
    );
    let kept_env = humoco_market::PostEnvelope::new(
        "kept-1",
        &my_did,
        1_760_000_001,
        Vec::new(),
        b"keep".to_vec(),
    );
    let pruned_box =
        humoco_market::seal_post_box(&my_ed_sk, &my_ed_sk.verifying_key(), &pruned_env).unwrap();
    let kept_box =
        humoco_market::seal_post_box(&my_ed_sk, &my_ed_sk.verifying_key(), &kept_env).unwrap();
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
        .insert(server_url.clone(), "\"etag-stale-1\"".to_string());
    ps.pruned_outbox_tag16s.insert(hex::encode(pruned_tag));
    save_post_state(&alice_state, &ps, Some(pw)).unwrap();

    let session = alice_state.begin_session_op().unwrap();
    let outcome = flush_post_slots_impl(&alice_state, &session, &mut ps, None, &[], Some(pw)).await;
    assert_eq!(outcome.deposited_to, 1, "errors: {:?}", outcome.errors);
    assert!(ps.pruned_outbox_tag16s.is_empty());

    let raw_after = server
        .slots
        .lock()
        .unwrap()
        .get(&slot_key)
        .cloned()
        .unwrap();
    let entries_after = humoco_market::decode_post_slot(&raw_after).unwrap();
    let real_entries_after: Vec<_> = entries_after
        .into_iter()
        .filter(|e| !humoco_market::is_own_dummy_post_box(&master_seed, e))
        .collect();
    assert_eq!(real_entries_after.len(), 1);
    assert_eq!(real_entries_after[0].tag16, kept_tag);
    assert!(!real_entries_after.iter().any(|e| e.tag16 == pruned_tag));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn test_post_anchored_signing_request_recovers_encrypted_manifest_and_delegations() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::humoco_proto::{
        encode_quarter, AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry,
    };

    let base_dir = temp_test_dir("post_anchored_sign_req_s06_f10");
    let alice_dir = base_dir.join("alice");
    let bob_dir = base_dir.join("bob");
    let password = "post-s06-f10-password";

    let alice_svc = setup_test_app(&alice_dir, "Alice", ALICE_PHRASE, password);
    let bob_svc = setup_test_app(&bob_dir, "Bob", BOB_PHRASE, password);
    let alice_state = create_test_state(alice_svc);
    let bob_state = create_test_state(bob_svc);

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

    let mut creator_probe_cache = std::collections::BTreeMap::new();
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
        creator_probe_cache.insert(
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

    let standard_toml =
        test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
    let local_id = {
        let mut s_alice = alice_state.service.lock().unwrap();
        s_alice
            .update_l2_anchor_manifest(manifest, Some(password))
            .unwrap();
        let v = s_alice
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
                    layer2_anchor: Some(anchor_id),
                },
                Some(password),
            )
            .unwrap();
        human_money_core::Wallet::calculate_local_instance_id(&v, &alice_did).unwrap()
    };

    let signing_req_bytes = {
        let s = alice_state.service.lock().unwrap();
        crate::commands::vouchers::create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
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
        bob_did.clone(),
        signing_req_bytes,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    let bob_sync =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(bob_sync.new_sign_requests, 1);

    let bob_post = load_post_state(&bob_state, Some(password)).unwrap();
    let sign_req = &bob_post.pending_sign_requests[0];

    let opened = {
        let s_bob = bob_state.service.lock().unwrap();
        crate::commands::vouchers::open_voucher_signing_request_impl(
            &sign_req.container_bytes,
            None,
            &s_bob,
        )
        .unwrap()
    };
    assert!(
        opened.anchor_manifest_json.is_some(),
        "PoST signing request must recover encrypted AnchorManifest from SigningRequestBundle"
    );
    assert!(
        opened.anchor_delegations_json.is_some(),
        "PoST signing request must recover encrypted delegations from SigningRequestBundle"
    );
    assert_eq!(opened.manifest_revision, Some(1));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn s10_f1_f9_f10_container_validation_skew_and_bounds() {
    use human_money_core::models::secure_container::{
        EncryptionType, PayloadType, SecureContainer,
    };

    let base_dir = temp_test_dir("s10_f1_f9_f10");
    let password = "TestPassword123!";

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);

    let (alice_did, alice_id, alice_sk) = {
        let s = alice_state.service.lock().unwrap();
        let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
        (id.user_id.clone(), id.clone(), id.signing_key.clone())
    };
    let (bob_did, bob_id) = {
        let s = bob_state.service.lock().unwrap();
        let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
        (id.user_id.clone(), id)
    };
    let bob_pk = human_money_core::crypto::get_pubkey_from_user_id(&bob_did).unwrap();

    // 1. Outgoing container validation rejects Cleartext and mismatched PayloadType
    let cleartext = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::Cleartext,
        br#"{"hello":"world"}"#,
        PayloadType::VoucherForSigning,
    )
    .unwrap();
    let cleartext_bytes = serde_json::to_vec(&cleartext).unwrap();
    assert!(send_signing_request_via_post_impl(
        bob_did.clone(),
        cleartext_bytes,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .is_err());

    let wrong_type = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
        br#"{"hello":"world"}"#,
        PayloadType::VoucherForSigning,
    )
    .unwrap();
    assert!(reply_detached_signature_via_post_impl(
        None,
        bob_did.clone(),
        serde_json::to_vec(&wrong_type).unwrap(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .is_err());

    // 2. Setup sync and establish mutual contact so Bob polls Alice's slot
    let server = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let (write_url, _info) = setup_test_sync(&server, DROP_PUB_A, password, &alice_state).await;
    let alice_read_url = format!("{}/{}", server.base_url, server.data_dir);
    let _bob_info = setup_test_sync(&server, DROP_PUB_A, password, &bob_state).await;
    {
        let mut s_bob = bob_state.service.lock().unwrap();
        bob_state
            .save_contact(
                &mut s_bob,
                FrontendContact {
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
                    syncs: vec![FrontendSyncDescriptor {
                        write_url: write_url.clone(),
                        read_url: alice_read_url.clone(),
                        slot_id: None,
                        drop_pub: DROP_PUB_A.to_string(),
                        status: "active".to_string(),
                        ..Default::default()
                    }],
                },
                Some(password),
            )
            .unwrap();
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
                    tags: vec![],
                    added_at: Utc::now().to_rfc3339(),
                    notes: None,
                    trust_level: Some(1),
                    desired_hops: Some(3),
                    has_peer_list: false,
                    peer_list_updated_at: None,
                    syncs: vec![],
                },
                Some(password),
            )
            .unwrap();
    }

    // Bob publishes profile + inquiries enabled so MarketInquiry can execute if accepted
    save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        },
        Some(password.to_string()),
        &bob_state,
    )
    .unwrap();
    set_card_published_impl(true, Some(password.to_string()), &bob_state)
        .await
        .unwrap();

    // Deposit:
    // (a) unsigned Asymmetric non-TransactionBundle container (stripped signature)
    // (b) Cleartext EncryptionType::None container
    // (c) valid TrustAssertion/Signing request with control char msg_id
    // (d) valid VoucherForSigning with far-future created_at timestamp
    let now = crate::sync_client::current_unix_timestamp();
    let mut unsigned_inq_container = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
        &serde_json::to_vec(&humoco_market::MarketInquiry {
            inquiry_id: "inq-unsigned".to_string(),
            target: InquiryTarget::Person { skill_title: None },
            message: "hi".to_string(),
            created_at: now,
        })
        .unwrap(),
        PayloadType::Generic(humoco_market::PAYLOAD_MARKET_INQUIRY_V1.to_string()),
    )
    .unwrap();
    unsigned_inq_container.signature.clear();

    let cleartext_inq_container = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::Cleartext,
        &serde_json::to_vec(&humoco_market::MarketInquiry {
            inquiry_id: "inq-cleartext".to_string(),
            target: InquiryTarget::Person { skill_title: None },
            message: "hi".to_string(),
            created_at: now,
        })
        .unwrap(),
        PayloadType::Generic(humoco_market::PAYLOAD_MARKET_INQUIRY_V1.to_string()),
    )
    .unwrap();
    assert_eq!(cleartext_inq_container.et, EncryptionType::None);

    let signed_req = SecureContainer::seal(
        &alice_id,
        &ContainerConfig::TargetDid(bob_did.clone(), PrivacyMode::TrialDecryption),
        br#"{"dummy":true}"#,
        PayloadType::VoucherForSigning,
    )
    .unwrap();
    let signed_req_bytes = serde_json::to_vec(&signed_req).unwrap();

    let boxes = vec![
        humoco_market::seal_post_box(
            &alice_sk,
            &bob_pk,
            &humoco_market::PostEnvelope::new(
                "msg-unsigned",
                alice_did.clone(),
                now,
                vec![],
                serde_json::to_vec(&unsigned_inq_container).unwrap(),
            ),
        )
        .unwrap(),
        humoco_market::seal_post_box(
            &alice_sk,
            &bob_pk,
            &humoco_market::PostEnvelope::new(
                "msg-cleartext",
                alice_did.clone(),
                now,
                vec![],
                serde_json::to_vec(&cleartext_inq_container).unwrap(),
            ),
        )
        .unwrap(),
        humoco_market::seal_post_box(
            &alice_sk,
            &bob_pk,
            &humoco_market::PostEnvelope::new(
                "bad\nmsg-id",
                alice_did.clone(),
                now,
                vec![],
                signed_req_bytes.clone(),
            ),
        )
        .unwrap(),
        humoco_market::seal_post_box(
            &alice_sk,
            &bob_pk,
            &humoco_market::PostEnvelope::new(
                "msg-future-skew",
                alice_did.clone(),
                now + 999_999,
                vec![humoco_market::DropDescriptor {
                    write_url: "http://169.254.169.254/drop".to_string(),
                    read_url: "http://169.254.169.254/read".to_string(),
                    slot_id: None,
                    drop_pub: DROP_PUB_A.to_string(),
                }],
                signed_req_bytes,
            ),
        )
        .unwrap(),
    ];

    let master_seed = {
        let s_alice = alice_state.service.lock().unwrap();
        let ctx =
            crate::services::common::SyncIdentityContext::from_service(&s_alice, Some(password))
                .unwrap();
        ctx.master_seed
    };
    let packed = humoco_market::pack_post_slot(&master_seed, &boxes, DROP_POST_MAX_BYTES).unwrap();
    let alice_slot_key = {
        let s_alice = alice_state.service.lock().unwrap();
        let my_prefix = s_alice
            .with_wallet(|w| {
                human_money_core::crypto::get_prefix_from_user_id(w.get_user_id())
                    .map(str::to_string)
            })
            .unwrap();
        let ctx =
            crate::services::common::SyncIdentityContext::from_service(&s_alice, Some(password))
                .unwrap();
        drop(bob_id);
        let spk = {
            let mut s2 = s_alice;
            let pol = alice_state
                .get_or_load_security_policy(&mut s2, Some(password))
                .unwrap();
            let desc = humoco_market::DropDescriptor {
                write_url: pol.public_syncs[0].write_url.clone(),
                read_url: pol.public_syncs[0].read_url.clone(),
                slot_id: Some(pol.public_syncs[0].access_pubkey.clone()),
                drop_pub: pol.public_syncs[0].drop_pub.clone(),
            };
            let signer = crate::services::common::DropChannelSigner {
                is_public: true,
                master_seed: &ctx.master_seed,
                my_ed_sk: &ctx.my_ed_sk,
                drop_desc: &desc,
            };
            signer.derive_slot_pubkey().unwrap()
        };
        let suffix = my_prefix.map(|p| format!("_{}", p)).unwrap_or_default();
        format!("{}{}.post", spk, suffix)
    };
    server
        .slots
        .lock()
        .unwrap()
        .insert(alice_slot_key, packed.blob);

    // Ensure Bob's contact entry for Alice carries Alice's slot_id + prefix
    {
        let mut s_alice = alice_state.service.lock().unwrap();
        let pol = alice_state
            .get_or_load_security_policy(&mut s_alice, Some(password))
            .unwrap();
        let alice_prefix =
            human_money_core::crypto::get_prefix_from_user_id(&alice_did).map(str::to_string);
        let alice_slot_id = humoco_market::format_slot_id(
            &pol.public_syncs[0].access_pubkey,
            alice_prefix.as_deref(),
        )
        .unwrap();
        drop(s_alice);
        let mut s_bob = bob_state.service.lock().unwrap();
        let mut c = bob_state
            .load_contacts(&mut s_bob, Some(password))
            .unwrap()
            .into_iter()
            .find(|c| c.did == alice_did)
            .unwrap();
        c.syncs[0].slot_id = Some(alice_slot_id);
        bob_state
            .save_contact(&mut s_bob, c, Some(password))
            .unwrap();
    }

    let bob_sync =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(bob_sync.new_inquiries, 0);
    assert_eq!(bob_sync.new_sign_requests, 1);
    assert!(bob_sync.errors.iter().any(|e| e.contains("signature")));
    assert!(bob_sync.errors.iter().any(|e| e.contains("non-asymmetric")));
    assert!(bob_sync.errors.iter().any(|e| e.contains("invalid msg_id")));

    let bob_ps = load_post_state(&bob_state, Some(password)).unwrap();
    assert_eq!(bob_ps.pending_sign_requests.len(), 1);
    assert!(
        bob_ps.pending_sign_requests[0].created_at <= now + MAX_POST_FUTURE_SKEW_SECS + 5,
        "Future timestamp must be clamped to MAX_POST_FUTURE_SKEW_SECS"
    );
    assert!(
        bob_ps.pending_sign_requests[0].reply_syncs.is_empty(),
        "Untrusted SSRF reply_drop must be sanitized out"
    );
    let bob_contacts = {
        let mut s_bob = bob_state.service.lock().unwrap();
        bob_state.load_contacts(&mut s_bob, Some(password)).unwrap()
    };
    let alice_contact = bob_contacts.iter().find(|c| c.did == alice_did).unwrap();
    assert!(
        !alice_contact
            .syncs
            .iter()
            .any(|s| s.write_url.contains("169.254.169.254")),
        "Contact syncs must never be poisoned with rejected SSRF reply_drops"
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn s10_f2_tombstones_survive_partial_own_flush_and_per_tag_attribution() {
    let base_dir = temp_test_dir("s10_f2_tombstones");
    let password = "TestPassword123!";
    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    let live_srv = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let (_live_w, _live_info) =
        setup_test_sync(&live_srv, DROP_PUB_A, password, &alice_state).await;

    // Bind and drop a dead port for second own sync
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
    assert!(
        ps.pruned_outbox_tag16s.contains(&tomb),
        "Partial own-target failure must preserve pruned_outbox_tag16s tombstones"
    );

    // Also test deferred/omitted large item cannot steal another server's deposited_to attribution
    let bob_vk = human_money_core::crypto::generate_ed25519_keypair_for_tests(None).0;
    let bob_did = human_money_core::crypto::create_user_id(&bob_vk, None).unwrap();
    let my_sk = {
        let s_alice = alice_state.service.lock().unwrap();
        crate::services::common::SyncIdentityContext::from_service(&s_alice, Some(password))
            .unwrap()
            .my_ed_sk
    };
    let large_payload = vec![9u8; 40_000];
    let large1_env = humoco_market::PostEnvelope::new(
        "large-box-1",
        bob_did.clone(),
        1_750_000_000,
        vec![],
        large_payload.clone(),
    );
    let large1_entry = humoco_market::seal_post_box(&my_sk, &bob_vk, &large1_env).unwrap();
    let large2_env = humoco_market::PostEnvelope::new(
        "large-box-2",
        bob_did.clone(),
        1_750_000_001,
        vec![],
        large_payload.clone(),
    );
    let large2_entry = humoco_market::seal_post_box(&my_sk, &bob_vk, &large2_env).unwrap();
    let large3_env = humoco_market::PostEnvelope::new(
        "large-box-3",
        bob_did.clone(),
        1_750_000_002,
        vec![],
        large_payload,
    );
    let large3_entry = humoco_market::seal_post_box(&my_sk, &bob_vk, &large3_env).unwrap();

    for (id, entry, ts) in [
        ("large-box-1", &large1_entry, 1_750_000_000u64),
        ("large-box-2", &large2_entry, 1_750_000_001u64),
        ("large-box-3", &large3_entry, 1_750_000_002u64),
    ] {
        ps.outbox_entries.push(crate::state::PostOutboxEntry {
            msg_id: id.to_string(),
            recipient_did: bob_did.clone(),
            tag16_hex: hex::encode(entry.tag16),
            bundle_id: None,
            message_class: crate::state::OutboxMessageClass::Control,
            entry: entry.clone(),
            created_at: ts,
        });
    }

    let outcome2 =
        flush_post_slots_impl(&alice_state, &session, &mut ps, None, &[], Some(password)).await;
    assert_eq!(
        outcome2
            .uploaded_counts_by_tag
            .get(&large1_entry.tag16)
            .copied(),
        Some(1)
    );
    assert_eq!(
        outcome2.uploaded_counts_by_tag.get(&large3_entry.tag16).copied(),
        None,
        "Deferred overflowing box must have 0 per-tag deposited count even when earlier boxes succeeded"
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn s10_f3_canonical_inquiry_rate_limit_and_no_phantom_own_upload() {
    let base_dir = temp_test_dir("s10_f3_inquiry");
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

    let alice_srv = MockSyncServer::start(DROP_PUB_A.to_string(), ADMIN_A.to_string()).await;
    let (alice_w, alice_info) =
        setup_test_sync(&alice_srv, DROP_PUB_A, password, &alice_state).await;

    // Bind and drop a dead port for second own sync (keep _aw above unused)
    let _ = (&alice_w, &alice_info);

    // 1. Unreachable recipient upload must NOT leak a phantom box onto Alice's own sync server
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
        bob_did.clone(),
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

    // 2. Canonical ad_title + Person skill_title shares single "person" rate-limit bucket + dismiss frees quota
    let bob_srv = MockSyncServer::start(DROP_PUB_B.to_string(), ADMIN_B.to_string()).await;
    let (bob_w, bob_info) = setup_test_sync(&bob_srv, DROP_PUB_B, password, &bob_state).await;
    let alice_probe_b =
        probe_public_sync_impl(bob_w.clone(), Some(password.to_string()), &alice_state)
            .await
            .unwrap();
    bob_srv
        .members
        .lock()
        .unwrap()
        .push(alice_probe_b.access_pubkey);
    let bob_prefix =
        human_money_core::crypto::get_prefix_from_user_id(&bob_did).map(str::to_string);
    let bob_slot_id =
        humoco_market::format_slot_id(&bob_info.access_pubkey, bob_prefix.as_deref()).unwrap();
    let bob_syncs = vec![FrontendSyncDescriptor {
        write_url: bob_w,
        read_url: bob_info.read_url,
        slot_id: Some(bob_slot_id),
        drop_pub: bob_info.drop_pub,
        status: "active".to_string(),
        ..Default::default()
    }];
    // Bob adds Alice so collect_authorized_post_senders includes Alice (while Alice keeps Alice's sync readable)
    {
        let mut s_bob = bob_state.service.lock().unwrap();
        bob_state
            .save_contact(
                &mut s_bob,
                FrontendContact {
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
                    syncs: vec![FrontendSyncDescriptor {
                        write_url: alice_w,
                        read_url: alice_info.read_url,
                        slot_id: Some(
                            humoco_market::format_slot_id(
                                &alice_info.access_pubkey,
                                human_money_core::crypto::get_prefix_from_user_id(&alice_did),
                            )
                            .unwrap(),
                        ),
                        drop_pub: alice_info.drop_pub,
                        status: "active".to_string(),
                        ..Default::default()
                    }],
                },
                Some(password),
            )
            .unwrap();
    }

    save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: None,
        },
        Some(password.to_string()),
        &bob_state,
    )
    .unwrap();
    set_card_published_impl(true, Some(password.to_string()), &bob_state)
        .await
        .unwrap();
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
        &bob_state,
    )
    .unwrap();
    let ad_id = prof.ads[0].id.clone();

    // Send 1 ad inquiry with spoofed ad_title + 2 Person inquiries with distinct skill_title values
    send_market_inquiry_via_post_impl(
        bob_did.clone(),
        FrontendInquiryTarget::Ad {
            ad_id,
            ad_title: "SPOOFED TITLE".to_string(),
        },
        "Interested in ad".to_string(),
        bob_syncs.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    send_market_inquiry_via_post_impl(
        bob_did.clone(),
        FrontendInquiryTarget::Person {
            skill_title: Some("Skill 0".to_string()),
        },
        "Person inquiry 0".to_string(),
        bob_syncs.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();
    // Second Person inquiry with a different skill_title must be rate-limited ("person" bucket)
    send_market_inquiry_via_post_impl(
        bob_did.clone(),
        FrontendInquiryTarget::Person {
            skill_title: Some("Skill BypassAttempt".to_string()),
        },
        "Blocked bypass".to_string(),
        bob_syncs.clone(),
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    let bob_sync =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(bob_sync.new_inquiries, 2);

    let bob_ps = load_post_state(&bob_state, Some(password)).unwrap();
    let ad_inq = bob_ps
        .inquiries
        .iter()
        .find(|i| matches!(&i.target, FrontendInquiryTarget::Ad { .. }))
        .unwrap();
    assert_eq!(
        ad_inq.target,
        FrontendInquiryTarget::Ad {
            ad_id: match &ad_inq.target {
                FrontendInquiryTarget::Ad { ad_id, .. } => ad_id.clone(),
                _ => unreachable!(),
            },
            ad_title: "Genuine Honey".to_string()
        }
    );

    // Dismissing one open Person seller inquiry frees rate-limit slot so next Person inquiry succeeds
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
        &bob_state,
    )
    .unwrap();

    send_market_inquiry_via_post_impl(
        bob_did,
        FrontendInquiryTarget::Person {
            skill_title: Some("Skill AfterDismiss".to_string()),
        },
        "Now allowed".to_string(),
        bob_syncs,
        Some(password.to_string()),
        &alice_state,
    )
    .await
    .unwrap();

    let bob_sync_after =
        sync_post_channels_impl(None, None, None, Some(password.to_string()), &bob_state)
            .await
            .unwrap();
    assert_eq!(bob_sync_after.new_inquiries, 1);
    let _ = alice_did;

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn s10_f4_f5_timeout_zero_summary_and_3way_merge_safety() {
    let base_dir = temp_test_dir("s10_f4_f5");
    let password = "TestPassword123!";
    let bob_pk = human_money_core::crypto::generate_ed25519_keypair_for_tests(None).0;
    let bob_did = human_money_core::crypto::create_user_id(&bob_pk, None).unwrap();
    let carol_pk = human_money_core::crypto::generate_ed25519_keypair_for_tests(None).0;
    let carol_did = human_money_core::crypto::create_user_id(&carol_pk, None).unwrap();

    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    // Seed contact and inquiry in post_state, then reload wallet under session_timeout_seconds = 0 (!is_session_active())
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
    let wallet_dir = base_dir.join("alice");
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s,
                FrontendContact {
                    did: bob_did.clone(),
                    profile: FrontendUserProfile {
                        first_name: Some("Cached Bob".to_string()),
                        ..Default::default()
                    },
                    tags: vec![],
                    added_at: Utc::now().to_rfc3339(),
                    notes: None,
                    trust_level: Some(1),
                    desired_hops: Some(3),
                    has_peer_list: false,
                    peer_list_updated_at: None,
                    syncs: vec![],
                },
                Some(password),
            )
            .unwrap();
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

    // 3-way merge tests: working marked unread=false, concurrent reply didn't set working_added_msgs falsely
    let mut base = StoredPostState::default();
    base.pruned_outbox_tag16s.insert("tomb-a".to_string());
    base.replied_sign_request_ids
        .insert(scoped_processed_msg_key(&bob_did, "req-1"));
    let mut inq_base = summary.inquiries[0].clone();
    inq_base.unread = true;
    base.inquiries.push(inq_base.clone());

    let mut latest = base.clone();
    // Concurrent sync cleared tomb-a and kept inq_base untouched
    latest.pruned_outbox_tag16s.clear();
    latest
        .pending_wot_handshakes
        .push(crate::models::FrontendPendingWotHandshake {
            msg_id: "shared-id".to_string(),
            sender_did: carol_did.clone(),
            sender_name: Some("Carol".to_string()),
            proposed_level: 1,
            reply_requested: false,
            created_at: 200,
        });

    let mut working = base.clone();
    // Working marked read and also received a same-msg_id handshake from Bob
    working.inquiries[0].unread = false;
    working
        .pending_wot_handshakes
        .push(crate::models::FrontendPendingWotHandshake {
            msg_id: "shared-id".to_string(),
            sender_did: bob_did.clone(),
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
    assert!(
        !working.pruned_outbox_tag16s.contains("tomb-a"),
        "Concurrent deletion of pruned_outbox_tag16s must propagate via 3-way set merge"
    );
    assert_eq!(
        working.pending_wot_handshakes.len(),
        2,
        "Identical msg_id from distinct senders must both survive scoped 3-way merge"
    );
    assert!(
        !working.inquiries[0].unread,
        "Concurrent latest.messages must not clobber explicit mark-read"
    );

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[tokio::test]
async fn s10_f7_f8_locator_ordering_retry_and_accept_wot_preserves_contact() {
    let base_dir = temp_test_dir("s10_f7_f8");
    let password = "TestPassword123!";
    let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);
    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);

    let (alice_did, alice_id) = {
        let s = alice_state.service.lock().unwrap();
        let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
        (id.user_id.clone(), id)
    };
    let (bob_did, _bob_id) = {
        let s = bob_state.service.lock().unwrap();
        let id = s.with_wallet_and_identity(|_, id| id.clone()).unwrap();
        (id.user_id.clone(), id)
    };

    // 1. accept_wot_handshake_impl must preserve existing contact syncs, notes, and tags
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

    // 2. handle_guardian_locator before guardian_assign returns RetryLater; stale/revoked returns Handled
    let session = alice_state.begin_session_op().unwrap();
    let now = 1_760_000_000u64;
    let update = humoco_market::GuardianLocatorUpdate {
        owner_did: bob_did.clone(),
        rotation_epoch: 1,
        backup_sync_descriptors: vec![],
        updated_at: now,
    };
    let env = humoco_market::PostEnvelope::new("loc-1", bob_did.clone(), now, vec![], vec![]);
    let my_sk = alice_id.signing_key.clone();
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
        decrypted_bytes: &serde_json::to_vec(&update).unwrap(),
        now_secs: now,
    };
    let mut ps = StoredPostState::default();
    let mut base_ps = ps.clone();
    let mut stats = super::handlers::PostDispatchStats::default();
    let outcome_early = super::handlers::dispatch_post_payload(
        &ctx,
        &human_money_core::models::secure_container::PayloadType::Generic(
            humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1.to_string(),
        ),
        &mut ps,
        &mut base_ps,
        &mut stats,
    )
    .await;
    assert_eq!(
        outcome_early,
        super::handlers::PostDispatchOutcome::RetryLater,
        "Locator arriving before guardian_assign must return RetryLater"
    );

    // Record revoked epoch 1 in guardian vault, then replay epoch 1 locator -> permanent Handled (no ACK)
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
        &human_money_core::models::secure_container::PayloadType::Generic(
            humoco_market::PAYLOAD_GUARDIAN_LOCATOR_V1.to_string(),
        ),
        &mut ps,
        &mut base_ps,
        &mut stats,
    )
    .await;
    assert_eq!(
        outcome_revoked,
        super::handlers::PostDispatchOutcome::Handled,
        "Revoked epoch locator must return permanent Handled so poison envelope stops retrying"
    );
    assert_eq!(ps.outbox_entries.len(), outbox_before);
    let _ = alice_did;

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn s10_q2_processed_msg_ids_30d_retention_and_no_resurrection_on_merge() {
    let base_dir = temp_test_dir("s10_q2_retention");
    let password = "TestPassword123!";
    let alice_state = create_test_state(setup_test_app(
        &base_dir.join("alice"),
        "Alice",
        ALICE_PHRASE,
        password,
    ));

    let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
    let exact_expiry = now_secs.saturating_sub(super::PROCESSED_MSG_RETENTION_SECS);
    let fresh_ts = now_secs.saturating_sub(humoco_market::POST_TTL_SECS);

    let mut map = std::collections::BTreeMap::new();
    map.insert("expired-exact".to_string(), exact_expiry);
    map.insert(
        "expired-older".to_string(),
        exact_expiry.saturating_sub(100),
    );
    map.insert("fresh-within-ttl".to_string(), fresh_ts);
    map.insert("fresh-recent".to_string(), now_secs);
    super::prune_expired_processed_msg_ids(&mut map, now_secs);
    assert!(!map.contains_key("expired-exact"));
    assert!(!map.contains_key("expired-older"));
    assert!(map.contains_key("fresh-within-ttl"));
    assert!(map.contains_key("fresh-recent"));

    // 3-way merge must NOT resurrect pruned expired IDs from `latest`, while keeping concurrent inserts
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

    assert!(
        !working.processed_msg_ids.contains_key("old-key"),
        "Pruned expired processed_msg_id must NOT be resurrected during 3-way merge"
    );
    assert!(working.processed_msg_ids.contains_key("keep-key"));
    assert!(working.processed_msg_ids.contains_key("concurrent-new"));
    assert!(working.processed_msg_ids.contains_key("worker-new"));

    let _ = std::fs::remove_dir_all(&base_dir);
}
