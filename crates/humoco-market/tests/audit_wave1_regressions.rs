use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ed25519_dalek::SigningKey;
use human_money_core::crypto::create_user_id;
use human_money_core::services::mnemonic::MnemonicLanguage;
use humoco_market::media::{seal_media_blob, MEDIA_DATA_URI_PREFIX};
use humoco_market::recovery::{
    build_emergency_kit_uri, build_recovery_request_uri, create_social_recovery_vault_bundle,
    parse_emergency_kit_uri, parse_recovery_request_uri, recover_master_payload_from_shares,
    EmergencyKitData, GuardianVaultStore, RecoveryKdfParams, RecoveryMasterPayload,
};
use humoco_market::sd_card::is_loopback_http;
use humoco_market::{
    accept_envelope, decode_post_slot, open_media_blob, render_wire, sign_sd_card,
    validate_market_inquiry_message, AcceptOutcome, Ad, AdKind, CardEnvelope, CardItem,
    CardPayloadInput, CardStore, DropDescriptor, MarketError, MarketInquiryMessage, WireFilter,
    AD_MAX_DURATION_SECS, POST_SLOT_MAGIC,
};

fn make_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn assert_validation_err<T: std::fmt::Debug>(
    res: Result<T, MarketError>,
    expected_substr: &str,
) {
    let err = res.expect_err("expected MarketError::Validation");
    assert!(
        matches!(&err, MarketError::Validation(msg) if msg.contains(expected_substr)),
        "expected MarketError::Validation containing {:?}, got {:?}",
        expected_substr,
        err
    );
}

fn assert_crypto_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    let err = res.expect_err("expected MarketError::Crypto");
    assert!(
        matches!(&err, MarketError::Crypto(msg) if msg.contains(expected_substr)),
        "expected MarketError::Crypto containing {:?}, got {:?}",
        expected_substr,
        err
    );
}

fn assert_slot_err<T: std::fmt::Debug>(res: Result<T, MarketError>, expected_substr: &str) {
    let err = res.expect_err("expected MarketError::Slot");
    assert!(
        matches!(&err, MarketError::Slot(msg) if msg.contains(expected_substr)),
        "expected MarketError::Slot containing {:?}, got {:?}",
        expected_substr,
        err
    );
}

fn minimal_webp(width: u16, height: u16) -> Vec<u8> {
    let mut buf = Vec::with_capacity(30);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&22u32.to_le_bytes());
    buf.extend_from_slice(b"WEBP");
    buf.extend_from_slice(b"VP8 ");
    buf.extend_from_slice(&10u32.to_le_bytes());
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a]);
    buf.extend_from_slice(&(width & 0x3FFF).to_le_bytes());
    buf.extend_from_slice(&(height & 0x3FFF).to_le_bytes());
    buf
}

#[test]
fn test_h20_strict_picture_data_uri_and_media_owner_binding() {
    let sk = make_key(11);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let payload = CardPayloadInput {
        did: did.clone(),
        v: 100,
        valid_until: 1_800_000_000,
        max_hops: 2,
        ad_count: 0,
        allow_inquiries: true,
        drops: vec![],
    };

    // Plain URL / free-form string in CardItem::Picture MUST be rejected (strict data-URI only).
    assert_validation_err(
        sign_sd_card(
            payload.clone(),
            vec![(
                CardItem::Picture("https://evil.example/avatar.png".to_string()),
                1,
            )],
            &sk,
        ),
        "Avatar data URI must start with",
    );
    assert_validation_err(
        sign_sd_card(
            payload.clone(),
            vec![(CardItem::Picture("not-a-data-uri".to_string()), 1)],
            &sk,
        ),
        "Avatar data URI must start with",
    );

    // Valid 96x96 WebP data URI succeeds.
    let avatar_uri = format!(
        "{}{}",
        MEDIA_DATA_URI_PREFIX,
        STANDARD.encode(minimal_webp(96, 96))
    );
    assert!(sign_sd_card(payload, vec![(CardItem::Picture(avatar_uri), 1)], &sk).is_ok());

    // Media blob decryption must verify owner_did matches embedded token.
    let other_did = create_user_id(&make_key(12).verifying_key(), None).unwrap();
    let webp = minimal_webp(320, 240);
    let (token, _blob_id, ct) = seal_media_blob(&did, &webp).unwrap();
    assert_eq!(open_media_blob(&did, &token, &ct).unwrap(), webp);
    assert_crypto_err(
        open_media_blob(&other_did, &token, &ct),
        "Media image_token does not match owner DID and content",
    );
}

