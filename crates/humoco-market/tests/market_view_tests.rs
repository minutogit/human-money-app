use ed25519_dalek::SigningKey;
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{Voucher, VoucherStandard};
use human_money_core::services::crypto::create_user_id;
use humoco_market::{
    accept_envelope, build_ad_board, build_market_view, forward_envelope, new_ad_id, render_wire,
    sign_sd_card, verify_sd_card, AcceptOutcome, Ad, AdKind, CardEnvelope, CardItem,
    CardPayloadInput, CardStore, DropDescriptor, EntrySource, MarketPerson, SdCard, Skill,
    SkillOrigin, StoredCard, TrustTier, WireFilter, MAX_SKILL_TEXT_CHARS, MAX_SKILL_TITLE_CHARS,
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

fn make_test_ad(id: &str, title: &str, published_at: u64, expires_at: u64) -> Ad {
    Ad {
        id: id.to_string(),
        kind: AdKind::Offer,
        title: title.to_string(),
        text: None,
        pricing: None,
        published_at,
        renewed_at: None,
        expires_at,
        allow_inquiries: true,
        is_remote: false,
        image_token: None,
        event_at: None,
        event_all_day: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_named_ad_card(
    sk: &SigningKey,
    did: &str,
    v: u64,
    valid_until: u64,
    max_hops: u8,
    item_hops: u8,
    first_name: &str,
    ad: Ad,
    drops: Vec<DropDescriptor>,
) -> SdCard {
    sign_sd_card(
        CardPayloadInput {
            did: did.to_string(),
            v,
            valid_until,
            max_hops,
            ad_count: 1,
            allow_inquiries: true,
            drops,
        },
        vec![
            (
                CardItem::Name {
                    first_name: Some(first_name.to_string()),
                    last_name: None,
                },
                item_hops,
            ),
            (CardItem::Ad(ad), item_hops),
        ],
        sk,
    )
    .unwrap()
}

#[test]
fn test_prefix_variants_separate_profiles_and_merge_within_prefix() {
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
        "std-1", // duplicate standard uuid within prefix1
        false,
    );

    let vouchers = vec![v1, v2, v3];
    let own_sk = create_keypair();
    let own_did = create_user_id(&own_sk.verifying_key(), None).unwrap();

    let classify = |_did: &str| TrustTier::Trusted;
    let persons = build_market_view(&vouchers, &[], &own_did, &classify, 1700000000);

    // Distinct prefixes stay separate MarketPerson entries while sharing canonical root did & trust
    assert_eq!(persons.len(), 2);

    let bob_person = persons
        .iter()
        .find(|p| p.recipient_did == prefix2_id)
        .expect("prefix2 entry");
    assert_eq!(bob_person.did, canonical_did);
    assert_eq!(bob_person.as_of, 1769940000); // 2026-02-01T10:00:00Z
    assert_eq!(bob_person.trust, TrustTier::Trusted);
    assert_eq!(bob_person.sources, vec![EntrySource::Voucher]);
    assert_eq!(bob_person.standards.len(), 1);
    assert_eq!(bob_person.standards[0].uuid, "std-2");
    assert_eq!(bob_person.skills.len(), 1);
    assert_eq!(bob_person.skills[0].title, "Updated Web Design");
    assert_eq!(bob_person.skills[0].origin, SkillOrigin::Voucher);
    assert_eq!(bob_person.profile.needs.as_deref(), Some("Logo Design"));

    let alice_person = persons
        .iter()
        .find(|p| p.recipient_did == prefix1_id)
        .expect("prefix1 entry");
    assert_eq!(alice_person.did, canonical_did);
    assert_eq!(alice_person.trust, TrustTier::Trusted);
    // Same-prefix vouchers merge and deduplicate std-1
    assert_eq!(alice_person.standards.len(), 1);
    assert_eq!(alice_person.standards[0].uuid, "std-1");
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

    let now = 1_700_000_000;
    let own_card = make_named_ad_card(
        &own_sk,
        &own_did_canonical,
        10,
        now + 86_400,
        2,
        2,
        "Own Card",
        make_test_ad("00000000000000aa", "Own Card Ad", now - 5, now + 86_400),
        vec![],
    );
    let own_stored_card = StoredCard {
        card_wire: render_wire(&own_card, 1, WireFilter::Full).unwrap(),
        card: own_card,
        hops: 1,
        received_at: now,
    };

    let vouchers = vec![v_own, v_other];
    let classify = |_did: &str| TrustTier::Trusted;

    let persons = build_market_view(
        &vouchers,
        &[own_stored_card],
        &own_did_prefixed,
        &classify,
        now,
    );
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

    // Exactly 60 chars (title only, text None)
    let text_60 = "a".repeat(60);
    // 70 chars with space at 50
    let text_70_words = format!("{} extra words", "b".repeat(50));
    // 80 chars single word without spaces
    let text_80_single_word = "c".repeat(80);
    // Exactly 160 chars single word vs 161 chars (tests MAX_SKILL_TEXT_CHARS boundary)
    let text_160_exact = "x".repeat(160);
    let text_161_over = "x".repeat(161);
    // 250+ chars with spaces
    let text_250 = format!("{} end of offer", "d ".repeat(100));

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
    assert_eq!(p1[0].skills[0].title, text_60);
    assert_eq!(p1[0].skills[0].text, None);

    // 70 chars with whitespace at index 50 -> exact word-boundary cut at 50 + '…'
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
    assert_eq!(p2[0].skills[0].title, format!("{} extra…", "b".repeat(50)));
    assert_eq!(p2[0].skills[0].text, Some(text_70_words));

    // 80-char unspaced word -> hard cut at 59 + '…' = 60 chars
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

    // Exact 160-char boundary stays intact; 161 chars truncates to 159 + '…'
    let v_exact_160 = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some(&text_160_exact),
        None,
        "M",
        "1",
        false,
    );
    let p_160 = build_market_view(
        &[v_exact_160],
        &[],
        "did:key:other",
        &|_| TrustTier::Trusted,
        1700000000,
    );
    assert_eq!(p_160[0].skills[0].text.as_deref(), Some(text_160_exact.as_str()));

    let v_over_161 = make_test_voucher(
        &did,
        "2026-01-01T10:00:00Z",
        Some(&text_161_over),
        None,
        "M",
        "1",
        false,
    );
    let p_161 = build_market_view(
        &[v_over_161],
        &[],
        "did:key:other",
        &|_| TrustTier::Trusted,
        1700000000,
    );
    assert_eq!(
        p_161[0].skills[0].text.as_deref(),
        Some(format!("{}…", "x".repeat(159)).as_str())
    );

    // 250+ chars -> exact word-boundary cuts for both title (<= 60) and text (<= 160)
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
    assert_eq!(
        p4[0].skills[0].title,
        format!("{}…", "d ".repeat(29).trim_end())
    );
    assert_eq!(
        p4[0].skills[0].text.as_deref(),
        Some(format!("{}…", "d ".repeat(79).trim_end()).as_str())
    );
}

#[test]
fn test_sorting_order() {
    let sk1 = create_keypair();
    let did1 = create_user_id(&sk1.verifying_key(), None).unwrap();

    let sk2 = create_keypair();
    let did2 = create_user_id(&sk2.verifying_key(), None).unwrap();

    let sk3 = create_keypair();
    let did3 = create_user_id(&sk3.verifying_key(), None).unwrap();

    let sk4 = create_keypair();
    let did4 = create_user_id(&sk4.verifying_key(), None).unwrap();

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
    let v4 = make_test_voucher(
        &did4,
        "2026-01-01T10:00:00Z",
        Some("T1-tie"),
        None,
        "M",
        "1",
        false,
    );

    let vouchers = vec![v1, v2, v3, v4];
    let classify = |did: &str| {
        if did == did1 {
            TrustTier::Unknown
        } else {
            TrustTier::Trusted
        }
    };

    let (tie_first, tie_second) = if did2 <= did4 {
        (&did2, &did4)
    } else {
        (&did4, &did2)
    };
    let persons = build_market_view(&vouchers, &[], "did:key:other", &classify, 1700000000);
    assert_eq!(persons.len(), 4);
    assert_eq!(persons[0].did, did3);
    assert_eq!(&persons[1].did, tie_first);
    assert_eq!(&persons[2].did, tie_second);
    assert_eq!(persons[3].did, did1);
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

    // 2. Card with a Skill "organic apples" (same title, case-insensitive) and ad_count = 3 (1 disclosed -> 2 pending)
    let card_payload = CardPayloadInput {
        did: did.clone(),
        v: 1769940000,
        valid_until: 1800000000,
        max_hops: 2,
        ad_count: 3,
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
                    event_at: None,
                    event_all_day: false,
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

    // Active ad is present and undisclosed ads are reported in ads_pending
    assert_eq!(p.active_ads.len(), 1);
    assert_eq!(p.active_ads[0].title, "Active Apple Ad");
    assert_eq!(p.ads_pending, 2);

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
                    event_at: None,
                    event_all_day: false,
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
                    event_at: None,
                    event_all_day: false,
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
                event_at: None,
                event_all_day: false,
            }),
            2,
        )],
        &sk2,
    )
    .unwrap();

    let sk3 = create_keypair();
    let did3 = create_user_id(&sk3.verifying_key(), None).unwrap();
    let card_expired_at_boundary = make_named_ad_card(
        &sk3,
        &did3,
        100,
        now, // exact valid_until == now boundary must be excluded
        2,
        2,
        "Boundary Expired",
        make_test_ad("0000000000000004", "Boundary Card Ad", now - 5, now),
        vec![],
    );
    let stored_boundary_expired = StoredCard {
        card_wire: render_wire(&card_expired_at_boundary, 1, WireFilter::Full).unwrap(),
        card: card_expired_at_boundary,
        hops: 1,
        received_at: now,
    };

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
        &[stored1, stored2, stored_boundary_expired],
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

