use ed25519_dalek::SigningKey;
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{Voucher, VoucherStandard};
use human_money_core::services::crypto::create_user_id;
use humoco_market::{
    build_market_view, EntrySource, ListingOrigin, ListingType, TrustTier,
    MAX_LISTING_TEXT_CHARS, MAX_LISTING_TITLE_CHARS,
};
use proptest::prelude::*;

static KEY_COUNTER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

fn create_keypair() -> SigningKey {
    let mut bytes = [0u8; 32];
    bytes[0] = KEY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    SigningKey::from_bytes(&bytes)
}

fn make_test_voucher(
    creator_id: &str,
    creation_date: &str,
    service_offer: Option<&str>,
    needs: Option<&str>,
    std_name: &str,
    std_uuid: &str,
    is_test: bool,
) -> Voucher {
    Voucher {
        voucher_id: format!("v-{}", std_uuid),
        creation_date: creation_date.to_string(),
        non_redeemable_test_voucher: is_test,
        voucher_standard: VoucherStandard {
            name: std_name.to_string(),
            uuid: std_uuid.to_string(),
            standard_definition_hash: "mock_hash".to_string(),
        },
        creator_profile: PublicProfile {
            id: Some(creator_id.to_string()),
            service_offer: service_offer.map(|s| s.to_string()),
            needs: needs.map(|s| s.to_string()),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn test_prefix_variants_merge_and_deduplicate_standards() {
    let sk = create_keypair();
    let pk = sk.verifying_key();
    let canonical_did = create_user_id(&pk, None).unwrap();
    let prefix1_id = create_user_id(&pk, Some("alice")).unwrap();
    let prefix2_id = create_user_id(&pk, Some("bob")).unwrap();

    let v1 = make_test_voucher(
        &prefix1_id,
        "2026-01-01T10:00:00Z",
        Some("Web Design"),
        None,
        "Minuto",
        "std-1",
        false,
    );
    let v2 = make_test_voucher(
        &prefix2_id,
        "2026-02-01T10:00:00Z",
        Some("Updated Web Design"),
        Some("Logo Design"),
        "FreeTaler",
        "std-2",
        false,
    );
    let v3 = make_test_voucher(
        &prefix1_id,
        "2026-01-15T10:00:00Z",
        None,
        None,
        "Minuto",
        "std-1", // duplicate standard uuid
        false,
    );

    let vouchers = vec![v1, v2, v3];
    let own_sk = create_keypair();
    let own_did = create_user_id(&own_sk.verifying_key(), None).unwrap();

    let classify = |_did: &str| TrustTier::Trusted;
    let persons = build_market_view(&vouchers, &[], &own_did, &classify);

    assert_eq!(persons.len(), 1);
    let person = &persons[0];
    assert_eq!(person.did, canonical_did);
    assert_eq!(person.as_of, 1769940000); // 2026-02-01T10:00:00Z
    assert_eq!(person.trust, TrustTier::Trusted);
    assert_eq!(person.sources, vec![EntrySource::Voucher]);

    // Standards deduplicated: std-1 and std-2
    assert_eq!(person.standards.len(), 2);
    assert_eq!(person.standards[0].uuid, "std-1");
    assert_eq!(person.standards[1].uuid, "std-2");

    // Listings extracted from newest profile (v2)
    assert_eq!(person.listings.len(), 2);
    assert_eq!(person.listings[0].listing_type, ListingType::Offer);
    assert_eq!(person.listings[0].title, "Updated Web Design");
    assert_eq!(person.listings[0].origin, ListingOrigin::Voucher);
    assert_eq!(person.listings[1].listing_type, ListingType::Request);
    assert_eq!(person.listings[1].title, "Logo Design");
}

#[test]
fn test_newest_creation_date_profile_wins() {
    let sk = create_keypair();
    let pk = sk.verifying_key();
    let did = create_user_id(&pk, None).unwrap();

    let v1 = make_test_voucher(
        &did,
        "2026-03-01T12:00:00Z",
        Some("First Offer"),
        None,
        "Minuto",
        "std-1",
        false,
    );
    let v2 = make_test_voucher(
        &did,
        "2026-05-01T12:00:00Z",
        Some("Second Newer Offer"),
        None,
        "Minuto",
        "std-1",
        false,
    );

    let vouchers = vec![v1, v2];
    let classify = |_did: &str| TrustTier::Unknown;
    let persons = build_market_view(&vouchers, &[], "did:key:dummy", &classify);

    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].listings[0].title, "Second Newer Offer");
}

#[test]
fn test_test_vouchers_ignored() {
    let sk = create_keypair();
    let pk = sk.verifying_key();
    let did = create_user_id(&pk, None).unwrap();

    // Real voucher
    let v_real = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some("Real Offer"),
        None,
        "Minuto",
        "std-1",
        false,
    );
    // Newer test voucher
    let v_test = make_test_voucher(
        &did,
        "2026-06-01T10:00:00Z",
        Some("Fake Test Offer"),
        None,
        "TestStandard",
        "std-test",
        true, // non_redeemable_test_voucher
    );

    let vouchers = vec![v_real, v_test];
    let classify = |_did: &str| TrustTier::Trusted;
    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify);

    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].listings[0].title, "Real Offer");
    assert_eq!(persons[0].standards.len(), 1);
    assert_eq!(persons[0].standards[0].uuid, "std-1");
}

