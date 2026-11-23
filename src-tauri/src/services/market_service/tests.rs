use super::ads::*;
use super::card_signing::*;
use super::media::*;
use super::notices::*;
use super::profile::*;
use super::storage::*;
use super::view_and_import::*;
use crate::models::{
    CardFieldVisibility, FieldAudience, FrontendAdInput, FrontendError,
    FrontendSaveMyMarketProfileRequest, FrontendSyncDescriptor, FrontendUserProfile,
    StoredOwnMarketProfile,
};
use crate::state::{AppState, MARKET_CARD_STORE_KEY, MARKET_OWN_PROFILE_KEY};
use ed25519_dalek::SigningKey;
use human_money_core::app_service::AppService;
use std::path::{Path, PathBuf};

const ALICE_PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const BOB_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
const CHARLIE_PHRASE: &str =
    "legal winner thank year wave sausage worth useful legal winner thank yellow";

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

fn temp_test_dir(name: &str) -> TempDirGuard {
    let p = std::env::temp_dir().join(format!(
        "humoco_market_test_{}_{}",
        name,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&p).unwrap();
    TempDirGuard(p)
}

fn assert_frontend_err_msg<T: std::fmt::Debug>(
    res: Result<T, FrontendError>,
    expected_substr: &str,
) {
    let err = res.expect_err("expected FrontendError");
    assert!(
        err.message.contains(expected_substr),
        "Expected FrontendError message containing {:?}, got code={:?} msg={:?}",
        expected_substr,
        err.code,
        err.message
    );
}

fn assert_frontend_err_code<T: std::fmt::Debug>(
    res: Result<T, FrontendError>,
    expected_code: &str,
) {
    let err = res.expect_err("expected FrontendError");
    assert_eq!(
        err.code, expected_code,
        "Expected FrontendError code {:?}, got code={:?} msg={:?}",
        expected_code, err.code, err.message
    );
}

fn offer_ad_input(title: &str, duration_days: u64, published: Option<bool>) -> FrontendAdInput {
    FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Offer,
        title: title.to_string(),
        text: None,
        pricing: None,
        duration_days,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: None,
        event_all_day: false,
        published,
    }
}

fn setup_test_app(dir: &Path, profile_name: &str, phrase: &str, password: &str) -> AppService {
    let mut service = AppService::new(dir).unwrap();
    service
        .create_profile(
            profile_name,
            phrase,
            None,
            Some("pc"),
            password,
            human_money_core::MnemonicLanguage::English,
            "device-market-test".into(),
            true,
        )
        .unwrap();
    service
}

fn create_test_state(service: AppService) -> AppState {
    AppState::new(service)
}

#[test]
fn test_ad_duration_validation_and_renewals() {
    let base_dir = temp_test_dir("ad_duration_and_renewals");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    // Invalid duration 365 rejected
    let bad_ad = offer_ad_input("Guitar lessons", 365, Some(true));
    assert_frontend_err_msg(
        upsert_ad_impl(bad_ad, Some(password.to_string()), &alice_state),
        "duration_days must be 30, 90, or 180",
    );

    // Valid duration 30 accepted
    let ad_30 = offer_ad_input("Guitar lessons", 30, Some(true));
    let p1 = upsert_ad_impl(ad_30, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p1.ads.len(), 1);
    let ad_id = p1.ads[0].id.clone();
    let pub_at = p1.ads[0].published_at;
    assert_eq!(p1.ads[0].renewed_at, None);
    assert_eq!(p1.ads[0].status, "active");

    // Renew ad with 90 days
    let p2 = renew_ad_impl(ad_id.clone(), 90, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(
        p2.ads[0].published_at, pub_at,
        "published_at must stay unchanged"
    );
    assert!(p2.ads[0].renewed_at.expect("renewed_at must be set") >= pub_at);
    assert!(p2.ads[0].expires_at > p1.ads[0].expires_at);

    // Renew ad with invalid 45 days rejected
    assert_frontend_err_msg(
        renew_ad_impl(ad_id.clone(), 45, Some(password.to_string()), &alice_state),
        "duration_days must be 30, 90, or 180",
    );

    // Renew ad with 180 days succeeds
    let p3 = renew_ad_impl(ad_id, 180, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p3.ads[0].published_at, pub_at);
    assert!(p3.ads[0].expires_at > p2.ads[0].expires_at);
}

#[test]
fn test_ad_limit_enforced_and_edit_preserves_timestamps() {
    let base_dir = temp_test_dir("ad_limits_and_edit");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    // Create 10 active ads
    let mut ad_ids = Vec::new();
    for i in 1..=10 {
        let p = upsert_ad_impl(
            offer_ad_input(&format!("Service {}", i), 90, Some(true)),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
        ad_ids.push(p.ads.last().unwrap().id.clone());
    }

    // 11th active ad fails with market.adLimitReached
    let err = upsert_ad_impl(
        offer_ad_input("Service 11", 90, Some(true)),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap_err();
    assert_eq!(err.code, "error.market.adLimitReached");

    // Editing an existing ad succeeds even when 10 ads are active
    let target_id = ad_ids[0].clone();
    let p_before = get_my_market_profile_impl(Some(password.to_string()), &alice_state).unwrap();
    let ad_before = p_before
        .ads
        .iter()
        .find(|a| a.id == target_id)
        .unwrap()
        .clone();

    let edit_input = FrontendAdInput {
        id: Some(target_id.clone()),
        kind: humoco_market::AdKind::Request,
        title: "Service 1 (Updated)".to_string(),
        text: Some("Updated description".to_string()),
        pricing: Some("15 Minuto".to_string()),
        duration_days: 90,
        allow_inquiries: false,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: None,
        event_all_day: false,
        published: Some(true),
    };
    let p_after = upsert_ad_impl(edit_input, Some(password.to_string()), &alice_state).unwrap();
    let ad_after = p_after.ads.iter().find(|a| a.id == target_id).unwrap();

    assert_eq!(ad_after.title, "Service 1 (Updated)");
    assert_eq!(ad_after.kind, humoco_market::AdKind::Request);
    assert_eq!(ad_after.text.as_deref(), Some("Updated description"));
    assert_eq!(ad_after.pricing.as_deref(), Some("15 Minuto"));
    assert!(!ad_after.allow_inquiries);

    // Crucial requirement: edit MUST preserve published_at, renewed_at, and expires_at!
    assert_eq!(ad_after.published_at, ad_before.published_at);
    assert_eq!(ad_after.renewed_at, ad_before.renewed_at);
    assert_eq!(ad_after.expires_at, ad_before.expires_at);

    // Deleting 1 ad allows creating another
    let p_del = delete_ad_impl(target_id, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p_del.ads.len(), 9);

    let p_new = upsert_ad_impl(
        offer_ad_input("New Service", 30, Some(true)),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert_eq!(p_new.ads.len(), 10);
}

#[test]
fn test_event_ad_lifecycle_and_validation() {
    let base_dir = temp_test_dir("event_ad_lifecycle");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);
    let now = chrono::Utc::now().timestamp().max(0) as u64;

    // 1. Event without event_at rejected
    let missing_date = FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Event,
        title: "Stammtisch".to_string(),
        text: None,
        pricing: None,
        duration_days: 90,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: None,
        event_all_day: false,
        published: Some(true),
    };
    assert_frontend_err_msg(
        upsert_ad_impl(missing_date, Some(password.to_string()), &alice_state),
        "Event ads require an event date",
    );

    // 2. Offer with event_at rejected
    let offer_with_date = FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Offer,
        title: "Honig".to_string(),
        text: None,
        pricing: None,
        duration_days: 90,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: Some(now + 86_400),
        event_all_day: false,
        published: Some(true),
    };
    assert_frontend_err_msg(
        upsert_ad_impl(offer_with_date, Some(password.to_string()), &alice_state),
        "Offer/Request ads must not specify an event date",
    );

    // 3. Timed event > 180 days in future rejected
    let too_far = FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Event,
        title: "Far future".to_string(),
        text: None,
        pricing: None,
        duration_days: 90,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: Some(now + 185 * 86_400),
        event_all_day: false,
        published: Some(true),
    };
    assert_frontend_err_msg(
        upsert_ad_impl(too_far, Some(password.to_string()), &alice_state),
        "Event date cannot be more than 180 days in the future",
    );

    // 4. Valid timed event with 500 chars text succeeds and sets expires_at = event_at + 6h
    let ev_ts = now + 7 * 86_400;
    let valid_event = FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Event,
        title: "Minuto Stammtisch".to_string(),
        text: Some("T".repeat(500)),
        pricing: Some("Kostenlos".to_string()),
        duration_days: 90,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: Some(ev_ts),
        event_all_day: false,
        published: Some(true),
    };
    let p1 = upsert_ad_impl(valid_event, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p1.ads.len(), 1);
    assert_eq!(p1.ads[0].kind, humoco_market::AdKind::Event);
    assert_eq!(p1.ads[0].event_at, Some(ev_ts));
    assert!(!p1.ads[0].event_all_day);
    assert_eq!(p1.ads[0].expires_at, ev_ts + 6 * 3_600);

    // 5. Renewing an Event ad by duration is rejected
    let ev_id = p1.ads[0].id.clone();
    assert_frontend_err_msg(
        renew_ad_impl(ev_id.clone(), 90, Some(password.to_string()), &alice_state),
        "Event ads cannot be renewed by duration; edit the event date instead.",
    );

    // 6. Editing the event date updates event_at & expires_at
    let new_ev_ts = now + 14 * 86_400;
    let edit_event = FrontendAdInput {
        id: Some(ev_id.clone()),
        kind: humoco_market::AdKind::Event,
        title: "Minuto Stammtisch (Verschoben)".to_string(),
        text: None,
        pricing: None,
        duration_days: 90,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: Some(new_ev_ts),
        event_all_day: true,
        published: None,
    };
    let p2 = upsert_ad_impl(edit_event, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p2.ads[0].event_at, Some(new_ev_ts));
    assert!(p2.ads[0].event_all_day);
    assert_eq!(p2.ads[0].expires_at, new_ev_ts + 64_800);
}

