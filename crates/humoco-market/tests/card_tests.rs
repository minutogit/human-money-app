use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use humoco_market::{
    accept_envelope, derive_effective_hops, encode_qr_svg, haversine_km, mark_sent, merge,
    new_ad_id, next_card_version, parse_coordinates_pair, render_wire, should_attach_card,
    sign_sd_card, verify_sd_card, AcceptOutcome, Ad, AdKind, CardEnvelope, CardItem,
    CardPayloadInput, CardStore, DropDescriptor, Skill, WireFilter, CARD_SD_PADDED_LEN,
    MAX_BUNDLE_CARD_WIRE_BYTES, MAX_PRICING_CHARS, MAX_QR_SHARE_BYTES, MAX_SKILL_TEXT_CHARS,
    MAX_SKILL_TITLE_CHARS,
};
use proptest::prelude::*;

fn make_key(seed: u8) -> SigningKey {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    SigningKey::from_bytes(&bytes)
}

fn valid_payload_input(sk: &SigningKey) -> CardPayloadInput {
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    CardPayloadInput {
        did,
        v: 100,
        valid_until: 1800000000,
        max_hops: 2,
        ad_count: 1,
        allow_inquiries: true,
        drops: vec![],
    }
}

fn sample_items() -> Vec<(CardItem, u8)> {
    vec![
        (
            CardItem::Name {
                first_name: Some("Alice".to_string()),
                last_name: Some("Wonder".to_string()),
            },
            2,
        ),
        (
            CardItem::City {
                zip_code: Some("10115".to_string()),
                city: Some("Berlin".to_string()),
                country: Some("DE".to_string()),
            },
            2,
        ),
        (
            CardItem::Phone("+4912345678".to_string()),
            1, // Contacts only!
        ),
        (
            CardItem::Skill(Skill {
                title: "Bread Baking".to_string(),
                text: Some("Sourdough bread once a week".to_string()),
                pricing: Some("1 Minuto / loaf".to_string()),
                is_remote: false,
            }),
            2,
        ),
        (
            CardItem::Ad(Ad {
                id: new_ad_id(),
                kind: AdKind::Offer,
                title: "Fresh Sourdough Bread".to_string(),
                text: Some("Ready for pickup this Friday afternoon".to_string()),
                pricing: Some("1 Minuto".to_string()),
                published_at: 1700000000,
                renewed_at: None,
                expires_at: 1700000000 + 30 * 86_400,
                allow_inquiries: true,
                is_remote: false,
                image_token: None,
            }),
            2,
        ),
    ]
}

#[test]
fn test_sd_card_signing_and_verification_roundtrip() {
    let sk = make_key(1);
    let payload_input = valid_payload_input(&sk);
    let items = sample_items();

    let signed = sign_sd_card(payload_input, items.clone(), &sk).unwrap();
    assert_eq!(signed.payload._sd.len(), CARD_SD_PADDED_LEN);

    let wire = render_wire(&signed, 1, WireFilter::Full).unwrap();
    let verified = verify_sd_card(&wire).unwrap();

    assert_eq!(verified.payload.did, signed.payload.did);
    assert_eq!(verified.payload.v, 100);
    assert_eq!(verified.payload._sd, signed.payload._sd);
    assert_eq!(verified.disclosures.len(), items.len());
}

