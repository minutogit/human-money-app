use ed25519_dalek::SigningKey;
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{Voucher, VoucherStandard};
use human_money_core::services::crypto::create_user_id;
use humoco_market::{
    build_ad_board, build_market_view, new_ad_id, render_wire, sign_sd_card, Ad, AdKind, CardItem,
    CardPayloadInput, EntrySource, Skill, SkillOrigin, StoredCard, TrustTier, WireFilter,
    MAX_SKILL_TEXT_CHARS, MAX_SKILL_TITLE_CHARS,
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
    let persons = build_market_view(&vouchers, &[], &own_did, &classify, 1700000000);

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

    // Skills extracted from newest voucher service_offer (v2)
    assert_eq!(person.skills.len(), 1);
    assert_eq!(person.skills[0].title, "Updated Web Design");
    assert_eq!(person.skills[0].origin, SkillOrigin::Voucher);

    // Voucher needs is NOT a skill or ad, but is preserved on profile.needs!
    assert_eq!(person.profile.needs.as_deref(), Some("Logo Design"));
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
    let persons = build_market_view(&vouchers, &[], "did:key:dummy", &classify, 1700000000);

    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].skills[0].title, "Second Newer Offer");
}

#[test]
fn test_test_vouchers_ignored() {
    let sk = create_keypair();
    let pk = sk.verifying_key();
    let did = create_user_id(&pk, None).unwrap();

    let v_real = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some("Real Offer"),
        None,
        "Minuto",
        "std-1",
        false,
    );
    let v_test = make_test_voucher(
        &did,
        "2026-06-01T10:00:00Z",
        Some("Fake Test Offer"),
        None,
        "TestStandard",
        "std-test",
        true,
    );

    let vouchers = vec![v_real, v_test];
    let classify = |_did: &str| TrustTier::Trusted;
    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify, 1700000000);

    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].skills[0].title, "Real Offer");
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

    let persons = build_market_view(&vouchers, &[], &own_did_prefixed, &classify, 1700000000);
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].did, other_did);
}

#[test]
fn test_hidden_trust_tier_excluded() {
    let sk1 = create_keypair();
    let did1 = create_user_id(&sk1.verifying_key(), None).unwrap();

    let sk2 = create_keypair();
    let did2 = create_user_id(&sk2.verifying_key(), None).unwrap();

    let v1 = make_test_voucher(
        &did1,
        "2026-01-01T10:00:00Z",
        Some("Good"),
        None,
        "M",
        "1",
        false,
    );
    let v2 = make_test_voucher(
        &did2,
        "2026-01-01T10:00:00Z",
        Some("Offender"),
        None,
        "M",
        "1",
        false,
    );

    let vouchers = vec![v1, v2];
    let classify = |did: &str| {
        if did == did2 {
            TrustTier::Hidden
        } else {
            TrustTier::Trusted
        }
    };

    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify, 1700000000);
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].did, did1);
}

#[test]
fn test_person_without_skills_is_included() {
    let sk = create_keypair();
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    let v = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        None,
        Some("   "),
        "M",
        "1",
        false,
    );
    let vouchers = vec![v];
    let classify = |_did: &str| TrustTier::Unknown;

    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify, 1700000000);
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].did, did);
    assert!(persons[0].skills.is_empty());
}

