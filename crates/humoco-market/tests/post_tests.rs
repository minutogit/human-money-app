use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use humoco_market::{
    compute_dummy_tag16, compute_post_tag16, create_dummy_post_box, decode_post_slot,
    derive_dummy_key, derive_post_box_key, derive_post_shared_secret, encode_post_slot,
    is_own_dummy_post_box, pack_post_slot, prepare_post_senders, seal_post_box,
    try_open_post_box_from_sender, try_open_post_box_prepared, validate_market_inquiry,
    validate_market_inquiry_message, validate_post_delivery_receipt,
    validate_wot_handshake_assertion, DropDescriptor, InquiryRateTracker, InquiryTarget,
    MarketError, MarketInquiry, MarketInquiryDecision, MarketInquiryMessage, PostDeliveryReceipt,
    PostEnvelope, WotHandshakeAssertion, DROP_POST_MAX_BYTES, MAX_INQUIRY_CHARS, POST_BOX_TIERS,
    POST_SLOT_MAGIC,
};
use proptest::prelude::*;

fn assert_err_variant<T: std::fmt::Debug>(
    res: Result<T, MarketError>,
    variant_name: &str,
    expected_substr: &str,
    extract: impl FnOnce(&MarketError) -> Option<&str>,
) {
    match res {
        Err(ref err) => match extract(err) {
            Some(msg) => assert!(
                msg.contains(expected_substr),
                "Expected MarketError::{} containing {:?}, got {:?}: {:?}",
                variant_name,
                expected_substr,
                err,
                msg
            ),
            None => panic!(
                "Expected MarketError::{}({:?}), got different variant {:?}",
                variant_name, expected_substr, err
            ),
        },
        Ok(val) => panic!(
            "Expected Err(MarketError::{}({:?})), got Ok({:?})",
            variant_name, expected_substr, val
        ),
    }
}

fn assert_validation_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    assert_err_variant(res, "Validation", expected_substr, |e| match e {
        MarketError::Validation(msg) => Some(msg.as_str()),
        _ => None,
    });
}

fn assert_slot_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    assert_err_variant(res, "Slot", expected_substr, |e| match e {
        MarketError::Slot(msg) => Some(msg.as_str()),
        _ => None,
    });
}

fn assert_crypto_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    assert_err_variant(res, "Crypto", expected_substr, |e| match e {
        MarketError::Crypto(msg) => Some(msg.as_str()),
        _ => None,
    });
}

fn assert_payload_too_large_err<T: std::fmt::Debug>(
    res: Result<T, MarketError>,
    expected_substr: &str,
) {
    assert_err_variant(res, "PayloadTooLarge", expected_substr, |e| match e {
        MarketError::PayloadTooLarge(msg) => Some(msg.as_str()),
        _ => None,
    });
}

