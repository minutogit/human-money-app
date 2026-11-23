use ed25519_dalek::{SigningKey, VerifyingKey};
use human_money_core::crypto::{create_user_id, ed25519_sk_to_x25519_sk};
use humoco_market::{
    accept_envelope_with_writer, build_card_bundle, derive_delta_prefix,
    open_multi_class_slot_blob, open_slot_blob, open_slot_payload, pad_backup_slot_bytes,
    seal_multi_class_slot_blob, seal_slot_blob, seal_slot_payload, sign_card_bundle, sign_sd_card,
    unpad_backup_slot_bytes, validate_sync_prefix, AcceptOutcome, CardEnvelope, CardItem,
    CardPayload, CardPayloadInput, CardStore, MarketError, SdCard, Skill, StoredCard,
    BACKUP_PAD_BLOCK_BYTES, BACKUP_SLOT_MAGIC, CARD_SD_PADDED_LEN, DROP_BACKUP_MAX_BYTES,
    MAX_DECOMPRESSED_SLOT_BYTES, SLOT_MAGIC, SLOT_MULTI_MAGIC,
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

fn assert_identity_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    assert_err_variant(res, "Identity", expected_substr, |e| match e {
        MarketError::Identity(msg) => Some(msg.as_str()),
        _ => None,
    });
}

fn assert_crypto_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    assert_err_variant(res, "Crypto", expected_substr, |e| match e {
        MarketError::Crypto(msg) => Some(msg.as_str()),
        _ => None,
    });
}