#[tokio::test]
async fn test_ad_draft_lifecycle_no_card_sign_publish_pause_and_delete() {
    let base_dir = temp_test_dir("ad_draft_lifecycle");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    // 1. Create a draft ad while card is unpublished -> saved locally as draft
    let p1 = upsert_ad_impl(
        offer_ad_input("Draft Bread", 90, None),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert!(!p1.published);
    assert_eq!(p1.ads.len(), 1);
    assert!(!p1.ads[0].published);
    let draft_id = p1.ads[0].id.clone();

    // Publish card -> signed card has ad_count == 0 (draft is excluded!)
    let p_pub_card = set_card_published_impl(true, Some(password.to_string()), &alice_state)
        .await
        .unwrap();
    assert!(p_pub_card.published);
    let stored_after_pub_card = alice_state.cache_own_market_profile().unwrap();
    let v_before_draft_edit = stored_after_pub_card.last_card.as_ref().unwrap().payload.v;
    assert_eq!(
        stored_after_pub_card
            .last_card
            .as_ref()
            .unwrap()
            .payload
            .ad_count,
        0
    );
    assert!(
        get_market_view_impl(Some(password.to_string()), &alice_state)
            .unwrap()
            .board
            .is_empty()
    );

    // 2. Editing a draft must NOT bump card version v
    let mut edit_draft = offer_ad_input("Draft Bread Updated", 90, None);
    edit_draft.id = Some(draft_id.clone());
    let p_edit = upsert_ad_impl(edit_draft, Some(password.to_string()), &alice_state).unwrap();
    assert!(!p_edit.ads[0].published);
    assert_eq!(
        alice_state
            .cache_own_market_profile()
            .unwrap()
            .last_card
            .as_ref()
            .unwrap()
            .payload
            .v,
        v_before_draft_edit
    );

    // 3. Publish draft via set_ad_published_impl(true) -> bumps v & appears on board
    let p_pub_ad = set_ad_published_impl(
        draft_id.clone(),
        true,
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert!(p_pub_ad.ads[0].published);
    let stored_after_pub_ad = alice_state.cache_own_market_profile().unwrap();
    let v_after_pub_ad = stored_after_pub_ad.last_card.as_ref().unwrap().payload.v;
    assert!(v_after_pub_ad > v_before_draft_edit);
    assert_eq!(
        stored_after_pub_ad
            .last_card
            .as_ref()
            .unwrap()
            .payload
            .ad_count,
        1
    );
    let snap2 = get_market_view_impl(Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(snap2.board.len(), 1);
    assert_eq!(snap2.board[0].ad.title, "Draft Bread Updated");

    // 4. Pausing ad via set_ad_published_impl(false) -> bumps v, removes from card & board
    let p_paused = set_ad_published_impl(
        draft_id.clone(),
        false,
        None,
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert!(!p_paused.ads[0].published);
    let stored_after_pause = alice_state.cache_own_market_profile().unwrap();
    let v_after_pause = stored_after_pause.last_card.as_ref().unwrap().payload.v;
    assert!(v_after_pause > v_after_pub_ad);
    assert_eq!(
        stored_after_pause
            .last_card
            .as_ref()
            .unwrap()
            .payload
            .ad_count,
        0
    );
    assert!(
        get_market_view_impl(Some(password.to_string()), &alice_state)
            .unwrap()
            .board
            .is_empty()
    );

    // 5. Deleting a draft does NOT bump card version v
    let p_del = delete_ad_impl(draft_id, Some(password.to_string()), &alice_state).unwrap();
    assert!(p_del.ads.is_empty());
    assert_eq!(
        alice_state
            .cache_own_market_profile()
            .unwrap()
            .last_card
            .as_ref()
            .unwrap()
            .payload
            .v,
        v_after_pause
    );
}

#[test]
fn test_ad_draft_allowed_at_active_limit_but_publish_fails_with_ad_limit_reached() {
    let base_dir = temp_test_dir("ad_draft_limit");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    for i in 1..=10 {
        upsert_ad_impl(
            offer_ad_input(&format!("Pub {}", i), 90, Some(true)),
            Some(password.to_string()),
            &alice_state,
        )
        .unwrap();
    }
    let p_extra = upsert_ad_impl(
        offer_ad_input("Extra Draft 11", 90, Some(false)),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert_eq!(p_extra.ads.len(), 11);
    let extra_id = p_extra.ads.last().unwrap().id.clone();
    assert_frontend_err_code(
        set_ad_published_impl(
            extra_id,
            true,
            None,
            Some(password.to_string()),
            &alice_state,
        ),
        "error.market.adLimitReached",
    );
}

#[test]
fn test_atomic_also_publish_card_publishes_card_and_draft_together() {
    let base_dir = temp_test_dir("atomic_also_publish_card");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    let draft = upsert_ad_impl(
        offer_ad_input("Atomic Draft", 90, Some(false)),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert!(!draft.published);
    assert!(!draft.ads[0].published);

    let res = set_ad_published_impl(
        draft.ads[0].id.clone(),
        true,
        Some(true),
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert!(res.published);
    assert!(res.ads[0].published);
    let stored = alice_state.cache_own_market_profile().unwrap();
    assert_eq!(stored.last_card.unwrap().payload.ad_count, 1);
}

#[tokio::test]
async fn test_unpublish_tombstone_only_signed_on_true_to_false_transition() {
    let base_dir = temp_test_dir("unpublish_tombstone_guard");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    // Never published -> unpublishing leaves last_card == None
    let p0 = set_card_published_impl(false, Some(password.to_string()), &alice_state)
        .await
        .unwrap();
    assert!(!p0.published);
    assert!(alice_state
        .cache_own_market_profile()
        .unwrap()
        .last_card
        .is_none());

    // Publish -> signs card (v1)
    set_card_published_impl(true, Some(password.to_string()), &alice_state)
        .await
        .unwrap();
    let v1 = alice_state
        .cache_own_market_profile()
        .unwrap()
        .last_card
        .unwrap()
        .payload
        .v;

    // Unpublish (true -> false) -> signs tombstone (v2 > v1)
    set_card_published_impl(false, Some(password.to_string()), &alice_state)
        .await
        .unwrap();
    let v2 = alice_state
        .cache_own_market_profile()
        .unwrap()
        .last_card
        .unwrap()
        .payload
        .v;
    assert!(v2 > v1);

    // Repeated unpublish (false -> false) or local save with published: Some(false) must NOT re-sign tombstone
    set_card_published_impl(false, Some(password.to_string()), &alice_state)
        .await
        .unwrap();
    assert_eq!(
        alice_state
            .cache_own_market_profile()
            .unwrap()
            .last_card
            .unwrap()
            .payload
            .v,
        v2
    );
    save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: Vec::new(),
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(false),
        },
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert_eq!(
        alice_state
            .cache_own_market_profile()
            .unwrap()
            .last_card
            .unwrap()
            .payload
            .v,
        v2
    );
}

#[tokio::test]
async fn test_visibility_rules_and_sd_card_disclosures() {
    let base_dir = temp_test_dir("visibility_rules");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    let save_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            first_name: Some("Alice".to_string()),
            last_name: Some("Smith".to_string()),
            organization: Some("Acme Corp".to_string()),
            email: Some("alice@example.com".to_string()),
            ..Default::default()
        },
        visibility: CardFieldVisibility {
            name: FieldAudience::Everyone,
            organization: FieldAudience::Contacts,
            email: FieldAudience::Nobody,
            ..Default::default()
        },
        skills: vec![humoco_market::Skill {
            title: "Engineering".to_string(),
            text: None,
            pricing: None,
            is_remote: false,
        }],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };

    save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();
    set_card_published_impl(true, Some(password.to_string()), &alice_state)
        .await
        .unwrap();

    let mut stored = {
        let mut s = alice_state.service.lock().unwrap();
        crate::state::load_encrypted_json_opt::<StoredOwnMarketProfile>(
            &mut s,
            MARKET_OWN_PROFILE_KEY,
            Some(password),
        )
        .unwrap()
        .unwrap()
    };

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    {
        let mut s = alice_state.service.lock().unwrap();
        rebuild_and_sign_own_card(&mut s, &mut stored, now).unwrap();
    }

    let card = stored.last_card.unwrap();

    // Distance 1 wire: Name (Everyone, max_hops=2) and Organization (Contacts, dist=1) revealed; Email omitted
    let wire_hop1 = humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap();
    let verified_hop1 = humoco_market::verify_sd_card(&wire_hop1).unwrap();
    let profile_hop1 = humoco_market::extract_public_profile(&verified_hop1);

    assert_eq!(profile_hop1.first_name.as_deref(), Some("Alice"));
    assert_eq!(profile_hop1.last_name.as_deref(), Some("Smith"));
    assert_eq!(profile_hop1.organization.as_deref(), Some("Acme Corp"));
    assert_eq!(
        profile_hop1.email, None,
        "Email (Nobody) must never be disclosed"
    );

    // Distance 2 wire: Name revealed (Everyone), but Organization (Contacts only) is redacted!
    let wire_hop2 = humoco_market::render_wire(&card, 2, humoco_market::WireFilter::Full).unwrap();
    let verified_hop2 = humoco_market::verify_sd_card(&wire_hop2).unwrap();
    let profile_hop2 = humoco_market::extract_public_profile(&verified_hop2);

    assert_eq!(profile_hop2.first_name.as_deref(), Some("Alice"));
    assert_eq!(
        profile_hop2.organization, None,
        "Organization (Contacts) must be redacted at hop 2"
    );
    assert_eq!(profile_hop2.email, None);
}

#[test]
fn test_service_offer_derivation_and_needs_preservation() {
    let base_dir = temp_test_dir("service_offer_and_needs");
    let password = "TestPassword123!";
    let mut alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);

    // Precondition: Set Core needs on wallet profile
    let mut p = alice_service
        .with_wallet(|w| w.profile.to_public_profile())
        .unwrap();
    p.needs = Some("Volunteers needed".to_string());
    alice_service
        .update_public_profile(p, Some(password))
        .unwrap();

    let alice_state = create_test_state(alice_service);

    let save_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            first_name: Some("Alice".to_string()),
            ..Default::default()
        },
        visibility: Default::default(),
        skills: vec![
            humoco_market::Skill {
                title: "Carpentry".to_string(),
                text: None,
                pricing: None,
                is_remote: false,
            },
            humoco_market::Skill {
                title: "Plumbing".to_string(),
                text: None,
                pricing: None,
                is_remote: false,
            },
        ],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };

    save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();

    let s = alice_state.service.lock().unwrap();
    let core_profile = s.with_wallet(|w| w.profile.to_public_profile()).unwrap();

    // Rule 7: service_offer is derived from skills
    assert_eq!(
        core_profile.service_offer.as_deref(),
        Some("Carpentry, Plumbing")
    );
    // Rule 7: needs is preserved!
    assert_eq!(core_profile.needs.as_deref(), Some("Volunteers needed"));
}

#[tokio::test]
async fn test_full_marketplace_flow_import_forward_and_board() {
    let base_dir = temp_test_dir("full_marketplace_flow");
    let password = "TestPassword123!";

    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let bob_service = setup_test_app(&base_dir.path().join("bob"), "Bob", BOB_PHRASE, password);
    let charlie_service = setup_test_app(
        &base_dir.path().join("charlie"),
        "Charlie",
        CHARLIE_PHRASE,
        password,
    );

    let alice_state = create_test_state(alice_service);
    let bob_state = create_test_state(bob_service);
    let charlie_state = create_test_state(charlie_service);

    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        let raw = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
        crate::state::canonicalize_did(&raw).unwrap()
    };

    // 1. Alice creates profile, publishes card with 1 ad
    let save_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            first_name: Some("Alice".to_string()),
            last_name: Some("Baker".to_string()),
            ..Default::default()
        },
        visibility: Default::default(),
        skills: vec![humoco_market::Skill {
            title: "Artisan Bread".to_string(),
            text: None,
            pricing: None,
            is_remote: false,
        }],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };
    save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();

    let ad_req = FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Offer,
        title: "Fresh Baguettes".to_string(),
        text: Some("Daily baked".to_string()),
        pricing: Some("3 M".to_string()),
        duration_days: 90,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: None,
        event_all_day: false,
        published: Some(true),
    };
    upsert_ad_impl(ad_req, Some(password.to_string()), &alice_state).unwrap();
    let alice_profile = set_card_published_impl(true, Some(password.to_string()), &alice_state)
        .await
        .unwrap();

    assert!(alice_profile.published);
    let share_uri = alice_profile
        .share_uri
        .as_deref()
        .expect("published card must expose share_uri");
    assert!(share_uri.starts_with("humoco://card?env="));

    // Alice's own active ad must appear on her own board with is_own = true
    let alice_view = get_market_view_impl(Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(alice_view.board.len(), 1);
    assert!(alice_view.board[0].is_own);
    assert_eq!(alice_view.board[0].ad.title, "Fresh Baguettes");

    // 2. Bob imports Alice's full card via file envelope (since QR share_uri is 100% ad-free & image-free)
    let file_env_json = alice_profile.file_wire.expect("file_wire must be present");
    let import_res_bob =
        import_contact_card_impl(file_env_json, Some(password.to_string()), &bob_state).unwrap();
    assert_eq!(import_res_bob.status, "accepted");

    // Bob checks market view and board
    let bob_view = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
    assert_eq!(bob_view.people.len(), 1);
    assert_eq!(bob_view.people[0].did, alice_did);
    assert_eq!(
        bob_view.people[0].profile.first_name.as_deref(),
        Some("Alice")
    );

    assert_eq!(bob_view.board.len(), 1);
    assert_eq!(bob_view.board[0].owner_did, alice_did);
    assert_eq!(bob_view.board[0].ad.title, "Fresh Baguettes");
    assert!(!bob_view.board[0].is_own);

    // 3. Bob forwards Alice's card to Charlie (hops = 2) using exact subaccount recipient DID
    let alice_recipient_did = bob_view.people[0].recipient_did.clone();
    let bob_export = export_forward_card_impl(
        alice_recipient_did.clone(),
        Some(password.to_string()),
        &bob_state,
    )
    .unwrap();
    let charlie_import = import_contact_card_impl(
        bob_export.envelope_json,
        Some(password.to_string()),
        &charlie_state,
    )
    .unwrap();
    assert_eq!(charlie_import.status, "accepted");

    let charlie_view = get_market_view_impl(Some(password.to_string()), &charlie_state).unwrap();
    assert_eq!(charlie_view.people.len(), 1);
    assert_eq!(charlie_view.people[0].did, alice_did);
    assert_eq!(
        charlie_view.people[0].sources[0],
        crate::models::FrontendEntrySource::Card { hops: 2 }
    );
    // Since max_hops = 2, Charlie cannot forward further
    assert!(!charlie_view.people[0].can_forward);
    assert_frontend_err_msg(
        export_forward_card_impl(
            alice_recipient_did.clone(),
            Some(password.to_string()),
            &charlie_state,
        ),
        "Card reached its maximum hop count",
    );

    // 4. Bob deletes Alice's card from his store
    delete_foreign_contact_card_impl(alice_did.clone(), Some(password.to_string()), &bob_state)
        .unwrap();
    let bob_view_empty = get_market_view_impl(Some(password.to_string()), &bob_state).unwrap();
    assert!(bob_view_empty.people.is_empty());
    assert!(bob_view_empty.board.is_empty());
}