fn assert_unknown_field_err<T: std::fmt::Debug>(
    res: Result<T, serde_json::Error>,
    field_name: &str,
) {
    let err = res.expect_err("expected serde_json::from_str to reject unknown field");
    let msg = err.to_string();
    let expected = format!("unknown field `{}`", field_name);
    assert!(
        msg.contains(&expected),
        "Expected serde_json unknown field error for {:?}, got {:?}",
        field_name,
        msg
    );
}

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
        sender_did: "did:key:zSenderPlaceholder".to_string(),
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

    // Multi-sender lookup: prepared senders containing Alice returns (alice_did, envelope)
    let senders = vec![
        (charlie_did.clone(), charlie_vk),
        (alice_did.clone(), alice_vk),
    ];
    let prepared_with_alice = prepare_post_senders(&bob_sk, &senders);
    let matched = try_open_post_box_prepared(&prepared_with_alice, &entry).unwrap();
    assert_eq!(matched, Some((alice_did.clone(), envelope.clone())));

    // Reuse same prepared_with_alice across a second box from Alice
    let envelope_2 = sample_envelope("msg-002", 64);
    let entry_2 = seal_post_box(&alice_sk, &bob_vk, &envelope_2).unwrap();
    let matched_2 = try_open_post_box_prepared(&prepared_with_alice, &entry_2).unwrap();
    assert_eq!(matched_2, Some((alice_did, envelope_2)));

    // Multi-sender lookup without Alice returns Ok(None)
    let senders_without_alice = vec![(charlie_did, charlie_vk)];
    let prepared_without_alice = prepare_post_senders(&bob_sk, &senders_without_alice);
    let unmatched = try_open_post_box_prepared(&prepared_without_alice, &entry).unwrap();
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
    assert_payload_too_large_err(err, "exceeds maximum post box tier 262144");

    // 6. Ciphertext tampering: mutating 1 byte causes AEAD decryption failure
    let mut tampered_entry = entry_small.clone();
    tampered_entry.ciphertext[100] ^= 0xFF;
    let tamper_res =
        try_open_post_box_from_sender(&bob_sk, &alice_sk.verifying_key(), &tampered_entry);
    assert_crypto_err(tamper_res, "AEAD authentication failed:");

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
    let slot_dev_a = encode_post_slot(
        &master_seed,
        std::slice::from_ref(&entry_1),
        DROP_POST_MAX_BYTES,
    )
    .unwrap();

    // Device B packs non-empty additions without entry_1, dropping old dummy boxes via pack_post_slot
    let remote_real: Vec<_> = decode_post_slot(&slot_dev_a)
        .unwrap()
        .into_iter()
        .filter(|e| !is_own_dummy_post_box(&master_seed, e) && e.tag16 != entry_1.tag16)
        .collect();
    let mut additions = remote_real;
    additions.push(entry_2.clone());
    additions.push(entry_3.clone());
    let packed = pack_post_slot(&master_seed, &additions, DROP_POST_MAX_BYTES).unwrap();

    let decoded = decode_post_slot(&packed.blob).unwrap();
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

    // Test idempotent re-pack deduplication of already present entries (no duplicates)
    let mut dup_input = decoded;
    dup_input.push(entry_2);
    let re_packed = pack_post_slot(&master_seed, &dup_input, DROP_POST_MAX_BYTES).unwrap();
    let decoded_re = decode_post_slot(&re_packed.blob).unwrap();
    assert_eq!(decoded_re.len(), 2);
}

#[test]
fn test_decode_post_slot_corruptions() {
    let master_seed = [1u8; 32];
    let alice_sk = make_key(1);
    let bob_vk = make_key(2).verifying_key();
    let real_entry = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("c", 20)).unwrap();

    let valid_slot = encode_post_slot(
        &master_seed,
        std::slice::from_ref(&real_entry),
        DROP_POST_MAX_BYTES,
    )
    .unwrap();
    // Positive baseline: valid encoded slot decodes cleanly
    assert_eq!(decode_post_slot(&valid_slot).unwrap().len(), 2);

    // Too short (< 10 bytes)
    assert_slot_err(
        decode_post_slot(&valid_slot[0..8]),
        "Post slot too short: 8 bytes",
    );

    // Wrong magic
    let mut bad_magic = valid_slot.clone();
    bad_magic[0] = b'X';
    assert_slot_err(decode_post_slot(&bad_magic), "Invalid post slot magic header");

    // Truncated entry header
    assert_slot_err(
        decode_post_slot(&valid_slot[0..20]),
        "Truncated post box header at entry 0",
    );

    // Invalid tier length on entry 0 (bytes 50..54 hold u32_be ct_len)
    let mut bad_tier = valid_slot.clone();
    bad_tier[50..54].copy_from_slice(&1234u32.to_be_bytes());
    assert_slot_err(
        decode_post_slot(&bad_tier),
        "Invalid post box tier length 1234 at entry 0",
    );

    // Truncated ciphertext on entry 1
    assert_slot_err(
        decode_post_slot(&valid_slot[..valid_slot.len() - 1]),
        "Truncated ciphertext for entry 1:",
    );

    // Trailing junk
    let mut trailing = valid_slot.clone();
    trailing.push(0xAA);
    assert_slot_err(
        decode_post_slot(&trailing),
        "Trailing unexpected bytes in post slot:",
    );
}

