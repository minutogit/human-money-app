// src-tauri/src/state/tests.rs
use crate::models::{FrontendContact, FrontendSyncDescriptor, FrontendUserProfile};
use chrono::Utc;

#[test]
fn test_save_contact_authoritative_syncs_deletion() {
    let temp_dir = std::env::temp_dir().join(format!("humoco_state_test_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let bob_service = crate::test_support::setup_test_app(
        &temp_dir.join("bob"),
        "Test Bob",
        crate::test_support::BOB_PHRASE,
        password,
    );
    let bob_did = bob_service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    let sync1 = FrontendSyncDescriptor {
        read_url: "https://sync1.example.org/sync.php".to_string(),
        write_url: "https://sync1.example.org/sync.php".to_string(),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0x01),
        slot_id: None,
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "unverified".to_string(),
        last_probed_at: None,
        ..Default::default()
    };
    let sync2 = FrontendSyncDescriptor {
        read_url: "https://sync2.example.org/sync.php".to_string(),
        write_url: "https://sync2.example.org/sync.php".to_string(),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0x02),
        slot_id: None,
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "unverified".to_string(),
        last_probed_at: None,
        ..Default::default()
    };

    // 1. Initial contact with 2 sync endpoints
    let initial_contact = FrontendContact {
        did: bob_did.clone(),
        profile: FrontendUserProfile {
            id: Some(bob_did.clone()),
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![sync1.clone(), sync2.clone()],
    };

    state
        .save_contact(&mut service, initial_contact, Some(password))
        .unwrap();

    let loaded = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = loaded.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob.syncs.len(), 2);

    // 2. Update contact with sync2 removed (only sync1 remains)
    let updated_one_sync = FrontendContact {
        did: bob_did.clone(),
        profile: FrontendUserProfile {
            id: Some(bob_did.clone()),
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: String::new(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![sync1.clone()], // sync2 removed!
    };

    state
        .save_contact(&mut service, updated_one_sync, Some(password))
        .unwrap();

    // 3. Verify sync2 is deleted and NOT resurrected from old syncs
    let loaded2 = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob2 = loaded2.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob2.syncs.len(), 1, "Deleted sync2 must not be resurrected");
    assert_eq!(bob2.syncs[0].read_url, "https://sync1.example.org/sync.php");

    // 4. Update contact with empty syncs (all syncs deleted)
    let updated_zero_syncs = FrontendContact {
        did: bob_did.clone(),
        profile: FrontendUserProfile {
            id: Some(bob_did.clone()),
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: String::new(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![], // All removed!
    };

    state
        .save_contact(&mut service, updated_zero_syncs, Some(password))
        .unwrap();

    // 5. Verify all syncs deleted
    let loaded3 = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob3 = loaded3.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(
        bob3.syncs.len(),
        0,
        "All syncs must be removed authoritatively"
    );
}

#[test]
fn test_save_contact_allows_clearing_last_tag() {
    let temp_dir =
        std::env::temp_dir().join(format!("humoco_tag_clear_test_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let state = super::AppState::new(alice_service);
    let bob_did = "did:key:z6MkrJVnaZkeFzdQyMZu1cgjg7k1pZZ6pvBQ7XJPt4swbTQ2";
    let mut service = state.service.lock().unwrap();

    let with_tags = crate::models::FrontendContact {
        did: bob_did.to_string(),
        profile: crate::models::FrontendUserProfile {
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec!["friend".to_string(), "neighbor".to_string()],
        added_at: String::new(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    };
    state
        .save_contact(&mut service, with_tags, Some(password))
        .unwrap();

    let cleared_tags = crate::models::FrontendContact {
        did: bob_did.to_string(),
        profile: crate::models::FrontendUserProfile {
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: String::new(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    };
    state
        .save_contact(&mut service, cleared_tags, Some(password))
        .unwrap();

    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
    assert!(
        bob.tags.is_empty(),
        "Clearing the last tag must persist empty tags instead of restoring old tags"
    );
}

#[test]
fn test_record_contact_sync_health_batch() {
    let temp_dir =
        std::env::temp_dir().join(format!("humoco_health_test_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let bob_service = crate::test_support::setup_test_app(
        &temp_dir.join("bob"),
        "Test Bob",
        crate::test_support::BOB_PHRASE,
        password,
    );
    let bob_did = bob_service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    let sync_desc = FrontendSyncDescriptor {
        read_url: "https://sync-endpoint.org/sync.php".to_string(),
        write_url: "https://sync-endpoint.org/sync.php".to_string(),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0x03),
        slot_id: None,
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "unverified".to_string(),
        last_probed_at: None,
        ..Default::default()
    };

    let contact = FrontendContact {
        did: bob_did.clone(),
        profile: FrontendUserProfile {
            id: Some(bob_did.clone()),
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![sync_desc],
    };
    state
        .save_contact(&mut service, contact, Some(password))
        .unwrap();

    // 1. Success update preserves "unverified" (does NOT overwrite with "active")
    let updated = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), true)],
        1000,
        Some(password),
    );
    assert!(updated);
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob.syncs[0].last_seen_at, Some(1000));
    assert_eq!(bob.syncs[0].fail_count, 0);
    assert_eq!(
        bob.syncs[0].status, "unverified",
        "unverified status must not be overwritten with active on static read success"
    );

    // 1b. Test that "pending_auth" is also preserved on read success
    let mut bob_pending = bob.clone();
    bob_pending.syncs[0].status = "pending_auth".to_string();
    state
        .save_contact(&mut service, bob_pending, Some(password))
        .unwrap();
    let updated_pending = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), true)],
        1050,
        Some(password),
    );
    assert!(updated_pending);
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(
        bob.syncs[0].status, "pending_auth",
        "pending_auth status must not be overwritten with active on static read success"
    );

    // 2. Transient failure on pending_auth increments fail_count without immediately flipping to unreachable
    let updated2 = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), false)],
        1100,
        Some(password),
    );
    assert!(updated2);
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob.syncs[0].last_error_at, Some(1100));
    assert_eq!(bob.syncs[0].fail_count, 1);
    assert_eq!(bob.syncs[0].status, "pending_auth");

    // 3. Second failure still keeps pending_auth; third consecutive failure transitions to unreachable
    let updated3 = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), false)],
        1200,
        Some(password),
    );
    assert!(updated3);
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob.syncs[0].last_error_at, Some(1200));
    assert_eq!(bob.syncs[0].fail_count, 2);
    assert_eq!(bob.syncs[0].status, "pending_auth");

    let updated3b = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), false)],
        1250,
        Some(password),
    );
    assert!(updated3b);
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob.syncs[0].last_error_at, Some(1250));
    assert_eq!(bob.syncs[0].fail_count, 3);
    assert_eq!(bob.syncs[0].status, "unreachable");

    // 4. Recovery success resets fail_count; since not in market_selected_syncs, transitions to "unverified"
    let updated4 = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), true)],
        1300,
        Some(password),
    );
    assert!(updated4);
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob = contacts.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob.syncs[0].last_seen_at, Some(1300));
    assert_eq!(bob.syncs[0].fail_count, 0);
    assert_eq!(
        bob.syncs[0].status, "unverified",
        "unreachable contact sync transitions to unverified on read success when not in market_selected_syncs"
    );

    // 5. If sync is in market_selected_syncs, unreachable transitions to "active" on recovery
    state
        .mutate_security_policy(&mut service, Some(password), |policy| {
            policy.market_selected_syncs = vec![FrontendSyncDescriptor {
                read_url: "https://sync-endpoint.org/sync.php".to_string(),
                write_url: "https://sync-endpoint.org/sync.php".to_string(),
                drop_pub: crate::test_support::test_ed25519_pub_hex(0x03),
                slot_id: None,
                last_seen_at: None,
                last_error_at: None,
                fail_count: 0,
                status: "active".to_string(),
                last_probed_at: None,
                ..Default::default()
            }];
        })
        .unwrap();

    // Mark contact as unreachable again
    let _ = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), false)],
        1400,
        Some(password),
    );
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(
        contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status,
        "unreachable"
    );

    // Now recover: should transition to "active" because it's in market_selected_syncs
    let _ = state.record_contact_sync_health_batch(
        &mut service,
        &[("https://sync-endpoint.org/sync.php".to_string(), true)],
        1500,
        Some(password),
    );
    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(
        contacts.iter().find(|c| c.did == bob_did).unwrap().syncs[0].status,
        "active"
    );
}

#[test]
fn test_prune_orphaned_market_selected_syncs() {
    let temp_dir = std::env::temp_dir().join(format!("humoco_prune_test_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let bob_service = crate::test_support::setup_test_app(
        &temp_dir.join("bob"),
        "Test Bob",
        crate::test_support::BOB_PHRASE,
        password,
    );
    let bob_did = bob_service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    let bob_sync = FrontendSyncDescriptor {
        read_url: "https://bob-sync.org/sync.php".to_string(),
        write_url: "https://bob-sync.org/sync.php".to_string(),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0x04),
        slot_id: None,
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "active".to_string(),
        last_probed_at: None,
        ..Default::default()
    };

    let contact = FrontendContact {
        did: bob_did.clone(),
        profile: FrontendUserProfile {
            id: Some(bob_did.clone()),
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![bob_sync.clone()],
    };
    state
        .save_contact(&mut service, contact.clone(), Some(password))
        .unwrap();

    // Put Bob's sync into security_policy.market_selected_syncs
    let mut policy = state
        .load_security_policy(&mut service, Some(password))
        .unwrap();
    policy.market_selected_syncs = vec![bob_sync];
    state
        .save_security_policy(&mut service, policy, Some(password))
        .unwrap();

    // 1. Updating contact to remove sync should prune it from security_policy.market_selected_syncs
    let mut updated_contact = contact.clone();
    updated_contact.syncs = vec![];
    state
        .save_contact(&mut service, updated_contact, Some(password))
        .unwrap();

    let s_after = state
        .load_security_policy(&mut service, Some(password))
        .unwrap();
    assert_eq!(
        s_after.market_selected_syncs.len(),
        0,
        "Orphaned contact sync must be pruned from market_selected_syncs on save_contact"
    );

    // 2. Add contact back with sync, add to market_selected_syncs, then delete_contact
    state
        .save_contact(&mut service, contact, Some(password))
        .unwrap();
    let mut policy2 = state
        .load_security_policy(&mut service, Some(password))
        .unwrap();
    policy2.market_selected_syncs = vec![FrontendSyncDescriptor {
        read_url: "https://bob-sync.org/sync.php".to_string(),
        write_url: "https://bob-sync.org/sync.php".to_string(),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0x04),
        slot_id: None,
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "active".to_string(),
        last_probed_at: None,
        ..Default::default()
    }];
    state
        .save_security_policy(&mut service, policy2, Some(password))
        .unwrap();

    state
        .delete_contact(&mut service, &bob_did, Some(password))
        .unwrap();
    let s_after_del = state
        .load_security_policy(&mut service, Some(password))
        .unwrap();
    assert_eq!(
        s_after_del.market_selected_syncs.len(),
        0,
        "Orphaned contact sync must be pruned from market_selected_syncs on delete_contact"
    );
}

