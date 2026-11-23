use super::session::*;
use super::storage::*;
use super::*;
use crate::models::{
    FrontendContact, FrontendUserProfile, GuardianAssignmentRecord, GuardianAssignmentStatus,
    RecoveryOwnerState,
};
use crate::services::{
    backup_service::probe_sync_backup_impl, common::are_dids_equal, post_service::load_post_state,
};
use crate::state::AppState;
use crate::test_support::{
    create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE, CHARLIE_PHRASE,
    READER1_PHRASE, READER2_PHRASE,
};
use chrono::Utc;
use human_money_core::app_service::AppService;
use humoco_market::{
    canonicalize_owner_account_id, create_social_recovery_vault_bundle, parse_emergency_kit_uri,
    DropDescriptor, RecoveryKdfParams, RecoveryMasterPayload,
};
use std::fs;
use std::path::Path;

fn make_test_contact(did: &str, first_name: &str, trust_level: Option<u8>) -> FrontendContact {
    FrontendContact {
        did: did.to_string(),
        profile: FrontendUserProfile {
            id: Some(did.to_string()),
            first_name: Some(first_name.to_string()),
            ..Default::default()
        },
        tags: vec![],
        added_at: Utc::now().to_rfc3339(),
        notes: None,
        trust_level,
        desired_hops: None,
        has_peer_list: false,
        peer_list_updated_at: None,
        syncs: vec![],
    }
}

fn create_wallet_state(
    base_dir: &Path,
    name: &str,
    phrase: &str,
    pass: &str,
) -> (AppState, String) {
    let dir = base_dir.join(name);
    let actual_phrase = if phrase.trim().is_empty() {
        human_money_core::services::mnemonic::MnemonicProcessor::generate(
            12,
            human_money_core::MnemonicLanguage::English,
        )
        .unwrap()
    } else {
        phrase.to_string()
    };
    let service = setup_test_app(&dir, name, &actual_phrase, pass);
    let state = create_test_state(service);
    let did = {
        let s = state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    (state, did)
}

#[tokio::test]
async fn test_emergency_sheet_confirmation_and_status() {
    let base_dir = temp_test_dir("emergency_sheet_test");
    let password = "TestPassword123!";
    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);

    // 1. Initial status has no emergency confirmation
    let initial_status = get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
    assert!(initial_status.emergency_sheet_confirmed_at.is_none());
    assert!(!initial_status.has_identity_recovery);

    // 2. Wrong control word is rejected
    let wrong_res = confirm_emergency_sheet_saved_impl(
        password.to_string(),
        4,
        "incorrect_word".to_string(),
        &alice_state,
        None,
    )
    .await;
    assert!(wrong_res.is_err());

    // 3. Right control word succeeds (Word 4 of ALICE_PHRASE is "abandon")
    let confirm_res = confirm_emergency_sheet_saved_impl(
        password.to_string(),
        4,
        "abandon".to_string(),
        &alice_state,
        None,
    )
    .await
    .unwrap();
    assert!(confirm_res.confirmed_at > 0);

    // 4. Protection status reflects confirmation
    let updated_status = get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
    assert!(updated_status.emergency_sheet_confirmed_at.is_some());
    assert!(updated_status.has_identity_recovery);
}

#[tokio::test]
async fn test_export_profile_mnemonic_kit() {
    let base_dir = temp_test_dir("mnemonic_kit_test");
    let password = "TestPassword123!";
    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);

    let kit = export_profile_mnemonic_impl(password.to_string(), &alice_state)
        .unwrap()
        .expect("Mnemonic kit DTO should be generated");

    assert_eq!(kit.phrase.split_whitespace().count(), 12);
    assert!(kit.kit_uri.starts_with("humoco://kit?"));
    assert!(kit.qr_svg.starts_with("<svg"));

    let parsed = parse_emergency_kit_uri(&kit.kit_uri).expect("URI must parse successfully");
    assert_eq!(parsed.language_code, 1);
    assert!(!parsed.entropy_hex.is_empty());
}

#[tokio::test]
async fn test_setup_social_recovery_validation_and_guards() {
    let base_dir = temp_test_dir("validation_guards_test");
    let password = "TestPassword123!";
    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
    let (_carol_state, carol_did) =
        create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

    // 1. Less than 3 guardians rejected
    let res_few = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did.clone(), carol_did.clone()],
        2,
        [1, 2, 3],
        ["A".to_string(), "B".to_string(), "C".to_string()],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await;
    assert!(res_few.is_err());

    // 2. Duplicate guardians rejected
    let res_dup = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did.clone(), bob_did.clone(), carol_did.clone()],
        2,
        [1, 2, 3],
        ["A".to_string(), "B".to_string(), "C".to_string()],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await;
    assert!(res_dup.is_err());

    // 3. Guardians not in AddressBook or trust level < 1 rejected
    let res_untrusted = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        ["A".to_string(), "B".to_string(), "C".to_string()],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await;
    assert!(res_untrusted.is_err());
}