#[test]
fn test_h21_card_store_future_bounds_and_inquiry_message_validation() {
    let sk = make_key(21);
    let did = create_user_id(&sk.verifying_key(), None).unwrap();
    let now = 1_700_000_000u64;
    let no_offender = |_: &str| false;

    // 1. Future card.payload.v > now + 300 rejected.
    let mut store = CardStore::default();
    let card_future_v = sign_sd_card(
        CardPayloadInput {
            did: did.clone(),
            v: now + 301,
            valid_until: now + 86_400,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![],
        &sk,
    )
    .unwrap();
    let res = accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: render_wire(&card_future_v, 1, WireFilter::Full).unwrap(),
            hops: 1,
        },
        now,
        100,
        None,
        &no_offender,
    );
    assert_eq!(
        res,
        AcceptOutcome::Invalid("Card version timestamp exceeds allowed skew".to_string())
    );

    // 2. Immortal card.payload.valid_until > now + 4y rejected.
    let card_immortal = sign_sd_card(
        CardPayloadInput {
            did: did.clone(),
            v: now,
            valid_until: now + 10 * 366 * 86_400,
            max_hops: 2,
            ad_count: 0,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![],
        &sk,
    )
    .unwrap();
    let res = accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: render_wire(&card_immortal, 1, WireFilter::Full).unwrap(),
            hops: 1,
        },
        now,
        100,
        None,
        &no_offender,
    );
    assert_eq!(
        res,
        AcceptOutcome::Invalid("Card valid_until exceeds maximum allowed lifetime".to_string())
    );

    // 3. Future Ad published_at > now + 300 rejected.
    let ad = Ad {
        id: "0123456789abcdef".to_string(),
        kind: AdKind::Offer,
        title: "Future Ad".to_string(),
        text: None,
        pricing: None,
        published_at: now + 500,
        renewed_at: None,
        expires_at: now + 500 + AD_MAX_DURATION_SECS,
        allow_inquiries: false,
        is_remote: false,
        image_token: None,
        event_at: None,
        event_all_day: false,
    };
    let card_future_ad = sign_sd_card(
        CardPayloadInput {
            did,
            v: now,
            valid_until: now + 365 * 86_400,
            max_hops: 2,
            ad_count: 1,
            allow_inquiries: true,
            drops: vec![],
        },
        vec![(CardItem::Ad(ad), 1)],
        &sk,
    )
    .unwrap();
    let res = accept_envelope(
        &mut store,
        &CardEnvelope {
            card_wire: render_wire(&card_future_ad, 1, WireFilter::Full).unwrap(),
            hops: 1,
        },
        now,
        100,
        None,
        &no_offender,
    );
    assert_eq!(
        res,
        AcceptOutcome::Invalid("Ad timestamps exceed allowed future bounds".to_string())
    );

    // 4. Inquiry payment_uri / amount_hint / standard_uuid bounds.
    let base_msg = MarketInquiryMessage {
        inquiry_id: "inq-1".into(),
        msg_id: "msg-1".into(),
        text: Some("Pay here".into()),
        decision: None,
        payment_uri: Some("humoco://pay?to=did:key:z123&amt=5".into()),
        amount_hint: Some("5.00".into()),
        standard_uuid: Some("std-uuid-1".into()),
        created_at: now,
    };
    assert!(validate_market_inquiry_message(&base_msg).is_ok());

    let mut bad_scheme = base_msg.clone();
    bad_scheme.payment_uri = Some("https://evil.com/pay".into());
    assert_validation_err(
        validate_market_inquiry_message(&bad_scheme),
        "payment_uri must start with 'humoco://pay?'",
    );

    let mut long_uri = base_msg.clone();
    long_uri.payment_uri = Some(format!("humoco://pay?{}", "a".repeat(520)));
    assert_validation_err(
        validate_market_inquiry_message(&long_uri),
        "payment_uri exceeds maximum length 512",
    );

    let mut long_hint = base_msg;
    long_hint.amount_hint = Some("9".repeat(65));
    assert_validation_err(
        validate_market_inquiry_message(&long_hint),
        "amount_hint must be 1..=64 characters",
    );
}

