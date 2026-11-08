use ed25519_dalek::SigningKey;
use human_money_core::crypto::{create_user_id, encode_base64};
use humoco_market::{
    accept_envelope, encode_qr_svg, forward_envelope, mark_sent, next_card_version, own_envelope,
    should_attach_card, sign_card, verify_card, AcceptOutcome, CardEnvelope, CardStore,
    ContactCard, DropDescriptor, Listing, ListingType, MAX_QR_SHARE_BYTES, MAX_STORED_CARDS,
};
use proptest::prelude::*;

fn make_key(seed: u8) -> SigningKey {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    SigningKey::from_bytes(&bytes)
}

fn valid_card(sk: &SigningKey) -> ContactCard {
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    ContactCard {
        did,
        v: 100,
        valid_until: 1800000000,
        max_hops: 2,
        profile: Default::default(),
        listings: vec![Listing {
            listing_type: ListingType::Offer,
            title: "Test Service".to_string(),
            text: Some("Description".to_string()),
            pricing: Some("10 M".to_string()),
        }],
        drops: vec![],
    }
}

#[test]
fn test_limits_listings_count() {
    let sk = make_key(1);
    let mut card = valid_card(&sk);
    card.listings = (0..10)
        .map(|i| Listing {
            listing_type: ListingType::Offer,
            title: format!("Title {}", i),
            text: None,
            pricing: None,
        })
        .collect();
    assert!(sign_card(card.clone(), &sk).is_ok());

    card.listings.push(Listing {
        listing_type: ListingType::Offer,
        title: "Eleventh".to_string(),
        text: None,
        pricing: None,
    });
    assert!(sign_card(card, &sk).is_err());
}

#[test]
fn test_limits_title_chars() {
    let sk = make_key(2);
    let mut card = valid_card(&sk);
    // 80 chars ok
    card.listings[0].title = "a".repeat(80);
    assert!(sign_card(card.clone(), &sk).is_ok());

    // 81 chars err
    card.listings[0].title = "a".repeat(81);
    assert!(sign_card(card.clone(), &sk).is_err());

    // empty title err
    card.listings[0].title = "   ".to_string();
    assert!(sign_card(card, &sk).is_err());
}

#[test]
fn test_limits_text_and_pricing_chars() {
    let sk = make_key(3);
    let mut card = valid_card(&sk);
    // 280 text chars ok, 281 err
    card.listings[0].text = Some("b".repeat(280));
    assert!(sign_card(card.clone(), &sk).is_ok());

    card.listings[0].text = Some("b".repeat(281));
    assert!(sign_card(card.clone(), &sk).is_err());
    card.listings[0].text = None;

    // 40 pricing chars ok, 41 err
    card.listings[0].pricing = Some("c".repeat(40));
    assert!(sign_card(card.clone(), &sk).is_ok());

    card.listings[0].pricing = Some("c".repeat(41));
    assert!(sign_card(card, &sk).is_err());
}

#[test]
fn test_limits_drops() {
    let sk = make_key(4);
    let mut card = valid_card(&sk);
    let valid_drop = DropDescriptor {
        write_url: "https://drop.example.com/write".to_string(),
        read_url: "https://drop.example.com/read".to_string(),
        drop_pub: bs58::encode([42u8; 32]).into_string(),
    };

    card.drops = vec![valid_drop.clone(), valid_drop.clone(), valid_drop.clone()];
    assert!(sign_card(card.clone(), &sk).is_ok());

    // 4 drops err
    card.drops.push(valid_drop);
    assert!(sign_card(card.clone(), &sk).is_err());

    // Non-https write_url
    card.drops = vec![DropDescriptor {
        write_url: "http://insecure.com".to_string(),
        read_url: "https://secure.com".to_string(),
        drop_pub: bs58::encode([42u8; 32]).into_string(),
    }];
    assert!(sign_card(card, &sk).is_err());
}

#[test]
fn test_limits_max_hops() {
    let sk = make_key(5);
    let mut card = valid_card(&sk);
    card.max_hops = 1;
    assert!(sign_card(card.clone(), &sk).is_ok());
    card.max_hops = 2;
    assert!(sign_card(card.clone(), &sk).is_ok());
    card.max_hops = 3;
    assert!(sign_card(card.clone(), &sk).is_ok());

    card.max_hops = 0;
    assert!(sign_card(card.clone(), &sk).is_err());
    card.max_hops = 4;
    assert!(sign_card(card, &sk).is_err());
}

