use ed25519_dalek::{SigningKey, VerifyingKey};
use human_money_core::crypto::{create_user_id, ed25519_sk_to_x25519_sk};
use humoco_market::{
    build_card_bundle, derive_delta_prefix, open_multi_class_slot_blob, open_slot_blob,
    open_slot_payload, pad_backup_slot_bytes, seal_multi_class_slot_blob, seal_slot_blob,
    seal_slot_payload, sign_sd_card, unpad_backup_slot_bytes, validate_sync_prefix, CardBundle,
    CardEnvelope, CardItem, CardPayload, CardPayloadInput, CardStore, MarketError, SdCard, Skill,
    StoredCard, BACKUP_PAD_BLOCK_BYTES, BACKUP_SLOT_MAGIC, CARD_SD_PADDED_LEN,
    DROP_BACKUP_MAX_BYTES, MAX_DECOMPRESSED_SLOT_BYTES, SLOT_MAGIC, SLOT_MULTI_MAGIC,
};
use proptest::prelude::*;

fn dummy_sd_card(did: &str, v: u64) -> SdCard {
    SdCard {
        jws: format!("header.{}.signature", did),
        payload: CardPayload {
            did: did.to_string(),
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
    let reader_sk = SigningKey::from_bytes(&[1u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_x_sk = ed25519_sk_to_x25519_sk(&reader_sk);

    let stranger_sk = SigningKey::from_bytes(&[2u8; 32]);
    let stranger_x_sk = ed25519_sk_to_x25519_sk(&stranger_sk);

    let bundle = CardBundle {
        envelopes: vec![CardEnvelope {
            card_wire: "header.payload.signature~".to_string(),
            hops: 1,
        }],
    };

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
    let bundle = CardBundle { envelopes: vec![] };

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

    // 257 readers -> Err
    let readers_257 = generate_readers(257);
    assert!(seal_slot_blob(&bundle, &readers_257, 65536).is_err());
}

#[test]
fn test_slot_blob_contains_no_plaintext_dids() {
    let reader_sk = SigningKey::from_bytes(&[5u8; 32]);
    let reader_vk = reader_sk.verifying_key();
    let reader_did = create_user_id(&reader_vk, None).unwrap();

    let author_sk = SigningKey::from_bytes(&[6u8; 32]);
    let author_vk = author_sk.verifying_key();
    let author_did = create_user_id(&author_vk, None).unwrap();

    let bundle = CardBundle {
        envelopes: vec![CardEnvelope {
            card_wire: format!("header.{}.signature~", author_did),
            hops: 1,
        }],
    };

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
            *vk.as_bytes(),
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

    let now = 1_700_000_000;
    let card_refs: Vec<&StoredCard> = store.cards.values().collect();
    let bundle = build_card_bundle(Some("own-wire~"), &card_refs, None, now, &trust_level, 16, 524_288);
    // Own card is first
    assert_eq!(bundle.envelopes[0].card_wire, "own-wire~");
    assert_eq!(bundle.envelopes[0].hops, 1);
    // No 20-card limit: all 1 own + 30 forwarded = 31 cards fit cleanly
    assert_eq!(bundle.envelopes.len(), 31);

    // Quiet friend (own_card_wire == None) still forwards stored cards
    let quiet_bundle = build_card_bundle(None, &card_refs, None, now, &trust_level, 16, 524_288);
    assert_eq!(quiet_bundle.envelopes.len(), 30);
}

#[test]
fn test_slot_deflate_compression_and_zip_bomb_cap() {
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
    let bundle = CardBundle { envelopes };
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
    assert!(matches!(open_err, Err(MarketError::PayloadTooLarge(_))));
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

    let refs = vec![&h1_far, &h1_near, &h2_far, &h2_remote];
    let trust_level = |_did: &str| -> u8 { 2 };
    let berlin = Some((52.52, 13.405));
    let bundle = build_card_bundle(Some("own~"), &refs, berlin, 1_700_000_000, &trust_level, 16, 524_288);

    // Expect fair 50/50 interleave with geo/remote priority:
    // [own, H1_near, H2_remote, H1_far, H2_far]
    assert_eq!(bundle.envelopes.len(), 5);
    assert_eq!(bundle.envelopes[0].card_wire, "own~");
    assert_eq!(bundle.envelopes[1].hops, 2);
    assert_eq!(bundle.envelopes[2].hops, 3);
    assert_eq!(bundle.envelopes[3].hops, 2);
    assert_eq!(bundle.envelopes[4].hops, 3);
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[1].card_wire).unwrap().payload.did == h1_near_did
    );
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[2].card_wire).unwrap().payload.did == h2_remote_did
    );
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[3].card_wire).unwrap().payload.did == h1_far_did
    );
    assert!(
        humoco_market::verify_sd_card(&bundle.envelopes[4].card_wire).unwrap().payload.did == h2_far_did
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
    assert!(matches!(err_empty, Err(MarketError::Validation(_))));

    // 4 classes -> Err(Validation)
    let four_classes: Vec<(&[u8], &[VerifyingKey])> = vec![
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
        (payload, std::slice::from_ref(&reader_vk)),
    ];
    let err_four = seal_multi_class_slot_blob(&four_classes, 131_072);
    assert!(matches!(err_four, Err(MarketError::Validation(_))));

    // Class with empty readers -> Err(Validation)
    let empty_readers: Vec<(&[u8], &[VerifyingKey])> = vec![(payload, &[])];
    let err_readers = seal_multi_class_slot_blob(&empty_readers, 65536);
    assert!(matches!(err_readers, Err(MarketError::Validation(_))));

    // Payload too large
    let valid_single: Vec<(&[u8], &[VerifyingKey])> = vec![(payload, std::slice::from_ref(&reader_vk))];
    let err_too_large = seal_multi_class_slot_blob(&valid_single, 100);
    assert!(matches!(err_too_large, Err(MarketError::PayloadTooLarge(_))));

    // Truncated buffers
    let valid_blob = seal_multi_class_slot_blob(&valid_single, 65536).unwrap();

    // Too short (< 9 bytes)
    assert!(open_multi_class_slot_blob(&valid_blob[0..8], &reader_x_sk).is_err());

    // Bad magic
    let mut bad_magic = valid_blob.clone();
    bad_magic[0] = b'X';
    assert!(open_multi_class_slot_blob(&bad_magic, &reader_x_sk).is_err());

    // Count is 0
    let mut bad_count = valid_blob.clone();
    bad_count[8] = 0;
    assert!(open_multi_class_slot_blob(&bad_count, &reader_x_sk).is_err());

    // Count is 4
    let mut bad_count4 = valid_blob.clone();
    bad_count4[8] = 4;
    assert!(open_multi_class_slot_blob(&bad_count4, &reader_x_sk).is_err());

    // Truncated during length header or sub-blob
    for len in 9..valid_blob.len() - 1 {
        let truncated = &valid_blob[..len];
        assert!(open_multi_class_slot_blob(truncated, &reader_x_sk).is_err());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn proptest_open_multi_class_arbitrary_bytes_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let sk = SigningKey::from_bytes(&[42u8; 32]);
        let x_sk = ed25519_sk_to_x25519_sk(&sk);
        // Must never panic on random input
        let _ = open_multi_class_slot_blob(&bytes, &x_sk);
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
        let _ = unpad_backup_slot_bytes(&bytes);
    }
}