#[test]
fn test_merge_contact_syncs_unprobed_distinct_write_urls() {
    let ep1 = FrontendSyncDescriptor {
        write_url: "https://sync-a.example.com/sync.php".to_string(),
        read_url: String::new(),
        slot_id: None,
        drop_pub: String::new(),
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "unverified".to_string(),
        last_probed_at: None,
        ..Default::default()
    };
    let ep2 = FrontendSyncDescriptor {
        write_url: "https://sync-b.example.com/sync.php".to_string(),
        read_url: String::new(),
        slot_id: None,
        drop_pub: String::new(),
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "unverified".to_string(),
        last_probed_at: None,
        ..Default::default()
    };

    // Merge two unprobed syncs with distinct write URLs: neither should be dropped
    let merged = super::merge_contact_syncs(std::slice::from_ref(&ep1), std::slice::from_ref(&ep2));
    assert_eq!(merged.len(), 2, "Both distinct unprobed syncs must be kept");
    assert_eq!(merged[0].write_url, "https://sync-b.example.com/sync.php");
    assert_eq!(merged[1].write_url, "https://sync-a.example.com/sync.php");

    // Merge populated card sync into an existing unprobed sync with same write_url
    let populated_card = FrontendSyncDescriptor {
        write_url: "https://sync-a.example.com/sync.php/".to_string(),
        read_url: "https://sync-a.example.com/read".to_string(),
        slot_id: Some("slot123".to_string()),
        drop_pub: "pubkey_a".to_string(),
        last_seen_at: None,
        last_error_at: None,
        fail_count: 0,
        status: "unverified".to_string(),
        last_probed_at: None,
        ..Default::default()
    };
    let merged2 = super::merge_contact_syncs(&[ep1], &[populated_card]);
    assert_eq!(
        merged2.len(),
        1,
        "Matching write_url should be merged into single entry"
    );
    assert_eq!(merged2[0].read_url, "https://sync-a.example.com/read");
    assert_eq!(merged2[0].drop_pub, "pubkey_a");
    assert_eq!(merged2[0].slot_id, Some("slot123".to_string()));
}