#[tokio::test]
async fn test_3_of_5_social_recovery_lifecycle_and_security() {
    let base_dir = temp_test_dir("social_recovery_lifecycle");
    let password = "TestPassword123!";

    // 1. Initialize Alice and 5 friends
    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
    let (carol_state, carol_did) =
        create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
    let (dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);
    let (_eve_state, eve_did) = create_wallet_state(&base_dir, "Eve", READER2_PHRASE, password);
    let (_frank_state, frank_did) = create_wallet_state(&base_dir, "Frank", "", password);

    // 2. Alice adds all 5 friends to contacts with Trust Level 2
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(2)),
                Some(password),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(password),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(2)),
                Some(password),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&eve_did, "Eve", Some(2)),
                Some(password),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&frank_did, "Frank", Some(2)),
                Some(password),
            )
            .unwrap();
    }

    // 3. Alice sets up 3-of-5 social recovery
    let setup_res = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![
            bob_did.clone(),
            carol_did.clone(),
            dave_did.clone(),
            eve_did.clone(),
            frank_did.clone(),
        ],
        3,
        [1, 2, 3],
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    assert_eq!(setup_res.rotation_epoch, 1);
    assert_eq!(setup_res.threshold_m, 3);
    assert_eq!(setup_res.guardians_count, 5);

    // 4. Verify Alice's outbox contains 5 assign envelopes
    let post_state = load_post_state(&alice_state, Some(password)).unwrap();
    assert_eq!(post_state.outbox_entries.len(), 5);

    // 5. Deliver sealed entries to Bob, Carol, Dave's vaults
    let owner_state = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(password))
            .unwrap()
            .unwrap()
    };

    // Bob stores his assigned vault entry
    {
        let bob_rec = owner_state
            .guardians
            .iter()
            .find(|g| g.user_id == bob_did)
            .unwrap();
        let mut s = bob_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
        let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
    }

    // Carol stores her assigned vault entry
    {
        let carol_rec = owner_state
            .guardians
            .iter()
            .find(|g| g.user_id == carol_did)
            .unwrap();
        let mut s = carol_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
        let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
    }

    // Dave stores his assigned vault entry
    {
        let dave_rec = owner_state
            .guardians
            .iter()
            .find(|g| g.user_id == dave_did)
            .unwrap();
        let mut s = dave_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(password)).unwrap();
        let _ = vault.upsert_ward(dave_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(password)).unwrap();
    }

    // 6. Simulate receipt of ACKs from Bob, Carol, Dave in Alice's state
    {
        let mut s = alice_state.service.lock().unwrap();
        let mut os = load_recovery_owner_state(&mut s, Some(password))
            .unwrap()
            .unwrap();
        for g in &mut os.guardians {
            if g.user_id == bob_did || g.user_id == carol_did || g.user_id == dave_did {
                g.status = GuardianAssignmentStatus::Confirmed {
                    acked_at: Utc::now().timestamp() as u64,
                };
            }
        }
        save_recovery_owner_state(&mut s, &os, Some(password)).unwrap();
    }

    // Protection status now reflects 3 confirmed guardians
    let prot_status = get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
    assert_eq!(prot_status.confirmed_guardians_count, 3);
    assert!(prot_status.has_identity_recovery);

    // 7. Disaster mode: fresh device session
    let session_dir = temp_test_dir("disaster_session");
    let session_file = session_dir.join("active_recovery_session.json");

    let active_sess = start_social_recovery_session_impl(
        Some("Alice".to_string()),
        Some("restore_new_device".to_string()),
        None,
        &session_file,
    )
    .unwrap();
    assert_eq!(active_sess.collected_count, 0);
    assert!(!active_sess.has_enough_shares);
    assert_eq!(active_sess.session_mode, "restore_new_device");

    // Bob inspects request
    let bob_inspection = inspect_guardian_recovery_request_impl(
        active_sess.request_uri.clone(),
        Some(password),
        &bob_state,
    )
    .unwrap();
    assert_eq!(bob_inspection.sas_code, active_sess.sas_code);
    assert_eq!(bob_inspection.wards.len(), 1);
    assert!(are_dids_equal(
        &bob_inspection.wards[0].owner_did,
        &alice_did
    ));

    // Bob, Carol, Dave approve
    let bob_approval = approve_guardian_recovery_request_impl(
        active_sess.request_uri.clone(),
        alice_did.clone(),
        Some(password),
        &bob_state,
    )
    .unwrap();

    let carol_approval = approve_guardian_recovery_request_impl(
        active_sess.request_uri.clone(),
        alice_did.clone(),
        Some(password),
        &carol_state,
    )
    .unwrap();

    let dave_approval = approve_guardian_recovery_request_impl(
        active_sess.request_uri.clone(),
        alice_did.clone(),
        Some(password),
        &dave_state,
    )
    .unwrap();

    // 8. Import release replies into session
    import_guardian_recovery_reply_impl(bob_approval.reply_uri, &session_file).unwrap();
    import_guardian_recovery_reply_impl(carol_approval.reply_uri, &session_file).unwrap();
    let progress =
        import_guardian_recovery_reply_impl(dave_approval.reply_uri, &session_file).unwrap();

    assert_eq!(progress.collected_count, 3);
    assert!(progress.has_enough_shares);

    // 9. WRONG answers: fails, and active_recovery_session.json MUST NOT be deleted!
    let restored_base = temp_test_dir("restored_wallet");
    let restored_wallet_dir = restored_base.join("wallet");
    let empty_service = AppService::new(&restored_wallet_dir).unwrap();
    let restored_state = create_test_state(empty_service);

    let wrong_err = complete_social_recovery_impl(
        alice_did.clone(),
        1,
        [
            "WrongCity".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        None,
        "restore_new_device".to_string(),
        "NewPassword456!".to_string(),
        Some("Alice Restored".to_string()),
        None,
        "device-restored-1".to_string(),
        None,
        None,
        &session_file,
        &restored_state,
        &restored_base,
        &restored_wallet_dir,
        None,
    )
    .await;

    assert!(
        wrong_err.is_err(),
        "Wrong answers must fail AEAD decryption"
    );
    assert!(
        session_file.exists(),
        "Active recovery session MUST NOT be deleted when answers are incorrect!"
    );

    // 10. RIGHT answers: first without allow_empty_fallback requests seed-only confirmation
    let confirm_res = complete_social_recovery_impl(
        alice_did.clone(),
        1,
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        None,
        "restore_new_device".to_string(),
        "NewPassword456!".to_string(),
        Some("Alice Restored".to_string()),
        None,
        "device-restored-1".to_string(),
        Some(false),
        None,
        &session_file,
        &restored_state,
        &restored_base,
        &restored_wallet_dir,
        None,
    )
    .await
    .unwrap();
    assert_eq!(confirm_res.outcome, "sync_empty_requires_confirm");
    assert!(
        session_file.exists(),
        "Session file must remain while awaiting seed-only confirmation"
    );

    // Now confirm seed-only setup explicitly with allow_empty_fallback: Some(true)
    let right_res = complete_social_recovery_impl(
        alice_did.clone(),
        1,
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        None,
        "restore_new_device".to_string(),
        "NewPassword456!".to_string(),
        Some("Alice Restored".to_string()),
        None,
        "device-restored-1".to_string(),
        Some(true),
        None,
        &session_file,
        &restored_state,
        &restored_base,
        &restored_wallet_dir,
        None,
    )
    .await
    .unwrap();

    assert_eq!(right_res.outcome, "restored_seed_only");
    assert!(
        !session_file.exists(),
        "Active recovery session MUST be cleaned up on successful recovery!"
    );

    // 11. Verify restored wallet identity matches Alice
    let restored_did = {
        let s = restored_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    assert_eq!(restored_did, alice_did);
}

#[tokio::test]
async fn test_local_password_reset_mode() {
    let base_dir = temp_test_dir("password_reset_test");
    let old_pass = "OldPassword123!";
    let new_pass = "NewPassword456!";

    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, old_pass);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, old_pass);
    let (carol_state, carol_did) =
        create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, old_pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, old_pass);

    // Add 3 guardians and setup recovery
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(2)),
                Some(old_pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(old_pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(2)),
                Some(old_pass),
            )
            .unwrap();
    }

    setup_social_recovery_guardians_with_params(
        old_pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "AnswerOne1".to_string(),
            "AnswerTwo2".to_string(),
            "AnswerThree3".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    // Deliver entries to Bob & Carol
    let owner_state = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(old_pass))
            .unwrap()
            .unwrap()
    };

    {
        let bob_rec = owner_state
            .guardians
            .iter()
            .find(|g| g.user_id == bob_did)
            .unwrap();
        let mut s = bob_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(old_pass)).unwrap();
        let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(old_pass)).unwrap();
    }

    {
        let carol_rec = owner_state
            .guardians
            .iter()
            .find(|g| g.user_id == carol_did)
            .unwrap();
        let mut s = carol_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(old_pass)).unwrap();
        let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(old_pass)).unwrap();
    }

    // Start session & approve
    let session_file = base_dir.join("active_recovery_session.json");
    let session_dto = start_social_recovery_session_impl(
        None,
        Some("reset_local_password".to_string()),
        Some("dummy".to_string()),
        &session_file,
    )
    .unwrap();
    assert_eq!(session_dto.session_mode, "reset_local_password");

    let b_rep = approve_guardian_recovery_request_impl(
        session_dto.request_uri.clone(),
        alice_did.clone(),
        Some(old_pass),
        &bob_state,
    )
    .unwrap();
    let c_rep = approve_guardian_recovery_request_impl(
        session_dto.request_uri.clone(),
        alice_did.clone(),
        Some(old_pass),
        &carol_state,
    )
    .unwrap();

    import_guardian_recovery_reply_impl(b_rep.reply_uri, &session_file).unwrap();
    import_guardian_recovery_reply_impl(c_rep.reply_uri, &session_file).unwrap();

    // Identify Alice's folder name
    let folder_name = {
        let s = alice_state.service.lock().unwrap();
        let profiles = s.list_profiles().unwrap();
        profiles[0].folder_name.clone()
    };

    // Reset local password
    let alice_wallet_dir = base_dir.join("Alice");
    let outcome = complete_social_recovery_impl(
        alice_did.clone(),
        1,
        [
            "AnswerOne1".to_string(),
            "AnswerTwo2".to_string(),
            "AnswerThree3".to_string(),
        ],
        None,
        "reset_local_password".to_string(),
        new_pass.to_string(),
        None,
        Some(folder_name.clone()),
        "device-reset-1".to_string(),
        None,
        None,
        &session_file,
        &alice_state,
        &base_dir,
        &alice_wallet_dir,
        None,
    )
    .await
    .unwrap();

    assert_eq!(outcome.outcome, "password_reset");
    assert!(!session_file.exists());

    // Verify login with new password succeeds and old password fails
    {
        let mut s = alice_state.service.lock().unwrap();
        s.logout();
        let old_login = s.login(&folder_name, old_pass, false, "device-reset-1".to_string());
        assert!(
            old_login.is_err(),
            "Old password must not unlock after reset"
        );

        let new_login = s.login(&folder_name, new_pass, false, "device-reset-1".to_string());
        assert!(new_login.is_ok(), "New password must unlock the wallet");
    }
}

