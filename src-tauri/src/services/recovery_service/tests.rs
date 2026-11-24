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

struct TempDirGuard(std::path::PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        Self(temp_test_dir(label))
    }
}

impl std::ops::Deref for TempDirGuard {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.0.exists() {
            if let Err(err) = fs::remove_dir_all(&self.0) {
                eprintln!(
                    "TempDirGuard cleanup failed for {}: {}",
                    self.0.display(),
                    err
                );
            }
        }
    }
}

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

fn create_custom_wallet_state(
    base_dir: &Path,
    name: &str,
    dir_name: &str,
    phrase: &str,
    bip39_pass: Option<&str>,
    prefix: Option<&str>,
    pass: &str,
) -> (std::path::PathBuf, AppState, String) {
    let dir = base_dir.join(dir_name);
    let mut svc = AppService::new(&dir).unwrap();
    svc.create_profile(
        name,
        phrase,
        bip39_pass,
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
}

fn save_trusted_contacts(state: &AppState, pass: &str, contacts: &[(&str, &str, u8)]) {
    let mut s = state.service.lock().unwrap();
    for &(did, name, trust_level) in contacts {
        state
            .save_contact(
                &mut s,
                make_test_contact(did, name, Some(trust_level)),
                Some(pass),
            )
            .unwrap();
    }
}

fn load_owner_state_unwrap(state: &AppState, pass: &str) -> RecoveryOwnerState {
    let mut s = state.service.lock().unwrap();
    load_recovery_owner_state(&mut s, Some(pass))
        .unwrap()
        .unwrap()
}

fn deliver_sealed_entry_to_guardian(
    owner_state: &RecoveryOwnerState,
    guardian_state: &AppState,
    guardian_did: &str,
    pass: &str,
) {
    let rec = owner_state
        .guardians
        .iter()
        .find(|g| g.user_id == guardian_did)
        .expect("guardian assignment must exist in owner state");
    let mut s = guardian_state.service.lock().unwrap();
    let mut vault = load_guardian_vault(&mut s, Some(pass)).unwrap();
    vault.upsert_ward(rec.sealed_entry.clone()).unwrap();
    save_guardian_vault(&mut s, &vault, Some(pass)).unwrap();
}

fn default_answers() -> [String; 3] {
    [
        "Berlin".to_string(),
        "Fluffy".to_string(),
        "Pizza".to_string(),
    ]
}

fn check_frontend_err(
    err: &crate::models::FrontendError,
    expected_code: Option<&str>,
    expected_substr: &str,
) {
    if let Some(code) = expected_code {
        assert_eq!(err.code, code, "unexpected FrontendError code: {:?}", err);
    }
    assert!(
        err.message.contains(expected_substr),
        "expected FrontendError message containing {:?}, got: {:?}",
        expected_substr,
        err
    );
}

fn assert_frontend_err<T: std::fmt::Debug>(
    res: Result<T, crate::models::FrontendError>,
    expected_code: Option<&str>,
    expected_substr: &str,
) {
    let err = res.expect_err("expected FrontendError");
    check_frontend_err(&err, expected_code, expected_substr);
}

struct Approved3Of5Fixture {
    base_dir: TempDirGuard,
    alice_state: AppState,
    alice_did: String,
    bob_state: AppState,
    bob_did: String,
    carol_did: String,
    dave_did: String,
    session_file: std::path::PathBuf,
    setup_res: crate::models::SetupSocialRecoveryResultDto,
    active_sess: crate::models::ActiveRecoverySessionDto,
    progress: crate::models::ActiveRecoverySessionDto,
}

async fn setup_3_of_5_approved_fixture(label: &str, password: &str) -> Approved3Of5Fixture {
    let base_dir = TempDirGuard::new(label);
    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
    let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
    let (dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);
    let (_eve_state, eve_did) = create_wallet_state(&base_dir, "Eve", READER2_PHRASE, password);
    let (_frank_state, frank_did) = create_wallet_state(&base_dir, "Frank", "", password);

    save_trusted_contacts(
        &alice_state,
        password,
        &[
            (&bob_did, "Bob", 2),
            (&carol_did, "Carol", 2),
            (&dave_did, "Dave", 2),
            (&eve_did, "Eve", 2),
            (&frank_did, "Frank", 2),
        ],
    );

    let setup_res = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![
            bob_did.clone(),
            carol_did.clone(),
            dave_did.clone(),
            eve_did,
            frank_did,
        ],
        3,
        [1, 2, 3],
        default_answers(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    let owner_state = load_owner_state_unwrap(&alice_state, password);
    deliver_sealed_entry_to_guardian(&owner_state, &bob_state, &bob_did, password);
    deliver_sealed_entry_to_guardian(&owner_state, &carol_state, &carol_did, password);
    deliver_sealed_entry_to_guardian(&owner_state, &dave_state, &dave_did, password);

    let session_dir = base_dir.join("disaster_session");
    fs::create_dir_all(&session_dir).unwrap();
    let session_file = session_dir.join("active_recovery_session.json");

    let active_sess = start_social_recovery_session_impl(
        Some("Alice".to_string()),
        Some("restore_new_device".to_string()),
        None,
        &session_file,
    )
    .unwrap();

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

    import_guardian_recovery_reply_impl(bob_approval.reply_uri, &session_file).unwrap();
    import_guardian_recovery_reply_impl(carol_approval.reply_uri, &session_file).unwrap();
    let progress =
        import_guardian_recovery_reply_impl(dave_approval.reply_uri, &session_file).unwrap();

    Approved3Of5Fixture {
        base_dir,
        alice_state,
        alice_did,
        bob_state,
        bob_did,
        carol_did,
        dave_did,
        session_file,
        setup_res,
        active_sess,
        progress,
    }
}

#[tokio::test]
async fn test_emergency_sheet_confirmation_and_status() {
    let base_dir = TempDirGuard::new("emergency_sheet_test");
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
    assert_frontend_err(
        wrong_res,
        None,
        "Das Kontrollwort stimmt nicht mit deinem Notfall-Blatt überein.",
    );

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

    // 4. Protection status reflects confirmation with exact timestamp
    let updated_status = get_recovery_protection_status_impl(Some(password), &alice_state).unwrap();
    assert_eq!(
        updated_status.emergency_sheet_confirmed_at,
        Some(confirm_res.confirmed_at)
    );
    assert!(updated_status.has_identity_recovery);
}

#[tokio::test]
async fn test_export_profile_mnemonic_kit() {
    let base_dir = TempDirGuard::new("mnemonic_kit_test");
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
    let base_dir = TempDirGuard::new("validation_guards_test");
    let password = "TestPassword123!";
    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
    let (_carol_state, carol_did) =
        create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

    // 1. Less than 3 guardians rejected
    let err_few = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did.clone(), carol_did.clone()],
        2,
        [1, 2, 3],
        ["A".to_string(), "B".to_string(), "C".to_string()],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .expect_err("fewer than 3 guardians must be rejected");
    check_frontend_err(
        &err_few,
        None,
        "Number of guardians must be between 3 and 16, got 2",
    );

    // 2. Duplicate guardians rejected
    let err_dup = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did.clone(), bob_did.clone(), carol_did.clone()],
        2,
        [1, 2, 3],
        ["A".to_string(), "B".to_string(), "C".to_string()],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .expect_err("duplicate guardian must be rejected");
    check_frontend_err(&err_dup, None, "Duplicate guardian identity in setup:");

    // 3. Guardians not in AddressBook rejected
    let err_missing_contact = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did.clone()],
        2,
        [1, 2, 3],
        ["A".to_string(), "B".to_string(), "C".to_string()],
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .expect_err("guardian missing from contacts must be rejected");
    check_frontend_err(&err_missing_contact, None, "is not in your contacts");

    // 4. Guardians in AddressBook with trust level 0 rejected
    save_trusted_contacts(
        &alice_state,
        password,
        &[(&bob_did, "Bob", 0), (&carol_did, "Carol", 2), (&dave_did, "Dave", 2)],
    );
    let err_low_trust = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did, carol_did, dave_did],
        2,
        [1, 2, 3],
        default_answers(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .expect_err("guardian with trust_level 0 must be rejected");
    check_frontend_err(&err_low_trust, None, "does not have trust level >= 1");
}

