use ed25519_dalek::{SigningKey, VerifyingKey};
use human_money_core::crypto::{create_user_id, ed25519_sk_to_x25519_sk};
use humoco_market::{
    build_card_bundle, open_slot_blob, seal_slot_blob, CardBundle, CardEnvelope, CardStore,
    ContactCard, StoredCard,
};

fn dummy_card(did: &str, v: u64) -> ContactCard {
    ContactCard {
        did: did.to_string(),
        v,
        valid_until: 1800000000,
        max_hops: 2,
        profile: Default::default(),
        listings: vec![],
        drops: vec![],
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
            card_jws: "header.payload.signature".to_string(),
            hops: 1,
        }],
    };

    let blob = seal_slot_blob(&bundle, &[reader_vk], 65536).unwrap();

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
            card_jws: format!("header.{}.signature", author_did),
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

        let card = dummy_card(&did, i as u64);
        store.cards.insert(
            *vk.as_bytes(),
            StoredCard {
                card,
                card_jws: format!("jws-{}", i),
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

    let bundle = build_card_bundle(Some("own-jws"), &store, &trust_level, 65536);
    // Own card is first
    assert_eq!(bundle.envelopes[0].card_jws, "own-jws");
    assert_eq!(bundle.envelopes[0].hops, 1);
    // Max 1 own + 20 forwarded = 21 cards
    assert_eq!(bundle.envelopes.len(), 21);
}