#[tokio::test]
async fn test_inspect_emergency_kit_uri_12_and_24_words() {
    let base_dir = temp_test_dir("inspect_emergency_kit");
    let pass = "TestPassword123!";
    let (alice_state, _) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);

    let kit_12 = export_profile_mnemonic_impl(pass.to_string(), &alice_state)
        .unwrap()
        .unwrap();
    let inspected_12 = inspect_emergency_kit_uri_impl(kit_12.kit_uri).unwrap();
    assert_eq!(inspected_12.word_count, 12);
    assert_eq!(inspected_12.phrase, ALICE_PHRASE);
    assert!(!inspected_12.has_bip39_passphrase);

    // 24-word German emergency kit URI
    let phrase_24 = human_money_core::services::mnemonic::MnemonicProcessor::generate(
        24,
        human_money_core::MnemonicLanguage::German,
    )
    .unwrap();
    let entropy_24 = human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(
        &phrase_24,
        human_money_core::MnemonicLanguage::German,
    )
    .unwrap();
    let kit_24_uri = humoco_market::build_emergency_kit_uri(&humoco_market::EmergencyKitData {
        version: 1,
        entropy_hex: hex::encode(entropy_24),
        language_code: 2, // German
        user_prefix: Some("verein".to_string()),
        has_bip39_passphrase: true,
        sync_descriptors: vec![DropDescriptor {
            write_url: "https://sync.example.org/sync.php".to_string(),
            read_url: "https://sync.example.org/data".to_string(),
            slot_id: None,
            drop_pub: "00".repeat(32),
        }],
    })
    .unwrap();

    let inspected_24 = inspect_emergency_kit_uri_impl(kit_24_uri).unwrap();
    assert_eq!(inspected_24.word_count, 24);
    assert_eq!(inspected_24.phrase, phrase_24);
    assert_eq!(
        inspected_24.language,
        crate::models::MnemonicLanguage::German
    );
    assert!(inspected_24.has_bip39_passphrase);
    assert_eq!(inspected_24.user_prefix.as_deref(), Some("verein"));
    assert_eq!(
        inspected_24.sync_urls,
        vec!["https://sync.example.org/sync.php".to_string()]
    );
    assert_eq!(inspected_24.sync_descriptors_json.len(), 1);
}