fn assert_serialization_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    assert_err_variant(res, "Serialization", expected_substr, |e| match e {
        MarketError::Serialization(msg) => Some(msg.as_str()),
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

fn dummy_sd_card(did: &str, v: u64) -> SdCard {
    SdCard {
        jws: format!("header.{}.signature", did),
        payload: CardPayload {
            did: did.to_string(),
            recipient_did: None,
            v,
            valid_until: 1800000000,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
            _sd: (0..CARD_SD_PADDED_LEN)
                .map(|i| format!("digest_{:02}", i))
                .collect(),
        },
        disclosures: std::collections::BTreeMap::new(),
    }
}

#[test]
fn test_slot_blob_seal_open_and_unauthorized() {
    let writer_sk = SigningKey::from_bytes(&[9u8; 32]);
    let writer_did = create_user_id(&writer_sk.verifying_key(), None).unwrap();

    let reader_sk = SigningKey::from_bytes(&[1u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    let stranger_sk = SigningKey::from_bytes(&[2u8; 32]);
    let stranger_x_sk = ed25519_sk_to_x25519_sk(&stranger_sk);

    let bundle = sign_card_bundle(
        vec![CardEnvelope {
            card_wire: "header.payload.signature~".to_string(),
            hops: 1,
        }],
        &writer_did,
        &writer_sk,
    )
    .unwrap();

    let blob = seal_slot_blob(&bundle, &[reader_vk], 65536).unwrap();
    assert_eq!(&blob[0..8], SLOT_MAGIC);

    // Authorized reader opens successfully
    let opened = open_slot_blob(&blob, &reader_x_sk).unwrap();
    assert_eq!(opened, Some(bundle));

    // Stranger gets Ok(None)
    let stranger_opened = open_slot_blob(&blob, &stranger_x_sk).unwrap();
    assert_eq!(stranger_opened, None);
}

#[test]
fn test_recipient_padding_bucket_sizes() {
    let writer_sk = SigningKey::from_bytes(&[10u8; 32]);
    let writer_did = create_user_id(&writer_sk.verifying_key(), None).unwrap();
    let bundle = sign_card_bundle(vec![], &writer_did, &writer_sk).unwrap();

    let generate_readers = |n: usize| -> Vec<VerifyingKey> {
        (0..n)
            .map(|i| {
                let mut seed = [0u8; 32];
                seed[0..8].copy_from_slice(&(i as u64).to_le_bytes());
                SigningKey::from_bytes(&seed).verifying_key()
            })
            .collect()
    };

    let check_bucket = |reader_count: usize, expected_bucket: usize| {
        let readers = generate_readers(reader_count);
        let blob = seal_slot_blob(&bundle, &readers, 65536).unwrap();
        let bucket = u16::from_be_bytes([blob[8], blob[9]]) as usize;
        assert_eq!(
            bucket, expected_bucket,
            "reader_count: {}, expected: {}, got: {}",
            reader_count, expected_bucket, bucket
        );
    };

    // Bucket boundaries: 0 -> 16, 1 -> 16, 16 -> 16, 17 -> 32, 256 -> 256, 257 -> Err
    check_bucket(0, 16);
    check_bucket(1, 16);
    check_bucket(16, 16);
    check_bucket(17, 32);
    check_bucket(32, 32);
    check_bucket(33, 64);
    check_bucket(64, 64);
    check_bucket(65, 128);
    check_bucket(128, 128);
    check_bucket(129, 256);
    check_bucket(256, 256);

    // 257 readers -> Err(Validation)
    let readers_257 = generate_readers(257);
    assert_validation_err(
        seal_slot_blob(&bundle, &readers_257, 65536),
        "Too many recipients: 257 > 256",
    );
}

#[test]
fn test_slot_blob_contains_no_plaintext_dids() {
    let reader_sk = SigningKey::from_bytes(&[5u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_did = create_user_id(&reader_vk, None).unwrap();

    let author_sk = SigningKey::from_bytes(&[6u8; 32]);
    let author_vk = author_sk.verifying_key();
    let author_did = create_user_id(&author_vk, None).unwrap();

    let bundle = sign_card_bundle(
        vec![CardEnvelope {
            card_wire: format!("header.{}.signature~", author_did),
            hops: 1,
        }],
        &author_did,
        &author_sk,
    )
    .unwrap();

    let blob = seal_slot_blob(&bundle, &[reader_vk], 65536).unwrap();

    // Verify raw blob bytes do not contain "did:key" or author_did or reader_did
    let blob_str = String::from_utf8_lossy(&blob);
    assert!(!blob_str.contains("did:key"));
    assert!(!blob_str.contains(&author_did));
    assert!(!blob_str.contains(&reader_did));
}

#[test]
fn test_build_card_bundle_sorting_and_trimming() {
    let mut store = CardStore::default();

    for i in 1..=30 {
        let mut seed = [0u8; 32];
        seed[0] = i as u8;
        let sk = SigningKey::from_bytes(&seed);
        let vk = sk.verifying_key();
        let did = create_user_id(&vk, None).unwrap();

        let card = dummy_sd_card(&did, i as u64);
        store.cards.insert(
            did,
            StoredCard {
                card,
                card_wire: format!("wire-{}", i),
                hops: 1,
                received_at: 1000,
            },
        );
    }

    let trust_level = |did: &str| -> u8 {
        if did.contains("z") {
            2
        } else {
            1
        }
    };

    let writer_sk = SigningKey::from_bytes(&[77u8; 32]);
    let writer_did = create_user_id(&writer_sk.verifying_key(), None).unwrap();

    let now = 1_700_000_000;
    let card_refs: Vec<&StoredCard> = store.cards.values().collect();
    let bundle = build_card_bundle(
        Some("own-wire~"),
        &card_refs,
        None,
        now,
        &trust_level,
        16,
        524_288,
        &writer_did,
        &writer_sk,
    )
    .unwrap();
    // Own card is first
    assert_eq!(bundle.envelopes[0].card_wire, "own-wire~");
    assert_eq!(bundle.envelopes[0].hops, 1);
    // No 20-card limit: all 1 own + 30 forwarded = 31 cards fit cleanly
    assert_eq!(bundle.envelopes.len(), 31);

    // Quiet friend (own_card_wire == None) still forwards stored cards
    let quiet_bundle = build_card_bundle(
        None,
        &card_refs,
        None,
        now,
        &trust_level,
        16,
        524_288,
        &writer_did,
        &writer_sk,
    )
    .unwrap();
    assert_eq!(quiet_bundle.envelopes.len(), 30);
}

#[test]
fn test_slot_deflate_compression_and_zip_bomb_cap() {
    let writer_sk = SigningKey::from_bytes(&[30u8; 32]);
    let writer_did = create_user_id(&writer_sk.verifying_key(), None).unwrap();

    let reader_sk = SigningKey::from_bytes(&[31u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    // Build repetitive realistic JSON CardBundle (~40 KB raw JSON)
    let envelopes: Vec<CardEnvelope> = (0..100)
        .map(|i| CardEnvelope {
            card_wire: format!(
                "eyJhbGciOiJFZERTQSJ9.eyJkaWQiOiJkaWQ6a2V5OnoxMjM0NTY3ODkwIn0.{:04}~disclosure_abcdef_{:04}~",
                i, i
            ),
            hops: (i % 2 + 1) as u8,
        })
        .collect();
    let bundle = sign_card_bundle(envelopes, &writer_did, &writer_sk).unwrap();
    let raw_json = serde_json::to_vec(&bundle).unwrap();
    let sealed = seal_slot_blob(&bundle, &[reader_vk], 65_536).unwrap();

    // Compressed sealed blob (including 16-recipient overhead ~1.5 KB) must be far smaller than raw JSON
    assert!(
        sealed.len() * 2 < raw_json.len(),
        "Expected Deflate sealed blob ({}) to be < 50% of raw JSON ({})",
        sealed.len(),
        raw_json.len()
    );
    assert_eq!(open_slot_blob(&sealed, &reader_x_sk).unwrap(), Some(bundle));

    // Zip-bomb test: 9 MiB zeros Deflate-compress to ~9 KiB, seal succeeds, open must reject (> 8 MiB limit)
    let oversized_zeros = vec![0u8; MAX_DECOMPRESSED_SLOT_BYTES + 1024 * 1024];
    let bomb_blob = seal_slot_payload(&oversized_zeros, &[reader_vk], 65_536).unwrap();
    let open_err = open_slot_payload(&bomb_blob, &reader_x_sk);
    assert_payload_too_large_err(
        open_err,
        "Decompressed slot payload exceeds 8388608 bytes safety limit",
    );
}

#[test]
fn test_build_card_bundle_fair_hop_interleave_and_geo_remote_priority() {
    let make_signed = |seed_byte: u8, coords: Option<&str>, remote: bool| -> StoredCard {
        let sk = SigningKey::from_bytes(&[seed_byte; 32]);
        let vk = sk.verifying_key();
        let did = create_user_id(&vk, None).unwrap();
        let mut items = Vec::new();
        if let Some(c) = coords {
            items.push((CardItem::Coordinates(c.to_string()), 3));
        }
        if remote {
            items.push((
                CardItem::Skill(Skill {
                    title: "Online tutoring".to_string(),
                    text: None,
                    pricing: None,
                    is_remote: true,
                }),
                3,
            ));
        }
        let card = sign_sd_card(
            CardPayloadInput {
                did,
                v: 100,
                valid_until: 1_800_000_000,
                max_hops: 3,
                ad_count: 0,
                allow_inquiries: true,
                drops: vec![],
            },
            items,
            &sk,
        )
        .unwrap();
        StoredCard {
            card,
            card_wire: format!("wire-{:02}", seed_byte),
            hops: if seed_byte < 50 { 1 } else { 2 },
            received_at: 1000,
        }
    };

    // Hop 1: near Berlin (~0km) vs far Munich (~500km) vs unknown remote
    let h1_far = make_signed(10, Some("48.1351, 11.5820"), false);
    let h1_near = make_signed(11, Some("52.5200, 13.4050"), false);
    // Hop 2: one card with remote skill
    let h2_remote = make_signed(60, Some("48.1351, 11.5820"), true);
    let h2_far = make_signed(61, Some("48.1351, 11.5820"), false);

    let h1_near_did = h1_near.card.payload.did.clone();
    let h1_far_did = h1_far.card.payload.did.clone();
    let h2_remote_did = h2_remote.card.payload.did.clone();
    let h2_far_did = h2_far.card.payload.did.clone();

    let writer_sk = SigningKey::from_bytes(&[78u8; 32]);
    let writer_did = create_user_id(&writer_sk.verifying_key(), None).unwrap();
    let refs = vec![&h1_far, &h1_near, &h2_far, &h2_remote];
    let trust_level = |_did: &str| -> u8 { 2 };
    let berlin = Some((52.52, 13.405));
    let bundle = build_card_bundle(
        Some("own~"),
        &refs,
        berlin,
        1_700_000_000,
        &trust_level,
        16,
        524_288,
        &writer_did,
        &writer_sk,
    )
    .unwrap();

    // Expect fair 50/50 interleave with geo/remote priority:
    // [own, H1_near, H2_remote, H1_far, H2_far]
    assert_eq!(bundle.envelopes.len(), 5);
    assert_eq!(bundle.envelopes[0].card_wire, "own~");
    assert_eq!(bundle.envelopes[1].hops, 2);
    assert_eq!(bundle.envelopes[2].hops, 3);
    assert_eq!(bundle.envelopes[3].hops, 2);
    assert_eq!(bundle.envelopes[4].hops, 3);
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[1].card_wire)
            .unwrap()
            .payload
            .did
            == h1_near_did
    );
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[2].card_wire)
            .unwrap()
            .payload
            .did
            == h2_remote_did
    );
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[3].card_wire)
            .unwrap()
            .payload
            .did
            == h1_far_did
    );
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[4].card_wire)
            .unwrap()
            .payload
            .did
            == h2_far_did
    );
}