#[test]
fn test_empty_redacted_card_hides_person_and_ads_in_market_view() {
    let now = 1_700_000_000;
    let alice_sk = create_keypair();
    let alice_did = create_user_id(&alice_sk.verifying_key(), None).unwrap();

    // v1 published at max_hops = 3 with Name + Ad
    let card_v1 = make_named_ad_card(
        &alice_sk,
        &alice_did,
        100,
        now + 30 * 86_400,
        3,
        3,
        "Alice",
        make_test_ad("0000000000000011", "Fresh Honey", now - 10, now + 86_400),
        vec![],
    );

    // Carol (Hop 2) receives v1 and sees both person and ad
    let mut carol_store = CardStore::default();
    let v1_hop2_wire = render_wire(&card_v1, 2, WireFilter::Full).unwrap();
    assert_eq!(
        accept_envelope(
            &mut carol_store,
            &CardEnvelope {
                hops: 2,
                card_wire: v1_hop2_wire,
            },
            now,
            500,
            None,
            &|_| false
        ),
        AcceptOutcome::Accepted {
            did: alice_did.clone(),
            replaced: false,
        }
    );
    let carol_view_v1 = build_market_view(
        &[],
        &carol_store.cards.values().cloned().collect::<Vec<_>>(),
        "did:key:carol",
        &|_| TrustTier::Trusted,
        now,
    );
    assert_eq!(carol_view_v1.len(), 1);
    assert_eq!(build_ad_board(&carol_view_v1).len(), 1);

    // v2 shrinks items to Hop 1 while keeping envelope max_hops = 3 -> hop 1 forward produces bare `<jws>~`
    let card_v2 = make_named_ad_card(
        &alice_sk,
        &alice_did,
        200,
        now + 30 * 86_400,
        3,
        1,
        "Alice",
        make_test_ad("0000000000000011", "Fresh Honey", now - 10, now + 86_400),
        vec![],
    );
    let hop1_wire = render_wire(&card_v2, 1, WireFilter::Full).unwrap();
    let hop1_stored = StoredCard {
        card: verify_sd_card(&hop1_wire).unwrap(),
        card_wire: hop1_wire,
        hops: 1,
        received_at: now,
    };
    let fwd_to_carol = forward_envelope(&hop1_stored).unwrap();
    assert!(fwd_to_carol.card_wire.ends_with('~'));
    assert_eq!(
        accept_envelope(&mut carol_store, &fwd_to_carol, now + 5, 500, None, &|_| {
            false
        }),
        AcceptOutcome::Accepted {
            did: alice_did,
            replaced: true,
        }
    );

    // Store keeps 1 empty shell card (to block zombies/forward), while UI views show nothing
    assert_eq!(carol_store.cards.len(), 1);
    let carol_view_v2 = build_market_view(
        &[],
        &carol_store.cards.values().cloned().collect::<Vec<_>>(),
        "did:key:carol",
        &|_| TrustTier::Trusted,
        now + 5,
    );
    assert!(carol_view_v2.is_empty());
    assert!(build_ad_board(&carol_view_v2).is_empty());
}