#[tokio::test]
async fn test_revoke_social_recovery_clears_guardians() {
    let base_dir = temp_test_dir("revoke_recovery_test");
    let pass = "TestPassword123!";

    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(2)),
                Some(pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(2)),
                Some(pass),
            )
            .unwrap();
    }

    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "Alpha123".to_string(),
            "Bravo456".to_string(),
            "Charlie789".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    // Call revoke
    let revoke_res = revoke_social_recovery_impl(Some(pass.to_string()), &alice_state, None)
        .await
        .unwrap();

    assert_eq!(revoke_res.rotation_epoch, 2);
    assert_eq!(revoke_res.revoked_guardians_count, 3);

    // Verify owner state is now cleared
    let owner_state = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(pass))
            .unwrap()
            .unwrap()
    };
    assert_eq!(owner_state.rotation_epoch, 2);
    assert!(owner_state.guardians.is_empty());

    // Verify post outbox contains revoke messages
    let post_state = load_post_state(&alice_state, Some(pass)).unwrap();
    let has_revokes = post_state
        .outbox_entries
        .iter()
        .any(|e| !e.entry.ciphertext.is_empty());
    assert!(has_revokes);
}

#[test]
fn test_ward_vault_max_limit_and_epoch_guards() {
    use humoco_market::GuardianVaultStore;

    let mut vault = GuardianVaultStore::default();
    let password = "TestPassword123!";
    let base_dir = temp_test_dir("ward_vault_test");

    // Create a valid entry from Alice
    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
    let (_carol_state, carol_did) =
        create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(2)),
                Some(password),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(password),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(2)),
                Some(password),
            )
            .unwrap();
    }

    let setup_res = tokio::runtime::Runtime::new().unwrap().block_on(async {
        setup_social_recovery_guardians_with_params(
            password.to_string(),
            vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
            2,
            [1, 2, 3],
            [
                "AnswerOne1".to_string(),
                "AnswerTwo2".to_string(),
                "AnswerThree3".to_string(),
            ],
            RecoveryKdfParams::new_fast_test(),
            &alice_state,
            None,
        )
        .await
        .unwrap()
    });
    assert_eq!(setup_res.rotation_epoch, 1);

    let owner_state = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(password))
            .unwrap()
            .unwrap()
    };
    let valid_entry = owner_state.guardians[0].sealed_entry.clone();

    // 1. Upsert succeeds for epoch 1
    assert!(vault.upsert_ward(valid_entry.clone()).is_ok());
    assert_eq!(vault.wards.len(), 1);

    // 2. Re-upserting same valid entry succeeds
    assert!(vault.upsert_ward(valid_entry.clone()).is_ok());

    // 3. Fill up to 64 wards and verify an authentic 65th ward is rejected on capacity
    vault.wards.clear();
    for i in 0..64 {
        let mut dummy = valid_entry.clone();
        dummy.owner_did = format!("did:key:dummy_owner_{}", i);
        let _ = vault.wards.insert(dummy.owner_did.clone(), dummy);
    }
    assert_eq!(vault.wards.len(), 64);

    // valid_entry has authentic signature but capacity is full
    let res_excess = vault.upsert_ward(valid_entry);
    assert!(res_excess.is_err());
    assert!(res_excess
        .unwrap_err()
        .to_string()
        .contains("capacity reached"));
}

#[tokio::test]
async fn test_probe_sync_backup_unreachable_and_empty_guard() {
    let base_dir = temp_test_dir("mirror_discovery_test");
    let wallet_dir = base_dir.join("empty");
    let empty_service = AppService::new(&wallet_dir).unwrap();
    let state = create_test_state(empty_service);
    let tmp_restore = base_dir.join("tmp_restore");
    std::fs::create_dir_all(&tmp_restore).unwrap();

    // 1. Empty inputs return error
    let empty_res = probe_sync_backup_impl(
        Vec::new(),
        ALICE_PHRASE.to_string(),
        None,
        None,
        Some(crate::models::MnemonicLanguage::English),
        &wallet_dir,
        &tmp_restore,
        &state,
    )
    .await;
    assert!(empty_res.is_err());

    // 2. Unreachable descriptor returns status = "unreachable" cleanly (never silent seed-only)
    let unreachable_desc = serde_json::to_string(&DropDescriptor {
        write_url: "http://127.0.0.1:9".to_string(),
        read_url: "http://127.0.0.1:9".to_string(),
        slot_id: Some("deadslot".to_string()),
        drop_pub: "deadkey".to_string(),
    })
    .unwrap();
    let unreach_res = probe_sync_backup_impl(
        vec![unreachable_desc],
        ALICE_PHRASE.to_string(),
        None,
        None,
        Some(crate::models::MnemonicLanguage::English),
        &wallet_dir,
        &tmp_restore,
        &state,
    )
    .await
    .unwrap();
    assert_eq!(unreach_res.status, "unreachable");
    assert!(unreach_res.error.is_some());
}