fn make_test_vp8_webp(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(30);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(22u32).to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8 ");
    bytes.extend_from_slice(&(10u32).to_le_bytes());
    bytes.push(0x00);
    bytes.extend_from_slice(&[0x00, 0x00]);
    bytes.extend_from_slice(&[0x9d, 0x01, 0x2a]);
    bytes.extend_from_slice(&(width & 0x3fff).to_le_bytes());
    bytes.extend_from_slice(&(height & 0x3fff).to_le_bytes());
    bytes
}

fn make_test_jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0xFF, 0xD8]);
    bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
    bytes.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    bytes.extend_from_slice(&[0xFF, 0xD9]);
    bytes
}

#[test]
fn test_market_profile_avatar_validation_webp_and_jpeg() {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let base_dir = temp_test_dir("market_avatar_validation");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    let initial_profile =
        get_my_market_profile_impl(Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(
        initial_profile.media_max_bytes,
        humoco_market::MAX_AD_MEDIA_BYTES
    );
    assert_eq!(
        initial_profile.media_max_count,
        humoco_market::MAX_ADS_PER_CARD
    );
    assert_eq!(initial_profile.media_used_count, 0);

    let bad_avatar_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            picture_url: Some(format!(
                "data:image/webp;base64,{}",
                STANDARD.encode(make_test_vp8_webp(120, 120))
            )),
            ..Default::default()
        },
        visibility: CardFieldVisibility::default(),
        skills: vec![],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };
    assert_frontend_err_msg(
        save_my_market_profile_impl(bad_avatar_req, Some(password.to_string()), &alice_state),
        "Avatar image dimensions must be 96x96, got 120x120",
    );

    let good_webp_uri = format!(
        "data:image/webp;base64,{}",
        STANDARD.encode(make_test_vp8_webp(96, 96))
    );
    let good_avatar_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            picture_url: Some(good_webp_uri.clone()),
            ..Default::default()
        },
        visibility: CardFieldVisibility::default(),
        skills: vec![],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };
    let saved_profile =
        save_my_market_profile_impl(good_avatar_req, Some(password.to_string()), &alice_state)
            .unwrap();
    assert_eq!(
        saved_profile.profile.picture_url.as_deref(),
        Some(good_webp_uri.as_str())
    );

    let good_jpeg_uri = format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(make_test_jpeg(96, 96))
    );
    let good_jpeg_avatar_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            picture_url: Some(good_jpeg_uri.clone()),
            ..Default::default()
        },
        visibility: CardFieldVisibility::default(),
        skills: vec![],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };
    let saved_jpeg_profile = save_my_market_profile_impl(
        good_jpeg_avatar_req,
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert_eq!(
        saved_jpeg_profile.profile.picture_url.as_deref(),
        Some(good_jpeg_uri.as_str())
    );
}

#[tokio::test]
async fn test_market_ad_media_crud_webp_and_jpeg() {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let base_dir = temp_test_dir("market_ad_media_crud");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);
    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    // 1. Create ad with WebP image
    let ad_data_uri = format!(
        "data:image/webp;base64,{}",
        STANDARD.encode(make_test_vp8_webp(400, 300))
    );
    let mut ad_with_img = offer_ad_input("Handmade Pottery", 90, Some(true));
    ad_with_img.text = Some("High quality clay".to_string());
    ad_with_img.pricing = Some("15 M".to_string());
    ad_with_img.image_webp_data_uri = Some(Some(ad_data_uri.clone()));

    let p_after_ad = upsert_ad_impl(ad_with_img, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p_after_ad.media_used_count, 1);
    let ad_id = p_after_ad.ads[0].id.clone();
    let token = p_after_ad.ads[0]
        .image_token
        .clone()
        .expect("Must have image_token");
    assert!(humoco_market::is_valid_image_token(&token));
    assert_eq!(
        get_ad_image_impl(
            alice_did.clone(),
            token.clone(),
            Some(password.to_string()),
            &alice_state
        )
        .await
        .unwrap(),
        Some(ad_data_uri)
    );

    // 2. Edit ad without changing image (image_webp_data_uri: None) -> image_token preserved
    let mut edit_no_img_change = offer_ad_input("Handmade Pottery (Refined)", 90, Some(true));
    edit_no_img_change.id = Some(ad_id.clone());
    edit_no_img_change.text = Some("High quality clay".to_string());
    edit_no_img_change.pricing = Some("15 M".to_string());
    let p_edit =
        upsert_ad_impl(edit_no_img_change, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p_edit.ads[0].image_token, Some(token));
    assert_eq!(p_edit.media_used_count, 1);

    // 3. Edit ad removing image (JSON null -> Some(None)) -> image_token becomes None
    let edit_remove_img: FrontendAdInput = serde_json::from_value(serde_json::json!({
        "id": ad_id,
        "kind": "offer",
        "title": "Handmade Pottery (No Photo)",
        "text": "High quality clay",
        "pricing": "15 M",
        "durationDays": 90,
        "allowInquiries": true,
        "isRemote": false,
        "imageWebpDataUri": null
    }))
    .unwrap();
    let p_removed =
        upsert_ad_impl(edit_remove_img, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p_removed.ads[0].image_token, None);
    assert_eq!(p_removed.media_used_count, 0);

    // 4. Create ad with JPEG image (WebKitGTK fallback)
    let jpeg_ad_data_uri = format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(make_test_jpeg(400, 300))
    );
    let mut ad_with_jpeg = offer_ad_input("Vintage Camera", 30, Some(true));
    ad_with_jpeg.text = Some("Fully functional".to_string());
    ad_with_jpeg.pricing = Some("50 M".to_string());
    ad_with_jpeg.is_remote = true;
    ad_with_jpeg.image_webp_data_uri = Some(Some(jpeg_ad_data_uri.clone()));

    let p_after_jpeg_ad =
        upsert_ad_impl(ad_with_jpeg, Some(password.to_string()), &alice_state).unwrap();
    let jpeg_token = p_after_jpeg_ad.ads[1]
        .image_token
        .clone()
        .expect("Must have jpeg image_token");
    assert_eq!(
        get_ad_image_impl(
            alice_did,
            jpeg_token,
            Some(password.to_string()),
            &alice_state
        )
        .await
        .unwrap(),
        Some(jpeg_ad_data_uri)
    );
}