#[test]
fn test_own_did_excluded_even_with_prefix() {
    let own_sk = create_keypair();
    let own_pk = own_sk.verifying_key();
    let own_did_prefixed = create_user_id(&own_pk, Some("my-account")).unwrap();
    let own_did_canonical = create_user_id(&own_pk, None).unwrap();

    let other_sk = create_keypair();
    let other_did = create_user_id(&other_sk.verifying_key(), None).unwrap();

    let v_own = make_test_voucher(
        &own_did_canonical,
        "2026-01-01T10:00:00Z",
        Some("My own services"),
        None,
        "Minuto",
        "std-1",
        false,
    );
    let v_other = make_test_voucher(
        &other_did,
        "2026-01-01T10:00:00Z",
        Some("Other services"),
        None,
        "Minuto",
        "std-1",
        false,
    );

    let vouchers = vec![v_own, v_other];
    let classify = |_did: &str| TrustTier::Trusted;

    // Passing own_did with prefix
    let persons = build_market_view(&vouchers, &[], &own_did_prefixed, &classify);
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].did, other_did);
}

#[test]
fn test_hidden_trust_tier_excluded() {
    let sk1 = create_keypair();
    let did1 = create_user_id(&sk1.verifying_key(), None).unwrap();

    let sk2 = create_keypair();
    let did2 = create_user_id(&sk2.verifying_key(), None).unwrap();

    let v1 = make_test_voucher(&did1, "2026-01-01T10:00:00Z", Some("Good"), None, "M", "1", false);
    let v2 = make_test_voucher(&did2, "2026-01-01T10:00:00Z", Some("Offender"), None, "M", "1", false);

    let vouchers = vec![v1, v2];
    let classify = |did: &str| {
        if did == did2 {
            TrustTier::Hidden
        } else {
            TrustTier::Trusted
        }
    };

    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify);
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].did, did1);
}

#[test]
fn test_person_without_listings_is_included() {
    let sk = create_keypair();
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    let v = make_test_voucher(&did, "2026-01-01T10:00:00Z", None, Some("   "), "M", "1", false);
    let vouchers = vec![v];
    let classify = |_did: &str| TrustTier::Unknown;

    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify);
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].did, did);
    assert!(persons[0].listings.is_empty());
}

#[test]
fn test_word_boundary_truncation_rules() {
    let sk = create_keypair();
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    // Exactly 80 chars
    let text_80 = "a".repeat(80);
    // 85 chars with space at 70
    let text_85_words = format!("{} extra words", "b".repeat(70));
    // 90 chars single word without spaces
    let text_90_single_word = "c".repeat(90);
    // 350 chars with spaces
    let text_350 = format!("{} end of offer", "d ".repeat(150));

    // Test text_80: should have title len 80, text None
    let v1 = make_test_voucher(&did, "2026-01-01T10:00:00Z", Some(&text_80), None, "M", "1", false);
    let p1 = build_market_view(&[v1], &[], "did:key:other", &|_| TrustTier::Trusted);
    assert_eq!(p1[0].listings[0].title.chars().count(), 80);
    assert_eq!(p1[0].listings[0].text, None);

    // Test text_85_words: title truncated at word boundary <= 80, text is Some(text_85_words)
    let v2 = make_test_voucher(&did, "2026-01-01T10:00:00Z", Some(&text_85_words), None, "M", "1", false);
    let p2 = build_market_view(&[v2], &[], "did:key:other", &|_| TrustTier::Trusted);
    assert!(p2[0].listings[0].title.chars().count() <= 80);
    assert!(p2[0].listings[0].title.ends_with('…'));
    assert_eq!(p2[0].listings[0].text, Some(text_85_words));

    // Test text_90_single_word: hard cut at 79 + '…' = 80 chars
    let v3 = make_test_voucher(&did, "2026-01-01T10:00:00Z", Some(&text_90_single_word), None, "M", "1", false);
    let p3 = build_market_view(&[v3], &[], "did:key:other", &|_| TrustTier::Trusted);
    assert_eq!(p3[0].listings[0].title.chars().count(), 80);
    assert_eq!(p3[0].listings[0].title, format!("{}…", "c".repeat(79)));
    assert_eq!(p3[0].listings[0].text, Some(text_90_single_word));

    // Test text_350: title <= 80, text <= 280
    let v4 = make_test_voucher(&did, "2026-01-01T10:00:00Z", Some(&text_350), None, "M", "1", false);
    let p4 = build_market_view(&[v4], &[], "did:key:other", &|_| TrustTier::Trusted);
    assert!(p4[0].listings[0].title.chars().count() <= 80);
    let text = p4[0].listings[0].text.as_ref().unwrap();
    assert!(text.chars().count() <= 280);
    assert!(text.ends_with('…'));
}