#[test]
fn test_market_inquiry_models_and_rate_limiting() {
    let ad_target = InquiryTarget::Ad {
        ad_id: "0123456789abcdef".to_string(),
        ad_title: "Fresh Bio Honey".to_string(),
    };
    let inquiry = MarketInquiry {
        inquiry_id: "inq-123".to_string(),
        target: ad_target,
        message: "Hello! Is this still available for pickup this weekend?".to_string(),
        created_at: 1700000000,
    };
    assert_eq!(validate_market_inquiry(&inquiry), Ok(()));

    // Person inquiry variant
    let person_inquiry = MarketInquiry {
        inquiry_id: "inq-456".to_string(),
        target: InquiryTarget::Person {
            skill_title: Some("Honey Maker".to_string()),
        },
        message: "Hi, can you make custom honey?".to_string(),
        created_at: 1700000000,
    };
    assert_eq!(validate_market_inquiry(&person_inquiry), Ok(()));

    // Empty message
    let mut empty_msg = inquiry.clone();
    empty_msg.message = "   ".to_string();
    assert_validation_err(
        validate_market_inquiry(&empty_msg),
        "message cannot be empty",
    );

    // Message exceeding 280 chars
    let mut long_msg = inquiry.clone();
    long_msg.message = "A".repeat(MAX_INQUIRY_CHARS + 1);
    assert_validation_err(
        validate_market_inquiry(&long_msg),
        "message length 281 exceeds MAX_INQUIRY_CHARS (280)",
    );

    // Exact 280 chars is accepted
    long_msg.message = "A".repeat(MAX_INQUIRY_CHARS);
    assert_eq!(validate_market_inquiry(&long_msg), Ok(()));

    // Thread message & decision testing
    let msg_confirm = MarketInquiryMessage {
        inquiry_id: "inq-123".to_string(),
        msg_id: "msg-1".to_string(),
        text: Some("Yes, pickup available!".to_string()),
        decision: Some(MarketInquiryDecision::Confirm),
        payment_uri: Some("humoco://pay?to=did:key:zAlice&amount=5".to_string()),
        amount_hint: Some("5 Minuto".to_string()),
        standard_uuid: None,
        created_at: 1700001000,
    };
    assert_eq!(validate_market_inquiry_message(&msg_confirm), Ok(()));

    let msg_text_only = MarketInquiryMessage {
        inquiry_id: "inq-123".to_string(),
        msg_id: "msg-2".to_string(),
        text: Some("Does 4pm work?".to_string()),
        decision: None,
        payment_uri: None,
        amount_hint: None,
        standard_uuid: None,
        created_at: 1700002000,
    };
    assert_eq!(validate_market_inquiry_message(&msg_text_only), Ok(()));

    let mut empty_both = msg_text_only.clone();
    empty_both.text = None;
    assert_validation_err(
        validate_market_inquiry_message(&empty_both),
        "message must include either text or a decision",
    );

    let mut blank_text = msg_text_only.clone();
    blank_text.text = Some("   ".to_string());
    assert_validation_err(
        validate_market_inquiry_message(&blank_text),
        "message text cannot be blank",
    );

    let mut long_note = msg_confirm.clone();
    long_note.text = Some("B".repeat(MAX_INQUIRY_CHARS + 1));
    assert_validation_err(
        validate_market_inquiry_message(&long_note),
        "message text length 281 exceeds MAX_INQUIRY_CHARS (280)",
    );

    // Exact 280 chars thread message accepted; non-humoco payment_uri rejected
    long_note.text = Some("B".repeat(MAX_INQUIRY_CHARS));
    assert_eq!(validate_market_inquiry_message(&long_note), Ok(()));

    let mut bad_pay_uri = msg_confirm.clone();
    bad_pay_uri.payment_uri = Some("https://evil.example/pay".to_string());
    assert_validation_err(
        validate_market_inquiry_message(&bad_pay_uri),
        "payment_uri must start with 'humoco://pay?'",
    );

    // Rate tracker testing (max 3 for testing)
    let mut tracker = InquiryRateTracker::new(3);
    let target_key = "ad:0123456789abcdef";

    let buyer_1 = "aaaa".repeat(16);
    let buyer_2 = "bbbb".repeat(16);
    let buyer_3 = "cccc".repeat(16);
    let buyer_4 = "dddd".repeat(16);

    // Record buyer 1
    assert_eq!(tracker.record_inquiry(&buyer_1, target_key), Ok(()));
    assert_eq!(tracker.open_count(target_key), 1);
    assert!(tracker.has_open_inquiry(&buyer_1, target_key));

    // Duplicate inquiry from buyer 1 rejected
    let dup_err = tracker.record_inquiry(&buyer_1, target_key);
    assert_validation_err(
        dup_err,
        "already has an open inquiry for target ad:0123456789abcdef",
    );

    // Record buyer 2 and 3
    assert_eq!(tracker.record_inquiry(&buyer_2, target_key), Ok(()));
    assert_eq!(tracker.record_inquiry(&buyer_3, target_key), Ok(()));
    assert_eq!(tracker.open_count(target_key), 3);

    // Buyer 4 exceeds limit (max 3)
    let cap_err = tracker.record_inquiry(&buyer_4, target_key);
    assert_validation_err(
        cap_err,
        "Target ad:0123456789abcdef reached maximum open inquiries (3)",
    );

    // Resolve buyer 1
    assert!(tracker.resolve_inquiry(&buyer_1, target_key));
    assert_eq!(tracker.open_count(target_key), 2);
    assert!(!tracker.has_open_inquiry(&buyer_1, target_key));

    // Now buyer 4 can record
    assert_eq!(tracker.record_inquiry(&buyer_4, target_key), Ok(()));
    assert_eq!(tracker.open_count(target_key), 3);
}