#[tokio::test]
async fn test_3_of_5_social_recovery_setup_and_guardian_approval() {
    let password = "TestPassword123!";
    let f = setup_3_of_5_approved_fixture("social_recovery_setup_approval", password).await;

    assert_eq!(f.setup_res.rotation_epoch, 1);
    assert_eq!(f.setup_res.threshold_m, 3);
    assert_eq!(f.setup_res.guardians_count, 5);
    assert_eq!(f.active_sess.collected_count, 0);
    assert!(!f.active_sess.has_enough_shares);
    assert_eq!(f.active_sess.session_mode, "restore_new_device");

    let post_state = load_post_state(&f.alice_state, Some(password)).unwrap();
    assert_eq!(post_state.outbox_entries.len(), 5);

    {
        let mut s = f.alice_state.service.lock().unwrap();
        let mut os = load_recovery_owner_state(&mut s, Some(password))
            .unwrap()
            .unwrap();
        for g in &mut os.guardians {
            if g.user_id == f.bob_did || g.user_id == f.carol_did || g.user_id == f.dave_did {
                g.status = GuardianAssignmentStatus::Confirmed {
                    acked_at: Utc::now().timestamp() as u64,
                };
            }
        }
        save_recovery_owner_state(&mut s, &os, Some(password)).unwrap();
    }

    let prot_status = get_recovery_protection_status_impl(Some(password), &f.alice_state).unwrap();
    assert_eq!(prot_status.confirmed_guardians_count, 3);
    assert!(prot_status.has_identity_recovery);

    let bob_inspection = inspect_guardian_recovery_request_impl(
        f.active_sess.request_uri.clone(),
        Some(password),
        &f.bob_state,
    )
    .unwrap();
    assert_eq!(bob_inspection.sas_code, f.active_sess.sas_code);
    assert_eq!(bob_inspection.wards.len(), 1);
    assert!(are_dids_equal(
        &bob_inspection.wards[0].owner_did,
        &f.alice_did
    ));
    assert_eq!(f.progress.collected_count, 3);
    assert!(f.progress.has_enough_shares);
}