#[tokio::test]
async fn test_reply_import_guards_and_ward_contact_name_resolution() {
    let base_dir = temp_test_dir("reply_import_guards_test");
    let pass = "TestPassword123!";

    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Privat", ALICE_PHRASE, pass);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);
    let (eve_state, eve_did) = create_wallet_state(&base_dir, "Eve", READER2_PHRASE, pass);

    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(2)),
                Some(pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(2)),
                Some(pass),
            )
            .unwrap();
    }
    {
        let mut s = eve_state.service.lock().unwrap();
        eve_state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(2)),
                Some(pass),
            )
            .unwrap();
        eve_state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(pass),
            )
            .unwrap();
        eve_state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(2)),
                Some(pass),
            )
            .unwrap();
    }
    // Bob saves Alice in his AddressBook as "Hans Müller"
    {
        let mut s = bob_state.service.lock().unwrap();
        let mut hans_contact = make_test_contact(&alice_did, "Hans", Some(2));
        hans_contact.profile.last_name = Some("Müller".to_string());
        bob_state
            .save_contact(&mut s, hans_contact, Some(pass))
            .unwrap();
    }

    // Alice sets up epoch 1, then rotates to epoch 2
    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "Alpha123".to_string(),
            "Bravo456".to_string(),
            "Charlie789".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    let owner_e1 = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(pass))
            .unwrap()
            .unwrap()
    };

    // Carol keeps epoch 1 (stale)
    {
        let carol_rec = owner_e1
            .guardians
            .iter()
            .find(|g| g.user_id == carol_did)
            .unwrap();
        let mut s = carol_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
        let _ = vault.upsert_ward(carol_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
    }

    // Alice rotates to epoch 2 and delivers to Bob
    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "Alpha123".to_string(),
            "Bravo456".to_string(),
            "Charlie789".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    let owner_e2 = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(pass))
            .unwrap()
            .unwrap()
    };

    {
        let bob_rec = owner_e2
            .guardians
            .iter()
            .find(|g| g.user_id == bob_did)
            .unwrap();
        let mut s = bob_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
        let _ = vault.upsert_ward(bob_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
    }

    // Eve also adds Dave so Bob holds a second ward for mismatch test
    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "Delta111".to_string(),
            "Echo2222".to_string(),
            "Foxtrot3".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &eve_state,
        None,
    )
    .await
    .unwrap();
    let eve_owner = {
        let mut s = eve_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(pass))
            .unwrap()
            .unwrap()
    };
    {
        let bob_eve_rec = eve_owner
            .guardians
            .iter()
            .find(|g| g.user_id == bob_did)
            .unwrap();
        let mut s = bob_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
        let _ = vault.upsert_ward(bob_eve_rec.sealed_entry.clone());
        save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
    }

    let session_file = base_dir.join("active_recovery_session.json");
    let sess = start_social_recovery_session_impl(None, None, None, &session_file).unwrap();

    // Bob inspects -> Alice's ward uses AddressBook name "Hans Müller" instead of "Privat"
    let inspection =
        inspect_guardian_recovery_request_impl(sess.request_uri.clone(), Some(pass), &bob_state)
            .unwrap();
    let hans_ward = inspection
        .wards
        .iter()
        .find(|w| are_dids_equal(&w.owner_did, &alice_did))
        .unwrap();
    assert_eq!(hans_ward.owner_display_name, "Hans Müller");

    // Bob approves Alice (epoch 2)
    let bob_alice_rep = approve_guardian_recovery_request_impl(
        sess.request_uri.clone(),
        alice_did.clone(),
        Some(pass),
        &bob_state,
    )
    .unwrap();
    assert_eq!(bob_alice_rep.owner_display_name, "Hans Müller");

    let added = import_guardian_recovery_reply_impl(bob_alice_rep.reply_uri.clone(), &session_file)
        .unwrap();
    assert_eq!(added.last_import_status.as_deref(), Some("added"));
    assert_eq!(added.total_n, Some(3));
    assert_eq!(added.collected_share_indices.len(), 1);

    // Re-importing same reply yields duplicate status
    let dup = import_guardian_recovery_reply_impl(bob_alice_rep.reply_uri, &session_file).unwrap();
    assert_eq!(dup.last_import_status.as_deref(), Some("duplicate"));
    assert_eq!(dup.collected_count, 1);

    // Stale epoch reply from Carol (epoch 1 < epoch 2) is rejected with clear error
    let carol_stale_rep = approve_guardian_recovery_request_impl(
        sess.request_uri.clone(),
        alice_did.clone(),
        Some(pass),
        &carol_state,
    )
    .unwrap();
    let stale_err =
        import_guardian_recovery_reply_impl(carol_stale_rep.reply_uri, &session_file).unwrap_err();
    assert_eq!(stale_err.code, "error.recovery.staleEpoch");

    // Mismatched ward reply from Bob (Eve instead of Alice) is rejected
    let bob_eve_rep =
        approve_guardian_recovery_request_impl(sess.request_uri, eve_did, Some(pass), &bob_state)
            .unwrap();
    let mismatch_err =
        import_guardian_recovery_reply_impl(bob_eve_rep.reply_uri, &session_file).unwrap_err();
    assert_eq!(mismatch_err.code, "error.recovery.wrongProfile");
}

#[tokio::test]
async fn test_12_question_setup_and_spaced_repetition_rehearsal() {
    let base_dir = temp_test_dir("recovery_rehearsal");
    let pass = "RehearsalPass!99";
    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    {
        let mut s = alice_state.service.lock().unwrap();
        s.unlock_session(pass, 300).unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(3)),
                Some(pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(pass),
            )
            .unwrap();
        alice_state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(1)),
                Some(pass),
            )
            .unwrap();
    }

    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did, carol_did, dave_did],
        2,
        [7, 10, 12],
        [
            "Sandra".to_string(),
            "Nokia 3210".to_string(),
            "Blauer Pinguin".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    let status_now = get_recovery_protection_status_impl(None, &alice_state).unwrap();
    assert_eq!(status_now.question_ids, Some([7, 10, 12]));
    assert_eq!(status_now.rehearsal_stage, 0);
    assert!(!status_now.rehearsal_due);
    let first_due = status_now.next_rehearsal_at.unwrap();

    // Wrong middle answer pinpoints question index 1
    let wrong_res = verify_recovery_rehearsal_with_password_impl(
        [
            "sandra".to_string(),
            "Siemens C35".to_string(),
            "blauer pinguin".to_string(),
        ],
        first_due + 10,
        None,
        &alice_state,
    )
    .await
    .unwrap();
    assert!(!wrong_res.all_correct);
    assert_eq!(wrong_res.question_matches, [true, false, true]);
    assert_eq!(wrong_res.rehearsal_stage, 0);

    // Snooze pushes next_rehearsal_at forward by 24h
    let snoozed_at =
        snooze_recovery_rehearsal_with_password_impl(Some(24), first_due + 10, None, &alice_state)
            .unwrap();
    assert_eq!(snoozed_at, first_due + 10 + 24 * 3600);

    // Exact (normalized) match advances stage 0 -> 1 (7 days)
    let ok_res = verify_recovery_rehearsal_with_password_impl(
        [
            " sandra ".to_string(),
            "nokia-3210".to_string(),
            "BLAUER_pinguin".to_string(),
        ],
        snoozed_at + 10,
        None,
        &alice_state,
    )
    .await
    .unwrap();
    assert!(ok_res.all_correct);
    assert_eq!(ok_res.question_matches, [true, true, true]);
    assert_eq!(ok_res.rehearsal_stage, 1);
    assert_eq!(ok_res.next_rehearsal_at, snoozed_at + 10 + 7 * 86_400);
}