#[test]
fn test_sd_card_manipulation_rejection() {
    let sk = make_key(2);
    let payload_input = valid_payload_input(&sk);
    let items = sample_items();
    let signed = sign_sd_card(payload_input, items, &sk).unwrap();
    let wire = render_wire(&signed, 1, WireFilter::Full).unwrap();

    // 1. Modified disclosure token (corrupted Base64) -> verify fails
    let mut parts: Vec<&str> = wire.split('~').collect();
    let corrupted = format!("{}XYZ", parts[1]);
    parts[1] = &corrupted;
    let corrupted_wire = parts.join("~");
    assert!(verify_sd_card(&corrupted_wire).is_err());

    // 2. Foreign disclosure token (from another card, not in _sd) -> verify fails
    let foreign_sk = make_key(99);
    let foreign_card = sign_sd_card(valid_payload_input(&foreign_sk), sample_items(), &foreign_sk).unwrap();
    let foreign_wire = render_wire(&foreign_card, 1, WireFilter::Full).unwrap();
    let foreign_disc = foreign_wire.split('~').nth(1).unwrap();

    let mut parts2: Vec<&str> = wire.split('~').collect();
    parts2.insert(1, foreign_disc);
    let foreign_injected_wire = parts2.join("~");
    assert!(verify_sd_card(&foreign_injected_wire).is_err());

    // 3. Removed disclosure token -> MUST remain valid! (Selective Disclosure)
    let parts3: Vec<&str> = wire.split('~').collect();
    let mut omitted_parts = vec![parts3[0]]; // keep JWS
    omitted_parts.extend_from_slice(&parts3[2..]); // omit disclosure 1
    let omitted_wire = omitted_parts.join("~");
    let verified_omitted = verify_sd_card(&omitted_wire).unwrap();
    assert_eq!(verified_omitted.disclosures.len(), signed.disclosures.len() - 1);
}

#[test]
fn test_render_wire_per_distance() {
    let sk = make_key(3);
    let payload_input = valid_payload_input(&sk);
    let items = sample_items(); // Contains Phone with max_distance = 1, others with 2
    let card = sign_sd_card(payload_input, items, &sk).unwrap();

    // Distance 1: All disclosures included (including Phone)
    let wire_dist1 = render_wire(&card, 1, WireFilter::Full).unwrap();
    let verified_dist1 = verify_sd_card(&wire_dist1).unwrap();
    let items_dist1 = humoco_market::sd_card::decoded_items(&verified_dist1);
    assert!(items_dist1.iter().any(|(item, _)| matches!(item, CardItem::Phone(_))));

    // Distance 2: Phone (max_distance = 1) is omitted!
    let wire_dist2 = render_wire(&card, 2, WireFilter::Full).unwrap();
    let verified_dist2 = verify_sd_card(&wire_dist2).unwrap();
    let items_dist2 = humoco_market::sd_card::decoded_items(&verified_dist2);
    assert!(!items_dist2.iter().any(|(item, _)| matches!(item, CardItem::Phone(_))));
    assert_eq!(items_dist2.len(), items_dist1.len() - 1);

    // Distance 3 (> max_hops 2): Returns None
    assert!(render_wire(&card, 3, WireFilter::Full).is_none());
}

#[test]
fn test_fit_bytes_budget_2900() {
    let sk = make_key(4);
    let mut payload_input = valid_payload_input(&sk);
    let valid_vk_hex = hex::encode(make_key(50).verifying_key().as_bytes());
    payload_input.drops = vec![
        DropDescriptor {
            write_url: "https://drop1.example.com/write".to_string(),
            read_url: "https://drop1.example.com/read".to_string(),
            slot_id: None,
            drop_pub: valid_vk_hex.clone(),
        },
        DropDescriptor {
            write_url: "https://drop2.example.com/write".to_string(),
            read_url: "https://drop2.example.com/read".to_string(),
            slot_id: None,
            drop_pub: valid_vk_hex.clone(),
        },
        DropDescriptor {
            write_url: "https://drop3.example.com/write".to_string(),
            read_url: "https://drop3.example.com/read".to_string(),
            slot_id: None,
            drop_pub: valid_vk_hex,
        },
    ];

    let items = sample_items();
    let card = sign_sd_card(payload_input, items, &sk).unwrap();

    let qr_wire = render_wire(&card, 1, WireFilter::FitBytes(MAX_QR_SHARE_BYTES)).unwrap();
    assert!(qr_wire.len() <= MAX_QR_SHARE_BYTES);

    let verified = verify_sd_card(&qr_wire).unwrap();
    let decoded = humoco_market::sd_card::decoded_items(&verified);
    // Name and City are prioritized and must be present
    assert!(decoded.iter().any(|(item, _)| matches!(item, CardItem::Name { .. })));
    assert!(decoded.iter().any(|(item, _)| matches!(item, CardItem::City { .. })));
}