#[tokio::test]
async fn test_3_of_5_social_recovery_wrong_answers_preserves_session_then_restores() {
    let password = "TestPassword123!";
    let f = setup_3_of_5_approved_fixture("social_recovery_restore", password).await;

    let restored_base = f.base_dir.join("restored_wallet");
    let restored_wallet_dir = restored_base.join("wallet");
    let restored_state = create_test_state(AppService::new(&restored_wallet_dir).unwrap());

    // Wrong answers must fail with exact wrongAnswers code and keep session file intact.
    let wrong_res = complete_social_recovery_impl(
        f.alice_did.clone(),
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
        &f.session_file,
        &restored_state,
        &restored_base,
        &restored_wallet_dir,
        None,
    )
    .await;
    assert_frontend_err(
        wrong_res,
        Some("error.recovery.wrongAnswers"),
        "Deine Antworten stimmen noch nicht",
    );
    assert!(
        f.session_file.exists(),
        "Active recovery session MUST NOT be deleted when answers are incorrect!"
    );

    // Right answers without allow_empty_fallback ask for explicit seed-only confirmation.
    let confirm_res = complete_social_recovery_impl(
        f.alice_did.clone(),
        1,
        default_answers(),
        None,
        "restore_new_device".to_string(),
        "NewPassword456!".to_string(),
        Some("Alice Restored".to_string()),
        None,
        "device-restored-1".to_string(),
        Some(false),
        None,
        &f.session_file,
        &restored_state,
        &restored_base,
        &restored_wallet_dir,
        None,
    )
    .await
    .unwrap();
    assert_eq!(confirm_res.outcome, "sync_empty_requires_confirm");
    assert!(f.session_file.exists());

    // Confirm seed-only restore -> restores identity and deletes active session file.
    let right_res = complete_social_recovery_impl(
        f.alice_did.clone(),
        1,
        default_answers(),
        None,
        "restore_new_device".to_string(),
        "NewPassword456!".to_string(),
        Some("Alice Restored".to_string()),
        None,
        "device-restored-1".to_string(),
        Some(true),
        None,
        &f.session_file,
        &restored_state,
        &restored_base,
        &restored_wallet_dir,
        None,
    )
    .await
    .unwrap();
    assert_eq!(right_res.outcome, "restored_seed_only");
    assert!(!f.session_file.exists());

    let restored_did = {
        let s = restored_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };
    assert_eq!(restored_did, f.alice_did);
}