#[tokio::test]
async fn test_subaccount_social_recovery_vault_isolation_and_locked_session_status_cache() {
    let base_dir = temp_test_dir("subaccount_recovery_isolation_and_cache");
    let pass = "TestPassword123!";

    let create_sub = |name: &str, dir_name: &str, prefix: Option<&str>| {
        let dir = base_dir.join(dir_name);
        let mut svc = AppService::new(&dir).unwrap();
        svc.create_profile(
            name,
            ALICE_PHRASE,
            None,
            prefix,
            pass,
            human_money_core::MnemonicLanguage::English,
            format!("inst-{}", dir_name),
            true,
        )
        .unwrap();
        svc.unlock_session(pass, 300).unwrap();
        let state = create_test_state(svc);
        let did = state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        (dir, state, did)
    };

    let (laden_dir, laden_state, laden_did) = create_sub("Alice Laden", "laden", Some("laden"));
    let (_privat_dir, privat_state, privat_did) =
        create_sub("Alice Privat", "privat", Some("privat"));
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    for state in [&laden_state, &privat_state] {
        let mut s = state.service.lock().unwrap();
        state
            .save_contact(
                &mut s,
                make_test_contact(&bob_did, "Bob", Some(2)),
                Some(pass),
            )
            .unwrap();
        state
            .save_contact(
                &mut s,
                make_test_contact(&carol_did, "Carol", Some(2)),
                Some(pass),
            )
            .unwrap();
        state
            .save_contact(
                &mut s,
                make_test_contact(&dave_did, "Dave", Some(2)),
                Some(pass),
            )
            .unwrap();
    }

    // Both subaccounts configure Social Recovery with epoch 1 and the same guardian Bob
    let laden_setup = setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &laden_state,
        None,
    )
    .await
    .unwrap();
    let privat_setup = setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [4, 5, 6],
        [
            "Hamburg".to_string(),
            "Rex".to_string(),
            "Pasta".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &privat_state,
        None,
    )
    .await
    .unwrap();
    assert_eq!(laden_setup.rotation_epoch, 1);
    assert_eq!(privat_setup.rotation_epoch, 1);

    let laden_owner = {
        let mut s = laden_state.service.lock().unwrap();
        load_recovery_owner_state_with_cache(&laden_state, &mut s, Some(pass))
            .unwrap()
            .unwrap()
    };
    let privat_owner = {
        let mut s = privat_state.service.lock().unwrap();
        load_recovery_owner_state_with_cache(&privat_state, &mut s, Some(pass))
            .unwrap()
            .unwrap()
    };
    assert_eq!(laden_owner.guardians[0].sealed_entry.owner_did, laden_did);
    assert_eq!(privat_owner.guardians[0].sealed_entry.owner_did, privat_did);

    // Bob stores guardian entries for both subaccounts — second setup must NOT overwrite first
    {
        let mut s = bob_state.service.lock().unwrap();
        let mut vault = load_guardian_vault_with_cache(&bob_state, &mut s, Some(pass)).unwrap();
        vault
            .upsert_ward(
                laden_owner
                    .guardians
                    .iter()
                    .find(|g| g.user_id == bob_did)
                    .unwrap()
                    .sealed_entry
                    .clone(),
            )
            .unwrap();
        vault
            .upsert_ward(
                privat_owner
                    .guardians
                    .iter()
                    .find(|g| g.user_id == bob_did)
                    .unwrap()
                    .sealed_entry
                    .clone(),
            )
            .unwrap();
        save_guardian_vault_with_cache(&bob_state, &mut s, &vault, Some(pass)).unwrap();
        assert_eq!(vault.wards.len(), 2);
        assert!(vault.get_ward(&laden_did).is_some());
        assert!(vault.get_ward(&privat_did).is_some());
    }

    // Finding 11: Simulate timeout = 0 (unlocked session absent after initialize_profile_session)
    {
        let mut s = laden_state.service.lock().unwrap();
        let mut policy = laden_state
            .load_security_policy(&mut s, Some(pass))
            .unwrap();
        policy.session_timeout_seconds = 0;
        laden_state
            .save_security_policy(&mut s, policy, Some(pass))
            .unwrap();
    }
    let mut reloaded_svc = AppService::new(&laden_dir).unwrap();
    let folder_name = reloaded_svc.list_profiles().unwrap()[0].folder_name.clone();
    reloaded_svc
        .login(&folder_name, pass, false, "inst-laden".to_string())
        .unwrap();
    let reloaded_state = create_test_state(AppService::new(&laden_dir).unwrap());
    {
        reloaded_state
            .initialize_profile_session(&mut reloaded_svc, pass)
            .unwrap();
        *reloaded_state.service.lock().unwrap() = reloaded_svc;
    }
    // Session has no cached password (timeout = 0), yet status reads cleanly from cache!
    let status = get_recovery_protection_status_impl(None, &reloaded_state).unwrap();
    assert_eq!(status.guardians.len(), 3);
    assert_eq!(status.required_threshold, 2);
    assert_eq!(status.question_ids, Some([1, 2, 3]));
}

#[tokio::test]
async fn s07_f5_setup_rejects_self_guardian_and_same_root_subaccount_duplicates() {
    let base_dir = temp_test_dir("s07_f5_guardian_validation");
    let pass = "TestPassword123!";
    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);

    let bob_sub_dir = base_dir.join("bob_sub");
    let mut bob_sub_svc = AppService::new(&bob_sub_dir).unwrap();
    bob_sub_svc
        .create_profile(
            "Bob Work",
            BOB_PHRASE,
            None,
            Some("work"),
            pass,
            human_money_core::MnemonicLanguage::English,
            "inst-bob-work".into(),
            true,
        )
        .unwrap();
    let bob_work_did = bob_sub_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();

    {
        let mut s = alice_state.service.lock().unwrap();
        s.unlock_session(pass, 300).unwrap();
        // Inject self-contact directly into address book to prove setup_social_recovery_guardians
        // itself rejects own identity even if a self entry exists in storage.
        let mut contacts_map = std::collections::HashMap::new();
        for (did, name) in [
            (&alice_did, "Self"),
            (&bob_did, "Bob"),
            (&bob_work_did, "Bob Work"),
            (&carol_did, "Carol"),
        ] {
            contacts_map.insert(did.clone(), make_test_contact(did, name, Some(2)));
        }
        let ab = crate::models::FrontendAddressBook {
            contacts: contacts_map,
        };
        crate::state::save_encrypted_json(
            &mut s,
            crate::state::CONTACTS_DATA_NAME,
            &ab,
            Some(pass),
        )
        .unwrap();
        alice_state.set_cached_contacts_for_service(&s, Some(ab));
    }

    // 1. Self-guardian must be rejected explicitly (even when present in contacts)
    let self_err = setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![alice_did.clone(), bob_did.clone(), carol_did.clone()],
        2,
        [1, 2, 3],
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap_err();
    assert!(
        self_err.message.contains("own identity"),
        "S07-F5: own identity must be rejected explicitly, got: {}",
        self_err.message
    );

    // 2. Two subaccounts sharing the same root key (Bob + Bob Work) must be rejected as duplicates
    let dup_root_res = setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did, bob_work_did, carol_did],
        2,
        [1, 2, 3],
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await;
    assert!(
        dup_root_res.is_err(),
        "S07-F5: two guardian DIDs with the same Ed25519 root key must be rejected as duplicate"
    );

    let _ = fs::remove_dir_all(base_dir);
}