#[test]
fn test_empty_redacted_card_blocks_stale_v1_zombie_replay() {
    let now = 1_700_000_000;
    let alice_sk = create_keypair();
    let alice_did = create_user_id(&alice_sk.verifying_key(), None).unwrap();

    let card_v1 = make_named_ad_card(
        &alice_sk,
        &alice_did,
        100,
        now + 30 * 86_400,
        3,
        3,
        "Alice",
        make_test_ad("0000000000000011", "Fresh Honey", now - 10, now + 86_400),
        vec![],
    );
    let card_v2 = make_named_ad_card(
        &alice_sk,
        &alice_did,
        200,
        now + 30 * 86_400,
        3,
        1,
        "Alice",
        make_test_ad("0000000000000011", "Fresh Honey", now - 10, now + 86_400),
        vec![],
    );

    let v1_wire = render_wire(&card_v1, 2, WireFilter::Full).unwrap();
    let v1_stored = StoredCard {
        card: verify_sd_card(&v1_wire).unwrap(),
        card_wire: v1_wire.clone(),
        hops: 2,
        received_at: now,
    };
    let hop1_wire = render_wire(&card_v2, 1, WireFilter::Full).unwrap();
    let v2_shell = forward_envelope(&StoredCard {
        card: verify_sd_card(&hop1_wire).unwrap(),
        card_wire: hop1_wire,
        hops: 1,
        received_at: now,
    })
    .unwrap();
    let v2_shell_stored = StoredCard {
        card: verify_sd_card(&v2_shell.card_wire).unwrap(),
        card_wire: v2_shell.card_wire.clone(),
        hops: v2_shell.hops,
        received_at: now + 5,
    };

    // Passing both stale v1 and newer v2 tombstone directly to build_market_view (either order) hides Alice
    assert!(build_market_view(
        &[],
        &[v1_stored.clone(), v2_shell_stored.clone()],
        "did:key:carol",
        &|_| TrustTier::Trusted,
        now + 5,
    )
    .is_empty());
    assert!(build_market_view(
        &[],
        &[v2_shell_stored, v1_stored],
        "did:key:carol",
        &|_| TrustTier::Trusted,
        now + 5,
    )
    .is_empty());

    // Delayed v1 replay into Carol's store carrying the v2 shell must return exact IgnoredStale
    let mut carol_store = CardStore::default();
    assert_eq!(
        accept_envelope(&mut carol_store, &v2_shell, now + 5, 500, None, &|_| false),
        AcceptOutcome::Accepted {
            did: alice_did,
            replaced: false,
        }
    );
    assert_eq!(
        accept_envelope(
            &mut carol_store,
            &CardEnvelope {
                hops: 2,
                card_wire: v1_wire,
            },
            now + 10,
            500,
            None,
            &|_| false
        ),
        AcceptOutcome::IgnoredStale
    );
}