#[tokio::test]
async fn test_market_ad_image_invalid_token_and_negative_cache() {
    let base_dir = temp_test_dir("market_ad_neg_cache");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);
    let alice_did = {
        let s = alice_state.service.lock().unwrap();
        s.with_wallet(|w| w.get_user_id().to_string()).unwrap()
    };

    assert_eq!(
        get_ad_image_impl(
            alice_did.clone(),
            "invalid_token".to_string(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap(),
        None
    );

    alice_state.set_cached_security_policy(Some(crate::settings::SecurityPolicy {
        market_selected_syncs: vec![FrontendSyncDescriptor {
            write_url: "http://127.0.0.1:9999".to_string(),
            read_url: "http://127.0.0.1:9999".to_string(),
            slot_id: None,
            drop_pub: "0".repeat(64),
            last_seen_at: None,
            last_error_at: None,
            fail_count: 0,
            status: "active".to_string(),
            last_probed_at: None,
            ..Default::default()
        }],
        ..Default::default()
    }));

    let unknown_token = "0123456789abcdef0123456789abcdef".to_string();
    assert_eq!(
        get_ad_image_impl(
            alice_did.clone(),
            unknown_token.clone(),
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap(),
        None
    );

    let blob_id = humoco_market::derive_media_blob_id(&unknown_token).unwrap();
    assert!(alice_state
        .profile_caches
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .media_negative_cache
        .contains_key(&(blob_id, "http://127.0.0.1:9999".to_string())));

    assert_eq!(
        get_ad_image_impl(
            alice_did,
            unknown_token,
            Some(password.to_string()),
            &alice_state,
        )
        .await
        .unwrap(),
        None
    );
}

#[tokio::test]
async fn test_rebuild_and_sign_own_card_with_empty_or_none_name() {
    let temp_dir = temp_test_dir("empty_name_test");
    let password = "test-password-123";
    let mut alice_service = crate::test_support::setup_test_app(
        temp_dir.path(),
        "Initial Name",
        crate::test_support::ALICE_PHRASE,
        password,
    );

    let mut public_profile = alice_service
        .with_wallet(|w| w.profile.to_public_profile())
        .unwrap();
    public_profile.first_name = Some("".to_string());
    public_profile.last_name = Some("   ".to_string());
    alice_service
        .update_public_profile(public_profile, Some(password))
        .unwrap();

    let alice_state = crate::test_support::create_test_state(alice_service);

    let res = set_card_published_impl(true, Some(password.to_string()), &alice_state).await;
    assert!(
        res.is_ok(),
        "Expected success publishing card with empty name, got: {:?}",
        res.err()
    );
    let prof = res.unwrap();
    assert!(prof.published);
}

#[test]
fn test_save_profile_p8_privacy_guardrails_and_inquiry_sync() {
    let base_dir = temp_test_dir("p8_guardrails");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    // Create an ad first with allow_inquiries = true
    let ad_input = FrontendAdInput {
        id: None,
        kind: humoco_market::AdKind::Offer,
        title: "Neighborhood Bike Help".to_string(),
        text: None,
        pricing: None,
        duration_days: 90,
        allow_inquiries: true,
        is_remote: false,
        image_webp_data_uri: None,
        event_at: None,
        event_all_day: false,
        published: Some(true),
    };
    let p_with_ad = upsert_ad_impl(ad_input, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(p_with_ad.ads.len(), 1);
    assert!(p_with_ad.ads[0].allow_inquiries);

    // Save profile attempting street=Everyone, public fine coords, and allow_inquiries=false
    let save_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            first_name: Some("Alice".to_string()),
            coordinates: Some("48.1351, 11.5820".to_string()),
            ..Default::default()
        },
        visibility: CardFieldVisibility {
            street: FieldAudience::Everyone,
            coordinates: FieldAudience::Everyone,
            ..Default::default()
        },
        skills: vec![],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: false,
        published: None,
    };

    let saved =
        save_my_market_profile_impl(save_req, Some(password.to_string()), &alice_state).unwrap();
    assert_eq!(
        saved.visibility.street,
        FieldAudience::Contacts,
        "street must never be public"
    );
    assert_eq!(
        saved.profile.coordinates.as_deref(),
        Some("48.14, 11.58"),
        "public coordinates must be coarsened to 2 decimal places"
    );
    assert!(!saved.allow_inquiries);
    assert!(
        !saved.ads[0].allow_inquiries,
        "profile allow_inquiries must propagate to existing ads"
    );

    // When coordinates visibility is contacts-only, finer coordinates are preserved
    let save_contacts_coords = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            first_name: Some("Alice".to_string()),
            coordinates: Some("48.1351, 11.5820".to_string()),
            ..Default::default()
        },
        visibility: CardFieldVisibility {
            street: FieldAudience::Contacts,
            coordinates: FieldAudience::Contacts,
            ..Default::default()
        },
        skills: vec![],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: None,
    };

    let saved_contacts = save_my_market_profile_impl(
        save_contacts_coords,
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    assert_eq!(
        saved_contacts.profile.coordinates.as_deref(),
        Some("48.1351, 11.5820")
    );
    assert!(saved_contacts.ads[0].allow_inquiries);
}

#[test]
fn test_import_contact_card_uri_with_surrounding_chat_text() {
    let base_dir = temp_test_dir("card_uri_chat_text");
    let password = "TestPassword123!";
    let alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    let alice_state = create_test_state(alice_service);

    let env_json = serde_json::to_string(&humoco_market::CardEnvelope {
        card_wire: "not-a-real-jws".to_string(),
        hops: 1,
    })
    .unwrap();
    let b64 = human_money_core::crypto::encode_base64(env_json.as_bytes());
    let chat_msg = format!(
        "Hallo Alice,\nhier ist meine Visitenkarte:\n\"humoco://card?env={}\"\nViele Grüße, Bob!",
        b64
    );

    let res = import_contact_card_impl(chat_msg, Some(password.to_string()), &alice_state)
        .expect("URI extraction and CardEnvelope JSON parsing should succeed");
    assert!(
        res.status.starts_with("invalid:"),
        "expected JWS validation error status after successful URI/JSON parse, got: {}",
        res.status
    );
}

#[tokio::test]
async fn test_k1_k2_market_cache_error_propagation_and_access_mode_guards() {
    let base_dir = temp_test_dir("market_k1_k2_guards");
    let password = "MasterPassword123!";
    let daily_pin = "445566";
    let mut alice_service =
        setup_test_app(&base_dir.path().join("alice"), "Alice", ALICE_PHRASE, password);
    alice_service
        .enable_secondary_auth(
            password,
            daily_pin,
            human_money_core::SecondaryScope::DailyPay,
        )
        .unwrap();
    let folder = alice_service.list_profiles().unwrap()[0]
        .folder_name
        .clone();
    let alice_state = create_test_state(alice_service);

    // Populate market profile with a skill and published card
    save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: vec![humoco_market::Skill {
                title: "Baking".to_string(),
                text: None,
                pricing: None,
                is_remote: false,
            }],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(true),
        },
        Some(password.to_string()),
        &alice_state,
    )
    .unwrap();
    upsert_ad_impl(
        FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Sourdough".to_string(),
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
        &alice_state,
    )
    .unwrap();

    // K1: Without login pre-cache and with timeout=0 (password: None), read MUST return SessionNotActive instead of empty default
    alice_state.clear_all_caches();
    assert_frontend_err_code(
        get_my_market_profile_impl(None, &alice_state),
        "error.auth.sessionNotActive",
    );

    // K1: Re-login with timeout = 0 populates the own_market_profile cache so read succeeds
    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .initialize_profile_session(&mut s, password)
            .unwrap();
    }
    let cached_profile = get_my_market_profile_impl(None, &alice_state).unwrap();
    assert_eq!(cached_profile.skills.len(), 1);
    assert!(cached_profile.published);

    // K2: Log in with DailyPay PIN + active session cache (timeout > 0);
    // Full-only profile mutations fail closed while DailyPay ad mutations succeed
    alice_state.clear_all_caches();
    {
        let mut s = alice_state.service.lock().unwrap();
        s.logout();
        let granted = s
            .login_with_secondary(&folder, daily_pin, true, "device-market-test".into())
            .unwrap();
        assert_eq!(granted, human_money_core::AccessMode::DailyPay);
        s.unlock_secondary_session(daily_pin, 300).unwrap();
    }

    let err_profile = save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Bob".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(true),
        },
        None,
        &alice_state,
    )
    .unwrap_err();
    assert_eq!(err_profile.code, "error.auth.insufficientAccessMode");

    assert!(upsert_ad_impl(
        FrontendAdInput {
            id: None,
            kind: humoco_market::AdKind::Offer,
            title: "Authorized DailyPay Ad".to_string(),
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
        None,
        &alice_state,
    )
    .is_ok());
}