#[tokio::test]
async fn test_local_password_reset_mode() {
    let base_dir = TempDirGuard::new("password_reset_test");
    let old_pass = "OldPassword123!";
    let new_pass = "NewPassword456!";

    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, old_pass);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, old_pass);
    let (carol_state, carol_did) =
        create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, old_pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, old_pass);

    save_trusted_contacts(
        &alice_state,
        old_pass,
        &[(&bob_did, "Bob", 2), (&carol_did, "Carol", 2), (&dave_did, "Dave", 2)],
    );

    let reset_answers = [
        "AnswerOne1".to_string(),
        "AnswerTwo2".to_string(),
        "AnswerThree3".to_string(),
    ];
    setup_social_recovery_guardians_with_params(
        old_pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did],
        2,
        [1, 2, 3],
        reset_answers.clone(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    let owner_state = load_owner_state_unwrap(&alice_state, old_pass);
    deliver_sealed_entry_to_guardian(&owner_state, &bob_state, &bob_did, old_pass);
    deliver_sealed_entry_to_guardian(&owner_state, &carol_state, &carol_did, old_pass);

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
        session_dto.request_uri,
        alice_did.clone(),
        Some(old_pass),
        &carol_state,
    )
    .unwrap();
    import_guardian_recovery_reply_impl(b_rep.reply_uri, &session_file).unwrap();
    import_guardian_recovery_reply_impl(c_rep.reply_uri, &session_file).unwrap();

    let folder_name = alice_state.service.lock().unwrap().list_profiles().unwrap()[0]
        .folder_name
        .clone();
    let alice_wallet_dir = base_dir.join("Alice");
    let outcome = complete_social_recovery_impl(
        alice_did,
        1,
        reset_answers,
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

    let mut s = alice_state.service.lock().unwrap();
    s.logout();
    let old_err = s
        .login(&folder_name, old_pass, false, "device-reset-1".to_string())
        .expect_err("Old password must not unlock after reset");
    assert!(
        matches!(
            &old_err,
            human_money_core::Error::Crypto(msg) if msg.contains("Login failed (check password)")
        ),
        "Expected password crypto failure, got: {:?}",
        old_err
    );
    s.login(&folder_name, new_pass, false, "device-reset-1".to_string())
        .expect("New password must unlock the wallet");
}

#[tokio::test]
async fn test_inspect_emergency_kit_uri_12_and_24_words() {
    let base_dir = TempDirGuard::new("inspect_emergency_kit");
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
    let base_dir = TempDirGuard::new("revoke_recovery_test");
    let pass = "TestPassword123!";

    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    save_trusted_contacts(
        &alice_state,
        pass,
        &[(&bob_did, "Bob", 2), (&carol_did, "Carol", 2), (&dave_did, "Dave", 2)],
    );

    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did, carol_did, dave_did],
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

    let owner_state = load_owner_state_unwrap(&alice_state, pass);
    assert_eq!(owner_state.rotation_epoch, 2);
    assert!(owner_state.guardians.is_empty());

    let post_state = load_post_state(&alice_state, Some(pass)).unwrap();
    assert!(post_state
        .outbox_entries
        .iter()
        .any(|e| !e.entry.ciphertext.is_empty()));
}