#[test]
fn test_no_ads_wire_filter_fits_7168_with_8_skills() {
    let sk = make_key(5);
    let payload_input = valid_payload_input(&sk);

    let mut items = vec![
        (
            CardItem::Name {
                first_name: Some("A".repeat(80)),
                last_name: Some("B".repeat(80)),
            },
            2,
        ),
        (
            CardItem::City {
                zip_code: Some("12345".to_string()),
                city: Some("CityName".repeat(5)),
                country: Some("DE".to_string()),
            },
            2,
        ),
    ];

    for _i in 0..8 {
        items.push((
            CardItem::Skill(Skill {
                title: format!("Skill {}", "S".repeat(MAX_SKILL_TITLE_CHARS - 10)),
                text: Some("T".repeat(MAX_SKILL_TEXT_CHARS)),
                pricing: Some("P".repeat(MAX_PRICING_CHARS)),
                is_remote: false,
            }),
            2,
        ));
    }

    // Add an Ad as well (should be omitted by NoAds)
    items.push((
        CardItem::Ad(Ad {
            id: new_ad_id(),
            kind: AdKind::Offer,
            title: "Omitted Ad".to_string(),
            text: Some("Text".to_string()),
            pricing: None,
            published_at: 1700000000,
            renewed_at: None,
            expires_at: 1700000000 + 30 * 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
        }),
        2,
    ));

    let card = sign_sd_card(payload_input, items, &sk).unwrap();
    let no_ads_wire = render_wire(&card, 1, WireFilter::NoAds).unwrap();
    assert!(no_ads_wire.len() <= MAX_BUNDLE_CARD_WIRE_BYTES);

    let verified = verify_sd_card(&no_ads_wire).unwrap();
    let decoded = humoco_market::sd_card::decoded_items(&verified);
    assert!(!decoded.iter().any(|(item, _)| matches!(item, CardItem::Ad(_))));
    assert_eq!(
        decoded.iter().filter(|(item, _)| matches!(item, CardItem::Skill(_))).count(),
        8
    );
}

#[test]
fn test_merge_two_partial_wires() {
    let sk = make_key(6);
    let payload_input = valid_payload_input(&sk);
    let items = sample_items();
    let card = sign_sd_card(payload_input, items, &sk).unwrap();

    let all_discs: Vec<(&String, &String)> = card.disclosures.iter().collect();
    assert!(all_discs.len() >= 4);

    // Wire 1: contains disclosures 0 and 1
    let wire_1 = format!(
        "{}~{}~{}~",
        card.jws, all_discs[0].1, all_discs[1].1
    );
    let mut card_1 = verify_sd_card(&wire_1).unwrap();
    assert_eq!(card_1.disclosures.len(), 2);

    // Wire 2: contains disclosures 2 and 3
    let wire_2 = format!(
        "{}~{}~{}~",
        card.jws, all_discs[2].1, all_discs[3].1
    );
    let card_2 = verify_sd_card(&wire_2).unwrap();
    assert_eq!(card_2.disclosures.len(), 2);

    // Merge wire 2 into wire 1
    let merged = merge(&mut card_1, card_2);
    assert!(merged);
    assert_eq!(card_1.disclosures.len(), 4);
}