#[test]
fn test_auxiliary_payload_validations() {
    let receipt = PostDeliveryReceipt {
        acknowledged_msg_id: "msg-001".to_string(),
        acknowledged_tag16_hex: hex::encode([5u8; 16]),
        bundle_id: Some("bundle-xyz".to_string()),
        received_at: 1700000000,
    };
    assert_eq!(validate_post_delivery_receipt(&receipt), Ok(()));

    let mut bad_receipt = receipt.clone();
    bad_receipt.acknowledged_tag16_hex = "short".to_string();
    assert_validation_err(
        validate_post_delivery_receipt(&bad_receipt),
        "acknowledged_tag16_hex must be a valid 32-lowercase-hex string",
    );

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
    assert_eq!(validate_wot_handshake_assertion(&handshake), Ok(()));

    let mut level0_handshake = handshake.clone();
    level0_handshake.proposed_level = 0;
    assert_eq!(validate_wot_handshake_assertion(&level0_handshake), Ok(()));

    let mut bad_level = handshake.clone();
    bad_level.proposed_level = 2; // only 0 or 1 allowed
    assert_validation_err(
        validate_wot_handshake_assertion(&bad_level),
        "proposed_level must be 0 or 1, got 2",
    );

    // InquiryTarget target_key tests
    let ad_t = InquiryTarget::Ad {
        ad_id: "abcdef0123456789".to_string(),
        ad_title: "Title".to_string(),
    };
    assert_eq!(ad_t.target_key(), "ad:abcdef0123456789");

    let person_t = InquiryTarget::Person {
        skill_title: Some("Garden Work".to_string()),
    };
    assert_eq!(person_t.target_key(), "person:garden work");

    let general_t = InquiryTarget::Person { skill_title: None };
    assert_eq!(general_t.target_key(), "person");
}