#[test]
fn test_word_boundary_truncation_rules() {
    let sk = create_keypair();
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    // Exactly 60 chars
    let text_60 = "a".repeat(60);
    // 70 chars with space at 50
    let text_70_words = format!("{} extra words", "b".repeat(50));
    // 80 chars single word without spaces
    let text_80_single_word = "c".repeat(80);
    // 250 chars with spaces
    let text_250 = format!("{} end of offer", "d ".repeat(100));

    // Test text_60: title len 60, text None
    let v1 = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some(&text_60),
        None,
        "M",
        "1",
        false,
    );
    let p1 = build_market_view(
        &[v1],
        &[],
        "did:key:other",
        &|_| TrustTier::Trusted,
        1700000000,
    );
    assert_eq!(p1[0].skills[0].title.chars().count(), 60);
    assert_eq!(p1[0].skills[0].text, None);

    // Test text_70_words: title truncated at word boundary <= 60, text is Some(text_70_words)
    let v2 = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some(&text_70_words),
        None,
        "M",
        "1",
        false,
    );
    let p2 = build_market_view(
        &[v2],
        &[],
        "did:key:other",
        &|_| TrustTier::Trusted,
        1700000000,
    );
    assert!(p2[0].skills[0].title.chars().count() <= 60);
    assert!(p2[0].skills[0].title.ends_with('…'));
    assert_eq!(p2[0].skills[0].text, Some(text_70_words));

    // Test text_80_single_word: hard cut at 59 + '…' = 60 chars
    let v3 = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some(&text_80_single_word),
        None,
        "M",
        "1",
        false,
    );
    let p3 = build_market_view(
        &[v3],
        &[],
        "did:key:other",
        &|_| TrustTier::Trusted,
        1700000000,
    );
    assert_eq!(p3[0].skills[0].title.chars().count(), 60);
    assert_eq!(p3[0].skills[0].title, format!("{}…", "c".repeat(59)));
    assert_eq!(p3[0].skills[0].text, Some(text_80_single_word));

    // Test text_250: title <= 60, text <= 160
    let v4 = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some(&text_250),
        None,
        "M",
        "1",
        false,
    );
    let p4 = build_market_view(
        &[v4],
        &[],
        "did:key:other",
        &|_| TrustTier::Trusted,
        1700000000,
    );
    assert!(p4[0].skills[0].title.chars().count() <= 60);
    let text = p4[0].skills[0].text.as_ref().unwrap();
    assert!(text.chars().count() <= 160);
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

    let v1 = make_test_voucher(
        &did1,
        "2026-05-01T10:00:00Z",
        Some("U2"),
        None,
        "M",
        "1",
        false,
    );
    let v2 = make_test_voucher(
        &did2,
        "2026-01-01T10:00:00Z",
        Some("T1"),
        None,
        "M",
        "1",
        false,
    );
    let v3 = make_test_voucher(
        &did3,
        "2026-08-01T10:00:00Z",
        Some("T3"),
        None,
        "M",
        "1",
        false,
    );

    let vouchers = vec![v1, v2, v3];
    let classify = |did: &str| {
        if did == did1 {
            TrustTier::Unknown
        } else {
            TrustTier::Trusted
        }
    };

    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify, 1700000000);
    assert_eq!(persons.len(), 3);
    assert_eq!(persons[0].did, did3);
    assert_eq!(persons[1].did, did2);
    assert_eq!(persons[2].did, did1);
}

#[test]
fn test_skills_deduplication_and_needs_preservation() {
    let sk = create_keypair();
    let did = create_user_id(&sk.verifying_key(), None).unwrap();

    // 1. Voucher with service_offer "Organic Apples" and needs "Tools"
    let v = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some("Organic Apples"),
        Some("Garden Tools"),
        "Minuto",
        "std-1",
        false,
    );

    // 2. Card with a Skill "organic apples" (same title, case-insensitive)
    let card_payload = CardPayloadInput {
        did: did.clone(),
        v: 1769940000,
        valid_until: 1800000000,
        max_hops: 2,
        ad_count: 1,
        allow_inquiries: true,
        drops: vec![],
    };
    let card = sign_sd_card(
        card_payload,
        vec![
            (
                CardItem::Skill(Skill {
                    title: "organic apples".to_string(),
                    text: Some("Card supersedes voucher description".to_string()),
                    pricing: Some("2 Minuto".to_string()),
                    is_remote: false,
                }),
                2,
            ),
            (
                CardItem::Ad(Ad {
                    id: new_ad_id(),
                    kind: AdKind::Offer,
                    title: "Active Apple Ad".to_string(),
                    text: None,
                    pricing: None,
                    published_at: 1700000000,
                    renewed_at: None,
                    expires_at: 1700000000 + 30 * 86_400,
                    allow_inquiries: true,
                    is_remote: false,
                    image_token: None,
                }),
                2,
            ),
        ],
        &sk,
    )
    .unwrap();

    let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
    let stored_card = StoredCard {
        card,
        card_wire: wire,
        hops: 1,
        received_at: 1700000000,
    };

    let persons = build_market_view(
        &[v],
        &[stored_card],
        "did:key:other",
        &|_| TrustTier::Trusted,
        1700000000,
    );

    assert_eq!(persons.len(), 1);
    let p = &persons[0];

    // Card skill superseded voucher skill (exactly 1 skill with Card origin)
    assert_eq!(p.skills.len(), 1);
    assert_eq!(p.skills[0].origin, SkillOrigin::Card);
    assert_eq!(
        p.skills[0].text.as_deref(),
        Some("Card supersedes voucher description")
    );

    // Voucher needs is preserved on profile.needs!
    assert_eq!(p.profile.needs.as_deref(), Some("Garden Tools"));

    // Active ad is present
    assert_eq!(p.active_ads.len(), 1);
    assert_eq!(p.active_ads[0].title, "Active Apple Ad");

    // Ad board test
    let board = build_ad_board(&persons);
    assert_eq!(board.len(), 1);
    assert_eq!(board[0].owner_did, did);
    assert_eq!(board[0].ad.title, "Active Apple Ad");
}