#[test]
fn h6_save_my_market_profile_rejects_oversized_card_field_without_mutating_core_profile() {
    let dir = temp_test_dir("h6_atomic_profile_save");
    let pw = "pw-h6-test";
    let state = crate::test_support::create_test_state(crate::test_support::setup_test_app(
        &dir.path().join("alice"),
        "Alice Initial",
        crate::test_support::ALICE_PHRASE,
        pw,
    ));

    let initial_ok = save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                organization: Some("Clean Org".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(true),
        },
        Some(pw.to_string()),
        &state,
    )
    .expect("Initial valid profile save should succeed");
    assert_eq!(initial_ok.profile.first_name.as_deref(), Some("Alice"));
    assert_eq!(
        initial_ok.profile.organization.as_deref(),
        Some("Clean Org")
    );

    // Attempt an update whose first_name exceeds sd_card's 80-char limit while published = true.
    // Validation/signing MUST fail BEFORE mutating Core's UserProfile on disk or in RAM.
    let too_long_name = "X".repeat(120);
    assert_frontend_err_msg(
        save_my_market_profile_impl(
            FrontendSaveMyMarketProfileRequest {
                profile: FrontendUserProfile {
                    first_name: Some(too_long_name),
                    organization: Some("Partially Mutated Org".to_string()),
                    ..Default::default()
                },
                visibility: CardFieldVisibility::default(),
                skills: vec![],
                max_hops: 2,
                validity_years: 2,
                allow_inquiries: true,
                published: Some(true),
            },
            Some(pw.to_string()),
            &state,
        ),
        "first_name exceeds 80 characters",
    );

    let after = get_my_market_profile_impl(Some(pw.to_string()), &state).unwrap();
    assert_eq!(
        after.profile.first_name.as_deref(),
        Some("Alice"),
        "Core profile first_name must stay untouched when card signing fails"
    );
    assert_eq!(
        after.profile.organization.as_deref(),
        Some("Clean Org"),
        "Core profile organization must stay untouched when card signing fails"
    );
}

#[test]
fn h3_h4_negative_cache_clears_on_logout_and_foreign_media_skips_wallet_seal() {
    let dir = temp_test_dir("h3_h4_cache_and_seal");
    let pw = "pw-h3h4-test";
    let state = crate::test_support::create_test_state(crate::test_support::setup_test_app(
        &dir.path().join("alice"),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    ));

    let session = {
        let s = state.service.lock().unwrap();
        state.session_token_for_service(&s).unwrap()
    };

    // H3: Negative cache entry lives inside ActiveProfileCaches and is evicted on clear_all_caches()
    let blob_id = "ab".repeat(32);
    let url = "http://127.0.0.1:9999/sync.php".to_string();
    state.record_media_negative_cache(
        &session,
        blob_id.clone(),
        url.clone(),
        1_760_000_000,
        86_400,
    );
    assert!(state.is_media_negatively_cached(&session, &blob_id, &url, 1_760_000_010));

    state.clear_all_caches();
    assert!(state.profile_caches.lock().unwrap().is_none());

    // H4: Caching foreign ad media in .local_* sidecar MUST NOT advance WalletSeal
    let seal_before = state.service.lock().unwrap().get_current_seal().unwrap();
    let cache_key = crate::state::cached_media_storage_key(&blob_id);
    {
        let mut s = state.service.lock().unwrap();
        crate::state::save_local_sidecar_bytes(&mut s, &cache_key, b"webp-payload", Some(pw))
            .unwrap();
    }
    let seal_after = state.service.lock().unwrap().get_current_seal().unwrap();
    assert_eq!(seal_before, seal_after);
}

