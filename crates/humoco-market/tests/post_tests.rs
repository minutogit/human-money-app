use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use humoco_market::{
    compute_dummy_tag16, compute_listing_id, compute_post_tag16, create_dummy_post_box,
    decode_post_slot, derive_dummy_key, derive_post_box_key, derive_post_shared_secret,
    encode_post_slot, is_own_dummy_post_box, merge_post_slot, seal_post_box,
    try_open_post_box_any_sender, try_open_post_box_from_sender, validate_market_inquiry,
    validate_market_inquiry_reply, validate_post_delivery_receipt, validate_wot_handshake_assertion,
    DropDescriptor, InquiryRateTracker, ListingType, MarketError, MarketInquiry,
    MarketInquiryDecision, MarketInquiryReply, PostDeliveryReceipt, PostEnvelope,
    WotHandshakeAssertion, DROP_POST_MAX_BYTES, MAX_INQUIRY_CHARS, POST_BOX_TIERS,
    POST_SLOT_MAGIC,
};
use proptest::prelude::*;

fn make_key(seed_byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed_byte; 32])
}

#[test]
fn test_direct_crypto_derivations_and_constants() {
    let alice_sk = make_key(1);
    let bob_sk = make_key(2);
    let bob_vk = bob_sk.verifying_key();
    let alice_vk = alice_sk.verifying_key();

    // Symmetry: X25519(x(a), x(B)) == X25519(x(b), x(A))
    let shared_ab = derive_post_shared_secret(&alice_sk, &bob_vk).unwrap();
    let shared_ba = derive_post_shared_secret(&bob_sk, &alice_vk).unwrap();
    assert_eq!(shared_ab, shared_ba);
    assert_ne!(shared_ab, [0u8; 32]);

    let nonce = [7u8; 24];
    let tag = compute_post_tag16(&shared_ab, &nonce);
    assert_ne!(tag, [0u8; 16]);

    let k_box = derive_post_box_key(&shared_ab, &nonce).unwrap();
    assert_ne!(k_box, [0u8; 32]);

    let master_seed = [9u8; 32];
    let dummy_k = derive_dummy_key(&master_seed);
    let dummy_tag = compute_dummy_tag16(&dummy_k, &nonce);
    assert_ne!(dummy_tag, [0u8; 16]);

    assert_eq!(POST_SLOT_MAGIC, b"HMPOST01");
}

fn sample_envelope(msg_id: &str, byte_len: usize) -> PostEnvelope {
    PostEnvelope {
        msg_id: msg_id.to_string(),
        created_at: 1700000000,
        reply_drops: vec![DropDescriptor {
            write_url: "https://drop.example.com/write".to_string(),
            read_url: "https://drop.example.com/read".to_string(),
            slot_id: None,
            drop_pub: hex::encode([7u8; 32]),
        }],
        container_bytes: vec![42u8; byte_len],
    }
}