#[test]
fn test_subaccount_card_preserves_recipient_did_for_market_and_board() {
    let now = 1_700_000_000;
    let anna_sk = create_keypair();
    let anna_root_did = create_user_id(&anna_sk.verifying_key(), None).unwrap();
    let anna_shop_did = create_user_id(&anna_sk.verifying_key(), Some("laden")).unwrap();

    let card = make_named_ad_card(
        &anna_sk,
        &anna_shop_did,
        100,
        now + 30 * 86_400,
        2,
        2,
        "Anna Laden",
        make_test_ad("0000000000000099", "Bio Brot", now - 5, now + 86_400),
        vec![],
    );

    assert_eq!(card.payload.did, anna_root_did);
    assert_eq!(
        card.payload.recipient_did.as_deref(),
        Some(anna_shop_did.as_str())
    );
    assert_eq!(card.payload.recipient_id(), anna_shop_did);

    let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
    let verified = verify_sd_card(&wire).unwrap();
    assert_eq!(verified.payload.did, anna_root_did);
    assert_eq!(verified.payload.recipient_id(), anna_shop_did);

    let stored = StoredCard {
        card_wire: wire,
        card: verified,
        hops: 1,
        received_at: now,
    };
    let persons = build_market_view(
        &[],
        &[stored],
        "did:key:bernd",
        &|_| TrustTier::Trusted,
        now,
    );
    assert_eq!(persons.len(), 1);
    assert_eq!(persons[0].did, anna_root_did);
    assert_eq!(persons[0].recipient_did, anna_shop_did);

    let board = build_ad_board(&persons);
    assert_eq!(board.len(), 1);
    assert_eq!(board[0].owner_did, anna_root_did);
    assert_eq!(board[0].recipient_did, anna_shop_did);
}