// ---------------------------------------------------------------------------
// Proptest Invariants
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn proptest_decode_post_slot_arbitrary_bytes_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let res = decode_post_slot(&bytes);
        if bytes.len() < 10 {
            prop_assert!(
                matches!(&res, Err(MarketError::Slot(msg)) if msg.contains("Post slot too short:")),
                "expected short slot error for len={}, got {:?}",
                bytes.len(),
                res
            );
        } else if &bytes[0..8] != POST_SLOT_MAGIC {
            prop_assert!(
                matches!(&res, Err(MarketError::Slot(msg)) if msg.contains("Invalid post slot magic header")),
                "expected bad magic error, got {:?}",
                res
            );
        } else {
            match res {
                Ok(entries) => {
                    prop_assert!(entries.iter().all(|e| POST_BOX_TIERS.contains(&e.ciphertext.len())));
                }
                Err(MarketError::Slot(_)) => {}
                other => prop_assert!(false, "unexpected error variant on arbitrary post slot: {:?}", other),
            }
        }
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

#[test]
fn test_pack_post_slot_defers_overflow_and_isolates_oversized_entries() {
    use humoco_market::{merge_and_pack_post_slot, pack_post_slot, POST_SLOT_HEADER_BYTES};

    let alice_sk = make_key(11);
    let bob_sk = make_key(12);
    let bob_vk = bob_sk.verifying_key();
    let master_seed = [22u8; 32];

    // Two ~120KB JSON payloads seal into the 256 KB tier; one small payload seals into the 4 KB tier.
    let e1 = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("m1", 40_000)).unwrap();
    let e2 = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("m2", 40_000)).unwrap();
    let e3_small = seal_post_box(&alice_sk, &bob_vk, &sample_envelope("m3", 120)).unwrap();

    assert_eq!(e1.ciphertext.len(), POST_BOX_TIERS[3]);
    assert_eq!(e2.ciphertext.len(), POST_BOX_TIERS[3]);
    assert_eq!(e3_small.ciphertext.len(), POST_BOX_TIERS[0]);

    // Total raw size exceeds 512 KB, so work-conserving FIFO packing includes e1 + e3_small and defers e2.
    let packed = pack_post_slot(
        &master_seed,
        &[e1.clone(), e2.clone(), e3_small.clone()],
        DROP_POST_MAX_BYTES,
    )
    .unwrap();
    assert!(packed.blob.len() <= DROP_POST_MAX_BYTES);
    assert_eq!(packed.included_tags, vec![e1.tag16, e3_small.tag16]);
    assert_eq!(packed.deferred_tags, vec![e2.tag16]);
    assert!(packed.oversized_tags.is_empty());

    let decoded = decode_post_slot(&packed.blob).unwrap();
    assert_eq!(decoded.len(), 2);

    // Oversized single entry (> max_bytes) is reported in oversized_tags without blocking smaller entries.
    let small_cap = 8_192usize;
    let oversized_pack =
        pack_post_slot(&master_seed, &[e1.clone(), e3_small.clone()], small_cap).unwrap();
    assert!(oversized_pack.blob.len() <= small_cap);
    assert!(POST_SLOT_HEADER_BYTES + 44 + e3_small.ciphertext.len() <= small_cap);
    assert_eq!(oversized_pack.included_tags, vec![e3_small.tag16]);
    assert_eq!(oversized_pack.oversized_tags, vec![e1.tag16]);
    assert!(oversized_pack.deferred_tags.is_empty());

    // 412 conflict merge with remote 256 KB box + local 256 KB + local 4 KB must succeed within 512 KB,
    // prioritizing local additions and deferring the overflowing entry.
    let remote_blob =
        encode_post_slot(&master_seed, std::slice::from_ref(&e1), DROP_POST_MAX_BYTES).unwrap();
    let merged = merge_and_pack_post_slot(
        &master_seed,
        Some(&remote_blob),
        &[e2.clone(), e3_small.clone()],
        &[],
        DROP_POST_MAX_BYTES,
    )
    .unwrap();
    assert!(merged.blob.len() <= DROP_POST_MAX_BYTES);
    assert_eq!(merged.included_tags, vec![e2.tag16, e3_small.tag16]);
    assert_eq!(merged.deferred_tags, vec![e1.tag16]);
}