#[test]
fn test_backup_slot_padding_exact_multiples_across_sizes() {
    let check_size = |size: usize, expected_padded_len: usize| {
        let payload = vec![0x42u8; size];
        let padded = pad_backup_slot_bytes(&payload, DROP_BACKUP_MAX_BYTES).unwrap();
        assert_eq!(padded.len(), expected_padded_len);
        assert_eq!(padded.len() % BACKUP_PAD_BLOCK_BYTES, 0);
        assert_eq!(&padded[0..8], BACKUP_SLOT_MAGIC);
        let unpadded = unpad_backup_slot_bytes(&padded).unwrap();
        assert_eq!(unpadded, payload);
    };

    // 1 Byte -> 12 + 1 = 13 bytes -> rounded up to 65,536 bytes
    check_size(1, 65_536);

    // 65,524 Bytes -> 12 + 65,524 = 65,536 bytes -> exactly 65,536 bytes
    check_size(65_524, 65_536);

    // 65,525 Bytes -> 12 + 65,525 = 65,537 bytes -> rounded up to 131,072 bytes
    check_size(65_525, 131_072);

    // 200 KB = 204,800 bytes -> 12 + 204,800 = 204,812 bytes -> 4 * 65,536 = 262,144 bytes
    check_size(200 * 1024, 262_144);
}

#[test]
fn test_backup_slot_padding_rejections_and_limits() {
    // Empty payload rejected
    assert!(matches!(
        pad_backup_slot_bytes(&[], DROP_BACKUP_MAX_BYTES),
        Err(MarketError::Validation(_))
    ));

    // Exceeds max_bytes
    let payload = vec![0x55u8; 65_525]; // needs 131,072 bytes
    let err_too_large = pad_backup_slot_bytes(&payload, 65_536);
    assert!(matches!(err_too_large, Err(MarketError::PayloadTooLarge(_))));

    // Valid padding
    let valid_padded = pad_backup_slot_bytes(&[0x77u8; 100], DROP_BACKUP_MAX_BYTES).unwrap();

    // Rejection: size too short (< 65536)
    assert!(unpad_backup_slot_bytes(&valid_padded[0..100]).is_err());

    // Rejection: size not a multiple of 65,536
    let mut bad_len = valid_padded.clone();
    bad_len.push(0);
    assert!(unpad_backup_slot_bytes(&bad_len).is_err());

    // Rejection: bad magic
    let mut bad_magic = valid_padded.clone();
    bad_magic[0] = b'X';
    assert!(unpad_backup_slot_bytes(&bad_magic).is_err());

    // Rejection: inner_len == 0
    let mut bad_zero_len = valid_padded.clone();
    bad_zero_len[8..12].copy_from_slice(&0u32.to_be_bytes());
    assert!(unpad_backup_slot_bytes(&bad_zero_len).is_err());

    // Rejection: inner_len exceeds slot length
    let mut bad_huge_len = valid_padded.clone();
    bad_huge_len[8..12].copy_from_slice(&1_000_000u32.to_be_bytes());
    assert!(unpad_backup_slot_bytes(&bad_huge_len).is_err());
}

