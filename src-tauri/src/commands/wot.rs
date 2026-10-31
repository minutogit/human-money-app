use chrono::Utc;
use serde::{Deserialize, Serialize};
use human_money_core::models::secure_container::{ContainerConfig, PrivacyMode};
use crate::models::{
    FrontendError, FrontendUserProfile, FrontendVoucherTrustDto, FrontendWotImportResultDto,
    FrontendWotSummaryDto,
};
use crate::state::AppState;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct WotFileEnvelope {
    pub format_version: u8,
    pub sender_did: String,
    pub sender_profile: Option<FrontendUserProfile>,
    pub sync_package: humoco_wot::WotSyncPackage,
}

#[tauri::command]
pub fn get_wot_summary(state: tauri::State<AppState>) -> Result<FrontendWotSummaryDto, FrontendError> {
    get_wot_summary_impl(&state)
}

pub fn get_wot_summary_impl(state: &AppState) -> Result<FrontendWotSummaryDto, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let reg = state.get_or_load_wot_register(&mut service, None).map_err(FrontendError::from)?;

    let rated_contacts_count = reg.own_contacts.values().filter(|e| e.level >= 1).count();
    let verified_humans_count = reg.own_contacts.values().filter(|e| e.level == 0).count();
    let close_confidants_count = reg.own_contacts.values().filter(|e| e.level == 3).count();
    let imported_peer_lists_count = reg.raw_list_store.len();
    let own_list_version = reg.own_signed_list.as_ref().map(|l| l.v);

    Ok(FrontendWotSummaryDto {
        rated_contacts_count,
        verified_humans_count,
        close_confidants_count,
        imported_peer_lists_count,
        own_list_version,
    })
}

#[tauri::command]
pub fn export_wot_package(
    recipient_did: Option<String>,
    container_password: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    export_wot_package_impl(recipient_did, container_password, password, &state)
}

pub fn export_wot_package_impl(
    recipient_did: Option<String>,
    container_password: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<Vec<u8>, FrontendError> {
    let mut service = state.service.lock().unwrap();
    let (sender_did, sender_profile) = service
        .with_wallet(|w| (w.get_user_id().to_string(), Some(w.profile.to_public_profile().into())))
        .map_err(FrontendError::from)?;

    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let signed_changed = state
        .ensure_wot_list_signed(&mut service, &mut reg)
        .map_err(FrontendError::from)?;
    if signed_changed {
        state
            .save_wot_register(&mut service, &reg, password.as_deref())
            .map_err(FrontendError::from)?;
    }

    let rcpt_can = recipient_did.as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| crate::state::canonicalize_did(s).unwrap_or_else(|_| s.to_string()));
    let (sync_package, receipt) = reg
        .prepare_sync_package(rcpt_can.as_deref(), Some(60_000), true)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    let config = if let Some(rcpt) = rcpt_can.as_deref() {
        ContainerConfig::TargetDid(rcpt.to_string(), PrivacyMode::TrialDecryption)
    } else if let Some(pwd) = container_password.filter(|s| !s.is_empty()) {
        ContainerConfig::Password(pwd)
    } else {
        return Err(FrontendError::from(
            "Either recipientDid or containerPassword is required".to_string(),
        ));
    };

    let envelope = WotFileEnvelope {
        format_version: 1,
        sender_did,
        sender_profile,
        sync_package,
    };

    let envelope_bytes = serde_json::to_vec(&envelope)
        .map_err(|e| FrontendError::from(format!("Failed to serialize envelope: {}", e)))?;

    let container_bytes = service
        .seal_trust_container(&envelope_bytes, config)
        .map_err(FrontendError::from)?;

    if rcpt_can.is_some() {
        let now_secs = Utc::now().timestamp() as u64;
        reg.commit_sync_receipt(receipt, now_secs);
        let _ = state.save_wot_register(&mut service, &reg, password.as_deref());
    }

    Ok(container_bytes)
}

#[tauri::command]
pub fn import_wot_package(
    container_bytes: Vec<u8>,
    container_password: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    import_wot_package_impl(container_bytes, container_password, password, &state)
}