#[test]
fn test_prefix_account_normalized_to_root_did() {
    let sk = make_key(6);
    let root_did = create_user_id(&sk.verifying_key(), None).unwrap();
    let prefix_did = format!("pc:device1@{}", root_did);

    let mut card = valid_card(&sk);
    card.did = prefix_did;

    let jws = sign_card(card, &sk).unwrap();
    let verified = verify_card(&jws).unwrap();
    assert_eq!(verified.did, root_did);
}

#[test]
fn test_wrong_typ_or_alg_rejected() {
    let sk = make_key(7);
    let card = valid_card(&sk);
    let canonical = serde_json_canonicalizer::to_string(&card).unwrap();
    let payload_b64 = encode_base64(canonical.as_bytes());

    // Wrong typ
    let bad_header = encode_base64(br#"{"alg":"EdDSA","typ":"JWT"}"#);
    let signing_input = format!("{}.{}", bad_header, payload_b64);
    let sig = human_money_core::crypto::sign_ed25519(&sk, signing_input.as_bytes());
    let jws = format!("{}.{}.{}", bad_header, payload_b64, encode_base64(&sig.to_bytes()));
    assert!(verify_card(&jws).is_err());

    // Wrong alg
    let bad_header2 = encode_base64(br#"{"alg":"ES256","typ":"hmc-card+jwt"}"#);
    let signing_input2 = format!("{}.{}", bad_header2, payload_b64);
    let sig2 = human_money_core::crypto::sign_ed25519(&sk, signing_input2.as_bytes());
    let jws2 = format!("{}.{}.{}", bad_header2, payload_b64, encode_base64(&sig2.to_bytes()));
    assert!(verify_card(&jws2).is_err());
}

#[test]
fn test_forwarding_hop_bounds() {
    let sk = make_key(8);
    let mut card = valid_card(&sk);
    card.max_hops = 2;
    let jws = sign_card(card.clone(), &sk).unwrap();

    let stored = humoco_market::StoredCard {
        card,
        card_jws: jws,
        hops: 1,
        received_at: 1000,
    };

    let fwd1 = forward_envelope(&stored).unwrap();
    assert_eq!(fwd1.hops, 2);

    let mut stored2 = stored.clone();
    stored2.hops = 2;
    // Cannot forward beyond max_hops
    assert!(forward_envelope(&stored2).is_none());
}

#[test]
fn test_accept_envelope_freshness_and_hop_upgrade() {
    let sk = make_key(9);
    let card_v1 = valid_card(&sk);
    let jws_v1 = sign_card(card_v1, &sk).unwrap();

    let mut store = CardStore::default();
    let env_v1_hop2 = CardEnvelope {
        card_jws: jws_v1.clone(),
        hops: 2,
    };

    let res = accept_envelope(&mut store, &env_v1_hop2, 1000, 2, &|_| false);
    assert!(matches!(res, AcceptOutcome::Accepted { replaced: false, .. }));

    // Older version rejected
    let mut card_v0 = valid_card(&sk);
    card_v0.v = 50;
    let jws_v0 = sign_card(card_v0, &sk).unwrap();
    let env_v0 = CardEnvelope {
        card_jws: jws_v0,
        hops: 1,
    };
    let res_older = accept_envelope(&mut store, &env_v0, 1000, 2, &|_| false);
    assert_eq!(res_older, AcceptOutcome::IgnoredStale);

    // Same version with smaller hops upgrades entry!
    let env_v1_hop1 = CardEnvelope {
        card_jws: jws_v1,
        hops: 1,
    };
    let res_upgrade = accept_envelope(&mut store, &env_v1_hop1, 1001, 2, &|_| false);
    assert!(matches!(res_upgrade, AcceptOutcome::Accepted { replaced: true, .. }));
    let pk_bytes = *sk.verifying_key().as_bytes();
    assert_eq!(store.cards.get(&pk_bytes).unwrap().hops, 1);
}

#[test]
fn test_store_eviction_order() {
    let mut store = CardStore::default();

    // Fill store directly to MAX_STORED_CARDS with hops = 1, received_at = 1000
    for i in 0..MAX_STORED_CARDS {
        let mut pk = [0u8; 32];
        pk[0..2].copy_from_slice(&(i as u16).to_le_bytes());
        store.cards.insert(
            pk,
            humoco_market::StoredCard {
                card: ContactCard {
                    did: format!("did:key:{}", i),
                    v: 100,
                    valid_until: 2000000000,
                    max_hops: 3,
                    profile: Default::default(),
                    listings: vec![],
                    drops: vec![],
                },
                card_jws: format!("jws-{}", i),
                hops: if i == 0 { 2 } else { 1 }, // entry 0 has largest hops = 2
                received_at: 1000,
            },
        );
    }
    assert_eq!(store.cards.len(), MAX_STORED_CARDS);

    // Now insert one more card -> entry with largest hops (entry 0) must be evicted!
    let new_sk = make_key(250);
    let new_card = valid_card(&new_sk);
    let new_jws = sign_card(new_card, &new_sk).unwrap();
    let new_env = CardEnvelope {
        card_jws: new_jws,
        hops: 1,
    };

    let outcome = accept_envelope(&mut store, &new_env, 1002, 3, &|_| false);
    assert!(matches!(outcome, AcceptOutcome::Accepted { .. }));
    assert_eq!(store.cards.len(), MAX_STORED_CARDS);

    // Check entry 0 was evicted
    let pk0 = [0u8; 32];
    assert!(!store.cards.contains_key(&pk0));
}

#[test]
fn test_sent_log_and_versioning() {
    let mut store = CardStore::default();
    let peer_key = [77u8; 32];

    assert!(should_attach_card(&store, &peer_key, 100));

    mark_sent(&mut store, peer_key, 100);
    assert!(!should_attach_card(&store, &peer_key, 100));
    assert!(should_attach_card(&store, &peer_key, 101));

    assert_eq!(next_card_version(None, 1000), 1000);
    assert_eq!(next_card_version(Some(1000), 500), 1001);
    assert_eq!(next_card_version(Some(1000), 2000), 2000);
}

#[test]
fn test_card_store_json_roundtrip() {
    let sk = make_key(10);
    let card = valid_card(&sk);
    let jws = sign_card(card.clone(), &sk).unwrap();
    let mut store = CardStore::default();

    let env = own_envelope(jws);
    accept_envelope(&mut store, &env, 1000, 2, &|_| false);
    mark_sent(&mut store, *sk.verifying_key().as_bytes(), 100);

    let json = serde_json::to_string(&store).unwrap();
    let deserialized: CardStore = serde_json::from_str(&json).unwrap();
    assert_eq!(store, deserialized);
}

#[test]
fn test_qr_svg_generation() {
    let short_data = b"humoco://card?env=test";
    let svg = encode_qr_svg(short_data).unwrap();
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>"));
    assert!(svg.contains("<path fill=\"#000000\" d=\""));
    assert!(svg.ends_with("</svg>"));

    // 2900 bytes ok
    let max_data = vec![b'a'; MAX_QR_SHARE_BYTES];
    assert!(encode_qr_svg(&max_data).is_ok());

    // 2901 bytes error
    let overflow_data = vec![b'a'; MAX_QR_SHARE_BYTES + 1];
    assert!(encode_qr_svg(&overflow_data).is_err());
}

proptest! {
    #[test]
    fn proptest_accept_envelope_invariants(
        seed in 1u8..30u8,
        version in 1u64..1000u64,
        max_hops in 1u8..=3u8,
        env_hops in 1u8..=5u8,
        max_store_hops in 1u8..=3u8,
    ) {
        let sk = make_key(seed);
        let mut card = valid_card(&sk);
        card.v = version;
        card.max_hops = max_hops;
        let jws = sign_card(card.clone(), &sk).unwrap();

        let env = CardEnvelope {
            card_jws: jws,
            hops: env_hops,
        };

        let mut store = CardStore::default();
        let outcome = accept_envelope(&mut store, &env, 1000, max_store_hops, &|_| false);

        if env_hops <= max_hops && env_hops <= max_store_hops {
            let is_accepted = matches!(outcome, AcceptOutcome::Accepted { .. });
            prop_assert!(is_accepted);
            prop_assert_eq!(store.cards.len(), 1);
            let stored = store.cards.get(sk.verifying_key().as_bytes()).unwrap();
            prop_assert!(stored.hops <= stored.card.max_hops);
            prop_assert!(stored.hops <= max_store_hops);
            prop_assert!(1000 < stored.card.valid_until);
        } else {
            prop_assert_eq!(outcome, AcceptOutcome::RejectedHops);
            prop_assert_eq!(store.cards.len(), 0);
        }
    }
}