#[tokio::test]
async fn s07_f11_wrong_bip39_passphrase_rejected_and_preserves_session() {
    let base_dir = temp_test_dir("s07_f11_bip39_mismatch");
    let pass = "TestPassword123!";
    let bip39_pw = "Correct25thWord!";

    let alice_dir = base_dir.join("alice");
    let mut alice_svc = AppService::new(&alice_dir).unwrap();
    alice_svc
        .create_profile(
            "Alice Passphrase",
            ALICE_PHRASE,
            Some(bip39_pw),
            None,
            pass,
            human_money_core::MnemonicLanguage::English,
            "inst-alice-bp".into(),
            true,
        )
        .unwrap();
    alice_svc.unlock_session(pass, 300).unwrap();
    let alice_did = alice_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let alice_state = create_test_state(alice_svc);

    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    {
        let mut s = alice_state.service.lock().unwrap();
        for (did, name) in [
            (&bob_did, "Bob"),
            (&carol_did, "Carol"),
            (&dave_did, "Dave"),
        ] {
            alice_state
                .save_contact(&mut s, make_test_contact(did, name, Some(2)), Some(pass))
                .unwrap();
        }
    }

    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    let owner_state = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(pass))
            .unwrap()
            .unwrap()
    };
    for (g_state, g_did) in [(&bob_state, &bob_did), (&carol_state, &carol_did)] {
        let rec = owner_state
            .guardians
            .iter()
            .find(|g| g.user_id == *g_did)
            .unwrap();
        let mut s = g_state.service.lock().unwrap();
        let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
        vault.upsert_ward(rec.sealed_entry.clone()).unwrap();
        save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
    }

    let session_file = base_dir.join("active_recovery_session.json");
    let sess = start_social_recovery_session_impl(None, None, None, &session_file).unwrap();
    let bob_rep = approve_guardian_recovery_request_impl(
        sess.request_uri.clone(),
        alice_did.clone(),
        Some(pass),
        &bob_state,
    )
    .unwrap();
    let carol_rep = approve_guardian_recovery_request_impl(
        sess.request_uri.clone(),
        alice_did.clone(),
        Some(pass),
        &carol_state,
    )
    .unwrap();
    import_guardian_recovery_reply_impl(bob_rep.reply_uri, &session_file).unwrap();
    import_guardian_recovery_reply_impl(carol_rep.reply_uri, &session_file).unwrap();

    let new_device_dir = base_dir.join("new_device");
    let new_wallet_dir = new_device_dir.join("wallet_data");
    fs::create_dir_all(&new_wallet_dir).unwrap();
    let new_state = create_test_state(AppService::new(&new_wallet_dir).unwrap());

    let err = complete_social_recovery_impl(
        alice_did.clone(),
        1,
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        Some("Wrong25thWord!".to_string()),
        "restore_new_device".to_string(),
        "FreshPass!123".to_string(),
        Some("Alice".to_string()),
        None,
        "inst-new".to_string(),
        Some(true),
        None,
        &session_file,
        &new_state,
        &new_device_dir,
        &new_wallet_dir,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.code, "error.recovery.wrongBip39Passphrase",
        "S07-F11: wrong BIP39 passphrase must fail with error.recovery.wrongBip39Passphrase"
    );
    assert!(
        session_file.exists(),
        "S07-F11: active_recovery_session.json must NOT be deleted on wrong BIP39 passphrase"
    );

    // S07-F11: Invalid mode must be rejected cleanly without deleting session file
    let bad_mode = complete_social_recovery_impl(
        alice_did.clone(),
        1,
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        Some(bip39_pw.to_string()),
        "invalid_mode".to_string(),
        "FreshPass!123".to_string(),
        Some("Alice".to_string()),
        None,
        "inst-new".to_string(),
        Some(true),
        None,
        &session_file,
        &new_state,
        &new_device_dir,
        &new_wallet_dir,
        None,
    )
    .await;
    assert!(bad_mode.is_err());
    assert!(session_file.exists());

    let _ = fs::remove_dir_all(base_dir);
}