#[test]
fn test_post_tag16_and_box_sealing_unauthorized_isolation() {
    let alice_sk = make_key(1);
    let alice_vk = alice_sk.verifying_key();
    let alice_did = create_user_id(&alice_vk, None).unwrap();

    let bob_sk = make_key(2);
    let bob_vk = bob_sk.verifying_key();

    let charlie_sk = make_key(3);
    let charlie_vk = charlie_sk.verifying_key();
    let charlie_did = create_user_id(&charlie_vk, None).unwrap();

    let envelope = sample_envelope("msg-001", 120);

    // Alice seals box for Bob
    let entry = seal_post_box(&alice_sk, &bob_vk, &envelope).unwrap();
    assert_eq!(entry.ciphertext.len(), POST_BOX_TIERS[0]); // 4 KB tier

    // Bob opens successfully from Alice
    let opened = try_open_post_box_from_sender(&bob_sk, &alice_vk, &entry).unwrap();
    assert_eq!(opened, Some(envelope.clone()));

    // Charlie tries to open from Alice: tag16 check fails, returns Ok(None) immediately
    let charlie_opened = try_open_post_box_from_sender(&charlie_sk, &alice_vk, &entry).unwrap();
    assert_eq!(charlie_opened, None);

    // Bob checks if message was from Charlie: tag16 check fails, returns Ok(None)
    let bob_from_charlie = try_open_post_box_from_sender(&bob_sk, &charlie_vk, &entry).unwrap();
    assert_eq!(bob_from_charlie, None);

    // Multi-sender lookup: list containing Alice returns (alice_did, envelope)
    let senders = vec![
        (charlie_did.clone(), charlie_vk),
        (alice_did.clone(), alice_vk),
    ];
    let matched = try_open_post_box_any_sender(&bob_sk, &senders, &entry).unwrap();
    assert_eq!(matched, Some((alice_did, envelope.clone())));

    // Multi-sender lookup without Alice returns Ok(None)
    let senders_without_alice = vec![(charlie_did, charlie_vk)];
    let unmatched =
        try_open_post_box_any_sender(&bob_sk, &senders_without_alice, &entry).unwrap();
    assert_eq!(unmatched, None);
}

#[test]
fn test_fixed_tier_padding_and_aad_tamper_detection() {
    let alice_sk = make_key(10);
    let bob_sk = make_key(20);
    let bob_vk = bob_sk.verifying_key();

    // 1. Tier 0: 4,096 bytes (small payload)
    let env_small = sample_envelope("small", 50);
    let entry_small = seal_post_box(&alice_sk, &bob_vk, &env_small).unwrap();
    assert_eq!(entry_small.ciphertext.len(), 4_096);

    // 2. Tier 1: 16,384 bytes (~3 KB payload -> ~9 KB JSON)
    let env_med = sample_envelope("med", 3_000);
    let entry_med = seal_post_box(&alice_sk, &bob_vk, &env_med).unwrap();
    assert_eq!(entry_med.ciphertext.len(), 16_384);

    // 3. Tier 2: 65,536 bytes (~15 KB payload -> ~45 KB JSON)
    let env_large = sample_envelope("large", 15_000);
    let entry_large = seal_post_box(&alice_sk, &bob_vk, &env_large).unwrap();
    assert_eq!(entry_large.ciphertext.len(), 65_536);

    // 4. Tier 3: 262,144 bytes (~60 KB payload -> ~180 KB JSON)
    let env_xl = sample_envelope("xlarge", 60_000);
    let entry_xl = seal_post_box(&alice_sk, &bob_vk, &env_xl).unwrap();
    assert_eq!(entry_xl.ciphertext.len(), 262_144);

    // 5. Payload too large (> 262,144 bytes in JSON)
    let env_too_large = sample_envelope("too_large", 100_000);
    let err = seal_post_box(&alice_sk, &bob_vk, &env_too_large);
    assert!(matches!(err, Err(MarketError::PayloadTooLarge(_))));

    // 6. Ciphertext tampering: mutating 1 byte causes AEAD decryption failure
    let mut tampered_entry = entry_small.clone();
    tampered_entry.ciphertext[100] ^= 0xFF;
    let tamper_res =
        try_open_post_box_from_sender(&bob_sk, &alice_sk.verifying_key(), &tampered_entry);
    assert!(matches!(tamper_res, Err(MarketError::Crypto(_))));

    // 7. Nonce tampering: mutating nonce causes tag mismatch (returns Ok(None))
    let mut nonce_tampered = entry_small.clone();
    nonce_tampered.msg_nonce[0] ^= 0xFF;
    let res =
        try_open_post_box_from_sender(&bob_sk, &alice_sk.verifying_key(), &nonce_tampered).unwrap();
    assert_eq!(res, None);
}

