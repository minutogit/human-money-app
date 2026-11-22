use ed25519_dalek::SigningKey;
use human_money_core::models::profile::PublicProfile;
use human_money_core::models::voucher::{Voucher, VoucherStandard};
use human_money_core::services::crypto::create_user_id;
use humoco_market::{
    accept_envelope, build_ad_board, build_market_view, new_ad_id, render_wire, sign_sd_card,
    AcceptOutcome, Ad, AdKind, CardEnvelope, CardItem, CardPayloadInput, CardStore, DropDescriptor,
    EntrySource, Skill, SkillOrigin, StoredCard, TrustTier, WireFilter, MAX_SKILL_TEXT_CHARS,
    MAX_SKILL_TITLE_CHARS,
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

#[test]
fn test_empty_redacted_card_hides_person_and_ads_and_blocks_stale_zombie() {
    use humoco_market::{
        accept_envelope, forward_envelope, verify_sd_card, AcceptOutcome, CardEnvelope, CardStore,
    };

    let now = 1_700_000_000;
    let alice_sk = create_keypair();
    let alice_did = create_user_id(&alice_sk.verifying_key(), None).unwrap();

    // v1 published at max_hops = 3 with Name + Ad
    let card_v1 = sign_sd_card(
        CardPayloadInput {
            did: alice_did.clone(),
            v: 100,
            valid_until: now + 30 * 86_400,
            max_hops: 3,
            ad_count: 1,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![
            (
                CardItem::Name {
                    first_name: Some("Alice".to_string()),
                    last_name: None,
                },
                3,
            ),
            (
                CardItem::Ad(Ad {
                    id: "0000000000000011".to_string(),
                    kind: AdKind::Offer,
                    title: "Fresh Honey".to_string(),
                    text: None,
                    pricing: None,
                    published_at: now - 10,
                    renewed_at: None,
                    expires_at: now + 86_400,
                    allow_inquiries: true,
                    is_remote: false,
                    image_token: None,
                    event_at: None,
                    event_all_day: false,
                }),
                3,
            ),
        ],
        &alice_sk,
    )
    .unwrap();

    // Bob (Hop 1) and Carol (Hop 2) receive v1
    let mut carol_store = CardStore::default();
    let v1_hop2_wire = render_wire(&card_v1, 2, WireFilter::Full).unwrap();
    assert!(matches!(
        accept_envelope(
            &mut carol_store,
            &CardEnvelope {
                hops: 2,
                card_wire: v1_hop2_wire.clone(),
            },
            now,
            500,
            None,
            &|_| false
        ),
        AcceptOutcome::Accepted { .. }
    ));

    let carol_cards_v1: Vec<StoredCard> = carol_store.cards.values().cloned().collect();
    let carol_view_v1 = build_market_view(
        &[],
        &carol_cards_v1,
        "did:key:carol",
        &|_| TrustTier::Trusted,
        now,
    );
    assert_eq!(carol_view_v1.len(), 1);
    assert_eq!(build_ad_board(&carol_view_v1).len(), 1);

    // v2 shrinks items to Hop 1 while keeping envelope max_hops = 3 (shrink_hops)
    let card_v2 = sign_sd_card(
        CardPayloadInput {
            did: alice_did.clone(),
            v: 200,
            valid_until: now + 30 * 86_400,
            max_hops: 3,
            ad_count: 1,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![
            (
                CardItem::Name {
                    first_name: Some("Alice".to_string()),
                    last_name: None,
                },
                1,
            ),
            (
                CardItem::Ad(Ad {
                    id: "0000000000000011".to_string(),
                    kind: AdKind::Offer,
                    title: "Fresh Honey".to_string(),
                    text: None,
                    pricing: None,
                    published_at: now - 10,
                    renewed_at: None,
                    expires_at: now + 86_400,
                    allow_inquiries: true,
                    is_remote: false,
                    image_token: None,
                    event_at: None,
                    event_all_day: false,
                }),
                1,
            ),
        ],
        &alice_sk,
    )
    .unwrap();

    // Hop 1 forwards v2 to Hop 2 -> produces bare `<jws>~`
    let hop1_stored = StoredCard {
        card_wire: render_wire(&card_v2, 1, WireFilter::Full).unwrap(),
        card: verify_sd_card(&render_wire(&card_v2, 1, WireFilter::Full).unwrap()).unwrap(),
        hops: 1,
        received_at: now,
    };
    let fwd_to_carol = forward_envelope(&hop1_stored).unwrap();
    assert!(fwd_to_carol.card_wire.ends_with('~'));

    assert!(matches!(
        accept_envelope(&mut carol_store, &fwd_to_carol, now + 5, 500, None, &|_| {
            false
        }),
        AcceptOutcome::Accepted { replaced: true, .. }
    ));

    // Carol still stores the empty shell (to forward to Dave & block v1 zombies),
    // but MarketView & AdBoard show NOTHING!
    assert_eq!(carol_store.cards.len(), 1);
    let carol_cards_v2: Vec<StoredCard> = carol_store.cards.values().cloned().collect();
    let carol_view_v2 = build_market_view(
        &[],
        &carol_cards_v2,
        "did:key:carol",
        &|_| TrustTier::Trusted,
        now + 5,
    );
    assert!(carol_view_v2.is_empty());
    assert!(build_ad_board(&carol_view_v2).is_empty());

    // Delayed v1 replay MUST be ignored as stale!
    assert!(matches!(
        accept_envelope(
            &mut carol_store,
            &CardEnvelope {
                hops: 2,
                card_wire: v1_hop2_wire,
            },
            now + 10,
            500,
            None,
            &|_| false
        ),
        AcceptOutcome::IgnoredStale
    ));
}

#[test]
fn test_subaccount_card_preserves_recipient_did_for_market_and_board() {
    let now = 1_700_000_000;
    let anna_sk = create_keypair();
    let anna_root_did = create_user_id(&anna_sk.verifying_key(), None).unwrap();
    let anna_shop_did = create_user_id(&anna_sk.verifying_key(), Some("laden")).unwrap();

    let card = sign_sd_card(
        CardPayloadInput {
            did: anna_shop_did.clone(),
            v: 100,
            valid_until: now + 30 * 86_400,
            max_hops: 2,
            ad_count: 1,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![
            (
                CardItem::Name {
                    first_name: Some("Anna Laden".to_string()),
                    last_name: None,
                },
                2,
            ),
            (
                CardItem::Ad(Ad {
                    id: "0000000000000099".to_string(),
                    kind: AdKind::Offer,
                    title: "Bio Brot".to_string(),
                    text: None,
                    pricing: None,
                    published_at: now - 5,
                    renewed_at: None,
                    expires_at: now + 86_400,
                    allow_inquiries: true,
                    is_remote: false,
                    image_token: None,
                    event_at: None,
                    event_all_day: false,
                }),
                2,
            ),
        ],
        &anna_sk,
    )
    .unwrap();

    assert_eq!(card.payload.did, anna_root_did);
    assert_eq!(
        card.payload.recipient_did.as_deref(),
        Some(anna_shop_did.as_str())
    );
    assert_eq!(card.payload.recipient_id(), anna_shop_did);

    let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
    let verified = humoco_market::verify_sd_card(&wire).unwrap();
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
        let card = sign_sd_card(
            CardPayloadInput {
                did: (*did).clone(),
                v: 100 + idx as u64,
                valid_until: now + 30 * 86_400,
                max_hops: 2,
                ad_count: 1,
                allow_inquiries: true,
                drops: vec![DropDescriptor {
                    write_url: (*url).to_string(),
                    read_url: (*url).to_string(),
                    slot_id: None,
                    drop_pub: hex::encode([9u8; 32]),
                }],
            },
            vec![
                (
                    CardItem::Name {
                        first_name: Some((*name).to_string()),
                        last_name: None,
                    },
                    2,
                ),
                (
                    CardItem::Ad(Ad {
                        id: format!("000000000000001{}", idx),
                        kind: AdKind::Offer,
                        title: (*ad_title).to_string(),
                        text: None,
                        pricing: None,
                        published_at: now - 5,
                        renewed_at: None,
                        expires_at: now + 86_400,
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