#[tokio::test]
async fn s07_f19_inconsistent_same_epoch_reply_rejected_and_tampered_share_not_wrong_answers() {
    let base_dir = temp_test_dir("s07_f19_epoch_metadata_and_error_honesty");
    let pass = "TestPassword123!";
    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    {
        let mut s = alice_state.service.lock().unwrap();
        s.unlock_session(pass, 300).unwrap();
        for (did, name) in [
            (&bob_did, "Bob"),
            (&carol_did, "Carol"),
            (&dave_did, "Dave"),
        ] {
            alice_state
                .save_contact(&mut s, make_test_contact(did, name, Some(2)), Some(pass))
                .unwrap();
        }
    }

    // Run setup #1 -> epoch 1 (bundle A)
    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();
    let owner_a = {
        let mut s = alice_state.service.lock().unwrap();
        load_recovery_owner_state(&mut s, Some(pass))
            .unwrap()
            .unwrap()
    };

    // Create a conflicting valid bundle B ALSO at epoch 1 (equivocating r_enc / commitments)
    let alice_sk = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet_and_identity(|_, id| id.signing_key.clone())
            .unwrap()
    };
    let entropy = human_money_core::services::mnemonic::MnemonicProcessor::to_entropy(
        ALICE_PHRASE,
        human_money_core::MnemonicLanguage::English,
    )
    .unwrap();
    let owner_canonical = canonicalize_owner_account_id(&alice_did).unwrap();
    let master_b = RecoveryMasterPayload::new(
        &owner_canonical,
        entropy,
        human_money_core::MnemonicLanguage::English,
        None,
        false,
    );
    let entries_b = create_social_recovery_vault_bundle(
        &alice_sk,
        &owner_canonical,
        "Alice",
        1, // same epoch 1!
        2,
        3,
        &[1, 2, 3],
        &["Berlin", "Fluffy", "Pizza"],
        &RecoveryKdfParams::new_fast_test(),
        &master_b,
        &[],
        1_760_000_000,
    )
    .unwrap();

    // Bob gets share 1 from bundle A; Carol gets share 2 from equivocating bundle B
    {
        let mut s = bob_state.service.lock().unwrap();
        let mut v = load_guardian_vault(&mut s, Some(pass)).unwrap();
        v.upsert_ward(owner_a.guardians[0].sealed_entry.clone())
            .unwrap();
        save_guardian_vault(&mut s, &v, Some(pass)).unwrap();
    }
    {
        let mut s = carol_state.service.lock().unwrap();
        let mut v = load_guardian_vault(&mut s, Some(pass)).unwrap();
        v.upsert_ward(entries_b[1].clone()).unwrap();
        save_guardian_vault(&mut s, &v, Some(pass)).unwrap();
    }

    let session_file = base_dir.join("active_recovery_session.json");
    let sess = start_social_recovery_session_impl(None, None, None, &session_file).unwrap();
    let bob_rep = approve_guardian_recovery_request_impl(
        sess.request_uri.clone(),
        alice_did.clone(),
        Some(pass),
        &bob_state,
    )
    .unwrap();
    let carol_rep = approve_guardian_recovery_request_impl(
        sess.request_uri.clone(),
        alice_did.clone(),
        Some(pass),
        &carol_state,
    )
    .unwrap();

    import_guardian_recovery_reply_impl(bob_rep.reply_uri, &session_file).unwrap();
    let inc_err =
        import_guardian_recovery_reply_impl(carol_rep.reply_uri, &session_file).unwrap_err();
    assert_eq!(
        inc_err.code, "error.recovery.inconsistentEpochMetadata",
        "S07-F19: conflicting same-epoch guardian release must be rejected at import time"
    );

    // Also verify tampered share commitment on disk does NOT report error.recovery.wrongAnswers
    let mut raw_sess = load_active_session_from_path(&session_file)
        .unwrap()
        .unwrap();
    let mut tampered_reply = raw_sess.replies[0].clone();
    tampered_reply.entry.share.index = 2;
    tampered_reply.entry.share.y[0] ^= 0xff;
    raw_sess.replies.push(tampered_reply);
    save_active_session_to_path(&session_file, &raw_sess).unwrap();

    let new_dev = base_dir.join("new_dev");
    let new_w = new_dev.join("wallet_data");
    fs::create_dir_all(&new_w).unwrap();
    let new_state = create_test_state(AppService::new(&new_w).unwrap());
    let tamper_err = complete_social_recovery_impl(
        alice_did,
        1,
        [
            "Berlin".to_string(),
            "Fluffy".to_string(),
            "Pizza".to_string(),
        ],
        None,
        "restore_new_device".to_string(),
        "FreshPass!123".to_string(),
        Some("Alice".to_string()),
        None,
        "inst-f19".to_string(),
        Some(true),
        None,
        &session_file,
        &new_state,
        &new_dev,
        &new_w,
        None,
    )
    .await
    .unwrap_err();
    assert_ne!(
        tamper_err.code, "error.recovery.wrongAnswers",
        "S07-F19: share commitment tampering must NOT be misreported as wrongAnswers"
    );

    let _ = fs::remove_dir_all(base_dir);
}

#[tokio::test]
async fn s09_x1_enqueue_guardian_locator_updates_aborts_when_session_superseded() {
    let base_dir = temp_test_dir("s09_x1_locator_superseded");
    let password = "TestPassword123!";
    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
    let (_, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
    let alice_state = std::sync::Arc::new(alice_state);

    let session_1 = {
        let mut svc = alice_state.lock_service().unwrap();
        alice_state
            .initialize_profile_session(&mut svc, password)
            .unwrap();
        let owner_state = RecoveryOwnerState {
            rotation_epoch: 1,
            threshold_m: 2,
            question_ids: [1, 2, 3],
            guardians: vec![GuardianAssignmentRecord {
                user_id: bob_did,
                display_name: "Bob".to_string(),
                share_index: 1,
                status: GuardianAssignmentStatus::Confirmed { acked_at: 100 },
                assigned_at: 90,
                sealed_entry: humoco_market::GuardianVaultEntry {
                    owner_did: alice_did,
                    owner_display_name: "Alice".to_string(),
                    rotation_epoch: 1,
                    threshold_m: 2,
                    total_n: 2,
                    share: humoco_market::ShamirShare {
                        index: 1,
                        y: [7u8; 32],
                    },
                    share_commitments: vec![],
                    r_enc: vec![],
                    kdf_params: RecoveryKdfParams::new_fast_test(),
                    question_ids: [1, 2, 3],
                    owner_sig: [0u8; 64],
                    backup_sync_descriptors: vec![],
                    updated_at: 90,
                },
            }],
            emergency_sheet_confirmed_at: None,
            answer_verifiers: None,
            rehearsal_stage: 0,
            next_rehearsal_at: None,
            rehearsal_kdf_m_kib: humoco_market::KDF_M_KIB_PRODUCTION,
            rehearsal_kdf_t_cost: humoco_market::KDF_T_COST_PRODUCTION,
        };
        save_recovery_owner_state(&mut svc, &owner_state, Some(password)).unwrap();
        alice_state.session_token_for_service(&svc).unwrap()
    };

    // Hold recovery_op_lock while spawning locator enqueue for session_1, then supersede session.
    let lock_guard = alice_state.recovery_op_lock.lock().await;
    let state_for_task = alice_state.clone();
    let session_for_task = session_1.clone();
    let pw_for_task = password.to_string();
    let handle = tokio::spawn(async move {
        enqueue_guardian_locator_updates_if_configured(
            &state_for_task,
            &session_for_task,
            Some(pw_for_task.as_str()),
        )
        .await
    });
    tokio::task::yield_now().await;

    {
        let mut svc = alice_state.lock_service().unwrap();
        alice_state.clear_all_caches();
        alice_state
            .initialize_profile_session(&mut svc, password)
            .unwrap();
    }
    drop(lock_guard);

    let enqueued = handle.await.unwrap().unwrap();
    assert_eq!(
        enqueued, 0,
        "S09-X1: superseded session must abort with Ok(0) without minting a fresh token"
    );
    let post_state = load_post_state(&alice_state, Some(password)).unwrap();
    assert!(
        post_state.outbox_entries.is_empty(),
        "S09-X1: superseded locator update must not enqueue into the new session's outbox"
    );

    let _ = fs::remove_dir_all(base_dir);
}