#[test]
fn test_sorting_order() {
    let sk1 = create_keypair();
    let did1 = create_user_id(&sk1.verifying_key(), None).unwrap();

    let sk2 = create_keypair();
    let did2 = create_user_id(&sk2.verifying_key(), None).unwrap();

    let sk3 = create_keypair();
    let did3 = create_user_id(&sk3.verifying_key(), None).unwrap();

    // did1: Unknown, timestamp 2000
    let v1 = make_test_voucher(&did1, "2026-05-01T10:00:00Z", Some("U2"), None, "M", "1", false);
    // did2: Trusted, timestamp 1000
    let v2 = make_test_voucher(&did2, "2026-01-01T10:00:00Z", Some("T1"), None, "M", "1", false);
    // did3: Trusted, timestamp 3000
    let v3 = make_test_voucher(&did3, "2026-08-01T10:00:00Z", Some("T3"), None, "M", "1", false);

    let vouchers = vec![v1, v2, v3];
    let classify = |did: &str| {
        if did == did1 {
            TrustTier::Unknown
        } else {
            TrustTier::Trusted
        }
    };

    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify);
    assert_eq!(persons.len(), 3);
    // Expected order:
    // 1. did3 (Trusted, newest 2026-08-01)
    // 2. did2 (Trusted, 2026-01-01)
    // 3. did1 (Unknown, 2026-05-01)
    assert_eq!(persons[0].did, did3);
    assert_eq!(persons[1].did, did2);
    assert_eq!(persons[2].did, did1);
}

#[test]
fn test_acceptance_criteria_phase_1() {
    // 3 creators:
    // Creator 1 uses 2 prefix accounts with the same public key
    let sk1 = create_keypair();
    let pk1 = sk1.verifying_key();
    let did1_canon = create_user_id(&pk1, None).unwrap();
    let did1_pre1 = create_user_id(&pk1, Some("laptop")).unwrap();
    let did1_pre2 = create_user_id(&pk1, Some("phone")).unwrap();

    // Creator 2
    let sk2 = create_keypair();
    let did2 = create_user_id(&sk2.verifying_key(), None).unwrap();

    // Creator 3 (will be KnownOffender)
    let sk3 = create_keypair();
    let did3 = create_user_id(&sk3.verifying_key(), None).unwrap();

    let v1a = make_test_voucher(&did1_pre1, "2026-01-01T10:00:00Z", Some("C1 Offer 1"), None, "M", "std-1", false);
    let v1b = make_test_voucher(&did1_pre2, "2026-02-01T10:00:00Z", Some("C1 Offer 2 Newer"), None, "F", "std-2", false);
    let v2 = make_test_voucher(&did2, "2026-01-10T10:00:00Z", Some("C2 Offer"), None, "M", "std-1", false);
    let v3 = make_test_voucher(&did3, "2026-01-20T10:00:00Z", Some("C3 Offender Offer"), None, "M", "std-1", false);

    let vouchers = vec![v1a, v1b, v2, v3];

    // Scenario A: Without KnownOffender (all Trusted or Unknown):
    // 3 creators in total -> exactly 3 persons returned!
    let classify_clean = |_did: &str| TrustTier::Trusted;
    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify_clean);
    assert_eq!(persons.len(), 3);
    assert_eq!(persons.iter().filter(|p| p.did == did1_canon).count(), 1);
    assert_eq!(persons.iter().filter(|p| p.did == did2).count(), 1);
    assert_eq!(persons.iter().filter(|p| p.did == did3).count(), 1);

    // Scenario B: Creator 3 is KnownOffender -> Hidden
    let classify_with_offender = |did: &str| {
        if did == did3 {
            TrustTier::Hidden
        } else {
            TrustTier::Trusted
        }
    };
    let persons_offender = build_market_view(&vouchers, &[], "did:key:other", &classify_with_offender);
    assert_eq!(persons_offender.len(), 2);
    assert!(persons_offender.iter().all(|p| p.did != did3));
    let c1 = persons_offender.iter().find(|p| p.did == did1_canon).unwrap();
    assert_eq!(c1.listings[0].title, "C1 Offer 2 Newer");
    assert_eq!(c1.standards.len(), 2);
}

proptest! {
    #[test]
    fn proptest_truncation_invariants(input in "\\PC*") {
        let sk = create_keypair();
        let did = create_user_id(&sk.verifying_key(), None).unwrap();
        let v = make_test_voucher(&did, "2026-01-01T10:00:00Z", Some(&input), None, "M", "1", false);
        let persons = build_market_view(&[v], &[], "did:key:other", &|_| TrustTier::Trusted);

        if input.trim().is_empty() {
            prop_assert!(persons[0].listings.is_empty());
        } else {
            prop_assert_eq!(persons[0].listings.len(), 1);
            let listing = &persons[0].listings[0];
            let title_chars = listing.title.chars().count();
            prop_assert!(title_chars <= MAX_LISTING_TITLE_CHARS);

            if let Some(ref text) = listing.text {
                let text_chars = text.chars().count();
                prop_assert!(text_chars <= MAX_LISTING_TEXT_CHARS);
            }
        }
    }
}