#[test]
fn test_apply_revocation_notice_to_address_book_grace_and_inplace_replace() {
    use std::collections::HashMap;
    let old_pub = "aa".repeat(32);
    let new_pub = "bb".repeat(32);
    let other_pub = "cc".repeat(32);

    let mut contacts = HashMap::new();
    contacts.insert(
        "did:key:zAlice".to_string(),
        FrontendContact {
            did: "did:key:zAlice".to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: "100".to_string(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![FrontendSyncDescriptor {
                write_url: "https://old.example.org/sync.php".to_string(),
                read_url: "https://old.example.org/d-old".to_string(),
                slot_id: Some("oldslot".to_string()),
                drop_pub: old_pub.clone(),
                status: "active".to_string(),
                ..Default::default()
            }],
        },
    );
    contacts.insert(
        "did:key:zBob".to_string(),
        FrontendContact {
            did: "did:key:zBob".to_string(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: "100".to_string(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![
                FrontendSyncDescriptor {
                    write_url: "https://old.example.org/sync.php".to_string(),
                    read_url: "https://old.example.org/d-old".to_string(),
                    slot_id: Some("bobold".to_string()),
                    drop_pub: old_pub.clone(),
                    status: "active".to_string(),
                    ..Default::default()
                },
                FrontendSyncDescriptor {
                    write_url: "https://keep.example.org/sync.php".to_string(),
                    read_url: "https://keep.example.org/d-keep".to_string(),
                    slot_id: Some("keep".to_string()),
                    drop_pub: other_pub.clone(),
                    status: "active".to_string(),
                    ..Default::default()
                },
            ],
        },
    );

    let mut ab = crate::models::FrontendAddressBook { contacts };
    let notice = crate::sync_client::SyncRevocationNotice {
        drop_pub: old_pub.clone(),
        issued_at: 1000,
        effective_at: 2000,
        reason: Some("Server-Umzug".to_string()),
        replacement: Some(crate::sync_client::SyncReplacementTarget {
            write_url: "https://new.example.org/sync.php".to_string(),
            read_url: "https://new.example.org/d-new".to_string(),
            drop_pub: new_pub.clone(),
        }),
        signature: "00".repeat(64),
    };

    // Wrong drop_pub -> no changes (anti-hijacking)
    let mut wrong_notice = notice.clone();
    wrong_notice.drop_pub = "ff".repeat(32);
    assert!(!super::apply_revocation_notice_to_address_book(
        &mut ab,
        "https://old.example.org/d-old",
        &wrong_notice,
        1500
    ));

    // During grace period (now = 1500 < 2000):
    // Alice (1 sync) gets replacement appended with slot_id = None
    // Bob (2 syncs) keeps 2 entries, old marked deprecating with replacement metadata
    assert!(super::apply_revocation_notice_to_address_book(
        &mut ab,
        "https://old.example.org/d-old",
        &notice,
        1500
    ));
    let alice = &ab.contacts["did:key:zAlice"];
    assert_eq!(alice.syncs.len(), 2);
    assert_eq!(alice.syncs[0].status, "deprecating");
    assert_eq!(alice.syncs[1].read_url, "https://new.example.org/d-new");
    assert_eq!(alice.syncs[1].slot_id, None);

    let bob = &ab.contacts["did:key:zBob"];
    assert_eq!(bob.syncs.len(), 2);
    assert_eq!(bob.syncs[0].status, "deprecating");
    assert!(bob.syncs[0].replacement.is_some());

    // After effective_at (now = 2500 >= 2000):
    // Bob's expired entry is replaced in-place with new server (slot_id = None)
    assert!(super::apply_revocation_notice_to_address_book(
        &mut ab,
        "https://old.example.org/d-old",
        &notice,
        2500
    ));
    let bob_after = &ab.contacts["did:key:zBob"];
    assert_eq!(bob_after.syncs.len(), 2);
    assert_eq!(bob_after.syncs[0].read_url, "https://new.example.org/d-new");
    assert_eq!(bob_after.syncs[0].slot_id, None);
    assert_eq!(
        bob_after.syncs[1].read_url,
        "https://keep.example.org/d-keep"
    );
    let alice_after = &ab.contacts["did:key:zAlice"];
    assert_eq!(alice_after.syncs.len(), 1);
    assert_eq!(
        alice_after.syncs[0].read_url,
        "https://new.example.org/d-new"
    );
}

#[test]
fn test_record_contact_sync_health_preserves_deprecating_and_revoked() {
    let temp_dir = std::env::temp_dir().join(format!("humoco_health_rev_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let bob_service = crate::test_support::setup_test_app(
        &temp_dir.join("bob"),
        "Test Bob",
        crate::test_support::BOB_PHRASE,
        password,
    );
    let bob_did = bob_service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    let dep_ep = FrontendSyncDescriptor {
        write_url: "https://dep.example.org/sync.php".to_string(),
        read_url: "https://dep.example.org/d-dep".to_string(),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0xaa),
        status: "deprecating".to_string(),
        sunset_at: Some(2000),
        revocation_reason: Some("Umzug".to_string()),
        ..Default::default()
    };
    let rev_ep = FrontendSyncDescriptor {
        write_url: "https://rev.example.org/sync.php".to_string(),
        read_url: "https://rev.example.org/d-rev".to_string(),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0xbb),
        status: "revoked".to_string(),
        sunset_at: Some(1500),
        ..Default::default()
    };

    let contact = FrontendContact {
        did: bob_did.clone(),
        profile: FrontendUserProfile {
            id: Some(bob_did.clone()),
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![dep_ep.clone(), rev_ep.clone()],
    };
    state
        .save_contact(&mut service, contact, Some(password))
        .unwrap();

    state
        .mutate_security_policy(&mut service, Some(password), |policy| {
            policy.market_selected_syncs = vec![dep_ep.clone(), rev_ep.clone()];
        })
        .unwrap();

    // 1. Before sunset (now = 1800 < 2000): success=true keeps deprecating, failure=false keeps revoked
    state.record_contact_sync_health_batch(
        &mut service,
        &[
            ("https://dep.example.org/d-dep".to_string(), true),
            ("https://rev.example.org/d-rev".to_string(), false),
        ],
        1800,
        Some(password),
    );

    let contacts_1 = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob_1 = contacts_1.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob_1.syncs[0].status, "deprecating");
    assert_eq!(bob_1.syncs[0].last_seen_at, Some(1800));
    assert_eq!(bob_1.syncs[1].status, "revoked");
    assert_eq!(bob_1.syncs[1].fail_count, 1);

    let policy_1 = state
        .load_security_policy(&mut service, Some(password))
        .unwrap();
    assert_eq!(policy_1.market_selected_syncs[0].status, "deprecating");
    assert_eq!(policy_1.market_selected_syncs[1].status, "revoked");

    // 2. Failure on deprecating before sunset preserves deprecating (not "unreachable")
    state.record_contact_sync_health_batch(
        &mut service,
        &[("https://dep.example.org/d-dep".to_string(), false)],
        1900,
        Some(password),
    );
    let contacts_2 = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob_2 = contacts_2.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob_2.syncs[0].status, "deprecating");
    assert_eq!(bob_2.syncs[0].fail_count, 1);

    // 3. Past sunset (now = 2100 >= 2000): deprecating is promoted to revoked even on success/fail
    state.record_contact_sync_health_batch(
        &mut service,
        &[("https://dep.example.org/d-dep".to_string(), false)],
        2100,
        Some(password),
    );
    let contacts_3 = state.load_contacts(&mut service, Some(password)).unwrap();
    let bob_3 = contacts_3.iter().find(|c| c.did == bob_did).unwrap();
    assert_eq!(bob_3.syncs[0].status, "revoked");

    let policy_3 = state
        .load_security_policy(&mut service, Some(password))
        .unwrap();
    assert_eq!(policy_3.market_selected_syncs[0].status, "revoked");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_save_and_delete_own_subaccounts_and_reject_self_profile_and_self_trust() {
    let temp_dir = std::env::temp_dir().join(format!(
        "humoco_subaccount_contact_test_{}",
        uuid::Uuid::new_v4()
    ));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let (own_uid, own_vk) = alice_service
        .with_wallet_and_identity(|w, id| {
            (w.get_user_id().to_string(), id.signing_key.verifying_key())
        })
        .unwrap();
    let laden_uid = human_money_core::crypto::create_user_id(&own_vk, Some("laden")).unwrap();
    let privat_uid = human_money_core::crypto::create_user_id(&own_vk, Some("privat")).unwrap();

    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    // 1. Exact own active profile must be rejected
    let self_contact = FrontendContact {
        did: own_uid.clone(),
        profile: FrontendUserProfile {
            id: Some(own_uid.clone()),
            first_name: Some("Ich".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: None,
        desired_hops: None,
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    };
    let self_err = state
        .save_contact(&mut service, self_contact, Some(password))
        .unwrap_err();
    assert!(self_err.to_string().contains("active profile"));

    // 2. Own subaccount with trust_level must be rejected (Self trust forbidden)
    let rated_sub = FrontendContact {
        did: laden_uid.clone(),
        profile: FrontendUserProfile {
            id: Some(laden_uid.clone()),
            first_name: Some("Mein Laden".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: Some(1),
        desired_hops: None,
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    };
    let trust_err = state
        .save_contact(&mut service, rated_sub, Some(password))
        .unwrap_err();
    assert!(trust_err.to_string().contains("Self trust forbidden"));

    // 3. Two distinct own subaccounts (trust_level: None) co-exist without overwriting each other
    let laden_contact = FrontendContact {
        did: laden_uid.clone(),
        profile: FrontendUserProfile {
            id: Some(laden_uid.clone()),
            first_name: Some("Mein Laden".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: None,
        desired_hops: None,
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    };
    let privat_contact = FrontendContact {
        did: privat_uid.clone(),
        profile: FrontendUserProfile {
            id: Some(privat_uid.clone()),
            first_name: Some("Mein Privat".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: None,
        desired_hops: None,
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    };
    state
        .save_contact(&mut service, laden_contact, Some(password))
        .unwrap();
    state
        .save_contact(&mut service, privat_contact, Some(password))
        .unwrap();

    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(
        contacts.len(),
        2,
        "Both own subaccounts must stay in address book"
    );
    assert!(contacts
        .iter()
        .any(|c| c.did == laden_uid && c.trust_level.is_none()));
    assert!(contacts
        .iter()
        .any(|c| c.did == privat_uid && c.trust_level.is_none()));

    // 4. lookup_contact_name resolves exact subaccount name rather than sibling
    let book = state.cache_contacts();
    assert_eq!(
        super::lookup_contact_name(book.as_ref(), &laden_uid).as_deref(),
        Some("Mein Laden")
    );
    assert_eq!(
        super::lookup_contact_name(book.as_ref(), &privat_uid).as_deref(),
        Some("Mein Privat")
    );

    // 5. Deleting one subaccount must leave sibling intact
    state
        .delete_contact(&mut service, &privat_uid, Some(password))
        .unwrap();
    let remaining = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].did, laden_uid);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_encrypted_json_helpers_reject_corrupt_and_locked_state() {
    use human_money_core::app_service::AppFacadeError;

    let temp_dir =
        std::env::temp_dir().join(format!("humoco_json_hardening_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.lock_service().unwrap();

    // Missing keys cleanly return defaults or initialize defaults
    let book: crate::models::FrontendAddressBook = super::load_encrypted_json_or_default(
        &mut service,
        super::CONTACTS_DATA_NAME,
        Some(password),
    )
    .unwrap();
    assert!(book.contacts.is_empty());
    assert!(state
        .load_ui_preferences(&mut service, Some(password))
        .is_ok());
    assert!(state
        .load_security_policy(&mut service, Some(password))
        .is_ok());
    assert!(state
        .load_internal_lifecycle(&mut service, Some(password))
        .is_ok());

    // Missing session / wrong auth propagates error instead of silent fallback
    assert!(matches!(
        state.load_ui_preferences(&mut service, None),
        Err(AppFacadeError::SessionNotActive(_))
    ));
    assert!(matches!(
        state.load_security_policy(&mut service, None),
        Err(AppFacadeError::SessionNotActive(_))
    ));
    assert!(matches!(
        state.load_contacts(&mut service, Some("wrong-password")),
        Err(AppFacadeError::Storage(
            human_money_core::StorageError::AuthenticationFailed
        ))
    ));

    // Corrupt JSON on disk MUST return AppFacadeError::Json (never silent default)
    for key in [
        crate::settings::UI_PREFERENCES_KEY,
        crate::settings::SECURITY_POLICY_KEY,
        super::CONTACTS_DATA_NAME,
        super::WOT_DATA_NAME,
        super::TRANSACTION_HISTORY_KEY,
    ] {
        service
            .save_encrypted_data(key, b"{corrupt-json", Some(password))
            .unwrap();
    }
    service
        .save_local_sidecar_data(
            crate::settings::INTERNAL_LIFECYCLE_STATE_KEY,
            b"{corrupt-json",
            Some(password),
        )
        .unwrap();
    assert!(matches!(
        state.load_ui_preferences(&mut service, Some(password)),
        Err(AppFacadeError::Json(_))
    ));
    assert!(matches!(
        state.load_security_policy(&mut service, Some(password)),
        Err(AppFacadeError::Json(_))
    ));
    assert!(matches!(
        state.load_internal_lifecycle(&mut service, Some(password)),
        Err(AppFacadeError::Json(_))
    ));
    assert!(matches!(
        state.load_contacts(&mut service, Some(password)),
        Err(AppFacadeError::Json(_))
    ));
    assert!(matches!(
        state.load_wot_register(&mut service, Some(password)),
        Err(AppFacadeError::Json(_))
    ));
    assert!(matches!(
        state.load_history_from_disk(&mut service, Some(password)),
        Err(AppFacadeError::Json(_))
    ));

    drop(service);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_generic_revocation_helpers_for_descriptor_and_public_sync() {
    let notice = crate::sync_client::SyncRevocationNotice {
        drop_pub: "aa".repeat(32),
        issued_at: 1000,
        effective_at: 2000,
        reason: Some("Umzug".to_string()),
        replacement: Some(crate::sync_client::SyncReplacementTarget {
            write_url: "https://new.example.org/sync.php".to_string(),
            read_url: "https://new.example.org/d".to_string(),
            drop_pub: "bb".repeat(32),
        }),
        signature: "00".repeat(64),
    };

    // 1. Grace-period ("deprecating", before sunset_at) can still be cancelled on HTTP 404
    let mut desc = FrontendSyncDescriptor::default();
    super::apply_revocation_notice(&mut desc, &notice, 1500);
    assert_eq!(desc.status, "deprecating");
    assert_eq!(desc.sunset_at, Some(2000));
    assert_eq!(desc.revocation_reason.as_deref(), Some("Umzug"));
    assert!(desc.replacement.is_some());
    assert!(super::clear_revocation(&mut desc, "active", 1600));
    assert_eq!(desc.status, "active");
    assert!(desc.sunset_at.is_none());
    assert!(desc.revocation_reason.is_none());
    assert!(desc.replacement.is_none());
    assert!(!super::clear_revocation(&mut desc, "active", 1600));

    // 2. Once now_secs >= sunset_at, even a still-"deprecating" entry becomes sticky and cannot clear
    super::apply_revocation_notice(&mut desc, &notice, 1500);
    assert_eq!(desc.status, "deprecating");
    assert!(!super::clear_revocation(&mut desc, "active", 2000));
    assert_eq!(desc.status, "deprecating");
    assert_eq!(desc.sunset_at, Some(2000));
    assert_eq!(desc.revocation_reason.as_deref(), Some("Umzug"));
    assert!(desc.replacement.is_some());

    // 3. An already-"revoked" endpoint stays permanently sticky regardless of timestamp
    let mut pub_sync = crate::settings::StoredPublicSync::default();
    super::apply_revocation_notice(&mut pub_sync, &notice, 2500);
    assert_eq!(pub_sync.status, "revoked");
    assert_eq!(pub_sync.sunset_at, Some(2000));
    assert!(!super::clear_revocation(&mut pub_sync, "active", 2500));
    assert!(!super::clear_revocation(&mut pub_sync, "active", 1000));
    assert_eq!(pub_sync.status, "revoked");
    assert_eq!(pub_sync.sunset_at, Some(2000));
    assert_eq!(pub_sync.revocation_reason.as_deref(), Some("Umzug"));
    assert!(pub_sync.replacement.is_some());
}

#[test]
fn test_split_config_stores_isolation_and_owner_checked_mutator() {
    let temp_dir =
        std::env::temp_dir().join(format!("humoco_split_cfg_test_{}", uuid::Uuid::new_v4()));
    let pw = "split-cfg-pw-2026";
    let alice_svc = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    );
    let alice_did = alice_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let state = crate::test_support::create_test_state(alice_svc);

    {
        let mut s = state.lock_service().unwrap();
        state
            .mutate_ui_preferences(&mut s, Some(pw), |p| {
                p.ui_mode = "expert".to_string();
            })
            .unwrap();
        state
            .mutate_security_policy(&mut s, Some(pw), |p| {
                p.session_timeout_seconds = 900;
            })
            .unwrap();
        state
            .mutate_internal_lifecycle(&mut s, Some(pw), |lc| {
                lc.pending_restore_l2_reconciliation = true;
            })
            .unwrap();
    }

    let session = crate::session::SessionToken {
        owner_did: alice_did.clone(),
        generation: state.current_session_generation(),
    };

    // Same owner & current generation: clears flag without clobbering UiPreferences or SecurityPolicy
    let updated = state
        .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
            lc.pending_restore_l2_reconciliation = false;
            lc.last_sync_backup_at = Some("2026-11-19T12:00:00Z".to_string());
            true
        })
        .unwrap()
        .expect("matching session must mutate lifecycle");
    assert!(!updated.pending_restore_l2_reconciliation);
    assert_eq!(
        updated.last_sync_backup_at.as_deref(),
        Some("2026-11-19T12:00:00Z")
    );

    let cfg = state.get_cached_app_config().unwrap();
    assert_eq!(cfg.ui_preferences.ui_mode, "expert");
    assert_eq!(cfg.security_policy.session_timeout_seconds, 900);

    // Stale owner DID: returns Ok(None) and does NOT touch store
    let stale_owner_session = crate::session::SessionToken {
        owner_did: "did:key:zStaleOwner".to_string(),
        generation: session.generation,
    };
    let stale = state
        .mutate_internal_lifecycle_for_session(&stale_owner_session, Some(pw), |lc| {
            lc.backup_dirty = true;
            true
        })
        .unwrap();
    assert!(stale.is_none());
    assert!(!state.get_cached_internal_lifecycle().unwrap().backup_dirty);

    // Stale generation: returns Ok(None) and does NOT touch store
    let stale_gen_session = crate::session::SessionToken {
        owner_did: alice_did,
        generation: session.generation.saturating_add(99),
    };
    let stale_gen = state
        .mutate_internal_lifecycle_for_session(&stale_gen_session, Some(pw), |lc| {
            lc.backup_dirty = true;
            true
        })
        .unwrap();
    assert!(stale_gen.is_none());

    // Logged-out / locked wallet: returns Ok(None) and does NOT touch store
    state.lock_service().unwrap().logout();
    let locked = state
        .mutate_internal_lifecycle_for_session(&session, Some(pw), |lc| {
            lc.backup_dirty = true;
            true
        })
        .unwrap();
    assert!(locked.is_none());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_canonicalize_did_rejects_malformed_prefixes_and_checksums() {
    let kp = human_money_core::test_utils::ACTORS.bob.identity.clone();
    let root_did = human_money_core::crypto::create_user_id(&kp.public_key, None).unwrap();
    let valid_sai =
        human_money_core::crypto::create_user_id(&kp.public_key, Some("shop-1")).unwrap();

    assert_eq!(super::canonicalize_did(&root_did).unwrap(), root_did);
    assert_eq!(
        super::canonicalize_did(&format!("  {} \n", valid_sai)).unwrap(),
        root_did
    );

    let broken_variants = [
        format!("fake@{}", root_did),
        format!("shop-1:BAD@{}", root_did),
        format!(":abc@{}", root_did),
        format!("@{}", root_did),
        format!("a@b:abc@{}", root_did),
        format!("Shop:abc@{}", root_did),
        format!("{}\n{}", &root_did[..12], &root_did[12..]),
    ];
    for invalid in &broken_variants {
        assert!(
            super::canonicalize_did(invalid).is_err(),
            "canonicalize_did must reject malformed input '{}'",
            invalid
        );
    }
}

#[test]
fn test_external_sibling_prefixes_keep_separate_syncs_mirror_trust_and_delete_cleanly() {
    use crate::models::{FrontendContact, FrontendSyncDescriptor, FrontendUserProfile};
    use chrono::Utc;

    let temp_dir = std::env::temp_dir().join(format!(
        "humoco_ext_sibling_prefix_{}",
        uuid::Uuid::new_v4()
    ));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    let bob_kp = human_money_core::test_utils::ACTORS.bob.identity.clone();
    let bob_sk = bob_kp.signing_key.clone();
    let bob_vk = bob_sk.verifying_key();
    let bob_laden = human_money_core::crypto::create_user_id(&bob_vk, Some("laden")).unwrap();
    let bob_privat = human_money_core::crypto::create_user_id(&bob_vk, Some("privat")).unwrap();
    let bob_can = super::canonicalize_did(&bob_laden).unwrap();

    // Populate CardStore with two distinct prefix cards for Bob
    let now = Utc::now().timestamp().max(0) as u64;
    let mut card_store = humoco_market::CardStore::default();
    for (did, url) in [
        (&bob_laden, "https://laden.example.com/sync.php"),
        (&bob_privat, "https://privat.example.com/sync.php"),
    ] {
        let card = humoco_market::sign_sd_card(
            humoco_market::CardPayloadInput {
                did: did.clone(),
                v: 100,
                valid_until: now + 86_400,
                max_hops: 2,
                ad_count: 0,
                allow_inquiries: false,
                drops: vec![humoco_market::DropDescriptor {
                    write_url: url.to_string(),
                    read_url: url.to_string(),
                    slot_id: None,
                    drop_pub: crate::test_support::test_ed25519_pub_hex(0x0a),
                }],
            },
            vec![(
                humoco_market::CardItem::Name {
                    first_name: Some("Bob".to_string()),
                    last_name: None,
                },
                1,
            )],
            &bob_sk,
        )
        .unwrap();
        let wire = humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap();
        humoco_market::accept_envelope(
            &mut card_store,
            &humoco_market::CardEnvelope {
                card_wire: wire,
                hops: 1,
            },
            now,
            100,
            None,
            &|_| false,
        );
    }
    super::save_encrypted_json(
        &mut service,
        super::MARKET_CARD_STORE_KEY,
        &card_store,
        Some(password),
    )
    .unwrap();

    let mk_sync = |url: &str| FrontendSyncDescriptor {
        write_url: url.to_string(),
        read_url: format!("{}/read", url),
        drop_pub: crate::test_support::test_ed25519_pub_hex(0x0b),
        status: "active".to_string(),
        ..Default::default()
    };
    let mk_ext = |did: &str, name: &str, sync_url: &str, trust_level: Option<u8>| FrontendContact {
        did: did.to_string(),
        profile: FrontendUserProfile {
            id: Some(did.to_string()),
            first_name: Some(name.to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level,
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![mk_sync(sync_url)],
    };

    // 1. Save both external prefixes with distinct sync endpoints
    state
        .save_contact(
            &mut service,
            mk_ext(
                &bob_laden,
                "Bob Laden",
                "https://sync-laden.example.com",
                Some(2),
            ),
            Some(password),
        )
        .unwrap();
    state
        .save_contact(
            &mut service,
            mk_ext(
                &bob_privat,
                "Bob Privat",
                "https://sync-privat.example.com",
                None,
            ),
            Some(password),
        )
        .unwrap();

    let contacts = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(contacts.len(), 2);
    assert_eq!(
        contacts
            .iter()
            .find(|c| c.did == bob_laden)
            .unwrap()
            .trust_level,
        Some(2)
    );
    assert_eq!(
        contacts
            .iter()
            .find(|c| c.did == bob_privat)
            .unwrap()
            .trust_level,
        Some(2)
    );
    assert_eq!(
        state
            .get_or_load_wot_register(&mut service, Some(password))
            .unwrap()
            .own_contacts
            .get(&bob_can)
            .map(|e| e.level),
        Some(2)
    );
    assert_eq!(
        contacts.iter().find(|c| c.did == bob_laden).unwrap().syncs[0].write_url,
        "https://sync-laden.example.com"
    );
    assert_eq!(
        contacts.iter().find(|c| c.did == bob_privat).unwrap().syncs[0].write_url,
        "https://sync-privat.example.com"
    );

    // 2. Updating trust on one prefix atomically mirrors trust_level to sibling on disk & cache
    state
        .save_contact(
            &mut service,
            mk_ext(
                &bob_laden,
                "Bob Laden",
                "https://sync-laden.example.com",
                Some(3),
            ),
            Some(password),
        )
        .unwrap();
    let disk_book: crate::models::FrontendAddressBook = super::load_encrypted_json_or_default(
        &mut service,
        super::CONTACTS_DATA_NAME,
        Some(password),
    )
    .unwrap();
    assert_eq!(
        disk_book.contacts.get(&bob_laden).unwrap().trust_level,
        Some(3)
    );
    assert_eq!(
        disk_book.contacts.get(&bob_privat).unwrap().trust_level,
        Some(3)
    );

    // 3. update_contact_from_card for bob_privat never mutates bob_laden syncs
    let updated = state
        .update_contact_from_card(
            &mut service,
            &bob_privat,
            &[mk_sync("https://sync-privat-2.example.com")],
            None,
            Some(password),
        )
        .unwrap();
    assert!(updated);
    let after_card = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(
        after_card
            .iter()
            .find(|c| c.did == bob_laden)
            .unwrap()
            .syncs
            .len(),
        1
    );
    assert_eq!(
        after_card
            .iter()
            .find(|c| c.did == bob_privat)
            .unwrap()
            .syncs
            .len(),
        2
    );

    // 4. Deleting bob_laden keeps bob_privat, root WoT trust, and remaining CardStore intact
    state
        .delete_contact(&mut service, &bob_laden, Some(password))
        .unwrap();
    let remaining = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].did, bob_privat);
    let reg_mid = state
        .get_or_load_wot_register(&mut service, Some(password))
        .unwrap();
    assert_eq!(reg_mid.own_contacts.get(&bob_can).map(|e| e.level), Some(3));
    let store_mid: humoco_market::CardStore = super::load_encrypted_json_or_default(
        &mut service,
        super::MARKET_CARD_STORE_KEY,
        Some(password),
    )
    .unwrap();
    assert_eq!(store_mid.cards.len(), 2);

    // 5. Deleting last sibling removes root WoT trust AND prunes ALL prefix cards of Bob's key
    state
        .delete_contact(&mut service, &bob_privat, Some(password))
        .unwrap();
    let reg_end = state
        .get_or_load_wot_register(&mut service, Some(password))
        .unwrap();
    assert!(!reg_end.own_contacts.contains_key(&bob_can));
    let store_end: humoco_market::CardStore = super::load_encrypted_json_or_default(
        &mut service,
        super::MARKET_CARD_STORE_KEY,
        Some(password),
    )
    .unwrap();
    assert!(store_end.cards.is_empty());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_known_lock_heads_sidecar_persists_without_advancing_seal_nonce() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::models::layer2_api::{LockReceipt, LockReceiptKind};

    let temp_dir = std::env::temp_dir().join(format!(
        "humoco_known_lock_heads_test_{}",
        uuid::Uuid::new_v4()
    ));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Test Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    let seal_before = service.get_current_seal().unwrap();
    let lock_sk = SigningKey::from_bytes(&[0x11u8; 32]);
    let lock_pub = lock_sk.verifying_key().to_bytes();
    let anchor_id = [0x99u8; 32];
    let mut receipt = LockReceipt {
        kind: LockReceiptKind::FirstWrite,
        lock_pub,
        bucket: human_money_core::humoco_proto::derive_bucket(&anchor_id, &lock_pub),
        slot: [0x33u8; 32],
        child_lock: [0x44u8; 32],
        entry_hash: [0x55u8; 32],
        retain_until_quarter: 20271,
        write_seq: 1,
        bucket_head: [0x66u8; 32],
        recipient_nonce: [0x77u8; 32],
        sig: [0u8; 64],
    };
    let digest = receipt.digest();
    receipt.sig = lock_sk.sign(&digest).to_bytes();

    super::record_known_head_receipts(
        &mut service,
        &anchor_id,
        std::slice::from_ref(&receipt),
        Some(password),
    )
    .unwrap();

    let loaded =
        super::load_known_head_receipts_for_anchor(&mut service, &anchor_id, Some(password))
            .expect("valid sidecar should load");
    assert_eq!(loaded.get(&lock_pub), Some(&receipt));

    assert!(service
        .load_local_sidecar_data(super::KNOWN_LOCK_HEADS_KEY, Some(password))
        .is_ok());
    assert!(service
        .load_encrypted_data(super::KNOWN_LOCK_HEADS_KEY, Some(password))
        .is_err());

    // Corrupt JSON inside the sidecar must fail closed rather than silently returning empty map.
    service
        .save_local_sidecar_data(
            super::KNOWN_LOCK_HEADS_KEY,
            b"{corrupt-sidecar",
            Some(password),
        )
        .unwrap();
    assert!(
        super::load_known_head_receipts_for_anchor(&mut service, &anchor_id, Some(password))
            .is_err(),
        "Corrupt known-lock-heads sidecar must fail closed"
    );
    assert!(
        super::record_known_head_receipts(
            &mut service,
            &anchor_id,
            std::slice::from_ref(&receipt),
            Some(password),
        )
        .is_err(),
        "Recording against corrupt known-lock-heads sidecar must also fail closed"
    );

    let seal_after = service.get_current_seal().unwrap();
    assert_eq!(
        serde_json::to_value(&seal_after).unwrap(),
        serde_json::to_value(&seal_before).unwrap()
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_profile_session_lock_cross_instance_raii_and_validation() {
    let temp_dir =
        std::env::temp_dir().join(format!("humoco_session_lock_test_{}", uuid::Uuid::new_v4()));
    let lock_dir = temp_dir.join("session_locks");

    let s1 = super::AppService::new(&temp_dir.join("w1")).unwrap();
    let s2 = super::AppService::new(&temp_dir.join("w2")).unwrap();
    let state1 = super::AppState::new(s1).with_session_lock_dir(lock_dir.clone());
    let state2 = super::AppState::new(s2).with_session_lock_dir(lock_dir.clone());

    // 1. Instance 1 acquires and commits profile_a
    let pending1 = state1.acquire_profile_lock("profile_a").unwrap();
    state1.commit_profile_lock(pending1);

    // 2. Instance 2 is blocked on profile_a while Instance 1 holds the RAII lock
    let err = state2.acquire_profile_lock("profile_a").err().unwrap();
    assert_eq!(err.code, "error.auth.profileAlreadyOpen");

    // 3. Instance 2 can open profile_b in parallel
    let pending_b = state2.acquire_profile_lock("profile_b").unwrap();
    state2.commit_profile_lock(pending_b);
    state2.release_profile_lock();

    // 4. Idempotency: Instance 1 re-acquiring its own active profile succeeds without blocking
    let reacquire = state1.acquire_profile_lock("profile_a").unwrap();
    assert!(matches!(reacquire, super::PendingProfileLock::AlreadyHeld));
    state1.commit_profile_lock(reacquire);
    assert_eq!(
        state2.acquire_profile_lock("profile_a").err().unwrap().code,
        "error.auth.profileAlreadyOpen"
    );

    // 5. After Instance 1 releases, Instance 2 can acquire and RAII-drop rolls back
    state1.release_profile_lock();
    {
        let uncommitted = state2.acquire_profile_lock("profile_a").unwrap();
        assert!(matches!(
            uncommitted,
            super::PendingProfileLock::Acquired(_)
        ));
        // Dropped here without commit -> lock released immediately
    }
    let after_rollback = state1.acquire_profile_lock("profile_a").unwrap();
    state1.commit_profile_lock(after_rollback);
    state1.release_profile_lock_if_matching("profile_a");
    // K2: flock-backed .lock file stays on disk and is never unlinked while Instance 2 can acquire cleanly.
    assert!(lock_dir.join("profile_a.lock").exists());
    assert!(state2.acquire_profile_lock("profile_a").is_ok());

    // 6. Path traversal / invalid folder names are rejected
    assert!(state1.acquire_profile_lock("../escape").is_err());
    assert!(state1.acquire_profile_lock("a/b").is_err());
    assert!(state1.acquire_profile_lock("").is_err());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn h1_save_contact_preserve_manual_card_keeps_card_on_trust_reset() {
    use humoco_market::CardStore;

    let dir_alice = crate::test_support::temp_test_dir("h1_card_alice");
    let dir_bob = crate::test_support::temp_test_dir("h1_card_bob");
    let pw = "StrongPw-H1-Card!";

    let alice_state = super::AppState::new(crate::test_support::setup_test_app(
        &dir_alice,
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    ));
    let bob_state = super::AppState::new(crate::test_support::setup_test_app(
        &dir_bob,
        "Bob",
        crate::test_support::BOB_PHRASE,
        pw,
    ));
    let alice_did = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let prof = crate::services::market_service::save_my_market_profile_impl(
        crate::models::FrontendSaveMyMarketProfileRequest {
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(true),
        },
        Some(pw.to_string()),
        &alice_state,
    )
    .unwrap();
    let share_uri = prof.share_uri.unwrap();

    crate::services::market_service::import_contact_card_impl(
        share_uri,
        Some(pw.to_string()),
        &bob_state,
    )
    .unwrap();

    {
        let mut s = bob_state.service.lock().unwrap();
        let trusted = crate::models::FrontendContact {
            did: alice_did.clone(),
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        bob_state
            .save_contact_with_options(&mut s, trusted, Some(pw), true)
            .unwrap();

        let reset = crate::models::FrontendContact {
            did: alice_did.clone(),
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };
        bob_state
            .save_contact_with_options(&mut s, reset, Some(pw), true)
            .unwrap();

        let store: CardStore =
            super::load_encrypted_json_opt(&mut s, super::MARKET_CARD_STORE_KEY, Some(pw))
                .unwrap()
                .unwrap();
        let can_alice = super::canonicalize_did(&alice_did).unwrap();
        assert!(
            store.cards.values().any(|sc| {
                super::canonicalize_did(&sc.card.payload.did)
                    .ok()
                    .as_deref()
                    == Some(can_alice.as_str())
            }),
            "H1: preserve_manual_card=true must keep manually imported card in CardStore"
        );
    }

    let _ = std::fs::remove_dir_all(&dir_alice);
    let _ = std::fs::remove_dir_all(&dir_bob);
}

#[test]
fn s03_bucket_binding_wot_delete_integrity_and_utf8_truncate() {
    use ed25519_dalek::{Signer, SigningKey};
    use human_money_core::models::layer2_api::{LockReceipt, LockReceiptKind};

    // 1. UTF-8 safe truncate_did (multi-byte char boundaries at byte 8 and len - 4)
    let utf8_did = "äöüäöüäöüäöüäöüäö";
    assert_eq!(super::truncate_did(utf8_did), "äöüäöüäö…öüäö");

    // 2. Bucket binding on record_known_head_receipts & load_known_head_receipts_for_anchor
    let dir = crate::test_support::temp_test_dir("s03_state_hardening");
    let pw = "StrongPw-S03-State!";
    let alice_svc = crate::test_support::setup_test_app(
        &dir.join("alice"),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    );
    let bob_svc = crate::test_support::setup_test_app(
        &dir.join("bob"),
        "Bob",
        crate::test_support::BOB_PHRASE,
        pw,
    );
    let bob_did = bob_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    let state = crate::test_support::create_test_state(alice_svc);
    let bob_state = crate::test_support::create_test_state(bob_svc);

    // Publish own market profile for Alice (so we can verify card version bump on delete)
    crate::services::market_service::save_my_market_profile_impl(
        crate::models::FrontendSaveMyMarketProfileRequest {
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(true),
        },
        Some(pw.to_string()),
        &state,
    )
    .unwrap();

    // Publish Bob's market profile and construct a CardEnvelope claiming hops = 2
    crate::services::market_service::save_my_market_profile_impl(
        crate::models::FrontendSaveMyMarketProfileRequest {
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            visibility: Default::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(true),
        },
        Some(pw.to_string()),
        &bob_state,
    )
    .unwrap();
    let bob_card = {
        let mut bs = bob_state.service.lock().unwrap();
        super::load_encrypted_json_opt::<crate::models::StoredOwnMarketProfile>(
            &mut bs,
            super::MARKET_OWN_PROFILE_KEY,
            Some(pw),
        )
        .unwrap()
        .unwrap()
        .last_card
        .unwrap()
    };
    let bob_wire =
        humoco_market::render_wire(&bob_card, 2, humoco_market::WireFilter::Full).unwrap();
    let bob_env_json = serde_json::to_string(&humoco_market::CardEnvelope {
        card_wire: bob_wire,
        hops: 2,
    })
    .unwrap();

    let mut service = state.service.lock().unwrap();

    let lock_sk = SigningKey::from_bytes(&[0x33u8; 32]);
    let lock_pub = lock_sk.verifying_key().to_bytes();
    let anchor_a = [0xAAu8; 32];
    let anchor_b = [0xBBu8; 32];
    let wrong_bucket = human_money_core::humoco_proto::derive_bucket(&anchor_b, &lock_pub);

    let mut wrong_receipt = LockReceipt {
        kind: LockReceiptKind::FirstWrite,
        lock_pub,
        bucket: wrong_bucket,
        slot: [0x12u8; 32],
        child_lock: [0x34u8; 32],
        entry_hash: [0x56u8; 32],
        retain_until_quarter: 20271,
        write_seq: 1,
        bucket_head: [0x78u8; 32],
        recipient_nonce: [0x9Au8; 32],
        sig: [0u8; 64],
    };
    wrong_receipt.sig = lock_sk.sign(&wrong_receipt.digest()).to_bytes();

    super::record_known_head_receipts(
        &mut service,
        &anchor_a,
        std::slice::from_ref(&wrong_receipt),
        Some(pw),
    )
    .unwrap();
    let loaded =
        super::load_known_head_receipts_for_anchor(&mut service, &anchor_a, Some(pw)).unwrap();
    assert!(
        loaded.is_empty(),
        "Receipt bound to anchor_b bucket must NOT be recorded under anchor_a"
    );

    // Persisted wrong-bucket receipt inside sidecar must fail closed on load
    let mut tampered_heads = super::KnownLockHeadsState::default();
    tampered_heads.heads.insert(
        format!(
            "{}:{}",
            human_money_core::hex::encode(anchor_a),
            human_money_core::hex::encode(lock_pub)
        ),
        wrong_receipt,
    );
    super::save_local_sidecar_json(
        &mut service,
        super::KNOWN_LOCK_HEADS_KEY,
        &tampered_heads,
        Some(pw),
    )
    .unwrap();
    assert!(
        super::load_known_head_receipts_for_anchor(&mut service, &anchor_a, Some(pw)).is_err(),
        "Persisted wrong-bucket receipt must fail closed on load"
    );
    // Clean sidecar back to default
    super::save_local_sidecar_json(
        &mut service,
        super::KNOWN_LOCK_HEADS_KEY,
        &super::KnownLockHeadsState::default(),
        Some(pw),
    )
    .unwrap();

    // 3. try_import_piggybacked_contact_card clamps hops to 1 when bundle_sender_id matches author
    state.try_import_piggybacked_contact_card(
        &mut service,
        Some(&bob_env_json),
        Some(&bob_did),
        Some(pw),
    );
    let card_store: humoco_market::CardStore =
        super::load_encrypted_json_opt(&mut service, super::MARKET_CARD_STORE_KEY, Some(pw))
            .unwrap()
            .unwrap();
    assert_eq!(
        card_store.cards.get(&bob_did).map(|c| c.hops),
        Some(1),
        "Direct bundle_sender_id matching card author must clamp stored hops to 1"
    );

    // 4. delete_contact_with_options fails atomically without deleting address book when wot_register is corrupt
    let bob_contact = crate::models::FrontendContact {
        did: bob_did.clone(),
        profile: crate::models::FrontendUserProfile {
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: chrono::Utc::now().to_rfc3339(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(2),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    };
    state
        .save_contact(&mut service, bob_contact, Some(pw))
        .unwrap();

    // Snapshot valid WoT state & card version before corruption test
    let valid_wot = state.load_wot_register(&mut service, Some(pw)).unwrap();
    let card_v_before = super::load_encrypted_json_opt::<crate::models::StoredOwnMarketProfile>(
        &mut service,
        super::MARKET_OWN_PROFILE_KEY,
        Some(pw),
    )
    .unwrap()
    .unwrap()
    .last_card
    .unwrap()
    .payload
    .v;

    // Corrupt WOT_DATA_NAME on disk and drop cached wot so reload fails
    service
        .save_encrypted_data(super::WOT_DATA_NAME, b"{corrupt-wot-json", Some(pw))
        .unwrap();
    state.write_cache_slot_for_service(&service, |c| &mut c.wot, None);

    let del_res = state.delete_contact(&mut service, &bob_did, Some(pw));
    assert!(
        del_res.is_err(),
        "delete_contact must propagate WoT load/save error instead of silently leaving trusted"
    );
    let contacts_after = state.load_contacts(&mut service, Some(pw)).unwrap();
    assert!(
        contacts_after.iter().any(|c| c.did == bob_did),
        "Contact must remain in address book when WoT revocation fails"
    );

    // Restore valid WoT state and delete trusted contact -> must remove contact, revoke WoT, AND bump own card version
    state
        .save_wot_register(&mut service, &valid_wot, Some(pw))
        .unwrap();
    state
        .delete_contact(&mut service, &bob_did, Some(pw))
        .unwrap();
    let contacts_final = state.load_contacts(&mut service, Some(pw)).unwrap();
    assert!(!contacts_final.iter().any(|c| c.did == bob_did));
    let card_v_after = super::load_encrypted_json_opt::<crate::models::StoredOwnMarketProfile>(
        &mut service,
        super::MARKET_OWN_PROFILE_KEY,
        Some(pw),
    )
    .unwrap()
    .unwrap()
    .last_card
    .unwrap()
    .payload
    .v;
    assert!(
        card_v_after > card_v_before,
        "Deleting a trusted contact must bump own card version (before={card_v_before}, after={card_v_after})"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s09_x4_stale_generation_caches_are_isolated_and_rejected() {
    use std::sync::atomic::Ordering;

    let dir = crate::test_support::temp_test_dir("s09_x4_stale_gen");
    let pw = "StrongPw-S09-X4!";
    let alice_svc = crate::test_support::setup_test_app(
        &dir.join("alice"),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    );
    let alice_did = alice_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let state = crate::test_support::create_test_state(alice_svc);
    {
        let mut svc = state.lock_service().unwrap();
        state.initialize_profile_session(&mut svc, pw).unwrap();
    }

    let stale_gen = state.current_session_generation();
    let mut book = crate::models::FrontendAddressBook::default();
    book.contacts.insert(
        "did:key:zBob".to_string(),
        crate::models::FrontendContact {
            did: "did:key:zBob".to_string(),
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        },
    );
    let wot_reg = humoco_wot::TrustRegister::new(
        &super::canonicalize_did(&alice_did).unwrap(),
        humoco_wot::WotPolicy::default(),
    )
    .unwrap();

    {
        let mut guard = state.profile_caches.lock().unwrap();
        let caches = guard.as_mut().unwrap();
        assert_eq!(caches.generation, stale_gen);
        caches.contacts = Some(book);
        caches.events = Some(vec![]);
        caches.wot = Some(wot_reg);
        caches.ui_preferences = Some(crate::settings::UiPreferences {
            ui_mode: "expert".to_string(),
            ..Default::default()
        });
        caches.security_policy = Some(crate::settings::SecurityPolicy {
            session_timeout_seconds: 777,
            ..Default::default()
        });
        caches.internal_lifecycle = Some(crate::settings::InternalLifecycleState {
            backup_dirty: true,
            ..Default::default()
        });
        caches.history = Some(vec![]);
        caches.own_market_profile = Some(crate::models::StoredOwnMarketProfile::default());
        caches.recovery_owner = Some(None);
        caches.guardian_vault = Some(humoco_market::GuardianVaultStore::default());
        caches.app_notices = Some(crate::models::StoredAppNotices::default());
    }

    // Advance session_generation without updating ActiveProfileCaches.generation
    // (simulates a stale-generation cache struct).
    state.session_generation.fetch_add(1, Ordering::SeqCst);

    assert!(state.cache_contacts().is_none());
    assert!(state.cache_events().is_none());
    assert!(state.cache_wot().is_none());
    assert!(state.cache_ui_preferences().is_none());
    assert!(state.cache_security_policy().is_none());
    assert!(state.cache_internal_lifecycle().is_none());
    assert!(state.cache_history().is_none());
    assert!(state.cache_own_market_profile().is_none());
    assert!(state.cache_recovery_owner().is_none());
    assert!(state.cache_guardian_vault().is_none());
    assert!(state.cache_app_notices().is_none());

    assert!(state.get_cached_app_config().is_err());
    assert!(state.get_cached_contacts().is_err());
    assert!(state.get_hydrated_history().is_err());

    let saw_stale = state.with_cached_contacts_mut(|opt| {
        let saw = opt.is_some();
        *opt = None;
        saw
    });
    assert!(
        !saw_stale,
        "with_cached_contacts_mut must not read stale contacts"
    );

    state.set_cached_contacts(None);
    state.set_cached_security_policy(None);
    state.set_cached_security_policy(Some(crate::settings::SecurityPolicy {
        session_timeout_seconds: 999,
        ..Default::default()
    }));
    {
        let guard = state.profile_caches.lock().unwrap();
        let stale_caches = guard.as_ref().unwrap();
        assert!(
            stale_caches.contacts.is_some(),
            "stale-generation ActiveProfileCaches.contacts must not be mutated"
        );
        assert_eq!(
            stale_caches
                .security_policy
                .as_ref()
                .map(|p| p.session_timeout_seconds),
            Some(777),
            "stale-generation ActiveProfileCaches.security_policy must not be mutated"
        );
    }

    state.clear_all_caches();
    let saw_none = state.with_cached_contacts_mut(|opt| {
        let saw = opt.is_some();
        *opt = Some(crate::models::FrontendAddressBook::default());
        saw
    });
    assert!(!saw_none);
    assert!(state.profile_caches.lock().unwrap().is_none());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s09_encrypted_batch_commit_atomicity_and_rollback() {
    let dir = crate::test_support::temp_test_dir("s09_batch_commit_rollback");
    let pw = "StrongPw-S09-Batch!";
    let alice_svc = crate::test_support::setup_test_app(
        &dir.join("alice"),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    );
    let state = crate::test_support::create_test_state(alice_svc);
    let mut svc = state.lock_service().unwrap();
    state.initialize_profile_session(&mut svc, pw).unwrap();

    let initial_policy = crate::settings::SecurityPolicy {
        session_timeout_seconds: 300,
        ..Default::default()
    };
    let initial_prefs = crate::settings::UiPreferences {
        ui_mode: "simple".to_string(),
        ..Default::default()
    };
    let initial_profile = crate::models::StoredOwnMarketProfile {
        max_hops: 1,
        published: false,
        ..Default::default()
    };
    let initial_admin = super::StoredSyncAdmin {
        descriptor: humoco_market::DropDescriptor {
            write_url: "https://old.example.org/sync.php".to_string(),
            read_url: "https://old.example.org/d".to_string(),
            slot_id: None,
            drop_pub: "aa".repeat(32),
        },
        sync_index: 0,
        data_dir: "d".to_string(),
        max_members: 100,
        max_total_storage_mb: 100,
        max_user_storage_mb: 10,
        policies: Default::default(),
        allow: vec![],
        last_members_hash: Default::default(),
        revocation: None,
    };
    let mut initial_book = crate::models::FrontendAddressBook::default();
    initial_book.contacts.insert(
        "did:key:zInitial".to_string(),
        crate::models::FrontendContact {
            did: "did:key:zInitial".to_string(),
            profile: crate::models::FrontendUserProfile {
                first_name: Some("Initial".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: chrono::Utc::now().to_rfc3339(),
            notes: None,
            trust_level: None,
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        },
    );

    let mut init_batch = super::EncryptedBatchCommit::new();
    init_batch
        .security_policy(initial_policy)
        .ui_preferences(initial_prefs)
        .own_market_profile(initial_profile)
        .sync_admin(initial_admin)
        .contacts(initial_book);
    init_batch.commit(&state, &mut svc, Some(pw)).unwrap();

    assert_eq!(
        state
            .cache_security_policy()
            .map(|p| p.session_timeout_seconds),
        Some(300)
    );
    assert_eq!(
        state.cache_ui_preferences().map(|p| p.ui_mode),
        Some("simple".to_string())
    );
    assert_eq!(
        state.cache_own_market_profile().map(|p| p.max_hops),
        Some(1)
    );
    assert_eq!(
        state
            .read_cache_slot_for_service(&svc, |c| &c.own_sync_descriptor)
            .flatten()
            .map(|d| d.write_url),
        Some("https://old.example.org/sync.php".to_string())
    );
    assert!(state
        .cache_contacts()
        .is_some_and(|ab| ab.contacts.contains_key("did:key:zInitial")));

    // Attempt a batch commit that fails in save_encrypted_batch (wrong password):
    // RAM caches and disk must remain completely unchanged.
    let mut failing_batch = super::EncryptedBatchCommit::new();
    failing_batch
        .security_policy(crate::settings::SecurityPolicy {
            session_timeout_seconds: 999,
            ..Default::default()
        })
        .ui_preferences(crate::settings::UiPreferences {
            ui_mode: "expert".to_string(),
            ..Default::default()
        })
        .own_market_profile(crate::models::StoredOwnMarketProfile {
            max_hops: 3,
            published: true,
            ..Default::default()
        })
        .clear_sync_admin()
        .contacts(crate::models::FrontendAddressBook::default());
    let err = failing_batch
        .commit(&state, &mut svc, Some("wrong-password"))
        .unwrap_err();
    assert!(
        matches!(
            err,
            super::AppFacadeError::Storage(human_money_core::StorageError::AuthenticationFailed)
        ),
        "unexpected commit failure variant: {err:?}"
    );

    assert_eq!(
        state
            .cache_security_policy()
            .map(|p| p.session_timeout_seconds),
        Some(300),
        "failed EncryptedBatchCommit must not update security_policy RAM cache"
    );
    assert_eq!(
        state.cache_ui_preferences().map(|p| p.ui_mode),
        Some("simple".to_string()),
        "failed EncryptedBatchCommit must not update ui_preferences RAM cache"
    );
    assert_eq!(
        state.cache_own_market_profile().map(|p| p.max_hops),
        Some(1),
        "failed EncryptedBatchCommit must not update own_market_profile RAM cache"
    );
    assert_eq!(
        state
            .read_cache_slot_for_service(&svc, |c| &c.own_sync_descriptor)
            .flatten()
            .map(|d| d.write_url),
        Some("https://old.example.org/sync.php".to_string()),
        "failed EncryptedBatchCommit must not clear own_sync_descriptor RAM cache"
    );
    assert!(
        state
            .cache_contacts()
            .is_some_and(|ab| ab.contacts.contains_key("did:key:zInitial")),
        "failed EncryptedBatchCommit must not update contacts RAM cache"
    );

    // Successful clear_sync_admin updates own_sync_descriptor to Some(None) atomically.
    let mut clear_batch = super::EncryptedBatchCommit::new();
    clear_batch.clear_sync_admin();
    clear_batch.commit(&state, &mut svc, Some(pw)).unwrap();
    assert_eq!(
        state.read_cache_slot_for_service(&svc, |c| &c.own_sync_descriptor),
        Some(None)
    );

    // AuthorizedSession accessors and Debug redaction.
    drop(svc);
    let auth = state
        .authorize_session(super::RequiredAccess::Full, Some(pw))
        .unwrap();
    assert_eq!(auth.pw(), Some(pw));
    assert_eq!(auth.generation(), state.current_session_generation());
    assert!(!auth.owner_did().is_empty());
    assert!(!format!("{auth:?}").contains(pw));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s11_x3_save_contact_rejects_invalid_sync_descriptors_and_allows_unprobed_qr() {
    let temp_dir =
        std::env::temp_dir().join(format!("humoco_s11_x3_save_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let bob_service = crate::test_support::setup_test_app(
        &temp_dir.join("bob"),
        "Bob",
        crate::test_support::BOB_PHRASE,
        password,
    );
    let bob_did = bob_service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    let mk_contact = |syncs: Vec<FrontendSyncDescriptor>| FrontendContact {
        did: bob_did.clone(),
        profile: FrontendUserProfile {
            id: Some(bob_did.clone()),
            first_name: Some("Bob".to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level: Some(2),
        desired_hops: Some(1),
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs,
    };

    let valid_pub = crate::test_support::test_ed25519_pub_hex(0x41);

    // 1. Reject weak small-order Ed25519 drop_pub
    let err_weak = state
        .save_contact_with_options(
            &mut service,
            mk_contact(vec![FrontendSyncDescriptor {
                write_url: "https://peer.example.org/sync.php".to_string(),
                read_url: "https://peer.example.org/d".to_string(),
                drop_pub: "00".repeat(32),
                status: "active".to_string(),
                ..Default::default()
            }]),
            Some(password),
            false,
        )
        .unwrap_err();
    assert!(matches!(
        err_weak,
        super::AppFacadeError::ValidationFailed(_)
    ));

    // 2. Reject SSRF private/link-local IP URL
    let err_ssrf = state
        .save_contact_with_options(
            &mut service,
            mk_contact(vec![FrontendSyncDescriptor {
                write_url: "https://169.254.169.254/sync.php".to_string(),
                read_url: "https://169.254.169.254/d".to_string(),
                drop_pub: valid_pub.clone(),
                status: "active".to_string(),
                ..Default::default()
            }]),
            Some(password),
            false,
        )
        .unwrap_err();
    assert!(matches!(
        err_ssrf,
        super::AppFacadeError::ValidationFailed(_)
    ));

    // 3. Reject malformed slot_id
    let err_slot = state
        .save_contact_with_options(
            &mut service,
            mk_contact(vec![FrontendSyncDescriptor {
                write_url: "https://peer.example.org/sync.php".to_string(),
                read_url: "https://peer.example.org/d".to_string(),
                slot_id: Some("not-a-valid-slot".to_string()),
                drop_pub: valid_pub.clone(),
                status: "active".to_string(),
                ..Default::default()
            }]),
            Some(password),
            false,
        )
        .unwrap_err();
    assert!(matches!(
        err_slot,
        super::AppFacadeError::ValidationFailed(_)
    ));

    // 4. Reject incomplete replacement target
    let err_rep = state
        .save_contact_with_options(
            &mut service,
            mk_contact(vec![FrontendSyncDescriptor {
                write_url: "https://peer.example.org/sync.php".to_string(),
                read_url: "https://peer.example.org/d".to_string(),
                drop_pub: valid_pub,
                status: "deprecating".to_string(),
                replacement: Some(crate::models::FrontendSyncReplacementTarget {
                    write_url: "https://new.example.org/sync.php".to_string(),
                    read_url: "https://new.example.org/d".to_string(),
                    drop_pub: String::new(),
                }),
                ..Default::default()
            }]),
            Some(password),
            false,
        )
        .unwrap_err();
    assert!(matches!(
        err_rep,
        super::AppFacadeError::ValidationFailed(_)
    ));

    // 5. Preserve unprobed QR write_url-only import (read_url, drop_pub, slot_id empty, replacement None)
    state
        .save_contact_with_options(
            &mut service,
            mk_contact(vec![FrontendSyncDescriptor {
                write_url: "https://qr-peer.example.org/sync.php".to_string(),
                read_url: String::new(),
                slot_id: None,
                drop_pub: String::new(),
                status: "unverified".to_string(),
                ..Default::default()
            }]),
            Some(password),
            false,
        )
        .expect("unprobed write_url-only QR import must remain allowed");

    let saved = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].syncs.len(), 1);
    assert_eq!(
        saved[0].syncs[0].write_url,
        "https://qr-peer.example.org/sync.php"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn s11_x3_card_sync_update_sanitizes_bad_and_revoked_drops_without_aborting_valid_ones() {
    let temp_dir =
        std::env::temp_dir().join(format!("humoco_s11_x3_card_{}", uuid::Uuid::new_v4()));
    let password = "test-password-123";
    let alice_service = crate::test_support::setup_test_app(
        &temp_dir.join("alice"),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        password,
    );
    let bob_service = crate::test_support::setup_test_app(
        &temp_dir.join("bob"),
        "Bob",
        crate::test_support::BOB_PHRASE,
        password,
    );
    let bob_did = bob_service
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let state = crate::test_support::create_test_state(alice_service);
    let mut service = state.service.lock().unwrap();

    state
        .save_contact(
            &mut service,
            FrontendContact {
                did: bob_did.clone(),
                profile: FrontendUserProfile {
                    id: Some(bob_did.clone()),
                    ..Default::default()
                },
                tags: vec![],
                added_at: Utc::now().to_rfc3339(),
                notes: None,
                trust_level: Some(2),
                desired_hops: Some(1),
                has_peer_list: false,
                peer_list_updated_at: None,
                syncs: vec![],
            },
            Some(password),
        )
        .unwrap();

    let valid_pub_lower = crate::test_support::test_ed25519_pub_hex(0x42);
    let mixed_card_syncs = vec![
        FrontendSyncDescriptor {
            write_url: "https://revoked.example.org/sync.php".to_string(),
            read_url: "https://revoked.example.org/d".to_string(),
            drop_pub: crate::test_support::test_ed25519_pub_hex(0x43),
            status: "revoked".to_string(),
            ..Default::default()
        },
        FrontendSyncDescriptor {
            write_url: "https://169.254.169.254/sync.php".to_string(),
            read_url: "https://169.254.169.254/d".to_string(),
            drop_pub: valid_pub_lower.clone(),
            status: "active".to_string(),
            ..Default::default()
        },
        FrontendSyncDescriptor {
            write_url: "https://weak.example.org/sync.php".to_string(),
            read_url: "https://weak.example.org/d".to_string(),
            drop_pub: "00".repeat(32),
            status: "active".to_string(),
            ..Default::default()
        },
        FrontendSyncDescriptor {
            write_url: "https://valid.example.org/sync.php".to_string(),
            read_url: "https://valid.example.org/d".to_string(),
            drop_pub: valid_pub_lower.to_ascii_uppercase(),
            status: "active".to_string(),
            ..Default::default()
        },
    ];

    let card_profile = human_money_core::models::profile::PublicProfile {
        first_name: Some("Robert".to_string()),
        ..Default::default()
    };

    let updated = state
        .update_contact_from_card(
            &mut service,
            &bob_did,
            &mixed_card_syncs,
            Some(&card_profile),
            Some(password),
        )
        .unwrap();
    assert!(updated);

    let saved = state.load_contacts(&mut service, Some(password)).unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].profile.first_name.as_deref(), Some("Robert"));
    assert_eq!(saved[0].syncs.len(), 1);
    assert_eq!(
        saved[0].syncs[0].write_url,
        "https://valid.example.org/sync.php"
    );
    assert_eq!(saved[0].syncs[0].drop_pub, valid_pub_lower);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_s03_s07_1_revoked_own_sync_excluded_from_descriptor_and_backup_targets() {
    let dir = crate::test_support::temp_test_dir("s03_s07_1_revoked_own");
    let wallet_dir = dir.join("wallet");
    let pw = "StrongPassword123!";
    let svc = crate::test_support::setup_test_app(
        &wallet_dir,
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    );
    let state = crate::test_support::create_test_state(svc);

    let own_write = "https://own.example.org/sync.php".to_string();
    let own_read = "https://own.example.org/d".to_string();
    let own_drop_pub = crate::test_support::test_ed25519_pub_hex(0x31);
    let active_admin = super::StoredSyncAdmin {
        descriptor: humoco_market::DropDescriptor {
            write_url: own_write.clone(),
            read_url: own_read.clone(),
            slot_id: None,
            drop_pub: own_drop_pub.clone(),
        },
        sync_index: 0,
        data_dir: "d".to_string(),
        max_members: 50,
        max_total_storage_mb: 100,
        max_user_storage_mb: 10,
        policies: Default::default(),
        allow: vec![],
        last_members_hash: Default::default(),
        revocation: None,
    };

    {
        let mut s = state.lock_service().unwrap();
        state
            .mutate_security_policy(&mut s, Some(pw), |p| {
                p.session_timeout_seconds = 0;
                p.market_selected_syncs.clear();
                p.public_syncs.clear();
            })
            .unwrap();

        let mut init_batch = super::EncryptedBatchCommit::new();
        init_batch.sync_admin(active_admin.clone());
        init_batch.commit(&state, &mut s, Some(pw)).unwrap();

        assert!(state
            .get_or_load_own_sync_descriptor(&mut s, Some(pw))
            .unwrap()
            .is_some());
        let active_targets = crate::services::common::collect_backup_sync_targets_with_service(
            &state,
            &mut s,
            Some(pw),
            None,
        )
        .unwrap();
        assert_eq!(active_targets.len(), 1);
        assert_eq!(active_targets[0].write_url, own_write);
    }

    let overview_active =
        crate::services::sync_engine::get_sync_overview_status_impl(Some(pw.to_string()), &state)
            .unwrap();
    assert!(overview_active.has_own_sync);

    // Revoke own StoredSyncAdmin via EncryptedBatchCommit (live cache update)
    let mut revoked_admin = active_admin;
    revoked_admin.revocation = Some(crate::sync_client::SyncRevocationNotice {
        drop_pub: own_drop_pub,
        issued_at: 1_760_000_000,
        effective_at: 1_760_000_000,
        reason: Some("Server decommissioned".to_string()),
        replacement: None,
        signature: "00".repeat(64),
    });
    {
        let mut s = state.lock_service().unwrap();
        let mut rev_batch = super::EncryptedBatchCommit::new();
        rev_batch.sync_admin(revoked_admin);
        rev_batch.commit(&state, &mut s, Some(pw)).unwrap();

        assert!(
            state
                .get_or_load_own_sync_descriptor(&mut s, Some(pw))
                .unwrap()
                .is_none(),
            "S03/S07-1: live cache must omit revoked own sync descriptor"
        );
        let rev_targets = crate::services::common::collect_backup_sync_targets_with_service(
            &state,
            &mut s,
            Some(pw),
            None,
        )
        .unwrap();
        assert!(
            rev_targets.is_empty(),
            "S07-1: backup targets must omit revoked own sync in live session"
        );
    }
    let overview_live_rev =
        crate::services::sync_engine::get_sync_overview_status_impl(Some(pw.to_string()), &state)
            .unwrap();
    assert!(
        !overview_live_rev.has_own_sync,
        "S03: sync overview must report has_own_sync == false once revoked"
    );

    // Fresh session initialization must also exclude revoked own sync from own_sync_descriptor
    let mut reloaded_svc = human_money_core::app_service::AppService::new(&wallet_dir).unwrap();
    let folder = reloaded_svc.list_profiles().unwrap()[0].folder_name.clone();
    reloaded_svc
        .login(&folder, pw, false, "device-drops-test".to_string())
        .unwrap();
    let reloaded_state = crate::test_support::create_test_state(
        human_money_core::app_service::AppService::new(&wallet_dir).unwrap(),
    );
    {
        reloaded_state
            .initialize_profile_session(&mut reloaded_svc, pw)
            .unwrap();
        *reloaded_state.service.lock().unwrap() = reloaded_svc;
    }

    {
        let mut s = reloaded_state.lock_service().unwrap();
        assert!(
            reloaded_state
                .get_or_load_own_sync_descriptor(&mut s, None)
                .unwrap()
                .is_none(),
            "S03/S07-1: fresh session init must omit revoked own sync descriptor"
        );
        let targets_after_reinit =
            crate::services::common::collect_backup_sync_targets_with_service(
                &reloaded_state,
                &mut s,
                None,
                None,
            )
            .unwrap();
        assert!(
            targets_after_reinit.is_empty(),
            "S07-1: collect_backup_sync_targets_with_service must omit revoked own sync after fresh session init"
        );
        // Full StoredSyncAdmin with revocation must still be readable for admin status/clearance UIs
        let full_admin =
            crate::services::sync_engine::admin::load_stored_sync_admin_opt(&mut s, Some(pw))
                .unwrap()
                .expect("StoredSyncAdmin should still exist");
        assert!(full_admin.revocation.is_some());
    }
    let overview_reinit =
        crate::services::sync_engine::get_sync_overview_status_impl(None, &reloaded_state).unwrap();
    assert!(!overview_reinit.has_own_sync);

    // Also verify timeout == 0 cache-only fallback when Core session is locked
    {
        let mut s = reloaded_state.lock_service().unwrap();
        s.lock_session();
        assert!(reloaded_state
            .get_or_load_own_sync_descriptor(&mut s, None)
            .unwrap()
            .is_none());
    }
    let overview_locked =
        crate::services::sync_engine::get_sync_overview_status_impl(None, &reloaded_state).unwrap();
    assert!(!overview_locked.has_own_sync);

    let _ = std::fs::remove_dir_all(&dir);
}