#[test]
fn s02_x3_deny_unknown_fields_on_market_and_post_wire_structs() {
    use humoco_market::{Ad, Skill};

    let ok_skill: Skill = serde_json::from_str(r#"{"title":"Repair"}"#).unwrap();
    assert_eq!(ok_skill.title, "Repair");
    assert_unknown_field_err(
        serde_json::from_str::<Skill>(r#"{"title":"Repair","unknownField":true}"#),
        "unknownField",
    );

    let ok_ad: Ad = serde_json::from_str(
        r#"{"id":"0123456789abcdef","kind":"offer","title":"Honey","publishedAt":100,"expiresAt":200}"#,
    )
    .unwrap();
    assert_eq!(ok_ad.id, "0123456789abcdef");
    assert_unknown_field_err(
        serde_json::from_str::<Ad>(
            r#"{"id":"0123456789abcdef","kind":"offer","title":"Honey","publishedAt":100,"expiresAt":200,"extra":1}"#,
        ),
        "extra",
    );

    let ok_drop: DropDescriptor = serde_json::from_str(
        r#"{"write_url":"https://a/w","read_url":"https://a/r","drop_pub":"aa"}"#,
    )
    .unwrap();
    assert_eq!(ok_drop.drop_pub, "aa");
    assert_unknown_field_err(
        serde_json::from_str::<DropDescriptor>(
            r#"{"write_url":"https://a/w","read_url":"https://a/r","drop_pub":"aa","extra":true}"#,
        ),
        "extra",
    );

    let ok_ad_target: InquiryTarget = serde_json::from_str(
        r#"{"kind":"ad","ad_id":"0123456789abcdef","ad_title":"Honey"}"#,
    )
    .unwrap();
    assert_eq!(ok_ad_target.target_key(), "ad:0123456789abcdef");
    assert_unknown_field_err(
        serde_json::from_str::<InquiryTarget>(
            r#"{"kind":"ad","ad_id":"0123456789abcdef","ad_title":"Honey","extra":1}"#,
        ),
        "extra",
    );

    let ok_person_target: InquiryTarget =
        serde_json::from_str(r#"{"kind":"person","skill_title":"Baking"}"#).unwrap();
    assert_eq!(ok_person_target.target_key(), "person:baking");
    assert_unknown_field_err(
        serde_json::from_str::<InquiryTarget>(
            r#"{"kind":"person","skill_title":"Baking","extra":1}"#,
        ),
        "extra",
    );

    let ok_inq: MarketInquiry = serde_json::from_str(
        r#"{"inquiry_id":"i1","target":{"kind":"person"},"message":"Hi","created_at":100}"#,
    )
    .unwrap();
    assert_eq!(ok_inq.inquiry_id, "i1");
    assert_unknown_field_err(
        serde_json::from_str::<MarketInquiry>(
            r#"{"inquiry_id":"i1","target":{"kind":"person"},"message":"Hi","created_at":100,"extra":1}"#,
        ),
        "extra",
    );

    let ok_receipt: PostDeliveryReceipt = serde_json::from_str(
        r#"{"acknowledged_msg_id":"m1","acknowledged_tag16_hex":"00000000000000000000000000000000","received_at":100}"#,
    )
    .unwrap();
    assert_eq!(ok_receipt.acknowledged_msg_id, "m1");
    assert_unknown_field_err(
        serde_json::from_str::<PostDeliveryReceipt>(
            r#"{"acknowledged_msg_id":"m1","acknowledged_tag16_hex":"00000000000000000000000000000000","received_at":100,"extra":1}"#,
        ),
        "extra",
    );

    let ok_handshake: WotHandshakeAssertion = serde_json::from_str(
        r#"{"sender_did":"did:key:z6Mki","proposed_level":1,"wot_sync_json":"{}","reply_requested":true,"created_at":100}"#,
    )
    .unwrap();
    assert_eq!(ok_handshake.proposed_level, 1);
    assert_unknown_field_err(
        serde_json::from_str::<WotHandshakeAssertion>(
            r#"{"sender_did":"did:key:z6Mki","proposed_level":1,"wot_sync_json":"{}","reply_requested":true,"created_at":100,"extra":1}"#,
        ),
        "extra",
    );
}