#[tokio::test]
async fn test_ward_vault_max_limit_and_epoch_guards() {
    use humoco_market::GuardianVaultStore;

    let mut vault = GuardianVaultStore::default();
    let password = "TestPassword123!";
    let base_dir = TempDirGuard::new("ward_vault_test");

    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, password);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, password);
    let (_carol_state, carol_did) =
        create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, password);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, password);

    save_trusted_contacts(
        &alice_state,
        password,
        &[(&bob_did, "Bob", 2), (&carol_did, "Carol", 2), (&dave_did, "Dave", 2)],
    );

    let setup_res = setup_social_recovery_guardians_with_params(
        password.to_string(),
        vec![bob_did, carol_did, dave_did],
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
    assert_eq!(setup_res.rotation_epoch, 1);

    let owner_state = load_owner_state_unwrap(&alice_state, password);
    let valid_entry = owner_state.guardians[0].sealed_entry.clone();

    // 1. Upsert + idempotent re-upsert succeed for epoch 1
    vault.upsert_ward(valid_entry.clone()).unwrap();
    assert_eq!(vault.wards.len(), 1);
    vault.upsert_ward(valid_entry.clone()).unwrap();

    // 2. Fill up to 64 wards and verify an authentic 65th ward is rejected on capacity
    vault.wards.clear();
    for i in 0..64 {
        let mut dummy = valid_entry.clone();
        dummy.owner_did = format!("did:key:dummy_owner_{}", i);
        assert!(vault.wards.insert(dummy.owner_did.clone(), dummy).is_none());
    }
    assert_eq!(vault.wards.len(), 64);

    let err_excess = vault
        .upsert_ward(valid_entry)
        .expect_err("65th ward must be rejected on vault capacity");
    assert!(
        matches!(
            &err_excess,
            humoco_market::MarketError::Validation(msg)
                if msg.contains("Guardian vault capacity reached (maximum 64 wards)")
        ),
        "expected capacity Validation error, got: {:?}",
        err_excess
    );
}

#[tokio::test]
async fn test_probe_sync_backup_unreachable_and_empty_guard() {
    let base_dir = TempDirGuard::new("mirror_discovery_test");
    let wallet_dir = base_dir.join("empty");
    let empty_service = AppService::new(&wallet_dir).unwrap();
    let state = create_test_state(empty_service);
    let tmp_restore = base_dir.join("tmp_restore");
    fs::create_dir_all(&tmp_restore).unwrap();

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
    assert_frontend_err(empty_res, None, "Sync-Dienst-Adresse darf nicht leer sein");

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
    let unreach_err = unreach_res
        .error
        .expect("unreachable probe must include error details");
    assert!(
        unreach_err.contains("http://127.0.0.1:9"),
        "expected error details referencing target URL, got: {unreach_err}"
    );
}