#[test]
fn h5_resolve_card_recipient_preserves_subaccount_and_rejects_tamper() {
    use human_money_core::app_service::AppService;
    use human_money_core::services::mnemonic::MnemonicLanguage;

    let dir = temp_test_dir("h5_resolve_subaccount");
    let pw = "pw-h5-test";
    let mut service = AppService::new(dir.path()).unwrap();
    let mnemonic = AppService::generate_mnemonic(12, MnemonicLanguage::English).unwrap();
    service
        .create_profile(
            "Alice Shop",
            &mnemonic,
            None,
            Some("shop"),
            pw,
            MnemonicLanguage::English,
            "dev-h5".into(),
            true,
        )
        .unwrap();
    let (expected_uid, alice_pk) = service
        .with_wallet_and_identity(|w, id| {
            (
                w.get_user_id().to_string(),
                id.signing_key.verifying_key(),
            )
        })
        .unwrap();
    assert!(expected_uid.starts_with("shop:"));

    let state = crate::test_support::create_test_state(service);
    let profile = save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice".to_string()),
                last_name: Some("Shop".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
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
    let share_uri = profile
        .share_uri
        .expect("published card must have share_uri");

    let resolved = resolve_card_recipient_impl(share_uri.clone(), &state).unwrap();
    assert_eq!(resolved.recipient_id, expected_uid);
    assert_eq!(resolved.display_name.as_deref(), Some("Alice Shop"));

    // Tampering payload.did fails Identity check early
    let b64_part = share_uri.split("env=").nth(1).unwrap();
    let env_bytes = human_money_core::crypto::decode_base64(b64_part).unwrap();
    let env: humoco_market::CardEnvelope = serde_json::from_slice(&env_bytes).unwrap();
    let parts: Vec<String> = env.card_wire.split('.').map(str::to_string).collect();
    let payload_bytes = human_money_core::crypto::decode_base64(&parts[1]).unwrap();
    let payload_val: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();

    let mut did_tampered = payload_val.clone();
    did_tampered["did"] = serde_json::json!(format!("evil:999@{}", resolved.did));
    let mut did_parts = parts.clone();
    did_parts[1] = human_money_core::crypto::encode_base64(&serde_json::to_vec(&did_tampered).unwrap());
    let mut env_bad_did = env.clone();
    env_bad_did.card_wire = did_parts.join(".");
    assert_frontend_err_msg(
        resolve_card_recipient_impl(serde_json::to_string(&env_bad_did).unwrap(), &state),
        "Invalid contact card: Identity error:",
    );

    // Tampering payload.recipient_did with another valid canonical subaccount for the same key passes
    // DID checks and MUST fail at JWS signature verification!
    let evil_recipient_did = human_money_core::crypto::create_user_id(&alice_pk, Some("evil")).unwrap();
    let mut sig_tampered = payload_val;
    sig_tampered["recipient_did"] = serde_json::json!(evil_recipient_did);
    let mut sig_parts = parts;
    sig_parts[1] = human_money_core::crypto::encode_base64(&serde_json::to_vec(&sig_tampered).unwrap());
    let mut env_bad_sig = env;
    env_bad_sig.card_wire = sig_parts.join(".");
    assert_frontend_err_msg(
        resolve_card_recipient_impl(serde_json::to_string(&env_bad_sig).unwrap(), &state),
        "Invalid contact card: Cryptographic error: Invalid JWS signature",
    );
}

#[test]
fn h10_import_contact_card_updates_contacts_atomically_and_rolls_back_cache_on_write_fault() {
    use human_money_core::app_service::AppService;

    let dir_alice = temp_test_dir("h10_alice");
    let dir_bob = temp_test_dir("h10_bob");
    let pw = "StrongPw-H10-2026!";

    let alice_svc = crate::test_support::setup_test_app(
        dir_alice.path(),
        "Alice",
        crate::test_support::ALICE_PHRASE,
        pw,
    );
    let alice_state = crate::test_support::create_test_state(alice_svc);
    let alice_did = alice_state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let alice_prof = save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                first_name: Some("Alice Updated".to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
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
    let share_uri = alice_prof.share_uri.unwrap();

    let bob_svc = AppService::new(dir_bob.path()).unwrap();
    let bob_state = crate::test_support::create_test_state(crate::test_support::setup_test_app(
        &dir_bob.path().join("w"),
        "Bob",
        crate::test_support::BOB_PHRASE,
        pw,
    ));
    drop(bob_svc);
    {
        let mut s = bob_state.service.lock().unwrap();
        bob_state
            .save_contact(
                &mut s,
                crate::models::FrontendContact {
                    did: alice_did.clone(),
                    profile: FrontendUserProfile::default(),
                    tags: vec![],
                    added_at: chrono::Utc::now().to_rfc3339(),
                    notes: None,
                    trust_level: Some(1),
                    desired_hops: None,
                    has_peer_list: false,
                    peer_list_updated_at: None,
                    syncs: vec![],
                },
                Some(pw),
            )
            .unwrap();
        // Warm cache
        assert_eq!(
            bob_state.load_contacts(&mut s, Some(pw)).unwrap()[0]
                .profile
                .first_name,
            None
        );
    }

    // Inject write fault on batch save -> card + contact update must abort together AND cache stay untouched!
    {
        let _fault_guard = crate::test_support::WRITE_FAULT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        human_money_core::storage::file_storage::inject_write_fault_once("seal.enc");
        assert_frontend_err_msg(
            import_contact_card_impl(share_uri.clone(), Some(pw.to_string()), &bob_state),
            "Injected write fault for seal.enc (test-utils)",
        );
    }
    {
        let mut s = bob_state.service.lock().unwrap();
        assert_eq!(
            bob_state.load_contacts(&mut s, Some(pw)).unwrap()[0]
                .profile
                .first_name,
            None,
            "H10: Contacts cache must NOT reflect unpersisted card update after batch failure"
        );
        assert!(
            crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
                &mut s,
                crate::state::MARKET_CARD_STORE_KEY,
                Some(pw),
            )
            .unwrap()
            .is_none(),
            "H10: CardStore must roll back when batch commit fails"
        );
    }

    // Clean retry -> both CardStore & Contact cache commit atomically!
    let ok = import_contact_card_impl(share_uri, Some(pw.to_string()), &bob_state).unwrap();
    assert_eq!(ok.status, "accepted");
    {
        let mut s = bob_state.service.lock().unwrap();
        assert_eq!(
            bob_state.load_contacts(&mut s, Some(pw)).unwrap()[0]
                .profile
                .first_name
                .as_deref(),
            Some("Alice Updated")
        );
        let persisted = crate::state::load_encrypted_json_opt::<humoco_market::CardStore>(
            &mut s,
            crate::state::MARKET_CARD_STORE_KEY,
            Some(pw),
        )
        .unwrap()
        .expect("CardStore must be persisted after clean retry");
        assert!(persisted.cards.contains_key(&alice_did));
    }
}

#[test]
fn test_f2_f3_strict_card_envelope_and_no_subaccount_sibling_fallback() {
    let dir = temp_test_dir("f2_f3_strict_adapters");
    let pw = "StrongPassword123!";
    let alice_svc = setup_test_app(&dir.path().join("alice"), "Alice", ALICE_PHRASE, pw);
    let state = create_test_state(alice_svc);

    let sample_env = serde_json::json!({
        "card_wire": "hdr.payload.sig",
        "hops": 1
    });
    let b64 = human_money_core::crypto::encode_base64(sample_env.to_string().as_bytes());
    assert!(parse_card_envelope_payload(&format!("humoco://card?env={}", b64)).is_ok());
    assert!(parse_card_envelope_payload(&format!("HUMOCO://CARD?foo=bar&env={}", b64)).is_ok());

    // F2: arbitrary URL prefixes must NOT be accepted just for containing ?env=
    assert_frontend_err_msg(
        parse_card_envelope_payload(&format!("https://evil.example/path?env={}", b64)),
        "Payload is not a valid CardEnvelope JSON",
    );
    assert_frontend_err_msg(
        parse_card_envelope_payload(&format!("humoco://pay?env={}", b64)),
        "Payload is not a valid CardEnvelope JSON",
    );

    // F2: unknown JSON fields on CardEnvelope must be rejected (#[serde(deny_unknown_fields)])
    let unknown_json = r#"{"card_wire":"hdr.payload.sig","hops":1,"extra":true}"#;
    assert_frontend_err_msg(
        parse_card_envelope_payload(unknown_json),
        "unknown field `extra`",
    );

    // F3: export_forward_card_impl for missing subaccount does not fall back to sibling card
    let sib_sk = SigningKey::from_bytes(&[77u8; 32]);
    let sib_vk = sib_sk.verifying_key();
    let sib_a = human_money_core::crypto::create_user_id(&sib_vk, Some("shop")).unwrap();
    let sib_missing = human_money_core::crypto::create_user_id(&sib_vk, Some("work")).unwrap();
    let card = humoco_market::sign_sd_card(
        humoco_market::CardPayloadInput {
            did: sib_a.clone(),
            v: 1,
            valid_until: 2_000_000_000,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: true,
            drops: vec![humoco_market::DropDescriptor {
                write_url: "https://sync.example/sync.php".into(),
                read_url: "https://sync.example/d".into(),
                slot_id: None,
                drop_pub: "11".repeat(32),
            }],
        },
        Vec::new(),
        &sib_sk,
    )
    .unwrap();
    let wire = humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap();
    let mut store = humoco_market::CardStore::default();
    store.cards.insert(
        sib_a.clone(),
        humoco_market::StoredCard {
            card,
            card_wire: wire,
            hops: 1,
            received_at: 1_760_000_000,
        },
    );
    {
        let mut s = state.lock_service().unwrap();
        crate::state::save_encrypted_json(&mut s, MARKET_CARD_STORE_KEY, &store, Some(pw)).unwrap();
    }
    assert!(export_forward_card_impl(sib_a, Some(pw.into()), &state).is_ok());
    assert_frontend_err_msg(
        export_forward_card_impl(sib_missing, Some(pw.into()), &state),
        "Card not found in store",
    );
}

#[test]
fn f8_parse_card_envelope_payload_accepts_quotes_linebreaks_and_standard_base64() {
    use base64::Engine;
    let (_, sib_sk) = human_money_core::services::crypto::derive_ed25519_keypair(
        ALICE_PHRASE,
        None,
        human_money_core::MnemonicLanguage::English,
    )
    .unwrap();
    let did = human_money_core::crypto::create_user_id(&sib_sk.verifying_key(), None).unwrap();
    let card = humoco_market::sign_sd_card(
        humoco_market::CardPayloadInput {
            did,
            v: 1,
            valid_until: 2_000_000_000,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: true,
            drops: vec![humoco_market::DropDescriptor {
                write_url: "https://sync.example/sync.php".into(),
                read_url: "https://sync.example/d".into(),
                slot_id: None,
                drop_pub: "11".repeat(32),
            }],
        },
        Vec::new(),
        &sib_sk,
    )
    .unwrap();
    let card_wire = humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap();
    let env = humoco_market::CardEnvelope { card_wire, hops: 0 };
    let json = serde_json::to_string(&env).unwrap();
    let std_b64 = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
    let split_at = std_b64.len() / 2;
    let wrapped = format!(
        "“humoco://card?env={}\n{}#frag”",
        &std_b64[..split_at],
        &std_b64[split_at..]
    );
    let parsed = parse_card_envelope_payload(&wrapped).expect("wrapped standard base64 URI");
    assert_eq!(parsed.card_wire, env.card_wire);
    assert_frontend_err_msg(
        parse_card_envelope_payload("humoco://card?other=1"),
        "Card share URI is missing env query parameter",
    );
}

#[test]
fn s11_f2_build_card_drops_filters_revoked_and_invalid_keys() {
    use crate::test_support::test_ed25519_pub_hex;

    let pub1 = test_ed25519_pub_hex(0x11);
    let pub2 = test_ed25519_pub_hex(0x22);
    let pub3 = test_ed25519_pub_hex(0x33);
    let pub4 = test_ed25519_pub_hex(0x44);
    let pub5 = test_ed25519_pub_hex(0x55);
    let pub6 = test_ed25519_pub_hex(0x66);

    let revoked_selected = FrontendSyncDescriptor {
        write_url: "https://revoked.example/sync.php".to_string(),
        read_url: "https://revoked.example/data".to_string(),
        drop_pub: pub1,
        status: "revoked".to_string(),
        ..Default::default()
    };
    let active_selected = FrontendSyncDescriptor {
        write_url: "https://s1.example/sync.php/".to_string(),
        read_url: "https://s1.example/data".to_string(),
        drop_pub: pub2.clone(),
        status: "active".to_string(),
        ..Default::default()
    };
    let bad_drop_pub_selected = FrontendSyncDescriptor {
        write_url: "https://weak.example/sync.php".to_string(),
        read_url: "https://weak.example/data".to_string(),
        drop_pub: "00".repeat(32),
        status: "active".to_string(),
        ..Default::default()
    };
    let public_syncs = vec![
        crate::settings::StoredPublicSync {
            write_url: "https://s1.example/sync.php".to_string(),
            read_url: "https://s1.example/data".to_string(),
            drop_pub: pub2,
            access_pubkey: pub3,
            enabled: true,
            status: "active".to_string(),
            ..Default::default()
        },
        crate::settings::StoredPublicSync {
            write_url: "https://badpk.example/sync.php".to_string(),
            read_url: "https://badpk.example/data".to_string(),
            drop_pub: pub4,
            access_pubkey: "invalid-pubkey".to_string(),
            enabled: true,
            status: "active".to_string(),
            ..Default::default()
        },
        crate::settings::StoredPublicSync {
            write_url: "https://goodpub.example/sync.php".to_string(),
            read_url: "https://goodpub.example/data".to_string(),
            drop_pub: pub5,
            access_pubkey: pub6,
            enabled: true,
            status: "active".to_string(),
            ..Default::default()
        },
    ];
    let drops = build_card_drops(
        &[revoked_selected, bad_drop_pub_selected, active_selected],
        &public_syncs,
        Some("laden"),
    );
    assert_eq!(drops.len(), 2);
    assert_eq!(drops[0].write_url, "https://s1.example/sync.php/");
    assert_eq!(drops[1].write_url, "https://goodpub.example/sync.php");
    assert!(drops[1].slot_id.as_deref().unwrap().ends_with("_laden"));
}

#[test]
fn s11_f4_profile_and_ad_input_validation_rejects_bad_urls_skills_and_oversized_media() {
    let dir = temp_test_dir("s11_f4_input_validation");
    let pw = "pw-s11-f4";
    let service = crate::test_support::setup_test_app(
        &dir.path().join("alice"),
        "Alice S11",
        ALICE_PHRASE,
        pw,
    );
    let state = crate::test_support::create_test_state(service);

    for (bad_pic, expected_err) in [
        (
            "data:image/svg+xml;base64,PHN2Zy8+",
            "Avatar data URI must start with 'data:image/webp;base64,', 'data:image/jpeg;base64,', or 'data:image/png;base64,'",
        ),
        (
            "javascript:alert(1)",
            "Profile picture URL must use https:// without userinfo",
        ),
        (
            "http://insecure.example/avatar.png",
            "Profile picture URL must use https:// without userinfo",
        ),
        (
            "https://user:pass@example.com/avatar.png",
            "Profile picture URL must use https:// without userinfo",
        ),
    ] {
        let req = FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile {
                picture_url: Some(bad_pic.to_string()),
                ..Default::default()
            },
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(false),
        };
        assert_frontend_err_msg(
            save_my_market_profile_impl(req, Some(pw.to_string()), &state),
            expected_err,
        );
    }

    let too_many_skills = (0..=humoco_market::MAX_SKILLS_PER_CARD)
        .map(|i| humoco_market::Skill {
            title: format!("Skill {}", i),
            text: None,
            pricing: None,
            is_remote: false,
        })
        .collect();
    let skills_err_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile::default(),
        visibility: CardFieldVisibility::default(),
        skills: too_many_skills,
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: Some(false),
    };
    assert_frontend_err_msg(
        save_my_market_profile_impl(skills_err_req, Some(pw.to_string()), &state),
        "Skill count 9 exceeds maximum 8",
    );

    let valid_https_req = FrontendSaveMyMarketProfileRequest {
        profile: FrontendUserProfile {
            picture_url: Some("  https://example.com/avatar.png  ".to_string()),
            ..Default::default()
        },
        visibility: CardFieldVisibility::default(),
        skills: vec![humoco_market::Skill {
            title: "Gardening".to_string(),
            text: None,
            pricing: None,
            is_remote: false,
        }],
        max_hops: 2,
        validity_years: 2,
        allow_inquiries: true,
        published: Some(false),
    };
    let saved = save_my_market_profile_impl(valid_https_req, Some(pw.to_string()), &state).unwrap();
    assert_eq!(
        saved.profile.picture_url.as_deref(),
        Some("https://example.com/avatar.png")
    );

    let mut oversized_ad = offer_ad_input("Too big", 30, Some(true));
    oversized_ad.image_webp_data_uri = Some(Some(format!(
        "data:image/webp;base64,{}",
        "A".repeat(140_005)
    )));
    assert_frontend_err_msg(
        upsert_ad_impl(oversized_ad, Some(pw.to_string()), &state),
        "Ad image data URI exceeds maximum length or contains control characters",
    );

    let mut unknown_id_ad = offer_ad_input("Unknown ID", 30, Some(true));
    unknown_id_ad.id = Some("0011223344556677".to_string());
    assert_frontend_err_msg(
        upsert_ad_impl(unknown_id_ad, Some(pw.to_string()), &state),
        "Ad not found",
    );

    assert_frontend_err_msg(
        parse_card_envelope_payload(&"x".repeat(64 * 1024 + 1)),
        "Card envelope payload exceeds 64 KiB limit",
    );
}