#[test]
fn test_dummy_post_box_and_encoding_properties() {
    let master_seed = [42u8; 32];
    let other_seed = [43u8; 32];

    let dummy = create_dummy_post_box(&master_seed);
    assert_eq!(dummy.ciphertext.len(), 4_096);
    assert!(is_own_dummy_post_box(&master_seed, &dummy));
    assert!(!is_own_dummy_post_box(&other_seed, &dummy));

    // A real entry is not recognized as own dummy
    let alice_sk = make_key(1);
    let bob_vk = make_key(2).verifying_key();
    let real_entry = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("test", 30)).unwrap();
    assert!(!is_own_dummy_post_box(&master_seed, &real_entry));

    // 0 real entries -> padded with 2 dummies
    let slot_0 = encode_post_slot(&master_seed, &[], DROP_POST_MAX_BYTES).unwrap();
    let decoded_0 = decode_post_slot(&slot_0).unwrap();
    assert_eq!(decoded_0.len(), 2);
    assert!(is_own_dummy_post_box(&master_seed, &decoded_0[0]));
    assert!(is_own_dummy_post_box(&master_seed, &decoded_0[1]));

    // 1 real entry -> padded with 1 dummy (total 2 entries)
    let slot_1 = encode_post_slot(
        &master_seed,
        std::slice::from_ref(&real_entry),
        DROP_POST_MAX_BYTES,
    )
    .unwrap();
    let decoded_1 = decode_post_slot(&slot_1).unwrap();
    assert_eq!(decoded_1.len(), 2);
    let dummy_count = decoded_1
        .iter()
        .filter(|e| is_own_dummy_post_box(&master_seed, e))
        .count();
    let real_count = decoded_1
        .iter()
        .filter(|e| e.tag16 == real_entry.tag16)
        .count();
    assert_eq!(dummy_count, 1);
    assert_eq!(real_count, 1);

    // 2 real entries -> 0 dummies added (total 2 entries)
    let real_entry_2 = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("test2", 40)).unwrap();
    let slot_2 = encode_post_slot(
        &master_seed,
        &[real_entry.clone(), real_entry_2.clone()],
        DROP_POST_MAX_BYTES,
    )
    .unwrap();
    let decoded_2 = decode_post_slot(&slot_2).unwrap();
    assert_eq!(decoded_2.len(), 2);
    assert_eq!(
        decoded_2
            .iter()
            .filter(|e| is_own_dummy_post_box(&master_seed, e))
            .count(),
        0
    );

    // 3 real entries -> 0 dummies added (total 3 entries)
    let real_entry_3 = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("test3", 50)).unwrap();
    let slot_3 = encode_post_slot(
        &master_seed,
        &[real_entry, real_entry_2, real_entry_3],
        DROP_POST_MAX_BYTES,
    )
    .unwrap();
    let decoded_3 = decode_post_slot(&slot_3).unwrap();
    assert_eq!(decoded_3.len(), 3);
}

#[test]
fn test_multi_device_if_match_merge() {
    let master_seed = [99u8; 32];
    let alice_sk = make_key(1);
    let bob_vk = make_key(2).verifying_key();

    let entry_1 = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("item-1", 80)).unwrap();
    let entry_2 = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("item-2", 80)).unwrap();
    let entry_3 = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("item-3", 80)).unwrap();

    // Device A encodes slot with entry_1 (generates 1 dummy)
    let slot_dev_a =
        encode_post_slot(&master_seed, std::slice::from_ref(&entry_1), DROP_POST_MAX_BYTES).unwrap();

    // Device B merges entry_2 and entry_3, acknowledging entry_1's tag
    let merged_slot = merge_post_slot(
        &master_seed,
        Some(&slot_dev_a),
        &[entry_2.clone(), entry_3.clone()],
        &[entry_1.tag16], // remove entry_1
        DROP_POST_MAX_BYTES,
    )
    .unwrap();

    let decoded = decode_post_slot(&merged_slot).unwrap();
    // entry_1 should be gone, old dummy from dev A stripped, entry_2 and entry_3 present (total 2)
    assert_eq!(decoded.len(), 2);
    assert!(!decoded.iter().any(|e| e.tag16 == entry_1.tag16));
    assert!(decoded.iter().any(|e| e.tag16 == entry_2.tag16));
    assert!(decoded.iter().any(|e| e.tag16 == entry_3.tag16));
    // Dummies count should be 0 because 2 real entries exist
    let dummies = decoded
        .iter()
        .filter(|e| is_own_dummy_post_box(&master_seed, e))
        .count();
    assert_eq!(dummies, 0);

    // Test idempotent re-merge of already present entries (no duplicates)
    let re_merged = merge_post_slot(
        &master_seed,
        Some(&merged_slot),
        std::slice::from_ref(&entry_2),
        &[],
        DROP_POST_MAX_BYTES,
    )
    .unwrap();
    let decoded_re = decode_post_slot(&re_merged).unwrap();
    assert_eq!(decoded_re.len(), 2);
}