#[test]
fn test_derive_delta_prefix_valid_under_sync_spec() {
    assert_eq!(derive_delta_prefix(None), "delta");
    assert_eq!(derive_delta_prefix(Some("")), "delta");
    assert_eq!(derive_delta_prefix(Some("work")), "work-delta");

    let long_prefix = "a".repeat(63);
    let derived = derive_delta_prefix(Some(&long_prefix));
    assert!(derived.len() <= 63);
    assert!(derived.ends_with("-delta"));
    assert!(validate_sync_prefix(Some(&derived)).is_ok());
}

#[test]
fn test_slot_payload_raw_bytes_roundtrip() {
    let reader_sk = SigningKey::from_bytes(&[7u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    let stranger_sk = SigningKey::from_bytes(&[8u8; 32]);
    let stranger_x_sk = ed25519_sk_to_x25519_sk(&stranger_sk);

    let raw_payload = b"arbitrary-binary-payload-for-wot-or-backup-0123456789";
    let blob = seal_slot_payload(raw_payload, &[reader_vk], 65536).unwrap();

    let opened = open_slot_payload(&blob, &reader_x_sk).unwrap();
    assert_eq!(opened, Some(raw_payload.to_vec()));

    let stranger_opened = open_slot_payload(&blob, &stranger_x_sk).unwrap();
    assert_eq!(stranger_opened, None);
}

#[test]
fn test_multi_class_slot_blob_separation_and_privacy() {
    let reader1_sk = SigningKey::from_bytes(&[11u8; 32]);
    let reader1_vk = reader1_sk.verifying_key();
    let reader1_x_sk = ed25519_sk_to_x25519_sk(&reader1_sk);

    let reader2_sk = SigningKey::from_bytes(&[12u8; 32]);
    let reader2_vk = reader2_sk.verifying_key();
    let reader2_x_sk = ed25519_sk_to_x25519_sk(&reader2_sk);

    let reader3_sk = SigningKey::from_bytes(&[13u8; 32]);
    let reader3_vk = reader3_sk.verifying_key();
    let reader3_x_sk = ed25519_sk_to_x25519_sk(&reader3_sk);

    let stranger_sk = SigningKey::from_bytes(&[99u8; 32]);
    let stranger_x_sk = ed25519_sk_to_x25519_sk(&stranger_sk);

    let payload1 = b"CLASS_1_FULL_WOT_DIRECT_FRIENDS";
    let payload2 = b"CLASS_2_REDACTED_WOT_HOP2";
    let payload3 = b"CLASS_3_REDACTED_WOT_HOP3";

    let classes: Vec<(&[u8], &[VerifyingKey])> = vec![
        (payload1, std::slice::from_ref(&reader1_vk)),
        (payload2, std::slice::from_ref(&reader2_vk)),
        (payload3, std::slice::from_ref(&reader3_vk)),
    ];

    let multi_blob = seal_multi_class_slot_blob(&classes, 131_072).unwrap();
    assert_eq!(&multi_blob[0..8], SLOT_MULTI_MAGIC);
    assert_eq!(multi_blob[8], 3);

    // Class 1 reader sees Class 1 payload
    let open1 = open_multi_class_slot_blob(&multi_blob, &reader1_x_sk).unwrap();
    assert_eq!(open1, Some(payload1.to_vec()));

    // Class 2 reader sees Class 2 payload (CANNOT see Class 1 payload!)
    let open2 = open_multi_class_slot_blob(&multi_blob, &reader2_x_sk).unwrap();
    assert_eq!(open2, Some(payload2.to_vec()));

    // Class 3 reader sees Class 3 payload
    let open3 = open_multi_class_slot_blob(&multi_blob, &reader3_x_sk).unwrap();
    assert_eq!(open3, Some(payload3.to_vec()));

    // Outsider cannot decrypt anything
    let open_stranger = open_multi_class_slot_blob(&multi_blob, &stranger_x_sk).unwrap();
    assert_eq!(open_stranger, None);
}

#[test]
fn test_multi_class_slot_blob_validation_and_errors() {
    let reader_sk = SigningKey::from_bytes(&[21u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    let payload = b"hello";

    // 0 classes -> Err(Validation)
    let err_empty = seal_multi_class_slot_blob(&[], 65536);
    assert_validation_err(err_empty, "Classes count must be between 1 and 4, got 0");

    // 5 classes -> Err(Validation) (1..=4 supported)
    let five_classes: Vec<(&[u8], &[VerifyingKey])> = vec![
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
    ];
    let err_five = seal_multi_class_slot_blob(&five_classes, 131_072);
    assert_validation_err(err_five, "Classes count must be between 1 and 4, got 5");

    // Class with empty readers -> Err(Validation)
    let empty_readers: Vec<(&[u8], &[VerifyingKey])> = vec![(payload, &[])];
    let err_readers = seal_multi_class_slot_blob(&empty_readers, 65536);
    assert_validation_err(err_readers, "Class 1 has empty reader list");

    // Payload too large (single sub-blob budget & combined multi-class budget)
    let valid_single: Vec<(&[u8], &[VerifyingKey])> =
        vec![(payload, std::slice::from_ref(&reader_vk))];
    let err_too_large = seal_multi_class_slot_blob(&valid_single, 100);
    assert_payload_too_large_err(err_too_large, "exceeds max_bytes 100");

    let single_sub_blob =
        seal_slot_payload(payload, std::slice::from_ref(&reader_vk), 65_536).unwrap();
    let two_class_cap = single_sub_blob.len() + 50;
    let two_classes: Vec<(&[u8], &[VerifyingKey])> = vec![
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
    ];
    let err_combined = seal_multi_class_slot_blob(&two_classes, two_class_cap);
    assert_payload_too_large_err(
        err_combined,
        &format!("exceeds max_total_bytes {}", two_class_cap),
    );

    // Positive baseline on valid multi-class blob before negative header/truncation checks
    let valid_blob = seal_multi_class_slot_blob(&valid_single, 65536).unwrap();
    assert_eq!(
        open_multi_class_slot_blob(&valid_blob, &reader_x_sk).unwrap(),
        Some(payload.to_vec())
    );

    // Too short (< 9 bytes)
    assert_slot_err(
        open_multi_class_slot_blob(&valid_blob[0..8], &reader_x_sk),
        "Multi-class SlotBlob too short",
    );

    // Bad magic
    let mut bad_magic = valid_blob.clone();
    bad_magic[0] = b'X';
    assert_slot_err(
        open_multi_class_slot_blob(&bad_magic, &reader_x_sk),
        "Invalid multi-class SlotBlob magic header",
    );

    // Count is 0
    let mut bad_count = valid_blob.clone();
    bad_count[8] = 0;
    assert_slot_err(
        open_multi_class_slot_blob(&bad_count, &reader_x_sk),
        "Invalid multi-class count: 0",
    );

    // Count is 5 (max is 4)
    let mut bad_count5 = valid_blob.clone();
    bad_count5[8] = 5;
    assert_slot_err(
        open_multi_class_slot_blob(&bad_count5, &reader_x_sk),
        "Invalid multi-class count: 5",
    );

    // Truncated during length header (9..13) or sub-blob body (13..len)
    for len in 9..13 {
        assert_slot_err(
            open_multi_class_slot_blob(&valid_blob[..len], &reader_x_sk),
            "Truncated class length in multi-class SlotBlob",
        );
    }
    for len in 13..valid_blob.len() {
        assert_slot_err(
            open_multi_class_slot_blob(&valid_blob[..len], &reader_x_sk),
            "Truncated sub-blob in multi-class SlotBlob",
        );
    }

    // Trailing unexpected bytes rejected
    let mut trailing_multi = valid_blob.clone();
    trailing_multi.push(0xAA);
    assert_slot_err(
        open_multi_class_slot_blob(&trailing_multi, &reader_x_sk),
        "Trailing unexpected bytes in multi-class SlotBlob",
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn proptest_open_multi_class_arbitrary_bytes_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let sk = SigningKey::from_bytes(&[42u8; 32]);
        let x_sk = ed25519_sk_to_x25519_sk(&sk);
        let res = open_multi_class_slot_blob(&bytes, &x_sk);
        if bytes.len() < 9 {
            prop_assert!(
                matches!(&res, Err(MarketError::Slot(msg)) if msg.contains("Multi-class SlotBlob too short")),
                "expected short error for len={}, got {:?}",
                bytes.len(),
                res
            );
        } else if &bytes[0..8] != SLOT_MULTI_MAGIC {
            prop_assert!(
                matches!(&res, Err(MarketError::Slot(msg)) if msg.contains("Invalid multi-class SlotBlob magic header")),
                "expected bad magic error, got {:?}",
                res
            );
        } else {
            prop_assert!(
                matches!(res, Err(MarketError::Slot(_)) | Err(MarketError::Crypto(_)) | Ok(None)),
                "unexpected outcome on arbitrary multi-class bytes: {:?}",
                res
            );
        }
    }

    #[test]
    fn proptest_backup_slot_padding_roundtrip(bytes in prop::collection::vec(any::<u8>(), 1..70_000)) {
        let padded = pad_backup_slot_bytes(&bytes, DROP_BACKUP_MAX_BYTES).unwrap();
        prop_assert_eq!(padded.len() % BACKUP_PAD_BLOCK_BYTES, 0);
        let unpadded = unpad_backup_slot_bytes(&padded).unwrap();
        prop_assert_eq!(unpadded, bytes);
    }

    #[test]
    fn proptest_unpad_backup_slot_arbitrary_bytes_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let res = unpad_backup_slot_bytes(&bytes);
        prop_assert!(
            matches!(&res, Err(MarketError::Slot(msg)) if msg.contains("Invalid backup slot size:")),
            "expected Invalid backup slot size for len={}, got {:?}",
            bytes.len(),
            res
        );
    }
}

#[test]
fn test_backup_slot_padding_exact_multiples_across_sizes() {
    use rand::{rngs::OsRng, RngCore};

    // Highly compressible 200 KB payload -> shrinks well below 64 KB after Deflate
    let compressible = vec![0x42u8; 200 * 1024];
    let padded_comp = pad_backup_slot_bytes(&compressible, DROP_BACKUP_MAX_BYTES).unwrap();
    assert_eq!(padded_comp.len(), BACKUP_PAD_BLOCK_BYTES);
    assert_eq!(&padded_comp[0..8], BACKUP_SLOT_MAGIC);
    assert_eq!(unpad_backup_slot_bytes(&padded_comp).unwrap(), compressible);

    // Incompressible 70 KB payload -> stays > 65,536 bytes -> rounds up to 2 * 64 KB
    let mut random_payload = vec![0u8; 70 * 1024];
    OsRng.fill_bytes(&mut random_payload);
    let padded_rand = pad_backup_slot_bytes(&random_payload, DROP_BACKUP_MAX_BYTES).unwrap();
    assert_eq!(padded_rand.len(), 2 * BACKUP_PAD_BLOCK_BYTES);
    assert_eq!(padded_rand.len() % BACKUP_PAD_BLOCK_BYTES, 0);
    assert_eq!(&padded_rand[0..8], BACKUP_SLOT_MAGIC);
    assert_eq!(
        unpad_backup_slot_bytes(&padded_rand).unwrap(),
        random_payload
    );
}

#[test]
fn test_backup_slot_padding_rejections_and_limits() {
    use rand::{rngs::OsRng, RngCore};

    // Empty payload rejected
    assert_validation_err(
        pad_backup_slot_bytes(&[], DROP_BACKUP_MAX_BYTES),
        "Backup payload cannot be empty",
    );

    // Exceeds max_bytes (incompressible 70 KB cannot fit in 64 KB)
    let mut payload = vec![0u8; 70 * 1024];
    OsRng.fill_bytes(&mut payload);
    assert_payload_too_large_err(
        pad_backup_slot_bytes(&payload, 65_536),
        "Padded backup slot size 131072 exceeds max allowed 65536",
    );

    // Positive baseline: valid padding roundtrips cleanly
    let valid_padded = pad_backup_slot_bytes(&[0x77u8; 100], DROP_BACKUP_MAX_BYTES).unwrap();
    assert_eq!(unpad_backup_slot_bytes(&valid_padded).unwrap(), vec![0x77u8; 100]);

    // Rejection: size too short (< 65536)
    assert_slot_err(
        unpad_backup_slot_bytes(&valid_padded[0..100]),
        "Invalid backup slot size: 100",
    );

    // Rejection: size not a multiple of 65,536
    let mut bad_len = valid_padded.clone();
    bad_len.push(0);
    assert_slot_err(
        unpad_backup_slot_bytes(&bad_len),
        "Invalid backup slot size: 65537",
    );

    // Rejection: bad magic
    let mut bad_magic = valid_padded.clone();
    bad_magic[0] = b'X';
    assert_slot_err(
        unpad_backup_slot_bytes(&bad_magic),
        "Invalid backup slot magic header",
    );

    // Rejection: inner_len == 0
    let mut bad_zero_len = valid_padded.clone();
    bad_zero_len[8..12].copy_from_slice(&0u32.to_be_bytes());
    assert_slot_err(
        unpad_backup_slot_bytes(&bad_zero_len),
        "Invalid inner backup payload length: 0 in slot size 65536",
    );

    // Rejection: inner_len exceeds slot length (off-by-one boundary 65525 -> 12 + 65525 > 65536, plus huge)
    let mut bad_boundary_len = valid_padded.clone();
    bad_boundary_len[8..12].copy_from_slice(&65_525u32.to_be_bytes());
    assert_slot_err(
        unpad_backup_slot_bytes(&bad_boundary_len),
        "Invalid inner backup payload length: 65525 in slot size 65536",
    );

    let mut bad_huge_len = valid_padded.clone();
    bad_huge_len[8..12].copy_from_slice(&1_000_000u32.to_be_bytes());
    assert_slot_err(
        unpad_backup_slot_bytes(&bad_huge_len),
        "Invalid inner backup payload length: 1000000 in slot size 65536",
    );
}

#[test]
fn test_open_slot_blob_supports_multi_class_tombstone_separation() {
    let active_sk = SigningKey::from_bytes(&[11u8; 32]);
    let active_vk = active_sk.verifying_key();
    let active_x_sk = ed25519_sk_to_x25519_sk(&active_sk);

    let evicted_sk = SigningKey::from_bytes(&[22u8; 32]);
    let evicted_vk = evicted_sk.verifying_key();
    let evicted_x_sk = ed25519_sk_to_x25519_sk(&evicted_sk);

    let author_sk = SigningKey::from_bytes(&[33u8; 32]);
    let author_did = create_user_id(&author_sk.verifying_key(), None).unwrap();
    let card = sign_sd_card(
        CardPayloadInput {
            did: author_did.clone(),
            v: 200,
            valid_until: 1_900_000_000,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
        },
        vec![(
            CardItem::Name {
                first_name: Some("Alice".to_string()),
                last_name: None,
            },
            2,
        )],
        &author_sk,
    )
    .unwrap();

    let full_bundle = sign_card_bundle(
        vec![CardEnvelope {
            hops: 1,
            card_wire: humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full)
                .unwrap(),
        }],
        &author_did,
        &author_sk,
    )
    .unwrap();
    let tomb_bundle = sign_card_bundle(
        vec![CardEnvelope {
            hops: 1,
            card_wire: format!("{}~", card.jws),
        }],
        &author_did,
        &author_sk,
    )
    .unwrap();
    let full_bytes = serde_json::to_vec(&full_bundle).unwrap();
    let tomb_bytes = serde_json::to_vec(&tomb_bundle).unwrap();

    let blob = seal_multi_class_slot_blob(
        &[
            (&full_bytes, std::slice::from_ref(&active_vk)),
            (&tomb_bytes, std::slice::from_ref(&evicted_vk)),
        ],
        131_072,
    )
    .unwrap();

    let active_opened = open_slot_blob(&blob, &active_x_sk).unwrap().unwrap();
    assert_eq!(active_opened, full_bundle);

    let evicted_opened = open_slot_blob(&blob, &evicted_x_sk).unwrap().unwrap();
    assert_eq!(evicted_opened, tomb_bundle);
}

#[test]
fn s02_x1_card_bundle_writer_attestation_rejects_tampered_and_mismatched_signers() {
    let writer_sk = SigningKey::from_bytes(&[41u8; 32]);
    let writer_did = create_user_id(&writer_sk.verifying_key(), Some("shop")).unwrap();
    let other_sk = SigningKey::from_bytes(&[42u8; 32]);
    let other_did = create_user_id(&other_sk.verifying_key(), None).unwrap();

    let reader_sk = SigningKey::from_bytes(&[43u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    // 1. Mismatched signing key vs writer_did rejected at sign & build time
    assert_identity_err(
        sign_card_bundle(vec![], &writer_did, &other_sk),
        "CardBundle signing_key does not match writer_did public key",
    );
    assert_identity_err(
        build_card_bundle(
            None,
            &[],
            None,
            1_700_000_000,
            &|_| 1,
            16,
            65_536,
            &writer_did,
            &other_sk,
        ),
        "CardBundle signing_key does not match writer_did public key",
    );
    assert_identity_err(
        sign_card_bundle(vec![], &format!(" {}", writer_did), &writer_sk),
        "CardBundle writer_did must be canonical without outer whitespace",
    );

    // Positive baseline: validly signed bundle seals and opens cleanly
    let valid_bundle = sign_card_bundle(
        vec![CardEnvelope {
            card_wire: "hdr.pay.sig~".to_string(),
            hops: 1,
        }],
        &writer_did,
        &writer_sk,
    )
    .unwrap();
    let valid_blob = seal_slot_blob(&valid_bundle, &[reader_vk], 65_536).unwrap();
    assert_eq!(
        open_slot_blob(&valid_blob, &reader_x_sk).unwrap(),
        Some(valid_bundle.clone())
    );

    // 2. Tampered writer_did (after signing) fails seal & open signature verification
    let mut tampered_did = valid_bundle.clone();
    tampered_did.writer_did = other_did.clone();
    assert_crypto_err(
        seal_slot_blob(&tampered_did, &[reader_vk], 65_536),
        "CardBundle writer signature verification failed",
    );
    let tampered_did_blob = seal_slot_payload(
        &serde_json::to_vec(&tampered_did).unwrap(),
        &[reader_vk],
        65_536,
    )
    .unwrap();
    assert_crypto_err(
        open_slot_blob(&tampered_did_blob, &reader_x_sk),
        "CardBundle writer signature verification failed",
    );

    // 3. Tampered writer_sig (flipped nibble & uppercase hex) fails open
    let mut tampered_sig = valid_bundle.clone();
    let flipped_char = if tampered_sig.writer_sig.starts_with('a') {
        "b"
    } else {
        "a"
    };
    tampered_sig.writer_sig = format!("{}{}", flipped_char, &tampered_sig.writer_sig[1..]);
    let tampered_sig_blob = seal_slot_payload(
        &serde_json::to_vec(&tampered_sig).unwrap(),
        &[reader_vk],
        65_536,
    )
    .unwrap();
    assert_crypto_err(
        open_slot_blob(&tampered_sig_blob, &reader_x_sk),
        "CardBundle writer signature verification failed",
    );

    let mut upper_sig = valid_bundle.clone();
    upper_sig.writer_sig = upper_sig.writer_sig.to_ascii_uppercase();
    let upper_sig_blob = seal_slot_payload(
        &serde_json::to_vec(&upper_sig).unwrap(),
        &[reader_vk],
        65_536,
    )
    .unwrap();
    assert_crypto_err(
        open_slot_blob(&upper_sig_blob, &reader_x_sk),
        "CardBundle writer_sig must be a 128-character lowercase hex string",
    );

    // 4. Tampered envelope hops fails open signature verification
    let mut tampered_hops = valid_bundle.clone();
    tampered_hops.envelopes[0].hops = 2;
    let tampered_hops_blob = seal_slot_payload(
        &serde_json::to_vec(&tampered_hops).unwrap(),
        &[reader_vk],
        65_536,
    )
    .unwrap();
    assert_crypto_err(
        open_slot_blob(&tampered_hops_blob, &reader_x_sk),
        "CardBundle writer signature verification failed",
    );

    // 5. Legacy unsigned JSON & unknown JSON fields rejected by #[serde(deny_unknown_fields)]
    let legacy_json = br#"{"envelopes":[{"card_wire":"hdr.pay.sig~","hops":1}]}"#;
    let legacy_blob = seal_slot_payload(legacy_json, &[reader_vk], 65_536).unwrap();
    assert_serialization_err(
        open_slot_blob(&legacy_blob, &reader_x_sk),
        "missing field `writer_did`",
    );

    let unknown_field_json = format!(
        r#"{{"writer_did":"{}","writer_sig":"{}","envelopes":[],"extra":1}}"#,
        valid_bundle.writer_did, valid_bundle.writer_sig
    );
    let unknown_blob =
        seal_slot_payload(unknown_field_json.as_bytes(), &[reader_vk], 65_536).unwrap();
    assert_serialization_err(
        open_slot_blob(&unknown_blob, &reader_x_sk),
        "unknown field `extra`",
    );
}

fn sign_rendered_card(
    did: &str,
    sk: &SigningKey,
    valid_until: u64,
    max_hops: u8,
    items: Vec<(CardItem, u8)>,
) -> String {
    let card = sign_sd_card(
        CardPayloadInput {
            did: did.to_string(),
            v: 10,
            valid_until,
            max_hops,
            ad_count: 0,
            allow_inquiries: false,
            drops: vec![],
        },
        items,
        sk,
    )
    .unwrap();
    humoco_market::render_wire(&card, 1, humoco_market::WireFilter::Full).unwrap()
}

#[test]
fn s02_x1_forwarded_card_claiming_hops_1_clamped_by_writer_did() {
    let now = 1_700_000_000u64;
    let valid_until = now + 30 * 86_400;

    let bob_sk = SigningKey::from_bytes(&[51u8; 32]);
    let bob_did = create_user_id(&bob_sk.verifying_key(), Some("bob")).unwrap();

    let carol_sk = SigningKey::from_bytes(&[52u8; 32]);
    let carol_did = create_user_id(&carol_sk.verifying_key(), None).unwrap();

    let dave_sk = SigningKey::from_bytes(&[53u8; 32]);
    let dave_did = create_user_id(&dave_sk.verifying_key(), None).unwrap();

    let alice_sk = SigningKey::from_bytes(&[54u8; 32]);
    let alice_vk = alice_sk.verifying_key();
    let alice_x_sk = ed25519_sk_to_x25519_sk(&alice_sk);

    let carol_wire = sign_rendered_card(
        &carol_did,
        &carol_sk,
        valid_until,
        2,
        vec![
            (
                CardItem::Name {
                    first_name: Some("Carol".to_string()),
                    last_name: None,
                },
                2,
            ),
            (CardItem::Phone("+49-123".to_string()), 1),
        ],
    );
    let dave_wire = sign_rendered_card(
        &dave_did,
        &dave_sk,
        valid_until,
        1,
        vec![(
            CardItem::Name {
                first_name: Some("Dave".to_string()),
                last_name: None,
            },
            1,
        )],
    );
    let bob_card_did = create_user_id(&bob_sk.verifying_key(), Some("shop")).unwrap();
    let bob_wire = sign_rendered_card(
        &bob_card_did,
        &bob_sk,
        valid_until,
        2,
        vec![(
            CardItem::Name {
                first_name: Some("Bob".to_string()),
                last_name: None,
            },
            1,
        )],
    );

    // Malicious Bob wraps all three cards in HIS signed CardBundle claiming hops: 1 for everyone
    let bundle = sign_card_bundle(
        vec![
            CardEnvelope {
                card_wire: bob_wire,
                hops: 1,
            },
            CardEnvelope {
                card_wire: carol_wire,
                hops: 1,
            },
            CardEnvelope {
                card_wire: dave_wire,
                hops: 1,
            },
        ],
        &bob_did,
        &bob_sk,
    )
    .unwrap();

    let blob = seal_slot_blob(&bundle, &[alice_vk], 65_536).unwrap();
    let opened = open_slot_blob(&blob, &alice_x_sk).unwrap().unwrap();
    assert_eq!(opened.writer_did, bob_did);

    let mut store = CardStore::default();

    // 1. Bob's own card accepted at hops = 1 (same root key as writer_did)
    let res_bob = accept_envelope_with_writer(
        &mut store,
        &opened.envelopes[0],
        Some(opened.writer_did.as_str()),
        now,
        100,
        None,
        &|_| false,
    );
    assert_eq!(
        res_bob,
        AcceptOutcome::Accepted {
            did: bob_card_did.clone(),
            replaced: false,
        }
    );
    assert_eq!(store.cards.get(&bob_card_did).unwrap().hops, 1);

    // 2. Carol's card claiming hops: 1 is clamped to effective_hops = 2, and hop-1 phone is stripped!
    let res_carol = accept_envelope_with_writer(
        &mut store,
        &opened.envelopes[1],
        Some(opened.writer_did.as_str()),
        now,
        100,
        None,
        &|_| false,
    );
    assert_eq!(
        res_carol,
        AcceptOutcome::Accepted {
            did: carol_did.clone(),
            replaced: false,
        }
    );
    let stored_carol = store.cards.get(&carol_did).unwrap();
    assert_eq!(stored_carol.hops, 2);
    let carol_items = humoco_market::decoded_items(&stored_carol.card);
    assert!(
        !carol_items
            .iter()
            .any(|(item, _)| matches!(item, CardItem::Phone(_))),
        "Hop-1-only Phone disclosure must be stripped when clamped to hops = 2"
    );

    // 3. Dave's max_hops=1 card claiming hops: 1 is rejected with RejectedHops!
    let res_dave = accept_envelope_with_writer(
        &mut store,
        &opened.envelopes[2],
        Some(opened.writer_did.as_str()),
        now,
        100,
        None,
        &|_| false,
    );
    assert_eq!(res_dave, AcceptOutcome::RejectedHops);
    assert!(!store.cards.contains_key(&dave_did));
}