#[tokio::test]
async fn s11_f4_delete_ad_missing_and_write_fault_preserves_media() {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let dir = temp_test_dir("s11_f4_delete_fault");
    let pw = "pw-s11-f4";
    let service = crate::test_support::setup_test_app(
        &dir.path().join("alice"),
        "Alice S11",
        ALICE_PHRASE,
        pw,
    );
    let state = crate::test_support::create_test_state(service);

    let valid_webp = format!(
        "data:image/webp;base64,{}",
        STANDARD.encode(make_test_vp8_webp(64, 64))
    );
    let mut ad_input = offer_ad_input("Bike", 30, Some(true));
    ad_input.image_webp_data_uri = Some(Some(valid_webp.clone()));
    let after_ad = upsert_ad_impl(ad_input, Some(pw.to_string()), &state).unwrap();
    let ad_id = after_ad.ads[0].id.clone();
    let ad_token = after_ad.ads[0].image_token.clone().unwrap();

    assert_frontend_err_msg(
        delete_ad_impl("nonexistent".to_string(), Some(pw.to_string()), &state),
        "Ad not found",
    );

    {
        let _fault_guard = crate::test_support::WRITE_FAULT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        human_money_core::storage::file_storage::inject_write_fault_once("seal.enc");
        assert_frontend_err_msg(
            delete_ad_impl(ad_id, Some(pw.to_string()), &state),
            "Injected write fault for seal.enc (test-utils)",
        );
    }
    let media_survived = get_ad_image_impl(String::new(), ad_token, Some(pw.to_string()), &state)
        .await
        .unwrap();
    assert_eq!(media_survived, Some(valid_webp));
}

struct SeededS11F5Env {
    _dir: TempDirGuard,
    wallet_dir: PathBuf,
    folder_name: String,
    bob_did: String,
    charlie_did: String,
    own_token: String,
    bob_token: String,
    valid_webp: Vec<u8>,
}

fn seed_s11_f5_wallet(name: &str, pw: &str, cashier_pin: &str) -> SeededS11F5Env {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let dir = temp_test_dir(name);
    let wallet_dir = dir.path().join("alice");
    let mut svc = crate::test_support::setup_test_app(&wallet_dir, "Alice F5", ALICE_PHRASE, pw);
    let folder_name = svc.list_profiles().unwrap()[0].folder_name.clone();
    svc.enable_secondary_auth(
        pw,
        cashier_pin,
        human_money_core::SecondaryScope::ReceiveOnly,
    )
    .unwrap();
    drop(svc);

    let svc2 = crate::test_support::setup_test_app(&dir.path().join("bob"), "Bob", BOB_PHRASE, pw);
    let (bob_did, bob_sk) = svc2
        .with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.signing_key.clone()))
        .unwrap();
    drop(svc2);

    let charlie_svc = crate::test_support::setup_test_app(
        &dir.path().join("charlie"),
        "Charlie",
        crate::test_support::CHARLIE_PHRASE,
        pw,
    );
    let charlie_did = charlie_svc
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    drop(charlie_svc);

    let mut master_svc = human_money_core::app_service::AppService::new(&wallet_dir).unwrap();
    master_svc
        .login(&folder_name, pw, false, "device-drops-test".into())
        .unwrap();
    let master_state = crate::test_support::create_test_state(master_svc);

    let valid_webp = make_test_vp8_webp(64, 64);
    let mut own_ad = offer_ad_input("Own", 30, Some(true));
    own_ad.image_webp_data_uri = Some(Some(format!(
        "data:image/webp;base64,{}",
        STANDARD.encode(&valid_webp)
    )));
    let own_res = upsert_ad_impl(own_ad, Some(pw.to_string()), &master_state).unwrap();
    let own_token = own_res.ads[0].image_token.clone().unwrap();

    save_my_market_profile_impl(
        FrontendSaveMyMarketProfileRequest {
            profile: FrontendUserProfile::default(),
            visibility: CardFieldVisibility::default(),
            skills: vec![],
            max_hops: 2,
            validity_years: 2,
            allow_inquiries: true,
            published: Some(true),
        },
        Some(pw.to_string()),
        &master_state,
    )
    .unwrap();

    let (bob_token, bob_blob_id, _ct) =
        humoco_market::seal_media_blob(&bob_did, &valid_webp).unwrap();
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let drop_desc = humoco_market::DropDescriptor {
        write_url: "https://sync.example/sync.php".into(),
        read_url: "https://sync.example/d".into(),
        slot_id: None,
        drop_pub: "11".repeat(32),
    };
    {
        let mut s = master_state.service.lock().unwrap();
        let (own_did, own_sk) = s
            .with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.signing_key.clone()))
            .unwrap();
        let mut stored = load_own_market_profile_for_write(&mut s, Some(pw)).unwrap();
        stored.last_card = Some(
            humoco_market::sign_sd_card(
                humoco_market::CardPayloadInput {
                    did: own_did,
                    v: 1,
                    valid_until: now + 60,
                    max_hops: 2,
                    ad_count: 1,
                    allow_inquiries: true,
                    drops: vec![drop_desc.clone()],
                },
                vec![],
                &own_sk,
            )
            .unwrap(),
        );
        save_own_market_profile_state(&master_state, &mut s, &stored, Some(pw)).unwrap();

        let expired_bob = humoco_market::sign_sd_card(
            humoco_market::CardPayloadInput {
                did: bob_did.clone(),
                v: 1,
                valid_until: 100,
                max_hops: 2,
                ad_count: 0,
                allow_inquiries: true,
                drops: vec![drop_desc],
            },
            vec![],
            &bob_sk,
        )
        .unwrap();
        let expired_wire =
            humoco_market::render_wire(&expired_bob, 1, humoco_market::WireFilter::Full).unwrap();
        let mut card_store = humoco_market::CardStore::default();
        card_store.cards.insert(
            bob_did.clone(),
            humoco_market::StoredCard {
                card: expired_bob,
                card_wire: expired_wire,
                hops: 1,
                received_at: 100,
            },
        );
        crate::state::save_encrypted_json(&mut s, MARKET_CARD_STORE_KEY, &card_store, Some(pw))
            .unwrap();
        crate::state::save_local_sidecar_bytes(
            &mut s,
            &crate::state::cached_media_storage_key(&bob_blob_id),
            &valid_webp,
            Some(pw),
        )
        .unwrap();
    }

    SeededS11F5Env {
        _dir: dir,
        wallet_dir,
        folder_name,
        bob_did,
        charlie_did,
        own_token,
        bob_token,
        valid_webp,
    }
}

#[test]
fn s11_f5_receive_only_cannot_auto_renew_own_card_or_persist_pruned_store() {
    let pw = "MasterPass-S11-F5!";
    let cashier_pin = "112233";
    let env = seed_s11_f5_wallet("s11_f5_receive_only", pw, cashier_pin);

    let mut ro_svc = human_money_core::app_service::AppService::new(&env.wallet_dir).unwrap();
    let granted = ro_svc
        .login_with_secondary(
            &env.folder_name,
            cashier_pin,
            true,
            "device-drops-test".into(),
        )
        .unwrap();
    assert_eq!(granted, human_money_core::AccessMode::ReceiveOnly);
    ro_svc.unlock_secondary_session(cashier_pin, 600).unwrap();
    let ro_state = crate::test_support::create_test_state(ro_svc);

    let snap = get_market_view_impl(None, &ro_state).unwrap();
    assert_eq!(snap.stored_card_count, 0);

    let mut s = ro_state.service.lock().unwrap();
    let stored_after = load_own_market_profile_for_write(&mut s, None).unwrap();
    assert_eq!(
        stored_after.last_card.unwrap().payload.v,
        1,
        "ReceiveOnly must NOT auto-renew/re-sign own card"
    );
    let disk_store: humoco_market::CardStore =
        crate::state::load_encrypted_json_or_default(&mut s, MARKET_CARD_STORE_KEY, None).unwrap();
    assert_eq!(
        disk_store.cards.len(),
        1,
        "ReceiveOnly must NOT persist expired-card prune to disk"
    );
}

#[tokio::test]
async fn s11_f5_one_shot_pin_grant_and_foreign_media_owner_binding() {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let pw = "MasterPass-S11-F5!";
    let env = seed_s11_f5_wallet("s11_f5_one_shot", pw, "112233");
    let expected_webp_uri = format!("data:image/webp;base64,{}", STANDARD.encode(&env.valid_webp));

    let mut m_svc = human_money_core::app_service::AppService::new(&env.wallet_dir).unwrap();
    m_svc
        .login(&env.folder_name, pw, false, "device-drops-test".into())
        .unwrap();
    let m_state = crate::test_support::create_test_state(m_svc);
    let issue_fresh_grant = |st: &AppState| -> String {
        let s = st.service.lock().unwrap();
        st.issue_one_shot_pin_grant(&s, zeroize::Zeroizing::new(pw.to_string()))
            .unwrap()
    };
    {
        let mut s = m_state.service.lock().unwrap();
        s.lock_session();
    }

    assert!(get_app_notices_impl(Some(issue_fresh_grant(&m_state)), &m_state).is_ok());
    assert!(get_market_view_impl(Some(issue_fresh_grant(&m_state)), &m_state).is_ok());
    assert_eq!(
        get_ad_image_impl(
            String::new(),
            env.own_token.clone(),
            Some(issue_fresh_grant(&m_state)),
            &m_state
        )
        .await
        .unwrap(),
        Some(expected_webp_uri.clone()),
        "AdEditorModal empty ownerDid own-media preview must return expected URI via OneShotPinGrant"
    );
    assert_frontend_err_msg(
        get_ad_image_impl(
            "bad-did".to_string(),
            env.bob_token.clone(),
            Some(issue_fresh_grant(&m_state)),
            &m_state,
        )
        .await,
        "Invalid owner DID:",
    );
    assert_eq!(
        get_ad_image_impl(
            env.charlie_did,
            env.bob_token.clone(),
            Some(issue_fresh_grant(&m_state)),
            &m_state
        )
        .await
        .unwrap(),
        None,
        "Foreign cached media must not be served under wrong owner_did"
    );
    assert_eq!(
        get_ad_image_impl(
            env.bob_did,
            env.bob_token,
            Some(issue_fresh_grant(&m_state)),
            &m_state
        )
        .await
        .unwrap(),
        Some(expected_webp_uri)
    );
}

