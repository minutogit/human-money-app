// src-tauri/src/commands/contacts.rs
use crate::models::{FrontendContact, FrontendError};
use crate::state::RequiredAccess;
use crate::AppState;
use log::info;

#[tauri::command]
pub fn get_contacts(state: tauri::State<AppState>) -> Result<Vec<FrontendContact>, FrontendError> {
    state.authorize(RequiredAccess::Unlocked, None)?;
    state.get_cached_contacts().map_err(FrontendError::from)
}

#[tauri::command]
pub async fn save_contact(
    contact: FrontendContact,
    password: Option<String>,
    preserve_manual_card: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<(), FrontendError> {
    let mut contact = contact;
    let validated_did =
        human_money_core::ValidatedUserId::parse(&contact.did).map_err(FrontendError::from)?;
    contact.did = validated_did.into_string();
    crate::services::sync_endpoints::validate_contact_sync_descriptors(&contact.syncs)?;
    let contact_did = contact.did.clone();
    let trust_level = contact.trust_level;
    let needs_sync_probe = contact
        .syncs
        .iter()
        .any(|d| d.drop_pub.trim().is_empty() || d.read_url.trim().is_empty());
    info!("Saving contact for DID: {}", contact_did);
    let (session, is_own_root, resolved_pw) = {
        let (mut service, resolved_pw) =
            state.authorize_and_lock_secret(RequiredAccess::DailyPay, password.as_deref())?;
        let session = state.session_token_for_service(&service)?;
        let pw = resolved_pw.as_deref().map(String::as_str);
        let own_can = service
            .with_wallet(|w| crate::state::canonicalize_did(w.get_user_id()).ok())
            .ok()
            .flatten();
        let contact_can = crate::state::canonicalize_did(&contact.did).ok();
        let same_root = own_can.is_some() && own_can == contact_can;
        state
            .save_contact_with_options(
                &mut service,
                contact,
                pw,
                preserve_manual_card.unwrap_or(false),
            )
            .map_err(FrontendError::from)?;
        (session, same_root, resolved_pw)
    };
    let follow_pw = resolved_pw.as_deref().map(String::as_str);

    // Probe unverified contact syncs if needed, then send initial WoT handshake (never to own root).
    if !is_own_root && state.is_generation_current(session.generation) {
        if needs_sync_probe {
            let _ = crate::services::sync_engine::probe_contact_syncs_with_session(
                &session, false, follow_pw, &state,
            )
            .await;
        }
        if let Some(lvl) = trust_level {
            if lvl <= 3 && state.is_generation_current(session.generation) {
                let _ = crate::services::post_service::send_wot_handshake_via_post_with_session(
                    &session,
                    contact_did,
                    lvl.min(1),
                    true,
                    follow_pw,
                    &state,
                )
                .await;
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn delete_contact(
    did: String,
    password: Option<String>,
    preserve_manual_card: Option<bool>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    let did = did.trim().to_string();
    info!("Deleting contact for DID: {}", did);
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    state
        .delete_contact_with_options(
            &mut service,
            &did,
            pw,
            preserve_manual_card.unwrap_or(false),
        )
        .map_err(FrontendError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contacts_ipc_parsing() {
        // Test that FrontendContact can be parsed from JSON
        let valid_json = r#"{
            "did": "did:key:z123",
            "profile": {
                "id": "did:key:z123",
                "firstName": "John",
                "lastName": "Doe",
                "organization": "Test Org",
                "address": {
                    "street": "Test Street",
                    "houseNumber": "123",
                    "zipCode": "12345",
                    "city": "Test City",
                    "country": "Germany",
                    "fullAddress": "Test Street 123, 12345 Test City, Germany"
                },
                "gender": "1",
                "email": "john@example.com",
                "phone": "123456789",
                "coordinates": "51.16, 10.45",
                "url": "https://example.com"
            },
            "tags": ["friend", "family"],
            "addedAt": "2024-01-01T00:00:00Z",
            "notes": "Test contact"
        }"#;

        let result: Result<FrontendContact, _> = serde_json::from_str(valid_json);
        assert!(
            result.is_ok(),
            "Valid FrontendContact JSON should parse successfully"
        );

        let contact = result.unwrap();
        assert_eq!(contact.did, "did:key:z123");
        assert_eq!(contact.tags, vec!["friend", "family"]);
        assert_eq!(contact.profile.first_name, Some("John".to_string()));
    }

    #[test]
    fn test_address_book_serialization() {
        // Test that FrontendAddressBook can be serialized and deserialized
        use crate::models::FrontendAddressBook;

        let mut address_book = FrontendAddressBook::default();

        let contact_json = r#"{
            "did": "did:key:z456",
            "profile": {
                "id": "did:key:z456",
                "firstName": "Jane",
                "lastName": "Smith"
            },
            "tags": ["colleague"],
            "addedAt": "2024-01-02T00:00:00Z"
        }"#;

        let contact: FrontendContact = serde_json::from_str(contact_json).unwrap();
        address_book.contacts.insert(contact.did.clone(), contact);

        let serialized = serde_json::to_string(&address_book);
        assert!(serialized.is_ok(), "FrontendAddressBook should serialize");

        let deserialized: Result<FrontendAddressBook, _> =
            serde_json::from_str(&serialized.unwrap());
        assert!(
            deserialized.is_ok(),
            "FrontendAddressBook should deserialize"
        );

        let deserialized_book = deserialized.unwrap();
        assert_eq!(deserialized_book.contacts.len(), 1);
        assert!(deserialized_book.contacts.contains_key("did:key:z456"));
    }

    #[test]
    fn test_contact_serialization_roundtrip() {
        // Test that a contact can be serialized and deserialized without data loss
        let original_json = r#"{
            "did": "did:key:z789",
            "profile": {
                "id": "did:key:z789",
                "firstName": "Alice",
                "lastName": "Wonder",
                "organization": "Wonderland Inc",
                "community": "Fantasy",
                "address": {
                    "street": "Wonder Lane",
                    "houseNumber": "42",
                    "zipCode": "54321",
                    "city": "Wonder City",
                    "country": "Wonderland",
                    "fullAddress": "Wonder Lane 42, 54321 Wonder City, Wonderland"
                },
                "gender": "2",
                "email": "alice@wonderland.com",
                "phone": "987654321",
                "coordinates": "52.52, 13.40",
                "url": "https://wonderland.com",
                "serviceOffer": "Wonderful services",
                "needs": "Nothing"
            },
            "tags": ["vip", "friend"],
            "addedAt": "2024-01-03T00:00:00Z",
            "notes": "VIP contact"
        }"#;

        let original: FrontendContact = serde_json::from_str(original_json).unwrap();
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: FrontendContact = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original.did, deserialized.did);
        assert_eq!(original.tags, deserialized.tags);
        assert_eq!(original.added_at, deserialized.added_at);
        assert_eq!(original.notes, deserialized.notes);
        assert_eq!(original.profile.id, deserialized.profile.id);
        assert_eq!(original.profile.first_name, deserialized.profile.first_name);
        assert_eq!(original.profile.last_name, deserialized.profile.last_name);
        assert_eq!(
            original.profile.organization,
            deserialized.profile.organization
        );
        assert_eq!(original.profile.community, deserialized.profile.community);
        let orig_addr = original.profile.address.as_ref().unwrap();
        let des_addr = deserialized.profile.address.as_ref().unwrap();
        assert_eq!(orig_addr.street, des_addr.street);
        assert_eq!(orig_addr.house_number, des_addr.house_number);
        assert_eq!(orig_addr.zip_code, des_addr.zip_code);
        assert_eq!(orig_addr.city, des_addr.city);
        assert_eq!(orig_addr.country, des_addr.country);
        assert_eq!(orig_addr.full_address, des_addr.full_address);
        assert_eq!(original.profile.gender, deserialized.profile.gender);
        assert_eq!(original.profile.email, deserialized.profile.email);
        assert_eq!(original.profile.phone, deserialized.profile.phone);
        assert_eq!(original.profile.coordinates, deserialized.profile.coordinates);
        assert_eq!(original.profile.url, deserialized.profile.url);
        assert_eq!(
            original.profile.service_offer,
            deserialized.profile.service_offer
        );
        assert_eq!(original.profile.needs, deserialized.profile.needs);
    }

    #[test]
    fn test_get_contacts_uses_cache() {
        use crate::models::{FrontendAddressBook, FrontendContact, FrontendUserProfile};
        use chrono::Utc;
        use human_money_core::app_service::AppService;

        let temp_guard =
            crate::services::fs_helpers::TempDirGuard(crate::test_support::temp_test_dir("contacts_cache"));
        let service = AppService::new(&temp_guard.0).unwrap();

        let state_raw = AppState::new(service);
        let init_err = state_raw
            .get_cached_contacts()
            .expect_err("uninitialized contacts cache must fail closed");
        assert!(
            init_err.to_string().contains("Contacts cache not initialized"),
            "unexpected error: {}",
            init_err
        );

        let mut book = FrontendAddressBook::default();
        book.contacts.insert(
            "did:key:zara".to_string(),
            FrontendContact {
                did: "did:key:zara".to_string(),
                profile: FrontendUserProfile {
                    id: Some("did:key:zara".to_string()),
                    first_name: Some("Zara".to_string()),
                    last_name: Some("Last".to_string()),
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
            },
        );
        book.contacts.insert(
            "did:key:alice".to_string(),
            FrontendContact {
                did: "did:key:alice".to_string(),
                profile: FrontendUserProfile {
                    id: Some("did:key:alice".to_string()),
                    first_name: Some("alice".to_string()),
                    last_name: Some("First".to_string()),
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
            },
        );

        state_raw.set_cached_contacts(Some(book));

        let cached_map = state_raw
            .cache_contacts()
            .expect("cached address book must be present");
        assert_eq!(cached_map.contacts.len(), 2);

        let contacts = state_raw
            .get_cached_contacts()
            .expect("cached contacts list must resolve");
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].did, "did:key:alice");
        assert_eq!(contacts[0].profile.first_name.as_deref(), Some("alice"));
        assert_eq!(contacts[1].did, "did:key:zara");
        assert_eq!(contacts[1].profile.first_name.as_deref(), Some("Zara"));
    }

    #[test]
    fn test_save_contact_merges_syncs_and_auto_activates_friend_sync() {
        use crate::models::{FrontendContact, FrontendSyncDescriptor, FrontendUserProfile};
        use chrono::Utc;

        let temp_guard = crate::services::fs_helpers::TempDirGuard(
            crate::test_support::temp_test_dir("humoco_contacts_save"),
        );
        let temp_dir = &temp_guard.0;
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

        let did = &bob_did;
        let sync1 = FrontendSyncDescriptor {
            read_url: "https://alice-drop.com/sync.php".to_string(),
            write_url: "https://alice-drop.com/sync.php".to_string(),
            drop_pub: crate::test_support::test_ed25519_pub_hex(0x11),
            slot_id: None,
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "unverified".to_string(),
            last_probed_at: None,
            ..Default::default()
        };

        // 1. Initial contact with sync1
        let initial_contact = FrontendContact {
            did: did.to_string(),
            profile: FrontendUserProfile {
                id: Some(did.to_string()),
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: Utc::now().to_rfc3339(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![sync1.clone()],
        };

        state
            .save_contact(&mut service, initial_contact, Some(password))
            .unwrap();

        // Check security policy: saving contact does not blindly auto-activate unverified syncs
        let policy = state
            .load_security_policy(&mut service, Some(password))
            .unwrap();
        assert_eq!(policy.market_selected_syncs.len(), 0);

        // 2. Overwrite contact with EMPTY drops (user deleted drop)
        let updated_contact = FrontendContact {
            did: did.to_string(),
            profile: FrontendUserProfile {
                id: Some(did.to_string()),
                first_name: Some("Alice Renamed".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: String::new(),
            notes: Some("Friend note".to_string()),
            trust_level: Some(2),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![], // Empty!
        };

        state
            .save_contact(&mut service, updated_contact, Some(password))
            .unwrap();

        // 3. Verify drops were deleted authoritatively and not resurrected
        let loaded_contacts = state.load_contacts(&mut service, Some(password)).unwrap();
        let alice = loaded_contacts
            .iter()
            .find(|c| c.did == *did)
            .expect("Alice should exist");
        assert_eq!(alice.profile.first_name, Some("Alice Renamed".to_string()));
        assert_eq!(alice.notes.as_deref(), Some("Friend note"));
        assert_eq!(alice.trust_level, Some(2));
        assert_eq!(
            alice.syncs.len(),
            0,
            "Sync endpoints must be deleted when saving with empty syncs"
        );
    }

    #[test]
    fn s11_f2_validate_contact_sync_descriptors_rules() {
        use crate::models::{FrontendSyncDescriptor, FrontendSyncReplacementTarget};
        use crate::services::sync_endpoints::validate_contact_sync_descriptors;
        use crate::test_support::test_ed25519_pub_hex;

        let unprobed = FrontendSyncDescriptor {
            write_url: "https://sync.example.com/sync.php".to_string(),
            read_url: String::new(),
            drop_pub: String::new(),
            slot_id: None,
            ..Default::default()
        };
        validate_contact_sync_descriptors(&[unprobed])
            .expect("unprobed write_url-only descriptor must pass");

        let read_only = FrontendSyncDescriptor {
            write_url: String::new(),
            read_url: "https://sync.example.com/d".to_string(),
            drop_pub: String::new(),
            slot_id: None,
            ..Default::default()
        };
        validate_contact_sync_descriptors(&[read_only])
            .expect("read_url-only descriptor must pass");

        let empty_both = FrontendSyncDescriptor::default();
        let err_empty = validate_contact_sync_descriptors(&[empty_both])
            .expect_err("empty descriptor must fail");
        assert!(
            err_empty.message.contains("writeUrl or readUrl"),
            "unexpected error for empty descriptor: {}",
            err_empty.message
        );

        let query_read = FrontendSyncDescriptor {
            write_url: "https://sync.example.com/sync.php".to_string(),
            read_url: "https://sync.example.com/data?leak=1".to_string(),
            ..Default::default()
        };
        let err_query = validate_contact_sync_descriptors(&[query_read])
            .expect_err("readUrl with query must fail");
        assert!(
            err_query
                .message
                .contains("must not contain a query string"),
            "unexpected error for query readUrl: {}",
            err_query.message
        );

        let ssrf_write = FrontendSyncDescriptor {
            write_url: "http://169.254.169.254/latest/meta-data".to_string(),
            ..Default::default()
        };
        let err_ssrf = validate_contact_sync_descriptors(&[ssrf_write])
            .expect_err("SSRF writeUrl must fail");
        assert!(
            err_ssrf.message.contains("blocked internal/private address"),
            "unexpected error for SSRF writeUrl: {}",
            err_ssrf.message
        );

        let bad_pub = FrontendSyncDescriptor {
            write_url: "https://sync.example.com/sync.php".to_string(),
            drop_pub: "not-hex".to_string(),
            ..Default::default()
        };
        let err_bad_pub = validate_contact_sync_descriptors(&[bad_pub])
            .expect_err("non-hex dropPub must fail");
        assert!(
            err_bad_pub.message.contains("Invalid sync dropPub"),
            "unexpected error for non-hex dropPub: {}",
            err_bad_pub.message
        );

        let weak_pub = FrontendSyncDescriptor {
            write_url: "https://sync.example.com/sync.php".to_string(),
            drop_pub: "00".repeat(32),
            ..Default::default()
        };
        let err_weak = validate_contact_sync_descriptors(&[weak_pub])
            .expect_err("weak small-order dropPub must fail");
        assert!(
            err_weak.message.contains("weak small-order point"),
            "unexpected error for weak dropPub: {}",
            err_weak.message
        );

        let bad_slot = FrontendSyncDescriptor {
            write_url: "https://sync.example.com/sync.php".to_string(),
            slot_id: Some("bad slot with spaces".to_string()),
            ..Default::default()
        };
        let err_slot = validate_contact_sync_descriptors(&[bad_slot])
            .expect_err("invalid slotId must fail");
        assert!(
            err_slot.message.contains("Invalid sync slotId"),
            "unexpected error for invalid slotId: {}",
            err_slot.message
        );

        let bad_rep = FrontendSyncDescriptor {
            write_url: "https://sync.example.com/sync.php".to_string(),
            replacement: Some(FrontendSyncReplacementTarget {
                write_url: "ftp://bad.example.com/sync.php".to_string(),
                read_url: "https://new.example.com/data".to_string(),
                drop_pub: test_ed25519_pub_hex(0x11),
            }),
            ..Default::default()
        };
        let err_rep = validate_contact_sync_descriptors(&[bad_rep])
            .expect_err("ftp replacement writeUrl must fail");
        assert!(
            err_rep.message.contains("must use HTTPS"),
            "unexpected error for ftp replacement: {}",
            err_rep.message
        );
    }

    #[test]
    fn test_save_contact_did_validation_and_normalization() {
        let bob_guard = crate::services::fs_helpers::TempDirGuard(
            crate::test_support::temp_test_dir("contacts_did_norm"),
        );
        let bob_service = crate::test_support::setup_test_app(
            &bob_guard.0,
            "Test Bob",
            crate::test_support::BOB_PHRASE,
            "pw-did-norm",
        );
        let bob_did = bob_service
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let padded = format!("  {}  ", bob_did);
        let parsed = human_money_core::ValidatedUserId::parse(&padded)
            .expect("padded valid DID must normalize cleanly");
        assert_eq!(parsed.as_str(), bob_did);

        let err_empty = human_money_core::ValidatedUserId::parse("   ")
            .expect_err("blank DID must be rejected");
        assert!(matches!(
            err_empty,
            human_money_core::crypto::GetPubkeyError::InvalidDidFormat
        ));

        let err_ws = human_money_core::ValidatedUserId::parse("did:key:bad space")
            .expect_err("DID with internal whitespace must be rejected");
        assert!(matches!(
            err_ws,
            human_money_core::crypto::GetPubkeyError::UnexpectedWhitespace
        ));
    }

    #[test]
    fn test_delete_contact_removes_saved_entry() {
        use crate::models::{FrontendContact, FrontendUserProfile};
        use chrono::Utc;

        let temp_guard = crate::services::fs_helpers::TempDirGuard(
            crate::test_support::temp_test_dir("contacts_delete"),
        );
        let temp_dir = &temp_guard.0;
        let password = "pw-contacts-delete";
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
            trust_level: Some(1),
            desired_hops: None,
            has_peer_list: false,
            peer_list_updated_at: None,
            syncs: vec![],
        };

        state
            .save_contact_with_options(&mut service, contact, Some(password), false)
            .unwrap();
        assert_eq!(state.load_contacts(&mut service, Some(password)).unwrap().len(), 1);

        state
            .delete_contact_with_options(&mut service, &bob_did, Some(password), false)
            .unwrap();
        let after_delete = state.load_contacts(&mut service, Some(password)).unwrap();
        assert!(
            after_delete.iter().all(|c| c.did != bob_did),
            "deleted contact {} must no longer appear in load_contacts",
            bob_did
        );
    }
}