#[tokio::test]
async fn test_reply_import_guards_and_ward_contact_name_resolution() {
    let base_dir = TempDirGuard::new("reply_import_guards_test");
    let pass = "TestPassword123!";

    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Privat", ALICE_PHRASE, pass);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);
    let (eve_state, eve_did) = create_wallet_state(&base_dir, "Eve", READER2_PHRASE, pass);

    let guardians = vec![bob_did.clone(), carol_did.clone(), dave_did.clone()];
    let contacts = [
        (bob_did.as_str(), "Bob", 2),
        (carol_did.as_str(), "Carol", 2),
        (dave_did.as_str(), "Dave", 2),
    ];
    save_trusted_contacts(&alice_state, pass, &contacts);
    save_trusted_contacts(&eve_state, pass, &contacts);

    // Bob saves Alice in his AddressBook as "Hans Müller"
    {
        let mut s = bob_state.service.lock().unwrap();
        let mut hans_contact = make_test_contact(&alice_did, "Hans", Some(2));
        hans_contact.profile.last_name = Some("Müller".to_string());
        bob_state
            .save_contact(&mut s, hans_contact, Some(pass))
            .unwrap();
    }

    let alice_answers = [
        "Alpha123".to_string(),
        "Bravo456".to_string(),
        "Charlie789".to_string(),
    ];
    // Alice sets up epoch 1 (Carol keeps), then rotates to epoch 2 (Bob gets)
    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        guardians.clone(),
        2,
        [1, 2, 3],
        alice_answers.clone(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();
    let owner_e1 = load_owner_state_unwrap(&alice_state, pass);
    deliver_sealed_entry_to_guardian(&owner_e1, &carol_state, &carol_did, pass);

    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        guardians.clone(),
        2,
        [1, 2, 3],
        alice_answers,
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();
    let owner_e2 = load_owner_state_unwrap(&alice_state, pass);
    deliver_sealed_entry_to_guardian(&owner_e2, &bob_state, &bob_did, pass);

    // Eve sets up recovery so Bob holds a second ward for mismatch test
    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        guardians,
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
    let eve_owner = load_owner_state_unwrap(&eve_state, pass);
    deliver_sealed_entry_to_guardian(&eve_owner, &bob_state, &bob_did, pass);

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

    // Bob approves Alice (epoch 2), import once -> added, twice -> duplicate
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
    let dup = import_guardian_recovery_reply_impl(bob_alice_rep.reply_uri, &session_file).unwrap();
    assert_eq!(dup.last_import_status.as_deref(), Some("duplicate"));
    assert_eq!(dup.collected_count, 1);

    // Stale epoch reply (Carol epoch 1 < epoch 2) and wrong ward (Eve != Alice) rejected
    let carol_stale_rep = approve_guardian_recovery_request_impl(
        sess.request_uri.clone(),
        alice_did,
        Some(pass),
        &carol_state,
    )
    .unwrap();
    let stale_err =
        import_guardian_recovery_reply_impl(carol_stale_rep.reply_uri, &session_file).unwrap_err();
    assert_eq!(stale_err.code, "error.recovery.staleEpoch");

    let bob_eve_rep =
        approve_guardian_recovery_request_impl(sess.request_uri, eve_did, Some(pass), &bob_state)
            .unwrap();
    let mismatch_err =
        import_guardian_recovery_reply_impl(bob_eve_rep.reply_uri, &session_file).unwrap_err();
    assert_eq!(mismatch_err.code, "error.recovery.wrongProfile");
}

#[tokio::test]
async fn test_12_question_setup_and_spaced_repetition_rehearsal() {
    let base_dir = TempDirGuard::new("recovery_rehearsal");
    let pass = "RehearsalPass!99";
    let (alice_state, _alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    {
        let mut s = alice_state.service.lock().unwrap();
        s.unlock_session(pass, 300).unwrap();
    }
    save_trusted_contacts(
        &alice_state,
        pass,
        &[(&bob_did, "Bob", 3), (&carol_did, "Carol", 2), (&dave_did, "Dave", 1)],
    );

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
    let base_dir = TempDirGuard::new("subaccount_recovery_isolation_and_cache");
    let pass = "TestPassword123!";

    let (laden_dir, laden_state, laden_did) = create_custom_wallet_state(
        &base_dir,
        "Alice Laden",
        "laden",
        ALICE_PHRASE,
        None,
        Some("laden"),
        pass,
    );
    let (_privat_dir, privat_state, privat_did) = create_custom_wallet_state(
        &base_dir,
        "Alice Privat",
        "privat",
        ALICE_PHRASE,
        None,
        Some("privat"),
        pass,
    );
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    let contacts = [
        (bob_did.as_str(), "Bob", 2),
        (carol_did.as_str(), "Carol", 2),
        (dave_did.as_str(), "Dave", 2),
    ];
    save_trusted_contacts(&laden_state, pass, &contacts);
    save_trusted_contacts(&privat_state, pass, &contacts);

    let guardians = vec![bob_did.clone(), carol_did, dave_did];
    let laden_setup = setup_social_recovery_guardians_with_params(
        pass.to_string(),
        guardians.clone(),
        2,
        [1, 2, 3],
        default_answers(),
        RecoveryKdfParams::new_fast_test(),
        &laden_state,
        None,
    )
    .await
    .unwrap();
    let privat_setup = setup_social_recovery_guardians_with_params(
        pass.to_string(),
        guardians,
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

    let laden_owner = load_owner_state_unwrap(&laden_state, pass);
    let privat_owner = load_owner_state_unwrap(&privat_state, pass);
    assert_eq!(laden_owner.guardians[0].sealed_entry.owner_did, laden_did);
    assert_eq!(privat_owner.guardians[0].sealed_entry.owner_did, privat_did);

    // Bob stores guardian entries for both subaccounts — second setup must NOT overwrite first
    deliver_sealed_entry_to_guardian(&laden_owner, &bob_state, &bob_did, pass);
    deliver_sealed_entry_to_guardian(&privat_owner, &bob_state, &bob_did, pass);
    {
        let mut s = bob_state.service.lock().unwrap();
        let vault = load_guardian_vault_with_cache(&bob_state, &mut s, Some(pass)).unwrap();
        assert_eq!(vault.wards.len(), 2);
        assert_eq!(
            vault
                .get_ward(&laden_did)
                .expect("laden ward must exist")
                .owner_did,
            laden_did
        );
        assert_eq!(
            vault
                .get_ward(&privat_did)
                .expect("privat ward must exist")
                .owner_did,
            privat_did
        );
    }

    // Timeout = 0 (unlocked session absent after initialize_profile_session): status reads cleanly from cache
    {
        let mut s = laden_state.service.lock().unwrap();
        let mut policy = laden_state.load_security_policy(&mut s, Some(pass)).unwrap();
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
    reloaded_state
        .initialize_profile_session(&mut reloaded_svc, pass)
        .unwrap();
    *reloaded_state.service.lock().unwrap() = reloaded_svc;

    let status = get_recovery_protection_status_impl(None, &reloaded_state).unwrap();
    assert_eq!(status.guardians.len(), 3);
    assert_eq!(status.required_threshold, 2);
    assert_eq!(status.question_ids, Some([1, 2, 3]));
}

#[tokio::test]
async fn s07_f5_setup_rejects_self_guardian_and_same_root_subaccount_duplicates() {
    let base_dir = TempDirGuard::new("s07_f5_guardian_validation");
    let pass = "TestPassword123!";
    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (_bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (_carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_, _, bob_work_did) =
        create_custom_wallet_state(&base_dir, "Bob Work", "bob_sub", BOB_PHRASE, None, Some("work"), pass);

    {
        let mut s = alice_state.service.lock().unwrap();
        s.unlock_session(pass, 300).unwrap();
        let contacts_map = [
            (&alice_did, "Self"),
            (&bob_did, "Bob"),
            (&bob_work_did, "Bob Work"),
            (&carol_did, "Carol"),
        ]
        .into_iter()
        .map(|(did, name)| (did.clone(), make_test_contact(did, name, Some(2))))
        .collect();
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
        vec![alice_did, bob_did.clone(), carol_did.clone()],
        2,
        [1, 2, 3],
        default_answers(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .expect_err("self-guardian must be rejected");
    check_frontend_err(&self_err, None, "own identity");

    // 2. Two subaccounts sharing the same root key (Bob + Bob Work) must be rejected as duplicates
    let dup_root_err = setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did, bob_work_did, carol_did],
        2,
        [1, 2, 3],
        default_answers(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .expect_err("two guardians sharing root Ed25519 key must be rejected");
    check_frontend_err(&dup_root_err, None, "Duplicate guardian identity in setup:");
}

#[tokio::test]
async fn s07_f11_wrong_bip39_passphrase_rejected_and_preserves_session() {
    let base_dir = TempDirGuard::new("s07_f11_bip39_mismatch");
    let pass = "TestPassword123!";
    let bip39_pw = "Correct25thWord!";

    let (_, alice_state, alice_did) = create_custom_wallet_state(
        &base_dir,
        "Alice Passphrase",
        "alice",
        ALICE_PHRASE,
        Some(bip39_pw),
        None,
        pass,
    );
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    save_trusted_contacts(
        &alice_state,
        pass,
        &[(&bob_did, "Bob", 2), (&carol_did, "Carol", 2), (&dave_did, "Dave", 2)],
    );

    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did.clone(), dave_did],
        2,
        [1, 2, 3],
        default_answers(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();

    let owner_state = load_owner_state_unwrap(&alice_state, pass);
    deliver_sealed_entry_to_guardian(&owner_state, &bob_state, &bob_did, pass);
    deliver_sealed_entry_to_guardian(&owner_state, &carol_state, &carol_did, pass);

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
        sess.request_uri,
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

    let err_res = complete_social_recovery_impl(
        alice_did.clone(),
        1,
        default_answers(),
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
    .await;
    assert_frontend_err(
        err_res,
        Some("error.recovery.wrongBip39Passphrase"),
        "BIP39-Passphrase stimmt nicht",
    );
    assert!(session_file.exists());

    let bad_mode = complete_social_recovery_impl(
        alice_did,
        1,
        default_answers(),
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
    assert_frontend_err(bad_mode, None, "Invalid recovery mode: invalid_mode");
    assert!(session_file.exists());
}

#[tokio::test]
async fn s07_f19_inconsistent_same_epoch_reply_rejected_and_tampered_share_not_wrong_answers() {
    let base_dir = TempDirGuard::new("s07_f19_epoch_metadata_and_error_honesty");
    let pass = "TestPassword123!";
    let (alice_state, alice_did) = create_wallet_state(&base_dir, "Alice", ALICE_PHRASE, pass);
    let (bob_state, bob_did) = create_wallet_state(&base_dir, "Bob", BOB_PHRASE, pass);
    let (carol_state, carol_did) = create_wallet_state(&base_dir, "Carol", CHARLIE_PHRASE, pass);
    let (_dave_state, dave_did) = create_wallet_state(&base_dir, "Dave", READER1_PHRASE, pass);

    {
        alice_state
            .service
            .lock()
            .unwrap()
            .unlock_session(pass, 300)
            .unwrap();
    }
    save_trusted_contacts(
        &alice_state,
        pass,
        &[(&bob_did, "Bob", 2), (&carol_did, "Carol", 2), (&dave_did, "Dave", 2)],
    );

    setup_social_recovery_guardians_with_params(
        pass.to_string(),
        vec![bob_did.clone(), carol_did, dave_did],
        2,
        [1, 2, 3],
        default_answers(),
        RecoveryKdfParams::new_fast_test(),
        &alice_state,
        None,
    )
    .await
    .unwrap();
    let owner_a = load_owner_state_unwrap(&alice_state, pass);

    // Create equivocating bundle B ALSO at epoch 1
    let alice_sk = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet_and_identity(|_, id| id.signing_key.clone())
        .unwrap();
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
        1,
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

    deliver_sealed_entry_to_guardian(&owner_a, &bob_state, &bob_did, pass);
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
        sess.request_uri,
        alice_did.clone(),
        Some(pass),
        &carol_state,
    )
    .unwrap();

    import_guardian_recovery_reply_impl(bob_rep.reply_uri, &session_file).unwrap();
    assert_frontend_err(
        import_guardian_recovery_reply_impl(carol_rep.reply_uri, &session_file),
        Some("error.recovery.inconsistentEpochMetadata"),
        "conflicting recovery metadata",
    );

    // Tampered share commitment on disk must report exact validation mismatch, NOT wrongAnswers
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
    let tamper_res = complete_social_recovery_impl(
        alice_did,
        1,
        default_answers(),
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
    .await;
    let tamper_err = tamper_res
        .as_ref()
        .expect_err("tampered share must be rejected");
    assert_ne!(
        tamper_err.code, "error.recovery.wrongAnswers",
        "S07-F19: share commitment tampering must NOT be misreported as wrongAnswers"
    );
    assert_frontend_err(
        tamper_res,
        Some("error.market.validation"),
        "Share commitment mismatch for index 2",
    );
}

#[tokio::test]
async fn s09_x1_enqueue_guardian_locator_updates_aborts_when_session_superseded() {
    let base_dir = TempDirGuard::new("s09_x1_locator_superseded");
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
}