#[tokio::test]
async fn s11_q1_media_negative_cache_differentiated_ttls() {
    let dir = temp_test_dir("s11_q1_neg_cache");
    let pw = "MasterPass-S11-Q1!";
    let alice_svc = crate::test_support::setup_test_app(dir.path(), "Alice Q1", ALICE_PHRASE, pw);
    let state = crate::test_support::create_test_state(alice_svc);
    let alice_did = state
        .service
        .lock()
        .unwrap()
        .with_wallet(|w| w.get_user_id().to_string())
        .unwrap();
    let session = {
        let s = state.service.lock().unwrap();
        state.session_token_for_service(&s).unwrap()
    };

    let server =
        crate::test_support::MockSyncServer::start("d-q1".to_string(), "a1".repeat(32)).await;
    let read_url_404 = format!("{}/d-q1", server.base_url);
    let read_url_corrupt = format!("{}/d-q1", server.base_url);

    // Start a one-shot closed port for deterministic transient error without touching real network
    let dead_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = dead_listener.local_addr().unwrap().port();
    drop(dead_listener);
    let dead_url = format!("http://127.0.0.1:{}/d-dead", dead_port);

    // 1. Transient failure -> ~60s expiry (cached at +30s, expired at +60s)
    let webp_transient = make_test_vp8_webp(64, 64);
    let (tok_transient, blob_transient, _) =
        humoco_market::seal_media_blob(&alice_did, &webp_transient).unwrap();
    state.set_cached_security_policy(Some(crate::settings::SecurityPolicy {
        market_selected_syncs: vec![crate::models::FrontendSyncDescriptor {
            write_url: format!("{}/sync.php", dead_url),
            read_url: dead_url.clone(),
            slot_id: None,
            drop_pub: "11".repeat(32),
            status: "active".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    }));
    let before = chrono::Utc::now().timestamp().max(0) as u64;
    assert_eq!(
        get_ad_image_impl(
            alice_did.clone(),
            tok_transient,
            Some(pw.to_string()),
            &state
        )
        .await
        .unwrap(),
        None
    );
    assert!(state.is_media_negatively_cached(&session, &blob_transient, &dead_url, before + 30));
    assert!(!state.is_media_negatively_cached(
        &session,
        &blob_transient,
        &dead_url,
        before + MEDIA_NEG_CACHE_TRANSIENT_TTL_SECS + 5
    ));

    // 2. Permanent 404 -> 24h expiry (still cached at +3600s, expired after 24h)
    let tok_404 = "fedcba9876543210fedcba9876543210".to_string();
    let blob_404 = humoco_market::derive_media_blob_id(&tok_404).unwrap();
    state.set_cached_security_policy(Some(crate::settings::SecurityPolicy {
        market_selected_syncs: vec![crate::models::FrontendSyncDescriptor {
            write_url: format!("{}/sync.php", server.base_url),
            read_url: read_url_404.clone(),
            slot_id: None,
            drop_pub: "22".repeat(32),
            status: "active".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    }));
    let before_perm = chrono::Utc::now().timestamp().max(0) as u64;
    assert_eq!(
        get_ad_image_impl(alice_did.clone(), tok_404, Some(pw.to_string()), &state)
            .await
            .unwrap(),
        None
    );
    assert!(state.is_media_negatively_cached(
        &session,
        &blob_404,
        &read_url_404,
        before_perm + 3600
    ));
    assert!(!state.is_media_negatively_cached(
        &session,
        &blob_404,
        &read_url_404,
        before_perm + MEDIA_NEG_CACHE_PERMANENT_TTL_SECS + 5
    ));

    // 3. Corrupt ciphertext -> 24h expiry
    let tok_corrupt = "11223344556677889900aabbccddeeff".to_string();
    let blob_corrupt = humoco_market::derive_media_blob_id(&tok_corrupt).unwrap();
    server.slots.lock().unwrap().insert(
        format!("{}.media", blob_corrupt),
        b"not-valid-aead".to_vec(),
    );
    let before_corrupt = chrono::Utc::now().timestamp().max(0) as u64;
    assert_eq!(
        get_ad_image_impl(alice_did, tok_corrupt, Some(pw.to_string()), &state)
            .await
            .unwrap(),
        None
    );
    assert!(state.is_media_negatively_cached(
        &session,
        &blob_corrupt,
        &read_url_corrupt,
        before_corrupt + 3600
    ));
}

#[tokio::test]
async fn s11_q2_timeout_zero_marketplace_view_and_ad_image_from_caches() {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let dir = temp_test_dir("s11_q2_timeout_zero");
    let pw = "MasterPass-S11-Q2!";
    let wallet_dir = dir.path().join("alice");

    let server =
        crate::test_support::MockSyncServer::start("d-q2".to_string(), "a1".repeat(32)).await;
    let read_url = format!("{}/d-q2", server.base_url);

    let alice_svc = crate::test_support::setup_test_app(&wallet_dir, "Alice Q2", ALICE_PHRASE, pw);
    let folder_name = alice_svc.list_profiles().unwrap()[0].folder_name.clone();
    let alice_state = crate::test_support::create_test_state(alice_svc);

    let bob_svc =
        crate::test_support::setup_test_app(&dir.path().join("bob"), "Bob Q2", BOB_PHRASE, pw);
    let (bob_did, bob_sk) = bob_svc
        .with_wallet_and_identity(|w, id| (w.get_user_id().to_string(), id.signing_key.clone()))
        .unwrap();
    drop(bob_svc);

    let ad_bytes = make_test_vp8_webp(128, 96);
    let ad_data_uri = format!("data:image/webp;base64,{}", STANDARD.encode(&ad_bytes));
    let (image_token, blob_id, sealed_media) =
        humoco_market::seal_media_blob(&bob_did, &ad_bytes).unwrap();
    server
        .slots
        .lock()
        .unwrap()
        .insert(format!("{}.media", blob_id), sealed_media);

    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let bob_card = humoco_market::sign_sd_card(
        humoco_market::CardPayloadInput {
            did: bob_did.clone(),
            v: 1,
            valid_until: now + 86_400 * 60,
            max_hops: 2,
            ad_count: 1,
            allow_inquiries: true,
            drops: vec![humoco_market::DropDescriptor {
                write_url: format!("{}/sync.php", server.base_url),
                read_url: read_url.clone(),
                slot_id: None,
                drop_pub: "11".repeat(32),
            }],
        },
        vec![(
            humoco_market::CardItem::Ad(humoco_market::Ad {
                id: "ad01020304050607".to_string(),
                kind: humoco_market::AdKind::Offer,
                title: "Cached Pottery".to_string(),
                text: None,
                pricing: None,
                published_at: now.saturating_sub(60),
                renewed_at: None,
                expires_at: now + 86_400 * 30,
                allow_inquiries: true,
                is_remote: false,
                image_token: Some(image_token.clone()),
                event_at: None,
                event_all_day: false,
            }),
            1,
        )],
        &bob_sk,
    )
    .unwrap();
    let bob_wire =
        humoco_market::render_wire(&bob_card, 1, humoco_market::WireFilter::Full).unwrap();
    let mut card_store = humoco_market::CardStore::default();
    card_store.cards.insert(
        bob_did.clone(),
        humoco_market::StoredCard {
            card: bob_card,
            card_wire: bob_wire,
            hops: 1,
            received_at: now,
        },
    );

    let sync_state = crate::state::StoredSyncState {
        last_upload_at: Some(now.saturating_sub(120)),
        ..Default::default()
    };

    {
        let mut s = alice_state.service.lock().unwrap();
        alice_state
            .save_card_store(&mut s, &card_store, Some(pw))
            .unwrap();
        alice_state
            .save_sync_state(&mut s, &sync_state, Some(pw))
            .unwrap();
        alice_state
            .mutate_security_policy(&mut s, Some(pw), |p| {
                p.session_timeout_seconds = 0;
            })
            .unwrap();
    }
    drop(alice_state);

    // Reload fresh AppService under session_timeout_seconds == 0 (!is_session_active())
    let mut reloaded_svc = human_money_core::app_service::AppService::new(&wallet_dir).unwrap();
    reloaded_svc
        .login(&folder_name, pw, false, "device-drops-test".to_string())
        .unwrap();
    let reloaded_state = crate::test_support::create_test_state(
        human_money_core::app_service::AppService::new(&wallet_dir).unwrap(),
    );
    reloaded_state
        .initialize_profile_session(&mut reloaded_svc, pw)
        .unwrap();
    *reloaded_state.service.lock().unwrap() = reloaded_svc;
    assert!(!reloaded_state.service.lock().unwrap().is_session_active());

    // 1. Marketplace snapshot + sync state work without password via pre-cached CardStore + StoredSyncState
    let snap = get_market_view_impl(None, &reloaded_state)
        .expect("get_market_view_impl must succeed under timeout == 0 using pre-caches");
    assert_eq!(snap.stored_card_count, 1);
    assert_eq!(snap.board.len(), 1);
    assert_eq!(snap.board[0].ad.title, "Cached Pottery");
    {
        let mut s = reloaded_state.service.lock().unwrap();
        let cached_sync = reloaded_state
            .load_sync_state_or_cached(&mut s, None)
            .unwrap();
        assert_eq!(cached_sync.last_upload_at, Some(now.saturating_sub(120)));
    }
    assert!(
        crate::services::sync_engine::get_sync_overview_status_impl(None, &reloaded_state).is_ok()
    );

    // 2. Remote ad image fetch & AEAD open works without password under timeout == 0
    let loaded_img = get_ad_image_impl(bob_did, image_token, None, &reloaded_state)
        .await
        .expect("get_ad_image_impl must succeed under timeout == 0");
    assert_eq!(loaded_img, Some(ad_data_uri));
}