#[test]
fn test_accept_envelope_all_cases() {
    let sk = make_key(7);
    let mut store = CardStore::default();
    let no_offender = |_did: &str| false;

    // Base card v = 100
    let mut payload = valid_payload_input(&sk);
    payload.v = 100;
    let card_v100 = sign_sd_card(payload.clone(), sample_items(), &sk).unwrap();
    let wire_v100 = render_wire(&card_v100, 1, WireFilter::Full).unwrap();

    // 1. Initial insert
    let env_v100 = CardEnvelope {
        card_wire: wire_v100.clone(),
        hops: 1,
    };
    let out = accept_envelope(&mut store, &env_v100, 1700000000, 2000, None, &no_offender);
    assert_eq!(
        out,
        AcceptOutcome::Accepted {
            did: card_v100.payload.did.clone(),
            replaced: false,
        }
    );

    // 2. Case 1: new.v > stored.v (v = 200) -> replaced = true
    payload.v = 200;
    let card_v200 = sign_sd_card(payload.clone(), sample_items(), &sk).unwrap();
    let wire_v200 = render_wire(&card_v200, 1, WireFilter::Full).unwrap();
    let env_v200 = CardEnvelope {
        card_wire: wire_v200,
        hops: 1,
    };
    let out2 = accept_envelope(&mut store, &env_v200, 1700000000, 2000, None, &no_offender);
    assert_eq!(
        out2,
        AcceptOutcome::Accepted {
            did: card_v200.payload.did.clone(),
            replaced: true,
        }
    );

    // 3. Case 2: new.v == stored.v && new.jws == stored.jws -> merge
    let out3 = accept_envelope(&mut store, &env_v200, 1700000000, 2000, None, &no_offender);
    assert_eq!(
        out3,
        AcceptOutcome::Accepted {
            did: card_v200.payload.did.clone(),
            replaced: false,
        }
    );

    // 4. Case 4: new.v < stored.v (v = 100 < 200) -> IgnoredStale
    let out4 = accept_envelope(&mut store, &env_v100, 1700000000, 2000, None, &no_offender);
    assert_eq!(out4, AcceptOutcome::IgnoredStale);
}

#[test]
fn test_receiver_rule_strip_contacts_at_hop2() {
    let sk = make_key(8);
    let mut store = CardStore::default();
    let no_offender = |_did: &str| false;

    let payload = valid_payload_input(&sk);
    let items = sample_items(); // Phone has max_distance = 1
    let card = sign_sd_card(payload, items, &sk).unwrap();

    // A malicious / buggy app forwards with ALL disclosures at hops = 2
    let full_wire = render_wire(&card, 1, WireFilter::Full).unwrap();
    let env = CardEnvelope {
        card_wire: full_wire,
        hops: 2,
    };

    let outcome = accept_envelope(&mut store, &env, 1700000000, 2000, None, &no_offender);
    assert!(matches!(outcome, AcceptOutcome::Accepted { .. }));

    // Honest app stripped Phone because max_distance (1) < hops (2)
    let pk_bytes = *sk.verifying_key().as_bytes();
    let stored = store.cards.get(&pk_bytes).unwrap();
    let decoded = humoco_market::sd_card::decoded_items(&stored.card);
    assert!(!decoded.iter().any(|(item, _)| matches!(item, CardItem::Phone(_))));
}

#[test]
fn test_distance_derivation_from_slot_writer() {
    let card_did = "did_author";
    let foreign_did = "did_relay";

    // Direct from owner -> 1
    assert_eq!(derive_effective_hops(2, card_did, Some(card_did)), 1);

    // From foreign slot writer -> env.hops.max(2)
    assert_eq!(derive_effective_hops(1, card_did, Some(foreign_did)), 2);
    assert_eq!(derive_effective_hops(3, card_did, Some(foreign_did)), 3);

    // No slot writer (direct file / QR) -> env.hops unchanged
    assert_eq!(derive_effective_hops(1, card_did, None), 1);
    assert_eq!(derive_effective_hops(2, card_did, None), 2);
}