#[test]
fn test_h16_guardian_vault_equivocation_and_share_commitment_mismatch() {
    let owner_sk = make_key(31);
    let owner_did = create_user_id(&owner_sk.verifying_key(), None).unwrap();
    let kdf = RecoveryKdfParams::new_fast_test();
    let answers = ["Garten", "Lindenstrasse", "Kroatien"];
    let master_a = RecoveryMasterPayload::new(
        &owner_did,
        vec![0x11; 16],
        MnemonicLanguage::German,
        None,
        false,
    );
    let master_b = RecoveryMasterPayload::new(
        &owner_did,
        vec![0x22; 16],
        MnemonicLanguage::German,
        None,
        false,
    );

    let bundle_a = create_social_recovery_vault_bundle(
        &owner_sk,
        &owner_did,
        "Alice",
        1,
        2,
        3,
        &[1, 2, 3],
        &answers,
        &kdf,
        &master_a,
        &[],
        1_700_000_200,
    )
    .unwrap();
    let bundle_b = create_social_recovery_vault_bundle(
        &owner_sk,
        &owner_did,
        "Alice",
        1, // same epoch = 1!
        2,
        3,
        &[1, 2, 3],
        &answers,
        &kdf,
        &master_b,
        &[],
        1_700_000_100,
    )
    .unwrap();

    // 1. GuardianVaultStore::upsert_ward must reject equivocating commitments at same epoch.
    let mut store = GuardianVaultStore::default();
    store.upsert_ward(bundle_a[0].clone()).unwrap();
    assert_validation_err(
        store.upsert_ward(bundle_b[0].clone()),
        "Equivocating share commitments or r_enc for same rotation epoch",
    );

    // Older updated_at with identical commitments must NOT overwrite newer descriptors.
    let mut newer = bundle_a[0].clone();
    newer.updated_at = 1_700_000_500;
    newer.backup_sync_descriptors = vec![DropDescriptor {
        write_url: "https://new.example/sync.php".into(),
        read_url: "https://new.example/read.php".into(),
        slot_id: None,
        drop_pub: "pubkey_new".into(),
    }];
    let mut older = bundle_a[0].clone();
    older.updated_at = 1_700_000_150;
    older.backup_sync_descriptors = vec![DropDescriptor {
        write_url: "https://old.example/sync.php".into(),
        read_url: "https://old.example/read.php".into(),
        slot_id: None,
        drop_pub: "pubkey_old".into(),
    }];
    store.upsert_ward(newer).unwrap();
    store.upsert_ward(older).unwrap();
    assert_eq!(
        store.get_ward(&owner_did).unwrap().backup_sync_descriptors[0].write_url,
        "https://new.example/sync.php"
    );

    // 2. recover_master_payload_from_shares must reject mixed bundles at the same epoch.
    let mixed = vec![bundle_a[0].clone(), bundle_b[1].clone()];
    assert_validation_err(
        recover_master_payload_from_shares(&mixed, &answers),
        "Mismatched share_commitments in entry 1",
    );
}

#[test]
fn test_f8_and_f9_uri_boundaries_loopback_host_decode_cap_and_kit_limits() {
    // 1. Loopback host check must reject attacker subdomains.
    assert!(is_loopback_http("http://localhost/sync"));
    assert!(is_loopback_http("http://localhost:8080/sync"));
    assert!(is_loopback_http("http://127.0.0.1:9000/sync"));
    assert!(is_loopback_http("http://[::1]:8443/sync"));
    assert!(!is_loopback_http("http://localhost.evil.com/sync"));
    assert!(!is_loopback_http("http://127.0.0.1.evil.com/sync"));
    assert!(!is_loopback_http("https://localhost/sync"));

    // 2. decode_post_slot must fail cleanly on spoofed count = 65535 without OOM.
    let mut bogus_slot = POST_SLOT_MAGIC.to_vec();
    bogus_slot.extend_from_slice(&u16::MAX.to_be_bytes());
    assert_slot_err(
        decode_post_slot(&bogus_slot),
        "Truncated post box header at entry 0",
    );

    // 3. Recovery URI boundary, #, typographic quotes, and hard-wrap checks.
    let req = build_recovery_request_uri(&[9u8; 32], 1_700_000_000, Some("Alice"));
    let embedded = format!("https://evil.example/evil{}", req);
    assert_validation_err(
        parse_recovery_request_uri(&embedded),
        "Invalid recovery request URI: missing 'recovery-req?'",
    );
    let quoted_with_frag = format!("„{}#ignored-frag“", req);
    let parsed_req = parse_recovery_request_uri(&quoted_with_frag).unwrap();
    assert_eq!(parsed_req.name_hint.as_deref(), Some("Alice"));

    // 4. Emergency kit caps & validation.
    let kit_ok = EmergencyKitData {
        version: 1,
        entropy_hex: hex::encode([0x55u8; 16]),
        language_code: 1,
        user_prefix: Some("laptop-1".into()),
        has_bip39_passphrase: false,
        sync_descriptors: vec![],
    };
    let uri_ok = build_emergency_kit_uri(&kit_ok).unwrap();
    let wrapped_ok = format!(
        "{}\n{}",
        &uri_ok[..uri_ok.len() / 2],
        &uri_ok[uri_ok.len() / 2..]
    );
    assert_eq!(parse_emergency_kit_uri(&wrapped_ok).unwrap(), kit_ok);

    let mut kit_too_many_syncs = kit_ok.clone();
    kit_too_many_syncs.sync_descriptors = (0..9)
        .map(|i| DropDescriptor {
            write_url: format!("https://s{}.example/sync.php", i),
            read_url: format!("https://s{}.example/read.php", i),
            slot_id: None,
            drop_pub: format!("pub_{}", i),
        })
        .collect();
    let uri_too_many = build_emergency_kit_uri(&kit_too_many_syncs).unwrap();
    assert_validation_err(
        parse_emergency_kit_uri(&uri_too_many),
        "kit sync descriptor count 9 exceeds maximum 8",
    );

    let mut kit_bad_prefix = kit_ok;
    kit_bad_prefix.user_prefix = Some("bad_underscore".into());
    let uri_bad_prefix = build_emergency_kit_uri(&kit_bad_prefix).unwrap();
    assert_validation_err(
        parse_emergency_kit_uri(&uri_bad_prefix),
        "Sync prefix contains invalid characters: 'bad_underscore'",
    );
}