#[test]
fn test_multi_prefix_cards_coexist_with_shared_trust_and_offender_hiding() {
    let now = 1_700_000_000;
    let sk = create_keypair();
    let vk = sk.verifying_key();
    let root_did = create_user_id(&vk, None).unwrap();
    let shop_did = create_user_id(&vk, Some("laden")).unwrap();
    let priv_did = create_user_id(&vk, Some("privat")).unwrap();

    let mut store = CardStore::default();
    for (idx, (did, name, ad_title, url)) in [
        (
            &root_did,
            "Anna Basis",
            "Honig",
            "https://basis.example.com",
        ),
        (&shop_did, "Anna Laden", "Brot", "https://laden.example.com"),
        (
            &priv_did,
            "Anna Privat",
            "Fahrrad",
            "https://privat.example.com",
        ),
    ]
    .iter()
    .enumerate()
    {
        let card = make_named_ad_card(
            &sk,
            did,
            100 + idx as u64,
            now + 30 * 86_400,
            2,
            2,
            name,
            make_test_ad(
                &format!("000000000000001{}", idx),
                ad_title,
                now - 5,
                now + 86_400,
            ),
            vec![DropDescriptor {
                write_url: (*url).to_string(),
                read_url: (*url).to_string(),
                slot_id: None,
                drop_pub: hex::encode([9u8; 32]),
            }],
        );
        let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
        let res = accept_envelope(
            &mut store,
            &CardEnvelope {
                card_wire: wire,
                hops: 1,
            },
            now,
            500,
            None,
            &|_| false,
        );
        assert_eq!(
            res,
            AcceptOutcome::Accepted {
                did: (*did).clone(),
                replaced: false,
            }
        );
    }

    assert_eq!(store.cards.len(), 3);
    let stored_vec: Vec<StoredCard> = store.cards.values().cloned().collect();
    let persons = build_market_view(
        &[],
        &stored_vec,
        "did:key:other",
        &|_| TrustTier::Trusted,
        now,
    );
    assert_eq!(persons.len(), 3);
    assert!(persons
        .iter()
        .all(|p| p.did == root_did && p.trust == TrustTier::Trusted));
    assert_eq!(
        persons
            .iter()
            .find(|p| p.recipient_did == shop_did)
            .unwrap()
            .card_drops[0]
            .read_url,
        "https://laden.example.com"
    );
    assert_eq!(
        persons
            .iter()
            .find(|p| p.recipient_did == priv_did)
            .unwrap()
            .card_drops[0]
            .read_url,
        "https://privat.example.com"
    );

    let board = build_ad_board(&persons);
    assert_eq!(board.len(), 3);

    // KnownOffender (TrustTier::Hidden on canonical root DID) hides ALL 3 profiles
    let hidden_persons = build_market_view(
        &[],
        &stored_vec,
        "did:key:other",
        &|d| {
            if d == root_did {
                TrustTier::Hidden
            } else {
                TrustTier::Trusted
            }
        },
        now,
    );
    assert!(hidden_persons.is_empty());
}