#[test]
fn test_filter_expired_ads_and_board_sorting() {
    let now = 1700000000;

    let sk1 = create_keypair();
    let did1 = create_user_id(&sk1.verifying_key(), None).unwrap();
    let card1 = sign_sd_card(
        CardPayloadInput {
            did: did1.clone(),
            v: 100,
            valid_until: 1800000000,
            max_hops: 2,
            ad_count: 2,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![
            (
                CardItem::Ad(Ad {
                    id: "0000000000000001".to_string(),
                    kind: AdKind::Offer,
                    title: "Active Older Ad".to_string(),
                    text: None,
                    pricing: None,
                    published_at: 1690000000,
                    renewed_at: None,
                    expires_at: now + 1000, // active
                    allow_inquiries: true,
                    is_remote: false,
                    image_token: None,
                }),
                2,
            ),
            (
                CardItem::Ad(Ad {
                    id: "0000000000000002".to_string(),
                    kind: AdKind::Offer,
                    title: "Expired Ad".to_string(),
                    text: None,
                    pricing: None,
                    published_at: now - 40 * 86_400,
                    renewed_at: None,
                    expires_at: now - 10 * 86_400, // EXPIRED!
                    allow_inquiries: true,
                    is_remote: false,
                    image_token: None,
                }),
                2,
            ),
        ],
        &sk1,
    )
    .unwrap();

    let sk2 = create_keypair();
    let did2 = create_user_id(&sk2.verifying_key(), None).unwrap();
    let card2 = sign_sd_card(
        CardPayloadInput {
            did: did2.clone(),
            v: 100,
            valid_until: 1800000000,
            max_hops: 2,
            ad_count: 1,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![(
            CardItem::Ad(Ad {
                id: "0000000000000003".to_string(),
                kind: AdKind::Request,
                title: "Active Newer Renewed Ad".to_string(),
                text: None,
                pricing: None,
                published_at: 1680000000,
                renewed_at: Some(1695000000), // renewed later than ad 1
                expires_at: now + 5000,
                allow_inquiries: true,
                is_remote: false,
                image_token: None,
            }),
            2,
        )],
        &sk2,
    )
    .unwrap();

    let stored1 = StoredCard {
        card_wire: render_wire(&card1, 1, WireFilter::Full).unwrap(),
        card: card1,
        hops: 1,
        received_at: now,
    };
    let stored2 = StoredCard {
        card_wire: render_wire(&card2, 1, WireFilter::Full).unwrap(),
        card: card2,
        hops: 1,
        received_at: now,
    };

    let persons = build_market_view(
        &[],
        &[stored1, stored2],
        "did:key:other",
        &|_| TrustTier::Trusted,
        now,
    );
    assert_eq!(persons.len(), 2);

    let p1 = persons.iter().find(|p| p.did == did1).unwrap();
    // Only 1 active ad (the expired one was filtered out)
    assert_eq!(p1.active_ads.len(), 1);
    assert_eq!(p1.active_ads[0].id, "0000000000000001");

    let p2 = persons.iter().find(|p| p.did == did2).unwrap();
    assert_eq!(p2.active_ads.len(), 1);
    assert_eq!(p2.active_ads[0].id, "0000000000000003");

    // Board sorting: Renewed Ad (1695000000) before Older Ad (1690000000)
    let board = build_ad_board(&persons);
    assert_eq!(board.len(), 2);
    assert_eq!(board[0].ad.id, "0000000000000003");
    assert_eq!(board[1].ad.id, "0000000000000001");
}

proptest! {
    #[test]
    fn proptest_truncation_invariants(input in "\\PC*") {
        let sk = create_keypair();
        let did = create_user_id(&sk.verifying_key(), None).unwrap();
        let v = make_test_voucher(&did, "2026-01-01T10:00:00Z", Some(&input), None, "M", "1", false);
        let persons = build_market_view(&[v], &[], "did:key:other", &|_| TrustTier::Trusted, 1700000000);

        if input.trim().is_empty() {
            prop_assert!(persons[0].skills.is_empty());
        } else {
            prop_assert_eq!(persons[0].skills.len(), 1);
            let skill = &persons[0].skills[0];
            let title_chars = skill.title.chars().count();
            prop_assert!(title_chars <= MAX_SKILL_TITLE_CHARS);

            if let Some(ref text) = skill.text {
                let text_chars = text.chars().count();
                prop_assert!(text_chars <= MAX_SKILL_TEXT_CHARS);
            }
        }
    }
}