#[test]
fn test_limits_skills_and_ads() {
    let sk = make_key(9);
    let payload = valid_payload_input(&sk);

    // 8 skills ok
    let mut items: Vec<(CardItem, u8)> = (0..8)
        .map(|i| {
            (
                CardItem::Skill(Skill {
                    title: format!("Skill {}", i),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            )
        })
        .collect();
    assert!(sign_sd_card(payload.clone(), items.clone(), &sk).is_ok());

    // 9 skills err
    items.push((
        CardItem::Skill(Skill {
            title: "Skill 9".to_string(),
            text: None,
            pricing: None,
            is_remote: false,
        }),
        2,
    ));
    assert!(sign_sd_card(payload.clone(), items, &sk).is_err());

    // 5 ads ok
    let mut ad_items: Vec<(CardItem, u8)> = (0..5)
        .map(|i| {
            (
                CardItem::Ad(Ad {
                    id: format!("{:016x}", i + 1),
                    kind: AdKind::Offer,
                    title: format!("Ad {}", i),
                    text: None,
                    pricing: None,
                    published_at: 1700000000,
                    renewed_at: None,
                    expires_at: 1700000000 + 30 * 86_400,
                    allow_inquiries: false,
                    is_remote: false,
                    image_token: None,
                }),
                2,
            )
        })
        .collect();
    assert!(sign_sd_card(payload.clone(), ad_items.clone(), &sk).is_ok());

    // 6 ads err
    ad_items.push((
        CardItem::Ad(Ad {
            id: format!("{:016x}", 6),
            kind: AdKind::Offer,
            title: "Ad 6".to_string(),
            text: None,
            pricing: None,
            published_at: 1700000000,
            renewed_at: None,
            expires_at: 1700000000 + 30 * 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
        }),
        2,
    ));
    assert!(sign_sd_card(payload, ad_items, &sk).is_err());
}

#[test]
fn test_ad_duration_and_validity_invariants() {
    let sk = make_key(10);
    let mut payload = valid_payload_input(&sk);
    payload.valid_until = 1700000000 + 100 * 86_400;

    // Ad duration > 180 days -> err
    let bad_duration_ad = (
        CardItem::Ad(Ad {
            id: new_ad_id(),
            kind: AdKind::Offer,
            title: "Long Ad".to_string(),
            text: None,
            pricing: None,
            published_at: 1700000000,
            renewed_at: None,
            expires_at: 1700000000 + 181 * 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
        }),
        2,
    );
    assert!(sign_sd_card(payload.clone(), vec![bad_duration_ad], &sk).is_err());

    // Ad expires_at > valid_until -> err
    let beyond_valid_ad = (
        CardItem::Ad(Ad {
            id: new_ad_id(),
            kind: AdKind::Offer,
            title: "Beyond Valid".to_string(),
            text: None,
            pricing: None,
            published_at: 1700000000,
            renewed_at: None,
            expires_at: 1700000000 + 101 * 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
        }),
        2,
    );
    assert!(sign_sd_card(payload.clone(), vec![beyond_valid_ad], &sk).is_err());

    // Duplicate Ad ID on same card -> err
    let duplicate_id = new_ad_id();
    let ad1 = (
        CardItem::Ad(Ad {
            id: duplicate_id.clone(),
            kind: AdKind::Offer,
            title: "Ad 1".to_string(),
            text: None,
            pricing: None,
            published_at: 1700000000,
            renewed_at: None,
            expires_at: 1700000000 + 30 * 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
        }),
        2,
    );
    let ad2 = (
        CardItem::Ad(Ad {
            id: duplicate_id,
            kind: AdKind::Request,
            title: "Ad 2".to_string(),
            text: None,
            pricing: None,
            published_at: 1700000000,
            renewed_at: None,
            expires_at: 1700000000 + 30 * 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
        }),
        2,
    );
    assert!(sign_sd_card(payload.clone(), vec![ad1, ad2], &sk).is_err());

    // Expired ad (expires_at <= now) is NOT a validation error on verify
    let expired_ad = (
        CardItem::Ad(Ad {
            id: new_ad_id(),
            kind: AdKind::Offer,
            title: "Expired Ad".to_string(),
            text: None,
            pricing: None,
            published_at: 1600000000,
            renewed_at: None,
            expires_at: 1600000000 + 30 * 86_400,
            allow_inquiries: false,
            is_remote: false,
            image_token: None,
        }),
        2,
    );
    let expired_card = sign_sd_card(payload, vec![expired_ad], &sk).unwrap();
    let wire = render_wire(&expired_card, 1, WireFilter::Full).unwrap();
    assert!(verify_sd_card(&wire).is_ok());
}

#[test]
fn test_decoy_padding_always_32() {
    let sk = make_key(11);
    let payload = valid_payload_input(&sk);

    // 0 items
    let card0 = sign_sd_card(payload.clone(), vec![], &sk).unwrap();
    assert_eq!(card0.payload._sd.len(), CARD_SD_PADDED_LEN);

    // 5 items
    let card5 = sign_sd_card(payload, sample_items(), &sk).unwrap();
    assert_eq!(card5.payload._sd.len(), CARD_SD_PADDED_LEN);

    // Digests unique and sorted
    let mut set = BTreeSet::new();
    for d in &card5.payload._sd {
        assert!(set.insert(d.clone()));
    }
    assert_eq!(set.len(), 32);
    assert!(card5.payload._sd.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn test_sent_log_and_versioning() {
    let mut store = CardStore::default();
    let peer_key = [42u8; 32];

    assert!(should_attach_card(&store, &peer_key, 1));
    mark_sent(&mut store, peer_key, 100);

    assert!(!should_attach_card(&store, &peer_key, 50));
    assert!(!should_attach_card(&store, &peer_key, 100));
    assert!(should_attach_card(&store, &peer_key, 101));

    assert_eq!(next_card_version(None, 500), 500);
    assert_eq!(next_card_version(Some(100), 50), 101);
    assert_eq!(next_card_version(Some(100), 200), 200);
}

#[test]
fn test_qr_svg_generation() {
    let sk = make_key(12);
    let card = sign_sd_card(valid_payload_input(&sk), sample_items(), &sk).unwrap();
    let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
    let uri = format!("humoco://card?c={}", wire);

    let svg = encode_qr_svg(uri.as_bytes());
    assert!(svg.is_ok());
    let svg_str = svg.unwrap();
    assert!(svg_str.starts_with("<svg"));
    assert!(svg_str.ends_with("</svg>"));
}

#[test]
fn test_parse_coordinates_pair() {
    assert_eq!(
        parse_coordinates_pair("52.5200, 13.4050"),
        Some((52.52, 13.405))
    );
    assert_eq!(
        parse_coordinates_pair("52.52; 13.4050"),
        Some((52.52, 13.405))
    );
    assert_eq!(
        parse_coordinates_pair("52.52 13.4050"),
        Some((52.52, 13.405))
    );
    assert_eq!(
        parse_coordinates_pair(" -12.34 , 56.78 "),
        Some((-12.34, 56.78))
    );
    assert_eq!(parse_coordinates_pair("91.0, 10.0"), None);
    assert_eq!(parse_coordinates_pair("-90.1, 10.0"), None);
    assert_eq!(parse_coordinates_pair("10.0, 180.1"), None);
    assert_eq!(parse_coordinates_pair("10.0, -180.1"), None);
    assert_eq!(parse_coordinates_pair("invalid, coords"), None);
    assert_eq!(parse_coordinates_pair("52.52"), None);
    assert_eq!(parse_coordinates_pair(""), None);
    assert_eq!(parse_coordinates_pair("   "), None);
}

#[test]
fn test_haversine_km() {
    let p1 = (52.52, 13.405); // Berlin
    let p2 = (48.137, 11.576); // Munich
    let dist = haversine_km(p1, p2);
    assert!((dist - 504.0).abs() < 5.0, "expected ~504 km, got {}", dist);
    assert_eq!(haversine_km(p1, p1), 0.0);
}

#[test]
fn test_geo_eviction_remote_protection() {
    let mut store = CardStore::default();
    let no_offender = |_did: &str| false;
    let own_coords = Some((52.52, 13.405)); // Berlin

    // Card A: Local skill, Berlin
    let sk_a = make_key(30);
    let card_a = sign_sd_card(
        valid_payload_input(&sk_a),
        vec![
            (CardItem::Coordinates("52.52, 13.405".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Local Berlin Bakery".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
        ],
        &sk_a,
    )
    .unwrap();
    let wire_a = render_wire(&card_a, 2, WireFilter::Full).unwrap();

    // Card B: Remote skill, Tokyo (9000 km away)
    let sk_b = make_key(31);
    let card_b = sign_sd_card(
        valid_payload_input(&sk_b),
        vec![
            (CardItem::Coordinates("35.676, 139.65".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Remote Rust Dev".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: true,
                }),
                2,
            ),
        ],
        &sk_b,
    )
    .unwrap();
    let wire_b = render_wire(&card_b, 2, WireFilter::Full).unwrap();

    // Card C: Local skill, Leipzig (~150 km from Berlin)
    let sk_c = make_key(32);
    let card_c = sign_sd_card(
        valid_payload_input(&sk_c),
        vec![
            (CardItem::Coordinates("51.339, 12.373".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Leipzig Pottery".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
        ],
        &sk_c,
    )
    .unwrap();
    let wire_c = render_wire(&card_c, 2, WireFilter::Full).unwrap();

    // Insert A and B with max_stored_cards = 2
    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire_a,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );
    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire_b,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );
    assert_eq!(store.cards.len(), 2);

    // Insert C with max_stored_cards = 2
    // B is protected (remote skill), so between A (dist ~0) and C (dist ~150), C is evicted!
    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire_c,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );

    assert_eq!(store.cards.len(), 2);
    let pk_a = *sk_a.verifying_key().as_bytes();
    let pk_b = *sk_b.verifying_key().as_bytes();
    let pk_c = *sk_c.verifying_key().as_bytes();

    assert!(store.cards.contains_key(&pk_a), "Card A (close local) should be kept");
    assert!(store.cards.contains_key(&pk_b), "Card B (remote protected) should be kept");
    assert!(!store.cards.contains_key(&pk_c), "Card C (farther local) should be evicted");
}

#[test]
fn test_geo_eviction_hop1_protection() {
    let mut store = CardStore::default();
    let no_offender = |_did: &str| false;
    let own_coords = Some((52.52, 13.405)); // Berlin

    // Card 1: Hop 1 (direct contact), local skill, Tokyo coords (far)
    let sk1 = make_key(40);
    let card1 = sign_sd_card(
        valid_payload_input(&sk1),
        vec![
            (CardItem::Coordinates("35.676, 139.65".to_string()), 1),
            (
                CardItem::Skill(Skill {
                    title: "Direct Friend".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                1,
            ),
        ],
        &sk1,
    )
    .unwrap();
    let wire1 = render_wire(&card1, 1, WireFilter::Full).unwrap();

    // Card 2: Hop 2, local skill, Leipzig coords (~150 km)
    let sk2 = make_key(41);
    let card2 = sign_sd_card(
        valid_payload_input(&sk2),
        vec![
            (CardItem::Coordinates("51.339, 12.373".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Leipzig Friend".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
        ],
        &sk2,
    )
    .unwrap();
    let wire2 = render_wire(&card2, 2, WireFilter::Full).unwrap();

    // Card 3: Hop 2, local skill, Potsdam coords (~30 km)
    let sk3 = make_key(42);
    let card3 = sign_sd_card(
        valid_payload_input(&sk3),
        vec![
            (CardItem::Coordinates("52.398, 13.065".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Potsdam Friend".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
        ],
        &sk3,
    )
    .unwrap();
    let wire3 = render_wire(&card3, 2, WireFilter::Full).unwrap();

    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire1,
            hops: 1,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );
    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire2,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );
    assert_eq!(store.cards.len(), 2);

    // Insert Card 3: Card 1 is hop 1 (protected), so Card 2 (hop 2, farther than Card 3) is evicted.
    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire3,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );

    assert_eq!(store.cards.len(), 2);
    let pk1 = *sk1.verifying_key().as_bytes();
    let pk2 = *sk2.verifying_key().as_bytes();
    let pk3 = *sk3.verifying_key().as_bytes();

    assert!(store.cards.contains_key(&pk1), "Card 1 (Hop 1 protected) should be kept");
    assert!(store.cards.contains_key(&pk3), "Card 3 (closer Hop 2) should be kept");
    assert!(!store.cards.contains_key(&pk2), "Card 2 (farther Hop 2) should be evicted");
}

#[test]
fn test_geo_eviction_far_vs_near() {
    let mut store = CardStore::default();
    let no_offender = |_did: &str| false;
    let own_coords = Some((52.52, 13.405)); // Berlin

    // Card 1: Hop 2, Munich (~504 km)
    let sk1 = make_key(50);
    let card1 = sign_sd_card(
        valid_payload_input(&sk1),
        vec![
            (CardItem::Coordinates("48.137, 11.576".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Munich Service".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
        ],
        &sk1,
    )
    .unwrap();
    let wire1 = render_wire(&card1, 2, WireFilter::Full).unwrap();

    // Card 2: Hop 2, Potsdam (~30 km)
    let sk2 = make_key(51);
    let card2 = sign_sd_card(
        valid_payload_input(&sk2),
        vec![
            (CardItem::Coordinates("52.398, 13.065".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Potsdam Service".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
        ],
        &sk2,
    )
    .unwrap();
    let wire2 = render_wire(&card2, 2, WireFilter::Full).unwrap();

    // Card 3: Hop 2, Berlin (~0 km)
    let sk3 = make_key(52);
    let card3 = sign_sd_card(
        valid_payload_input(&sk3),
        vec![
            (CardItem::Coordinates("52.52, 13.405".to_string()), 2),
            (
                CardItem::Skill(Skill {
                    title: "Berlin Service".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: false,
                }),
                2,
            ),
        ],
        &sk3,
    )
    .unwrap();
    let wire3 = render_wire(&card3, 2, WireFilter::Full).unwrap();

    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire1,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );
    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire2,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );
    assert_eq!(store.cards.len(), 2);

    // Insert Card 3 (Berlin): Munich (Card 1, ~504 km) is farthest, evicted!
    accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: wire3,
            hops: 2,
        },
        1700000000,
        2,
        own_coords,
        &no_offender,
    );

    assert_eq!(store.cards.len(), 2);
    let pk1 = *sk1.verifying_key().as_bytes();
    let pk2 = *sk2.verifying_key().as_bytes();
    let pk3 = *sk3.verifying_key().as_bytes();

    assert!(!store.cards.contains_key(&pk1), "Card 1 (Munich, far) should be evicted");
    assert!(store.cards.contains_key(&pk2), "Card 2 (Potsdam, closer) should be kept");
    assert!(store.cards.contains_key(&pk3), "Card 3 (Berlin, closest) should be kept");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    fn proptest_accept_envelope_invariants(hops in 1u8..=3, v in 1u64..1000) {
        let sk = make_key(20);
        let mut store = CardStore::default();
        let mut payload = valid_payload_input(&sk);
        payload.v = v;
        let card = sign_sd_card(payload, sample_items(), &sk).unwrap();
        let wire = render_wire(&card, 1, WireFilter::Full).unwrap();
        let env = CardEnvelope { card_wire: wire, hops };

        let outcome = accept_envelope(&mut store, &env, 1700000000, 2000, None, &|_| false);
        let is_accepted = matches!(outcome, AcceptOutcome::Accepted { .. });
        if hops <= card.payload.max_hops {
            prop_assert!(is_accepted);
        } else {
            prop_assert_eq!(outcome, AcceptOutcome::RejectedHops);
        }
    }
}