#[test]
fn test_decode_post_slot_corruptions() {
    let master_seed = [1u8; 32];
    let alice_sk = make_key(1);
    let bob_vk = make_key(2).verifying_key();
    let real_entry = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("c", 20)).unwrap();

    let valid_slot =
        encode_post_slot(&master_seed, std::slice::from_ref(&real_entry), DROP_POST_MAX_BYTES).unwrap();

    // Too short (< 10 bytes)
    assert!(matches!(
        decode_post_slot(&valid_slot[0..8]),
        Err(MarketError::Slot(_))
    ));

    // Wrong magic
    let mut bad_magic = valid_slot.clone();
    bad_magic[0] = b'X';
    assert!(matches!(
        decode_post_slot(&bad_magic),
        Err(MarketError::Slot(_))
    ));

    // Truncated entry header
    assert!(matches!(
        decode_post_slot(&valid_slot[0..20]),
        Err(MarketError::Slot(_))
    ));

    // Trailing junk
    let mut trailing = valid_slot.clone();
    trailing.push(0xAA);
    assert!(matches!(
        decode_post_slot(&trailing),
        Err(MarketError::Slot(_))
    ));
}

#[test]
fn test_market_inquiry_models_and_rate_limiting() {
    let inquiry = MarketInquiry {
        inquiry_id: "inq-123".to_string(),
        listing_id: "lst-abc".to_string(),
        listing_title: "Fresh Bio Honey".to_string(),
        message: "Hello! Is this still available for pickup this weekend?".to_string(),
        created_at: 1700000000,
    };
    assert!(validate_market_inquiry(&inquiry).is_ok());

    // Empty message
    let mut empty_msg = inquiry.clone();
    empty_msg.message = "   ".to_string();
    assert!(validate_market_inquiry(&empty_msg).is_err());

    // Message exceeding 280 chars
    let mut long_msg = inquiry.clone();
    long_msg.message = "A".repeat(MAX_INQUIRY_CHARS + 1);
    assert!(validate_market_inquiry(&long_msg).is_err());

    // Exact 280 chars is accepted
    long_msg.message = "A".repeat(MAX_INQUIRY_CHARS);
    assert!(validate_market_inquiry(&long_msg).is_ok());

    // Reply testing
    let reply = MarketInquiryReply {
        inquiry_id: "inq-123".to_string(),
        listing_id: "lst-abc".to_string(),
        decision: MarketInquiryDecision::Confirm,
        note: Some("Yes, pickup available!".to_string()),
        payment_uri: Some("humoco:transfer?amount=5".to_string()),
        amount_hint: Some("5 Minuto".to_string()),
        standard_uuid: None,
        replied_at: 1700001000,
    };
    assert!(validate_market_inquiry_reply(&reply).is_ok());

    let mut long_note = reply.clone();
    long_note.note = Some("B".repeat(MAX_INQUIRY_CHARS + 1));
    assert!(validate_market_inquiry_reply(&long_note).is_err());

    // Rate tracker testing (max 3 for testing)
    let mut tracker = InquiryRateTracker::new(3);
    let listing_id = "listing-honey-01";

    let buyer_1 = "aaaa".repeat(16);
    let buyer_2 = "bbbb".repeat(16);
    let buyer_3 = "cccc".repeat(16);
    let buyer_4 = "dddd".repeat(16);

    // Record buyer 1
    assert!(tracker.record_inquiry(&buyer_1, listing_id).is_ok());
    assert_eq!(tracker.open_count(listing_id), 1);
    assert!(tracker.has_open_inquiry(&buyer_1, listing_id));

    // Duplicate inquiry from buyer 1 rejected
    let dup_err = tracker.record_inquiry(&buyer_1, listing_id);
    assert!(dup_err.is_err());

    // Record buyer 2 and 3
    assert!(tracker.record_inquiry(&buyer_2, listing_id).is_ok());
    assert!(tracker.record_inquiry(&buyer_3, listing_id).is_ok());
    assert_eq!(tracker.open_count(listing_id), 3);

    // Buyer 4 exceeds limit (max 3)
    let cap_err = tracker.record_inquiry(&buyer_4, listing_id);
    assert!(cap_err.is_err());

    // Resolve buyer 1
    assert!(tracker.resolve_inquiry(&buyer_1, listing_id));
    assert_eq!(tracker.open_count(listing_id), 2);
    assert!(!tracker.has_open_inquiry(&buyer_1, listing_id));

    // Now buyer 4 can record
    assert!(tracker.record_inquiry(&buyer_4, listing_id).is_ok());
    assert_eq!(tracker.open_count(listing_id), 3);
}