#[test]
fn test_ad_board_owner_name_formatting_and_id_tiebreaker() {
    let make_person = |suffix: &str, profile: PublicProfile, ads: Vec<Ad>| MarketPerson {
        did: format!("did:key:z6MkPerson{suffix}"),
        recipient_did: format!("did:key:z6MkPerson{suffix}"),
        profile,
        skills: vec![],
        active_ads: ads,
        ads_pending: 0,
        allow_inquiries: true,
        sources: vec![EntrySource::Voucher],
        as_of: 100,
        standards: vec![],
        trust: TrustTier::Trusted,
        can_forward: false,
        card_drops: vec![],
    };

    let people = vec![
        make_person(
            "Full",
            PublicProfile {
                first_name: Some("Alice".into()),
                last_name: Some("Meyer".into()),
                organization: Some("IgnoredOrg".into()),
                ..Default::default()
            },
            vec![make_test_ad("000000000000000b", "B", 100, 200)],
        ),
        make_person(
            "First",
            PublicProfile {
                first_name: Some("Bob".into()),
                last_name: Some("   ".into()),
                ..Default::default()
            },
            vec![make_test_ad("000000000000000a", "A", 100, 200)],
        ),
        make_person(
            "Last",
            PublicProfile {
                last_name: Some("Schmidt".into()),
                ..Default::default()
            },
            vec![make_test_ad("000000000000000c", "C", 90, 200)],
        ),
        make_person(
            "Org",
            PublicProfile {
                organization: Some("BioHof".into()),
                community: Some("IgnoredCom".into()),
                ..Default::default()
            },
            vec![make_test_ad("000000000000000d", "D", 80, 200)],
        ),
        make_person(
            "Com",
            PublicProfile {
                community: Some("NordKiez".into()),
                ..Default::default()
            },
            vec![make_test_ad("000000000000000e", "E", 70, 200)],
        ),
        make_person(
            "Did",
            PublicProfile::default(),
            vec![make_test_ad("000000000000000f", "F", 60, 200)],
        ),
    ];

    let board = build_ad_board(&people);
    assert_eq!(board.len(), 6);
    // Equal published_at=100 breaks ties by ad.id ascending ("...0a" before "...0b")
    assert_eq!(board[0].ad.id, "000000000000000a");
    assert_eq!(board[0].owner_name, "Bob");
    assert_eq!(board[1].ad.id, "000000000000000b");
    assert_eq!(board[1].owner_name, "Alice Meyer");
    assert_eq!(board[2].owner_name, "Schmidt");
    assert_eq!(board[3].owner_name, "BioHof");
    assert_eq!(board[4].owner_name, "NordKiez");
    assert_eq!(board[5].owner_name, "did:key:z6MkPersonDid");
}