pub fn import_wot_package_impl(
    container_bytes: Vec<u8>,
    container_password: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendWotImportResultDto, FrontendError> {
    let mut service = state.service.lock().unwrap();

    let plaintext = service
        .open_trust_container(&container_bytes, container_password.as_deref(), |bytes| {
            let env: WotFileEnvelope = serde_json::from_slice(bytes)
                .map_err(|e| human_money_core::Error::Json(e.to_string()))?;
            Ok(env.sender_did)
        })
        .map_err(FrontendError::from)?;

    let envelope: WotFileEnvelope = serde_json::from_slice(&plaintext)
        .map_err(|e| FrontendError::from(format!("Failed to parse envelope: {}", e)))?;

    let mut reg = state
        .get_or_load_wot_register(&mut service, password.as_deref())
        .map_err(FrontendError::from)?;

    let can_sender = crate::state::canonicalize_did(&envelope.sender_did).ok();
    let now_secs = Utc::now().timestamp() as u64;
    let stats = reg
        .import_sync_package(&envelope.sync_package, can_sender.as_deref(), now_secs)
        .map_err(|e| FrontendError::from(e.to_string()))?;

    if reg.own_list_dirty {
        let _ = state.ensure_wot_list_signed(&mut service, &mut reg);
    }

    state
        .save_wot_register(&mut service, &reg, password.as_deref())
        .map_err(FrontendError::from)?;

    let contacts_cache = state.contacts.lock().unwrap();
    let is_known_contact = contacts_cache.as_ref().map(|book| {
        book.contacts.keys().any(|k| {
            k == &envelope.sender_did
                || (can_sender.is_some() && crate::state::canonicalize_did(k).ok() == can_sender)
        })
    }).unwrap_or(false);

    let existing_trust_level = can_sender.and_then(|c| reg.own_contacts.get(&c).map(|e| e.level));

    Ok(FrontendWotImportResultDto {
        sender_did: envelope.sender_did,
        sender_profile: envelope.sender_profile,
        imported: stats.imported,
        updated: stats.updated,
        merged: stats.merged,
        skipped_unchanged: stats.skipped_unchanged,
        rejected_spam: stats.rejected_spam,
        is_known_contact,
        existing_trust_level,
    })
}

#[tauri::command]
pub fn evaluate_voucher_trust_preview(
    creator_id: String,
    guarantor_ids: Vec<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendVoucherTrustDto, FrontendError> {
    evaluate_voucher_trust_preview_impl(creator_id, guarantor_ids, &state)
}

pub fn evaluate_voucher_trust_preview_impl(
    creator_id: String,
    guarantor_ids: Vec<String>,
    state: &AppState,
) -> Result<FrontendVoucherTrustDto, FrontendError> {
    let service = state.service.lock().unwrap();
    let trust = service
        .with_wallet(|w| state.evaluate_voucher_trust_preview(w, &creator_id, &guarantor_ids))
        .map_err(FrontendError::from)?;
    Ok(trust)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use human_money_core::app_service::AppService;
    use crate::models::{FrontendContact, FrontendUserProfile};

    const ALICE_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";

    fn temp_test_dir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("humoco_wot_test_{}_{}", name, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
        let mut service = AppService::new(dir).unwrap();
        service.create_profile(
            profile_name,
            phrase,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-wot-test".into(),
        ).unwrap();
        service
    }

    fn create_test_state(service: AppService) -> AppState {
        AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(None),
            regional_views: Mutex::new(None),
            wot: Mutex::new(None),
        }
    }

    #[test]
    fn test_contact_trust_sync_canonical_dedup_and_delete() {
        let base_dir = temp_test_dir("trust_sync");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let state = create_test_state(alice_service);

        let contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                last_name: Some("Tester".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
        };

        {
            let mut s = state.service.lock().unwrap();
            state.save_contact(&mut s, contact.clone(), Some(password)).unwrap();
        }

        // Verify WoT register reflects level 2 and desired_hops 3
        {
            let mut s = state.service.lock().unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            let entry = reg.own_contacts.get(&can_did).expect("Bob should be in own_contacts");
            assert_eq!(entry.level, 2);
            assert_eq!(entry.desired_hops, 3);
        }

        // Verify summary
        let summary = get_wot_summary_impl(&state).unwrap();
        assert_eq!(summary.rated_contacts_count, 1);
        assert_eq!(summary.verified_humans_count, 0);

        // Update Bob's trust level to 0 (verified human only)
        let mut updated_contact = contact;
        updated_contact.trust_level = Some(0);
        {
            let mut s = state.service.lock().unwrap();
            state.save_contact(&mut s, updated_contact, Some(password)).unwrap();
        }
        {
            let mut s = state.service.lock().unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            let entry = reg.own_contacts.get(&can_did).unwrap();
            assert_eq!(entry.level, 0);
            assert_eq!(entry.desired_hops, 0);
        }

        let summary2 = get_wot_summary_impl(&state).unwrap();
        assert_eq!(summary2.rated_contacts_count, 0);
        assert_eq!(summary2.verified_humans_count, 1);

        // Delete contact -> verify removed from register
        {
            let mut s = state.service.lock().unwrap();
            state.delete_contact(&mut s, &bob_did, Some(password)).unwrap();
        }
        {
            let mut s = state.service.lock().unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let can_did = crate::state::canonicalize_did(&bob_did).unwrap();
            assert!(!reg.own_contacts.contains_key(&can_did));
        }
    }

    #[test]
    fn test_container_export_import_roundtrip_and_invalid_password() {
        let base_dir = temp_test_dir("container_roundtrip");
        let alice_dir = base_dir.join("alice");
        let bob_dir = base_dir.join("bob");
        let password = "TestPassword123!";

        let alice_service = setup_test_app(&alice_dir, "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&bob_dir, "Bob", BOB_PHRASE, password);

        let alice_state = create_test_state(alice_service);
        let bob_state = create_test_state(bob_service);

        let bob_did = {
            let s = bob_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };
        let alice_did = {
            let s = alice_state.service.lock().unwrap();
            s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
        };

        // Alice adds Bob as contact level 2
        let bob_contact = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(2),
            desired_hops: Some(3),
            has_peer_list: false,
            peer_list_updated_at: None,
        };
        {
            let mut s = alice_state.service.lock().unwrap();
            alice_state.save_contact(&mut s, bob_contact, Some(password)).unwrap();
        }

        // 1. Password container export
        let container_pwd = "SecretContainerPass123!";
        let container_bytes = export_wot_package_impl(
            None,
            Some(container_pwd.to_string()),
            Some(password.to_string()),
            &alice_state,
        ).unwrap();

        // Bob attempts import with wrong container password -> should fail
        let wrong_pwd_res = import_wot_package_impl(
            container_bytes.clone(),
            Some("WrongPassword".to_string()),
            Some(password.to_string()),
            &bob_state,
        );
        assert!(wrong_pwd_res.is_err());

        // Bob imports with correct container password -> should succeed
        let import_res = import_wot_package_impl(
            container_bytes,
            Some(container_pwd.to_string()),
            Some(password.to_string()),
            &bob_state,
        ).unwrap();

        assert_eq!(import_res.sender_did, alice_did);
        assert_eq!(import_res.imported, 1);

        // 2. TargetDid container export (targeted to Bob's DID)
        let target_container_bytes = export_wot_package_impl(
            Some(bob_did.clone()),
            None,
            Some(password.to_string()),
            &alice_state,
        ).unwrap();

        // Bob imports without container password -> decrypted via Bob's DID key
        let target_import_res = import_wot_package_impl(
            target_container_bytes,
            None,
            Some(password.to_string()),
            &bob_state,
        ).unwrap();

        assert_eq!(target_import_res.sender_did, alice_did);
    }

    #[test]
    fn test_piggybacking_condition() {
        let base_dir = temp_test_dir("piggyback_cond");
        let password = "TestPassword123!";
        let alice_service = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, password);
        let bob_service = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, password);

        let bob_did = bob_service.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        let state = create_test_state(alice_service);

        // Bob added with level 0
        let contact_lvl0 = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(0),
            desired_hops: Some(0),
            has_peer_list: false,
            peer_list_updated_at: None,
        };
        {
            let mut s = state.service.lock().unwrap();
            state.save_contact(&mut s, contact_lvl0, Some(password)).unwrap();
        }

        // Test piggybacking evaluation logic: level 0 => must NOT piggyback
        {
            let mut s = state.service.lock().unwrap();
            let can_rcpt = crate::state::canonicalize_did(&bob_did).unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let should_piggyback = reg.own_contacts.get(&can_rcpt).is_some_and(|e| e.level >= 1);
            assert!(!should_piggyback);
        }

        // Update Bob to level 1 => must piggyback
        let contact_lvl1 = FrontendContact {
            did: bob_did.clone(),
            profile: FrontendUserProfile::default(),
            tags: vec![],
            added_at: "2026-01-01T00:00:00Z".to_string(),
            notes: None,
            trust_level: Some(1),
            desired_hops: Some(2),
            has_peer_list: false,
            peer_list_updated_at: None,
        };
        {
            let mut s = state.service.lock().unwrap();
            state.save_contact(&mut s, contact_lvl1, Some(password)).unwrap();
        }

        {
            let mut s = state.service.lock().unwrap();
            let can_rcpt = crate::state::canonicalize_did(&bob_did).unwrap();
            let reg = state.get_or_load_wot_register(&mut s, Some(password)).unwrap();
            let should_piggyback = reg.own_contacts.get(&can_rcpt).is_some_and(|e| e.level >= 1);
            assert!(should_piggyback);
        }
    }
}