#[test]
fn test_auxiliary_payload_validations() {
    let receipt = PostDeliveryReceipt {
        acknowledged_msg_id: "msg-001".to_string(),
        acknowledged_tag16_hex: hex::encode([5u8; 16]),
        bundle_id: Some("bundle-xyz".to_string()),
        received_at: 1700000000,
    };
    assert!(validate_post_delivery_receipt(&receipt).is_ok());

    let mut bad_receipt = receipt.clone();
    bad_receipt.acknowledged_tag16_hex = "short".to_string();
    assert!(validate_post_delivery_receipt(&bad_receipt).is_err());

    let alice_sk = make_key(1);
    let alice_did = create_user_id(&alice_sk.verifying_key(), None).unwrap();

    let handshake = WotHandshakeAssertion {
        sender_did: alice_did.clone(),
        sender_name: Some("Alice".to_string()),
        proposed_level: 1,
        wot_sync_json: "{\"version\":1}".to_string(),
        reply_requested: true,
        created_at: 1700000000,
    };
    assert!(validate_wot_handshake_assertion(&handshake).is_ok());

    let mut bad_level = handshake.clone();
    bad_level.proposed_level = 2; // only 0 or 1 allowed
    assert!(validate_wot_handshake_assertion(&bad_level).is_err());

    // Listing ID computation
    let id1 = compute_listing_id(&alice_did, ListingType::Offer, "Organic Apples");
    let id2 = compute_listing_id(&alice_did, ListingType::Offer, "Organic Apples");
    let id3 = compute_listing_id(&alice_did, ListingType::Request, "Organic Apples");
    assert_eq!(id1.len(), 16);
    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

// ---------------------------------------------------------------------------
// Proptest Invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn proptest_decode_post_slot_arbitrary_bytes_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let _ = decode_post_slot(&bytes);
    }

    #[test]
    fn proptest_seal_and_open_roundtrip(
        payload_len in 1usize..2000,
        msg_id in "[a-z0-9]{1,16}",
    ) {
        let alice_sk = make_key(5);
        let bob_sk = make_key(6);
        let bob_vk = bob_sk.verifying_key();

        let env = sample_envelope(&msg_id, payload_len);
        let entry = seal_post_box(&alice_sk, &bob_vk, &env).unwrap();

        assert!(POST_BOX_TIERS.contains(&entry.ciphertext.len()));

        let opened = try_open_post_box_from_sender(&bob_sk, &alice_sk.verifying_key(), &entry).unwrap();
        prop_assert_eq!(opened, Some(env));
    }
}
